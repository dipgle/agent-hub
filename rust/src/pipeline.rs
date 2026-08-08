//! One cycle of the hub: ingest → triage → policy → outbox flush.
//!
//! Ordering matters for durability: a poll cursor only advances AFTER the
//! messages from that window are committed, so a crash re-polls instead of
//! losing items.

use std::collections::BTreeMap;

use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};

use crate::adapters::{tfl5, ChannelCommand, CommandKind, PollResult, Skip};
use crate::config::Config;
use crate::db::{Db, Message, MessagePatch, NewDecision, NewOutbox, RunFinish};
use crate::logging;
use crate::outbound::{flush, FlushSummary};
use crate::policy::{
    decide_outcome, effective_tier, human_brief, resolve_project, resolve_trust, Action,
    OutcomeInput,
};
use crate::redaction::{compile_extra, is_external_channel, leak_scan};
use crate::triage::{triage, ThreadMemoryOwned};

const MAX_TRIAGE_ATTEMPTS: i64 = 3;

#[derive(Debug, Default, Serialize)]
pub struct TriageCounters {
    pub triaged: usize,
    pub auto_replied: usize,
    pub awaiting_human: usize,
    pub ignored: usize,
    pub coalesced: usize,
    pub failed: usize,
    pub cost_usd: f64,
    /// Set when the daily ceiling stopped this cycle — the reason travels in
    /// the cycle summary so "nothing happened" is never ambiguous.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_stop: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CycleSummary {
    pub ms: u128,
    pub ingested: Value,
    pub triaged: TriageCounters,
    pub sent: FlushSummary,
}

/// Folder names under `project_roots` — the set `/project <name>` accepts.
///
/// Was `devlog::discover_projects` (folders holding a devlog). With the devlog
/// adapter gone the list comes straight from the filesystem, which is also the
/// more honest answer: a project is a folder, whether or not it keeps a devlog.
pub fn known_projects(cfg: &Config) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for base in crate::config::project_bases(cfg) {
        let Ok(entries) = std::fs::read_dir(&base) else {
            continue;
        };
        for e in entries.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let Some(name) = e.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if name.starts_with('.') || out.contains(&name) {
                continue;
            }
            // A folder is a project when it looks like one. Without this the
            // list swallowed `logs`, `memory`, `scripts` and `crates` — and a
            // name in this list is a name `/project` will accept, so junk here
            // becomes a pin pointing at a folder that holds no work.
            let dir = e.path();
            let is_project = ["CLAUDE.md", ".git", "Cargo.toml", "package.json"]
                .iter()
                .any(|marker| dir.join(marker).exists())
                || dir.join("logs").join("devlog.sqlite").exists();
            if is_project {
                out.push(name);
            }
        }
    }
    out.sort();
    out
}

/// Cursor key holding the project pinned to a thread by `/project <name>`.
/// Cursor holding the Claude session the phone is currently reading.
pub const FOCUS_SESSION_KEY: &str = "focus:session";

/// Cursor holding the most recent handover, so the page can show it.
pub const HANDOVER_KEY: &str = "handover:last";

/// Cursor holding the most recent side question and its answer.
pub const ASIDE_KEY: &str = "aside:last";

pub fn project_pin_key(thread_key: &str) -> String {
    format!("pin:project:{thread_key}")
}

/// How far back a thread's own history may supply the project. Deliberately
/// longer than the coalesce window (which is about "same question") and
/// shorter than forever (which is about "wrong topic from yesterday").
const THREAD_PROJECT_HOURS: i64 = 12;

/// The project this conversation is already about: an explicit pin wins, then
/// the last message on the thread that had one.
///
/// Only a project that actually exists is accepted, so a typo in `/project`
/// cannot route work at a folder that is not there.
fn thread_project(
    db: &Db,
    cfg: &Config,
    thread_key: &Option<String>,
    known: &[String],
) -> Result<Option<String>> {
    let Some(key) = thread_key.as_deref().filter(|k| !k.is_empty()) else {
        return Ok(None);
    };
    let valid = |name: String| -> Option<String> {
        (known.contains(&name) || cfg.projects.contains_key(&name)).then_some(name)
    };

    if let Some(pinned) = db.get_cursor(&project_pin_key(key))? {
        if let Some(ok) = valid(pinned.clone()) {
            return Ok(Some(ok));
        }
        logging::warn(
            "thread_project_pin_unknown",
            json!({ "thread_key": key, "pinned": pinned }),
        );
    }

    let since = (chrono::Utc::now() - chrono::Duration::hours(THREAD_PROJECT_HOURS))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    Ok(db.last_project_for_thread(key, &since)?.and_then(valid))
}

/// Route + rate a message before the row lands, so the inbox is already
/// useful without a triage call.
///
/// **Both** ingest paths must call this. The live socket used to insert its
/// rows raw (`live.rs` → `db.insert_message`), and since live almost always
/// beats the poller to a chat message, EVERY chat row in the store had
/// `project = NULL` and `sender_trust` defaulted — the poller's routing was
/// dead code for that source (found 2026-08-07, and it is why the room never
/// knew which project was being discussed).
pub fn enrich_message(
    db: &Db,
    cfg: &Config,
    m: &mut crate::db::NewMessage,
    projects: &[String],
) -> Result<()> {
    let probe = Message {
        id: 0,
        source: m.source.clone(),
        external_id: m.external_id.clone(),
        thread_key: m.thread_key.clone(),
        project: m.project.clone(),
        sender: m.sender.clone(),
        sender_trust: m.sender_trust.clone().unwrap_or_else(|| "untrusted".into()),
        subject: m.subject.clone(),
        body: m.body.clone(),
        url: m.url.clone(),
        raw: m.raw.as_ref().map(|v| v.to_string()),
        received_at: m.received_at.clone(),
        ingested_at: String::new(),
        status: "new".into(),
        attempts: 0,
        last_error: None,
        claimed_at: None,
        coalesced_into: None,
    };
    if m.project.is_none() {
        m.project = resolve_project(&probe, cfg, projects);
    }
    // Conversation context. `resolve_project` is pure and sees one message; a
    // chat line carries no repo, so anything not literally prefixed "tfl5:"
    // resolves to nothing and hub answers without knowing which codebase is
    // meant. Fall back to what this thread is already about: an explicit pin
    // first, then the last project mentioned.
    // Replying to a line is the cheapest way to say "this is about that" — no
    // command, nothing to remember. It outranks the room's pin because it is a
    // deliberate act aimed at one message.
    if m.project.is_none() {
        if let Some(parent) = m
            .raw
            .as_ref()
            .and_then(|r| r.get("reply_to"))
            .and_then(|v| v.as_str())
        {
            let by_reply = db.project_for_external_id(&m.source, &format!("tfl5:{parent}"))?;
            if by_reply.is_some() {
                m.project = by_reply;
            }
        }
    }
    if m.project.is_none() {
        m.project = thread_project(db, cfg, &m.thread_key, projects)?;
    }
    if m.sender_trust.is_none() {
        m.sender_trust = Some(resolve_trust(&probe, cfg));
    }
    Ok(())
}

pub const ADAPTER_NAMES: [&str; 1] = ["tfl5"];

/// Is this adapter switched on? ONE table, used by both ingest and `doctor`.
/// `doctor` used to keep its own copy and they drifted: adding `tfl5` to the
/// pipeline left doctor reporting it "off" while the loop was polling it
/// happily — a status screen that lies is worse than no status screen.
pub fn adapter_enabled(cfg: &Config, name: &str) -> bool {
    match name {
        "tfl5" => cfg.adapters.tfl5.enabled,
        _ => false,
    }
}

fn poll_adapter(
    cfg: &Config,
    name: &str,
    cursors: &BTreeMap<String, String>,
) -> Result<PollResult> {
    match name {
        "tfl5" => tfl5::poll(&cfg.adapters.tfl5, cursors, &cfg.trust.tfl5_user_tids),
        other => Err(anyhow::anyhow!("unknown adapter {other}")),
    }
}

pub fn ingest(db: &Db, cfg: &Config) -> Result<Value> {
    let mut summary = serde_json::Map::new();
    let projects = known_projects(cfg);

    for name in ADAPTER_NAMES {
        if !adapter_enabled(cfg, name) {
            summary.insert(name.into(), json!({ "skipped": "disabled in config" }));
            logging::info(
                "adapter_skipped",
                json!({ "adapter": name, "reason": "disabled" }),
            );
            continue;
        }

        let run_id = db.start_run(name, "poll")?;
        let cursors = db.all_cursors()?;

        match poll_adapter(cfg, name, &cursors) {
            Ok(res) => {
                let polled = res.messages.len();
                let mut inserted = 0usize;

                for m in res.messages {
                    let mut m = m;
                    enrich_message(db, cfg, &mut m, &projects)?;
                    let (_, is_new) = db.insert_message(&m)?;
                    if is_new {
                        inserted += 1;
                    }
                }

                // Cursors last: an insert failure above must not skip the window.
                for (k, v) in &res.cursors {
                    db.set_cursor(k, v)?;
                }

                // Button presses arrive on the same poll as messages; act on
                // them through the shared approve/reject path.
                let commands = res.commands.len();
                if commands > 0 {
                    execute_commands(db, cfg, name, &res.commands);
                }

                db.finish_run(
                    run_id,
                    RunFinish {
                        ok: true,
                        n_new: inserted as i64,
                        err: None,
                        skipped: res.skipped.clone(),
                    },
                )?;
                summary.insert(
                    name.into(),
                    json!({ "polled": polled, "new": inserted, "partial": res.skipped, "commands": commands }),
                );
                logging::info(
                    "adapter_polled",
                    json!({ "adapter": name, "polled": polled, "new": inserted, "partial": res.skipped }),
                );
            }
            Err(e) => {
                // A missing credential is a deliberate skip, recorded and logged.
                if let Some(skip) = e.downcast_ref::<Skip>() {
                    let reason = skip.to_string();
                    db.finish_run(
                        run_id,
                        RunFinish {
                            ok: true,
                            n_new: 0,
                            err: None,
                            skipped: Some(reason.clone()),
                        },
                    )?;
                    summary.insert(name.into(), json!({ "skipped": reason }));
                    logging::warn(
                        "adapter_skipped",
                        json!({ "adapter": name, "reason": reason }),
                    );
                    continue;
                }
                let msg = logging::err_chain(&e);
                db.finish_run(
                    run_id,
                    RunFinish {
                        ok: false,
                        n_new: 0,
                        err: Some(msg.clone()),
                        skipped: None,
                    },
                )?;
                db.dead_letter(Some(name), None, "ingest", None, &msg)?;
                summary.insert(name.into(), json!({ "error": msg }));
                logging::error(
                    "adapter_poll_failed",
                    json!({ "adapter": name, "err": msg }),
                );
            }
        }
    }

    Ok(Value::Object(summary))
}

/// Execute button presses that arrived on a channel, then acknowledge them on
/// that channel. Never propagates: one bad press must not fail the whole poll,
/// but every outcome is logged.
fn execute_commands(db: &Db, cfg: &Config, adapter: &str, commands: &[ChannelCommand]) {
    for cmd in commands {
        let current = match db.get_decision(cmd.decision_id) {
            Ok(Some(d)) => Some(d),
            Ok(None) => None,
            Err(e) => {
                logging::error(
                    "command_lookup_failed",
                    json!({ "decision_id": cmd.decision_id, "err": e.to_string() }),
                );
                None
            }
        };

        // Commands that answer without touching a decision.
        let ack = match cmd.kind {
            CommandKind::Help => {
                let ack = "Lệnh dùng được trong phòng này:\n\
                     — Phiên Claude —\n\
                     /session <id> — theo một phiên (bỏ theo: /session -)\n\
                     /ask <câu hỏi> — hỏi bên lề phiên đang theo; phiên gốc KHÔNG bị đụng ($)\n\
                     /handover [id] — đóng sổ, lấy bản bàn giao + id để làm tiếp ($)\n\
                     — Hộp việc —\n\
                     /approve <decision-id> — duyệt và gửi\n\
                     /reject <decision-id> [lý do] — bỏ\n\
                     /close <message-id> [lý do] — đóng, huỷ mọi thứ đang chờ gửi\n\
                     /reply <message-id> <nội dung> — tự trả lời tay\n\
                     — Vận hành —\n\
                     /project [tên] — xem / ghim dự án cho phòng (bỏ ghim: /project -)\n\
                     /ingest · /run · /doctor — poll kênh · chạy một vòng · kiểm tra thật\n\
                     /set <khoá> <giá trị> — sửa một trường cấu hình\n\
                     /help — bảng này\n\n\
                     ($) = tốn tiền, tính vào owner_daily_budget_usd.\n\
                     `/act <id>` (sửa code) chỉ chạy được từ terminal — nó ghi code và có thể chạy hàng chục phút."
                    .to_string();
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                continue;
            }
            // Whole-cycle verbs. `Ingest` and `Run` are answered rather than
            // executed here on purpose: this code already runs INSIDE a cycle
            // (`run_once` → `ingest` → `execute_commands`), so calling either
            // one would re-enter the pipeline recursively. The cycle carrying
            // this command does the work a moment later anyway.
            CommandKind::Ingest | CommandKind::Run => {
                let what = if matches!(cmd.kind, CommandKind::Run) {
                    "Vòng đang chạy ngay bây giờ (lệnh này được xử lý bên trong nó) — ingest → triage → gửi."
                } else {
                    "Đang poll mọi kênh trong vòng hiện tại."
                };
                reply_in_channel(db, cfg, adapter, cmd, what);
                continue;
            }
            CommandKind::Doctor => {
                let probe = crate::portal::probe_now(cfg);
                reply_in_channel(db, cfg, adapter, cmd, &probe);
                continue;
            }
            CommandKind::Handover => {
                // Costs a `claude` call, so it obeys the same ceiling as triage
                // and lands in the same books — a spending path the budget
                // cannot see is not a budget (non-negotiable #9). Owner budget,
                // NOT the robot's: see `owner_budget_state`.
                let budget = owner_budget_state(db, cfg);
                let ack = match () {
                    _ if budget.blocks => budget.refusal(),
                    _ => {
                        let want = cmd.arg.trim().to_string();
                        let want = if want.is_empty() {
                            db.get_cursor(FOCUS_SESSION_KEY)
                                .ok()
                                .flatten()
                                .unwrap_or_default()
                        } else {
                            want
                        };
                        let live = crate::sessions::snapshot(cfg);
                        match live.sessions.iter().find(|s| s.session_id == want) {
                            None => format!(
                                "⚠ không thấy phiên '{}' đang chạy",
                                crate::exec::truncate(&want, 40)
                            ),
                            Some(s) => match crate::sessions::handover(cfg, s) {
                                Ok(h) => {
                                    if let Err(e) = db.record_spend(
                                        "handover",
                                        &h.source_id,
                                        h.cost_usd,
                                        &format!("→ {}", h.new_session_id),
                                    ) {
                                        logging::error(
                                            "spend_record_failed",
                                            json!({ "kind": "handover", "err": e.to_string() }),
                                        );
                                    }
                                    let line = serde_json::to_string(&h).unwrap_or_default();
                                    if let Err(e) = db.set_cursor(HANDOVER_KEY, &line) {
                                        logging::error(
                                            "handover_store_failed",
                                            json!({ "err": e.to_string() }),
                                        );
                                    }
                                    logging::info(
                                        "handover_done",
                                        json!({ "from": h.source_id, "to": h.new_session_id, "cost_usd": h.cost_usd }),
                                    );
                                    format!(
                                        "📋 Đã đóng sổ phiên {}. Tiếp tục bằng:\n{}\n\n{}",
                                        h.source_name, h.resume_command, h.checkpoint
                                    )
                                }
                                Err(e) => format!(
                                    "⚠ bàn giao hỏng: {}",
                                    crate::exec::truncate(&e.to_string(), 200)
                                ),
                            },
                        }
                    }
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                ack
            }
            CommandKind::Ask => {
                // Same books, same ceiling as handover — both are the owner
                // spending money by pressing a button on a phone.
                let budget = owner_budget_state(db, cfg);
                let ack = match () {
                    _ if budget.blocks => budget.refusal(),
                    _ => {
                        // No id in the verb: the target is the session being
                        // read. Asking a question with nothing open is a
                        // mistake worth naming, not a silent no-op.
                        let want = db
                            .get_cursor(FOCUS_SESSION_KEY)
                            .ok()
                            .flatten()
                            .unwrap_or_default();
                        let live = crate::sessions::snapshot(cfg);
                        match live.sessions.iter().find(|s| s.session_id == want) {
                            None if want.is_empty() => {
                                "⚠ chưa mở phiên nào. Chạm một phiên trên màn Phiên rồi hỏi lại."
                                    .to_string()
                            }
                            None => format!(
                                "⚠ không thấy phiên '{}' đang chạy nữa",
                                crate::exec::truncate(&want, 40)
                            ),
                            Some(s) => match crate::sessions::ask_aside(cfg, s, &cmd.arg) {
                                Ok(a) => {
                                    if let Err(e) = db.record_spend(
                                        "aside",
                                        &a.source_id,
                                        a.cost_usd,
                                        &format!("→ {}", a.new_session_id),
                                    ) {
                                        logging::error(
                                            "spend_record_failed",
                                            json!({ "kind": "aside", "err": e.to_string() }),
                                        );
                                    }
                                    let line = serde_json::to_string(&a).unwrap_or_default();
                                    if let Err(e) = db.set_cursor(ASIDE_KEY, &line) {
                                        logging::error(
                                            "aside_store_failed",
                                            json!({ "err": e.to_string() }),
                                        );
                                    }
                                    logging::info(
                                        "aside_done",
                                        json!({ "from": a.source_id, "to": a.new_session_id, "cost_usd": a.cost_usd }),
                                    );
                                    // Say what it did NOT do as well as what it
                                    // did: the whole point of the feature is
                                    // that the running session was left alone,
                                    // and the person cannot see that from here.
                                    format!(
                                        "💬 Hỏi bên lề phiên {} (phiên gốc không bị đụng):\n\n{}",
                                        a.source_name, a.answer
                                    )
                                }
                                Err(e) => format!(
                                    "⚠ hỏi bên lề hỏng: {}",
                                    crate::exec::truncate(&e.to_string(), 200)
                                ),
                            },
                        }
                    }
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                ack
            }
            CommandKind::Session => {
                // Which session the phone is reading. Stored as a cursor so it
                // survives a restart, and so the next snapshot — whoever
                // builds it — carries that session's stream.
                let want = cmd.arg.trim();
                let ack = if want.is_empty() {
                    match db.get_cursor(FOCUS_SESSION_KEY) {
                        Ok(Some(id)) if !id.is_empty() => format!("👁 Đang theo phiên {id}"),
                        _ => "Chưa theo phiên nào. Chọn một phiên trên màn Phiên.".to_string(),
                    }
                } else if want == "-" || want.eq_ignore_ascii_case("off") {
                    match db.set_cursor(FOCUS_SESSION_KEY, "") {
                        Ok(()) => "👁 Đã thôi theo phiên.".to_string(),
                        Err(e) => format!("⚠ không bỏ theo được: {e}"),
                    }
                } else {
                    // Only a session this machine actually has: an id from a
                    // stale page must not send the reader to an empty screen
                    // with no explanation.
                    let live = crate::sessions::snapshot(cfg);
                    match live.sessions.iter().find(|s| s.session_id == want) {
                        Some(s) => match db.set_cursor(FOCUS_SESSION_KEY, want) {
                            Ok(()) => format!("👁 Đang theo phiên {} ({})", s.name, s.account),
                            Err(e) => format!("⚠ không theo được: {e}"),
                        },
                        None => format!(
                            "⚠ không thấy phiên '{}' đang chạy ({} phiên đang sống)",
                            crate::exec::truncate(want, 40),
                            live.sessions.len()
                        ),
                    }
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                ack
            }
            CommandKind::Project => {
                // The pin belongs to the conversation, so it is keyed on the
                // same thread the messages use.
                let thread = format!(
                    "tfl5:{}:{}",
                    cfg.adapters.tfl5.app_tid, cfg.adapters.tfl5.room
                );
                let key = project_pin_key(&thread);
                let want = cmd.arg.trim();
                let known = known_projects(cfg);
                let ack = if want.is_empty() {
                    match db.get_cursor(&key) {
                        Ok(Some(p)) => format!("📌 Đang ghim dự án: {p}"),
                        Ok(None) => {
                            let since = (chrono::Utc::now() - chrono::Duration::hours(12))
                                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
                            match db.last_project_for_thread(&thread, &since) {
                                Ok(Some(p)) => format!(
                                    "Chưa ghim. Đang bám theo dự án nhắc gần nhất: {p}. \
                                     Ghim cố định bằng: /project <tên>"
                                ),
                                _ => "Chưa có dự án nào cho phòng này. Đặt bằng: /project <tên>"
                                    .to_string(),
                            }
                        }
                        Err(e) => format!("⚠ không đọc được ghim: {e}"),
                    }
                } else if want == "-" || want.eq_ignore_ascii_case("off") {
                    match db.set_cursor(&key, "") {
                        Ok(()) => "📌 Đã bỏ ghim dự án cho phòng này.".to_string(),
                        Err(e) => format!("⚠ không bỏ ghim được: {e}"),
                    }
                } else if !known.iter().any(|k| k == want) && !cfg.projects.contains_key(want) {
                    // Refuse unknown names: a pin nobody can satisfy would
                    // route every later question at a folder that is not there.
                    format!("⚠ không có dự án '{want}'. Đang biết: {}", known.join(", "))
                } else {
                    match db.set_cursor(&key, want) {
                        Ok(()) => format!(
                            "📌 Từ giờ các câu trong phòng này mặc định thuộc dự án {want} \
                             (bỏ ghim: /project -)"
                        ),
                        Err(e) => format!("⚠ không ghim được: {e}"),
                    }
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                continue;
            }
            CommandKind::SetConfig => {
                let (key, value) = cmd
                    .arg
                    .split_once(char::is_whitespace)
                    .unwrap_or((&cmd.arg, ""));
                let ack = match set_config_field(cfg, key.trim(), value) {
                    Ok(msg) => {
                        format!("{msg}\n(daemon nạp lại theo mtime, có hiệu lực từ vòng kế)")
                    }
                    Err(e) => {
                        logging::error(
                            "command_set_config_failed",
                            json!({ "key": key, "err": logging::err_chain(&e) }),
                        );
                        format!("⚠ không đặt được {key}: {e}")
                    }
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                continue;
            }
            // Message-level verbs: they take a message id, so they must NOT go
            // through the decision lookup below.
            CommandKind::Close => {
                let ack = match close_message(db, cmd.decision_id, &cmd.arg) {
                    Ok(msg) => format!("🗄 {msg}"),
                    Err(e) => {
                        logging::error(
                            "command_close_failed",
                            json!({ "message_id": cmd.decision_id, "err": logging::err_chain(&e) }),
                        );
                        format!("⚠ Đóng #{} lỗi: {e}", cmd.decision_id)
                    }
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                continue;
            }
            CommandKind::Reply => {
                let ack = match reply_to_message(db, cfg, cmd.decision_id, &cmd.arg) {
                    Ok(msg) => format!("✉ {msg}"),
                    Err(e) => {
                        logging::error(
                            "command_reply_failed",
                            json!({ "message_id": cmd.decision_id, "err": logging::err_chain(&e) }),
                        );
                        format!("⚠ Trả lời #{} lỗi: {e}", cmd.decision_id)
                    }
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                continue;
            }
            CommandKind::ActRefused => {
                let ack = format!(
                    "Không chạy act stage từ chat. Nó sửa code và có thể chạy rất lâu — gõ ở terminal:\n\
                     hub approve {id} && hub act {id}",
                    id = cmd.decision_id
                );
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                continue;
            }
            _ => String::new(),
        };
        let _ = ack;

        let ack = match current {
            None => format!("Không tìm thấy decision #{}", cmd.decision_id),
            Some(d) if d.status != "pending" => {
                format!("Decision #{} đã ở trạng thái '{}' rồi", d.id, d.status)
            }
            Some(d) => match cmd.kind {
                // Text after the id REPLACES the draft — the console lets you
                // edit before sending, and dropping `cmd.arg` here silently
                // sent the model's version instead of yours.
                CommandKind::Approve => match approve_decision(
                    db,
                    cfg,
                    d.id,
                    Some(&cmd.arg)
                        .filter(|a| !a.trim().is_empty())
                        .map(|a| a.as_str()),
                ) {
                    Ok(r) if r.queued => format!(
                        "✅ Đã duyệt #{} — đã gửi tới {}",
                        d.id,
                        r.target.unwrap_or_default()
                    ),
                    Ok(_) => format!("✅ Đã duyệt #{} (không có gì để gửi)", d.id),
                    Err(e) => {
                        logging::error(
                            "command_approve_failed",
                            json!({ "decision_id": d.id, "err": logging::err_chain(&e) }),
                        );
                        format!("⚠ Duyệt #{} lỗi: {e}", d.id)
                    }
                },
                CommandKind::Reject => match reject_decision(
                    db,
                    d.id,
                    if cmd.arg.is_empty() {
                        "rejected via channel button"
                    } else {
                        &cmd.arg
                    },
                ) {
                    Ok(()) => format!("🚫 Đã bỏ #{}", d.id),
                    Err(e) => {
                        logging::error(
                            "command_reject_failed",
                            json!({ "decision_id": d.id, "err": logging::err_chain(&e) }),
                        );
                        format!("⚠ Bỏ #{} lỗi: {e}", d.id)
                    }
                },
                // Answered above; never reaches here.
                CommandKind::Help
                | CommandKind::ActRefused
                | CommandKind::Close
                | CommandKind::Reply
                | CommandKind::Ingest
                | CommandKind::Run
                | CommandKind::Doctor
                | CommandKind::SetConfig
                | CommandKind::Project
                | CommandKind::Session
                | CommandKind::Handover
                | CommandKind::Ask => continue,
            },
        };

        logging::info(
            "channel_command_handled",
            json!({ "adapter": adapter, "decision_id": cmd.decision_id, "kind": format!("{:?}", cmd.kind), "ack": ack }),
        );

        // Telegram used to get a second treatment here — answer the callback,
        // then rewrite the original message with the outcome. That channel went
        // with the inbox product on 2026-08-08; the chat room answers through
        // `reply_in_channel` like every other verb.
    }
}

/// Answer a command on the channel it came from. Failing to answer would leave
/// the owner staring at a room that swallowed their command, so a send failure
/// is logged rather than dropped.
fn reply_in_channel(db: &Db, cfg: &Config, adapter: &str, cmd: &ChannelCommand, text: &str) {
    let _ = db;
    if adapter != tfl5::NAME {
        logging::info(
            "channel_command_ack",
            json!({ "adapter": adapter, "ack": text }),
        );
        return;
    }
    if let Err(e) = tfl5::send(&cfg.adapters.tfl5, &cmd.chat_id, None, text) {
        logging::error(
            "tfl5_command_ack_failed",
            json!({ "target": cmd.chat_id, "err": logging::err_chain(&e) }),
        );
    }
}

/// Short recap used when rewriting a Telegram brief after a button press.
/// Where a human brief goes. Telegram used to win when configured; with that
/// channel gone there is one destination left — the local notify file.
fn human_channel(_cfg: &Config) -> (String, String) {
    ("notify".into(), "local".into())
}

/// Triage exactly one message: coalesce → classify → policy → outbox.
pub fn process_message(
    db: &Db,
    cfg: &Config,
    row: &Message,
    projects: &[String],
    out: &mut TriageCounters,
    allow_coalesce: bool,
) -> Result<()> {
    // Same thread, already waiting on a human? Attach, do not pay again.
    let coalesce_window = coalesce_window_for(cfg, &row.source);
    if allow_coalesce && coalesce_window > chrono::Duration::zero() {
        // Anchor the window on WHEN THIS MESSAGE WAS WRITTEN, not on now.
        // Anchoring on now made a backlog collapse into one decision the moment
        // hub caught up, regardless of how far apart the messages really were.
        let anchor = row
            .received_at
            .as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);
        let since = (anchor - coalesce_window).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        if let Some(open) = db.pending_decision_for_thread(row.thread_key.as_deref(), &since)? {
            db.set_message_status(
                row.id,
                "coalesced",
                // Record WHICH decision it joined. Previously this link existed
                // only in the log, so the owner answering a decision had no way
                // to see the other messages folded into it.
                MessagePatch {
                    last_error: Some(None),
                    coalesced_into: Some(open.id),
                    ..Default::default()
                },
            )?;
            out.coalesced += 1;
            logging::info(
                "message_coalesced",
                json!({ "message_id": row.id, "into_decision": open.id, "thread_key": row.thread_key }),
            );
            return Ok(());
        }
    }

    // Ceiling checked per message, not per cycle: a batch of 6 under a $0.50
    // per-call cap could otherwise overshoot a $3 day by 100%. This also covers
    // `triage_message_by_id` (hub say / the web console), which does not go
    // through triage_new at all.
    if let Some((spent, cap)) = budget_state(db, cfg)? {
        if spent >= cap {
            let reason = format!("daily budget reached: ${spent:.4} / ${cap:.2}");
            db.set_message_status(
                row.id,
                "new",
                MessagePatch {
                    last_error: Some(Some(reason.clone())),
                    ..Default::default()
                },
            )?;
            out.budget_stop = Some(reason);
            return Ok(());
        }
    }

    // Per-source ceiling, checked in the same place for the same reason: one
    // noisy channel must not drain the day before the others are looked at.
    if let Some((spent, cap)) = source_budget_state(db, cfg, &row.source)? {
        if spent >= cap {
            let reason = format!(
                "daily budget for source '{}' reached: ${spent:.4} / ${cap:.2}",
                row.source
            );
            db.set_message_status(
                row.id,
                "new",
                MessagePatch {
                    last_error: Some(Some(reason.clone())),
                    ..Default::default()
                },
            )?;
            logging::warn(
                "source_budget_reached",
                json!({ "source": row.source, "spent_usd": spent, "cap_usd": cap, "message_id": row.id }),
            );
            out.budget_stop = Some(reason);
            return Ok(());
        }
    }

    let project = row
        .project
        .clone()
        .or_else(|| resolve_project(row, cfg, projects));
    let trust = if row.sender_trust.is_empty() {
        resolve_trust(row, cfg)
    } else {
        row.sender_trust.clone()
    };
    db.set_message_status(
        row.id,
        "triaging",
        MessagePatch {
            project: project.clone(),
            sender_trust: Some(trust.clone()),
            ..Default::default()
        },
    )?;

    let mut msg = row.clone();
    msg.project = project.clone();
    msg.sender_trust = trust.clone();

    let memory = thread_memory_for(db, cfg, row)?;
    let t = triage(&msg, cfg, memory.as_ref())?;
    out.cost_usd += t.cost_usd;

    let decision = match (&t.decision, &t.error) {
        (Some(d), None) => d.clone(),
        _ => {
            let err = t
                .error
                .unwrap_or_else(|| "triage produced no decision".into());

            // A failed call can still have cost money (schema mismatch, budget
            // abort mid-flight). Recording it keeps `cost_on_day` — and with it
            // the daily ceiling — from reading $0.00 forever while the bill grows.
            if t.cost_usd > 0.0 {
                db.insert_decision(&NewDecision {
                    message_id: row.id,
                    tier: effective_tier(project.as_deref(), &trust, cfg),
                    model: Some(t.model.clone()),
                    kind: Some("triage_failed".into()),
                    project: project.clone(),
                    summary: Some(crate::exec::truncate(&err, 500)),
                    needs_human: true,
                    cost_usd: Some(t.cost_usd),
                    session_id: t.session_id.clone(),
                    status: "failed".into(),
                    ..Default::default()
                })?;
            }

            let attempts = row.attempts + 1;
            if attempts >= MAX_TRIAGE_ATTEMPTS {
                db.set_message_status(
                    row.id,
                    "failed",
                    MessagePatch {
                        last_error: Some(Some(err.clone())),
                        ..Default::default()
                    },
                )?;
                db.dead_letter(
                    Some(&row.source),
                    Some(&row.external_id),
                    "triage",
                    Some(&json!({ "subject": row.subject })),
                    &err,
                )?;
                let (channel, target) = human_channel(cfg);
                db.enqueue_outbox(&NewOutbox {
                    message_id: Some(row.id),
                    channel,
                    target,
                    subject: Some(format!("hub: triage failed {attempts}× ({})", row.source)),
                    body: format!(
                        "message #{} {}\n\nlast error: {err}",
                        row.id,
                        row.subject.clone().unwrap_or_default()
                    ),
                    ..Default::default()
                })?;
            } else {
                db.set_message_status(
                    row.id,
                    "new",
                    MessagePatch {
                        last_error: Some(Some(err)),
                        ..Default::default()
                    },
                )?;
            }
            out.failed += 1;
            return Ok(());
        }
    };

    let tier = effective_tier(project.as_deref(), &trust, cfg);
    let mut outcome = decide_outcome(OutcomeInput {
        msg: &msg,
        decision: &decision,
        tier: &tier,
        trust: &trust,
        tripwire: &t.tripwire,
        cfg,
    });

    // Last gate before anything leaves the machine: internal detail in an
    // outbound reply downgrades the item to human review.
    if outcome.action == Action::AutoReply
        && outcome
            .channel
            .as_deref()
            .map(is_external_channel)
            .unwrap_or(false)
    {
        let extra = compile_extra(&cfg.leak_patterns);
        let leaks = leak_scan(&decision.reply_draft, &extra);
        if !leaks.is_empty() {
            logging::warn(
                "outbound_leak_scan_blocked",
                json!({ "message_id": row.id, "channel": outcome.channel, "leaks": leaks }),
            );
            outcome.action = Action::AwaitHuman;
            outcome.reason = format!("outbound leak scan: {}", leaks.join(", "));
        }
    }

    // One commit point for the whole outcome: decision row, outbox rows and the
    // message's new status land together or not at all.
    db.begin()?;
    let committed = commit_outcome(db, cfg, row, &msg, &decision, &outcome, &tier, &t);
    let decision_id = match committed {
        Ok(id) => {
            db.commit()?;
            id
        }
        Err(e) => {
            if let Err(re) = db.rollback() {
                logging::error(
                    "rollback_failed",
                    json!({ "message_id": row.id, "err": re.to_string() }),
                );
            }
            return Err(e);
        }
    };
    out.triaged += 1;
    match outcome.action {
        Action::AutoReply => out.auto_replied += 1,
        Action::Ignore => {
            out.ignored += 1;
            logging::info(
                "message_ignored",
                json!({ "message_id": row.id, "kind": decision.kind }),
            );
        }
        Action::AwaitHuman => out.awaiting_human += 1,
    }

    logging::info(
        "message_triaged",
        json!({
            "message_id": row.id, "decision_id": decision_id, "source": row.source, "project": project,
            "kind": decision.kind, "severity": decision.severity, "confidence": decision.confidence,
            "tier": tier, "action": outcome.action.as_str(), "reason": outcome.reason, "cost_usd": t.cost_usd,
            "tripwire": if t.tripwire.is_empty() { Value::Null } else { json!(t.tripwire) },
        }),
    );
    Ok(())
}

/// The write half of `process_message`, run inside one transaction.
#[allow(clippy::too_many_arguments)]
fn commit_outcome(
    db: &Db,
    cfg: &Config,
    row: &Message,
    msg: &Message,
    decision: &crate::triage::Decision,
    outcome: &crate::policy::Outcome,
    tier: &str,
    t: &crate::triage::TriageResult,
) -> Result<i64> {
    let project = msg.project.clone();
    let decision_project = if decision.project == "unknown" || decision.project.is_empty() {
        project.clone()
    } else {
        Some(decision.project.clone())
    };

    let decision_id = db.insert_decision(&NewDecision {
        message_id: row.id,
        tier: tier.to_string(),
        model: Some(t.model.clone()),
        kind: Some(decision.kind.clone()),
        severity: Some(decision.severity.clone()),
        project: decision_project,
        summary: Some(decision.summary.clone()),
        reply_draft: Some(decision.reply_draft.clone()),
        actions: Some(serde_json::to_value(&decision.proposed_actions)?),
        evidence: Some(serde_json::to_value(&decision.evidence)?),
        confidence: Some(decision.confidence),
        needs_human: outcome.action == Action::AwaitHuman,
        tripwire: t.tripwire.clone(),
        cost_usd: Some(t.cost_usd),
        session_id: t.session_id.clone(),
        raw: Some(json!({ "claude": t.raw, "outcome": outcome })),
        status: match outcome.action {
            Action::AutoReply | Action::Ignore => "auto".into(),
            Action::AwaitHuman => "pending".to_string(),
        },
    })?;

    let brief = human_brief(msg, decision, outcome, tier, decision_id);
    let (human_ch, human_target) = human_channel(cfg);

    match outcome.action {
        Action::AutoReply => {
            db.enqueue_outbox(&NewOutbox {
                decision_id: Some(decision_id),
                message_id: Some(row.id),
                channel: outcome
                    .channel
                    .clone()
                    .unwrap_or_else(|| row.source.clone()),
                target: outcome.target.clone().unwrap_or_default(),
                subject: if row.source == "email" {
                    Some(
                        format!("Re: {}", row.subject.clone().unwrap_or_default())
                            .chars()
                            .take(200)
                            .collect(),
                    )
                } else {
                    None
                },
                body: decision.reply_draft.clone(),
            })?;
            // Always tell the human what went out under their name.
            db.enqueue_outbox(&NewOutbox {
                decision_id: Some(decision_id),
                message_id: Some(row.id),
                channel: human_ch,
                target: human_target,
                subject: Some(format!("hub auto-replied ({})", row.source)),
                body: brief,
            })?;
            db.set_message_status(row.id, "answered", MessagePatch::default())?;
        }
        Action::Ignore => {
            db.set_message_status(row.id, "closed", MessagePatch::default())?;
        }
        Action::AwaitHuman => {
            db.enqueue_outbox(&NewOutbox {
                decision_id: Some(decision_id),
                message_id: Some(row.id),
                channel: human_ch,
                target: human_target,
                subject: Some(format!(
                    "hub cần bạn xem ({}/{})",
                    decision.kind, decision.severity
                )),
                body: brief,
            })?;
            db.set_message_status(row.id, "awaiting_human", MessagePatch::default())?;
        }
    }

    Ok(decision_id)
}

/// Should this message continue an earlier conversation, and which one?
///
/// Opt-in per source. A GitHub notification is a standalone event; a line in a
/// chat room is usually a follow-up, and answering "và cái kia thì sao?" with
/// no memory of the previous turn is useless.
///
pub fn thread_memory_for(db: &Db, cfg: &Config, row: &Message) -> Result<ThreadMemoryOwned> {
    let hours = match cfg.source_thread_memory_hours.get(&row.source) {
        Some(h) if *h > 0.0 => *h,
        _ => return Ok(ThreadMemoryOwned::Off),
    };
    let key = match row.thread_key.as_deref() {
        Some(k) if !k.is_empty() => k,
        // Memory is on for the source, but this row has no thread to remember —
        // keep the session anyway rather than silently downgrading to Off.
        _ => return Ok(ThreadMemoryOwned::Start),
    };
    let anchor = row
        .received_at
        .as_deref()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);
    let since = (anchor - chrono::Duration::milliseconds((hours * 3_600_000.0).round() as i64))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    Ok(match db.last_session_for_thread(key, &since)? {
        Some(sid) => ThreadMemoryOwned::Resume(sid),
        None => ThreadMemoryOwned::Start,
    })
}

/// How far back a source may attach a new message to an open decision.
///
/// `thread_key` means different things per source. A GitHub issue is a topic,
/// so 12 hours of activity really is one conversation. A chat ROOM is not a
/// topic — two unrelated questions an hour apart are two questions, and folding
/// the second into the first means the second is never answered. So chat gets a
/// short window (minutes), configured per source.
pub fn coalesce_window_for(cfg: &Config, source: &str) -> chrono::Duration {
    match cfg.source_coalesce_hours.get(source) {
        Some(h) if *h > 0.0 => chrono::Duration::milliseconds((*h * 3_600_000.0).round() as i64),
        // An explicit 0 disables coalescing for that source entirely.
        Some(_) => chrono::Duration::zero(),
        None => chrono::Duration::hours(cfg.coalesce_hours),
    }
}

/// Today's spend for one source against its own ceiling, when it has one.
pub fn source_budget_state(db: &Db, cfg: &Config, source: &str) -> Result<Option<(f64, f64)>> {
    let cap = match cfg.source_daily_budget_usd.get(source) {
        Some(c) if *c > 0.0 => *c,
        _ => return Ok(None),
    };
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    Ok(Some((db.cost_on_day_for_source(&today, source)?, cap)))
}

/// Today's OWNER spend — the person pressing buttons — against its own ceiling.
///
/// Separate from `budget_state` on purpose: `daily_budget_usd` exists to rein in
/// a robot nobody is watching, and refusing the owner's own press because the
/// robot had a busy morning is the wrong answer to the wrong question.
#[derive(Debug, Clone, Copy)]
pub struct OwnerBudget {
    pub spent_usd: f64,
    pub cap_usd: f64,
    /// The worst case ONE press can add.
    pub per_call_usd: f64,
    /// Whether the next press is refused. This field is the product's actual
    /// decision, published so a test can assert the outcome instead of
    /// re-deriving it — a check that recomputes the rule is a check that can
    /// agree with a broken product (`fe-stream-uc` did exactly that, and only
    /// passed because two different ceilings happened to reach the same answer).
    pub blocks: bool,
}

/// Refuse on the WORST CASE, not after the fact. `spent >= cap` lets one
/// unbounded call through, and the first real handover used that hole to spend
/// $1.72 on a $3 day (2026-08-08).
pub fn owner_budget_state(db: &Db, cfg: &Config) -> OwnerBudget {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let spent = db.owner_cost_on_day(&today).unwrap_or_else(|e| {
        // Reading the books failed, so hub cannot know what it has spent.
        // Report zero and say so — but never let this decide "plenty left".
        logging::error("owner_spend_read_failed", json!({ "err": e.to_string() }));
        f64::NAN
    });
    let cap = cfg.owner_daily_budget_usd;
    let per_call = cfg.triage.max_budget_usd;
    let worst_case = spent + per_call;
    OwnerBudget {
        spent_usd: if spent.is_nan() { 0.0 } else { spent },
        cap_usd: cap,
        per_call_usd: per_call,
        // Unreadable books (NaN) refuse: not knowing the balance is not the
        // same as having room. Said out loud rather than left to fall out of a
        // negated comparison, because that is the case a reader skips.
        blocks: cap > 0.0 && (worst_case.is_nan() || worst_case > cap),
    }
}

impl OwnerBudget {
    /// The sentence shown when a press is refused.
    fn refusal(&self) -> String {
        format!(
            "⚠ hết ngân sách cho thao tác của bạn hôm nay: đã dùng ${:.2}/${:.2}, một lần gọi có thể tốn tới ${:.2}. Nâng owner_daily_budget_usd nếu cần.",
            self.spent_usd, self.cap_usd, self.per_call_usd
        )
    }
}

/// Today's spend against the ceiling. `None` when no ceiling is configured.
pub fn budget_state(db: &Db, cfg: &Config) -> Result<Option<(f64, f64)>> {
    if cfg.daily_budget_usd <= 0.0 {
        return Ok(None);
    }
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    Ok(Some((db.cost_on_day(&today)?, cfg.daily_budget_usd)))
}

pub fn triage_new(db: &Db, cfg: &Config) -> Result<TriageCounters> {
    // Only reclaim rows no live cycle can still own: a triage cannot outlive
    // its own timeout by much, so anything older than 2× that was stranded.
    let stale_after = (cfg.triage.timeout_sec as i64) * 2 + 60;
    let recovered = db.reset_triaging(stale_after)?;
    if recovered > 0 {
        logging::warn(
            "recovered_stuck_triaging",
            json!({ "rows": recovered, "older_than_secs": stale_after }),
        );
    }

    let mut out = TriageCounters::default();

    // Daily ceiling: an always-on daemon spends money while nobody is looking.
    // Stopping is loud (log + one notify), never a silent no-op.
    if let Some((spent, cap)) = budget_state(db, cfg)? {
        if spent >= cap {
            out.budget_stop = Some(format!("daily budget reached: ${spent:.4} / ${cap:.2}"));
            logging::warn(
                "daily_budget_reached",
                json!({ "spent_usd": spent, "cap_usd": cap }),
            );
            let (channel, target) = human_channel(cfg);
            // One heads-up per day, not once per cycle.
            let already = db.conn.query_row(
                "SELECT COUNT(*) FROM outbox WHERE channel = ?1 AND subject = ?2 AND substr(created_at, 1, 10) = ?3",
                rusqlite::params![channel, "hub: chạm trần chi phí ngày", chrono::Utc::now().format("%Y-%m-%d").to_string()],
                |r| r.get::<_, i64>(0),
            )?;
            if already == 0 {
                db.enqueue_outbox(&NewOutbox {
                    channel,
                    target,
                    subject: Some("hub: chạm trần chi phí ngày".into()),
                    body: format!(
                        "Đã tiêu ${spent:.4} hôm nay, trần là ${cap:.2} → tạm dừng triage.\n\
                         Hàng đợi vẫn nhận item, sẽ xử lại sau nửa đêm UTC.\n\
                         Muốn tiếp tục ngay: tăng daily_budget_usd trong Cấu hình."
                    ),
                    ..Default::default()
                })?;
            }
            return Ok(out);
        }
    }

    let batch = db.claim_new_messages(cfg.max_triage_per_cycle)?;
    let projects = known_projects(cfg);
    for row in &batch {
        process_message(db, cfg, row, &projects, &mut out, true)?;
    }
    Ok(out)
}

/// Triage one specific message now — what `hub say` needs.
pub fn triage_message_by_id(
    db: &Db,
    cfg: &Config,
    message_id: i64,
    allow_coalesce: bool,
) -> Result<TriageCounters> {
    let row = db
        .get_message(message_id)?
        .ok_or_else(|| anyhow::anyhow!("no message #{message_id}"))?;
    let projects = known_projects(cfg);
    let mut out = TriageCounters::default();
    process_message(db, cfg, &row, &projects, &mut out, allow_coalesce)?;
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct ApproveResult {
    pub decision_id: i64,
    pub queued: bool,
    pub channel: Option<String>,
    pub target: Option<String>,
    pub sent: FlushSummary,
    pub code_change_proposed: bool,
    /// Leak-scan hits on an approved outbound reply. Not a block — a human
    /// approved it — but every surface should be able to say so out loud.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub leaks: Vec<String>,
}

/// Approve a decision: queue its reply (if there is one and a target exists),
/// mark the books, flush. Shared by the CLI, the Telegram buttons and the web
/// UI so all three can never drift apart.
pub fn approve_decision(
    db: &Db,
    cfg: &Config,
    decision_id: i64,
    body_override: Option<&str>,
) -> Result<ApproveResult> {
    let d = db
        .get_decision(decision_id)?
        .ok_or_else(|| anyhow::anyhow!("no decision #{decision_id}"))?;
    let m = db.get_message(d.message_id)?.ok_or_else(|| {
        anyhow::anyhow!("decision #{decision_id} has no message (db inconsistent)")
    })?;

    let outcome = d.raw_json().get("outcome").cloned().unwrap_or(Value::Null);
    let target = outcome
        .get("target")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let channel = outcome
        .get("channel")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| m.source.clone());
    let body = body_override
        .map(|s| s.to_string())
        .or_else(|| d.reply_draft.clone())
        .unwrap_or_default();

    // The triage path leak-scans an auto-reply before it leaves (rule #4). The
    // approve path never did, on the theory that a human read it. That was
    // defensible while every channel was a repo or a mailbox the owner already
    // knew; it is thinner now that a chat room can hold people the owner has
    // never met. So scan here too — but WARN rather than block: a human made
    // this call, and silently refusing to send their approved words would be
    // its own failure. Loud, recorded, still delivered.
    let mut leaks: Vec<String> = vec![];
    if is_external_channel(&channel) && !body.trim().is_empty() {
        leaks = leak_scan(&body, &compile_extra(&cfg.leak_patterns));
        if !leaks.is_empty() {
            logging::warn(
                "approved_reply_has_internal_detail",
                json!({ "decision_id": d.id, "channel": channel, "target": target, "leaks": leaks }),
            );
            if let Err(e) = crate::outbound::notify(
                cfg,
                Some(&format!("hub: bản duyệt #{} có chi tiết nội bộ", d.id)),
                &format!(
                    "Đã gửi ra {channel} theo lệnh duyệt của bạn, nhưng nội dung khớp: {}.\n\nNội dung:\n{}",
                    leaks.join(", "),
                    crate::exec::truncate(&body, 800)
                ),
            ) {
                logging::error("leak_warning_notify_failed", json!({ "decision_id": d.id, "err": e.to_string() }));
            }
        }
    }

    let mut queued = false;
    if !body.trim().is_empty() {
        if let Some(t) = &target {
            db.enqueue_outbox(&NewOutbox {
                decision_id: Some(d.id),
                message_id: Some(m.id),
                channel: channel.clone(),
                target: t.clone(),
                subject: if m.source == "email" {
                    Some(
                        format!("Re: {}", m.subject.clone().unwrap_or_default())
                            .chars()
                            .take(200)
                            .collect(),
                    )
                } else {
                    None
                },
                body: body.trim().to_string(),
            })?;
            queued = true;
        }
    }

    db.set_decision_status(
        d.id,
        "approved",
        Some(if queued {
            "approved, reply queued"
        } else {
            "approved"
        }),
    )?;
    db.set_message_status(
        m.id,
        if queued { "answered" } else { "closed" },
        MessagePatch::default(),
    )?;
    let sent = flush(db, cfg, 20)?;

    let code_change_proposed = matches!(d.actions_json(), Value::Array(ref a) if a.iter().any(|x| x.get("type").and_then(|v| v.as_str()) == Some("code_change")));

    logging::info(
        "decision_approved",
        json!({ "decision_id": d.id, "queued": queued, "channel": channel, "target": target, "sent": sent.sent }),
    );
    Ok(ApproveResult {
        decision_id: d.id,
        queued,
        channel: Some(channel),
        target,
        sent,
        code_change_proposed,
        leaks,
    })
}

/// Reject a decision: cancel anything queued for it and close the message.
pub fn reject_decision(db: &Db, decision_id: i64, reason: &str) -> Result<()> {
    let d = db
        .get_decision(decision_id)?
        .ok_or_else(|| anyhow::anyhow!("no decision #{decision_id}"))?;
    let reason = if reason.trim().is_empty() {
        "rejected by owner"
    } else {
        reason
    };
    db.cancel_outbox_for(d.id)?;
    db.set_decision_status(d.id, "rejected", Some(reason))?;
    db.set_message_status(d.message_id, "closed", MessagePatch::default())?;
    logging::info(
        "decision_rejected",
        json!({ "decision_id": d.id, "reason": reason }),
    );
    Ok(())
}

/// Close a message by hand: mark it closed and cancel any pending decision
/// (and its queued send) so nothing goes out afterwards.
///
/// Lives here, not in the CLI, because three surfaces now call it — `hub
/// close`, the console, and the `/close` command from the chat room — and a
/// second copy is how two of them start behaving differently.
pub fn close_message(db: &Db, message_id: i64, reason: &str) -> Result<String> {
    let m = db
        .get_message(message_id)?
        .ok_or_else(|| anyhow::anyhow!("no message #{message_id}"))?;
    let reason = if reason.trim().is_empty() {
        "closed by owner"
    } else {
        reason
    };
    db.set_message_status(
        m.id,
        "closed",
        MessagePatch {
            last_error: Some(None),
            ..Default::default()
        },
    )?;
    if let Some(d) = db.latest_decision_for(m.id)? {
        if d.status == "pending" {
            db.cancel_outbox_for(d.id)?;
            db.set_decision_status(d.id, "rejected", Some(reason))?;
        }
    }
    logging::info(
        "message_closed",
        json!({ "message_id": m.id, "reason": reason }),
    );
    Ok(format!("closed message #{} ({reason})", m.id))
}

/// Answer a message by hand: queue the text on the channel it arrived from and
/// flush immediately. Same call for CLI, console and the `/reply` command.
pub fn reply_to_message(db: &Db, cfg: &Config, message_id: i64, text: &str) -> Result<String> {
    if text.trim().is_empty() {
        anyhow::bail!("cần nội dung trả lời");
    }
    let m = db
        .get_message(message_id)?
        .ok_or_else(|| anyhow::anyhow!("no message #{message_id}"))?;
    let raw = m.raw_json();
    let target = match m.source.as_str() {
        "github" => crate::policy::github_reply_target(&m, &raw),
        "email" => crate::policy::email_address(m.sender.as_deref()),
        "telegram" => raw.get("chat_id").map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }),
        "tfl5" => Some(crate::adapters::tfl5::target_of(
            raw.get("app_tid")
                .and_then(Value::as_str)
                .unwrap_or(&cfg.adapters.tfl5.app_tid),
            raw.get("room")
                .and_then(Value::as_str)
                .unwrap_or(&cfg.adapters.tfl5.room),
        )),
        "cli" => Some("local".into()),
        _ => None,
    }
    .ok_or_else(|| anyhow::anyhow!("cannot reply to source={} (no target)", m.source))?;

    let channel = if m.source == "cli" {
        "notify".to_string()
    } else {
        m.source.clone()
    };
    db.enqueue_outbox(&crate::db::NewOutbox {
        message_id: Some(m.id),
        channel: channel.clone(),
        target: target.clone(),
        subject: if m.source == "email" {
            Some(
                format!("Re: {}", m.subject.clone().unwrap_or_default())
                    .chars()
                    .take(200)
                    .collect(),
            )
        } else {
            None
        },
        body: text.to_string(),
        ..Default::default()
    })?;
    db.set_message_status(m.id, "answered", MessagePatch::default())?;
    let sent = crate::outbound::flush(db, cfg, 20)?;
    Ok(format!(
        "reply queued to {channel}:{target}; sent={} failed={}",
        sent.sent, sent.failed
    ))
}

/// Set ONE config field by dotted path, then round-trip through `Config` so a
/// typo is a rejection, not a corrupted file.
///
/// Deliberately field-at-a-time rather than "paste a JSON blob": the value
/// travels through a chat room, and one key + one value is auditable at a
/// glance. The type of the EXISTING value decides how the text is parsed, so
/// `/set adapters.github.enabled false` cannot turn a bool into the string
/// "false" and silently disable the check that reads it.
pub fn set_config_field(cfg: &Config, dotted: &str, raw: &str) -> Result<String> {
    let path = cfg.config_file.clone();
    let text = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    let mut root: Value = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("config file is not valid JSON: {e}"))?;

    let parts: Vec<&str> = dotted.split('.').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        anyhow::bail!("cần đường dẫn, ví dụ: autonomy.default");
    }
    let mut node = &mut root;
    for key in &parts[..parts.len() - 1] {
        node = node
            .get_mut(*key)
            .ok_or_else(|| anyhow::anyhow!("không có mục '{key}' trong cấu hình"))?;
    }
    let leaf = parts[parts.len() - 1];
    let current = node
        .get(leaf)
        .ok_or_else(|| anyhow::anyhow!("không có trường '{dotted}' trong cấu hình"))?
        .clone();

    let next = match &current {
        Value::Bool(_) => match raw.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "on" | "bat" | "bật" => Value::Bool(true),
            "false" | "0" | "off" | "tat" | "tắt" => Value::Bool(false),
            other => anyhow::bail!("'{other}' không phải true/false"),
        },
        Value::Number(_) => {
            let n: f64 = raw
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("'{raw}' không phải số"))?;
            if n.fract() == 0.0 && current.is_i64() {
                Value::from(n as i64)
            } else {
                Value::from(n)
            }
        }
        // Comma-separated in, array out — matches how the console's text
        // inputs for repos / chat ids / trust lists behave.
        Value::Array(_) => Value::Array(
            raw.split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| Value::String(s.to_string()))
                .collect(),
        ),
        Value::String(_) => Value::String(raw.trim().to_string()),
        other => anyhow::bail!("chưa hỗ trợ sửa kiểu {other:?} qua lệnh; dùng console"),
    };

    node[leaf] = next.clone();

    // The real gate: unknown keys are dropped, types enforced, and
    // `config::save` validates + backs up + temp-renames.
    let incoming: Config = serde_json::from_value(root)
        .map_err(|e| anyhow::anyhow!("cấu hình sau khi sửa không hợp lệ: {e}"))?;
    let mut incoming = incoming;
    // Paths are runtime-derived, never taken from the edited copy.
    incoming.config_file = cfg.config_file.clone();
    incoming.hub_home = cfg.hub_home.clone();
    incoming.db = cfg.db.clone();
    incoming.log_file = cfg.log_file.clone();
    incoming.notify.file = cfg.notify.file.clone();
    crate::config::save(&incoming)?;

    logging::info(
        "config_field_set",
        json!({ "key": dotted, "value": next, "via": "chat" }),
    );
    Ok(format!("⚙ đã đặt {dotted} = {next}"))
}

pub fn run_once(db: &Db, cfg: &Config) -> Result<CycleSummary> {
    let started = std::time::Instant::now();
    let ingested = ingest(db, cfg)?;
    let triaged = triage_new(db, cfg)?;
    let sent = flush(db, cfg, 20)?;
    let summary = CycleSummary {
        ms: started.elapsed().as_millis(),
        ingested,
        triaged,
        sent,
    };
    logging::info("cycle_done", serde_json::to_value(&summary)?);
    Ok(summary)
}
