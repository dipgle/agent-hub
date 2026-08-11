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

/// Phiên mà MỆNH LỆNH NÀY nói tới — id nằm trong chính câu lệnh, không phải
/// trong một con trỏ ai đó vừa đặt.
///
/// 🔴 Vì sao có hàm này (đo 2026-08-11, và đây là lỗi nặng nhất của cả ngày):
/// `/ask`, `/tell`, `/type`, `/key` đều định vị bằng `FOCUS_SESSION_KEY` — một
/// biến toàn cục đổi được bởi một lệnh KHÁC. Trang vì thế phải gửi hai câu
/// (`/session <id>` rồi `/type <chữ>`), và hai câu ấy là hai bản ghi trong
/// phòng chat: **thứ tự KHÔNG được bảo đảm**. Trace thật:
///
/// ```text
/// 10:32:38.834  /session 3e9a7fd6…   ← hoãn
/// 10:32:51.794  /ask Tóm tắt…        ← hoãn
/// 10:32:5x      ack: "Hỏi bên lề phiên projects-1f"   ← SAI PHIÊN
/// 10:33:42.128  ack: "Đang theo phiên projects-ff"    ← lệnh trước, chạy sau
/// ```
///
/// Hậu quả không dừng ở một câu trả lời lạc: hub đã **gõ thật** vào cửa sổ của
/// một phiên đang làm việc khác. Cùng cơ chế ấy, `/type` gửi chữ và `/key` gửi
/// phím vào nhầm terminal. `/stop` và `/handover` không dính vì chúng mang id
/// ngay trong câu lệnh — và đó chính là bản vá: mệnh lệnh nào ĐỤNG vào một
/// phiên sống thì phải TỰ NÓI nó đụng vào phiên nào.
///
/// Trả về `(id, phần còn lại của câu lệnh)`. Không có id ở đầu thì rơi về con
/// trỏ focus như cũ — nhưng có log, vì đó là đường đã biết là hỏng.
fn target_and_rest(db: &Db, arg: &str) -> (String, String) {
    if let Some((id, rest)) = split_target(arg) {
        return (id, rest);
    }
    let focus = db.cursor_or_log(FOCUS_SESSION_KEY).unwrap_or_default();
    logging::info(
        "route_target_from_focus",
        json!({
            "focus": focus,
            "why": "câu lệnh không mang id — dùng con trỏ đang theo, thứ tự lệnh KHÔNG bảo đảm",
        }),
    );
    (focus, arg.trim().to_string())
}

/// Nửa THUẦN của `target_and_rest`: câu lệnh này có tự nói nó nhắm vào phiên nào
/// không? `Some((id, phần còn lại))` nếu có, `None` nếu không.
///
/// Tách ra để kiểm được mà không cần một cái máy đang chạy `claude` — và vì đây
/// là chỗ dễ sai theo cả hai chiều: bắt hụt id thì lệnh rơi về con trỏ focus
/// (đúng con đường đã gõ nhầm phiên), còn bắt nhầm thì **chữ đầu của câu người
/// ta gõ bị nuốt mất** và phiên nhận một câu cụt.
///
/// Nhận bằng HÌNH DẠNG uuid, không hỏi danh sách phiên: chỗ gọi vẫn phải tự
/// kiểm phiên có sống không, còn một câu tiếng Việt hay một tên dự án thì không
/// bao giờ mang hình dạng này.
pub fn split_target(arg: &str) -> Option<(String, String)> {
    let arg = arg.trim();
    let (head, rest) = match arg.split_once(char::is_whitespace) {
        Some((h, r)) => (h, r.trim()),
        None => (arg, ""),
    };
    let looks_like_id = head.len() >= 32
        && head.matches('-').count() == 4
        && head.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    looks_like_id.then(|| (head.to_string(), rest.to_string()))
}

/// Sổ ghi trạng thái từng phiên ở lượt trước, để thấy được CHUYỂN trạng thái.
pub const WATCH_KEY: &str = "watch:sessions";

/// Nói ra những phiên vừa xong việc / vừa tắt hẳn — một lần cho mỗi lần chuyển.
///
/// Hà 2026-08-10: *"có bắt được trường hợp đang chạy và dừng lại hoàn toàn
/// không? nếu có thì thể hiện được trên ui và gửi vào tele"*.
///
/// Hai đường loa, cùng MỘT câu chữ (`Change::say`): phòng chat (nên nó nằm luôn
/// trên tab Trao đổi của điện thoại, và có dấu vết đọc lại được) và Telegram
/// (nên nó tới được lúc không ai mở trang). Khác câu ở hai nơi là sau này không
/// ai đối chiếu được.
///
/// Không lời gọi `claude` nào ⟹ **không tốn hạn mức** (luật §8). Lỗi ở một
/// đường không được làm câm đường kia, và cả hai đều log khi hỏng — một cái loa
/// im lặng thì tệ hơn không có loa.
pub fn announce_changes(db: &Db, cfg: &Config, live: &[crate::sessions::LiveSession]) {
    let prev: BTreeMap<String, crate::watch::Mark> = db
        .cursor_or_log(WATCH_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    let (changes, next) = crate::watch::changes(&prev, live, chrono::Utc::now().timestamp());

    match serde_json::to_string(&next) {
        // Ghi sổ TRƯỚC khi nói: nói xong mới ghi mà sập giữa chừng thì lượt sau
        // nói lại y hệt. Thà lỡ một lời báo còn hơn một cái loa lặp.
        Ok(v) => {
            if let Err(e) = db.set_cursor(WATCH_KEY, &v) {
                logging::error("watch_state_save_failed", json!({ "err": e.to_string() }));
                return;
            }
        }
        Err(e) => {
            logging::error("watch_state_encode_failed", json!({ "err": e.to_string() }));
            return;
        }
    }

    for c in changes {
        // Tra lại phiên để có `tty` (đọc màn), câu cuối nó nói, và ai mở nó.
        let id = match &c {
            crate::watch::Change::Finished { id, .. } => id.clone(),
            crate::watch::Change::Ended { id, .. } => id.clone(),
        };
        let row = live.iter().find(|s| s.session_id == id);

        // NHÌN màn đúng một lần, cho đúng phiên vừa im. Chuyện này hiếm (vài
        // lần một giờ) nên nó rẻ; đọc màn cho mọi phiên mỗi vòng mới là thứ
        // từng kéo một vòng lên 90 giây.
        let idle = match (&c, row) {
            (crate::watch::Change::Finished { .. }, Some(s)) => {
                match crate::keys::look(&s.tty, 8) {
                    crate::keys::Look::Saw { choices, .. } if !choices.is_empty() => {
                        crate::watch::Idle::Asking {
                            n: choices.len(),
                            // Chữ đi kèm con số: `look` đã trả `Saw` nghĩa là màn
                            // qua được cổng quét rò rỉ, nên nội dung này đi ra
                            // ngoài được.
                            options: choices.iter().map(|(_, t)| t.clone()).collect(),
                        }
                    }
                    crate::keys::Look::Saw { .. } => crate::watch::Idle::Prompt,
                    crate::keys::Look::Withheld { choices, .. } if choices > 0 => {
                        // Chỉ con số — con số không mang chữ nào ra khỏi máy.
                        crate::watch::Idle::Asking { n: choices, options: vec![] }
                    }
                    crate::keys::Look::Withheld { .. } => crate::watch::Idle::Prompt,
                    crate::keys::Look::Blind { .. } => crate::watch::Idle::Unknown,
                }
            }
            _ => crate::watch::Idle::Unknown,
        };

        // KẾT CỤC của một phiên biến mất: nói ĐÚNG thứ dò được, không gọi tất cả
        // là "tắt hẳn".
        //
        // Hà 2026-08-10: *"tắt hẳn là sao? ý chung chung thế… tắt hẳn là phải
        // thoát khỏi cli mới đúng, tắt hẳn terminal"*. Đúng, và "biến khỏi danh
        // sách `claude agents`" gộp ba chuyện khác hẳn nhau. Phân biệt bằng một
        // câu hỏi rẻ, hỏi đúng lúc (chuyện này hiếm): cửa sổ Terminal mang tty
        // ấy còn không?
        let fate = if let crate::watch::Change::Ended { tty, kind, .. } = &c {
            // Phiên nền không có cửa sổ nào để đóng, nên dừng nó LÀ tắt hẳn.
            if kind == "background" || tty.is_empty() {
                Some("đã tắt hẳn".to_string())
            } else {
                match crate::keys::window_of(tty) {
                    // Cửa sổ còn ⟹ CHƯA phải "tắt hẳn" theo đúng định nghĩa Hà
                    // đặt ("thoát cli VÀ đóng terminal"). Nói "đã tắt", kèm ĐÚNG
                    // một cụm trong ngoặc — vừa đủ để biết không phải làm gì.
                    Ok(Some(_)) => Some("đã tắt (thoát CLI, cửa sổ terminal còn mở)".to_string()),
                    Ok(None) => Some("đã tắt hẳn".to_string()),
                    Err(_) => Some("đã tắt".to_string()),
                }
            }
        } else {
            None
        };

        // IM cho phiên chủ máy đang ngồi gõ, TRỪ khi nó đang kẹt hỏi.
        //
        // Hà 2026-08-10: một phiên terminal anh đang nhìn thẳng vào bắn ba tin
        // trong mười sáu phút. Cái loa này có giá trị ở phiên KHÔNG ai nhìn —
        // phiên hub tự mở từ điện thoại — hoặc khi phiên KẸT, vì kẹt thì dù
        // đang ngồi trước máy cũng đáng được gọi. Còn "một lượt vừa xong" trên
        // phiên anh tự tay gõ thì anh thấy trước hub.
        let stuck = matches!(idle, crate::watch::Idle::Asking { .. });
        let hub_opened = row.is_some_and(|s| s.started_by_hub);
        if matches!(c, crate::watch::Change::Finished { .. }) && !stuck && !hub_opened {
            logging::info(
                "session_change_muted",
                json!({ "session": id, "why": "phiên terminal của chủ máy, không kẹt" }),
            );
            continue;
        }

        // IM khi một phiên CON kết thúc bình thường.
        //
        // Hà 2026-08-11: *"phiên con được gọi từ phiên cha mà tắt cũng đang gửi
        // qua tele, có cần không?"* — không. Phiên con là một chi tiết bên
        // trong lượt việc của phiên cha; phiên cha xong thì tự có tin của nó,
        // nên tin của con chỉ làm loãng đúng cái tin đáng đọc. Cùng luật với
        // "đừng kêu vào mặt người đang nhìn" (2026-08-10).
        //
        // MỘT ngoại lệ, và nó là lý do chỗ này không phải `continue` thẳng: con
        // tắt lúc ĐANG CHẠY DỞ là chuyện đáng xem lại — phiên cha có thể đang
        // đứng chờ một kết quả sẽ không bao giờ tới.
        if let crate::watch::Change::Ended { parent, was_working, .. } = &c {
            if !parent.is_empty() && !was_working {
                logging::info(
                    "session_change_muted",
                    json!({ "session": id, "parent": parent, "why": "phiên con kết thúc bình thường" }),
                );
                continue;
            }
        }

        // Câu cuối phiên nói ra — thứ làm mỗi tin KHÁC nhau. Nó đã qua cổng
        // quét rò rỉ ở `sessions::snapshot` trước khi vào ảnh chụp.
        let tail = row.and_then(|s| s.last_text.as_deref());
        let text = match (&c, &fate) {
            (crate::watch::Change::Ended { name, was_working, .. }, Some(f)) => {
                // Tắt lúc đang chạy dở là chuyện ĐÁNG XEM LẠI — đó là lần duy
                // nhất một tin "đã tắt" đòi người ta làm gì.
                let warn = if *was_working {
                    " — nó đang chạy dở, nên xem lại"
                } else {
                    ""
                };
                format!("⏹ {name} {f}{warn}.")
            }
            _ => c.say(&idle, tail),
        };
        logging::info("session_change", json!({ "text": text }));
        if let Err(e) = tfl5::send(&cfg.adapters.tfl5, "", None, &text) {
            logging::error("session_change_room_failed", json!({ "err": logging::err_chain(&e) }));
        }
        if let Err(e) = crate::confirm::tell(cfg, &text) {
            logging::error("session_change_telegram_failed", json!({ "err": e }));
        }
    }
}

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

/// Đóng sổ hộ, cho phiên đã đầy ngữ cảnh VÀ đã chạy xong chỗ dở.
///
/// Chạy mỗi vòng. Không có nút nào bấm ra nó — đó là điểm: một phiên đầy 80%
/// thì mỗi lượt sau vừa chậm vừa tốn, và người ta thường chỉ nhận ra khi đã
/// muộn. Nhưng nó chỉ ra tay khi CHẮC CHẮN phiên đang rảnh; mọi lý do giữ lại
/// đều được ghi log, vì một cơ chế tự chạy mà im lặng là một cơ chế không ai
/// dám tin.
fn auto_handover(db: &Db, cfg: &Config) {
    if !cfg.auto_handover.enabled {
        return;
    }
    let mut live = crate::sessions::snapshot(cfg);
    mark_started_by_hub(db, &mut live);
    let done: Vec<String> = db
        .cursor_or_log(AUTO_DONE_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();

    for s in &live.sessions {
        if s.host == "dead" || s.context_tokens == 0 {
            continue;
        }
        // Cửa sổ ngữ cảnh theo model; không biết model thì lấy mức phổ biến.
        let window: u64 = if s.model.as_deref().is_some_and(|m| m.contains("haiku")) {
            200_000
        } else {
            1_000_000
        };
        let pct = ((s.context_tokens * 100) / window).min(100) as u8;
        // Chỉ đọc màn khi phiên ĐÃ đủ đầy. Mỗi lần đọc là một lần gọi
        // AppleScript vào Terminal, và đọc cho mọi phiên mỗi vòng đã kéo một
        // vòng từ ~18 giây lên **90 giây** (đo 2026-08-10) — đủ chậm để một
        // lệnh gõ từ điện thoại nằm chờ hơn một phút. Phiên còn 5% ngữ cảnh thì
        // màn của nó không đổi được quyết định nào, nên đừng hỏi.
        let screen = if pct >= cfg.auto_handover.at_percent {
            crate::keys::screen_of(&s.tty, 40)
        } else {
            None
        };
        let idle_sec = s
            .last_activity
            .as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_seconds().max(0) as u64)
            .unwrap_or(0);

        let why = auto_handover_why(
            pct,
            cfg.auto_handover.at_percent,
            done.iter().any(|d| d == &s.session_id),
            screen.is_some(),
            screen.as_ref().is_some_and(|(t, _)| crate::keys::is_busy(t)),
            screen.as_ref().is_some_and(|(_, c)| !c.is_empty()),
            s.pending_subagents,
            idle_sec,
            cfg.auto_handover.idle_sec,
        );
        if why != AutoWhy::Do {
            // Chỉ log khi ĐÃ đủ đầy — dưới ngưỡng thì im, không thì mỗi vòng
            // sinh một dòng cho mọi phiên.
            if pct >= cfg.auto_handover.at_percent {
                logging::info(
                    "auto_handover_held",
                    json!({ "session": s.session_id, "pct": pct, "why": format!("{why:?}") }),
                );
            }
            continue;
        }

        logging::info(
            "auto_handover_firing",
            json!({ "session": s.session_id, "name": s.name, "pct": pct, "idle_sec": idle_sec }),
        );
        match crate::sessions::handover(cfg, s) {
            Ok(h) => {
                let mut next = done.clone();
                next.push(s.session_id.clone());
                if next.len() > 50 {
                    let cut = next.len() - 50;
                    next.drain(..cut);
                }
                if let Ok(v) = serde_json::to_string(&next) {
                    let _ = db.set_cursor(AUTO_DONE_KEY, &v);
                }
                if let Err(e) = db.record_spend("auto_handover", &h.new_session_id, h.cost_usd, &s.name) {
                    logging::error("spend_record_failed", json!({ "err": e.to_string() }));
                }
                let msg = format!(
                    "📋 Tự đóng sổ {} (ngữ cảnh {}%, đã rảnh {} phút).\nPhiên mới: {}\n{}",
                    s.name,
                    pct,
                    idle_sec / 60,
                    &h.new_session_id[..8.min(h.new_session_id.len())],
                    h.resume_command
                );
                // Báo vào phòng: mọi thứ hub tự làm đều phải có vết ở nơi đọc
                // được, nhất là thứ chạy khi không ai bấm.
                if let Err(e) = crate::adapters::tfl5::send(
                    &cfg.adapters.tfl5,
                    &cfg.adapters.tfl5.room,
                    None,
                    &msg,
                ) {
                    logging::error("auto_handover_notice_failed", json!({ "err": e.to_string() }));
                }
            }
            Err(e) => logging::error(
                "auto_handover_failed",
                json!({ "session": s.session_id, "err": e.to_string() }),
            ),
        }
        // MỘT phiên mỗi vòng: đóng sổ tốn hạn mức, và làm hàng loạt trong một
        // nhịp là thứ không ai kịp can.
        break;
    }
}

/// Phiên nào đã được hub tự đóng sổ rồi — để không đóng hai lần.
pub const AUTO_DONE_KEY: &str = "auto_handover:done";

/// Vì sao MỘT phiên nên (hoặc không nên) được tự đóng sổ lúc này.
///
/// Trả về `Err(lý do)` khi chưa nên — lý do là chữ, để log nói được điều gì đã
/// giữ nó lại. Một cơ chế tự động mà không giải thích được vì sao nó im lặng là
/// một cơ chế không ai dám tin.
#[derive(Debug, PartialEq)]
pub enum AutoWhy {
    Do,
    NotFull(u8),
    Busy,
    Asking,
    Subagents(usize),
    TooFresh(u64),
    AlreadyDone,
    NoWindow,
}

/// Quyết định thuần: không đọc đĩa, không gọi ai — để test được từng điều kiện.
///
/// Thứ tự kiểm là có chủ ý: những lý do RẺ và CHẮC đứng trước, để log ghi đúng
/// nguyên nhân đầu tiên chứ không phải nguyên nhân cuối cùng.
#[allow(clippy::too_many_arguments)]
pub fn auto_handover_why(
    pct: u8,
    at_percent: u8,
    already_done: bool,
    has_window: bool,
    busy: bool,
    asking: bool,
    subagents: usize,
    idle_sec: u64,
    need_idle_sec: u64,
) -> AutoWhy {
    if already_done {
        return AutoWhy::AlreadyDone;
    }
    if pct < at_percent {
        return AutoWhy::NotFull(pct);
    }
    // Không đọc được màn thì KHÔNG đoán là rảnh. Đóng sổ giữa chừng làm mất
    // đúng thứ phiên đang làm, nên thiếu tín hiệu là lý do để KHÔNG làm.
    if !has_window {
        return AutoWhy::NoWindow;
    }
    if busy {
        return AutoWhy::Busy;
    }
    // Đang hỏi thì càng không: đóng sổ lúc ấy là trả lời thay người dùng.
    if asking {
        return AutoWhy::Asking;
    }
    if subagents > 0 {
        return AutoWhy::Subagents(subagents);
    }
    if idle_sec < need_idle_sec {
        return AutoWhy::TooFresh(idle_sec);
    }
    AutoWhy::Do
}

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
        s.started_by_hub = ids.contains(&s.session_id);
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
                     — Gõ thẳng vào cửa sổ phiên —\n\
                     /type <chữ> — gõ chữ vào phiên đang theo (Terminal, kèm Enter)\n\
                     /key <up|down|left|right|enter|esc|tab|space|1-9> — bấm một phím\n\
                     /shot — đọc chữ đang hiện trên màn của phiên\n\
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
                    db.cursor_or_log(FOCUS_SESSION_KEY).unwrap_or_default()
                } else {
                    want
                };
                let live = crate::sessions::snapshot(cfg);
                let ack = match live.sessions.iter().find(|s| s.session_id == want) {
                    None => format!(
                        "⚠ không thấy phiên '{}' đang chạy",
                        crate::exec::truncate(&want, 40)
                    ),
                    // Hà hỏi 2026-08-10: *"nút đóng sổ chưa gửi xác nhận qua
                    // tele?"* — đúng, và nó nên có. Đóng sổ KHÔNG phá phiên gốc
                    // (chạy trên bản fork), nhưng nó gọi `claude` thật: hai lần
                    // lỡ tay trong CHÍNH kịch bản kiểm thử của tôi sáng nay tốn
                    // 3.19 + 4.44 theo thước đo. Với acc3 đang ở 98% hạn mức
                    // tuần, một cú chạm nhầm trên danh sách là một cú chạm đắt.
                    // Cùng chốt chặn, khác câu hỏi: ở đây cái mất là HẠN MỨC.
                    Some(s) => match ask_owner(
                        db,
                        cfg,
                        adapter,
                        cmd,
                        &format!(
                            "Đóng sổ phiên {} ({})? Việc này gọi claude trên bản fork và tốn hạn mức.",
                            s.name, s.account
                        ),
                        "đóng sổ phiên nào",
                    ) {
                        Some(refusal) => refusal,
                        None => match crate::sessions::handover(cfg, s) {
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
                // `@tài-khoản` đứng ngay sau tên dự án: `/new hub @acc2 việc…`.
                // Không có thì dùng tài khoản mặc định — giữ nguyên cách gõ cũ.
                let (account, task) = match task.trim().strip_prefix('@') {
                    Some(rest) => {
                        let (acc, rest) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                        (Some(acc.trim().to_string()), rest)
                    }
                    None => (None, task),
                };
                // Tài khoản lạ thì TỪ CHỐI, đừng lặng lẽ rơi về mặc định: mở
                // phiên nhầm tài khoản là mở nhầm cả kho phiên.
                let known_accounts: Vec<String> = cfg
                    .claude_accounts_or_ambient()
                    .iter()
                    .map(|a| a.name.clone())
                    .collect();
                let bad_account = account
                    .as_ref()
                    .filter(|a| !known_accounts.contains(a))
                    .cloned();
                let known = known_projects(cfg);
                let dir = crate::config::project_dir(cfg, name);
                let ack = if let Some(a) = bad_account {
                    format!(
                        "⚠ không biết tài khoản '{}'. Đang có: {}",
                        crate::exec::truncate(&a, 24),
                        known_accounts.join(", ")
                    )
                } else {
                    match dir {
                    Some(d)
                        if known.contains(&name.to_string()) || cfg.projects.contains_key(name) =>
                    {
                        match crate::sessions::start_background(cfg, name, &d, task, account.as_deref()) {
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
                                // Câu chào phải mô tả thứ VỪA xảy ra.
                                //
                                // Bản cũ nói "phiên nền … tại <thư mục dự án>"
                                // và cả hai vế nay đều sai: từ 2026-08-11 hub
                                // mở một CỬA SỔ thật, và mở ở GỐC WORKSPACE
                                // (thư mục duy nhất cả ba tài khoản đã duyệt —
                                // dự án được nói trong đề bài). Người đọc câu
                                // ấy trên điện thoại sẽ đi tìm một cửa sổ ở chỗ
                                // không có, hoặc tìm một phiên nền không tồn
                                // tại. Nói sai chỗ còn tệ hơn không nói.
                                let cua_so = if s.window { "cửa sổ terminal" } else { "phiên nền" };
                                format!(
                                    "🚀 Đã mở {} cho {}.\nPhiên {} — đang chạy trên máy, xem màn sống ngay trên thẻ của nó.\n\n⚠ Nó chạy không hỏi ai. Tắt bằng nút Tắt hẳn hoặc /stop.",
                                    cua_so,
                                    s.project,
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
                    }
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
            }
            CommandKind::Stop => {
                let want = cmd.arg.trim().to_string();
                let want = if want.is_empty() {
                    db.cursor_or_log(FOCUS_SESSION_KEY).unwrap_or_default()
                } else {
                    want
                };
                // Đóng dấu `started_by_hub` TRƯỚC khi quyết định.
                //
                // `snapshot` chỉ đọc `claude agents`; dấu sở hữu nằm trong sổ
                // riêng của hub và do `mark_started_by_hub` dán vào. Thiếu bước
                // này thì mọi phiên đều "không phải của hub", và từ 2026-08-11
                // — khi `/new` mở cửa sổ thật — nó biến thành lỗi nhìn thấy
                // được: hub mở được cửa sổ rồi từ chối đóng chính nó, với câu
                // *"chỉ dừng được phiên do hub mở"*. Nhánh phiên nền không lộ
                // vì nó xét `kind`, không xét quyền sở hữu.
                let mut live = crate::sessions::snapshot(cfg);
                mark_started_by_hub(db, &mut live);
                let ack = match live.sessions.iter().find(|s| s.session_id == want) {
                    None if want.is_empty() => "⚠ chưa mở phiên nào.".to_string(),
                    None => format!(
                        "⚠ không thấy phiên '{}' đang chạy",
                        crate::exec::truncate(&want, 40)
                    ),
                    Some(s) => {
                        // Chốt chặn thứ hai, qua Telegram (Hà 2026-08-10). Dừng
                        // một phiên là thứ không lùi lại được, và cái nút gây ra
                        // nó nay nằm ngay trên danh sách — một chạm nhầm là mất
                        // tiến trình đang chạy dở.
                        let what = format!("Dừng phiên {} ({})?", s.name, s.account);
                        if let Some(refusal) =
                            ask_owner(db, cfg, adapter, cmd, &what, "dừng phiên nào")
                        {
                            refusal
                        } else {
                            match crate::sessions::stop_background(cfg, s) {
                                Ok(()) => {
                                    remember_stopped(db, s);
                                    logging::info(
                                        "session_stopped",
                                        json!({ "session": s.session_id }),
                                    );
                                    // Hai đường dừng, hai kết cục khác nhau —
                                    // nói đúng cái đã xảy ra, vì câu trả lời
                                    // quyết định người ta làm gì tiếp.
                                    if s.kind == "background" {
                                        format!(
                                            "⏹ Đã dừng phiên {}. Hội thoại vẫn còn — nói tiếp bằng /tell hoặc mở lại trên máy.",
                                            s.name
                                        )
                                    } else {
                                        format!(
                                            "⏹ Đã tắt hẳn phiên {} — thoát CLI và đóng cửa sổ terminal. Nhật ký vẫn còn trên máy.",
                                            s.name
                                        )
                                    }
                                }
                                Err(e) => format!(
                                    "⚠ không dừng được: {}",
                                    crate::exec::truncate(&e.to_string(), 200)
                                ),
                            }
                        }
                    }
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
            }
            CommandKind::Tell => {
                // Id đi CÙNG mệnh lệnh — xem `target_and_rest`.
                let (want, said) = target_and_rest(db, &cmd.arg);
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
                    Some(s) => match crate::sessions::tell(cfg, s, &said) {
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
            CommandKind::Type | CommandKind::Key | CommandKind::Shot => {
                // Gõ vào ĐÚNG cửa sổ của phiên đang theo. Không ghép được cửa
                // sổ thì TỪ CHỐI — gõ vào cửa sổ lạ là gõ vào việc của người
                // khác, và đó là hàng rào duy nhất còn lại ở đường này.
                let (want, typed) = target_and_rest(db, &cmd.arg);
                let live = crate::sessions::snapshot(cfg);
                let ack = match live.sessions.iter().find(|s| s.session_id == want) {
                    None if want.is_empty() => {
                        "⚠ chưa mở phiên nào. Chạm một phiên rồi gõ.".to_string()
                    }
                    None => format!(
                        "⚠ không thấy phiên '{}' trong danh sách",
                        crate::exec::truncate(&want, 40)
                    ),
                    Some(s) => match crate::keys::window_of(&s.tty) {
                        Ok(Some(w)) => {
                            // `/shot` đi đường riêng: nó không gõ gì, chỉ nhìn.
                            if matches!(cmd.kind, CommandKind::Shot) {
                                // Trả về CHỮ đang hiện, không phải ảnh. Hà
                                // 2026-08-10: ảnh chỉ nhìn được; cái cần là
                                // biết nó đang hỏi gì rồi bấm số trả lời — và
                                // hàng phím đã có sẵn.
                                match crate::keys::screen_text(w) {
                                    Ok(screen) => {
                                        // Chữ trên màn CŨNG phải qua cổng quét
                                        // rò rỉ: nó rời máy này y như phần xem
                                        // trước của phiên (điều 5).
                                        let risk = crate::sessions::preview_risk(&screen);
                                        if !risk.is_empty() {
                                            format!(
                                                "📷 Màn của {} có thể chứa bí mật ({}) — không đưa ra ngoài.",
                                                s.name,
                                                risk.join(", ")
                                            )
                                        } else {
                                            let choices = crate::keys::parse_choices(&screen);
                                            let tail: Vec<&str> = screen
                                                .lines()
                                                .filter(|l| !l.trim().is_empty())
                                                .rev()
                                                .take(14)
                                                .collect();
                                            let body: String = tail
                                                .into_iter()
                                                .rev()
                                                .collect::<Vec<_>>()
                                                .join("\n");
                                            if choices.is_empty() {
                                                format!("📷 Màn của {}:\n\n{}", s.name, body)
                                            } else {
                                                // Có hộp chọn thì nói THẲNG các
                                                // lựa chọn: đó là thứ người ta
                                                // mở lên để xem.
                                                let list: Vec<String> = choices
                                                    .iter()
                                                    .map(|(n, l)| format!("  {n}. {l}"))
                                                    .collect();
                                                format!(
                                                    "📷 {} đang hỏi — bấm số ở hàng phím để chọn:\n{}\n\n{}",
                                                    s.name,
                                                    list.join("\n"),
                                                    body
                                                )
                                            }
                                        }
                                    }
                                    Err(e) => format!(
                                        "⚠ không đọc được màn: {}",
                                        crate::exec::truncate(&e.to_string(), 300)
                                    ),
                                }
                            } else {
                            let is_key = matches!(cmd.kind, CommandKind::Key);
                            let arrow = matches!(
                                typed.trim(),
                                "up" | "down" | "left" | "right"
                            );
                            // `do script` LUÔN kèm một dấu xuống dòng, không tắt
                            // được — nên trên hộp chọn, một phím mũi tên vừa DI
                            // vừa CHỐT. Chốt nhầm một lựa chọn của người khác là
                            // thứ không lùi lại được, nên thà không gửi.
                            // Điều kiện để gửi mũi tên là **biết chắc KHÔNG có
                            // hộp chọn**, không phải "không thấy hộp chọn nào".
                            //
                            // Bản trước hỏi `screen_of(...).is_some_and(...)`, mà
                            // `screen_of` gộp cả ba kết cục vào `None` — không có
                            // cửa sổ, osascript hỏng, và **màn có dấu hiệu lộ bí
                            // mật**. Cả ba đọc thành "không có hộp chọn" ⟹ GỬI.
                            // Tức chốt hỏng về phía nguy hiểm, và hỏng nặng nhất
                            // đúng lúc màn đang hiện một mật khẩu.
                            let refusal = if is_key && arrow {
                                match crate::keys::arrow_verdict(&crate::keys::look(&s.tty, 24)) {
                                    crate::keys::Arrow::Send => None,
                                    crate::keys::Arrow::RefuseDialog => {
                                        logging::info(
                                            "keys_arrow_refused",
                                            json!({ "session": s.session_id, "key": typed.trim(),
                                                    "why": "dialog" }),
                                        );
                                        Some(format!(
                                            "⚠ {} đang có hộp chọn, nên tôi KHÔNG gửi mũi tên: đường gõ của \
                                             Terminal luôn kèm một dấu xuống dòng, tức mũi tên vừa di vừa CHỐT \
                                             — dễ chọn nhầm hộ Hà. Gõ thẳng SỐ của mục cần chọn thì an toàn.",
                                            s.name
                                        ))
                                    }
                                    crate::keys::Arrow::RefuseBlind(why) => {
                                        logging::warn(
                                            "keys_arrow_refused",
                                            json!({ "session": s.session_id, "key": typed.trim(),
                                                    "why": "blind", "detail": why }),
                                        );
                                        Some(format!(
                                            "⚠ Lúc này tôi KHÔNG đọc được màn của {} ({}), nên KHÔNG gửi mũi \
                                             tên. Không đọc được không có nghĩa là không có hộp chọn — mà nếu \
                                             đang có thì mũi tên vừa di vừa CHỐT, và chốt nhầm hộ Hà là thứ \
                                             không lùi lại được. Gõ thẳng SỐ của mục cần chọn thì an toàn dù \
                                             màn có đọc được hay không.",
                                            s.name, why
                                        ))
                                    }
                                }
                            } else {
                                None
                            };
                            if let Some(msg) = refusal {
                                msg
                            } else {
                            let res = if is_key {
                                crate::keys::press(w, typed.trim())
                            } else {
                                crate::keys::type_into(w, &typed, true)
                            };
                            match res {
                                Ok(()) => {
                                    // Nội dung KHÔNG vào log: nó là chữ của chủ
                                    // máy, còn log là tệp nằm lâu. Ghi đủ để
                                    // truy: phiên nào, cửa sổ nào, dài bao nhiêu.
                                    logging::info(
                                        "keys_typed",
                                        json!({ "session": s.session_id, "window": w,
                                                "kind": if is_key { "key" } else { "text" },
                                                "len": typed.trim().len() }),
                                    );
                                    // Soi lại màn rồi mới nói. Mã trả về 0 chỉ
                                    // chứng minh byte đã vào tab, KHÔNG chứng minh
                                    // `claude` làm gì với nó — chính chỗ này từng
                                    // báo "đã bấm" trong khi Hà không thấy gì.
                                    std::thread::sleep(std::time::Duration::from_millis(900));
                                    // Không đọc lại được màn thì nói là KHÔNG
                                    // BIẾT. Bản trước rơi về `Landed::Idle`, tức
                                    // trả lời "phiên đang đứng ở dấu nhắc" cho
                                    // một chuyện chưa hề nhìn thấy — cùng họ với
                                    // con bug chốt mũi tên ngay phía trên: đọc
                                    // "mù" thành một khẳng định.
                                    let what = match crate::keys::look(&s.tty, 24) {
                                        crate::keys::Look::Saw { body, .. } => {
                                            Some(crate::keys::landed(&body))
                                        }
                                        _ => None,
                                    };
                                    let did = if is_key {
                                        format!("đã bấm '{}'", typed.trim())
                                    } else {
                                        format!("đã gõ {} ký tự", typed.trim().len())
                                    };
                                    match what {
                                        Some(crate::keys::Landed::Queued) => format!(
                                            "⌨ {} vào {} — phiên đang chạy dở nên chữ nằm ở HÀNG CHỜ, \
                                             `claude` sẽ xử lý ngay khi xong việc.",
                                            did, s.name
                                        ),
                                        Some(crate::keys::Landed::Running) => {
                                            format!("⌨ {} vào {} — phiên đã nhận và bắt đầu chạy.", did, s.name)
                                        }
                                        Some(crate::keys::Landed::Idle) => {
                                            format!("⌨ {} vào {} — phiên đang đứng ở dấu nhắc.", did, s.name)
                                        }
                                        // Byte đã vào tab (mã trả về 0), nhưng
                                        // đọc lại màn thì không được — nói đúng
                                        // chừng ấy, đừng đoán hộ.
                                        None => format!(
                                            "⌨ {} vào {} — nhưng tôi KHÔNG đọc lại được màn, nên chưa \
                                             biết chữ đã rơi vào dấu nhắc hay vào hàng chờ.",
                                            did, s.name
                                        ),
                                    }
                                }
                                Err(e) => format!(
                                    "⚠ không gõ được: {}",
                                    crate::exec::truncate(&e.to_string(), 300)
                                ),
                            }
                            }
                            }
                        }
                        // Phiên nền không có cửa sổ nào — nói đúng lý do thay vì
                        // một lời từ chối chung chung.
                        Ok(None) => format!(
                            "⚠ {} không có cửa sổ terminal để gõ (host: {}). Chỉ phiên mở trong Terminal mới gõ được.",
                            s.name, s.host
                        ),
                        Err(e) => format!(
                            "⚠ không tìm được cửa sổ: {}",
                            crate::exec::truncate(&e.to_string(), 200)
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
                let (want, asked) = target_and_rest(db, &cmd.arg);
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
                    Some(s) => match crate::sessions::ask_aside(cfg, s, &asked) {
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
                            // Nói rõ ĐI ĐƯỜNG NÀO, vì hai đường khác nhau ở
                            // đúng chỗ người hỏi cần biết: phiên gốc có bị thêm
                            // một lượt hay không.
                            //
                            // `new_session_id == source_id` ⟹ hỏi thẳng phiên
                            // sống bằng `/btw` (như ngồi trước terminal): rẻ,
                            // sát việc, nhưng phiên gốc CÓ thêm lượt.
                            // Khác nhau ⟹ fork: phiên gốc y nguyên byte.
                            // 📌 Câu này từng nói "phiên gốc CÓ thêm một lượt"
                            // — sai, đo 2026-08-11: `/btw` mở một bảng bên
                            // trong TUI và KHÔNG ghi byte nào vào nhật ký.
                            // Cái nó thật sự ăn là NGỮ CẢNH đang chạy của
                            // phiên, thứ không nhìn thấy trên đĩa.
                            let how = if a.new_session_id == a.source_id {
                                "hỏi thẳng vào phiên bằng /btw — nhật ký không dài thêm, nhưng câu hỏi ăn vào ngữ cảnh đang chạy"
                            } else {
                                "hỏi trên bản sao — phiên gốc không bị đụng"
                            };
                            format!(
                                "💬 Hỏi bên lề phiên {} ({how}):\n\n{}",
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
                    match db.cursor_or_log(FOCUS_SESSION_KEY) {
                        Some(id) if !id.is_empty() => format!("👁 Đang theo phiên {id}"),
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
/// Hỏi chủ máy qua Telegram trước khi làm một việc đắt hoặc không lùi lại được.
///
/// Trả `None` khi được phép đi tiếp, `Some(câu từ chối)` khi không — hình dạng
/// ấy khiến chỗ gọi không thể "quên" nhánh từ chối: nó phải trả lời một cái gì.
///
/// Trả lời trong phòng chat TRƯỚC khi đứng chờ, vì `confirm::ask` đứng tới 90
/// giây và một cái màn im 90 giây là một cái màn hỏng.
fn ask_owner(
    db: &Db,
    cfg: &Config,
    adapter: &str,
    cmd: &ChannelCommand,
    what: &str,
    nothing_done: &str,
) -> Option<String> {
    if cfg.confirm.enabled {
        reply_in_channel(
            db,
            cfg,
            adapter,
            cmd,
            &format!("🔒 Đã gửi yêu cầu xác nhận sang Telegram: {what} Chưa làm gì cho tới khi bấm nút."),
        );
    }
    let verdict = crate::confirm::ask(cfg, what);
    if verdict.allows() {
        None
    } else {
        Some(verdict.refusal(nothing_done))
    }
}

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
    auto_handover(db, cfg);
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
