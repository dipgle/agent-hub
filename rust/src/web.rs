//! Local web UI — the console for reading the inbox, approving/rejecting, and
//! editing the config without hand-writing JSON.
//!
//! SECURITY (this UI can send replies and rewrite policy, so it is treated as a
//! privileged surface):
//!   * binds **127.0.0.1 only** — never 0.0.0.0, no LAN exposure
//!   * every `/api/*` call must carry `x-hub-token`, a 32-hex token generated
//!     from `/dev/urandom` at startup and embedded in the page it serves. A
//!     random website in your browser cannot set that header cross-origin, so
//!     CSRF/DNS-rebinding cannot drive approvals.
//!   * config writes go through `config::validate` + `config::save` (backup +
//!     temp-file rename), and are round-tripped through the `Config` struct, so
//!     unknown keys and injected secrets cannot survive.
//!   * nothing here can bypass policy: approve/reject reuse the same
//!     `pipeline::approve_decision` / `reject_decision` as the CLI and Telegram.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::{self, Config};
use crate::db::{Db, MessagePatch, NewMessage};
use crate::logging;
use crate::pipeline::{
    approve_decision, ingest, known_projects, reject_decision, run_once, triage_message_by_id,
};

const UI_HTML: &str = include_str!("../assets/ui.html");
const ECHARTS_JS: &str = include_str!("../assets/echarts.min.js");

struct AppState {
    // Arc so each handler can hand a clone to `spawn_blocking`; Mutex because a
    // rusqlite Connection is Send but not Sync, and one process = one writer.
    cfg: Arc<Mutex<Config>>,
    db: Arc<Mutex<Db>>,
    token: String,
    /// Set when `web.password_env` holds a value. Required for a non-loopback
    /// bind; optional (and usually absent) on loopback.
    password: Option<String>,
    /// Hostnames this console answers to — anything else is a rebinding attempt.
    allowed_hosts: Vec<String>,
}

/// 32 hex chars from the OS RNG. No crate needed, and no weak fallback: if the
/// system RNG is unreadable we refuse to start rather than serve a guessable
/// token on a surface that can send mail.
///
/// NOTE: must be a bounded `read_exact` — `/dev/urandom` is an endless stream,
/// so `fs::read()` on it never returns (that bug hung the first build of this
/// server before it could even print its address).
fn random_token() -> Result<String> {
    use std::io::Read;
    let mut buf = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .context("cannot read /dev/urandom for the UI token")?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

type ApiResult = std::result::Result<Json<Value>, ApiError>;

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, logging::err_chain(&e))
    }
}

fn slow_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// Minimal base64 encoder — only needed to build the expected Basic header.
fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// When a password is configured (mandatory for a non-loopback bind), every
/// request — including the page itself — must carry HTTP Basic credentials.
fn basic_ok(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(pw) = &state.password else {
        return true;
    };
    let got = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let expected = format!("Basic {}", base64(format!("hub:{pw}").as_bytes()));
    slow_eq(got, &expected)
}

/// Reject any request whose Host header is not one we serve.
///
/// WHY (found by adversarial review, and the old comment here was wrong): the
/// `x-hub-token` header defeats CSRF, but NOT DNS rebinding. An attacker page
/// on `http://evil.tld:9200/` whose A record flips to 127.0.0.1 becomes
/// same-origin with this console, can read `/` (which embeds the token) and
/// then drive approvals. A Host allowlist is the standard fix — the same one
/// Vite, webpack-dev-server, Jupyter and Home Assistant shipped after their
/// rebinding CVEs.
fn host_ok(state: &AppState, headers: &HeaderMap) -> bool {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let host = host
        .split(':')
        .next()
        .unwrap_or("")
        .trim_matches(['[', ']']);
    state.allowed_hosts.iter().any(|h| h == host)
}

fn deny(state: &AppState, headers: &HeaderMap, what: &str) {
    logging::warn(
        "web_request_denied",
        json!({
            "reason": what,
            "host": headers.get(axum::http::header::HOST).and_then(|v| v.to_str().ok()),
            "path_hint": "see the next access in logs; hub does not log full request paths",
        }),
    );
    let _ = state;
}

fn check_token(state: &AppState, headers: &HeaderMap) -> std::result::Result<(), ApiError> {
    if !host_ok(state, headers) {
        deny(state, headers, "host_not_allowed");
        return Err(ApiError(StatusCode::FORBIDDEN, "host not allowed".into()));
    }
    if !basic_ok(state, headers) {
        // Silent 401s leave brute force and token theft with no trace.
        deny(state, headers, "bad_password");
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "missing or wrong password".into(),
        ));
    }
    let got = headers
        .get("x-hub-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if slow_eq(got, &state.token) {
        Ok(())
    } else {
        deny(state, headers, "bad_token");
        Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "missing or wrong x-hub-token".into(),
        ))
    }
}

/// Headers every response carries: the page holds a live API token, so it must
/// not be framed (clickjacking on the "Duyệt & gửi" button) and must not be
/// written to the browser's on-disk cache.
fn hardening_headers() -> [(axum::http::HeaderName, &'static str); 4] {
    [
        (axum::http::header::CACHE_CONTROL, "no-store, max-age=0"),
        (axum::http::header::X_FRAME_OPTIONS, "DENY"),
        (
            axum::http::header::CONTENT_SECURITY_POLICY,
            "frame-ancestors 'none'",
        ),
        (axum::http::header::REFERRER_POLICY, "no-referrer"),
    ]
}

/// Run blocking work (sqlite, subprocesses) off the async runtime.
async fn blocking<T, F>(f: F) -> std::result::Result<T, ApiError>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            logging::err_chain(&e),
        )),
        Err(e) => Err(ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("task panicked: {e}"),
        )),
    }
}

// ─── handlers ────────────────────────────────────────────────────────────

async fn index(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    // The page carries the API token, so it is gated by Host + password.
    if !host_ok(&state, &headers) {
        deny(&state, &headers, "host_not_allowed");
        return (
            StatusCode::FORBIDDEN,
            hardening_headers(),
            "host not allowed",
        )
            .into_response();
    }
    if !basic_ok(&state, &headers) {
        deny(&state, &headers, "bad_password");
        return (
            StatusCode::UNAUTHORIZED,
            [(axum::http::header::WWW_AUTHENTICATE, "Basic realm=\"hub\"")],
            "cần đăng nhập",
        )
            .into_response();
    }
    (
        hardening_headers(),
        Html(UI_HTML.replace("__HUB_TOKEN__", &state.token)),
    )
        .into_response()
}

async fn echarts(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    // Gated like everything else: a route that skips the check is a route that
    // teaches the next one to skip it too.
    if !host_ok(&state, &headers) || !basic_ok(&state, &headers) {
        deny(&state, &headers, "asset_denied");
        return (StatusCode::FORBIDDEN, "denied").into_response();
    }
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        ECHARTS_JS,
    )
        .into_response()
}

#[derive(Deserialize)]
struct InboxQuery {
    status: Option<String>,
    project: Option<String>,
    limit: Option<i64>,
}

async fn api_inbox(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<InboxQuery>,
) -> ApiResult {
    check_token(&state, &headers)?;
    let db = state.db.clone();
    let out = blocking(move || {
        let db = db.lock().map_err(|_| anyhow!("db mutex poisoned"))?;
        let rows = db.list_messages(q.status.as_deref(), q.project.as_deref(), q.limit.unwrap_or(50))?;
        let mut items = vec![];
        for m in rows {
            let d = db.latest_decision_for(m.id)?;
            items.push(json!({
                "id": m.id,
                "status": m.status,
                "source": m.source,
                "project": m.project,
                "sender": m.sender,
                "sender_trust": m.sender_trust,
                "subject": m.subject,
                "url": m.url,
                "received_at": m.received_at,
                "sender": m.sender,
                "attempts": m.attempts,
                "last_error": m.last_error,
                "coalesced_into": m.coalesced_into,
                "decision": d.as_ref().map(|d| json!({
                    "id": d.id, "kind": d.kind, "severity": d.severity, "status": d.status,
                    "confidence": d.confidence, "needs_human": d.needs_human, "cost_usd": d.cost_usd,
                    "tripwire": d.tripwire,
                    // Why it is sitting here — which of the ten gates stopped it.
                    "reason": d.raw_json().get("outcome").and_then(|o| o.get("reason")).cloned(),
                    "also": db.coalesced_count(d.id).unwrap_or(0),
                    "delivery": db.outbox_state_for(d.id).ok().flatten().map(|(st, at, err)| json!({
                        "status": st, "attempts": at, "last_error": err,
                    })),
                })),
            }));
        }
        Ok(json!({ "items": items, "counts": db.counts()? }))
    })
    .await?;
    Ok(Json(out))
}

async fn api_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    check_token(&state, &headers)?;
    let db = state.db.clone();
    let out = blocking(move || {
        let db = db.lock().map_err(|_| anyhow!("db mutex poisoned"))?;
        let m = db.get_message(id)?.ok_or_else(|| anyhow!("no message #{id}"))?;
        let d = db.latest_decision_for(id)?;
        Ok(json!({
            "message": {
                "id": m.id, "status": m.status, "source": m.source, "project": m.project,
                "sender": m.sender, "sender_trust": m.sender_trust, "subject": m.subject,
                "body": m.body, "url": m.url, "received_at": m.received_at, "ingested_at": m.ingested_at,
                "external_id": m.external_id, "last_error": m.last_error,
            },
            "decision": d.as_ref().map(|d| json!({
                "id": d.id, "status": d.status, "tier": d.tier, "model": d.model, "kind": d.kind,
                "severity": d.severity, "project": d.project, "summary": d.summary,
                "reply_draft": d.reply_draft, "actions": d.actions_json(),
                "evidence": d.evidence.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
                "confidence": d.confidence, "needs_human": d.needs_human, "tripwire": d.tripwire,
                "cost_usd": d.cost_usd, "outcome": d.raw_json().get("outcome").cloned(),
            })),
        }))
    })
    .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
struct ApproveBody {
    body: Option<String>,
}

async fn api_approve(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(payload): Json<ApproveBody>,
) -> ApiResult {
    check_token(&state, &headers)?;
    let (db, cfg) = (state.db.clone(), state.cfg.clone());
    let out = blocking(move || {
        let db = db.lock().map_err(|_| anyhow!("db mutex poisoned"))?;
        let cfg = cfg.lock().map_err(|_| anyhow!("cfg mutex poisoned"))?;
        let r = approve_decision(&db, &cfg, id, payload.body.as_deref())?;
        Ok(serde_json::to_value(r)?)
    })
    .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
struct ReasonBody {
    reason: Option<String>,
}

async fn api_reject(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(payload): Json<ReasonBody>,
) -> ApiResult {
    check_token(&state, &headers)?;
    let db = state.db.clone();
    let out = blocking(move || {
        let db = db.lock().map_err(|_| anyhow!("db mutex poisoned"))?;
        reject_decision(
            &db,
            id,
            payload.reason.as_deref().unwrap_or("rejected in web UI"),
        )?;
        Ok(json!({ "ok": true }))
    })
    .await?;
    Ok(Json(out))
}

async fn api_close(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(payload): Json<ReasonBody>,
) -> ApiResult {
    check_token(&state, &headers)?;
    let db = state.db.clone();
    let out = blocking(move || {
        let db = db.lock().map_err(|_| anyhow!("db mutex poisoned"))?;
        let reason = payload.reason.unwrap_or_else(|| "closed in web UI".into());
        db.set_message_status(
            id,
            "closed",
            MessagePatch {
                last_error: Some(None),
                ..Default::default()
            },
        )?;
        if let Some(d) = db.latest_decision_for(id)? {
            if d.status == "pending" {
                db.cancel_outbox_for(d.id)?;
                db.set_decision_status(d.id, "rejected", Some(&reason))?;
            }
        }
        Ok(json!({ "ok": true }))
    })
    .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
struct SayBody {
    text: String,
    project: Option<String>,
}

async fn api_say(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<SayBody>,
) -> ApiResult {
    check_token(&state, &headers)?;
    if payload.text.trim().is_empty() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "text is empty".into()));
    }
    // Same gate as the CLI: a project that cannot resolve is a typo, and a
    // silent typo means a blind answer.
    if let Some(p) = payload.project.as_deref().filter(|p| !p.is_empty()) {
        let cfg = state.cfg.lock().map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "cfg mutex poisoned".into(),
            )
        })?;
        if !crate::config::is_project_name(p) || crate::config::project_dir(&cfg, p).is_none() {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                format!("không thấy project \"{p}\" — kiểm lại tên thư mục"),
            ));
        }
    }
    let (db, cfg) = (state.db.clone(), state.cfg.clone());
    let out = blocking(move || {
        let db = db.lock().map_err(|_| anyhow!("db mutex poisoned"))?;
        let cfg = cfg.lock().map_err(|_| anyhow!("cfg mutex poisoned"))?;
        let stamp = chrono::Utc::now().timestamp_millis();
        let (id, inserted) = db.insert_message(&NewMessage {
            source: "cli".into(),
            external_id: format!("web:{stamp}"),
            thread_key: Some(format!("web:{stamp}")),
            project: payload.project.filter(|p| !p.is_empty()),
            sender: Some("web:owner".into()),
            sender_trust: Some("trusted".into()),
            subject: Some(payload.text.chars().take(100).collect()),
            body: Some(payload.text.clone()),
            url: None,
            received_at: Some(logging::now_iso()),
            raw: Some(json!({ "stream": "web" })),
        })?;
        if !inserted {
            return Ok(json!({ "ok": false, "error": "duplicate" }));
        }
        let id = id.ok_or_else(|| anyhow!("insert reported new but returned no id"))?;
        let counters = triage_message_by_id(&db, &cfg, id, false)?;
        crate::outbound::flush(&db, &cfg, 20)?;
        Ok(json!({ "ok": true, "message_id": id, "counters": counters }))
    })
    .await?;
    Ok(Json(out))
}

async fn api_config_get(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    check_token(&state, &headers)?;
    let cfg_handle = state.cfg.clone();
    // Off the async worker: this touches a std Mutex that a running cycle can
    // hold for minutes, and it reads the file from disk.
    let out = blocking(move || {
        let mut cfg = cfg_handle
            .lock()
            .map_err(|_| anyhow!("cfg mutex poisoned"))?;
        // Read the file back rather than trusting our in-memory copy: the
        // daemon or a hand edit may have changed it since we started.
        if let Ok(fresh) = config::load(Some(&cfg.config_file.clone())) {
            *cfg = fresh;
        }
        Ok(json!({
            "config": serde_json::to_value(&*cfg)?,
            "config_file": cfg.config_file.display().to_string(),
            "projects": known_projects(&cfg),
        }))
    })
    .await?;
    Ok(Json(out))
}

async fn api_config_put(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> ApiResult {
    check_token(&state, &headers)?;
    // Round-trip through Config: unknown keys are dropped, types are enforced,
    // and validate() runs before anything touches disk.
    let incoming: Config = serde_json::from_value(payload).map_err(|e| {
        ApiError(
            StatusCode::BAD_REQUEST,
            format!("config shape is wrong: {e}"),
        )
    })?;

    let cfg_handle = state.cfg.clone();
    let out = blocking(move || {
        let mut cfg = cfg_handle
            .lock()
            .map_err(|_| anyhow!("cfg mutex poisoned"))?;
        let mut incoming = incoming;
        // Paths and identity are not editable from the UI.
        incoming.config_file = cfg.config_file.clone();
        incoming.hub_home = cfg.hub_home.clone();
        incoming.db = cfg.db.clone();
        incoming.log_file = cfg.log_file.clone();
        incoming.workspace_root = cfg.workspace_root.clone();
        incoming.notify.file = cfg.notify.file.clone();

        config::validate(&incoming)?;
        config::save(&incoming)?;
        logging::info(
            "config_saved_from_web",
            json!({ "file": incoming.config_file.display().to_string() }),
        );
        *cfg = incoming;
        // The daemon loop re-reads the file when its mtime changes, so a switch
        // flipped here reaches the running loop within one poll interval.
        Ok(json!({ "ok": true }))
    })
    .await?;
    Ok(Json(out))
}

async fn api_doctor(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    check_token(&state, &headers)?;
    let (db, cfg) = (state.db.clone(), state.cfg.clone());
    let out = blocking(move || {
        let cfg = cfg
            .lock()
            .map_err(|_| anyhow!("cfg mutex poisoned"))?
            .clone();
        let db = db.lock().map_err(|_| anyhow!("db mutex poisoned"))?;

        let claude = crate::exec::run(
            "claude",
            &["--version"],
            crate::exec::RunOpts {
                timeout: Some(std::time::Duration::from_secs(20)),
                ..Default::default()
            },
        )
        .map(|r| json!({ "ok": r.code == Some(0), "detail": r.stdout.trim() }))
        .unwrap_or_else(|e| json!({ "ok": false, "detail": e.to_string() }));

        // One channel left — the inbox adapters went on 2026-08-08.
        let mut channels = serde_json::Map::new();
        channels.insert(
            "tfl5".into(),
            if cfg.adapters.tfl5.enabled {
                let h = crate::adapters::tfl5::health(&cfg.adapters.tfl5);
                json!({ "enabled": true, "ok": h.ok, "detail": h.detail })
            } else {
                json!({ "enabled": false })
            },
        );

        Ok(json!({
            "claude": claude,
            "channels": channels,
            "counts": db.counts()?,
            "runs": db.last_runs(12)?,
            "projects": known_projects(&cfg),
            "config_file": cfg.config_file.display().to_string(),
        }))
    })
    .await?;
    Ok(Json(out))
}

async fn api_cycle(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    check_token(&state, &headers)?;
    let (db, cfg) = (state.db.clone(), state.cfg.clone());
    let out = blocking(move || {
        let db = db.lock().map_err(|_| anyhow!("db mutex poisoned"))?;
        let cfg = cfg.lock().map_err(|_| anyhow!("cfg mutex poisoned"))?;
        Ok(serde_json::to_value(run_once(&db, &cfg)?)?)
    })
    .await?;
    Ok(Json(out))
}

async fn api_ingest(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    check_token(&state, &headers)?;
    let (db, cfg) = (state.db.clone(), state.cfg.clone());
    let out = blocking(move || {
        let db = db.lock().map_err(|_| anyhow!("db mutex poisoned"))?;
        let cfg = cfg.lock().map_err(|_| anyhow!("cfg mutex poisoned"))?;
        ingest(&db, &cfg)
    })
    .await?;
    Ok(Json(out))
}

async fn api_cost(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult {
    check_token(&state, &headers)?;
    let db = state.db.clone();
    let out = blocking(move || {
        let db = db.lock().map_err(|_| anyhow!("db mutex poisoned"))?;
        let mut stmt = db.conn.prepare(
            "SELECT substr(ts, 1, 10) AS day, COUNT(*) AS n, COALESCE(SUM(cost_usd), 0) AS cost
               FROM decisions GROUP BY day ORDER BY day",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, f64>(2)?,
            ))
        })?;
        let mut days = vec![];
        for row in rows {
            let (day, n, cost) = row?;
            days.push(json!({ "day": day, "decisions": n, "cost_usd": cost }));
        }
        Ok(json!({ "days": days, "counts": db.counts()? }))
    })
    .await?;
    Ok(Json(out))
}

// ─── server ──────────────────────────────────────────────────────────────

pub fn serve(cfg: Config, port: u16) -> Result<()> {
    let db = Db::open(&cfg.db)?;
    let token = random_token()?;
    let password = config::secret_from_env(&cfg.web.password_env);
    let bind = cfg.web.bind.clone();
    let cfg_extra_hosts = cfg.web.allowed_hosts.clone();

    // Refuse to expose the console without a password: the served page contains
    // the API token, so reachability == full control.
    if !config::is_loopback_bind(&bind) && password.is_none() {
        anyhow::bail!(
            "web.bind = {bind} is not loopback and {} is not set.\n\
             Either keep bind = 127.0.0.1 and reach it over an SSH tunnel:\n  \
             ssh -N -L {port}:127.0.0.1:{port} <host>\n\
             or set a password: export {}='...'",
            cfg.web.password_env,
            cfg.web.password_env
        );
    }

    let allowed_hosts = if config::is_loopback_bind(&bind) {
        vec![
            "127.0.0.1".to_string(),
            "localhost".to_string(),
            "::1".to_string(),
        ]
    } else {
        // Off-loopback: answer to the bind address plus whatever hostname the
        // operator fronts it with (Caddy/tailscale), which they set explicitly.
        let mut v = vec![bind.clone()];
        v.extend(cfg_extra_hosts.iter().cloned());
        v
    };

    let state = Arc::new(AppState {
        cfg: Arc::new(Mutex::new(cfg)),
        db: Arc::new(Mutex::new(db)),
        token: token.clone(),
        password,
        allowed_hosts,
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/echarts.js", get(echarts))
        .route("/api/inbox", get(api_inbox))
        .route("/api/message/:id", get(api_message))
        .route("/api/message/:id/close", post(api_close))
        .route("/api/decision/:id/approve", post(api_approve))
        .route("/api/decision/:id/reject", post(api_reject))
        .route("/api/say", post(api_say))
        .route("/api/config", get(api_config_get))
        .route("/api/config", put(api_config_put))
        .route("/api/doctor", get(api_doctor))
        .route("/api/cycle", post(api_cycle))
        .route("/api/ingest", post(api_ingest))
        .route("/api/cost", get(api_cost))
        .with_state(state.clone());
    let has_password = state.password.is_some();

    let ip: std::net::IpAddr = match bind.as_str() {
        "localhost" => std::net::IpAddr::from([127, 0, 0, 1]),
        other => other
            .parse()
            .with_context(|| format!("web.bind = {other} is not an IP address"))?,
    };
    let addr = SocketAddr::new(ip, port);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .context("build tokio runtime")?;

    // Print a bare URL: it gets copy-pasted, and a query string with angle
    // brackets or a trailing token turns into a shell redirect when pasted.
    println!("hub web UI → http://{bind}:{port}/");
    println!(
        "({}; token nhúng sẵn trong trang, đổi mỗi lần khởi động; Ctrl-C để dừng)",
        if has_password {
            "có mật khẩu Basic"
        } else {
            "chỉ nghe loopback, không mật khẩu"
        }
    );
    logging::info(
        "web_ui_started",
        json!({ "bind": bind, "port": port, "password": has_password }),
    );

    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .with_context(|| format!("bind {addr}"))?;
        axum::serve(listener, app)
            .await
            .context("web server stopped")
    })
}
