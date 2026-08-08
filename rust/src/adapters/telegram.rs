//! Telegram channel — the human's control surface: a summary lands on your
//! phone and you answer/approve from there.
//!
//! Long-polling `getUpdates` means NO public endpoint and no inbound firewall
//! hole (unlike a webhook). The bot token comes from the env var named in
//! `adapters.telegram.token_env`; without it the adapter SKIPS WITH A LOG.
//!
//! Setup: @BotFather → /newbot → copy token →
//!   export HUB_TELEGRAM_TOKEN=123456:AA...
//! then message your bot once and run `hub doctor` to learn your chat id, and
//! put that id in adapters.telegram.allowed_chat_ids.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::adapters::{ChannelCommand, CommandKind, Health, PollResult, Skip};
use crate::config::{secret_from_env, TelegramCfg};
use crate::db::NewMessage;
use crate::logging;

pub const NAME: &str = "telegram";

fn api(token: &str, method: &str, payload: Value, timeout: Duration) -> Result<Value> {
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| anyhow!("cannot build http client: {e}"))?;
    let res = client
        .post(format!("https://api.telegram.org/bot{token}/{method}"))
        .json(&payload)
        .send()
        // `without_url` is load-bearing: the bot token lives in the URL path and
        // reqwest's Display appends "for url (...)", which would write the token
        // into logs/hub.log, runs.err, dead_letter and the /api/doctor response
        // on any transport hiccup (a dropped wifi during the 20s long-poll is
        // routine). Rotating hub.env would not clean those copies.
        .map_err(|e| anyhow!("telegram {method} failed: {}", e.without_url()))?;

    let status = res.status();
    let body: Value = res.json().unwrap_or(Value::Null);
    let ok = body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if !status.is_success() || !ok {
        return Err(anyhow!(
            "telegram {method} failed: HTTP {} {}",
            status.as_u16(),
            crate::exec::truncate(&body.to_string(), 200)
        ));
    }
    Ok(body.get("result").cloned().unwrap_or(Value::Null))
}

pub fn health(cfg: &TelegramCfg) -> Health {
    let token = match secret_from_env(&cfg.token_env) {
        Some(t) => t,
        None => {
            return Health {
                ok: false,
                detail: format!("{} not set", cfg.token_env),
            }
        }
    };
    match api(&token, "getMe", json!({}), Duration::from_secs(15)) {
        Ok(me) => Health {
            ok: true,
            detail: format!(
                "bot @{}",
                me.get("username").and_then(|v| v.as_str()).unwrap_or("?")
            ),
        },
        Err(e) => Health {
            ok: false,
            detail: e.to_string(),
        },
    }
}

/// Chat ids that have written to the bot recently — used by `hub doctor`.
pub fn observed_chat_ids(cfg: &TelegramCfg) -> Result<Vec<String>> {
    let token = match secret_from_env(&cfg.token_env) {
        Some(t) => t,
        None => return Ok(vec![]),
    };
    let updates = api(
        &token,
        "getUpdates",
        json!({ "timeout": 0, "limit": 20 }),
        Duration::from_secs(20),
    )?;
    let mut out: Vec<String> = vec![];
    for u in updates.as_array().cloned().unwrap_or_default() {
        let chat = u
            .get("message")
            .or_else(|| u.get("edited_message"))
            .and_then(|m| m.get("chat"));
        if let Some(chat) = chat {
            let id = chat.get("id").map(|v| v.to_string()).unwrap_or_default();
            let who = chat
                .get("username")
                .or_else(|| chat.get("title"))
                .or_else(|| chat.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let entry = format!("{id} ({who})");
            if !out.contains(&entry) {
                out.push(entry);
            }
        }
    }
    Ok(out)
}

pub fn poll(cfg: &TelegramCfg, cursors: &BTreeMap<String, String>) -> Result<PollResult> {
    let token = secret_from_env(&cfg.token_env).ok_or_else(|| {
        anyhow!(Skip(format!(
            "{} not set — telegram channel disabled",
            cfg.token_env
        )))
    })?;

    let offset: Option<i64> = cursors.get("telegram:offset").and_then(|v| v.parse().ok());
    let allowed: Vec<String> = cfg.allowed_chat_ids.iter().map(|s| s.to_string()).collect();

    let mut payload = json!({ "timeout": cfg.poll_timeout_sec, "allowed_updates": ["message", "callback_query"] });
    if let Some(o) = offset {
        payload["offset"] = json!(o);
    }
    let updates = api(
        &token,
        "getUpdates",
        payload,
        Duration::from_secs(cfg.poll_timeout_sec + 15),
    )?;
    let updates = updates.as_array().cloned().unwrap_or_default();

    let mut out = PollResult::default();
    let mut max_update_id = offset.map(|o| o - 1).unwrap_or(0);

    for u in &updates {
        let update_id = u.get("update_id").and_then(|v| v.as_i64()).unwrap_or(0);
        max_update_id = max_update_id.max(update_id);

        // Button press on a brief → a command for the pipeline to execute.
        if let Some(cb) = u.get("callback_query") {
            match parse_callback(cb, &allowed) {
                Ok(Some(cmd)) => out.commands.push(cmd),
                Ok(None) => {}
                Err(e) => {
                    logging::warn("telegram_callback_ignored", json!({ "err": e.to_string() }))
                }
            }
            continue;
        }

        let m = match u.get("message") {
            Some(m) => m,
            None => continue,
        };
        let text = match m.get("text").and_then(|v| v.as_str()) {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => continue,
        };
        let chat_id = m
            .get("chat")
            .and_then(|c| c.get("id"))
            .map(|v| v.to_string())
            .unwrap_or_default();

        if !allowed.is_empty() && !allowed.contains(&chat_id) {
            // Stranger messaging the bot: recorded, not ingested. Never silent.
            logging::warn(
                "telegram_chat_not_allowed",
                json!({
                    "chat_id": chat_id,
                    "from": m.get("from").and_then(|f| f.get("username")),
                    "text_len": text.len(),
                }),
            );
            continue;
        }

        let from = m
            .get("from")
            .and_then(|f| {
                f.get("username")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| f.get("id").map(|v| v.to_string()))
            })
            .unwrap_or_else(|| chat_id.clone());
        let received_at = m
            .get("date")
            .and_then(|v| v.as_i64())
            .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
            .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));

        out.messages.push(NewMessage {
            source: NAME.into(),
            external_id: format!("tg:{update_id}"),
            thread_key: Some(format!("telegram:{chat_id}")),
            project: None,
            sender: Some(format!("telegram:{from}")),
            // Only ids in allowed_chat_ids reach here, so this is the owner.
            sender_trust: Some(if allowed.contains(&chat_id) { "trusted".into() } else { "untrusted".to_string() }),
            subject: Some(text.lines().next().unwrap_or("").chars().take(120).collect()),
            body: Some(text),
            url: None,
            received_at,
            raw: Some(json!({ "stream": "telegram", "chat_id": chat_id, "update_id": update_id, "from": from })),
        });
    }

    // Telegram requires offset = last_update_id + 1 to acknowledge.
    if !updates.is_empty() {
        out.cursors
            .insert("telegram:offset".into(), (max_update_id + 1).to_string());
    }
    Ok(out)
}

/// Parse a `callback_query` (button press) into a command for the pipeline.
/// Returns `Ok(None)` when the press is from a chat we do not accept.
pub fn parse_callback(cb: &Value, allowed: &[String]) -> Result<Option<ChannelCommand>> {
    let callback_id = cb
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let chat_id = cb
        .get("message")
        .and_then(|m| m.get("chat"))
        .and_then(|c| c.get("id"))
        .map(|v| v.to_string())
        .unwrap_or_default();

    if !allowed.is_empty() && !allowed.contains(&chat_id) {
        logging::warn(
            "telegram_callback_not_allowed",
            json!({ "chat_id": chat_id }),
        );
        return Ok(None);
    }

    let data = cb.get("data").and_then(|v| v.as_str()).unwrap_or_default();
    let (verb, id) = data
        .split_once(':')
        .ok_or_else(|| anyhow!("callback data \"{data}\" is not <verb>:<id>"))?;
    let decision_id: i64 = id
        .parse()
        .map_err(|_| anyhow!("callback data \"{data}\" has a non-numeric id"))?;
    let kind = match verb {
        "a" => CommandKind::Approve,
        "r" => CommandKind::Reject,
        other => return Err(anyhow!("unknown callback verb \"{other}\"")),
    };

    Ok(Some(ChannelCommand {
        kind,
        // Telegram buttons carry no free text.
        arg: String::new(),
        decision_id,
        chat_id,
        callback_id,
        message_id: cb
            .get("message")
            .and_then(|m| m.get("message_id"))
            .and_then(|v| v.as_i64()),
    }))
}

/// Stop the button's spinner and show a toast in the Telegram client.
pub fn answer_callback(cfg: &TelegramCfg, callback_id: &str, text: &str) -> Result<()> {
    let token = secret_from_env(&cfg.token_env)
        .ok_or_else(|| anyhow!("cannot answer callback: {} not set", cfg.token_env))?;
    api(
        &token,
        "answerCallbackQuery",
        json!({ "callback_query_id": callback_id, "text": text.chars().take(190).collect::<String>() }),
        Duration::from_secs(20),
    )?;
    Ok(())
}

/// Rewrite the brief after the button was pressed, and drop the keyboard so it
/// cannot be pressed twice.
pub fn finalize_message(
    cfg: &TelegramCfg,
    chat_id: &str,
    message_id: i64,
    text: &str,
) -> Result<()> {
    let token = secret_from_env(&cfg.token_env)
        .ok_or_else(|| anyhow!("cannot edit message: {} not set", cfg.token_env))?;
    api(
        &token,
        "editMessageText",
        json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "text": text.chars().take(4000).collect::<String>(),
            "disable_web_page_preview": true,
            "reply_markup": { "inline_keyboard": [] }
        }),
        Duration::from_secs(20),
    )?;
    Ok(())
}

/// `target` = chat id
pub fn send(
    cfg: &TelegramCfg,
    target: &str,
    subject: Option<&str>,
    body: &str,
) -> Result<Option<String>> {
    send_with_approval(cfg, target, subject, body, None)
}

/// Same, plus Approve/Reject buttons when the brief is still awaiting a human.
pub fn send_with_approval(
    cfg: &TelegramCfg,
    target: &str,
    subject: Option<&str>,
    body: &str,
    pending_decision: Option<i64>,
) -> Result<Option<String>> {
    let token = secret_from_env(&cfg.token_env)
        .ok_or_else(|| anyhow!("cannot send telegram: {} not set", cfg.token_env))?;
    let text = match subject {
        Some(s) if !s.is_empty() => format!("{s}\n\n{body}"),
        _ => body.to_string(),
    };
    // Plain text on purpose: a reply draft full of backticks/underscores would
    // break Markdown parsing and Telegram would reject the whole message.
    let mut payload = json!({
        "chat_id": target,
        "text": text.chars().take(4000).collect::<String>(),
        "disable_web_page_preview": true
    });
    if let Some(id) = pending_decision {
        payload["reply_markup"] = json!({
            "inline_keyboard": [[
                { "text": "✅ Duyệt", "callback_data": format!("a:{id}") },
                { "text": "🚫 Bỏ", "callback_data": format!("r:{id}") }
            ]]
        });
    }
    let res = api(&token, "sendMessage", payload, Duration::from_secs(40))?;
    Ok(res.get("message_id").map(|v| v.to_string()))
}
