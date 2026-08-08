//! GitHub ingest via the `gh` CLI — no webhook, no public endpoint, no PAT in
//! config: `gh` already holds the auth. Two streams:
//!
//!   1. /notifications  — CI failures, mentions, review requests, comments
//!   2. per-repo issues + issue comments (opt-in `repos: []`), so GitHub Issues
//!      works as a user-feedback channel even after notifications are read
//!
//! `external_id` embeds the item's `updated_at`, so an updated thread is a NEW
//! message while a replayed poll window is deduped.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::adapters::{Health, PollResult};
use crate::config::GithubCfg;
use crate::db::NewMessage;
use crate::exec::{run, run_json, truncate, RunOpts};
use crate::logging;

pub const NAME: &str = "github";
const MAX_BODY: usize = 20_000;

fn body_clip(s: &str) -> String {
    if s.chars().count() <= MAX_BODY {
        s.to_string()
    } else {
        let kept: String = s.chars().take(MAX_BODY).collect();
        format!("{kept}\n…[truncated]")
    }
}

fn gh_api(path: &str, timeout_secs: u64) -> Result<Value> {
    run_json(
        "gh",
        &["api", "-H", "Accept: application/vnd.github+json", path],
        RunOpts {
            timeout: Some(Duration::from_secs(timeout_secs)),
            ..Default::default()
        },
    )
}

/// Verify the CLI is usable before we blame the network for empty results.
pub fn health() -> Health {
    match run(
        "gh",
        &["auth", "status"],
        RunOpts {
            timeout: Some(Duration::from_secs(20)),
            ..Default::default()
        },
    ) {
        Ok(r) => {
            let text = format!("{}{}", r.stdout, r.stderr);
            let detail = text
                .lines()
                .take(3)
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string();
            Health {
                ok: r.code == Some(0),
                detail,
            }
        }
        Err(e) => Health {
            ok: false,
            detail: e.to_string(),
        },
    }
}

fn api_path_to_html(url: Option<&str>) -> Option<String> {
    // https://api.github.com/repos/o/r/issues/12 → https://github.com/o/r/issues/12
    url.map(|u| u.replace("https://api.github.com/repos/", "https://github.com/"))
}

/// Pure normalizer — kept separate from fetching so it can be unit-tested
/// against captured real payloads.
pub fn normalize_notification(
    n: &Value,
    detail: Option<&Value>,
    detail_error: Option<&str>,
) -> NewMessage {
    let repo = n
        .get("repository")
        .and_then(|r| r.get("full_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown/unknown");
    let title = n
        .get("subject")
        .and_then(|s| s.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("(no title)");
    let kind = n
        .get("subject")
        .and_then(|s| s.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    let subject_url = n
        .get("subject")
        .and_then(|s| s.get("url"))
        .and_then(|v| v.as_str());
    let updated = n.get("updated_at").and_then(|v| v.as_str()).unwrap_or("");
    let notif_id = n
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| n.get("id").map(|v| v.to_string()).unwrap_or_default());

    let mut body = title.to_string();
    let mut sender = format!("github:{repo}");
    let mut url = api_path_to_html(subject_url).or_else(|| {
        n.get("repository")
            .and_then(|r| r.get("html_url"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    });

    let detail_summary = match detail {
        Some(d) => {
            if let Some(b) = d
                .get("body")
                .and_then(|v| v.as_str())
                .filter(|b| !b.is_empty())
            {
                body = body_clip(b);
            }
            if let Some(login) = d
                .get("user")
                .and_then(|u| u.get("login"))
                .and_then(|v| v.as_str())
            {
                sender = login.to_string();
            }
            if let Some(h) = d.get("html_url").and_then(|v| v.as_str()) {
                url = Some(h.to_string());
            }
            json!({
                "number": d.get("number"),
                "state": d.get("state"),
                "user": d.get("user").and_then(|u| u.get("login")),
                "html_url": d.get("html_url"),
            })
        }
        None => {
            if let Some(err) = detail_error {
                body = format!("{title}\n\n[hub: could not fetch item body: {err}]");
            }
            Value::Null
        }
    };

    NewMessage {
        source: NAME.into(),
        external_id: format!("notif:{notif_id}:{updated}"),
        thread_key: Some(format!("{repo}:{kind}:{}", subject_url.unwrap_or(title))),
        project: None,
        sender: Some(sender),
        sender_trust: None,
        subject: Some(format!("[{repo}] {title}")),
        body: Some(body),
        url,
        received_at: Some(updated.to_string()),
        raw: Some(json!({
            "stream": "notifications",
            "reason": n.get("reason"),
            "type": kind,
            "repo": repo,
            "notification_id": notif_id,
            "detail": detail_summary,
            // The failing branch, parsed once at ingest so triage never has to
            // re-derive it from the subject. Absent when the title does not
            // match GitHub's shape — the consumer must handle that.
            "ci": match kind {
                "CheckSuite" => parse_check_suite_title(title)
                    .map(|(workflow, branch)| json!({ "workflow": workflow, "branch": branch }))
                    .unwrap_or(Value::Null),
                _ => Value::Null,
            },
        })),
    }
}

fn notification_stream(
    cfg: &GithubCfg,
    cursor: Option<&String>,
) -> Result<(Vec<NewMessage>, Option<String>)> {
    let mut path = format!(
        "/notifications?all={}&per_page={}",
        cfg.include_read, cfg.per_page
    );
    if let Some(since) = cursor {
        path.push_str(&format!("&since={since}"));
    }

    let value = gh_api(&path, 45).map_err(|e| anyhow!("gh /notifications failed: {e}"))?;
    let items = value.as_array().cloned().unwrap_or_default();

    let mut messages = vec![];
    let mut detail_budget = cfg.detail_limit;

    for n in &items {
        let mut detail: Option<Value> = None;
        let mut detail_error: Option<String> = None;

        // Bodies live behind a second call; spend the budget on the newest items.
        let detail_url = n
            .get("subject")
            .and_then(|s| {
                s.get("latest_comment_url")
                    .and_then(|v| v.as_str())
                    .or_else(|| s.get("url").and_then(|v| v.as_str()))
            })
            .map(|s| s.to_string());

        if let Some(u) = detail_url {
            if detail_budget > 0 && u.starts_with("https://api.github.com/") {
                detail_budget -= 1;
                match gh_api(&u.replace("https://api.github.com", ""), 45) {
                    Ok(d) => detail = Some(d),
                    Err(e) => {
                        // Detail fetch is best-effort, but must never vanish quietly.
                        logging::warn(
                            "github_detail_fetch_failed",
                            json!({ "url": u, "err": e.to_string() }),
                        );
                        detail_error = Some(truncate(&e.to_string(), 200));
                    }
                }
            }
        }

        messages.push(normalize_notification(
            n,
            detail.as_ref(),
            detail_error.as_deref(),
        ));
    }

    let newest = items
        .iter()
        .filter_map(|n| {
            n.get("updated_at")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .max();

    Ok((messages, newest))
}

fn repo_stream(repo: &str, since: Option<&String>) -> Result<Vec<NewMessage>> {
    let since_param = since.map(|s| format!("&since={s}")).unwrap_or_default();
    let mut messages = vec![];

    // Issue comments (the actual feedback text people write).
    let comments = gh_api(
        &format!(
            "/repos/{repo}/issues/comments?sort=updated&direction=desc&per_page=30{since_param}"
        ),
        45,
    )
    .map_err(|e| anyhow!("gh issue comments {repo} failed: {e}"))?;
    for c in comments.as_array().cloned().unwrap_or_default() {
        let id = c.get("id").map(|v| v.to_string()).unwrap_or_default();
        let updated = c
            .get("updated_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let issue_url = c
            .get("issue_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let number = issue_url.rsplit('/').next().unwrap_or("issue").to_string();
        messages.push(NewMessage {
            source: NAME.into(),
            external_id: format!("comment:{repo}:{id}:{updated}"),
            thread_key: Some(format!("{repo}:Issue:{issue_url}")),
            project: None,
            sender: Some(c.get("user").and_then(|u| u.get("login")).and_then(|v| v.as_str()).unwrap_or("unknown").to_string()),
            sender_trust: None,
            subject: Some(format!("[{repo}] comment on #{number}")),
            body: Some(body_clip(c.get("body").and_then(|v| v.as_str()).unwrap_or(""))),
            url: c.get("html_url").and_then(|v| v.as_str()).map(|s| s.to_string()),
            received_at: Some(updated),
            raw: Some(json!({ "stream": "issue_comments", "repo": repo, "comment_id": id, "issue_url": issue_url })),
        });
    }

    // Newly opened / updated issues.
    let issues = gh_api(
        &format!(
            "/repos/{repo}/issues?state=open&sort=updated&direction=desc&per_page=20{since_param}"
        ),
        45,
    )
    .map_err(|e| anyhow!("gh issues {repo} failed: {e}"))?;
    for i in issues.as_array().cloned().unwrap_or_default() {
        if i.get("pull_request").is_some() {
            continue; // PRs arrive through notifications
        }
        let number = i.get("number").and_then(|v| v.as_i64()).unwrap_or(0);
        let updated = i
            .get("updated_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let title = i
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let labels: Vec<String> = i
            .get("labels")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|l| {
                        l.get("name")
                            .and_then(|n| n.as_str())
                            .map(|s| s.to_string())
                            .or_else(|| l.as_str().map(|s| s.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        messages.push(NewMessage {
            source: NAME.into(),
            external_id: format!("issue:{repo}:{number}:{updated}"),
            thread_key: Some(format!(
                "{repo}:Issue:{}",
                i.get("url").and_then(|v| v.as_str()).unwrap_or("")
            )),
            project: None,
            sender: Some(
                i.get("user")
                    .and_then(|u| u.get("login"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
            ),
            sender_trust: None,
            subject: Some(format!("[{repo}] #{number} {title}")),
            body: Some(body_clip(
                i.get("body").and_then(|v| v.as_str()).unwrap_or(&title),
            )),
            url: i
                .get("html_url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            received_at: Some(updated),
            raw: Some(
                json!({ "stream": "issues", "repo": repo, "number": number, "labels": labels }),
            ),
        });
    }

    Ok(messages)
}

// ----------------------------------------------------------------------
// Why a check suite is red.
//
// A CheckSuite notification has no `subject.url`, so there is no detail call to
// make and the item reaches triage as a bare title. The model then pays
// $0.15–$0.22 to answer "CI failed, cause unknown" — measured 2026-08-08:
// $4.99 of hub's $9.12 lifetime spend went to 38 such items, and all 38 were
// still sitting in the inbox saying nothing. The cause IS fetchable host-side,
// deterministically, for free: the failed jobs, their failed steps, and the
// check-run annotations that carry the real message. On 08-08 that message was
// "The job was not started because recent account payments have failed…" — a
// billing wall, not a code defect, on 60/60 tfl5 runs since 08-03.
// ----------------------------------------------------------------------

/// GitHub's CheckSuite notification title has a fixed shape:
/// `"<workflow> workflow run failed for <branch> branch"`. It is the only place
/// the failing branch is carried, so this is what turns a notification into a
/// query. `None` when the shape does not match — a rename upstream must degrade
/// to "no CI context", never to the wrong branch.
pub fn parse_check_suite_title(title: &str) -> Option<(String, String)> {
    let rest = title.trim().strip_suffix(" branch")?;
    let (workflow, branch) = rest.split_once(" workflow run failed for ")?;
    let (workflow, branch) = (workflow.trim(), branch.trim());
    if workflow.is_empty() || !is_safe_ref(branch) {
        return None;
    }
    Some((workflow.to_string(), branch.to_string()))
}

/// A ref lifted out of an inbound payload becomes an argv element for `gh`.
/// `exec::run` is argv-exact, so this is not shell-injection defence — it stops
/// a malformed or hostile ref from arriving as a FLAG (`--repo …`).
pub fn is_safe_ref(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 120
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
}

#[derive(Debug, Clone, Default)]
pub struct CiJobFailure {
    pub name: String,
    pub failed_steps: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CiFailure {
    pub run_id: i64,
    pub url: String,
    pub workflow: String,
    pub display_title: String,
    pub created_at: String,
    pub jobs: Vec<CiJobFailure>,
    /// Distinct annotation messages across the run, most repeated first.
    pub annotations: Vec<String>,
    pub log_tail: Option<String>,
    /// Everything that could not be fetched. Never empty-and-silent: an empty
    /// context with no note reads as "nothing was wrong".
    pub notes: Vec<String>,
}

const MAX_JOBS_INSPECTED: usize = 6;
const MAX_ANNOTATION_CHARS: usize = 300;
const MAX_LOG_TAIL_LINES: usize = 40;
const MAX_LOG_TAIL_CHARS: usize = 1500;

impl CiFailure {
    /// Prompt-shaped text. Empty only when there is genuinely nothing to say,
    /// which cannot happen while `notes` records the failures.
    pub fn render(&self) -> String {
        let mut lines = vec![];
        if self.run_id != 0 {
            lines.push(format!(
                "failed run: {} · {} · {} · {}",
                self.workflow, self.created_at, self.display_title, self.url
            ));
        }
        for a in &self.annotations {
            lines.push(format!("annotation: {a}"));
        }
        for j in &self.jobs {
            if j.failed_steps.is_empty() {
                lines.push(format!("failed job: {} (no step reached)", j.name));
            } else {
                lines.push(format!(
                    "failed job: {} → step {}",
                    j.name,
                    j.failed_steps.join(" · ")
                ));
            }
        }
        if let Some(t) = &self.log_tail {
            lines.push(format!("failed-step log (tail):\n{t}"));
        }
        for n in &self.notes {
            lines.push(n.clone());
        }
        lines.join("\n")
    }
}

/// Why the newest failed run on `branch` failed. Deterministic, host-side,
/// read-only. Every fetch error becomes a note, never silence.
pub fn ci_failure_context(repo: &str, branch: &str, workflow: Option<&str>) -> CiFailure {
    let mut out = CiFailure::default();
    if !is_safe_ref(branch) {
        out.notes.push(format!("[hub: unusable branch {branch:?}]"));
        return out;
    }

    let runs = match run_json(
        "gh",
        &[
            "run",
            "list",
            "--repo",
            repo,
            "--branch",
            branch,
            "--limit",
            "5",
            "--json",
            "databaseId,conclusion,workflowName,displayTitle,createdAt,url",
        ],
        RunOpts {
            timeout: Some(Duration::from_secs(30)),
            ..Default::default()
        },
    ) {
        Ok(v) => v.as_array().cloned().unwrap_or_default(),
        Err(e) => {
            logging::warn(
                "github_ci_run_list_failed",
                json!({ "repo": repo, "branch": branch, "err": e.to_string() }),
            );
            out.notes.push(format!(
                "[hub: gh run list failed: {}]",
                truncate(&e.to_string(), 200)
            ));
            return out;
        }
    };

    let failed = |r: &&Value| r.get("conclusion").and_then(|v| v.as_str()) == Some("failure");
    let pick = runs
        .iter()
        .filter(failed)
        .find(|r| match workflow {
            Some(w) => r.get("workflowName").and_then(|v| v.as_str()) == Some(w),
            None => true,
        })
        .or_else(|| runs.iter().find(failed));
    let Some(failed_run) = pick else {
        out.notes.push(format!(
            "[hub: no failed run on {repo}@{branch} among the newest {}]",
            runs.len()
        ));
        return out;
    };

    let str_of = |k: &str| {
        failed_run
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    out.run_id = failed_run
        .get("databaseId")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    out.url = str_of("url");
    out.workflow = str_of("workflowName");
    out.display_title = str_of("displayTitle");
    out.created_at = str_of("createdAt");
    if out.run_id == 0 {
        out.notes
            .push("[hub: run carries no databaseId, cannot fetch jobs]".into());
        return out;
    }

    let jobs = match gh_api(
        &format!("/repos/{repo}/actions/runs/{}/jobs", out.run_id),
        30,
    ) {
        Ok(v) => v
            .get("jobs")
            .and_then(|j| j.as_array())
            .cloned()
            .unwrap_or_default(),
        Err(e) => {
            logging::warn(
                "github_ci_jobs_failed",
                json!({ "repo": repo, "run_id": out.run_id, "err": e.to_string() }),
            );
            out.notes.push(format!(
                "[hub: jobs fetch failed: {}]",
                truncate(&e.to_string(), 200)
            ));
            vec![]
        }
    };

    // Annotation text repeats verbatim across every job of an infrastructure
    // failure (7 identical "payments have failed" lines on run 31230387960),
    // so count instead of listing.
    let mut seen: Vec<(String, usize)> = vec![];
    for j in jobs
        .iter()
        .filter(|j| j.get("conclusion").and_then(|v| v.as_str()) == Some("failure"))
        .take(MAX_JOBS_INSPECTED)
    {
        let name = j
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("(unnamed job)")
            .to_string();
        let failed_steps = j
            .get("steps")
            .and_then(|s| s.as_array())
            .map(|steps| {
                steps
                    .iter()
                    .filter(|s| s.get("conclusion").and_then(|v| v.as_str()) == Some("failure"))
                    .filter_map(|s| s.get("name").and_then(|v| v.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        if let Some(job_id) = j.get("id").and_then(|v| v.as_i64()) {
            match gh_api(
                &format!("/repos/{repo}/check-runs/{job_id}/annotations"),
                30,
            ) {
                Ok(v) => {
                    for a in v.as_array().cloned().unwrap_or_default() {
                        if a.get("annotation_level").and_then(|v| v.as_str()) == Some("notice") {
                            continue;
                        }
                        let Some(m) = a.get("message").and_then(|v| v.as_str()) else {
                            continue;
                        };
                        let m = truncate(m.trim(), MAX_ANNOTATION_CHARS);
                        if m.is_empty() {
                            continue;
                        }
                        match seen.iter_mut().find(|(t, _)| *t == m) {
                            Some((_, n)) => *n += 1,
                            None => seen.push((m, 1)),
                        }
                    }
                }
                Err(e) => {
                    logging::warn(
                        "github_ci_annotations_failed",
                        json!({ "repo": repo, "job_id": job_id, "err": e.to_string() }),
                    );
                    out.notes.push(format!(
                        "[hub: annotations for job {job_id} failed: {}]",
                        truncate(&e.to_string(), 160)
                    ));
                }
            }
        }

        out.jobs.push(CiJobFailure { name, failed_steps });
    }

    seen.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    out.annotations = seen
        .into_iter()
        .map(|(m, n)| {
            if n > 1 {
                format!("{m}  [×{n} jobs]")
            } else {
                m
            }
        })
        .collect();

    // Only when nothing above explained it: the failing step log. Bounded, and
    // often absent — a job that never started has no log ("log not found",
    // observed on run 31230387960).
    let unexplained =
        out.annotations.is_empty() && out.jobs.iter().all(|j| j.failed_steps.is_empty());
    if unexplained {
        let run_arg = out.run_id.to_string();
        match run(
            "gh",
            &["run", "view", &run_arg, "--repo", repo, "--log-failed"],
            RunOpts {
                timeout: Some(Duration::from_secs(60)),
                ..Default::default()
            },
        ) {
            Ok(r) if r.ok() && !r.stdout.trim().is_empty() => {
                let lines: Vec<&str> = r.stdout.lines().collect();
                let tail = lines[lines.len().saturating_sub(MAX_LOG_TAIL_LINES)..].join("\n");
                out.log_tail = Some(truncate(&tail, MAX_LOG_TAIL_CHARS));
            }
            Ok(r) => {
                let why = if r.timed_out {
                    "timed out".to_string()
                } else {
                    let detail = [r.stderr.trim(), r.stdout.trim()]
                        .into_iter()
                        .find(|s| !s.is_empty())
                        .unwrap_or("empty output");
                    truncate(detail, 160)
                };
                out.notes.push(format!("[hub: no failed-step log ({why})]"));
            }
            Err(e) => out.notes.push(format!(
                "[hub: gh run view failed: {}]",
                truncate(&e.to_string(), 160)
            )),
        }
    }

    out
}

pub fn poll(cfg: &GithubCfg, cursors: &BTreeMap<String, String>) -> Result<PollResult> {
    let h = health();
    if !h.ok {
        // Not a silent skip: the caller records this on the run row.
        return Err(anyhow!("gh CLI not authenticated: {}", h.detail));
    }

    let mut out = PollResult::default();
    let mut partial: Vec<String> = vec![];

    let (mut messages, newest) = notification_stream(cfg, cursors.get("github:since"))?;
    out.messages.append(&mut messages);
    if let Some(ts) = newest {
        out.cursors.insert("github:since".into(), ts);
    }

    for repo in &cfg.repos {
        let key = format!("github:repo:{repo}:since");
        match repo_stream(repo, cursors.get(&key)) {
            Ok(msgs) => {
                let newest = msgs.iter().filter_map(|m| m.received_at.clone()).max();
                out.messages.extend(msgs);
                if let Some(ts) = newest {
                    out.cursors.insert(key, ts);
                }
            }
            Err(e) => {
                // One bad repo must not sink the whole poll — but it is logged
                // and reported as a partial on the run row.
                logging::error(
                    "github_repo_stream_failed",
                    json!({ "repo": repo, "err": e.to_string() }),
                );
                partial.push(format!("{repo}: {}", truncate(&e.to_string(), 200)));
            }
        }
    }

    if !partial.is_empty() {
        out.skipped = Some(format!("repo stream failed: {}", partial.join("; ")));
    }
    Ok(out)
}

/// Post a comment back onto the originating issue/PR thread.
/// `target` = "owner/repo#123"
pub fn send(target: &str, body: &str) -> Result<Option<String>> {
    let re = regex::Regex::new(r"^([^/]+/[^#]+)#(\d+)$")?;
    let caps = re
        .captures(target)
        .ok_or_else(|| anyhow!("github send: bad target \"{target}\", want owner/repo#123"))?;
    let bad = || anyhow!("github send: target \"{target}\" matched but lacks repo/number");
    let repo = caps.get(1).ok_or_else(bad)?.as_str();
    let number = caps.get(2).ok_or_else(bad)?.as_str();

    let path = format!("/repos/{repo}/issues/{number}/comments");
    let body_arg = format!("body={body}");
    let r = run(
        "gh",
        &["api", "--method", "POST", &path, "-f", &body_arg],
        RunOpts {
            timeout: Some(Duration::from_secs(45)),
            ..Default::default()
        },
    )?;
    if r.code != Some(0) {
        return Err(anyhow!(
            "gh comment failed (exit {:?}): {}",
            r.code,
            truncate(&r.stderr, 500)
        ));
    }
    match serde_json::from_str::<Value>(&r.stdout) {
        Ok(v) => Ok(v
            .get("html_url")
            .and_then(|u| u.as_str())
            .map(|s| s.to_string())),
        Err(e) => {
            // The comment did post (exit 0); only the confirmation is unreadable.
            logging::warn(
                "github_comment_response_unparseable",
                json!({ "target": target, "err": e.to_string(), "head": truncate(&r.stdout, 200) }),
            );
            Ok(None)
        }
    }
}
