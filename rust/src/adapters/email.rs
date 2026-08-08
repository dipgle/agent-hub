//! Email channel over the house mail server (mailler, live at mail.dipgle.com).
//!
//! Webmail REST with a per-user API key (Bearer), verified against mailler's
//! source (crates/server/src/main.rs:660 routes, Bearer auth at 490-503):
//!   GET  /api/v1/messages?folder=inbox&limit=N   → list
//!   GET  /api/v1/messages/:id                    → body
//!   POST /api/v1/messages {to,cc,bcc,subject,text} → compose/send
//!
//! Mint a key in webmail → Settings → API keys, then export it:
//!   export HUB_MAILLER_API_KEY=...
//! Without the key this adapter SKIPS WITH A LOG (never silently).

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::adapters::{Health, PollResult, Skip};
use crate::config::{secret_from_env, EmailCfg};
use crate::db::NewMessage;
use crate::logging;

pub const NAME: &str = "email";
const MAX_BODY: usize = 20_000;

fn client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| anyhow!("cannot build http client: {e}"))
}

fn api(cfg: &EmailCfg, key: &str, path: &str, body: Option<Value>) -> Result<Value> {
    let url = format!("{}{}", cfg.base_url.trim_end_matches('/'), path);
    let c = client()?;
    let req = match &body {
        Some(b) => c.post(&url).json(b),
        None => c.get(&url),
    };
    let res = req
        .header("Authorization", format!("Bearer {key}"))
        .header("Accept", "application/json")
        .send()
        .map_err(|e| {
            anyhow!(
                "{} {path} failed: {e}",
                if body.is_some() { "POST" } else { "GET" }
            )
        })?;

    let status = res.status();
    let text = res.text().unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!(
            "{path} → HTTP {}: {}",
            status.as_u16(),
            crate::exec::truncate(&text, 300)
        ));
    }
    serde_json::from_str(&text).map_err(|e| {
        anyhow!(
            "{path} returned non-JSON: {e}; head={}",
            crate::exec::truncate(&text, 200)
        )
    })
}

pub fn health(cfg: &EmailCfg) -> Health {
    let key = match secret_from_env(&cfg.api_key_env) {
        Some(k) => k,
        None => {
            return Health {
                ok: false,
                detail: format!("{} not set", cfg.api_key_env),
            }
        }
    };
    match api(cfg, &key, "/api/v1/auth/me", None) {
        Ok(me) => {
            let who = me
                .get("user")
                .and_then(|u| u.get("address").or_else(|| u.get("email")))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            Health {
                ok: true,
                detail: format!("authenticated as {who}"),
            }
        }
        Err(e) => Health {
            ok: false,
            detail: e.to_string(),
        },
    }
}

fn pick<'a>(obj: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    for k in keys {
        match obj.get(*k) {
            Some(Value::Null) | None => continue,
            Some(v) => return Some(v),
        }
    }
    None
}

fn as_text(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
        None => None,
    }
}

pub fn poll(cfg: &EmailCfg, cursors: &BTreeMap<String, String>) -> Result<PollResult> {
    let key = secret_from_env(&cfg.api_key_env).ok_or_else(|| {
        anyhow!(Skip(format!(
            "{} not set — email ingest disabled",
            cfg.api_key_env
        )))
    })?;

    let listed = api(
        cfg,
        &key,
        &format!("/api/v1/messages?folder={}&limit={}", cfg.folder, cfg.limit),
        None,
    )?;
    let items: Vec<Value> = match &listed {
        Value::Array(a) => a.clone(),
        other => other
            .get("messages")
            .or_else(|| other.get("items"))
            .and_then(|v| v.as_array())
            .cloned()
            .ok_or_else(|| {
                anyhow!(
                    "unexpected list shape: {}",
                    crate::exec::truncate(&other.to_string(), 200)
                )
            })?,
    };

    let ids: Vec<i64> = items
        .iter()
        .filter_map(|m| pick(m, &["id", "message_id", "uid"]).and_then(|v| v.as_i64()))
        .collect();

    // First run takes the current inbox tip as the baseline: the hub answers
    // mail that arrives from now on rather than re-litigating the whole mailbox.
    if !cursors.contains_key("email:last_id") && !cfg.backfill {
        let tip = ids.iter().copied().max().unwrap_or(0);
        logging::info(
            "email_baseline_set",
            json!({ "last_id": tip, "seen": ids.len() }),
        );
        let mut out = PollResult::default();
        out.cursors.insert("email:last_id".into(), tip.to_string());
        return Ok(out);
    }

    let last_seen: i64 = cursors
        .get("email:last_id")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut out = PollResult::default();
    let mut max_id = last_seen;

    for m in &items {
        let id = match pick(m, &["id", "message_id", "uid"]).and_then(|v| v.as_i64()) {
            Some(i) => i,
            None => {
                logging::warn(
                    "email_item_without_id",
                    json!({ "item": crate::exec::truncate(&m.to_string(), 200) }),
                );
                continue;
            }
        };
        if id <= last_seen {
            continue;
        }
        max_id = max_id.max(id);

        // The list endpoint returns headers + snippet; fetch the body.
        let mut body = as_text(pick(m, &["text", "snippet", "preview"])).unwrap_or_default();
        match api(cfg, &key, &format!("/api/v1/messages/{id}"), None) {
            Ok(full) => {
                let msg = full.get("message").cloned().unwrap_or(full);
                if let Some(t) = as_text(pick(&msg, &["text", "body_text", "body", "html"])) {
                    body = t;
                }
            }
            Err(e) => {
                logging::warn(
                    "email_body_fetch_failed",
                    json!({ "id": id, "err": e.to_string() }),
                );
                body = format!("{body}\n\n[hub: body fetch failed: {e}]");
            }
        }

        let from = pick(m, &["from", "from_address", "sender"])
            .map(|v| match v {
                Value::String(s) => s.clone(),
                Value::Object(o) => o
                    .get("address")
                    .and_then(|a| a.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| v.to_string()),
                other => other.to_string(),
            })
            .unwrap_or_else(|| "unknown".into());
        let subject = as_text(pick(m, &["subject"])).unwrap_or_else(|| "(no subject)".into());

        out.messages.push(NewMessage {
            source: NAME.into(),
            external_id: format!("mailler:{id}"),
            thread_key: Some(
                as_text(pick(m, &["thread_id", "message_id_header"]))
                    .unwrap_or_else(|| format!("email:{subject}")),
            ),
            project: None,
            sender: Some(from),
            sender_trust: None,
            subject: Some(subject),
            body: Some(crate::exec::truncate(&body, MAX_BODY)),
            url: Some(format!(
                "{}/#/message/{id}",
                cfg.base_url.trim_end_matches('/')
            )),
            received_at: as_text(pick(m, &["date", "received_at", "created_at"])),
            raw: Some(json!({ "stream": "mailler_inbox", "id": id, "folder": cfg.folder })),
        });
    }

    if max_id > last_seen {
        out.cursors
            .insert("email:last_id".into(), max_id.to_string());
    }
    Ok(out)
}

/// `target` = recipient address
pub fn send(
    cfg: &EmailCfg,
    target: &str,
    subject: Option<&str>,
    body: &str,
) -> Result<Option<String>> {
    let key = secret_from_env(&cfg.api_key_env)
        .ok_or_else(|| anyhow!("cannot send email: {} not set", cfg.api_key_env))?;
    let payload = json!({
        "to": target,
        "cc": "",
        "bcc": "",
        "subject": subject.unwrap_or("(no subject)"),
        "text": body,
    });
    let res = api(cfg, &key, "/api/v1/messages", Some(payload))?;
    Ok(res.get("id").map(|v| v.to_string()))
}
