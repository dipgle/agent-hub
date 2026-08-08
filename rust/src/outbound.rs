//! Outbound dispatcher. Everything the hub says to the outside world goes
//! through the outbox table first, so nothing is sent twice, every failure is
//! retried with a visible attempt count, and giving up lands in dead_letter.

use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::json;

use crate::adapters::tfl5;
use crate::config::Config;
use crate::db::{Db, OutboxRow};
use crate::exec::{run, truncate, RunOpts};
use crate::logging;

const MAX_ATTEMPTS: i64 = 5;

#[derive(Debug, Default, Serialize)]
pub struct FlushSummary {
    pub sent: usize,
    pub failed: usize,
    pub gave_up: usize,
}

/// Local channel that always works: a log file + a macOS banner.
pub fn notify(cfg: &Config, subject: Option<&str>, body: &str) -> Result<()> {
    if let Some(dir) = cfg.notify.file.parent() {
        create_dir_all(dir)?;
    }
    let entry = format!(
        "\n===== {} {} =====\n{}\n",
        logging::now_iso(),
        subject.unwrap_or(""),
        body
    );
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cfg.notify.file)
        .and_then(|mut f| f.write_all(entry.as_bytes()))
        .map_err(|e| anyhow!("cannot append to {}: {e}", cfg.notify.file.display()))?;

    if cfg.notify.macos_notification && cfg.notify.file.exists() && cfg!(target_os = "macos") {
        let raw = subject.unwrap_or(body);
        let text: String = raw
            .chars()
            .take(200)
            .map(|c| {
                if c == '"' || c == '\\' || c == '\n' {
                    ' '
                } else {
                    c
                }
            })
            .collect();
        let script = format!("display notification \"{text}\" with title \"hub\"");
        match run(
            "osascript",
            &["-e", &script],
            RunOpts {
                timeout: Some(Duration::from_secs(10)),
                ..Default::default()
            },
        ) {
            Ok(r) if r.code != Some(0) => logging::warn(
                "osascript_notify_failed",
                json!({ "err": truncate(&r.stderr, 200) }),
            ),
            Err(e) => logging::warn("osascript_notify_failed", json!({ "err": e.to_string() })),
            _ => {}
        }
    }
    Ok(())
}

/// Send one outbox row. Errors bubble up so `flush` can record the attempt.
pub fn send_one(_db: &Db, cfg: &Config, row: &OutboxRow) -> Result<Option<String>> {
    // One outbound channel plus the local notify file. GitHub comments, email
    // replies and Telegram approval buttons went with the inbox product on
    // 2026-08-08; `git show backup/inbox-adapters` still has them.
    match row.channel.as_str() {
        "tfl5" => tfl5::send(
            &cfg.adapters.tfl5,
            &row.target,
            row.subject.as_deref(),
            &row.body,
        ),
        "notify" => notify(cfg, row.subject.as_deref(), &row.body).map(|_| None),
        // Nothing to reply to (the sender is a log file) — surface it locally.
        "devlog" => notify(
            cfg,
            row.subject.as_deref().or(Some("devlog item")),
            &row.body,
        )
        .map(|_| None),
        other => Err(anyhow!("unknown outbound channel: {other}")),
    }
}

pub fn flush(db: &Db, cfg: &Config, limit: i64) -> Result<FlushSummary> {
    let rows = db.queued_outbox(limit)?;
    let mut s = FlushSummary::default();

    for row in rows {
        match send_one(db, cfg, &row) {
            Ok(remote) => {
                db.mark_outbox_sent(row.id)?;
                s.sent += 1;
                logging::info(
                    "outbox_sent",
                    json!({ "outbox_id": row.id, "channel": row.channel, "target": row.target, "remote": remote }),
                );
            }
            Err(e) => {
                let (attempts, status) =
                    db.mark_outbox_failed(row.id, &e.to_string(), MAX_ATTEMPTS)?;
                s.failed += 1;
                logging::error(
                    "outbox_send_failed",
                    json!({
                        "outbox_id": row.id, "channel": row.channel, "target": row.target,
                        "attempts": attempts, "status": status, "err": logging::err_chain(&e),
                    }),
                );
                if status == "failed" {
                    s.gave_up += 1;
                    db.dead_letter(
                        Some(&row.channel),
                        Some(&row.id.to_string()),
                        "outbound",
                        Some(&json!({
                            "channel": row.channel, "target": row.target,
                            "subject": row.subject, "body": truncate(&row.body, 2000),
                        })),
                        &e.to_string(),
                    )?;
                    // Losing an outbound message must be loud, not a row nobody reads.
                    if let Err(e2) = notify(
                        cfg,
                        Some(&format!(
                            "hub: gave up sending {} → {}",
                            row.channel, row.target
                        )),
                        &format!(
                            "outbox #{} failed {attempts}×: {e}\n\n{}",
                            row.id,
                            truncate(&row.body, 500)
                        ),
                    ) {
                        logging::error(
                            "deadletter_notify_failed",
                            json!({ "err": e2.to_string() }),
                        );
                    }
                }
            }
        }
    }

    Ok(s)
}
