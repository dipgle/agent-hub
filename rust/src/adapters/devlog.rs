//! Project-progress ingest: every project already writes a devlog
//! (`<project>/logs/devlog.sqlite`, table `events`). We tail it read-only and
//! turn attention-worthy events (warning / blocker / bug / test_fail /
//! question) into hub messages, so a project shouting into its own log reaches
//! the same triage brain as an email or a GitHub comment.
//!
//! Source of truth for the schema: init-project/mcp/project-agent-rs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use rusqlite::{params_from_iter, Connection, OpenFlags};
use serde_json::json;

use crate::adapters::PollResult;
use crate::config::DevlogCfg;
use crate::db::NewMessage;
use crate::logging;

pub const NAME: &str = "devlog";

const NOT_PROJECTS: [&str; 9] = [
    "AI",
    "scripts",
    "logs",
    "memory",
    "docs-map",
    "test-results",
    "crates",
    "sdk",
    "app-source",
];

/// Where a project's devlog lives, searched across the SAME bases everything
/// else uses so one project never resolves two different ways.
fn devlog_path(bases: &[PathBuf], project: &str) -> PathBuf {
    for base in bases {
        let p = base.join(project).join("logs").join("devlog.sqlite");
        if p.is_file() {
            return p;
        }
    }
    // Nothing found: return the first candidate so the caller's `is_file()`
    // check fails on a path that is at least reportable.
    bases
        .first()
        .map(|b| b.join(project).join("logs").join("devlog.sqlite"))
        .unwrap_or_default()
}

fn dirs_with_devlog(base: &Path) -> Vec<String> {
    let entries = match std::fs::read_dir(base) {
        Ok(e) => e,
        Err(_) => return vec![],
    };
    let mut out = vec![];
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('_') || name.starts_with('.') || NOT_PROJECTS.contains(&name.as_str()) {
            continue;
        }
        if !entry.path().is_dir() {
            continue;
        }
        if entry.path().join("logs").join("devlog.sqlite").is_file() {
            out.push(name);
        }
    }
    out
}

/// Projects that keep a devlog, searched in the SAME folders `project_dir`
/// resolves against — one list, one config key. They used to disagree: this
/// function hardcoded `AI/` first while `project_dir` had its own order, so
/// "which folders hold projects" had two answers.
pub fn discover_projects(bases: &[PathBuf]) -> Vec<String> {
    let mut all: Vec<String> = bases.iter().flat_map(|b| dirs_with_devlog(b)).collect();
    all.sort();
    all.dedup();
    all
}

/// A devlog file that exists but was never written to by project-agent.
fn is_uninitialized(e: &anyhow::Error) -> bool {
    e.to_string()
        .to_lowercase()
        .contains("no such table: events")
}

fn open_ro(path: &Path) -> Result<Connection> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| anyhow!("open {} read-only failed: {e}", path.display()))
}

fn max_event_id(path: &Path) -> Result<i64> {
    let conn = open_ro(path)?;
    let id: i64 = conn
        .query_row("SELECT COALESCE(MAX(id), 0) FROM events", [], |r| r.get(0))
        .map_err(|e| anyhow!("{e}"))?;
    Ok(id)
}

struct Event {
    id: i64,
    ts: String,
    kind: String,
    actor: Option<String>,
    ref_type: Option<String>,
    ref_id: Option<String>,
    content: Option<String>,
}

fn read_events(path: &Path, kinds: &[String], after_id: i64, limit: i64) -> Result<Vec<Event>> {
    // Read-only: the hub must never write into a project's devlog by accident.
    let conn = open_ro(path)?;
    let placeholders = kinds.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT id, ts, kind, actor, ref_type, ref_id, content
           FROM events
          WHERE id > ? AND kind IN ({placeholders})
          ORDER BY id ASC
          LIMIT ?"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| anyhow!("{e}"))?;

    let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(after_id)];
    for k in kinds {
        args.push(Box::new(k.clone()));
    }
    args.push(Box::new(limit));

    let rows = stmt
        .query_map(params_from_iter(args.iter().map(|b| b.as_ref())), |r| {
            Ok(Event {
                id: r.get(0)?,
                ts: r.get(1)?,
                kind: r.get(2)?,
                actor: r.get(3)?,
                ref_type: r.get(4)?,
                ref_id: r.get(5)?,
                content: r.get(6)?,
            })
        })
        .map_err(|e| anyhow!("{e}"))?;

    let mut out = vec![];
    for row in rows {
        out.push(row.map_err(|e| anyhow!("{e}"))?);
    }
    Ok(out)
}

pub fn poll(
    cfg: &DevlogCfg,
    cursors: &BTreeMap<String, String>,
    bases: &[PathBuf],
) -> Result<PollResult> {
    let projects = if cfg.projects.is_empty() {
        discover_projects(bases)
    } else {
        cfg.projects.clone()
    };
    let kinds = if cfg.kinds.is_empty() {
        ["warning", "blocker", "bug", "test_fail", "question"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        cfg.kinds.clone()
    };

    let mut out = PollResult::default();
    let mut failures: Vec<String> = vec![];
    let mut not_initialized: Vec<String> = vec![];
    let mut healthy = 0usize;

    for project in projects {
        let path = devlog_path(bases, &project);
        if !path.is_file() {
            continue;
        }
        let key = format!("devlog:{project}:last_id");

        // First sight of a project: take the current tip as the baseline instead
        // of replaying years of history (`backfill: true` ingests the backlog).
        if !cursors.contains_key(&key) && !cfg.backfill {
            match max_event_id(&path) {
                Ok(tip) => {
                    out.cursors.insert(key, tip.to_string());
                    logging::info(
                        "devlog_baseline_set",
                        json!({ "project": project, "last_id": tip }),
                    );
                    healthy += 1;
                }
                Err(e) if is_uninitialized(&e) => {
                    // File exists but the project never logged anything — normal.
                    logging::info(
                        "devlog_not_initialized",
                        json!({ "project": project, "path": path.display().to_string() }),
                    );
                    not_initialized.push(project);
                }
                Err(e) => {
                    logging::error(
                        "devlog_baseline_failed",
                        json!({ "project": project, "err": e.to_string() }),
                    );
                    failures.push(format!("{project} (baseline): {e}"));
                }
            }
            continue;
        }

        let after_id: i64 = cursors.get(&key).and_then(|v| v.parse().ok()).unwrap_or(0);
        let rows = match read_events(&path, &kinds, after_id, cfg.max_per_project) {
            Ok(r) => {
                healthy += 1;
                r
            }
            Err(e) if is_uninitialized(&e) => {
                logging::info(
                    "devlog_not_initialized",
                    json!({ "project": project, "path": path.display().to_string() }),
                );
                not_initialized.push(project);
                continue;
            }
            Err(e) => {
                // A locked/corrupt project DB is a real condition — surface it.
                logging::error(
                    "devlog_read_failed",
                    json!({ "project": project, "err": e.to_string() }),
                );
                failures.push(format!("{project}: {e}"));
                continue;
            }
        };

        let last = rows.last().map(|r| r.id);
        for r in rows {
            out.messages.push(NewMessage {
                source: NAME.into(),
                external_id: format!("{project}:{}", r.id),
                thread_key: Some(format!(
                    "devlog:{project}:{}:{}",
                    r.ref_type.clone().unwrap_or_else(|| "-".into()),
                    r.ref_id.clone().unwrap_or_else(|| "-".into())
                )),
                project: Some(project.clone()),
                sender: Some(match &r.actor {
                    Some(a) => format!("devlog:{a}"),
                    None => format!("devlog:{project}"),
                }),
                sender_trust: Some("trusted".into()), // our own machine wrote this
                subject: Some(format!(
                    "[{project}] {}{}",
                    r.kind,
                    r.ref_id
                        .clone()
                        .map(|i| format!(" {i}"))
                        .unwrap_or_default()
                )),
                body: Some(r.content.clone().unwrap_or_default()),
                url: None,
                received_at: Some(r.ts.clone()),
                raw: Some(json!({
                    "stream": "devlog_events",
                    "project": project,
                    "event_id": r.id,
                    "kind": r.kind,
                    "ref_type": r.ref_type,
                    "ref_id": r.ref_id,
                })),
            });
        }
        if let Some(id) = last {
            out.cursors
                .insert(format!("devlog:{project}:last_id"), id.to_string());
        }
    }

    // Only a total wipeout is an adapter failure; otherwise report the partial
    // so the cursors earned by the healthy projects still get committed.
    if !failures.is_empty() && healthy == 0 {
        return Err(anyhow!("devlog poll failed: {}", failures.join("; ")));
    }
    let mut notes = vec![];
    if !failures.is_empty() {
        notes.push(format!("failed: {}", failures.join("; ")));
    }
    if !not_initialized.is_empty() {
        notes.push(format!(
            "no events table yet: {}",
            not_initialized.join(", ")
        ));
    }
    if !notes.is_empty() {
        out.skipped = Some(notes.join(" | "));
    }
    Ok(out)
}
