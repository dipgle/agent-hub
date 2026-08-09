//! One cycle of the hub: poll the room for orders, run them, push a snapshot.
//!
//! There is no triage, no queue and no outbox. Until 2026-08-08 this file WAS
//! `ingest → triage → policy → outbox flush`: every line typed anywhere went
//! through a `claude -p` call that sorted it into an inbox. That product is
//! gone, and with it the only thing on this machine that spent money while
//! nobody was watching. What runs here now is free: parse an order, do it,
//! answer in the room.
//!
//! Ordering still matters for durability: a poll cursor only advances AFTER the
//! commands from that window have been executed, so a crash re-polls instead of
//! losing an order.

use std::collections::BTreeMap;

use anyhow::Result;
use serde::Serialize;
use serde_json::{json, Value};

use crate::adapters::{tfl5, ChannelCommand, CommandKind, PollResult, Skip};
use crate::config::Config;
use crate::db::{Db, RunFinish};
use crate::logging;

#[derive(Debug, Serialize)]
pub struct CycleSummary {
    pub ms: u128,
    pub ingested: Value,
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

/// The session hub stopped most recently, kept whole so `/tell` can resume it.
///
/// `/stop` answers "hội thoại vẫn còn — nói tiếp bằng /tell", and that promise used to
/// break on the very next command: `claude agents` drops a stopped background
/// session from its list within seconds, and `/tell` gated on that list, so the
/// reply was "không thấy phiên đang chạy nữa" for the session hub had just
/// stopped ON PURPOSE. Resuming does not need a process — it needs a transcript
/// and the account that owns it, which is exactly what this row carries.
pub const STOPPED_KEY: &str = "stopped:session";

/// Ids of the sessions THIS hub started, newest last.
///
/// Nothing in `claude agents` says who opened a session: a background row looks
/// the same whether hub ran `/new` from the phone or someone typed `claude --bg`
/// in a window. The phone needs the difference — those are the rows it can stop
/// and talk to — so hub writes down what it starts instead of guessing.
pub const STARTED_KEY: &str = "started:by_hub";

/// How many ids to keep. Enough to cover every session alive at once on this
/// machine many times over; the list is for labelling a screen, not an audit.
const STARTED_KEEP: usize = 50;

fn started_ids(db: &Db) -> Vec<String> {
    match db.get_cursor(STARTED_KEY) {
        Ok(Some(raw)) => serde_json::from_str::<Vec<String>>(&raw).unwrap_or_else(|e| {
            logging::warn("started_list_unparseable", json!({ "err": e.to_string() }));
            Vec::new()
        }),
        Ok(None) => Vec::new(),
        Err(e) => {
            logging::warn("started_list_unreadable", json!({ "err": e.to_string() }));
            Vec::new()
        }
    }
}

fn remember_started(db: &Db, session_id: &str) {
    let mut ids = started_ids(db);
    if ids.iter().any(|i| i == session_id) {
        return;
    }
    ids.push(session_id.to_string());
    if ids.len() > STARTED_KEEP {
        let cut = ids.len() - STARTED_KEEP;
        ids.drain(..cut);
    }
    match serde_json::to_string(&ids) {
        Ok(v) => {
            if let Err(e) = db.set_cursor(STARTED_KEY, &v) {
                logging::error(
                    "started_list_not_saved",
                    json!({ "session": session_id, "err": e.to_string() }),
                );
            }
        }
        Err(e) => logging::error("started_list_not_encodable", json!({ "err": e.to_string() })),
    }
}

/// Stamp `started_by_hub` on the rows hub opened.
///
/// Lives here rather than in `sessions` because it needs the book; every
/// surface that shows sessions (portal snapshot, `hub sessions`) calls it, so
/// the phone and the CLI cannot disagree about who opened what.
pub fn mark_started_by_hub(db: &Db, snap: &mut crate::sessions::SessionsSnapshot) {
    let ids = started_ids(db);
    if ids.is_empty() {
        return;
    }
    for s in snap.sessions.iter_mut() {
        s.started_by_hub = ids.iter().any(|i| *i == s.session_id);
    }
}

/// Keep a stopped session whole, minus the fields that only make sense while a
/// process is behind it.
///
/// `status`/`state`/`pid` are cleared on purpose: `sessions::tell` refuses a
/// session whose status is `busy`, and a row frozen at the instant of stopping
/// still says `busy` — the session would be unreachable forever on the strength
/// of a field describing a process that no longer exists.
fn remember_stopped(db: &Db, s: &crate::sessions::LiveSession) {
    let mut row = s.clone();
    row.status = None;
    row.state = None;
    row.pid = 0;
    row.host = "dead".to_string();
    match serde_json::to_string(&row) {
        Ok(json) => {
            if let Err(e) = db.set_cursor(STOPPED_KEY, &json) {
                logging::error(
                    "stopped_session_not_saved",
                    json!({ "session": row.session_id, "err": e.to_string() }),
                );
            }
        }
        Err(e) => logging::error(
            "stopped_session_not_encodable",
            json!({ "session": row.session_id, "err": e.to_string() }),
        ),
    }
}

/// The session hub stopped a moment ago, if it is the one being asked for.
///
/// Returns `None` — never a guess — when the stored row is for some other
/// session: telling the WRONG session would be worse than refusing.
fn stopped_session(db: &Db, want: &str) -> Option<crate::sessions::LiveSession> {
    if want.is_empty() {
        return None;
    }
    let raw = match db.get_cursor(STOPPED_KEY) {
        Ok(Some(v)) => v,
        Ok(None) => return None,
        Err(e) => {
            logging::warn("stopped_session_unreadable", json!({ "err": e.to_string() }));
            return None;
        }
    };
    match serde_json::from_str::<crate::sessions::LiveSession>(&raw) {
        Ok(s) if s.session_id == want => Some(s),
        Ok(_) => None,
        Err(e) => {
            logging::warn(
                "stopped_session_unparseable",
                json!({ "err": e.to_string() }),
            );
            None
        }
    }
}

pub fn project_pin_key(thread_key: &str) -> String {
    format!("pin:project:{thread_key}")
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
                let polled = res.seen;
                // The lines themselves are not kept. They used to be inserted,
                // routed, rated and triaged; now a line that is not an order is
                // just conversation, and conversation belongs in the room it was
                // typed in — not in a database on its way to a paid classifier.
                let inserted = 0usize;

                // Cursors last: a command that failed to run must not have its
                // window skipped.
                for (k, v) in &res.cursors {
                    db.set_cursor(k, v)?;
                }

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
                // The failure is on the run row and in the log; there is no
                // dead-letter table any more to hold a copy of a message hub
                // never stored in the first place.
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
        // Every verb answers for itself. There used to be a second stage below
        // this match — "look the decision up, then approve or reject it" — and
        // a verb that forgot to end with `Some(ack)` fell into it and logged
        // "Không tìm thấy decision #0" as its reply. That whole stage went with
        // the inbox on 2026-08-08; there are no decisions left to look up.
        let answered: Option<String> = match cmd.kind {
            CommandKind::Help => {
                let ack = "Lệnh dùng được trong phòng này:\n\
                     — Phiên Claude —\n\
                     /session <id> — theo một phiên (bỏ theo: /session -)\n\
                     /new <dự án> <việc> — mở phiên nền làm việc đó (chạy không hỏi ai)\n\
                     /ask <câu hỏi> — hỏi bên lề phiên đang theo; phiên gốc KHÔNG bị đụng\n\
                     /tell <nội dung> — nói tiếp vào phiên nền (phải dừng nó trước)\n\
                     /stop [id] — dừng phiên nền, hội thoại vẫn giữ\n\
                     /handover [id] — đóng sổ, lấy bản bàn giao + id để làm tiếp\n\
                     — Vận hành —\n\
                     /project [tên] — xem / ghim dự án cho phòng (bỏ ghim: /project -)\n\
                     /ingest · /run · /doctor — poll kênh · chạy một vòng · kiểm tra thật\n\
                     /set <khoá> <giá trị> — sửa một trường cấu hình\n\
                     /help — bảng này"
                    .to_string();
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
            }
            // Whole-cycle verbs. `Ingest` and `Run` are answered rather than
            // executed here on purpose: this code already runs INSIDE a cycle
            // (`run_once` → `ingest` → `execute_commands`), so calling either
            // one would re-enter the pipeline recursively. The cycle carrying
            // this command does the work a moment later anyway.
            CommandKind::Ingest | CommandKind::Run => {
                let what = if matches!(cmd.kind, CommandKind::Run) {
                    "Vòng đang chạy ngay bây giờ (lệnh này được xử lý bên trong nó)."
                } else {
                    "Đang đọc phòng trong vòng hiện tại."
                };
                reply_in_channel(db, cfg, adapter, cmd, what);
                Some(what.to_string())
            }
            CommandKind::Doctor => {
                let probe = crate::portal::probe_now(cfg);
                reply_in_channel(db, cfg, adapter, cmd, &probe);
                Some(probe)
            }
            CommandKind::Handover => {
                // Books, not brakes. This costs a `claude` call and every cent
                // lands in `spend` — but it is the OWNER asking, so it is not
                // gated the way the unattended robot is (see `owner_budget_state`).
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
                let ack = match live.sessions.iter().find(|s| s.session_id == want) {
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
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
            }
            CommandKind::New => {
                // `<dự án> <việc>` — the project decides the folder, and only a
                // folder hub already knows about is accepted: a typo must not
                // start an agent loose in the wrong repo.
                let (name, task) = cmd
                    .arg
                    .split_once(char::is_whitespace)
                    .unwrap_or((&cmd.arg, ""));
                let name = name.trim();
                let known = known_projects(cfg);
                let dir = crate::config::project_dir(cfg, name);
                let ack = match dir {
                    Some(d)
                        if known.contains(&name.to_string()) || cfg.projects.contains_key(name) =>
                    {
                        match crate::sessions::start_background(cfg, name, &d, task) {
                            Ok(s) => {
                                // Follow it straight away: the person who just
                                // started a job wants to watch it, and making
                                // them hunt for it in the list is a step hub
                                // can take for them.
                                remember_started(db, &s.session_id);
                                if let Err(e) = db.set_cursor(FOCUS_SESSION_KEY, &s.session_id) {
                                    logging::error(
                                        "focus_after_start_failed",
                                        json!({ "err": e.to_string() }),
                                    );
                                }
                                logging::info(
                                    "session_started",
                                    json!({ "project": s.project, "session": s.session_id, "cwd": s.cwd }),
                                );
                                format!(
                                    "🚀 Đã mở phiên nền cho {} tại {}.\nPhiên {}\n\n⚠ Nó chạy không hỏi ai. Dừng bằng nút Dừng hoặc /stop.",
                                    s.project,
                                    s.cwd,
                                    &s.session_id[..8.min(s.session_id.len())]
                                )
                            }
                            // Không cắt 200 như các ack khác: lời báo hỏng ở đây
                            // MANG THEO cách gỡ, và cắt 200 chặt đúng nửa đó —
                            // người đọc nhận được tin xấu mà không nhận được
                            // lối ra.
                            Err(e) => format!(
                                "⚠ không mở được phiên: {}",
                                crate::exec::truncate(&e.to_string(), 700)
                            ),
                        }
                    }
                    _ => format!(
                        "⚠ không biết dự án '{}'. Đang có: {}",
                        crate::exec::truncate(name, 40),
                        known.join(", ")
                    ),
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
            }
            CommandKind::Stop => {
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
                let ack = match live.sessions.iter().find(|s| s.session_id == want) {
                    None if want.is_empty() => "⚠ chưa mở phiên nào.".to_string(),
                    None => format!(
                        "⚠ không thấy phiên '{}' đang chạy",
                        crate::exec::truncate(&want, 40)
                    ),
                    Some(s) => match crate::sessions::stop_background(cfg, s) {
                        Ok(()) => {
                            remember_stopped(db, s);
                            logging::info("session_stopped", json!({ "session": s.session_id }));
                            format!(
                                "⏹ Đã dừng phiên {}. Hội thoại vẫn còn — nói tiếp bằng /tell hoặc mở lại trên máy.",
                                s.name
                            )
                        }
                        Err(e) => format!(
                            "⚠ không dừng được: {}",
                            crate::exec::truncate(&e.to_string(), 200)
                        ),
                    },
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
            }
            CommandKind::Tell => {
                let want = db
                    .get_cursor(FOCUS_SESSION_KEY)
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let live = crate::sessions::snapshot(cfg);
                // Đã dừng KHÔNG phải là đã mất: `--resume` nối vào nhật ký, nó
                // không cần tiến trình nào đang sống. Và dừng-rồi-nói-tiếp
                // chính là đường DUY NHẤT — claude từ chối resume một phiên nền
                // đang chạy (đo 2026-08-08).
                let target = live
                    .sessions
                    .iter()
                    .find(|s| s.session_id == want)
                    .cloned()
                    .or_else(|| stopped_session(db, &want));
                let ack = match target.as_ref() {
                    None if want.is_empty() => {
                        "⚠ chưa mở phiên nào. Chạm một phiên rồi nói tiếp.".to_string()
                    }
                    None => format!(
                        "⚠ không thấy phiên '{}' đang chạy nữa",
                        crate::exec::truncate(&want, 40)
                    ),
                    Some(s) => match crate::sessions::tell(cfg, s, &cmd.arg) {
                        Ok(t) => {
                            if let Err(e) = db.record_spend(
                                "tell",
                                &t.session_id,
                                t.cost_usd,
                                &crate::exec::truncate(&t.text, 80),
                            ) {
                                logging::error(
                                    "spend_record_failed",
                                    json!({ "kind": "tell", "err": e.to_string() }),
                                );
                            }
                            logging::info(
                                "tell_done",
                                json!({ "session": t.session_id, "cost_usd": t.cost_usd }),
                            );
                            format!(
                                "➡️ Đã nói tiếp vào phiên {}:\n\n{}",
                                t.source_name, t.answer
                            )
                        }
                        Err(e) => format!(
                            "⚠ không nói tiếp được: {}",
                            crate::exec::truncate(&e.to_string(), 300)
                        ),
                    },
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
            }
            CommandKind::Ask => {
                // Books, not brakes — same as handover. The owner asking their
                // own session a question is the owner working, not a robot
                // running loose; the price is reported, not used to refuse.
                //
                // No id in the verb: the target is the session being read.
                // Asking with nothing open is a mistake worth naming, not a
                // silent no-op.
                let want = db
                    .get_cursor(FOCUS_SESSION_KEY)
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let live = crate::sessions::snapshot(cfg);
                let ack = match live.sessions.iter().find(|s| s.session_id == want) {
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
                            // Say what it did NOT do as well as what it did:
                            // the whole point of the feature is that the running
                            // session was left alone, and the person cannot see
                            // that from here.
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
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
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
                    // Phiên VỪA DỪNG vẫn phải theo được: màn chi tiết đang mở
                    // chính nó, và `/tell` sau đó cần đúng con trỏ này. Không có
                    // vế dưới thì bấm Dừng xong là màn tự đá mình ra — đo được
                    // 2026-08-09, và nó nuốt luôn cả đường /tell.
                    let target = live
                        .sessions
                        .iter()
                        .find(|s| s.session_id == want)
                        .cloned()
                        .or_else(|| stopped_session(db, want));
                    match target {
                        Some(s) => match db.set_cursor(FOCUS_SESSION_KEY, want) {
                            Ok(()) => {
                                let how = if s.pid == 0 { " — đã dừng, vẫn nói tiếp được" } else { "" };
                                format!("👁 Đang theo phiên {} ({}){}", s.name, s.account, how)
                            }
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
                Some(ack)
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
                        // There used to be a fallback here: "no pin, but the
                        // last message on this thread mentioned <project>". It
                        // read the stored messages, and messages are no longer
                        // stored — a guess drawn from an empty table would be a
                        // confident answer with nothing behind it.
                        Ok(None) => {
                            "Chưa ghim dự án cho phòng này. Đặt bằng: /project <tên>".to_string()
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
                Some(ack)
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
                Some(ack)
            }
        };

        // Every arm above answers. This used to fall through into a decision
        // lookup, where `decision_id = 0` found nothing and the log recorded
        // "Không tìm thấy decision #0" as the reply — for `/session`, `/ask`
        // and `/handover`, every single time. The room got the right answer, so
        // nothing looked broken; only the log lied, which is the worst place
        // for it to lie because the log is where you go when something IS
        // broken.
        if let Some(ack) = answered {
            logging::info(
                "channel_command_handled",
                json!({ "adapter": adapter, "decision_id": cmd.decision_id, "kind": format!("{:?}", cmd.kind), "ack": ack }),
            );
            continue;
        }
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

/// Today's OWNER spend — what the person set off by pressing a button.
///
/// **It reports; it does not refuse.** A daily ceiling exists to rein in a robot
/// nobody is watching (`daily_budget_usd`, non-negotiable #9). Pressing "hỏi bên
/// lề" on a phone is the owner working, exactly as if they had typed it in the
/// terminal — and nobody puts a $2/day ceiling on their own terminal.
///
/// This was wired as a REFUSAL for one afternoon on 2026-08-08 and Hà threw it
/// out the same day ("bỏ hết github rồi sao vẫn trần chuồng gì thế"). The books
/// behind it were the giveaway: of $2.98 triaged that day, $2.24 belonged to the
/// github and devlog branches that had already been deleted, so the ceiling was
/// mostly the ghost of a product that no longer existed — and it was reaching
/// out to block the owner's own hand.
///
/// What stays is the accounting: every owner-triggered call books into `spend`
/// and its price travels to the screen, because a cost the person cannot see is
/// worse than one they can.
#[derive(Debug, Clone, Copy)]
pub struct OwnerBudget {
    pub spent_usd: f64,
}

pub fn owner_budget_state(db: &Db) -> OwnerBudget {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let spent = db.owner_cost_on_day(&today).unwrap_or_else(|e| {
        // Never swallow it: the number on screen would silently become a lie.
        logging::error("owner_spend_read_failed", json!({ "err": e.to_string() }));
        0.0
    });
    OwnerBudget { spent_usd: spent }
}

/// Set ONE config field by dotted path, then round-trip through `Config` so a
/// typo is a rejection, not a corrupted file.
///
/// Deliberately field-at-a-time rather than "paste a JSON blob": the value
/// travels through a chat room, and one key + one value is auditable at a
/// glance. The type of the EXISTING value decides how the text is parsed, so
/// `/set adapters.tfl5.enabled false` cannot turn a bool into the string
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
    // No triage, and nothing to flush. hub used to spend money on its own here:
    // every line typed in the room went through a `claude -p` call to be sorted
    // into an inbox, and a daily ceiling existed to stop that from running away.
    // The inbox is gone (2026-08-08) and the room now carries orders, not mail
    // — so the only thing that costs money is a button the owner presses
    // (`/ask`, `/handover`, `/new`, `/tell`). hub no longer spends unwatched,
    // which is why the ceiling that guarded it is gone too.
    let summary = CycleSummary {
        ms: started.elapsed().as_millis(),
        ingested,
    };
    logging::info("cycle_done", serde_json::to_value(&summary)?);
    Ok(summary)
}
