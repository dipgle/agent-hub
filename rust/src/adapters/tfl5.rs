//! tfl5 chat adapter — hub as an OUTBOUND client.
//!
//! WHY THIS SHAPE (the constraint, not a preference)
//! `claude` is installed on this machine, so the brain cannot move to a VPS.
//! The UI lives on tfl5, so a browser cannot reach `127.0.0.1` here. And hub
//! never opens a port (`PLAN.md` — "Không webhook … không mở cổng vào máy").
//! Exactly one shape satisfies all three: **tfl5 is the meeting point and hub
//! dials out**. That is the same posture as every other adapter in this folder
//! — nothing here ever listens.
//!
//! TWO HALVES, TWO PROTOCOLS — because that is what tfl5 offers:
//!   * READ  `POST /app/chat/history`  plain JSON, blocking client
//!   * WRITE `GET  /ws/chat`           **WebSocket is the only way in**
//!     (`ws_chat.rs:181 routes()` mounts history + 3 admin endpoints; there is
//!     no REST send. Verified by reading the router, not by assuming.)
//!
//! tungstenite's client API is blocking, so this file adds no async runtime —
//! the pipeline stays synchronous.
//!
//! TRUST IS TWO LAYERS, DO NOT CONFLATE THEM. tfl5 answers "may this account
//! enter this room" (`require_app_perm`). It does NOT answer "may this person
//! make hub act". Everything arriving here is `untrusted` unless the operator
//! lists the sender in `trust`, which keeps `policy.rs` capping it at L0.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::adapters::{ChannelCommand, CommandKind, Health, PollResult, Skip};
use crate::config::{secret_from_env, Tfl5Cfg};
use crate::logging;

pub const NAME: &str = "tfl5";

/// One logged-in session against tfl5: the `_token` cookie plus who we are.
#[derive(Debug, Clone)]
pub struct Session {
    pub cookie: String,
    pub user_tid: String,
    pub username: String,
}

fn http() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("cannot build http client")
}

fn base(cfg: &Tfl5Cfg) -> &str {
    cfg.base_url.trim_end_matches('/')
}

/// Log in and keep the cookie. A missing credential is a deliberate SKIP —
/// same contract as every other adapter, so `doctor` reports "not configured"
/// instead of the daemon crash-looping.
pub fn login(cfg: &Tfl5Cfg) -> Result<Session> {
    let user =
        secret_from_env(&cfg.user_env).ok_or_else(|| Skip(format!("{} not set", cfg.user_env)))?;
    let pass = secret_from_env(&cfg.password_env)
        .ok_or_else(|| Skip(format!("{} not set", cfg.password_env)))?;

    let res = http()?
        .post(format!("{}/login", base(cfg)))
        .json(&json!({ "username": user, "password": pass }))
        .send()
        .context("tfl5 /login request failed")?;

    let status = res.status();
    // The cookie only exists on the response headers, so read it BEFORE the
    // body is consumed.
    let cookie = res
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|v| v.split(';').next().filter(|c| c.starts_with("_token=")))
        .map(|c| c.to_string());

    let body: Value = res.json().context("tfl5 /login returned non-JSON")?;
    if !status.is_success() || body.get("result").and_then(Value::as_bool) != Some(true) {
        let msg = body.get("msg").and_then(Value::as_str).unwrap_or("unknown");
        let code = body.get("code").and_then(Value::as_str).unwrap_or("");
        return Err(anyhow!(
            "tfl5 login rejected (http {status}, code {code}): {msg}"
        ));
    }
    let cookie = cookie.ok_or_else(|| anyhow!("tfl5 login said ok but set no _token cookie"))?;

    let u = body.get("user").cloned().unwrap_or(Value::Null);
    Ok(Session {
        cookie,
        user_tid: u
            .get("tid")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        username: u
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or(&user)
            .to_string(),
    })
}

/// One page of scrollback, newest first — the shape `/app/chat/history`
/// returns: `{result, app_tid, room, messages:[{tid, from_user_tid, from,
/// text, ts}], next_before_ts, timestamp}`.
pub fn history(
    cfg: &Tfl5Cfg,
    s: &Session,
    limit: i64,
    before_ts: Option<i64>,
) -> Result<Vec<Value>> {
    let mut payload = json!({
        "app_tid": cfg.app_tid,
        "room": cfg.room,
        "limit": limit.clamp(1, 200),
    });
    if let Some(b) = before_ts {
        payload["before_ts"] = json!(b);
    }

    let res = http()?
        .post(format!("{}/app/chat/history", base(cfg)))
        .header(reqwest::header::COOKIE, &s.cookie)
        .json(&payload)
        .send()
        .context("tfl5 /app/chat/history request failed")?;

    let status = res.status();
    let body: Value = res
        .json()
        .context("tfl5 /app/chat/history returned non-JSON")?;
    if !status.is_success() || body.get("result").and_then(Value::as_bool) != Some(true) {
        let msg = body.get("msg").and_then(Value::as_str).unwrap_or("unknown");
        return Err(anyhow!("tfl5 history rejected (http {status}): {msg}"));
    }
    Ok(body
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// One authenticated POST to tfl5, with the envelope checked the way tfl5
/// actually answers: some refusals arrive as HTTP 200 + `result:false`, so the
/// status line alone is never the verdict.
fn post_json(cfg: &Tfl5Cfg, s: &Session, path: &str, payload: &Value) -> Result<Value> {
    let res = http()?
        .post(format!("{}{}", base(cfg), path))
        .header(reqwest::header::COOKIE, &s.cookie)
        .json(payload)
        .send()
        .with_context(|| format!("tfl5 {path} request failed"))?;

    let status = res.status();
    let body: Value = res
        .json()
        .with_context(|| format!("tfl5 {path} returned non-JSON"))?;
    if !status.is_success() || body.get("result").and_then(Value::as_bool) != Some(true) {
        let msg = body.get("msg").and_then(Value::as_str).unwrap_or("unknown");
        let code = body.get("code").and_then(Value::as_str).unwrap_or("");
        return Err(anyhow!(
            "tfl5 refused {path} (http {status}, code {code}): {msg}"
        ));
    }
    Ok(body)
}

/// Make sure a resource with this `ma` exists, creating it once if not
/// (Manager+ on the app). Returns true when it had to create it.
///
/// Why a resource and not a file: files live under the app's public asset
/// tree, where an empty per-file ACL means "anyone may fetch it" — and a
/// non-empty one would also lock the JSON read API. Docs are only reachable
/// through the API, gated by the app's own Reader check, which is exactly the
/// audience the snapshot should have.
pub fn resource_ensure(
    cfg: &Tfl5Cfg,
    s: &Session,
    app_tid: &str,
    ma: &str,
    name: &str,
    description: &str,
) -> Result<bool> {
    let list = post_json(cfg, s, "/app/resource/list", &json!({ "app_tid": app_tid }))?;
    let exists = list
        .get("resources")
        .or_else(|| list.get("data"))
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .any(|r| r.get("ma").and_then(Value::as_str) == Some(ma))
        })
        .unwrap_or(false);
    if exists {
        return Ok(false);
    }

    post_json(
        cfg,
        s,
        "/app/resource/create",
        &json!({
            "app_tid": app_tid,
            "ma": ma,
            "name": name,
            "description": description,
        }),
    )?;
    Ok(true)
}

/// Create-or-update the single doc identified by `match_on` inside `ma`.
pub fn doc_upsert(
    cfg: &Tfl5Cfg,
    s: &Session,
    app_tid: &str,
    ma: &str,
    match_on: Value,
    data: Value,
) -> Result<Value> {
    post_json(
        cfg,
        s,
        "/app/doc/upsert",
        &json!({
            "app_tid": app_tid,
            "resource_ma": ma,
            "match_on": match_on,
            "data": data,
        }),
    )
}

/// Remove a file from the app's storage. Used to clean up an earlier design's
/// leftovers; `stage` is explicit because tfl5's delete defaults to Release.
pub fn delete_file(
    cfg: &Tfl5Cfg,
    s: &Session,
    app_tid: &str,
    path: &str,
    stage: &str,
) -> Result<()> {
    post_json(
        cfg,
        s,
        "/app/file/del",
        &json!({ "app_tid": app_tid, "path": path, "stage": stage }),
    )
    .map(|_| ())
}

/// `http://host` → `ws://host`, `https://` → `wss://`.
///
/// `app_tid`/`room` are passed in rather than read from `cfg` on purpose: an
/// outbox row queued for room A must still land in room A even if the operator
/// re-points the config at room B before the flush.
pub fn ws_url(cfg: &Tfl5Cfg, app_tid: &str, room: &str) -> String {
    let b = base(cfg);
    let scheme = if b.starts_with("https://") {
        "wss://"
    } else {
        "ws://"
    };
    let hostpart = b
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    format!(
        "{scheme}{hostpart}/ws/chat?app_tid={}&room={}",
        urlencode(app_tid),
        urlencode(room)
    )
}

/// Outbox rows address a room as `"<app_tid>:<room>"`. An empty or malformed
/// target falls back to the configured room rather than dropping the reply —
/// but says so in the log, because a silent redirect is its own bug.
pub fn parse_target(cfg: &Tfl5Cfg, target: &str) -> (String, String) {
    match target.split_once(':') {
        Some((app, room)) if !app.is_empty() && !room.is_empty() => {
            (app.to_string(), room.to_string())
        }
        _ => {
            if !target.is_empty() {
                logging::warn(
                    "tfl5_target_unparseable",
                    json!({ "target": target, "falling_back_to": format!("{}:{}", cfg.app_tid, cfg.room) }),
                );
            }
            (cfg.app_tid.clone(), cfg.room.clone())
        }
    }
}

/// The address an outbox row should carry for a room.
pub fn target_of(app_tid: &str, room: &str) -> String {
    format!("{app_tid}:{room}")
}

/// Minimal percent-encoding for the two query values we send. Both are tids /
/// room names, so the alphabet is small — but encoding them anyway keeps a
/// stray space or `&` from silently changing which room we join.
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Turn a failed WebSocket handshake into something an operator can act on.
///
/// tfl5 answers an authenticated-but-unauthorized upgrade with HTTP **200** and
/// a JSON envelope (`error.rs:289` — deliberate w3c wire compat), NOT 403. A
/// client that only asks "was it 101?" reports the useless "HTTP error: 200 OK"
/// and the operator has no idea a role is missing. Only a caller with no
/// session at all sees 401. So dig the envelope out and name the fix.
pub fn ws_connect_error(e: tungstenite::Error, url: &str, cfg: &Tfl5Cfg) -> anyhow::Error {
    let tungstenite::Error::Http(resp) = &e else {
        return anyhow!("tfl5 ws handshake failed at {url}: {e}");
    };
    let status = resp.status();
    let body = resp
        .body()
        .as_ref()
        .map(|b| String::from_utf8_lossy(b).to_string())
        .unwrap_or_default();
    let parsed: Option<Value> = serde_json::from_str(&body).ok();
    let field = |k: &str| {
        parsed
            .as_ref()
            .and_then(|v| v.get(k))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let (code, msg) = (field("code"), field("msg"));
    match code.as_str() {
        "access_denied" => anyhow!(
            "tfl5 từ chối vào phòng: tài khoản {} không có vai trên app {} (code access_denied, http {status}). \
             Cấp cho nó Reader trở lên bằng POST /app/acl-set rồi thử lại.",
            cfg.user_env,
            cfg.app_tid
        ),
        "unauthorized" => anyhow!(
            "tfl5 chưa nhận phiên đăng nhập (code unauthorized, http {status}) — cookie hết hạn hoặc sai {}/{}",
            cfg.user_env,
            cfg.password_env
        ),
        "service_draining" => anyhow!("tfl5 cell đang drain, từ chối socket mới (http {status}) — thử lại sau vài giây"),
        _ if !msg.is_empty() || !code.is_empty() => {
            anyhow!("tfl5 ws handshake bị từ chối (http {status}, code {code}): {msg}")
        }
        _ => anyhow!("tfl5 ws handshake bị từ chối (http {status}) tại {url}"),
    }
}

/// Post one message into the room over `/ws/chat`, then wait for the server to
/// hand it back so the caller knows it was really persisted.
///
/// tfl5 writes the row BEFORE it broadcasts (`ws_chat.rs:19-24`), so seeing our
/// own text come back on the bus IS the receipt. A structured `error` frame
/// (`empty_msg`, `persist_failed`, …) is returned as an `Err` so `outbound.rs`
/// counts the attempt and eventually dead-letters — never a silent drop.
pub fn send_text(
    cfg: &Tfl5Cfg,
    s: &Session,
    app_tid: &str,
    room: &str,
    text: &str,
) -> Result<Option<String>> {
    use tungstenite::client::IntoClientRequest;
    use tungstenite::Message;

    if text.trim().is_empty() {
        return Err(anyhow!("refusing to send an empty message to tfl5 chat"));
    }

    let url = ws_url(cfg, app_tid, room);
    let mut req = url
        .as_str()
        .into_client_request()
        .with_context(|| format!("bad ws url {url}"))?;
    req.headers_mut().insert(
        "cookie",
        tungstenite::http::HeaderValue::from_str(&s.cookie)
            .context("cookie is not a valid header value")?,
    );

    let (mut socket, _res) =
        tungstenite::connect(req).map_err(|e| ws_connect_error(e, &url, cfg))?;

    // Bound the wait: a server that accepts the frame and then goes quiet must
    // not park this thread forever.
    //
    // Bản dựng này KHÔNG bật tính năng TLS của `tungstenite` (`Cargo.toml:30`,
    // `default-features = false`), nên `MaybeTlsStream` chỉ có đúng biến thể
    // `Plain` — `if let` ở đây là toàn phần, không phải bỏ sót nhánh `wss://`.
    // (Địa chỉ `https://` sẽ hỏng ngay ở bước `connect`, một lỗi ồn ào, chứ
    // không âm thầm mất trần đọc.)
    if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
        // Lời hứa "không park thread mãi mãi" phải NÓI khi nó không giữ được:
        // mất trần đọc thì một máy chủ nhận khung rồi im sẽ giữ luôn cả vòng
        // chạy của daemon, tức lệnh từ điện thoại nằm chờ vô hạn.
        if let Err(e) = stream.set_read_timeout(Some(Duration::from_secs(cfg.reply_timeout_sec))) {
            crate::logging::warn(
                "ws_read_timeout_unset",
                serde_json::json!({ "err": e.to_string(), "secs": cfg.reply_timeout_sec }),
            );
        }
    }

    socket
        .send(Message::Text(
            json!({ "type": "msg", "text": text }).to_string(),
        ))
        .context("tfl5 ws send failed")?;

    let mut receipt: Option<String> = None;
    let mut err: Option<String> = None;
    // The server sends `welcome` first, then fans our own row back. Read until
    // one of those two verdicts arrives or the socket goes quiet.
    for _ in 0..12 {
        let frame = match socket.read() {
            Ok(f) => f,
            Err(e) => {
                err = Some(format!("tfl5 ws read failed before receipt: {e}"));
                break;
            }
        };
        let txt = match frame {
            Message::Text(t) => t,
            Message::Close(_) => {
                err = Some("tfl5 closed the socket before echoing the message".into());
                break;
            }
            _ => continue,
        };
        let v: Value = match serde_json::from_str(&txt) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match v.get("type").and_then(Value::as_str).unwrap_or("") {
            "error" => {
                err = Some(format!(
                    "tfl5 rejected the message (code {}): {}",
                    v.get("code").and_then(Value::as_str).unwrap_or("?"),
                    v.get("msg").and_then(Value::as_str).unwrap_or("?")
                ));
                break;
            }
            "msg" if v.get("text").and_then(Value::as_str) == Some(text) => {
                receipt = v.get("tid").and_then(Value::as_str).map(|s| s.to_string());
                break;
            }
            _ => continue,
        }
    }

    let _ = socket.close(None);

    if let Some(e) = err {
        return Err(anyhow!(e));
    }
    match receipt {
        Some(tid) => {
            logging::info("tfl5_chat_sent", json!({ "room": room, "app_tid": app_tid, "tid": tid }));
            Ok(Some(tid))
        }
        // Persisted-or-not is genuinely unknown here. Say so instead of
        // reporting a success the caller cannot verify.
        None => Err(anyhow!(
            "tfl5 accepted the socket but never echoed the message back within {}s — delivery unconfirmed",
            cfg.reply_timeout_sec
        )),
    }
}

/// The outbound-channel entry point. `target` is `"<app_tid>:<room>"`.
pub fn send(
    cfg: &Tfl5Cfg,
    target: &str,
    _subject: Option<&str>,
    body: &str,
) -> Result<Option<String>> {
    let (app_tid, room) = parse_target(cfg, target);
    let s = login(cfg)?;
    send_text(cfg, &s, &app_tid, &room, body)
}

/// A live socket held open on the room, so tfl5 can push instead of hub asking.
///
/// This is the same `/ws/chat` the send path uses — the difference is only that
/// it stays open. tfl5 already fans every message in the room out to every
/// subscribed socket (`ws_chat.rs` ChatBus, plus cross-cell `pg_notify`), so
/// nothing on the server needs to change for this to work.
pub struct Live {
    socket: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    pub app_tid: String,
    pub room: String,
    pub self_user_tid: String,
}

/// Open the room and stay on it.
pub fn connect_live(cfg: &Tfl5Cfg, s: &Session) -> Result<Live> {
    use tungstenite::client::IntoClientRequest;

    let url = ws_url(cfg, &cfg.app_tid, &cfg.room);
    let mut req = url
        .as_str()
        .into_client_request()
        .with_context(|| format!("bad ws url {url}"))?;
    req.headers_mut().insert(
        "cookie",
        tungstenite::http::HeaderValue::from_str(&s.cookie)
            .context("cookie is not a valid header value")?,
    );
    let (socket, _) = tungstenite::connect(req).map_err(|e| ws_connect_error(e, &url, cfg))?;
    Ok(Live {
        socket,
        app_tid: cfg.app_tid.clone(),
        room: cfg.room.clone(),
        self_user_tid: s.user_tid.clone(),
    })
}

impl Live {
    /// Bound how long a read blocks, so the caller can flush its debounce
    /// buffer and notice shutdown even while the room is quiet.
    pub fn set_read_timeout(&mut self, d: Duration) {
        // Xem chú thích ở `say`: bản dựng này không có biến thể TLS nào.
        if let tungstenite::stream::MaybeTlsStream::Plain(s) = self.socket.get_mut() {
            if let Err(e) = s.set_read_timeout(Some(d)) {
                crate::logging::warn(
                    "ws_read_timeout_unset",
                    serde_json::json!({ "err": e.to_string(), "ms": d.as_millis() as u64 }),
                );
            }
        }
    }

    /// Next chat message from the room, or `None` when the read timed out with
    /// nothing to report. `Err` means the socket is finished — reconnect.
    ///
    /// Frames the caller does not need (`welcome`, `pong`) are swallowed here;
    /// an `error` frame is surfaced because a server that rejected something
    /// must never look like silence.
    pub fn next_message(&mut self) -> Result<Option<Value>> {
        let frame = match self.socket.read() {
            Ok(f) => f,
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(None)
            }
            Err(e) => return Err(anyhow!("tfl5 live socket read failed: {e}")),
        };
        match frame {
            tungstenite::Message::Text(t) => {
                let v: Value = serde_json::from_str(&t).unwrap_or(Value::Null);
                match v.get("type").and_then(Value::as_str).unwrap_or("") {
                    "msg" => Ok(Some(v)),
                    "error" => Err(anyhow!(
                        "tfl5 live socket error frame (code {}): {}",
                        v.get("code").and_then(Value::as_str).unwrap_or("?"),
                        v.get("msg").and_then(Value::as_str).unwrap_or("?")
                    )),
                    _ => Ok(None),
                }
            }
            tungstenite::Message::Close(c) => Err(anyhow!("tfl5 closed the live socket: {c:?}")),
            // Keep the connection honest through idle NATs and proxies.
            tungstenite::Message::Ping(p) => {
                let _ = self.socket.send(tungstenite::Message::Pong(p));
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    pub fn close(mut self) {
        let _ = self.socket.close(None);
    }
}

/// A slash command typed in the room, if this text is one AND the author is
/// allowed to give hub orders.
///
/// The trust check is the whole point: being in the room is tfl5's decision,
/// but starting a session on this Mac — or asking one a question, and paying
/// for the answer — is the owner's. Anyone else typing `/new` is just a person
/// typing text.
pub fn parse_command(
    text: &str,
    from_user_tid: &str,
    owner_tids: &[String],
) -> Option<(CommandKind, i64, String)> {
    let t = text.trim();
    if !t.starts_with('/') {
        return None;
    }
    if !owner_tids.iter().any(|u| u == from_user_tid) {
        logging::warn(
            "tfl5_command_from_non_owner",
            json!({ "from_user_tid": from_user_tid, "head": crate::exec::truncate(t, 40) }),
        );
        return None;
    }
    let mut parts = t[1..].splitn(3, char::is_whitespace);
    let verb = parts.next().unwrap_or("").to_lowercase();
    // No verb takes a numeric id any more — the ones that did (`/approve 12`,
    // `/close 87`) pointed at inbox rows. Session verbs take a session id or
    // nothing, and each one re-splits the text for itself.
    match verb.as_str() {
        "help" | "?" => Some((CommandKind::Help, 0, String::new())),
        // `/approve` `/reject` `/close` `/reply` `/act` were parsed here until
        // 2026-08-08. They acted on an inbox that no longer exists, and a verb
        // that parses but has no handler is the worst of both: the room accepts
        // it, nothing happens, and nothing says so. Unparsed, they are ordinary
        // text — which is the truth.
        // Whole-cycle verbs — the console's header and health buttons. They
        // take no id, so whatever followed the verb is ignored.
        "ingest" | "poll" => Some((CommandKind::Ingest, 0, String::new())),
        "run" | "cycle" => Some((CommandKind::Run, 0, String::new())),
        "doctor" | "health" => Some((CommandKind::Doctor, 0, String::new())),
        // `/accounts` — cũng là một verb không mang id. `acc` để gõ nhanh trên
        // điện thoại, `taikhoan` cho lối gõ không dấu quen thuộc của phòng này.
        "accounts" | "acc" | "taikhoan" => Some((CommandKind::Accounts, 0, String::new())),
        // `/cmd <dòng lệnh>` — phần sau động từ là NGUYÊN VĂN dòng lệnh, nên
        // cắt lại từ chuỗi thô: `id` của bộ phân tích chung sẽ nuốt mất chữ đầu.
        "cmd" | "sh" => {
            let rest = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            Some((CommandKind::Cmd, 0, rest))
        }
        // `/set <key> <value>` — the id slot holds the KEY here, so re-split
        // the raw text instead of using the parsed id.
        "set" | "cauhinh" => {
            let rest = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            // Needs BOTH a key and a value; "/set foo" alone would otherwise
            // arrive with an empty value and blank the field.
            (!rest.is_empty() && rest.contains(char::is_whitespace)).then_some((
                CommandKind::SetConfig,
                0,
                rest,
            ))
        }
        // `/project` takes a NAME, so re-split: the id slot is meaningless.
        "project" | "duan" => {
            let name = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            Some((CommandKind::Project, 0, name))
        }
        // `/session <uuid>` — the id slot only parses integers, and a session
        // id is a uuid, so re-split like `/project`.
        //
        // **`/sessions` (số nhiều) = xem danh sách** (Hà 2026-08-11: *"mở kênh
        // /sessions để xem danh sách phiên"*). Cùng một route, khác đúng cái tên
        // gõ vào: người ta hỏi "có những phiên nào" bằng số nhiều, và bắt họ
        // nhớ rằng "/session không tham số" mới là danh sách là bắt nhớ một luật
        // của mã. Số nhiều thì KHÔNG nhận id — `/sessions <id>` là câu gõ nhầm,
        // và im lặng theo một phiên vì gõ nhầm thì mọi lệnh sau đó đi sai chỗ.
        "sessions" | "phiens" | "danhsach" => Some((CommandKind::Session, 0, String::new())),
        "session" | "phien" => {
            let want = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            Some((CommandKind::Session, 0, want))
        }
        // `/handover [<uuid>]` — id slot is a uuid, so re-split like /session.
        "handover" | "bangiao" => {
            let want = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            Some((CommandKind::Handover, 0, want))
        }
        // `/new <dự án> <việc>` — opens a background session in that project.
        // Both halves are required: without a task there is nothing to run, and
        // without a project there is no folder to run it in.
        "new" | "moi" => {
            let rest = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            (!rest.is_empty() && rest.contains(char::is_whitespace)).then_some((
                CommandKind::New,
                0,
                rest,
            ))
        }
        // `/stop [id]` — empty means the session being read.
        "stop" | "dung" => {
            let want = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            Some((CommandKind::Stop, 0, want))
        }
        // `/tell <nội dung>` — next turn on the focused session.
        "tell" | "noi" => {
            let what = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            (!what.is_empty()).then_some((CommandKind::Tell, 0, what))
        }
        // `/type <chữ>` — gõ thẳng vào cửa sổ của phiên đang theo.
        "type" | "go" => {
            let what = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            (!what.is_empty()).then_some((CommandKind::Type, 0, what))
        }
        // `/upgrade` — hub tự dựng lại chính nó từ mã hiện tại.
        "upgrade" | "capnhat" => Some((CommandKind::Upgrade, 0, String::new())),
        // `/shot` — chụp cửa sổ của phiên đang theo.
        "shot" | "chup" => Some((CommandKind::Shot, 0, String::new())),
        // `/key <tên phím>` — một phím điều khiển.
        "key" | "phim" => {
            let what = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            (!what.is_empty()).then_some((CommandKind::Key, 0, what))
        }
        // `/ask <câu hỏi>` — everything after the verb is the question, so
        // re-split like `/project`. No id: the target is the focused session,
        // because this is typed while looking at that session's stream.
        "ask" | "hoi" => {
            let question = t[1..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim().to_string())
                .unwrap_or_default();
            // An empty `/ask` would otherwise pay for a claude call to answer
            // nothing. Fall through to `None` so it goes through triage like
            // any other text and the person sees it was not understood.
            (!question.is_empty()).then_some((CommandKind::Ask, 0, question))
        }
        _ => None,
    }
}

/// Poll the room for messages newer than the cursor. Used by ingest in F1;
/// the cursor is the newest `ts` we have already turned into a message row.
pub fn poll(
    cfg: &Tfl5Cfg,
    cursors: &BTreeMap<String, String>,
    owner_tids: &[String],
) -> Result<PollResult> {
    let s = login(cfg)?;
    let key = format!("tfl5:{}:{}:last_ts", cfg.app_tid, cfg.room);
    let last_ts: i64 = cursors.get(&key).and_then(|v| v.parse().ok()).unwrap_or(0);

    let rows = history(cfg, &s, cfg.limit, None)?;

    // First run on a room with history: take the tip as the baseline instead of
    // triaging every message ever written (and paying for all of them).
    if !cursors.contains_key(&key) && !cfg.backfill {
        let mut out = PollResult::default();
        if let Some(tip) = rows.iter().filter_map(|m| m["ts"].as_i64()).max() {
            // Baseline the command cursor too, or a first run would execute
            // every `/approve` ever typed in the room's history.
            out.cursors.insert(
                format!("tfl5:{}:{}:last_cmd_ts", cfg.app_tid, cfg.room),
                tip.to_string(),
            );
            out.cursors.insert(key, tip.to_string());
            out.skipped = Some(format!(
                "baseline set at ts={tip}; set backfill=true to ingest older messages"
            ));
        }
        return Ok(out);
    }

    let now_ms = chrono::Utc::now().timestamp_millis();

    // Orders jump the queue. The silence window exists to fold a burst of
    // typing into ONE triage call — it has nothing to protect on a slash
    // command, and making `/close` wait out the window plus a poll interval
    // meant a button press sat for ~2 minutes with no visible reason
    // (measured 2026-08-07). Commands are taken straight from the raw page;
    // everything else still goes through `select_new` untouched.
    // Commands get their OWN cursor. Sharing the message cursor forces a
    // choice between two bugs: advance it past a command and any message still
    // held in the silence window is lost; leave it and the command runs again
    // next cycle — `/reply` would send the same answer twice.
    let cmd_key = format!("tfl5:{}:{}:last_cmd_ts", cfg.app_tid, cfg.room);
    let last_cmd_ts: i64 = cursors
        .get(&cmd_key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(last_ts);

    let mut out = PollResult::default();
    let mut newest_command_ts = last_cmd_ts;
    let mut plain: Vec<Value> = Vec::with_capacity(rows.len());
    for m in &rows {
        let ts = m["ts"].as_i64().unwrap_or(0);
        let uid = m["from_user_tid"].as_str().unwrap_or_default();
        let text = m["text"].as_str().unwrap_or_default();
        match parse_command(text, uid, owner_tids) {
            // New order: execute it.
            Some((kind, id, arg)) if ts > last_cmd_ts => {
                newest_command_ts = newest_command_ts.max(ts);
                out.commands.push(ChannelCommand {
                    quiet: false,
                    kind,
                    decision_id: id,
                    arg,
                    chat_id: target_of(&cfg.app_tid, &cfg.room),
                    callback_id: String::new(),
                    message_id: None,
                });
            }
            // Already executed on an earlier cycle. It must still be dropped
            // here: letting it fall through to `plain` turned yesterday's
            // `/close` into today's inbox item and paid a model to read it.
            Some(_) => {}
            None => plain.push(m.clone()),
        }
    }
    if newest_command_ts > last_cmd_ts {
        out.cursors.insert(cmd_key, newest_command_ts.to_string());
    }

    let picked = select_new(&plain, last_ts, now_ms, &s.user_tid, cfg);
    out.seen = picked.seen;
    if picked.newest_ts > last_ts {
        out.cursors.insert(key, picked.newest_ts.to_string());
    }
    if picked.held > 0 {
        // Recorded on the run row so "0 new" during an active conversation
        // reads as "still settling", not as "the adapter is broken".
        out.skipped = Some(format!(
            "{} tin còn trong cửa sổ im lặng {}s",
            picked.held, cfg.silence_window_sec
        ));
    }
    for f in picked.filtered {
        logging::info("tfl5_message_filtered", f);
    }
    Ok(out)
}

/// What one poll decided to do with the page it fetched.
#[derive(Debug, Default)]
pub struct Selection {
    /// Ordinary chat lines walked past this poll — counted, not kept.
    pub seen: usize,
    /// Where the cursor should land. Never moves past a held-back message.
    pub newest_ts: i64,
    /// Still inside the silence window — deliberately left for the next cycle.
    pub held: usize,
    /// Dropped for good, with the reason. Logged by the caller; never silent.
    pub filtered: Vec<Value>,
}

/// The whole decision layer of the adapter, with no network in it.
///
/// Kept pure on purpose: the silence window is a money guard, and a money guard
/// that can only be exercised by racing a live server is a guard nobody ever
/// actually tests. (This project has already been bitten once by "the guard was
/// fine, the experiment was wrong" — see `PLAN.md` 2026-07-27.)
pub fn select_new(
    rows: &[Value],
    last_ts: i64,
    now_ms: i64,
    self_user_tid: &str,
    cfg: &Tfl5Cfg,
) -> Selection {
    // Chat invites short bursts — "chào", "cho hỏi", "về vụ deploy" is three
    // rows, and each triaged alone is three times the bill for one question.
    // Hold anything still inside the window; next cycle sees the whole burst
    // and `thread_key` coalesces it into a single decision.
    let cutoff = now_ms - (cfg.silence_window_sec as i64) * 1000;

    // history is newest-first; walk oldest-first so the cursor only ever moves
    // across a contiguous prefix and a held-back burst is never jumped over.
    let mut ordered: Vec<&Value> = rows.iter().collect();
    ordered.sort_by_key(|m| m["ts"].as_i64().unwrap_or(0));

    let mut out = Selection {
        newest_ts: last_ts,
        ..Default::default()
    };
    for m in ordered {
        let ts = m["ts"].as_i64().unwrap_or(0);
        if ts <= last_ts {
            continue;
        }
        if ts > cutoff {
            // Leave the cursor behind it so it arrives intact next cycle.
            out.held += 1;
            continue;
        }
        // Our own replies come back on the same feed — ingesting them would
        // triage hub's own words and bill for the privilege.
        if m["from_user_tid"].as_str() == Some(self_user_tid) {
            out.newest_ts = out.newest_ts.max(ts);
            continue;
        }
        let text = m["text"].as_str().unwrap_or("");
        if text.chars().count() < cfg.min_chars {
            // Decided, not deferred: advance past it so "ok" / "👍" never
            // re-enters the queue — but say out loud that it was dropped.
            out.newest_ts = out.newest_ts.max(ts);
            out.filtered.push(json!({
                "reason": "too_short", "min_chars": cfg.min_chars,
                "len": text.chars().count(), "tid": m["tid"],
            }));
            continue;
        }
        out.newest_ts = out.newest_ts.max(ts);

        // An ordinary line: counted, and that is all. Everything that used
        // to be built here — id, sender, thread key, body, reply marker — was
        // for a row in a table that no longer exists.
        out.seen += 1;
    }
    out
}

pub fn health(cfg: &Tfl5Cfg) -> Health {
    if !cfg.enabled {
        return Health {
            ok: false,
            detail: "tắt".into(),
        };
    }
    match login(cfg) {
        Ok(s) => match history(cfg, &s, 1, None) {
            Ok(rows) => Health {
                ok: true,
                detail: format!(
                    "{} @ {} · app {} · room {} · {} tin gần nhất đọc được",
                    s.username,
                    base(cfg),
                    cfg.app_tid,
                    cfg.room,
                    rows.len()
                ),
            },
            Err(e) => Health {
                ok: false,
                detail: format!("đăng nhập được nhưng đọc history lỗi: {e}"),
            },
        },
        Err(e) => Health {
            ok: false,
            detail: logging::err_chain(&e),
        },
    }
}
