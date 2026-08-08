//! Policy gate — decides what the hub is ALLOWED to do with a decision.
//!
//! Autonomy tiers (per project, `autonomy.projects`):
//!   L0  draft only — nothing leaves the machine before a human approves
//!   L1  may auto-send an informational reply on the same channel
//!   L2  may additionally run the act stage (code change on a branch)
//!
//! Invariants no config can loosen:
//!   * an untrusted sender caps the tier at L0
//!   * deploy / merge / force-push / data deletion / secret rotation need a human
//!   * a tripwire hit (prompt injection) forces human review
//!   * `kind=security` is never auto-answered

use serde::Serialize;
use serde_json::Value;

use crate::config::{Config, ALWAYS_HUMAN_ACTIONS, TIERS};
use crate::db::Message;
use crate::triage::Decision;

pub fn tier_rank(tier: &str) -> u8 {
    match tier {
        "L2" => 2,
        "L1" => 1,
        _ => 0,
    }
}

const AUTO_REPLY_KINDS: [&str; 3] = ["question", "status_update", "feature_request"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    AutoReply,
    AwaitHuman,
    Ignore,
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::AutoReply => "auto_reply",
            Action::AwaitHuman => "await_human",
            Action::Ignore => "ignore",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub action: Action,
    pub channel: Option<String>,
    pub target: Option<String>,
    pub reason: String,
    pub human_only: Vec<String>,
}

/// "owner/repo#123" for a GitHub message that has a thread to reply on.
pub fn github_reply_target(msg: &Message, raw: &Value) -> Option<String> {
    let repo = raw.get("repo")?.as_str()?;
    let num = raw
        .get("detail")
        .and_then(|d| d.get("number"))
        .and_then(|n| n.as_i64())
        .or_else(|| raw.get("number").and_then(|n| n.as_i64()))
        .or_else(|| {
            raw.get("issue_url")
                .and_then(|u| u.as_str())
                .and_then(|u| u.rsplit('/').next())
                .and_then(|n| n.parse::<i64>().ok())
        })
        .or_else(|| {
            let url = msg.url.as_deref()?;
            let re = regex::Regex::new(r"/(issues|pull)/(\d+)").ok()?;
            re.captures(url)?.get(2)?.as_str().parse::<i64>().ok()
        })?;
    if num > 0 {
        Some(format!("{repo}#{num}"))
    } else {
        None
    }
}

/// Bare address out of "Name <a@b.com>".
pub fn email_address(sender: Option<&str>) -> Option<String> {
    let s = sender?;
    let inner = match (s.find('<'), s.find('>')) {
        (Some(a), Some(b)) if b > a + 1 => &s[a + 1..b],
        _ => s,
    };
    let t = inner.trim().to_lowercase();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

/// Shared matcher, so the project registry and the legacy routing table can
/// never disagree about what "matches" means. Every unset field matches
/// anything; an empty matcher therefore matches everything, which is why the
/// caller must only reach it for rules the operator actually wrote.
fn matches_when(
    w: &crate::config::RoutingWhen,
    msg: &Message,
    raw_repo: Option<&str>,
    raw_chat: Option<&str>,
    subject: &str,
    body: &str,
) -> bool {
    w.source.as_deref().is_none_or(|s| s == msg.source)
        && w.repo.as_deref().is_none_or(|r| Some(r) == raw_repo)
        && w.sender.as_deref().is_none_or(|s| {
            msg.sender
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(&s.to_lowercase())
        })
        && w.chat_id.as_ref().is_none_or(|c| {
            let want = match c {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            raw_chat == Some(want.as_str())
        })
        && w.subject_contains
            .as_deref()
            .is_none_or(|s| subject.to_lowercase().contains(&s.to_lowercase()))
        && w.body_contains
            .as_deref()
            .is_none_or(|s| body.to_lowercase().contains(&s.to_lowercase()))
}

/// Which project does this belong to? explicit → registry → legacy routing → heuristics.
pub fn resolve_project(msg: &Message, cfg: &Config, known: &[String]) -> Option<String> {
    if let Some(p) = msg.project.as_ref().filter(|p| !p.is_empty()) {
        return Some(p.clone());
    }
    let raw = msg.raw_json();
    let raw_repo = raw.get("repo").and_then(|v| v.as_str());
    let raw_chat = raw.get("chat_id").map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    });
    let subject = msg.subject.clone().unwrap_or_default();
    let body = msg.body.clone().unwrap_or_default();

    // THE registry first: a project owns its repos and matchers, so there is
    // one place to look and the key is the folder the work actually lives in.
    for (name, p) in &cfg.projects {
        if raw_repo.is_some_and(|r| p.repos.iter().any(|want| want == r)) {
            return Some(name.clone());
        }
        if p.matchers
            .iter()
            .any(|w| matches_when(w, msg, raw_repo, raw_chat.as_deref(), &subject, &body))
        {
            return Some(name.clone());
        }
    }

    // Legacy table, still honoured so an existing config keeps working.
    for rule in &cfg.routing {
        let w = &rule.when;
        if matches_when(w, msg, raw_repo, raw_chat.as_deref(), &subject, &body) {
            return Some(rule.project.clone());
        }
    }

    // "dipgle/tfl5" → tfl5 when a project folder of that name exists.
    if let Some(repo) = raw_repo {
        if let Some(name) = repo.split('/').next_back() {
            if known.iter().any(|k| k == name) {
                return Some(name.to_string());
            }
        }
    }

    // "[tfl5] …" or "tfl5: …" at the start of a subject.
    let re =
        regex::Regex::new(r"(?i)^\s*[\[(]([a-z0-9._-]{2,30})[\])]|^\s*([a-z0-9._-]{2,30})\s*:")
            .ok()?;
    let caps = re.captures(&subject)?;
    let candidate = caps.get(1).or_else(|| caps.get(2))?.as_str().to_lowercase();
    if known.contains(&candidate) {
        return Some(candidate);
    }
    None
}

/// "trusted" | "untrusted" — who may make the hub act, not merely answer.
pub fn resolve_trust(msg: &Message, cfg: &Config) -> String {
    let t = &cfg.trust;
    if t.trusted_sources.contains(&msg.source) {
        return "trusted".into();
    }

    // GitHub logins, email addresses and Telegram chat ids used to be checked
    // here. Those sources are gone (2026-08-08), so the only identity hub still
    // resolves is a tfl5 account.
    if msg.source == "tfl5" {
        // Being allowed into the room is tfl5's decision; being trusted by hub
        // is a separate list the owner keeps. Never infer one from the other.
        let raw = msg.raw_json();
        if let Some(uid) = raw.get("from_user_tid").and_then(Value::as_str) {
            if t.tfl5_user_tids.iter().any(|u| u == uid) {
                return "trusted".into();
            }
        }
    }
    "untrusted".into()
}

pub fn effective_tier(project: Option<&str>, trust: &str, cfg: &Config) -> String {
    let configured = project
        .and_then(|p| {
            // Registry wins; the legacy per-project map still works.
            cfg.projects
                .get(p)
                .and_then(|c| c.tier.clone())
                .or_else(|| cfg.autonomy.projects.get(p).cloned())
        })
        .unwrap_or_else(|| cfg.autonomy.default.0.clone());
    let base = if TIERS.contains(&configured.as_str()) {
        configured
    } else {
        "L0".to_string()
    };
    // Untrusted senders can never move the hub beyond drafting.
    if trust == "trusted" {
        base
    } else {
        "L0".into()
    }
}

/// Actions in the decision a human must green-light no matter what.
pub fn human_only_actions(decision: &Decision) -> Vec<String> {
    decision
        .proposed_actions
        .iter()
        .map(|a| a.action_type.clone())
        .filter(|t| ALWAYS_HUMAN_ACTIONS.contains(&t.as_str()))
        .collect()
}

pub struct OutcomeInput<'a> {
    pub msg: &'a Message,
    pub decision: &'a Decision,
    pub tier: &'a str,
    pub trust: &'a str,
    pub tripwire: &'a [String],
    pub cfg: &'a Config,
}

pub fn decide_outcome(input: OutcomeInput<'_>) -> Outcome {
    let OutcomeInput {
        msg,
        decision,
        tier,
        trust,
        tripwire,
        cfg,
    } = input;
    let raw = msg.raw_json();
    let human_only = human_only_actions(decision);

    // Where an answer would go, per source. `cli` talks back through the local
    // notify channel; `devlog` has nobody to answer, so it needs a human.
    let target = match msg.source.as_str() {
        "github" => github_reply_target(msg, &raw),
        "email" => email_address(msg.sender.as_deref()),
        "telegram" => raw.get("chat_id").map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }),
        // The room the message came from, pinned onto the row at ingest. Taken
        // from `raw` rather than from config so an approved reply still lands
        // in the conversation it answers, even if the config moved on.
        "tfl5" => match (
            raw.get("app_tid").and_then(Value::as_str),
            raw.get("room").and_then(Value::as_str),
        ) {
            (Some(app), Some(room)) if !app.is_empty() && !room.is_empty() => {
                Some(crate::adapters::tfl5::target_of(app, room))
            }
            _ => None,
        },
        "cli" => Some("local".to_string()),
        _ => None,
    };
    let channel = Some(if msg.source == "cli" {
        "notify".to_string()
    } else {
        msg.source.clone()
    });

    let mk = |action: Action, reason: String| Outcome {
        action,
        channel: channel.clone(),
        target: target.clone(),
        reason,
        human_only: human_only.clone(),
    };

    if !tripwire.is_empty() {
        return mk(
            Action::AwaitHuman,
            format!("tripwire: {}", tripwire.join(", ")),
        );
    }
    if ["spam", "noise"].contains(&decision.kind.as_str()) && !decision.needs_human {
        return mk(Action::Ignore, format!("kind={}", decision.kind));
    }
    if decision.needs_human {
        return mk(Action::AwaitHuman, "model set needs_human".into());
    }
    if !human_only.is_empty() {
        return mk(
            Action::AwaitHuman,
            format!("action requires human: {}", human_only.join(", ")),
        );
    }
    if decision.kind == "security" {
        return mk(
            Action::AwaitHuman,
            "security items are never auto-answered".into(),
        );
    }
    if tier_rank(tier) < tier_rank("L1") {
        return mk(
            Action::AwaitHuman,
            format!("tier {tier} (trust={trust}) drafts only"),
        );
    }
    if !AUTO_REPLY_KINDS.contains(&decision.kind.as_str()) {
        return mk(
            Action::AwaitHuman,
            format!("kind={} is not auto-repliable", decision.kind),
        );
    }
    if decision.confidence < cfg.triage.min_confidence_auto {
        return mk(
            Action::AwaitHuman,
            format!(
                "confidence {} < {}",
                decision.confidence, cfg.triage.min_confidence_auto
            ),
        );
    }
    if decision.reply_draft.trim().is_empty() {
        return mk(Action::AwaitHuman, "empty reply_draft".into());
    }
    if target.is_none() {
        return mk(
            Action::AwaitHuman,
            format!("no reply target for source={}", msg.source),
        );
    }
    mk(
        Action::AutoReply,
        format!("tier {tier}, confidence {}", decision.confidence),
    )
}

/// Compact human-facing brief used for the notify / telegram channel.
pub fn human_brief(
    msg: &Message,
    decision: &Decision,
    outcome: &Outcome,
    tier: &str,
    decision_id: i64,
) -> String {
    let acts = decision
        .proposed_actions
        .iter()
        .map(|a| format!("{}: {}", a.action_type, a.detail))
        .collect::<Vec<_>>()
        .join("\n  ");

    let mut lines = vec![
        format!(
            "#{decision_id} [{}/{}] {} — {}",
            decision.kind,
            decision.severity,
            if decision.project.is_empty() {
                "unknown".into()
            } else {
                decision.project.clone()
            },
            msg.source
        ),
        format!(
            "từ: {} ({})  tier={tier}  conf={}",
            msg.sender.clone().unwrap_or_else(|| "?".into()),
            msg.sender_trust,
            decision.confidence
        ),
    ];
    if let Some(s) = &msg.subject {
        lines.push(format!("chủ đề: {s}"));
    }
    if let Some(u) = &msg.url {
        lines.push(format!("link: {u}"));
    }
    lines.push(String::new());
    lines.push(format!("tóm tắt: {}", decision.summary));
    if !acts.is_empty() {
        lines.push(format!("đề xuất:\n  {acts}"));
    }
    if !decision.reply_draft.trim().is_empty() {
        lines.push(format!("\nnháp trả lời:\n{}", decision.reply_draft.trim()));
    }
    lines.push(String::new());
    lines.push(format!(
        "→ {} ({})",
        outcome.action.as_str(),
        outcome.reason
    ));
    if outcome.action == Action::AwaitHuman {
        lines.push(format!(
            "duyệt: hub approve {decision_id}   |   bỏ: hub reject {decision_id}"
        ));
    }
    lines.join("\n")
}
