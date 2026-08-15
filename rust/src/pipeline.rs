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

use crate::adapters::{ChannelCommand, CommandKind};
use crate::config::Config;
use crate::db::{Db, RunFinish};
use crate::logging;
use crate::verbs;

#[derive(Debug, Serialize)]
pub struct CycleSummary {
    pub ms: u128,
}

/// Tách cờ `-x <giá trị>` ra khỏi phần chữ còn lại của một lệnh.
///
/// Hà 2026-08-12: *"kiến trúc lại lệnh cho hợp lý, ví dụ: `/new -a acc2 -s
/// dwork`"*. Lối gõ cũ là VỊ TRÍ (`/new <dự án> @acc <việc>`) — thứ tự phải
/// thuộc lòng, và cái `@acc` phải nằm đúng khe thứ hai nếu không nó thành một
/// phần của đề bài. Cờ thì gõ đâu cũng được và tự nói nó là gì.
///
/// **Chỉ cờ ĐÃ BIẾT mới bị bóc.** Một `-p` lạ trong đề bài (`sửa cờ -p của
/// script`) phải ở nguyên trong chữ: nuốt im lặng một mẩu đề bài là đúng cái
/// loại lỗi không ai truy ra được, vì phiên vẫn mở và vẫn chạy — chỉ là chạy
/// một đề bài khác với đề bài đã gõ.
///
/// Cờ ở CUỐI mà không có giá trị cũng trả về (giá trị rỗng), để chỗ gọi nói
/// được "thiếu giá trị cho -a" thay vì lặng lẽ bỏ qua.
pub fn split_flags(
    arg: &str,
    known: &[&str],
) -> (std::collections::BTreeMap<String, String>, String) {
    let mut flags = std::collections::BTreeMap::new();
    let mut rest: Vec<&str> = Vec::new();
    let mut it = arg.split_whitespace().peekable();
    while let Some(tok) = it.next() {
        let name = tok.trim_start_matches('-');
        let is_flag = tok.starts_with('-') && !name.is_empty() && known.contains(&name);
        if !is_flag {
            rest.push(tok);
            continue;
        }
        // Giá trị là token kế tiếp — trừ khi token ấy lại là một cờ đã biết,
        // nghĩa là cờ này bị bỏ trống (`/new -a -s dwork`).
        let takes = it
            .peek()
            .map(|n| {
                let nn = n.trim_start_matches('-');
                !(n.starts_with('-') && known.contains(&nn))
            })
            .unwrap_or(false);
        let val = if takes { it.next().unwrap_or("") } else { "" };
        flags.insert(name.to_string(), val.to_string());
    }
    (flags, rest.join(" "))
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
            if crate::config::looks_like_project(&dir) {
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
    let full_uuid = head.len() >= 32
        && head.matches('-').count() == 4
        && head.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    // Id NGẮN (8 ký tự hex) cũng là một cái tên phiên thật: hub in nó khắp nơi
    // (`f7612183`), và một lệnh tự tô sáng thì BẮT BUỘC phải dùng nó — tên lệnh
    // chỉ được 32 ký tự, một uuid đầy đủ đã 36. Hẹp có chủ ý: đúng 8, toàn hex,
    // và phải có chữ đi sau — `/type deadbeef` trống thì vẫn là chữ gõ vào
    // phiên, không phải một lệnh nhắm vào phiên `deadbeef`.
    let short_id =
        head.len() == 8 && head.chars().all(|c| c.is_ascii_hexdigit()) && !rest.is_empty();
    (full_uuid || short_id).then(|| (head.to_string(), rest.to_string()))
}

/// Hai chuỗi này có chỉ vào CÙNG một phiên không?
///
/// `want` được phép là **8 ký tự đầu** của id — dạng hub in ra khắp nơi, và là
/// dạng DUY NHẤT một lệnh tự tô sáng dùng được (tên lệnh tối đa 32 ký tự, một
/// uuid đã 36). Khớp tiền tố chỉ nhận đúng độ dài ấy: nửa vời hơn thì hai phiên
/// khác nhau có thể cùng khớp, và gõ vào nhầm phiên là thứ không lùi lại được.
pub fn same_session(id: &str, want: &str) -> bool {
    !want.is_empty() && (id == want || (want.len() == 8 && id.starts_with(want)))
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
/// Nút "👁 Vào phiên" trỏ vào đâu — hoặc KHÔNG có nút nào.
///
/// 🔴 Hà 2026-08-12, đọc đúng tin `⏹ hub-67 (033059d8) đã tắt — cửa sổ ấy nay
/// đang chạy phiên hub-ec.` kèm một cái nút: *"tại sao 1 phiên đã tắt mà vẫn
/// gắn nút vào phiên để làm gì?"* và *"hình như phiên nào bạn cũng mặc định gắn
/// nút vào phiên"*. Đúng cả hai. Luật cũ chỉ có MỘT điều kiện — `id != focused`
/// — tức nó hỏi *"có phải phiên đang theo không"* mà không bao giờ hỏi
/// *"phiên còn sống không"*. Nên tin BÁO TỬ cũng mọc nút, và bấm vào là đi tới
/// một phiên không còn tồn tại: `/session` nhận id, đặt con trỏ, rồi mọi
/// `/shot` · `/type` · `/key` sau đó đều nói vào chỗ trống.
///
/// Luật đúng: nút chỉ tồn tại khi có một phiên **SỐNG** để vào.
/// - phiên vừa xong / đang hỏi ⟹ chính nó;
/// - phiên đã tắt ⟹ **không có gì để vào**, TRỪ khi cửa sổ của nó đã bị một
///   phiên khác chiếm — lúc ấy nút trỏ vào **phiên đang ngồi ở đó**, và nhãn
///   phải mang tên phiên MỚI, vì một cái nút gọi tên người chết là một cái nút
///   nói dối.
pub fn enter_button(
    c: &crate::watch::Change,
    id: &str,
    takeover: Option<(&str, &str)>,
    focused: &str,
) -> Option<(String, String)> {
    let (target, name) = match c {
        crate::watch::Change::Ended { .. } => takeover?,
        _ => (id, c.name()),
    };
    if target == focused {
        return None;
    }
    Some((
        format!("👁 Vào phiên {}", crate::exec::truncate(name, 24)),
        format!("sess:{target}"),
    ))
}

/// Tên + tài khoản của một phiên, lấy từ SỔ — **0 tiến trình, 0 chờ**.
///
/// 🔴 Hà 2026-08-12: *"bấm vào phiên vẫn phản hồi rất chậm, sao không chỉnh để
/// nhận được luôn"*. Đo cú bấm ấy: `command_done kind=Session` **ms=48407**.
/// Hàng chờ không liên quan (đã vá 18:29); 48 giây nằm gọn trong chính lệnh, và
/// nó đi vào đúng một dòng: `snapshot_cached(20s)` — dựng lại ảnh chụp phiên
/// **chỉ để lấy `s.name` và `s.account`** cho câu chào. Đệm 20 giây từng đủ khi
/// một lượt dựng mất ~10 giây; tối nay `sessions_snapshot_ms` đo được **18–92
/// giây** mỗi vòng, nên gần như cú bấm nào cũng rơi đúng vào lượt dựng lại.
///
/// Mà hai thứ ấy hub đã nhớ sẵn: `Mark::n` (tên) và `Mark::a` (tài khoản), ghi
/// mỗi vòng chính vì lúc phiên biến mất thì không còn chỗ nào hỏi nữa. Đọc sổ
/// là một lượt đọc SQLite.
///
/// Cái giá phải nói thẳng: sổ cũ hơn ảnh chụp đúng **một vòng**. Với câu "đang
/// theo phiên nào" thì đó là cái giá đúng — một cái tên trễ một vòng vẫn là cái
/// tên ấy, còn 48 giây im lặng thì người ta bấm lần hai.
pub fn session_name_from_book(book_json: &str, id: &str) -> Option<(String, String)> {
    let book: BTreeMap<String, crate::watch::Mark> = serde_json::from_str(book_json).ok()?;
    let mark = book.get(id)?;
    // Tên để ĐỌC, không phải tên `claude` tự đặt. Sổ nhớ luôn cái nhãn ĐÃ
    // TÍNH (`Mark::l`) chứ không tính lại từ `n`+`d`: nhãn duy nhất là tính
    // chất của cả tập, mà ở đây chỉ còn một hàng trong sổ. Sổ cũ chưa có `l`
    // thì rơi về cách tính cũ — đúng dự án, chỉ thiếu vế duy-nhất.
    let shown = if mark.l.is_empty() {
        crate::sessions::display_name(&mark.n, &mark.d)
    } else {
        mark.l.clone()
    };
    if mark.n.is_empty() {
        // Sổ có id mà không có tên (bản ghi từ trước khi sổ nhớ tên) — trả None
        // để rơi về đường ảnh chụp, đừng chào bằng một cái tên rỗng.
        return None;
    }
    Some((shown, mark.a.clone()))
}

/// Xoá hòm thư của một phiên đã kết thúc — xem chỗ gọi trong `announce_changes`.
fn clean_inbox(cfg: &Config, session_id: &str, folder: &str) {
    let short = session_id.split('-').next().unwrap_or("");
    // Chỉ nhận id ngắn dạng hex: đường dẫn xoá KHÔNG được nhận một chuỗi tuỳ ý.
    if short.len() < 6 || !short.chars().all(|c| c.is_ascii_hexdigit()) {
        return;
    }
    // Hòm thư nay ở GỐC workspace (Hà 2026-08-13). Vẫn dọn cả đường CŨ
    // `<dự án>/.inbox/<id>` — tệp nhận trước lúc đổi vẫn phải có người dọn,
    // không thì đúng cái rác mà việc chia theo phiên sinh ra để tránh.
    let mut dirs = vec![cfg.workspace_root.join(".inbox").join(short)];
    if !folder.is_empty() {
        dirs.push(cfg.workspace_root.join(folder).join(".inbox").join(short));
    }
    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        let n = std::fs::read_dir(&dir)
            .map(|d| d.flatten().count())
            .unwrap_or(0);
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => logging::info(
                "inbox_cleaned",
                json!({ "session": session_id, "files": n, "dir": dir.display().to_string() }),
            ),
            Err(e) => logging::warn(
                "inbox_clean_failed",
                json!({ "dir": dir.display().to_string(), "err": e.to_string() }),
            ),
        }
    }
}

pub fn announce_changes(db: &Db, cfg: &Config, snap: &crate::sessions::SessionsSnapshot) {
    let live = &snap.sessions;
    let prev: BTreeMap<String, crate::watch::Mark> = db
        .cursor_or_log(WATCH_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    // Tài khoản nào KHÔNG liệt kê được phiên ở lượt này đi thẳng vào phép so:
    // vắng mặt trong một danh sách hỏng không phải là một cái chết. Trước
    // 2026-08-12 hàm này chỉ nhận `sessions`, nên `notes` — chỗ duy nhất ghi
    // chuyện tài khoản hỏng — không tới được đây, và ba phiên còn sống bị báo
    // tắt trong 8 giây.
    let (changes, next) =
        crate::watch::changes(&prev, live, chrono::Utc::now().timestamp(), &snap.blind);
    // Phiên đang theo — để biết tin nào cần kèm nút "vào phiên".
    let focused = db.cursor_or_log(FOCUS_SESSION_KEY).unwrap_or_default();
    // Phiên do CHÍNH hub đóng sổ thì cái chết của nó KHÔNG phải tin.
    //
    // 🔴 Hà 2026-08-13, đọc đúng tin ấy: *"sao lại có thông báo này: ⏹
    // projects-fb · AI/hub (76534706) đã tắt hẳn — nó đang chạy dở, nên xem
    // lại"*. Log cùng lúc: `auto_handover_firing` 00:09:15 →
    // `handover_window_opened` 00:09:49 → tin báo tử 00:10:07. Tức hub vừa cố ý
    // đóng cửa sổ ấy xong (cách A, dựng đêm nay), rồi cái loa nhìn thấy phiên
    // biến mất và **báo động như một cái chết bất thường** — còn thêm câu "đang
    // chạy dở, nên xem lại", vì lúc bị đóng nó đang giữa lượt viết bản bàn giao.
    //
    // Một hệ thống tự làm gì đó rồi tự giật mình vì chính việc mình vừa làm là
    // hệ thống chưa nối hai đầu với nhau. Sổ `AUTO_DONE_KEY` đã có sẵn tên
    // những phiên ấy — chỉ là chưa ai hỏi nó ở đây.
    let handed_over: Vec<String> = db
        .cursor_or_log(AUTO_DONE_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    // …và phiên hub ĐANG ĐÓNG theo lệnh `/close` cũng vậy — cùng một lý do,
    // một cuốn sổ khác.
    //
    // 🔴 Hà 2026-08-13, đếm tin sau đúng MỘT cú `/close`: *"Đóng 1 phiên mà lắm
    // thông báo thế"*. Trên ảnh có `⏳ Đã gõ /exit … chờ CLI chạy nốt`, rồi
    // `⚫ [mailler] đã tắt (thoát CLI, cửa sổ terminal còn mở)`, rồi `⏹ Đã đóng
    // hẳn [mailler] … (chờ 24s)`. Tin giữa là cái loa nhìn thấy phiên biến mất
    // và báo động — về đúng việc hub vừa cố ý làm, ba mươi giây trước. Nó còn
    // mâu thuẫn với tin sau nó ("cửa sổ còn mở" rồi "cửa sổ đã đóng"), nên
    // người đọc phải tự ghép hai câu mới ra một sự thật.
    let closing: Vec<String> = db
        .cursor_or_log(CLOSING_KEY)
        .and_then(|v| serde_json::from_str::<BTreeMap<String, Closing>>(&v).ok())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();

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
            crate::watch::Change::Failed { id, .. } => id.clone(),
            crate::watch::Change::Finished { id, .. } => id.clone(),
            crate::watch::Change::Asking { id, .. } => id.clone(),
            crate::watch::Change::Ended { id, .. } => id.clone(),
        };
        let row = live.iter().find(|s| s.session_id == id);

        // Phiên kết thúc ⟹ dọn hòm thư của nó.
        //
        // Hà 2026-08-13: *"`.inbox` nên đưa vào theo mã phiên cho dễ dọn rác"* ·
        // *"vì hết phiên nó ko cần nữa"*. Tệp gửi vào một phiên chỉ có nghĩa
        // trong đời phiên ấy; để lại thì sau một tuần không ai biết cái nào còn
        // dùng, mà đó đúng là cách một thư mục rác hình thành.
        //
        // Xoá HẸP và NÓI RA: chỉ đúng thư mục `<dự án>/.inbox/<id ngắn>` do
        // chính hub tạo, và log kèm số tệp — xoá tệp của người khác mà im lặng
        // là thứ không ai tha thứ lần thứ hai.
        if matches!(c, crate::watch::Change::Ended { .. }) {
            clean_inbox(cfg, &id, row.map(|r| r.folder.as_str()).unwrap_or(""));
        }
        // hub vừa tự đóng sổ phiên này ⟹ cái chết của nó là KẾ HOẠCH, không
        // phải tin. Xem `handed_over` ở đầu hàm.
        if matches!(c, crate::watch::Change::Ended { .. })
            && (handed_over.iter().any(|d| d == &id) || closing.iter().any(|d| d == &id))
        {
            logging::info(
                "session_end_muted",
                json!({ "session": id,
                        "why": "hub vừa tự đóng sổ phiên này — cái chết của nó là kế hoạch" }),
            );
            continue;
        }

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
                            // Đọc từ MÀN thì không biết được câu ấy cho chọn
                            // mấy cái — `multiSelect` chỉ có trong nhật ký. Nói
                            // "chọn một" khi không biết là đoán; để `false` ở
                            // đây nghĩa là KHÔNG hứa gì thêm.
                            multi: false,
                        }
                    }
                    // LỖI API đứng TRƯỚC "đang ở dấu nhắc": hai thứ nhìn giống
                    // nhau (nhật ký thôi lớn lên, màn đứng im) mà việc phải làm
                    // ngược nhau — xem `keys::api_error`.
                    crate::keys::Look::Saw { body, .. }
                        if crate::keys::api_error(&body).is_some() =>
                    {
                        crate::watch::Idle::Failed {
                            line: crate::keys::api_error(&body).unwrap_or_default(),
                        }
                    }
                    crate::keys::Look::Saw { .. } => crate::watch::Idle::Prompt,
                    crate::keys::Look::Withheld { choices, .. } if choices > 0 => {
                        // Chỉ con số — con số không mang chữ nào ra khỏi máy.
                        crate::watch::Idle::Asking {
                            n: choices,
                            options: vec![],
                            multi: false,
                        }
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
        // Phiên đang GIỮ cửa sổ của phiên vừa tắt, nếu có. Nó là thứ duy nhất
        // còn "vào" được khi tin nói về một phiên đã chết — xem `enter_button`.
        let mut takeover: Option<(String, String)> = None;
        let fate = if let crate::watch::Change::Ended { tty, kind, .. } = &c {
            // Phiên nền không có cửa sổ nào để đóng, nên dừng nó LÀ tắt hẳn.
            // `??` (không có tty điều khiển) cũng là "không cửa sổ" — xem
            // `sessions::is_real_tty`; đọc `??` như một cửa sổ có thật là cách
            // hub từng nói "cửa sổ ấy nay đang chạy phiên khác" về hai phiên
            // chưa bao giờ có cửa sổ nào.
            if kind == "background" || !crate::sessions::is_real_tty(tty) {
                Some("đã tắt hẳn".to_string())
            } else if let Some(other) = crate::sessions::window_taken_over(&id, tty, live) {
                takeover = Some((other.session_id.clone(), other.name.clone()));
                // Cửa sổ ấy CÒN, nhưng nó không còn là cửa sổ của phiên này —
                // xem `sessions::window_taken_over`. Nói tên phiên đang ngồi ở
                // đó: đấy là thứ người cầm điện thoại cần để khỏi đi tìm một
                // cửa sổ bỏ không không tồn tại.
                Some(format!(
                    "đã tắt — cửa sổ ấy nay đang chạy phiên {}",
                    other.name
                ))
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

        // MỌI phiên terminal dừng lại chờ đều được báo (Hà 2026-08-12).
        //
        // Luật cũ (08-10) im cho phiên terminal của chủ máy trừ khi nó KẸT HỎI,
        // với lý do *"anh đang nhìn thẳng vào nó"* — một phiên bắn ba tin trong
        // mười sáu phút. Lý do ấy hết đúng từ lúc anh làm việc **qua điện
        // thoại**: đang theo một phiên từ Telegram nghĩa là KHÔNG ngồi trước
        // cửa sổ nào cả.
        //
        // 🔴 Đo được chính hôm nay, và đây là thứ làm Hà hỏi: phiên `e27806c2`
        // bị im **ba lần** (16:57:47 · 17:53:35 · 17:58:16) đúng những lúc nó
        // dừng lại chờ anh. Thêm một khe mù nữa: hub chỉ NHÌN mỗi ~139 giây
        // (đo 15 vòng, thấp nhất 49s, cao nhất 161s), nên một hộp chọn sống 40
        // giây thì lọt trọn giữa hai lượt nhìn — nhịp ấy Hà để bàn riêng.
        //
        // Cái chặn ồn còn lại là `watch::MIN_RUN_SEC` (120s): một lượt chạy
        // chớp nhoáng vẫn không phải tin. Nhánh KẸT HỎI thì không đi qua cửa ấy
        // — hỏi là hỏi, dài ngắn không đổi.

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
        if let crate::watch::Change::Ended {
            parent,
            was_working,
            ..
        } = &c
        {
            if !parent.is_empty() && !was_working {
                logging::info(
                    "session_change_muted",
                    json!({ "session": id, "parent": parent, "why": "phiên con kết thúc bình thường" }),
                );
                continue;
            }
        }

        // Câu cuối phiên nói ra — thứ làm mỗi tin KHÁC nhau.
        //
        // Đọc BẢN DÀI của lượt cuối rồi rút thông tin chốt (Hà 2026-08-12:
        // *"khi phiên dừng chờ thì cần hiện các thông tin chốt quan trọng để đọc
        // trên tele"*). `last_text` trong ảnh chụp chỉ có 240 ký tự — và 240 ký
        // tự đầu của một báo cáo thường là câu dẫn nhập, tức đúng phần không
        // quyết định được gì. Đọc thêm một lần ở đây thì rẻ: chuyện này hiếm.
        // Không đọc được (hoặc chữ có dấu hiệu bí mật) thì rơi về bản ngắn đã
        // qua cổng quét trong ảnh chụp.
        let long = row.and_then(|s| crate::sessions::last_say(cfg, s, crate::sessions::SAY_MAX));
        let points = long
            .as_deref()
            .map(|t| crate::watch::key_points(t, 700))
            .filter(|p| !p.trim().is_empty());
        let tail = points
            .as_deref()
            .or_else(|| row.and_then(|s| s.last_text.as_deref()));
        let text = match (&c, &fate) {
            (
                crate::watch::Change::Ended {
                    name, was_working, ..
                },
                Some(f),
            ) => {
                // Tắt lúc đang chạy dở là chuyện ĐÁNG XEM LẠI — đó là lần duy
                // nhất một tin "đã tắt" đòi người ta làm gì.
                let warn = if *was_working {
                    " — nó đang chạy dở, nên xem lại"
                } else {
                    ""
                };
                // Chấm TRẠNG THÁI, cùng bộ với danh sách — `⏹` là nút "dừng"
                // của máy phát nhạc, và Hà đã bắt đúng chỗ lẫn ấy 2026-08-13.
                format!("⚫ {name} {f}{warn}.")
            }
            _ => c.say(&idle, tail),
        };
        // Phiên vừa rời danh sách: giữ lại đủ dữ kiện để CÒN HỎI ĐƯỢC về nó
        // (xem `ENDED_KEY`). Ghi trước khi nói, vì sau lượt này cuốn sổ theo dõi
        // đã bỏ nó rồi — không còn chỗ nào lấy `cwd` với tài khoản nữa.
        if matches!(c, crate::watch::Change::Ended { .. }) {
            if let Some(m) = prev.get(&id) {
                remember_ended(db, &id, m, chrono::Utc::now().timestamp());
            }
        }
        logging::info("session_change", json!({ "text": text }));
        // (Nhánh gửi vào phòng chat tfl5 đã bỏ 2026-08-14 — xem `verbs.rs`.
        // Telegram nay là cái mồm duy nhất, và nó nằm ngay dưới đây.)
        // Phiên đang DỪNG LẠI HỎI thì tin nhắn phải BẤM ĐƯỢC.
        //
        // Hà 2026-08-11: *"lựa chọn vừa rồi không thể hiện được trên tele để
        // chọn à"* + *"cần thêm thông tin mô tả liên quan tới lựa chọn đó mới
        // hợp lý"*. Trước đó tin chỉ nói "có N lựa chọn": người đọc biết mình
        // bị chặn, mà vẫn phải mở máy ra mới gỡ được — tức cái chuông báo đúng
        // nhưng không tiết kiệm cho ai một bước nào.
        //
        // Nút gửi `/key <session_id> <n>` — đi đúng con đường của trang, không
        // đẻ thêm một lối riêng cho Telegram.
        // Lựa chọn lấy từ NHẬT KÝ trước (đầy đủ, có cả với phiên hub không đọc
        // được màn), rồi mới tới thứ đọc được trên màn.
        // Kèm luôn cờ CHỌN NHIỀU: nút và câu chữ phải khai đúng bản chất câu
        // hỏi, không thì bấm một cái rồi ngồi chờ một việc không xảy ra.
        // `rest` đi kèm: bảng nhiều câu phải có nút cho TỪNG câu, không thì
        // điện thoại trả lời được câu đầu rồi đứng — xem `sessions::Asking`.
        const NO_REST: &[crate::sessions::Question] = &[];
        let buttons = match (&c, &idle) {
            (
                crate::watch::Change::Asking {
                    options,
                    multi,
                    rest,
                    ..
                },
                _,
            ) if !options.is_empty() => Some((options, *multi, rest.as_slice())),
            (_, crate::watch::Idle::Asking { options, multi, .. }) if !options.is_empty() => {
                // Nhánh này đọc từ MÀN (`keys::parse_choices`), mà màn chỉ vẽ
                // câu đang mở — nên nó không biết bảng có mấy câu. Không bịa.
                Some((options, *multi, NO_REST))
            }
            _ => None,
        };
        // Tin của một phiên KHÁC phiên đang theo phải mang theo đường vào nó.
        //
        // Hà 2026-08-12: *"nếu báo phiên khác phiên đang theo thì thêm nút vào
        // phiên"*. Không có nút thì tin báo bắt người đọc tự gõ `/session
        // <uuid>` trên điện thoại — đúng loại việc làm người ta bỏ tính năng.
        // Nút gửi `sess:<id>`, tức đi đúng route `/session` sẵn có
        // (`telegram::callback_to_command`), không đẻ thêm lối riêng.
        //
        // Phiên ĐANG theo thì không cần nút: bấm vào chỉ để tới chỗ đang đứng.
        let enter = enter_button(
            &c,
            &id,
            takeover.as_ref().map(|(i, n)| (i.as_str(), n.as_str())),
            &focused,
        );
        // Lệnh nằm TRONG CHÍNH TIN BÁO cũng phải bấm chạy được.
        //
        // 🔴 Hà 2026-08-12: *"nội dung của phiên có lệnh script cần chạy đã có
        // tính năng bấm chạy luôn chưa"*. Có — nhưng từ 08-12 tối tới giờ nó
        // chỉ gắn vào câu trả lời của `/shot` (`cmd.kind == Shot`), tức chủ máy
        // phải CHỦ ĐỘNG đi xin ảnh màn rồi mới thấy nút. Mà chỗ lệnh hay xuất
        // hiện nhất lại là tin tự phát: phiên dừng lại và câu chốt của nó là
        // một dòng lệnh để gõ. Cây cầu đi được một chiều thì vẫn là chưa nối.
        //
        // Cùng máy móc, không đẻ lối riêng: `commands_on_screen` (nhận theo
        // HÌNH DẠNG, cố ý hẹp) → `remember_quick` → nút `run:<n>` → `/type
        // <lệnh>` vào chính phiên (KHÔNG còn dấu `!` — xem `telegram.rs`).
        //
        // ⛔ Trừ tin BÁO TỬ: gõ vào một phiên đã tắt là gõ vào chỗ trống — cùng
        // lý do nút "vào phiên" đã bị gỡ khỏi nhánh ấy.
        // Nút dựng từ BẢN DÀI, không từ bản đã rút gọn.
        //
        // 🔴 Hà 2026-08-13: *"ở [dwork] đang có lệnh và cũng không hiển thị nút
        // chạy"*. Ảnh chụp cho thấy ba dòng `bash ./dci-deploy-be.sh …` nằm
        // trong một khối ```, và `watch::key_points` **cố ý bỏ khối code** khi
        // rút gọn (có test riêng cho việc ấy). Hai luật đều đúng một mình: bỏ
        // khối code cho tin dễ đọc, và dựng nút từ chữ của tin. Ghép lại thì
        // đúng những dòng ĐÁNG BẤM NHẤT — lệnh người ta viết trong khối code —
        // là những dòng duy nhất không bao giờ tới được chỗ nhận diện.
        //
        // `long` đã qua cổng quét rò ở `last_say` (có dấu hiệu bí mật thì nó
        // trả `None`), nên đọc nó ở đây không nới rào nào.
        let scan = long.as_deref().unwrap_or(&text);
        let cmds = if matches!(c, crate::watch::Change::Ended { .. }) {
            Vec::new()
        } else {
            // BÁO CÁO, không phải màn: `scan` là `long` — chữ đọc thẳng từ
            // nhật ký phiên, chưa qua bề ngang cửa sổ nào. Xem
            // `keys::BTN_CMD_REPORT_MAX`.
            // 🔴 HAI, không phải ba. Hà 2026-08-14, ảnh chụp một tin mang ba
            // nút lệnh: *"sao lắm nút lệnh thế"*. Một báo cáo dài nhắc tới
            // nhiều lệnh, nhưng thứ chủ máy cần bấm ngay thì gần như luôn là
            // câu chốt — những cái còn lại chỉ là chữ trong lời kể. Ba nút gần
            // giống nhau không cho thêm lựa chọn nào, chúng bắt người đọc dừng
            // lại đoán xem cái nào mới đúng.
            crate::keys::commands_in_report(scan, 2)
        };
        let mut quick = remember_quick(db, &id, &cmds);
        // …và file được NHẮC TỚI thì phải MỞ ĐƯỢC. Một báo cáo nói "xem
        // ARCHITECTURE.md" trên điện thoại là nói tới thứ không mở nổi, trừ khi
        // có cái nút. Tin BÁO TỬ thì thôi, cùng lý do với nút lệnh: phiên đã
        // tắt, nhưng ở đây lý do khác — file thì vẫn còn, chỉ là một tin báo tử
        // không phải chỗ để đọc tài liệu.
        if !matches!(c, crate::watch::Change::Ended { .. }) {
            quick.extend(remember_files(
                db,
                cfg,
                &id,
                &crate::keys::paths_on_screen(scan, 4),
            ));
        }
        // "… (còn N dòng)" phải có đường đi tiếp — xem `remember_full`.
        if text.contains("… (còn ") {
            let shown_name = row
                .map(crate::sessions::shown)
                .unwrap_or_else(|| c.name().to_string());
            if let Some(b) = long
                .as_deref()
                .and_then(|full| remember_full(db, &id, &shown_name, full))
            {
                quick.push(b);
            }
        }
        match (buttons, crate::telegram::inbox()) {
            (Some((opts, multi, rest)), Some(tg)) => {
                if let Err(e) = tg.ask_choices(&text, &id, opts, enter.is_some(), multi, rest) {
                    logging::error("session_change_telegram_failed", json!({ "err": e }));
                }
            }
            (None, Some(tg)) if enter.is_some() || !quick.is_empty() => {
                // 🔴 Hà 2026-08-14: *"nút chạy lệnh chỉ cần 1 icon là đủ chèn
                // ngay sau câu lệnh"* · *"Chèn ngay sau câu lệnh chứ không phải
                // 1 nút ở cuối"*. Cắt tin ngay sau dòng lệnh thì bàn phím rơi
                // đúng chỗ ấy — xem `command_slices`.
                //
                // Nhãn rút còn ICON, và chỉ ở đường này: dòng lệnh đang nằm
                // ngay trên đầu nút, NGUYÊN VĂN, nên nhắc lại nó trong nhãn
                // vừa thừa vừa bị cắt còn 52 ký tự. Đường không tách được thì
                // nhãn vẫn phải mang dòng lệnh, vì lúc ấy chẳng có chữ nào
                // quanh cái nút nói nó sắp chạy gì.
                let mut b: Vec<(String, String)> = Vec::new();
                b.extend(enter);
                b.extend(quick.clone());
                say_with_command_icons(tg, &text, &cmds, &b, "session_change_telegram_failed");
            }
            _ => {
                if let Err(e) = crate::confirm::tell(cfg, &text) {
                    logging::error("session_change_telegram_failed", json!({ "err": e }));
                }
            }
        }
    }
}

/// Trần cho phần chữ một lệnh trả về. Telegram cắt tin ở 4096 ký tự, và một tin
/// bị Telegram cắt là một tin mất đúng phần cuối — thường là phần kết luận.
pub const CMD_OUT_MAX: usize = 3000;

/// Dựng câu trả lời cho `/cmd` — hàm THUẦN, kiểm được không cần chạy lệnh nào.
///
/// Ba điều nó phải nói, và cả ba đều từng là chỗ người ta đoán mò:
/// * **Mã thoát**, luôn luôn — `exit 1` mà im lặng thì một lệnh hỏng đọc lên y
///   hệt một lệnh chạy xong.
/// * **Không in ra gì** khác **chưa chạy được**: nói thẳng câu đầu.
/// * **Bị cắt thì nói là bị cắt**, kèm số ký tự còn lại.
pub fn cmd_report(code: Option<i32>, timed_out: bool, out: &str, err: &str, ms: u128) -> String {
    if timed_out {
        return format!(
            "⏱ quá giờ sau {:.1}s — đã giết cả nhóm tiến trình.",
            ms as f64 / 1000.0
        );
    }
    let mut body = String::new();
    if !out.trim().is_empty() {
        body.push_str(out.trim_end());
    }
    if !err.trim().is_empty() {
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str("[stderr] ");
        body.push_str(err.trim_end());
    }
    let shown = if body.chars().count() > CMD_OUT_MAX {
        let cut: String = body.chars().take(CMD_OUT_MAX).collect();
        let left = body.chars().count() - CMD_OUT_MAX;
        format!("{cut}\n… (còn {left} ký tự — chạy lại kèm `| tail` để lấy khúc cuối)")
    } else {
        body
    };
    let head = match code {
        Some(0) => format!("✅ xong ({:.1}s)", ms as f64 / 1000.0),
        Some(c) => format!("❌ exit {c} ({:.1}s)", ms as f64 / 1000.0),
        None => format!("❌ không rõ mã thoát ({:.1}s)", ms as f64 / 1000.0),
    };
    if shown.trim().is_empty() {
        format!("{head}\n(không in ra gì)")
    } else {
        format!("{head}\n{shown}")
    }
}

/// Những phiên VỪA TẮT, giữ đủ lâu để còn hỏi được về chúng.
///
/// 🔴 Hà 2026-08-12 16:37 gõ `/ask` và nhận `⚠ không thấy phiên … đang chạy
/// nữa` — con trỏ đang theo trỏ vào một phiên vừa tắt lúc 16:08. Ngồi trước máy
/// thì câu ấy vẫn hỏi được (`claude --resume <id>` chạy trên NHẬT KÝ, không cần
/// tiến trình), nên theo đúng phép thử CẦU NỐI trong `CLAUDE.md`, phía điện
/// thoại không làm được là một **khoảng trống**, không phải một giới hạn.
///
/// Cùng khuôn với `STOPPED_KEY` — và cùng bài học: `claude agents` bỏ phiên khỏi
/// danh sách trong vài giây, nên "gác theo danh sách đang sống" là gác nhầm cửa.
/// Khác một chỗ: `STOPPED_KEY` chỉ nhớ phiên do CHÍNH hub dừng, còn sổ này nhớ
/// mọi phiên vừa rời danh sách, vì thứ Hà hỏi là phiên anh tự đóng.
pub const ENDED_KEY: &str = "ended:recent";

/// Giữ bao lâu. Đủ dài cho "phiên vừa tắt lúc nãy", đủ ngắn để `/ask` không âm
/// thầm chạy trên một phiên của tuần trước khi con trỏ bị bỏ quên.
pub const ENDED_KEEP_SEC: i64 = 24 * 3600;

/// Bao nhiêu phiên. Con trỏ chỉ trỏ được một phiên, nên vài dòng là đủ.
const ENDED_KEEP_N: usize = 10;

/// Chọn trong sổ ra phiên còn hỏi được — hàm THUẦN, kiểm không cần sổ thật.
pub fn pick_ended(
    list: &[(crate::sessions::LiveSession, i64)],
    id: &str,
    now: i64,
) -> Option<crate::sessions::LiveSession> {
    list.iter()
        .find(|(s, at)| s.session_id == id && now - at <= ENDED_KEEP_SEC)
        .map(|(s, _)| s.clone())
}

/// Ghi một phiên vừa tắt vào sổ, dựng từ chính cuốn sổ theo dõi.
///
/// Lúc phiên biến mất thì hàng của nó đi theo, nên mọi dữ kiện phải lấy từ
/// `watch::Mark` — đó là lý do `Mark` nhớ cả `a` (tài khoản) lẫn `c` (thư mục):
/// `--resume` cần đúng hai thứ ấy để tìm nhật ký và chạy bằng đúng tài khoản.
fn remember_ended(db: &Db, id: &str, mark: &crate::watch::Mark, now: i64) {
    let mut list: Vec<(crate::sessions::LiveSession, i64)> = db
        .cursor_or_log(ENDED_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    list.retain(|(s, at)| s.session_id != id && now - at <= ENDED_KEEP_SEC);
    let row = crate::sessions::LiveSession {
        session_id: id.to_string(),
        name: mark.n.clone(),
        account: mark.a.clone(),
        cwd: mark.c.clone(),
        folder: mark.d.clone(),
        kind: mark.k.clone(),
        // KHÔNG mang tty/pid theo: cửa sổ ấy có thể đã thuộc về phiên khác
        // (xem `sessions::window_taken_over`), và một `pid` của xác chết đọc lên
        // y hệt một phiên đang sống.
        ..Default::default()
    };
    list.push((row, now));
    if list.len() > ENDED_KEEP_N {
        let cut = list.len() - ENDED_KEEP_N;
        list.drain(..cut);
    }
    match serde_json::to_string(&list) {
        Ok(v) => {
            if let Err(e) = db.set_cursor(ENDED_KEY, &v) {
                logging::error("ended_list_not_saved", json!({ "err": e.to_string() }));
            }
        }
        Err(e) => logging::error("ended_list_not_encodable", json!({ "err": e.to_string() })),
    }
}

/// Phiên vừa tắt còn hỏi được không — `None` nếu quá hạn hoặc chưa từng ghi.
fn ended_session(db: &Db, id: &str) -> Option<crate::sessions::LiveSession> {
    let list: Vec<(crate::sessions::LiveSession, i64)> = db
        .cursor_or_log(ENDED_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    pick_ended(&list, id, chrono::Utc::now().timestamp())
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
    let mut live = crate::sessions::snapshot_cached(cfg, std::time::Duration::from_secs(20));
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
            .map(|t| {
                (chrono::Utc::now() - t.with_timezone(&chrono::Utc))
                    .num_seconds()
                    .max(0) as u64
            })
            .unwrap_or(0);

        // RÀO CHỐNG DÂY CHUYỀN: phiên vừa sinh ra thì đừng đóng sổ, dù % có
        // cao. Đêm 2026-08-12 bản `--resume` đẻ ra phiên mới mang nguyên ngữ
        // cảnh cũ (62% ngay khi sinh), tức nó đủ điều kiện đóng sổ lần nữa —
        // chỉ cần một lần rảnh là hub thay cửa sổ vô tận. Gốc đã vá (phiên mới
        // nay TRẮNG ngữ cảnh), nhưng rào này ở lại: một cơ chế tự động thay cửa
        // sổ của người khác thì phải có phanh riêng, không dựa vào việc "gốc đã
        // đúng rồi".
        let age_sec = (chrono::Utc::now().timestamp_millis() - s.started_at_ms).max(0) / 1000;
        if pct >= cfg.auto_handover.at_percent && age_sec < 600 {
            logging::info(
                "auto_handover_held",
                json!({ "session": s.session_id, "pct": pct,
                        "why": format!("TooYoung({age_sec}s)") }),
            );
            continue;
        }
        let why = auto_handover_why(
            pct,
            cfg.auto_handover.at_percent,
            done.iter().any(|d| d == &s.session_id),
            screen.is_some(),
            screen
                .as_ref()
                .is_some_and(|(t, _)| crate::keys::is_busy(t)),
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
                if let Err(e) =
                    db.record_spend("auto_handover", &h.new_session_id, h.cost_usd, &s.name)
                {
                    logging::error("spend_record_failed", json!({ "err": e.to_string() }));
                }
                // …rồi MỞ phiên mới và ĐÓNG phiên cũ (Hà chốt 2026-08-12, cách
                // A): *"tự chủ động đóng phiên rồi mở phiên mới luôn"*. Trước
                // đó hub dừng ở chỗ đưa một dòng `claude --resume …` cho chủ
                // máy tự gõ — vô dụng đúng lúc anh đang ở trên điện thoại, tức
                // đúng lúc tính năng này sinh ra để phục vụ.
                let moved = if s.tty.is_empty() {
                    Err(anyhow::anyhow!("phiên không có cửa sổ terminal"))
                } else {
                    crate::sessions::start_fresh_after_handover(cfg, s, &h.checkpoint)
                };
                // Con trỏ chuyển sang phiên MỚI THẬT (id ghép từ nhật ký), không
                // phải id bản fork: bản fork chỉ là chỗ lấy bản bàn giao, nó
                // không có cửa sổ nào để gõ vào.
                let err_text;
                let outcome = match &moved {
                    Ok(w) => match &w.new_id {
                        Some(new_id) => {
                            if let Err(e) = db.set_cursor(FOCUS_SESSION_KEY, new_id) {
                                logging::error(
                                    "focus_after_handover_failed",
                                    json!({ "err": e.to_string() }),
                                );
                            }
                            HandoverMove::Opened {
                                tty: &w.tty,
                                new_id,
                                closed_err: w.closed_err.as_deref(),
                            }
                        }
                        // Phiên mới chưa chào đời ⟹ con trỏ KHÔNG chuyển: nó
                        // phải trỏ vào một phiên gõ được, mà ở đây chưa có phiên
                        // nào cả — và cửa sổ cũ thì hub đã giữ lại.
                        None => HandoverMove::Stalled {
                            tty: &w.tty,
                            asking: &w.asking,
                        },
                    },
                    Err(e) => {
                        err_text = e.to_string();
                        HandoverMove::Failed {
                            err: &err_text,
                            resume_command: &h.resume_command,
                        }
                    }
                };
                let msg = auto_handover_notice(&crate::sessions::shown(s), pct, idle_sec, &outcome);
                // …và CÙNG CÂU ẤY sang Telegram (luật 11: hai cái mồm nói một
                // câu, không thì về sau không ai đối chiếu được).
                //
                // 🔴 Đo trên lượt nổ đầu tiên 2026-08-13 04:24:36: log chỉ có
                // `tfl5_chat_sent`, KHÔNG có một dòng telegram nào — tức hub tự
                // đóng cửa sổ đang làm việc của chủ máy rồi báo vào đúng cái
                // phòng anh không mở. Mà đây là tin duy nhất trong cả hub xảy
                // ra khi **không ai bấm gì**: bỏ sót nó là bỏ sót đúng lúc cần
                // nhất. `announce_changes` đã đi hai mồm từ đầu; chỗ này quên.
                //
                // Nút thì gắn có điều kiện — luật 14: chỉ trỏ vào phiên còn
                // sống, và ở đây phiên mới sống là điều kiện của chính nhánh
                // `Opened`. Ngoại lệ có chủ ý so với `enter_button` (nó bỏ nút
                // khi target == phiên đang theo): ở đây hub VỪA tự chuyển con
                // trỏ sang phiên mới, nên luật ấy sẽ gỡ nút trong 100% trường
                // hợp — và từ 0af884c, bấm vào phiên là thấy luôn màn, tức nút
                // này là đường ngắn nhất để nhìn tận mắt cái cửa sổ hub vừa mở.
                let button = match &outcome {
                    HandoverMove::Opened { new_id, .. } => {
                        Some(("👁 Xem phiên mới".to_string(), format!("sess:{new_id}")))
                    }
                    _ => None,
                };
                match (crate::telegram::inbox(), button) {
                    (Some(tg), Some(b)) => {
                        if let Err(e) = tg.send_buttons(&msg, &[b]) {
                            logging::error("auto_handover_telegram_failed", json!({ "err": e }));
                        }
                    }
                    // Không có nút (hoặc không có inbox — `hub once` chạy tay):
                    // vẫn phải tới điện thoại, thà một tin không nút còn hơn im.
                    _ => {
                        if let Err(e) = crate::confirm::tell(cfg, &msg) {
                            logging::error("auto_handover_telegram_failed", json!({ "err": e }));
                        }
                    }
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

/// Chuyện gì THẬT SỰ xảy ra khi hub thay cửa sổ — ba kết cục, không gộp.
///
/// Gộp lại là chỗ bản đầu nói sai: nó in `h.new_session_id` (id BẢN FORK) cho cả
/// ba, mà bản fork chỉ là chỗ lấy bản bàn giao — nó không có cửa sổ nào, không
/// nằm trong `claude agents`, nên `/session <id ấy>` không tới đâu (route Session
/// khớp id CHÍNH XÁC, xem `session_name_from_book`). Đo trên lượt nổ đầu tiên
/// 2026-08-13 04:24: tin nói `Phiên mới: f0883567`, còn phiên đang chạy thật là
/// `86fe1666` — và chính con trỏ `focus:session` đã trỏ đúng `86fe1666`. Tức tin
/// nhắn và cuốn sổ nói hai thứ khác nhau về cùng một việc.
pub enum HandoverMove<'a> {
    /// Cửa sổ mới đã mở VÀ ghép được id phiên mới ⟹ con trỏ đã chuyển sang nó.
    Opened {
        tty: &'a str,
        new_id: &'a str,
        closed_err: Option<&'a str>,
    },
    /// Cửa sổ mở rồi nhưng phiên mới KHÔNG chào đời (không có nhật ký để ghép
    /// id sau 12 giây) — nên hub **giữ nguyên cửa sổ cũ**. `asking` là hộp chọn
    /// đọc được trên cửa sổ mới, tức lý do nó đứng im.
    Stalled {
        tty: &'a str,
        asking: &'a [(usize, String)],
    },
    /// Không mở được cửa sổ nào: trả lại dòng `--resume` cho chủ máy tự gõ.
    Failed {
        err: &'a str,
        resume_command: &'a str,
    },
}

/// Câu hub nói khi nó vừa TỰ thay cửa sổ làm việc của chủ máy.
///
/// Thuần, và tách ra làm hàm riêng vì đây là tin nhắn khó nhất trong cả hub: nó
/// là thứ duy nhất báo một việc **không ai bấm ra** và **không lùi lại được** —
/// một cửa sổ đã đóng. Ba điều nó phải nói đúng, cả ba đều đã từng sai:
/// * **id gõ được** — id phiên MỚI THẬT, đủ dài để `/session` khớp, không phải
///   id bản fork và không cắt còn 8 ký tự.
/// * **con trỏ đang ở đâu** — vì trên Telegram, chữ thường gõ vào phòng đi
///   thẳng vào phiên đang theo. Nói sai chỗ này là gửi việc vào một phiên đã tắt.
/// * **cái gì CHƯA xong** — cửa sổ cũ chưa đóng được thì nói, đừng để chủ máy
///   phát hiện bằng cách nhìn thấy hai cửa sổ.
pub fn auto_handover_notice(name: &str, pct: u8, idle_sec: u64, moved: &HandoverMove) -> String {
    let head = format!(
        "📋 Tự đóng sổ {name} (ngữ cảnh {pct}%, đã rảnh {} phút) trước khi CLI phải nén ngữ cảnh.",
        idle_sec / 60
    );
    let (body, leftover) = match moved {
        HandoverMove::Opened {
            tty,
            new_id,
            closed_err,
        } => (
            format!(
                "Phiên mới {new_id} (TRẮNG ngữ cảnh, mang bản bàn giao) đang chạy ở cửa sổ {tty}.\n\
                 👁 Đang theo phiên mới — gõ thẳng vào đây là nói với nó.",
            ),
            *closed_err,
        ),
        HandoverMove::Stalled { tty, asking } => {
            let why = if asking.is_empty() {
                "\nKhông đọc được màn của nó — xem cửa sổ ấy trên máy.".to_string()
            } else {
                format!(
                    "\nNó đang DỪNG LẠI HỎI:\n{}",
                    asking
                        .iter()
                        .map(|(n, l)| format!("  {n}. {l}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            };
            (
                format!(
                    "⚠ Phiên mới mở ở cửa sổ {tty} nhưng CHƯA chào đời sau 12 giây.{why}\n\
                     ✅ Cửa sổ CŨ hub GIỮ NGUYÊN — không mất gì, phiên cũ vẫn ở đó. \
                     Trả lời câu hỏi ở cửa sổ mới rồi đóng cửa sổ cũ."
                ),
                None,
            )
        }
        HandoverMove::Failed {
            err,
            resume_command,
        } => (
            format!(
                "⚠ chưa mở được cửa sổ mới ({}) — mở tay bằng:\n{resume_command}",
                crate::exec::truncate(err, 120)
            ),
            None,
        ),
    };
    let tail = leftover
        .map(|e| format!("\n⚠ cửa sổ cũ chưa đóng được: {e}"))
        .unwrap_or_default();
    format!("{head}\n{body}{tail}")
}

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
        Err(e) => logging::error(
            "started_list_not_encodable",
            json!({ "err": e.to_string() }),
        ),
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
            logging::warn(
                "stopped_session_unreadable",
                json!({ "err": e.to_string() }),
            );
            return None;
        }
    };
    match serde_json::from_str::<crate::sessions::LiveSession>(&raw) {
        Ok(s) if same_session(&s.session_id, want) => Some(s),
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

// 🔴 ĐÃ XOÁ `project_pin_key` (2026-08-15) cùng route `/project`. Cái ghim ấy
// ghi vào một cuốn sổ mà CHỈ CHÍNH NÓ đọc lại: `/new` không hề tra ghim, nó lấy
// dự án từ cờ `-s`. Nên `/project dwork` chỉ làm được một việc — để `/project`
// trơn in lại chữ "dwork".

/// Dòng "lỗi gần đây" của `/doctor`, đọc từ bảng `runs`.
///
/// 🔴 Viết ngày 2026-08-14 vì tôi đã **báo sai** ở ba chỗ (`CLAUDE.md`, hai
/// commit, sổ phiên): tôi viết rằng `/doctor` đọc bảng này, và dựa vào đó để
/// biện minh cho việc `run_once` phải ghi `runs`. Kiểm lại thì `errors_block`
/// nằm trong `runtime::snapshot`, và hàm ấy có đúng MỘT chỗ gọi — `portal.rs`,
/// tệp đã chết cùng trang tfl5. Tức khối lỗi ấy không có ai đọc, và `/doctor`
/// chưa bao giờ hiện nó.
///
/// Sửa mã cho khớp thứ đã hứa, chứ không sửa lời hứa cho khớp mã: hai câu ấy
/// dẫn tới hai sản phẩm khác nhau, và cái người ta cần là cái `/doctor` nói
/// được "có 3 lỗi gần đây" thay vì im lặng đúng lúc cần nói nhất.
///
/// Chỉ đọc `runs`, không đọc tệp log: log là chữ nối đuôi, đã lên tới hàng chục
/// MB, và đọc nó mỗi lần bấm `/doctor` là biến một câu hỏi rẻ thành thứ đắt
/// nhất trong vòng.
pub fn recent_errors_line(db: &Db) -> String {
    match db.last_runs(40) {
        Ok(rows) => {
            let bad: Vec<&crate::db::RunRow> = rows
                .iter()
                .filter(|r| r.ok == Some(0) || r.err.as_deref().is_some_and(|e| !e.is_empty()))
                .take(3)
                .collect();
            if bad.is_empty() {
                // NÓI RÕ nó soi cái gì. "Không có lỗi" mà không nói phạm vi thì
                // người đọc tự hiểu thành "mọi thứ ổn" — trong khi phần lớn
                // trục trặc của hub sống ở mức `warn` và cố ý không lên đây.
                "✅ 40 vòng gần nhất: không có lỗi (mức `error`; `warn` không tính)".to_string()
            } else {
                let lines: Vec<String> = bad
                    .iter()
                    .map(|r| {
                        format!(
                            "  · {} {}",
                            crate::exec::truncate(&r.started_at, 19),
                            crate::exec::truncate(r.err.as_deref().unwrap_or("?"), 90)
                        )
                    })
                    .collect();
                format!("⚠ lỗi gần đây:\n{}", lines.join("\n"))
            }
        }
        // Không đọc được sổ thì NÓI, đừng in một dấu tích xanh — đó đúng là
        // hình dạng "im lặng khi mù" mà luật 3 cấm.
        Err(e) => {
            logging::warn("doctor_runs_unreadable", json!({ "err": e.to_string() }));
            "⚠ không đọc được sổ vòng chạy — xem logs/hub.log".to_string()
        }
    }
}

// 🔴 ĐÃ BỎ CẢ CHẶNG HỎI VÒNG (`ADAPTER_NAMES`, `adapter_enabled`,
// `poll_adapter`, `ingest`), 2026-08-14, cùng lượt gỡ tfl5.
//
// Chặng ấy tồn tại để hỏi phòng chat: một vòng lặp qua danh sách kênh, mỗi kênh
// một dòng `runs`, đọc con trỏ, và ghi con trỏ SAU khi lệnh đã chạy. Sau khi
// phòng đóng, danh sách còn đúng một tên và `poll_adapter` trả `unknown adapter`
// cho chính cái tên ấy — tức `/ingest` lẫn `hub ingest` chỉ còn đúng một câu trả
// lời khả dĩ: *"disabled in config"*. Đó đúng là thứ luật riêng của dự án cấm:
// **một động từ phân tích được mà không có việc gì để làm**.
//
// Telegram không hỏi vòng, nó ĐẨY TỚI: `telegram::Inbox` giữ một luồng riêng
// chạy `getUpdates`, xếp tin vào hộp, rồi đánh thức vòng bằng `Waker`. Không có
// con trỏ nào để tiến ở đây, vì `getUpdates` tự tiến bằng `offset` của nó.
//
// Đi theo nó là bảng `runs`: không còn ai ghi. Xem `run_once` — nay chính nó ghi
// một dòng cho mỗi vòng, để `hub status` và khối "lỗi gần đây" của `/doctor` còn
// có chỗ đọc, thay vì luôn luôn rỗng.

/// Nhiều nhất bấy nhiêu nút phiên trong một tin.
///
/// Không phải giới hạn của Telegram (nó chịu được nhiều hơn) mà của **ngón tay
/// trên điện thoại**: quá số này thì bảng phím dài hơn màn hình và cái nút cuối
/// nằm ngoài tầm nhìn. Cắt thì phải NÓI RA — xem `session_list_text`.
pub const MAX_SESSION_BUTTONS: usize = 12;

/// Danh sách phiên, viết cho một cái điện thoại.
///
/// Vì sao có hàm này (Hà 2026-08-11: *"chưa có lệnh để xem danh sách phiên?"*):
/// bảng `/help` cho tới hôm nay đòi **id có sẵn** ở `/session <id>`, `/stop
/// [id]`, `/handover [id]` — mà không route nào ĐƯA ra id. Từ Telegram nghĩa là
/// muốn làm gì cũng phải mở trang điện thoại ra chép id, tức đúng cái lỗ hổng
/// mà tiêu chí gốc của hub gọi tên: ngồi trước máy thì `claude agents` là thấy,
/// qua điện thoại thì không.
///
/// Mỗi dòng trả lời đúng ba câu: **phiên nào** (tên · tài khoản), **đang chạy
/// hay đứng chờ**, và **id ngắn** để gõ tiếp `/stop`, `/handover`. Phiên đang
/// theo có dấu 👁 vì mọi lệnh không mang id sẽ rơi vào chính nó.
/// Phiên này mở ra từ ĐÂU — và nó trả lời luôn câu "gõ vào được không".
///
/// 🔴 Hà 2026-08-13, ngay sau khi phiên VS Code hiện lên danh sách: *"ở danh
/// sách phiên nên thêm icon biểu diễn nguồn là terminal hay vs code"*. Đúng
/// chỗ: từ hôm nay danh sách trộn hai loại phiên **trông y hệt nhau mà làm
/// được hai việc khác nhau** — gõ thẳng (`/type` `/key` `/shot`) chỉ chạy trên
/// phiên Terminal. Không có dấu phân biệt thì người đọc phải bấm mới biết, và
/// biết bằng cách nhận một câu từ chối.
///
/// Chọn ký hiệu theo VIỆC LÀM ĐƯỢC, không theo thương hiệu: `⌨` = gõ thẳng vào
/// được; `💻` = xem và hỏi được, gõ thì không; `🌙` = phiên nền (`--bg`), không
/// có cửa sổ nào; `🔌` = tiến trình rời, không gắn tty.
pub fn source_icon(host: &str) -> &'static str {
    match host {
        "editor" => "💻",
        "background" => "🌙",
        "detached" => "🔌",
        // `dead` giữ ký hiệu của TÌNH TRẠNG (⚫) ở cột sau; ở cột nguồn thì một
        // phiên đã tắt vẫn từng mở ra từ một cái terminal.
        _ => "⌨",
    }
}

pub fn session_list_text(
    sessions: &[crate::sessions::LiveSession],
    focus: &str,
    now_ms: i64,
) -> String {
    if sessions.is_empty() {
        return "Không có phiên nào đang sống.".to_string();
    }
    let mut out = format!("📋 {} phiên đang sống:\n", sessions.len());
    for s in sessions.iter().take(MAX_SESSION_BUTTONS) {
        let eye = if !focus.is_empty() && s.session_id == focus {
            "👁 "
        } else {
            ""
        };
        // BA tình trạng, không phải hai (Hà 2026-08-12: *"phải thêm tình trạng
        // đang xử lý, đã dừng"*). Phiên đã tắt vẫn nằm trong danh sách vài giây
        // và vẫn `/handover` được, nên gộp nó vào "đứng chờ" là nói sai về thứ
        // người ta sắp làm với nó.
        // BỐN tình trạng. "Đang hỏi" đứng trên cả "đang chạy": nó là trạng thái
        // duy nhất trong bốn cái mà người đọc PHẢI làm gì đó thì việc mới đi
        // tiếp — mà nó lại nhìn y hệt "đứng chờ" nếu không nói ra.
        let run = match (s.host.as_str(), s.asking.is_some(), s.working) {
            // Chấm TRẠNG THÁI, không phải ký hiệu điều khiển.
            //
            // 🔴 Hà 2026-08-13: *"icon biểu diễn chạy và dừng bị ngược ở danh
            // sách phiên"*. Đo ra chỗ lẫn: `▶`/`⏸`/`⏹` là bộ ký hiệu của máy
            // phát nhạc, mà ở đó chúng là NÚT BẤM — `▶` nghĩa "bấm để chạy",
            // `⏸` nghĩa "bấm để dừng". hub lại dùng chúng làm TÌNH TRẠNG, nên
            // đọc ra đúng nghĩa ngược. Và chính hub cũng đang dùng `▶` làm nút
            // chạy lệnh thật (`remember_quick`) — một ký hiệu hai nghĩa trong
            // cùng một tin nhắn.
            ("dead", _, _) => "⚫ đã tắt",
            (_, true, _) => "⚠ dừng lại HỎI",
            // LỖI đứng trên "đang chạy": một phiên dừng vì lỗi nhìn từ xa y hệt
            // một phiên đã xong — Hà 2026-08-13: *"vì lỗi chưa thấy cảnh báo
            // gì"*. Đọc từ nhật ký nên không phụ thuộc việc bắt đúng nhịp.
            _ if s.error.is_some() => "🔴 dừng vì LỖI",
            (_, _, true) => "🟢 đang chạy",
            _ => "🟡 đứng chờ",
        };
        // Dự án ĐANG LÀM đứng trước tên: tên phiên do `claude` tự đặt
        // ("projects-ff") không nói được gì, còn `cwd` thì giống hệt nhau ở mọi
        // dòng trên máy này — xem `sessions::folder_from_tail`.
        // Nhãn dự án thay cho tên tự sinh — xem `sessions::display_name`.
        let what = crate::sessions::shown(s);
        out.push_str(&format!(
            "{}{} {} · {} · {} · {}\n",
            eye,
            source_icon(&s.host),
            what,
            s.account,
            run,
            short_id(&s.session_id)
        ));
        // Hai dòng phụ, và chúng trả lời hai câu khác nhau: *tình trạng* (còn
        // gõ tiếp được không, im bao lâu rồi) và *nội dung* (nó vừa nói gì).
        // Thiếu vế sau thì danh sách chỉ nói phiên nào TỒN TẠI, không nói phiên
        // nào ĐÁNG mở ra — mà đó mới là việc người ta cầm điện thoại lên để làm.
        let meta = session_meta(s, now_ms);
        if !meta.is_empty() {
            out.push_str(&format!("    {meta}\n"));
        }
        // Phiên đang hỏi thì CÂU HỎI thay chỗ câu cuối: câu cuối của nó chính là
        // lời dẫn vào câu hỏi, còn thứ người đọc cần là hỏi gì và chọn được gì.
        if let Some(a) = &s.asking {
            let head = if a.header.is_empty() { "" } else { &a.header };
            out.push_str(&format!(
                "    ⚠ {}{}\n",
                if head.is_empty() {
                    String::new()
                } else {
                    format!("{head}: ")
                },
                crate::exec::truncate(&a.question, 120)
            ));
            for (i, o) in a.options.iter().take(9).enumerate() {
                out.push_str(&format!(
                    "      {}. {}\n",
                    i + 1,
                    crate::exec::truncate(o, 60)
                ));
            }
            continue;
        }
        if let Some(said) = &s.last_text {
            let said = said.replace(['\n', '\r'], " ");
            let said = said.trim();
            if !said.is_empty() {
                out.push_str(&format!("    💬 {}\n", crate::exec::truncate(said, 70)));
            }
        }
    }
    if sessions.len() > MAX_SESSION_BUTTONS {
        // Cắt bớt mà im lặng thì danh sách này nói dối về số phiên đang chạy.
        out.push_str(&format!(
            "…còn {} phiên nữa không hiện nút — dùng /session <id>\n",
            sessions.len() - MAX_SESSION_BUTTONS
        ));
    }
    if focus.is_empty() {
        out.push_str("Chưa theo phiên nào — bấm một nút để theo.");
    } else if !sessions.iter().any(|s| s.session_id == focus) {
        // Con trỏ trỏ vào một phiên KHÔNG còn trong danh sách: nói ra, vì mọi
        // lệnh không mang id vẫn đang nhắm vào nó.
        out.push_str(&format!(
            "👁 Đang theo {} — phiên này không còn sống.",
            short_id(focus)
        ));
    }
    out.trim_end().to_string()
}

/// Một dòng gõ trên Telegram mà `parse_command` không nhận: nó là **chữ để gõ
/// vào phiên**, hay là một lệnh gõ nhầm?
///
/// Ranh giới là **dấu gạch chéo đầu dòng**, và nó phải rạch ròi vì hai bên hỏng
/// theo hai kiểu ngược nhau. Đọc nhầm chữ thành lệnh thì câu của chủ máy rơi vào
/// hư không (đường cũ: *"Chưa hiểu — kênh này nhận LỆNH"*). Đọc nhầm lệnh thành
/// chữ thì một lỗi chính tả (`/sesion`) **được gõ thẳng vào cửa sổ đang chạy**,
/// kèm Enter — hub biến cái gõ nhầm thành một lượt gõ thật.
///
/// `None` cho dòng rỗng và cho mọi dòng mở đầu bằng `/`. Muốn gửi một dòng có
/// dấu gạch chéo VÀO phiên thì đi qua `/type`, tức nói rõ ý định.
pub fn text_for_session(line: &str) -> Option<&str> {
    let t = line.trim();
    (!t.is_empty() && !t.starts_with('/')).then_some(t)
}

/// Bao nhiêu dòng màn hình đi ra ngoài trong một `/shot`.
///
/// 🔴 Hà 2026-08-12: *"phiên projects-d2 hiện ra rõ ràng có lệnh để chạy trên
/// terminal `git -C … push origin main` nhưng ở tele lại không hề có"*. Không
/// phải hub đọc nhầm nguồn — nó đọc ĐÚNG màn thật (`contents of selected tab`,
/// không phải nhật ký) — mà nó chỉ giữ **14 dòng cuối**, và câu lệnh ấy nằm cao
/// hơn cửa sổ 14 dòng. Một phép cắt im lặng đọc lên y hệt "trên màn không có".
///
/// 40 dòng: một màn terminal thường cao 40-50 dòng, nên đây là "gần cả màn", và
/// vẫn dưới trần 4096 ký tự của Telegram cho phần lớn nội dung.
pub const SHOT_LINES: usize = 40;

/// Trần cứng cho số dòng `/shot <n>` xin thêm — trên nữa thì Telegram tự cắt,
/// mà một tin bị Telegram cắt thì mất đúng phần cuối (phần mới nhất).
pub const SHOT_LINES_MAX: usize = 120;

/// Những lệnh vừa thấy trên màn, để cái nút "gửi nhanh" tra lại được.
///
/// Vì sao phải có sổ: `callback_data` của Telegram trần **64 byte**, mà một
/// dòng `git -C ~/projects/AI/tcc/amm push origin main` đã 52 — thêm
/// tiền tố là tràn, và một cái nút tràn thì Telegram từ chối cả tin. Nên nút
/// mang một CON SỐ, còn chữ nằm ở đây.
/// Cửa sổ ĐANG CHỜ được đóng: `/exit` đã gõ, giờ canh cho CLI chạy nốt.
///
/// 🔴 Hà 2026-08-13: *"trước khi đóng phải chờ cli chạy nốt mới đóng hẳn"* rồi
/// *"30 giây kiểm tra 1 lần nếu chưa xong thì chờ tiếp"*. Vế sau mới là chỗ
/// bắt phải có cuốn sổ này: **chờ không có hạn**. Bản cũ chờ tại chỗ 30 giây
/// rồi bỏ cuộc, và bỏ cuộc là câu trả lời sai — một lượt `claude` chạy hai
/// mươi phút thì cửa sổ ấy vẫn phải đóng, chỉ là muộn hơn.
///
/// Không chờ tại chỗ được: `execute_commands` giữ `CMD_LOCK`, nên chờ dài là
/// **khoá cả vòng chạy** — không tin báo, không lệnh nào khác đi được. Nên ghi
/// sổ rồi trả lời ngay, và mỗi vòng ngó lại một lượt. Cùng cỗ máy "so hai lượt,
/// nói một lần" của `watch.rs`, và cùng lý do: một việc kéo dài thì phải sống
/// trong sổ, không sống trong một lời gọi hàm.
pub const CLOSING_KEY: &str = "closing:windows";

/// Một cửa sổ đang chờ đóng.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Closing {
    /// id cửa sổ Terminal.
    pub w: i64,
    /// Nhãn để nói tên phiên lúc báo xong — hàng của nó sẽ biến mất trước đó.
    pub n: String,
    /// Lúc gõ `/exit` (epoch giây), để nói "chờ bao lâu rồi".
    pub t: i64,
    /// Lần kiểm gần nhất (epoch giây) — cách nhau 30 giây, không kiểm mỗi vòng.
    #[serde(default)]
    pub c: i64,
}

/// Bao lâu ngó lại một lần. Hà nói thẳng con số này.
const CLOSE_CHECK_SEC: i64 = 30;

/// Chờ tới đây mà cửa sổ vẫn bận thì THÔI CHỜ IM — nói ra và trả quyền quyết
/// định lại cho chủ máy.
///
/// 🔴 Hà 2026-08-14: *"Rõ ràng phiên dwork dừng rồi, tôi gửi lệnh close rồi 1h
/// hay lại xem shot nó vẫn ở đó"*. Đọc log đúng như thế: `/close` lúc 11:06:19,
/// hub gõ `/exit`, rồi `close_still_busy` đều đặn 30 giây một lần — 20s · 60s ·
/// 133s · 167s · 204s · 247s… và **không một dòng nào ra tới Telegram**. Câu hứa
/// gửi đi lúc đầu là *"Kiểm 30 giây một lần, xong tôi báo"*, nên im lặng ở đây
/// đọc thành "đang chạy êm", trong khi sự thật là hub đang chờ một điều kiện có
/// thể không bao giờ tới.
///
/// Vì sao nó có thể không bao giờ tới: `/exit` gõ vào một phiên ĐANG CHẠY thì
/// nằm trong hàng chờ của TUI cho tới khi lượt hiện tại xong — mà một lượt
/// `claude` chạy hàng chục phút là chuyện thường ở đây. Chờ vô hạn không sai về
/// logic, nó chỉ sai ở chỗ IM.
///
/// Mười phút: dài hơn hẳn một lượt bình thường, ngắn hơn hẳn một tiếng ngồi
/// đoán.
const CLOSE_GIVE_UP_SEC: i64 = 600;

/// Bao lâu thì nhắc một câu ra kênh chat trong lúc còn chờ.
const CLOSE_SAY_EVERY_SEC: i64 = 120;

/// Ghi một cửa sổ vào sổ chờ đóng.
pub fn remember_closing(db: &Db, session_id: &str, window: i64, shown_name: &str, now: i64) {
    let mut book = closing_book(db);
    book.insert(
        session_id.to_string(),
        Closing {
            w: window,
            n: shown_name.to_string(),
            t: now,
            c: 0,
        },
    );
    save_closing(db, &book);
}

fn closing_book(db: &Db) -> BTreeMap<String, Closing> {
    db.cursor_or_log(CLOSING_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}

fn save_closing(db: &Db, book: &BTreeMap<String, Closing>) {
    match serde_json::to_string(book) {
        Ok(v) => {
            if let Err(e) = db.set_cursor(CLOSING_KEY, &v) {
                // Không nuốt: mất sổ này là mất luôn cái cửa sổ đang chờ, và nó
                // sẽ nằm mở mãi với dòng `[Process completed]`.
                logging::error("closing_book_not_saved", json!({ "err": e.to_string() }));
            }
        }
        Err(e) => logging::error("closing_book_not_encoded", json!({ "err": e.to_string() })),
    }
}

/// Mỗi vòng: cửa sổ nào đang kẹt ở hộp tin-thư-mục thì BẤM HỘ.
///
/// 🔴 Hà 2026-08-13, sau khi tôi vá đường tự đóng sổ: *"vẫn đang đứng im"*. Bản
/// vá kia chỉ cứu lượt SAU — nó không gỡ được cửa sổ đang đứng, và cửa sổ ấy
/// **chưa có id phiên** nên KHÔNG route nào của hub với tới được: `/key`,
/// `/type`, `/shot`, `/close` đều nhắm bằng id. Một cửa sổ hub tự mở, rồi hub
/// tự mất đường vào.
///
/// Nên phép bấm hộ phải sống trong VÒNG CHẠY, không sống trong một lời gọi hàm
/// — cùng bài học với `CLOSING_KEY`: việc kéo dài thì phải có người ngó lại.
/// Không cần sổ: dấu hiệu nằm ngay trên màn, và `trust_dialog_choice` chỉ khớp
/// ĐÚNG hộp ấy (đúng hai lựa chọn, đúng chữ *"trust this folder"*). Màn nào
/// không phải hộp ấy thì hàm không bấm gì cả.
///
/// Quét MỌI tab, không riêng tab hub mở: hộp này hỏi một lần cho mỗi cặp tài
/// khoản × thư mục, và câu trả lời luôn là "có" — chủ máy uỷ quyền 2026-08-13.
/// Ba mươi giây một lượt, cùng nhịp với `close_pending_tick`, vì nó cũng là
/// một câu hỏi về màn hình chứ không phải một sự kiện.
pub fn trust_dialog_tick(now: i64) {
    static LAST: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    let last = LAST.load(std::sync::atomic::Ordering::Relaxed);
    if now - last < CLOSE_CHECK_SEC {
        return;
    }
    LAST.store(now, std::sync::atomic::Ordering::Relaxed);
    let tabs = match crate::keys::terminal_tabs() {
        Ok(t) => t,
        Err(e) => {
            logging::warn("trust_tick_probe_failed", json!({ "err": e.to_string() }));
            return;
        }
    };
    for tab in tabs {
        // Chỉ tab đang chạy `claude`: hộp ấy là của `claude`, và đọc màn của
        // một tab đang chạy thứ khác là đọc thứ không liên quan.
        if tab.cli() != Some("claude") {
            continue;
        }
        if let Some(n) = crate::sessions::answer_trust_dialog(&tab.tty) {
            logging::info(
                "trust_dialog_unstuck",
                json!({ "tty": tab.tty, "pressed": n,
                        "why": "cửa sổ kẹt ở hộp tin-thư-mục, chưa có id phiên nên không route nào với tới" }),
            );
        }
    }
}

/// Mỗi vòng: cửa sổ nào hết bận thì đóng, còn bận thì CHỜ TIẾP.
///
/// Ba kết cục, cả ba đều nói ra: đóng được · cửa sổ không còn (ai đó đã đóng
/// tay, hoặc `claude` thoát rồi Terminal tự dọn) · hỏi không được. Ca cuối
/// **giữ nguyên trong sổ** — không hỏi được ≠ không còn, đúng luật `Look::Blind`.
pub fn close_pending_tick(db: &Db, cfg: &Config, now: i64) {
    let mut book = closing_book(db);
    if book.is_empty() {
        return;
    }
    let mut changed = false;
    let mut done: Vec<String> = Vec::new();
    for (id, c) in book.iter_mut() {
        if now - c.c < CLOSE_CHECK_SEC {
            continue;
        }
        c.c = now;
        changed = true;
        match crate::keys::tab_busy(c.w) {
            Ok(true) => {
                let waited = now - c.t;
                logging::info(
                    "close_still_busy",
                    json!({ "session": id, "window": c.w, "waited_sec": waited }),
                );
                // Hết kiên nhẫn thì NÓI, và trả quyền quyết định lại: hub không
                // tự đóng cứng một cửa sổ đang chạy dở — đóng khi còn tiến trình
                // sống làm Terminal bật hộp thoại "terminate running processes?",
                // mà một hộp thoại thì khoá mọi lệnh tự động sau nó (bài học
                // 08-11, xem `keys::close_window`).
                if waited >= CLOSE_GIVE_UP_SEC {
                    say_closed(cfg, &format!(
                        "⚠ {} vẫn chưa đóng được sau {} phút — cửa sổ còn bận, tức CLI đang chạy dở \
                         một lượt và `/exit` nằm trong hàng chờ của nó.\nhub THÔI chờ (không tự đóng \
                         cửa sổ đang chạy: làm thế Terminal bật hộp thoại và khoá luôn mọi lệnh sau \
                         đó). Gõ /close lần nữa khi phiên rảnh, hoặc dừng lượt đang chạy rồi /close.",
                        c.n,
                        waited / 60
                    ));
                    done.push(id.clone());
                    logging::warn(
                        "close_gave_up",
                        json!({ "session": id, "window": c.w, "waited_sec": waited }),
                    );
                } else if waited >= CLOSE_SAY_EVERY_SEC
                    && (waited / CLOSE_SAY_EVERY_SEC)
                        != ((waited - CLOSE_CHECK_SEC) / CLOSE_SAY_EVERY_SEC)
                {
                    // Nhắc thưa thôi, nhưng phải có: một lời hứa "xong tôi báo"
                    // mà im mười phút thì người ta đi kiểm tay, đúng cái hub
                    // sinh ra để khỏi phải làm.
                    say_closed(cfg, &format!(
                        "⏳ {} chưa đóng được — còn bận sau {} phút. hub vẫn chờ (bỏ cuộc ở phút thứ {}).",
                        c.n,
                        waited / 60,
                        CLOSE_GIVE_UP_SEC / 60
                    ));
                }
            }
            Ok(false) => {
                match crate::keys::close_window(c.w) {
                    Ok(()) => {
                        logging::info(
                            "close_done",
                            json!({ "session": id, "window": c.w, "waited_sec": now - c.t }),
                        );
                        say_closed(cfg, &format!(
                            "⏹ Đã đóng hẳn {} — CLI chạy nốt rồi thoát, cửa sổ terminal đã đóng (chờ {}s).",
                            c.n,
                            now - c.t
                        ));
                    }
                    Err(e) => {
                        // Cửa sổ biến mất giữa hai lượt hỏi là chuyện thường
                        // (Terminal tự dọn khi shell thoát, tuỳ profile) — vẫn
                        // là XONG, và vẫn phải nói ra chứ không im.
                        logging::info(
                            "close_window_gone",
                            json!({ "session": id, "window": c.w,
                                    "err": crate::logging::err_chain(&e) }),
                        );
                        say_closed(cfg, &format!("⏹ {} đã thoát, cửa sổ không còn.", c.n));
                    }
                }
                done.push(id.clone());
            }
            Err(e) => {
                // KHÔNG bỏ khỏi sổ: hỏi không được là hub mù, không phải cửa sổ
                // đã đóng. Bỏ đi là im lặng đánh rơi việc.
                logging::warn(
                    "close_check_failed",
                    json!({ "session": id, "window": c.w,
                            "err": crate::logging::err_chain(&e) }),
                );
            }
        }
    }
    for id in &done {
        book.remove(id);
    }
    if changed || !done.is_empty() {
        save_closing(db, &book);
    }
}

/// Một câu ra Telegram, không cần biết lệnh đến từ đâu — lúc này lượt lệnh đã
/// trả lời xong từ lâu, đây là tin của vòng chạy.
fn say_closed(cfg: &Config, text: &str) {
    if let Some(tg) = crate::telegram::inbox() {
        if let Err(e) = tg.send_text(text) {
            logging::error("close_ack_failed", json!({ "err": e }));
        }
    }
    let _ = cfg;
}

pub const QUICK_KEY: &str = "quick:cmds";

// Ghi chú về NÚT LỆNH (chú thích của `remember_quick`, để lạc lại đây từ một
// lượt sửa cũ — nay là chú thích thường, không phải tài liệu của hàm bên dưới):
// nút gõ dòng lệnh VÀO PHIÊN chứ không chạy ngoài (Hà 2026-08-12: *"có thể sẽ
// chạy được trực tiếp từ ô chát trong cli bằng cách thêm ký tự `!` ở đầu"*).
// Khác biệt không nhỏ: chạy trong phiên thì phiên nhìn thấy kết quả và đi tiếp
// được, còn `/cmd` chạy ở một shell rời — kết quả về điện thoại, phiên không
// biết gì.
//
// 🔴 ĐÃ XOÁ `session_folder_from_book` (2026-08-14): `session_root` làm đúng
// việc ấy và làm đủ hơn — nó ghép luôn với `workspace_root` rồi trả đường dẫn
// thật, nên mọi chỗ gọi đều chọn nó. Hai hàm cùng đọc một cuốn sổ để trả lời
// một câu là hai chỗ để lệch nhau.

/// Nhớ bản ĐẦY ĐỦ của một báo cáo, để dòng "… (còn N dòng)" có đường đi tiếp.
///
/// 🔴 Hà 2026-08-12: *"cuối tin nhắn sao lại báo còn số dòng vậy, muốn xem nốt
/// thì làm thế nào"*. Đúng: bản rút gọn nói ra phần nó giấu (tử tế), rồi bỏ
/// người đọc ở đó (không tử tế). Mà bản đầy đủ vốn đã nằm sẵn trong tay
/// (`last_say`, tới 12 000 ký tự) — chỉ là chưa ai đưa nó ra.
///
/// Giữ 8 bản gần nhất kèm một `base` chạy tiến, nên số trên nút KHÔNG bị lệch
/// khi bản cũ rơi ra: lấy theo số tuyệt đối, không theo vị trí trong mảng — nút
/// cũ bấm lại thì trả về "bản ấy cũ quá rồi", chứ không trả về báo cáo của một
/// phiên khác.
pub const FULL_KEY: &str = "report:full";

/// Một bản đầy đủ, kèm CHỦ của nó.
///
/// 🔴 Hà 2026-08-13: *"bấm xem đầy đủ thì thêm nút vào phiên luôn nếu nó không
/// thuộc phiên đang chọn"*. Đúng — người đọc xong một báo cáo dài thì việc kế
/// tiếp gần như luôn là **đi vào chính phiên ấy**; bắt họ quay ra `/sessions`
/// rồi dò lại là cắt đứt đúng chỗ đang liền mạch. Muốn gắn được nút thì kho
/// phải nhớ báo cáo ấy **của phiên nào**, chứ không chỉ nhớ chữ.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct FullItem {
    /// id phiên — thứ nút `sess:<id>` cần.
    s: String,
    /// tên để đọc (`[amm] hanguyen-8e`), nhãn của nút.
    n: String,
    t: String,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct FullStore {
    base: usize,
    items: Vec<FullItem>,
}

pub fn remember_full(
    db: &Db,
    session_id: &str,
    name: &str,
    text: &str,
) -> Option<(String, String)> {
    let mut st: FullStore = db
        .cursor_or_log(FULL_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    st.items.push(FullItem {
        s: session_id.to_string(),
        n: name.to_string(),
        t: text.to_string(),
    });
    let n = st.base + st.items.len() - 1;
    if st.items.len() > 8 {
        let cut = st.items.len() - 8;
        st.items.drain(..cut);
        st.base += cut;
    }
    match serde_json::to_string(&st) {
        Ok(v) => {
            if let Err(e) = db.set_cursor(FULL_KEY, &v) {
                logging::error("full_report_not_saved", json!({ "err": e.to_string() }));
                return None;
            }
        }
        Err(e) => {
            logging::error("full_report_not_saved", json!({ "err": e.to_string() }));
            return None;
        }
    }
    Some(("📄 Xem đầy đủ".to_string(), format!("full:{n}")))
}

/// Dòng đuôi của bản đầy đủ: nói con trỏ đã đi đâu — hoặc nói là CHƯA đi.
///
/// Thuần, vì đây là chỗ một câu chữ có thể làm chủ máy gõ việc vào nhầm phiên.
/// `moved`: `None` = không cần chuyển (đang theo sẵn phiên ấy) · `Some(true)` =
/// đã ghi sổ xong · `Some(false)` = ghi hỏng.
///
/// Luật duy nhất nó giữ, và là lý do nó tồn tại: **chỉ được nói "đang theo" khi
/// sổ ĐÃ ghi xong**. Ghi hỏng mà vẫn in câu ấy là đúng loại nói dối khiến câu
/// tiếp theo của chủ máy rơi vào một phiên khác — im lặng còn đỡ hơn.
pub fn full_report_follow_note(sname: &str, moved: Option<bool>) -> String {
    match moved {
        None => String::new(),
        Some(true) => format!(
            "\n\n👁 Đang theo phiên {} — gõ thẳng vào đây là nói với nó.",
            crate::exec::truncate(sname, 40)
        ),
        Some(false) => format!(
            "\n\n⚠ chưa chuyển được sang phiên {} — vẫn đang theo phiên cũ.",
            crate::exec::truncate(sname, 40)
        ),
    }
}

/// Bản đầy đủ số `n` — trả `(id phiên, tên để đọc, nội dung)` nếu còn giữ.
pub fn full_report(db: &Db, n: usize) -> Option<(String, String, String)> {
    let st: FullStore = db
        .cursor_or_log(FULL_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())?;
    n.checked_sub(st.base)
        .and_then(|i| st.items.get(i))
        .map(|it| (it.s.clone(), it.n.clone(), it.t.clone()))
}

/// Đường dẫn file hub vừa nhắc tới trên màn — để nút `file:<n>` tìm lại được.
pub const FILES_KEY: &str = "quick:files";

/// Nhớ các đường dẫn rồi dựng nút `📎 <tên file>`.
///
/// Cùng khuôn với [`remember_quick`] và cố ý thế: một cuốn sổ, một dạng
/// `callback_data`, một chỗ hết hạn. Nút mang CHỈ SỐ chứ không mang đường dẫn,
/// vì `callback_data` của Telegram chỉ có 64 byte — một đường dẫn tuyệt đối
/// vượt trần ấy là chuyện thường, và khi vượt thì nút im lặng không hiện.
///
/// Sổ nhớ luôn **phiên nào đã nhắc tới đường dẫn ấy**, và đó là cả điểm:
/// 🔴 Hà 2026-08-13: *"giới hạn phiên nào chỉ nhận được file nằm trong đúng thư
/// mục của phiên đó thôi"*. Bản đầu gác ở GỐC WORKSPACE, tức một phiên dwork
/// nhắc tới đường dẫn của tfl5 là kéo được file tfl5 về điện thoại. Gác theo
/// phiên thì mỗi cái nút chỉ với tới đúng cây thư mục mà phiên ấy đang làm.
///
/// Buộc theo phiên ĐÃ SINH RA nút, không phải phiên đang theo lúc bấm: con trỏ
/// đổi được giữa hai thời điểm ấy (bấm "Xem đầy đủ" là đổi), và lúc đó cái nút
/// sẽ lặng lẽ đo bằng một cái thước khác.
pub fn remember_files(
    db: &Db,
    cfg: &Config,
    session_id: &str,
    paths: &[String],
) -> Vec<(String, String)> {
    // 🔴 MỘT CÁI TÊN KHÔNG PHẢI MỘT TỆP. Hà 2026-08-14, ảnh chụp một tin có nút
    // 📎 `com.dipgle.hubd.plist`: *"Com.dipgle.hubd.plist đâu phải là file"*.
    // Đúng — đó là một cái tên nhắc giữa câu văn của chính hub, và tệp thật thì
    // nằm ở `~/Library/LaunchAgents`, ngoài cây làm việc của phiên.
    //
    // Cửa "chỉ gửi tệp NẰM TRONG thư mục phiên" vốn đã có, nhưng nó đặt ở lúc
    // BẤM (`send_document`). Nên cái nút vẫn mọc ra, vẫn mời bấm, và chỉ trả
    // lời "chưa gửi được" sau khi người ta bấm — tức hub dựng một lời hứa rồi
    // để người dùng đi phát hiện hộ rằng nó rỗng. Hỏi ngay lúc DỰNG thì rẻ
    // (một lần `stat`) và cái nút không tồn tại nếu không có gì để mở.
    //
    // Không tra được thư mục phiên ⟹ giữ nguyên như cũ (dựng nút): thà một nút
    // có thể hỏng còn hơn im lặng nuốt mọi nút vì một cuốn sổ chưa kịp ghi.
    let paths: Vec<String> = match session_root(db, cfg, session_id) {
        Some(root) => {
            let kept: Vec<String> = paths
                .iter()
                .filter(|p| {
                    let expanded = match p.strip_prefix("~/") {
                        Some(rest) => std::env::var("HOME")
                            .map(|h| std::path::PathBuf::from(h).join(rest))
                            .unwrap_or_else(|_| std::path::PathBuf::from(p.as_str())),
                        None => std::path::PathBuf::from(p.as_str()),
                    };
                    let full = if expanded.is_absolute() {
                        expanded
                    } else {
                        root.join(&expanded)
                    };
                    full.is_file() && full.starts_with(&root)
                })
                .cloned()
                .collect();
            if kept.len() < paths.len() {
                logging::info(
                    "quick_files_filtered",
                    json!({ "kept": kept.len(), "seen": paths.len(),
                            "why": "tên nhắc trong câu văn, không phải tệp nằm trong thư mục phiên" }),
                );
            }
            kept
        }
        None => paths.to_vec(),
    };
    let paths = &paths[..];
    if paths.is_empty() {
        return Vec::new();
    }
    match serde_json::to_string(&json!({ "s": session_id, "p": paths })) {
        Ok(v) => {
            if let Err(e) = db.set_cursor(FILES_KEY, &v) {
                logging::error("quick_files_not_saved", json!({ "err": e.to_string() }));
                return Vec::new();
            }
        }
        Err(e) => {
            logging::error("quick_files_not_saved", json!({ "err": e.to_string() }));
            return Vec::new();
        }
    }
    // Nhãn phải PHÂN BIỆT được, không chỉ đọc được.
    //
    // 🔴 Cùng ảnh chụp ấy: hai nút cùng đọc là `Cargo.toml`. Lấy mỗi tên file
    // là bỏ đúng phần khác nhau — trong một cây mã thì `Cargo.toml`,
    // `index.html`, `mod.rs` trùng tên là chuyện thường, không phải ngoại lệ.
    // Trùng tên thì thêm thư mục cha, đủ để tách chứ không dán cả đường dẫn.
    let shown: Vec<String> = paths
        .iter()
        .take(4)
        .map(|p| {
            let name = p.rsplit('/').next().unwrap_or(p);
            let dup = paths
                .iter()
                .take(4)
                .filter(|q| q.rsplit('/').next().unwrap_or(q) == name)
                .count()
                > 1;
            if dup {
                let mut seg = p.rsplit('/');
                let last = seg.next().unwrap_or(p);
                match seg.next() {
                    Some(parent) => format!("{parent}/{last}"),
                    None => last.to_string(),
                }
            } else {
                name.to_string()
            }
        })
        .collect();
    shown
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            (
                format!("📎 {}", crate::exec::truncate(&name, 40)),
                format!("file:{i}"),
            )
        })
        .collect()
}

/// Đường dẫn số `n` trong sổ, kèm PHIÊN đã nhắc tới nó.
pub fn quick_file(db: &Db, n: usize) -> Option<(String, String)> {
    let v = db.cursor_or_log(FILES_KEY)?;
    let st: serde_json::Value = serde_json::from_str(&v).ok()?;
    let sid = st
        .get("s")
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();
    let path = st
        .get("p")
        .and_then(|p| p.as_array())
        .and_then(|a| a.get(n))
        .and_then(|p| p.as_str())?
        .to_string();
    Some((sid, path))
}

/// Cây thư mục MỘT PHIÊN được phép với tới — gốc workspace + thư mục dự án của nó.
///
/// Không tìm thấy phiên trong sổ ⟹ trả `None`, và chỗ gọi phải TỪ CHỐI. Rơi về
/// gốc workspace ở đây là biến "không biết phiên nào" thành "cho phép tất cả",
/// đúng kiểu hỏng-mở-toang mà `keys::look` đã trả giá một lần.
pub fn session_root(db: &Db, cfg: &Config, session_id: &str) -> Option<std::path::PathBuf> {
    let book = db.cursor_or_log(WATCH_KEY)?;
    let marks: std::collections::BTreeMap<String, crate::watch::Mark> =
        serde_json::from_str(&book).ok()?;
    let folder = marks.get(session_id).map(|m| m.d.clone())?;
    if folder.trim().is_empty() {
        return None;
    }
    Some(cfg.workspace_root.join(folder.trim_matches('/')))
}

pub fn remember_quick(db: &Db, session_id: &str, cmds: &[String]) -> Vec<(String, String)> {
    if cmds.is_empty() {
        return Vec::new();
    }
    // 🔴 Sổ phải nhớ PHIÊN NÀO đã sinh ra cái nút, không chỉ nhớ dòng lệnh.
    //
    // Hà 2026-08-13: *"Sao bấm nút được tạo phiên này lại gửi vào phiên đang
    // chọn thế"* — và bằng chứng rơi thẳng vào cuộc trò chuyện: một tin của
    // `[tfl5]` mang nút `▶ bash scripts/verify-acl-2026-08-13.sh`, anh bấm, và
    // dòng `!bash scripts/verify-acl-2026-08-13.sh` hiện ra trong phiên `[hub]`
    // — phiên đang được theo. Tệp ấy nằm ở `AI/tfl5/scripts/`, hub không có nó.
    //
    // Gốc: sổ này chỉ giữ MẢNG LỆNH, nên lúc bấm không còn gì để hỏi "của phiên
    // nào", và `/type` rơi về con trỏ focus. `remember_files` đã vá đúng lỗi
    // này cho nút 📎 sáng nay (*"giới hạn phiên nào chỉ nhận được file nằm
    // trong đúng thư mục của phiên đó"*) — một cuốn sổ được vá, cuốn bên cạnh
    // thì không, vì hai chỗ không ai bắt phải giống nhau.
    //
    // Con trỏ ĐỔI ĐƯỢC giữa lúc nút sinh ra và lúc nút được bấm: bấm "Xem đầy
    // đủ" là đổi, bấm một phiên khác là đổi. Nên buộc theo phiên đã SINH ra
    // nút, y như 📎.
    let payload = json!({ "s": session_id, "c": cmds });
    if let Ok(v) = serde_json::to_string(&payload) {
        if let Err(e) = db.set_cursor(QUICK_KEY, &v) {
            logging::error("quick_cmds_not_saved", json!({ "err": e.to_string() }));
            return Vec::new();
        }
    }
    // HAI nút cho mỗi lệnh, vì có hai chỗ chạy được và chúng KHÔNG thay nhau.
    //
    // 🔴 Hà 2026-08-13: *"với lệnh này chỉ chạy được trong terminal không chạy
    // được trong cli nên cần thêm cách tạo nút"*, kèm ảnh chụp lời một phiên
    // khác: *"`!` trong Claude Code không cấp tty, nên `ssh -t` không xin được
    // — không phải lỗi sudo hay script"*.
    //
    // `▶` gửi dòng lệnh vào chính phiên: phiên NHÌN THẤY kết quả rồi đi tiếp
    // được — đó là giá trị của nó, và phần lớn lệnh hợp ở đây. Nhưng phiên chạy
    // lệnh bằng công cụ của nó, KHÔNG có tty, nên `sudo`, `ssh -t`, `passwd`,
    // `read -s` chết ngay ở dòng hỏi mật khẩu. Thứ thiếu là cái tty.
    //
    // `🖥` mở một cửa sổ Terminal thật (`/win`) — đúng thứ chủ máy sẽ tự làm
    // khi ngồi trước máy. Đổi lại: kết quả nằm trên cửa sổ ấy, không về điện
    // thoại, và phải có người ngồi đó gõ.
    //
    // Không đoán hộ lệnh nào cần tty (một phép đoán ở đây là bấm nhầm rồi ngồi
    // chờ một lệnh đã chết): bày cả hai, chọn là việc của người bấm. Cắt còn 3
    // lệnh vì mỗi nút một hàng — 4 lệnh × 2 là tám hàng, dài hơn cả cái tin.
    cmds.iter()
        .enumerate()
        .take(3)
        .flat_map(|(i, c)| {
            // MỘT lệnh, MỘT nút, và nhãn đúng là lệnh ấy.
            //
            // 🔴 Hà 2026-08-13, ảnh chụp sáu nút dưới một tin: *"sao vẫn ra một
            // đống nút ở đây?"*, rồi nói thẳng cái cần: *"tôi đâu cần thông tin
            // chạy ở đâu làm gì, tôi chỉ cần biết nút đó chạy cái gì và hub phải
            // quản lý được đúng phiên đúng luồng"*.
            //
            // Hai nút mỗi lệnh là bắt người bấm chọn hộ một quyết định KỸ THUẬT
            // (chạy ở đâu) mà họ không có dữ kiện để chọn, và nó nhân đôi chiều
            // dài bảng phím. hub biết đường nào chạy được từ điện thoại
            // (`/runin`) nên hub chọn. Cần cửa sổ thật có tty thì gõ `/win`.
            // 🔴 Hà 2026-08-13: *"Nút chưa chèn vào đúng chỗ của nó"* · *"Bấm
            // vẫn chưa chạy được"*. Đo trong log: ba cú bấm
            // (16:29:39 · 16:30:55 · 16:31:26Z) đều xếp `/runin … ./hub
            // self-install`, và **không cú nào có dòng `runin_ran`** — trong
            // khi bản cài đổi lúc 16:31:37Z, tức lệnh CHẠY XONG. Nó chạy được;
            // thứ không về là lời báo.
            //
            // Gốc: lệnh ấy khởi động lại chính hubd, nên tiến trình đang xử lý
            // lệnh bị thay thế TRƯỚC khi kịp ghi log và gửi tin. Từ điện thoại
            // nhìn y hệt một cái nút hỏng — nên Hà bấm lại, và cài thêm hai
            // lần nữa. Đây là "lỗi im lặng" đúng nghĩa, chỉ khác chỗ: không
            // phải một `Err` bị nuốt, mà là **cái mồm bị giết giữa câu**.
            //
            // Đường đúng đã có sẵn: route `/upgrade` báo TRƯỚC rồi mới restart
            // (`CommandKind::Upgrade`). Nên nút phải trỏ vào đó — đúng nghĩa
            // "chèn vào đúng chỗ của nó".
            if is_self_rebuild(c) {
                return [(
                    format!("🔧 {}", crate::exec::truncate(c, 52)),
                    "upgrade".to_string(),
                )];
            }
            [(
                format!("▶ {}", crate::exec::truncate(c, 52)),
                format!("run:{i}"),
            )]
        })
        .collect()
}

/// Cắt tin thành từng mẩu, mỗi mẩu kết thúc NGAY SAU dòng lệnh của nó.
///
/// 🔴 Hà 2026-08-14: *"nút chạy lệnh chỉ cần 1 icon là đủ chèn ngay sau câu
/// lệnh"*. Telegram không đặt nút giữa chữ được — `inline_keyboard` luôn treo
/// dưới đáy MỘT tin (xem `telegram::keyboard_rows`). Nhưng "dưới đáy một tin"
/// là thứ điều khiển được: cắt tin ngay sau dòng lệnh thì cái nút rơi đúng chỗ
/// Hà muốn, và lúc ấy nhãn không cần nhắc lại dòng lệnh nữa — dòng lệnh đang
/// nằm ngay trên đầu nó, nguyên văn, không bị cắt còn 52 ký tự.
///
/// `Some(i)` = mẩu này kết bằng lệnh thứ `i`; `None` = mẩu đuôi, mang nốt các
/// nút khác (vào phiên, mở tệp, xem đầy đủ).
///
/// Mỗi lệnh khớp ĐÚNG MỘT LẦN, ở dòng đầu tiên chứa nó: một báo cáo hay nhắc
/// lại cùng một lệnh ở phần tóm tắt, và hai cái nút giống hệt nhau cho cùng
/// một việc là mời người ta bấm hai lần.
/// Dòng này có mang lệnh ấy không — kể cả khi cửa sổ đã bẻ nó làm đôi.
///
/// 🔴 Hà 2026-08-14: *"Rõ ràng là 1 dòng sao lại biến thành 2"*. Đúng, và đó là
/// cái giá của việc đọc chữ **hiển thị** (`contents of selected tab`) thay vì
/// chữ **gốc**: Terminal bẻ dòng theo bề ngang cửa sổ, nên một lệnh dài 83 ký
/// tự nằm trên một cửa sổ rộng 80 về tới đây là hai dòng. `commands_on_screen`
/// nối chúng lại, nên `cmds` mang bản ĐẦY ĐỦ — còn `text` thì vẫn là chữ đã bị
/// bẻ. So nguyên chuỗi ở đây là so bản đầy đủ với một nửa, và nó trượt: mẩu
/// không cắt được, icon rơi về khối nút ở đáy, tức đúng thứ vừa bị chê.
///
/// Khớp theo PHẦN ĐẦU, đủ dài để không nhầm hai lệnh khác nhau (12 ký tự trở
/// lên; `git -C /User…` đã vượt).
fn line_carries(line: &str, cmd: &str) -> bool {
    let c = cmd.trim();
    if line.contains(c) {
        return true;
    }
    // Cắt theo ranh giới ký tự, không theo byte: đường dẫn có dấu tiếng Việt là
    // chuyện có thật trong workspace này.
    let head: String = c.chars().take(40).collect();
    head.chars().count() >= 12 && line.contains(&head)
}

pub fn command_slices(text: &str, cmds: &[String]) -> Vec<(String, Option<usize>)> {
    let mut out: Vec<(String, Option<usize>)> = Vec::new();
    let mut buf: Vec<&str> = Vec::new();
    let mut used = vec![false; cmds.len()];
    for line in text.lines() {
        buf.push(line);
        let hit = cmds
            .iter()
            .enumerate()
            .find(|(i, c)| !used[*i] && !c.trim().is_empty() && line_carries(line, c));
        if let Some((i, _)) = hit {
            used[i] = true;
            out.push((buf.join("\n"), Some(i)));
            buf.clear();
        }
    }
    if !buf.is_empty() {
        let tail = buf.join("\n");
        // Đuôi chỉ toàn dòng trống thì không đáng một tin riêng — dán vào mẩu
        // trước, không thì điện thoại kêu thêm một tiếng cho một tin rỗng.
        if tail.trim().is_empty() {
            if let Some(last) = out.last_mut() {
                last.0.push('\n');
                last.0.push_str(&tail);
                return out;
            }
        }
        out.push((tail, None));
    }
    out
}

/// Trần chữ cho MỘT tin Telegram. Luật của Telegram là 4096; chừa lại chỗ cho
/// cái icon và thẻ `<a>` bọc nó.
const TG_TEXT_MAX: usize = 3500;

/// Trần AN TOÀN cho một lệnh chạy nền: một giờ.
///
/// Không phải "thời gian một lệnh được phép chạy" — đó là câu hỏi không ai trả
/// lời đúng được từ trước, và trần 120 giây cũ chính là một câu trả lời sai.
/// Đây là cái phanh cuối: một tiến trình treo một tiếng thì nó treo thật, và bỏ
/// nó chạy tới sáng là bỏ lại một thứ đang giữ tài nguyên mà không ai nhớ.
const LONG_JOB_MAX_SEC: u64 = 3600;

/// Bao lâu thì nhắc một lần rằng lệnh vẫn đang chạy.
const LONG_JOB_TICK_SEC: u64 = 90;

/// Việc đang chạy nền, để **theo dõi và dừng được** — thứ Hà đòi thay cho một
/// con số timeout.
#[derive(Debug, Clone)]
struct Job {
    n: usize,
    pid: u32,
    line: String,
    session: String,
    started: std::time::Instant,
}

static JOBS: std::sync::Mutex<Vec<Job>> = std::sync::Mutex::new(Vec::new());
static JOB_SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Dừng một việc đang chạy. `Err` mang câu nói ra được cho người bấm.
///
/// Giết cả NHÓM tiến trình, không riêng đứa con: `zsh -lc "bash deploy.sh"` đẻ
/// tiếp cháu chắt, và giết mỗi `zsh` để lại nguyên đàn phía dưới — cùng bài học
/// đã trả giá với `claude` (xem `exec::kill_group`).
pub fn stop_job(n: usize) -> Result<String, String> {
    let job = {
        let jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
        jobs.iter().find(|j| j.n == n).cloned()
    };
    let Some(job) = job else {
        return Err("việc ấy đã xong hoặc đã dừng rồi".to_string());
    };
    let out = std::process::Command::new("/bin/kill")
        .args(["-TERM", &format!("-{}", job.pid)])
        .output()
        .map_err(|e| e.to_string())?;
    logging::info(
        "long_job_stop_asked",
        json!({ "n": n, "pid": job.pid, "ok": out.status.success(),
                "cmd": crate::exec::truncate(&job.line, 120) }),
    );
    Ok(format!(
        "⏹ đã bảo dừng: {}",
        crate::exec::truncate(&job.line, 100)
    ))
}

/// Danh sách việc đang chạy, cho `/jobs` và cho câu trả lời khi có người hỏi.
pub fn jobs_line() -> Option<String> {
    let jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
    if jobs.is_empty() {
        return None;
    }
    Some(
        jobs.iter()
            .map(|j| {
                format!(
                    "#{} · {}s · [{}] {}",
                    j.n,
                    j.started.elapsed().as_secs(),
                    j.session.chars().take(8).collect::<String>(),
                    crate::exec::truncate(&j.line, 60)
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Chạy một lệnh ở luồng riêng, theo dõi nó, rồi báo lại — thay cho việc ngồi
/// chờ tới một cái trần.
///
/// 🔴 Hà 2026-08-14: *"Có những lệnh sẽ chạy khá lâu nên cần cơ chế theo dõi
/// riêng thay vì cố định timeout"*.
///
/// Ba việc, và cái thứ ba mới là thứ trước đây thiếu:
/// 1. **Chạy**, với cái phanh cuối một tiếng (`LONG_JOB_MAX_SEC`) — chạm phanh
///    thì NÓI rõ là bị dừng vì quá lâu, không lẫn với "lệnh chạy xong".
/// 2. **Báo lại** khi xong: cùng báo cáo, cùng đường dán vào phiên, cùng cổng
///    quét bí mật như đường cũ.
/// 3. **Theo dõi trong lúc chạy**: mỗi 90 giây nhắc một câu kèm nút ⏹ dừng.
///    Không có bước này thì "bỏ trần" chỉ đổi một cái chết ồn ào thành một sự
///    im lặng dài — mà im lặng dài thì người ta bấm lại lần nữa, và lần thứ hai
///    là một lệnh triển khai chạy hai lần.
fn watch_long_job(
    cfg: Config,
    s: crate::sessions::LiveSession,
    root: std::path::PathBuf,
    line: String,
    adapter: String,
    chat_id: String,
) {
    let n = JOB_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    // Bản sao cho nhánh "không dựng được luồng": luồng nuốt bản gốc, mà đúng ca
    // ấy mới cần nói ra là lệnh KHÔNG chạy.
    let (fb_cfg, fb_adapter, fb_chat) = (cfg.clone(), adapter.clone(), chat_id.clone());
    let spawned = std::thread::Builder::new()
        .name(format!("long-job-{n}"))
        .spawn(move || {
            // Việc này do một ngón tay gây ra ⟹ hạng gấp (xem `exec::Lane`).
            let _lane = crate::exec::urgent();
            let (tx, rx) = std::sync::mpsc::channel::<u32>();
            let ticker_line = line.clone();
            let ticker_cfg = cfg.clone();
            let ticker_adapter = adapter.clone();
            let ticker_chat = chat_id.clone();
            // Người báo tin: sống bằng cách hỏi cuốn sổ, nên nó tự tắt đúng lúc
            // việc rời sổ — không cần thêm một kênh thứ hai để bảo nó dừng.
            let ticker = std::thread::Builder::new()
                .name(format!("long-job-tick-{n}"))
                .spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(LONG_JOB_TICK_SEC));
                    let still = {
                        let jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
                        jobs.iter().find(|j| j.n == n).map(|j| j.started.elapsed())
                    };
                    match still {
                        Some(el) => {
                            // Nút ⏹ đi KÈM câu nhắc, không bắt nhớ một động từ:
                            // `/stop` đã là route dừng PHIÊN, nên một lệnh chữ
                            // ở đây vừa trùng tên vừa mời gõ nhầm thứ đáng sợ
                            // hơn hẳn — dừng cả phiên thay vì dừng một lệnh.
                            let text = format!(
                                "⏳ vẫn đang chạy ({} phút) — {}",
                                el.as_secs() / 60,
                                crate::exec::truncate(&ticker_line, 100),
                            );
                            match (
                                ticker_adapter == crate::telegram::NAME,
                                crate::telegram::inbox(),
                            ) {
                                (true, Some(tg)) => {
                                    if let Err(e) = tg.send_buttons(
                                        &text,
                                        &[("⏹ dừng lệnh này".to_string(), format!("stopjob:{n}"))],
                                    ) {
                                        logging::error("telegram_ack_failed", json!({ "err": e }));
                                    }
                                }
                                _ => say_back(&ticker_cfg, &ticker_adapter, &ticker_chat, &text),
                            }
                        }
                        None => break,
                    }
                });
            if let Err(e) = &ticker {
                // Mất người báo tin thì việc vẫn chạy — chỉ là chạy im. Nói ra.
                logging::warn("long_job_ticker_failed", json!({ "n": n, "err": e.to_string() }));
            }
            let watcher = {
                let line = line.clone();
                let session = s.session_id.clone();
                std::thread::Builder::new()
                    .name(format!("long-job-pid-{n}"))
                    .spawn(move || {
                        if let Ok(pid) = rx.recv() {
                            let mut jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
                            jobs.push(Job {
                                n,
                                pid,
                                line,
                                session,
                                started: std::time::Instant::now(),
                            });
                            logging::info("long_job_started", json!({ "n": n, "pid": pid }));
                        }
                    })
            };
            if let Err(e) = &watcher {
                logging::warn("long_job_book_failed", json!({ "n": n, "err": e.to_string() }));
            }
            let out = crate::exec::run(
                "/bin/zsh",
                &["-lc", &line],
                crate::exec::RunOpts {
                    cwd: Some(root.as_path()),
                    timeout: Some(std::time::Duration::from_secs(LONG_JOB_MAX_SEC)),
                    pid_out: Some(tx),
                    ..Default::default()
                },
            );
            {
                let mut jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
                jobs.retain(|j| j.n != n);
            }
            let ack = match out {
                Ok(r) => {
                    logging::info(
                        "runin_ran",
                        json!({ "session": s.session_id, "code": r.code,
                                "timed_out": r.timed_out, "ms": r.ms, "n": n,
                                "cmd": crate::exec::truncate(&line, 120) }),
                    );
                    let report = cmd_report(r.code, r.timed_out, &r.stdout, &r.stderr, r.ms);
                    let block = format!(
                        "[hub đã chạy hộ lệnh này trên máy — cwd {}, KHÔNG có tty]\n$ {}\n{}",
                        root.display(),
                        line,
                        report
                    );
                    // Đầu ra nằm lại trong NHẬT KÝ phiên, tức trên đĩa mãi mãi —
                    // gác giá trị bí mật trước khi nó vào đó.
                    let risk = crate::redaction::file_risk(&block);
                    if !risk.is_empty() {
                        format!(
                            "🔒 Lệnh chạy xong nhưng hub GIỮ LẠI kết quả, không dán vào phiên: có dấu hiệu bí mật ({}). Xem trên máy.",
                            risk.join(", ")
                        )
                    } else {
                        match crate::keys::window_of(&s.tty) {
                            Ok(Some(w)) => match crate::keys::type_into(w, &block, true) {
                                Ok(()) => {
                                    for wait_ms in [400u64, 1000] {
                                        std::thread::sleep(std::time::Duration::from_millis(
                                            wait_ms,
                                        ));
                                        let _ = crate::keys::press(w, "enter");
                                    }
                                    format!(
                                        "✅ Đã chạy trên máy rồi dán kết quả vào {}:\n$ {}\n{}",
                                        crate::sessions::shown(&s),
                                        line,
                                        crate::exec::truncate(&report, 400)
                                    )
                                }
                                Err(e) => format!(
                                    "⚠ chạy xong nhưng KHÔNG dán được vào phiên: {}\n\n$ {}\n{}",
                                    crate::exec::truncate(&e.to_string(), 160),
                                    line,
                                    crate::exec::truncate(&report, 600)
                                ),
                            },
                            _ => format!(
                                "⚠ phiên {} không có cửa sổ terminal để dán vào. Kết quả:\n\n$ {}\n{}",
                                crate::sessions::shown(&s),
                                line,
                                crate::exec::truncate(&report, 600)
                            ),
                        }
                    }
                }
                Err(e) => {
                    logging::error(
                        "runin_failed",
                        json!({ "err": e.to_string(), "n": n,
                                "cmd": crate::exec::truncate(&line, 120) }),
                    );
                    format!(
                        "⚠ không chạy được: {}",
                        crate::exec::truncate(&e.to_string(), 200)
                    )
                }
            };
            say_back(&cfg, &adapter, &chat_id, &ack);
        });
    if let Err(e) = spawned {
        // Không dựng được luồng thì lệnh KHÔNG chạy — và câu này phải tới được
        // người bấm, không chỉ nằm trong log.
        logging::error("long_job_spawn_failed", json!({ "err": e.to_string() }));
        say_back(
            &fb_cfg,
            &fb_adapter,
            &fb_chat,
            "⚠ không dựng được luồng chạy nền — lệnh KHÔNG chạy.",
        );
    }
}

/// Mở một phiên mới ở luồng riêng, rồi báo lại khi nó chào đời.
///
/// 🔴 Hà 2026-08-14: *"kiểm tra lệnh new đi đã chạy được cơ chế mới chưa"*.
/// Chưa — và ba lượt gần nhất đo được **64,7s · 39,4s · 61,5s** ngồi chờ trong
/// khi giữ `CMD_LOCK`. Thời gian ấy là việc thật (chờ nhật ký phiên sinh ra để
/// biết id, rồi bấm hộ hộp tin-thư-mục nếu cần), nhưng chỗ ngồi chờ thì sai.
///
/// Hai tin, vì có hai sự kiện thật: "đã bấm mở" và "phiên đã sống". Con trỏ
/// theo dõi chỉ chuyển ở tin thứ hai — lúc ấy mới có id để mà chuyển.
fn watch_new_session(
    cfg: Config,
    name: String,
    dir: std::path::PathBuf,
    task: String,
    account: Option<String>,
    adapter: String,
    chat_id: String,
) {
    let (fb_cfg, fb_adapter, fb_chat) = (cfg.clone(), adapter.clone(), chat_id.clone());
    let spawned = std::thread::Builder::new()
        .name("new-session".into())
        .spawn(move || {
            let _lane = crate::exec::urgent();
            let started =
                crate::sessions::start_background(&cfg, &name, &dir, &task, account.as_deref());
            let ack = match started {
                Ok(s) => {
                    // Sổ sách trước, câu chào sau: một câu nói "đang theo phiên
                    // này" mà sổ chưa ghi là câu làm người ta gõ việc vào nhầm
                    // chỗ.
                    match Db::open(&cfg.db) {
                        Ok(db) => {
                            remember_started(&db, &s.session_id);
                            if let Err(e) = db.set_cursor(FOCUS_SESSION_KEY, &s.session_id) {
                                logging::error(
                                    "focus_after_start_failed",
                                    json!({ "err": e.to_string() }),
                                );
                            }
                        }
                        Err(e) => logging::error(
                            "new_session_db_failed",
                            json!({ "err": e.to_string(),
                                    "why": "phiên đã mở nhưng sổ chưa ghi — con trỏ chưa chuyển" }),
                        ),
                    }
                    logging::info(
                        "session_started",
                        json!({ "project": s.project, "session": s.session_id, "cwd": s.cwd }),
                    );
                    let cua_so = if s.window {
                        "⌨ cửa sổ Terminal"
                    } else {
                        "🌙 phiên nền"
                    };
                    format!(
                        "🚀 Đã mở {} cho {}.\nPhiên {} — đang chạy trên máy.\n\n🎯 Nay đang theo phiên này: gõ thẳng câu hỏi ở đây là vào nó.\n⚠ Nó chạy không hỏi ai. Tắt bằng /stop.",
                        cua_so,
                        s.project,
                        &s.session_id[..8.min(s.session_id.len())]
                    )
                }
                // Không cắt 200 như các ack khác: lời báo hỏng ở đây MANG THEO
                // cách gỡ, và cắt 200 chặt đúng nửa đó — người đọc nhận được
                // tin xấu mà không nhận được lối ra.
                Err(e) => format!(
                    "⚠ không mở được phiên: {}",
                    crate::exec::truncate(&e.to_string(), 700)
                ),
            };
            say_back(&cfg, &adapter, &chat_id, &ack);
        });
    if let Err(e) = spawned {
        logging::error("new_session_spawn_failed", json!({ "err": e.to_string() }));
        say_back(
            &fb_cfg,
            &fb_adapter,
            &fb_chat,
            "⚠ không dựng được luồng mở phiên — KHÔNG có cửa sổ nào được mở.",
        );
    }
}

/// Nói một câu về kênh đã gõ lệnh, từ một luồng không cầm `ChannelCommand`.
fn say_back(_cfg: &Config, adapter: &str, _chat_id: &str, text: &str) {
    if adapter == crate::telegram::NAME {
        if let Some(i) = crate::telegram::inbox() {
            if let Err(e) = i.send_text(text) {
                logging::error("telegram_ack_failed", json!({ "err": e }));
            }
        }
        return;
    }
    logging::info(
        "long_job_ack_dropped",
        json!({ "adapter": adapter, "ack": text }),
    );
}

/// Gửi một tin CÓ LỆNH TRONG CHỮ: cắt ngay sau mỗi dòng lệnh, dán icon ▶️ chạm
/// được vào cuối dòng ấy, các nút còn lại treo dưới mẩu cuối.
///
/// 🔴 Hà 2026-08-14, hai câu cách nhau vài giờ và cùng một yêu cầu:
/// *"thêm 1 cái icon để bấm chạy bên trong text chỗ cuối dòng lệnh"*, rồi khi
/// bản ĐẦY ĐỦ vẫn ra khối nút ở đáy: *"Có lệnh bash sao lại ko có nút bấm chạy
/// cho nó … gắn icon bấm được ngay sau chuỗi lệnh đó"*.
///
/// Vì sao hàm này tồn tại thay vì hai bản chép: máy móc icon-trong-chữ được
/// dựng cho tin TỰ PHÁT (`announce_changes`), còn bản đầy đủ (`full:<n>`) đi
/// một đường riêng ra `send_text` — nên đúng lúc người ta bấm "Xem đầy đủ" để
/// ĐỌC KỸ, tức đúng lúc dòng lệnh hiện ra nguyên vẹn, thì đường bấm chạy lại
/// nghèo hơn tin rút gọn. Đó là lần thứ hai cùng một cuốn sổ được vá ở một chỗ
/// và bỏ quên ở chỗ bên cạnh (lần đầu: `remember_files` cho nút 📎).
///
/// Icon nằm giữa chữ thì không thể là nút — bàn phím Telegram luôn treo dưới
/// đáy tin. Thứ đặt được vào giữa chữ là một LIÊN KẾT: deep link về chính bot
/// (`t.me/<bot>?start=run_0`). Chưa biết tên bot ⟹ rơi về nút, vì một liên kết
/// không bấm được thì tệ hơn một cái nút.
pub fn say_with_command_icons(
    tg: &crate::telegram::Inbox,
    text: &str,
    cmds: &[String],
    buttons: &[(String, String)],
    log_key: &str,
) {
    let slices = command_slices(text, cmds);
    let is_cmd_btn = |d: &str| d.starts_with("run:") || d == "upgrade";
    let cmd_btns: Vec<(String, String)> = buttons
        .iter()
        .filter(|(_, d)| is_cmd_btn(d))
        .cloned()
        .collect();
    let rest_btns: Vec<(String, String)> = buttons
        .iter()
        .filter(|(_, d)| !is_cmd_btn(d))
        .cloned()
        .collect();
    let inline = slices.iter().any(|(_, i)| i.is_some());
    if !inline {
        // Không có lệnh nào trong chữ: một tin, nút như cũ — nhưng vẫn phải
        // chia nếu quá dài, vì Telegram từ chối cả tin chứ không cắt hộ.
        let parts = split_for_telegram(text);
        let last = parts.len().saturating_sub(1);
        for (n, p) in parts.into_iter().enumerate() {
            let sent = if n == last && !buttons.is_empty() {
                tg.send_buttons(&p, buttons)
            } else {
                tg.send_text(&p)
            };
            if let Err(e) = sent {
                logging::error(log_key, json!({ "err": e, "slice": n }));
            }
        }
        return;
    }
    let last = slices.len().saturating_sub(1);
    for (n, (part, idx)) in slices.into_iter().enumerate() {
        let mut row: Vec<(String, String)> = Vec::new();
        // ICON TRONG CHỮ: dựng liên kết cho lệnh của mẩu này. `run:<i>` →
        // payload `run_<i>`; nút cài lại hub → `upgrade`. Cùng bộ ký tự mà tên
        // lệnh cho phép, nên payload đi thẳng, không mã hoá gì thêm.
        let inline_icon = idx.and_then(|i| cmd_btns.get(i)).and_then(|(_, data)| {
            let payload = if data == "upgrade" {
                "upgrade".to_string()
            } else {
                data.replace("run:", "run_")
            };
            let icon = if data == "upgrade" { "🔧" } else { "▶️" };
            crate::telegram::deep_link(&payload).map(|href| (href, icon, data.clone()))
        });
        if n == last {
            row.extend(rest_btns.clone());
        }
        if inline_icon.is_none() {
            if let Some((_, data)) = idx.and_then(|i| cmd_btns.get(i)) {
                let icon = if data == "upgrade" { "🔧" } else { "▶" };
                row.insert(0, (icon.to_string(), data.clone()));
            }
        }
        // 🔴 Đo phép đo Ở ĐÚNG CHỖ TELEGRAM ĐO: `strip_markdown` bỏ hẳn dòng
        // rào ```, nên một mẩu chỉ chứa rào (chuyện thường khi cắt ngay sau một
        // dòng lệnh nằm trong khối code) trông đầy chữ ở đây mà tới Telegram là
        // RỖNG. Đo được ngay lượt chạy thật đầu tiên, 2026-08-14 08:58:32:
        // `telegram_ack_failed {"err":"Bad Request: message text is empty",
        // "slice":1}`. Kiểm `part.trim()` là kiểm chữ hub cầm, không phải chữ
        // Telegram nhận.
        let shown = crate::telegram::strip_markdown(&part);
        if shown.trim().is_empty() && inline_icon.is_none() {
            // Không còn chữ nào, nhưng nút thì vẫn phải giao — bỏ luôn cả nút
            // là đánh rơi đường bấm, đúng loại hỏng im lặng luật 3 cấm.
            if row.is_empty() {
                continue;
            }
            if let Err(e) = tg.send_buttons("⤵", &row) {
                logging::error(log_key, json!({ "err": e, "slice": n }));
            }
            continue;
        }
        // Một mẩu dài hơn trần thì đi làm nhiều tin, và **icon bám mẩu CUỐI** —
        // đó là mẩu chứa dòng lệnh (xem `command_slices`). Gắn vào mẩu đầu là
        // đặt cái icon cách dòng lệnh của nó vài màn hình.
        let mut chunks = split_for_telegram(&shown);
        let tail = chunks.pop().unwrap_or_default();
        for (k, head) in chunks.into_iter().enumerate() {
            if let Err(e) = tg.send_text(&head) {
                logging::error(log_key, json!({ "err": e, "slice": n, "chunk": k }));
            }
        }
        let sent = match &inline_icon {
            Some((href, icon, _)) => {
                let html = format!(
                    "{} <a href=\"{}\">{}</a>",
                    crate::telegram::html_escape(&tail),
                    crate::telegram::html_escape(href),
                    icon
                );
                if row.is_empty() {
                    tg.send_html(&html)
                } else {
                    // Mẩu cuối vẫn còn nút khác (vào phiên, mở tệp): gửi
                    // chữ+icon trước, nút theo sau.
                    let r = tg.send_html(&html);
                    let _ = tg.send_buttons("⤵", &row);
                    r
                }
            }
            None => tg.send_buttons(&tail, &row),
        };
        if let Err(e) = sent {
            logging::error(log_key, json!({ "err": e, "slice": n }));
        }
    }
}

/// Cắt chữ cho vừa MỘT tin Telegram — theo DÒNG, để không đứt giữa câu.
///
/// Luôn trả về ít nhất một mẩu (có thể rỗng), nên chỗ gọi `pop()` được mà không
/// phải kiểm rỗng.
pub fn split_for_telegram(text: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut chunk = String::new();
    for line in text.lines() {
        if !chunk.is_empty() && chunk.len() + line.len() + 1 > TG_TEXT_MAX {
            parts.push(std::mem::take(&mut chunk));
        }
        chunk.push_str(line);
        chunk.push('\n');
    }
    parts.push(chunk);
    parts
}

/// Bảng hỏi viết thành CHỮ CHẠM ĐƯỢC, mỗi lựa chọn một lệnh tự tô sáng.
///
/// 🔴 Hà 2026-08-14: *"Sao không dùng Deep Links để định dạng bên trong nội
/// dung văn bản như khối lệnh thay vì tạo 1 cái nút rất khó hiểu"* · *"Hạn chế
/// dùng khối nút ở cuối tin"*.
///
/// Tài liệu Bot API, mục *Commands*: *"Highlight commands in messages. When the
/// user taps a highlighted command, that command is immediately sent again."*
/// Nên một lựa chọn không cần cái nút nào cả — nó chỉ cần được VIẾT RA đúng chỗ
/// nó thuộc về, ngay dưới câu hỏi của nó. Khối nút ở cuối tin bắt người đọc tự
/// ghép "nút nào ứng với đoạn nào", và đó đúng là chỗ hôm nay đẻ ra hai nút
/// "Làm đi" chồng nhau mà không ai biết cái nào là cái nào.
///
/// Tham số nằm trong TÊN lệnh vì chạm chỉ gửi lại token lệnh — chữ sau dấu cách
/// rơi mất. `pick_<8 ký tự đầu id>_<câu>_<lựa chọn>` = 17 ký tự, dưới trần 32.
pub fn ask_command_lines(session_id: &str, a: &crate::sessions::Asking) -> String {
    let sid: String = session_id.chars().take(8).collect();
    if sid.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let all = std::iter::once((
        a.header.clone(),
        a.question.clone(),
        a.options.clone(),
        a.multi,
    ))
    .chain(a.rest.iter().map(|q| {
        (
            q.header.clone(),
            q.question.clone(),
            q.options.clone(),
            q.multi,
        )
    }));
    for (qi, (header, question, options, multi)) in all.enumerate() {
        if options.is_empty() {
            continue;
        }
        let head = if header.is_empty() {
            question.clone()
        } else {
            header
        };
        out.push_str(&format!(
            "\n\n▸ Câu {} — {}{}",
            qi + 1,
            crate::exec::truncate(&head, 60),
            if multi { " (CHỌN NHIỀU)" } else { "" }
        ));
        for (oi, label) in options.iter().take(9).enumerate() {
            out.push_str(&format!(
                "\n/pick_{sid}_{}_{} {}",
                qi + 1,
                oi + 1,
                crate::exec::truncate(label, 60)
            ));
        }
    }
    if !out.is_empty() {
        // 🔴 Dòng này từng viết `/key <id> enter` — và Hà chạm đúng vào nó lúc
        // 09:06, Telegram gửi mỗi `/key`, hub trả *"Chưa hiểu lệnh này"*. Tôi
        // tự viết ra luật "chạm chỉ gửi lại token lệnh, chữ sau dấu cách rơi
        // mất" ở ngay tệp bên cạnh, rồi dẫm đúng vào nó ở dòng này.
        //
        // Tham số phải nằm TRONG tên: `/send_<8 ký tự đầu id>`.
        out.push_str(&format!("\n\nTrả lời hết rồi gửi: /send_{sid}"));
    }
    out
}

/// Dòng lệnh này có phải là "hub dựng lại chính hub" không?
///
/// Hàng rào HẸP có chủ ý — đây là danh sách hai đường duy nhất cài lại hubd
/// trên máy này (`./hub self-install` là bản Rust, `deploy/install.sh` là bản
/// shell nó thay thế). Nới rộng bằng cách bắt mọi thứ có chữ "install" thì
/// `npm install` cũng thành "dựng lại hub", và người bấm nhận một câu trả lời
/// nói về chuyện khác hẳn.
pub fn is_self_rebuild(cmd: &str) -> bool {
    let c = cmd.trim();
    c.contains("hub self-install") || c.contains("deploy/install.sh")
}

/// Lệnh gợi ý thứ `n`, kèm PHIÊN đã sinh ra nó — cái nút chỉ mang con số.
///
/// Trả `None` khi sổ cũ (dạng mảng trần, chưa có tên phiên): thà bắt bấm lại
/// `/shot` còn hơn gõ một dòng lệnh vào một phiên đoán bừa.
pub fn quick_cmd(db: &Db, n: usize) -> Option<(String, String)> {
    let v = db.cursor_or_log(QUICK_KEY)?;
    let st: serde_json::Value = serde_json::from_str(&v).ok()?;
    let sid = st.get("s")?.as_str()?.to_string();
    let cmd = st.get("c")?.as_array()?.get(n)?.as_str()?.to_string();
    Some((sid, cmd))
}

/// Chữ ĐANG HIỆN trên màn một phiên — thứ `/shot` trả về, và thứ đi kèm khi
/// bấm một phiên trên Telegram.
///
/// Trả CHỮ chứ không phải ảnh (Hà 2026-08-10): ảnh chỉ để nhìn, còn cái cần là
/// biết nó đang hỏi gì rồi bấm số trả lời ngay.
///
/// Dãy phím đưa con trỏ từ câu `from` sang câu `to` rồi bấm lựa chọn `opt`.
///
/// Tách ra khỏi phần I/O để **kiểm được bằng test**: cái sai đắt nhất ở đây là
/// đi nhầm một tab — nó chốt một lựa chọn cho câu người ta chưa đọc, và không
/// lùi lại được. Số phím mũi tên là số học thuần, nên nó phải đứng chỗ mà test
/// nhìn thấy, không lẫn trong một hàm cần cửa sổ Terminal thật mới chạy.
pub fn pick_keys(from: usize, to: usize, opt: usize) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let dir = if to > from { "right" } else { "left" };
    for _ in 0..to.abs_diff(from) {
        keys.push(dir.to_string());
    }
    keys.push(opt.to_string());
    keys
}

/// `/pick <câu>.<lựa chọn>` — trả lời MỘT câu bất kỳ của bảng hỏi nhiều câu.
///
/// 🔴 Hà 2026-08-13: *"chọn option xong thì vẫn còn bước nữa nên không pass qua
/// được"*. Đường cũ (`/key <số>`) gửi số vào câu ĐANG MỞ; bảng nhiều câu thì
/// các câu sau nằm sau một phím mũi tên, mà mũi tên trần bị `arrow_verdict` từ
/// chối — đúng luật, vì `do script` kèm dấu xuống dòng nên mũi tên vừa di vừa
/// CHỐT. Ở đây cả dãy đi trong MỘT `do script`, nên dấu xuống dòng chỉ có một,
/// nằm sau con số — tức nó chốt đúng cái vừa chọn, không chốt hộ dọc đường.
///
/// Ba điều hàm này KHÔNG làm, và mỗi điều là một cái bẫy đã biết:
/// * **Không đếm phím đã bấm để đoán vị trí** — chủ máy có thể vừa tự bấm một
///   cái trên bàn phím. Vị trí đọc từ MÀN mỗi lần (`keys::cursor_on`).
/// * **Không gõ khi mù** — `Withheld`/`Blind` thì dừng và nói vì sao. Gõ vào
///   một màn không đọc được là chốt bừa vào việc của người khác.
/// * **Không tin mã trả về** — `osascript` trả 0 chỉ chứng minh byte vào tới
///   tab. Câu trả lời dựng từ việc ĐỌC LẠI bảng và so số ô trống trước/sau.
fn pick_answer(s: &crate::sessions::LiveSession, w: i64, arg: &str) -> String {
    let name = crate::sessions::shown(s);
    let (q_txt, opt_txt) = match arg.trim().split_once('.') {
        Some(p) => p,
        None => {
            return format!(
                "⚠ `/pick` cần dạng `<câu>.<lựa chọn>`, ví dụ `/pick 2.1` — nhận được '{}'",
                crate::exec::truncate(arg.trim(), 40)
            )
        }
    };
    let (Ok(q), Ok(opt)) = (
        q_txt.trim().parse::<usize>(),
        opt_txt.trim().parse::<usize>(),
    ) else {
        return format!("⚠ `/pick` cần hai con số, ví dụ `/pick 2.1` — nhận được '{arg}'");
    };
    if q == 0 || opt == 0 || opt > 9 {
        return "⚠ số câu và số lựa chọn đếm từ 1, và lựa chọn tối đa là 9".to_string();
    }

    // Câu hỏi lấy từ NHẬT KÝ (đủ chữ, không phụ thuộc bề ngang cửa sổ); vị trí
    // con trỏ lấy từ MÀN. Hai nguồn, mỗi nguồn trả lời đúng phần nó biết.
    let asking = s.asking.clone().unwrap_or_default();
    let questions: Vec<String> = std::iter::once(asking.question.clone())
        .chain(asking.rest.iter().map(|r| r.question.clone()))
        .collect();

    let body = match crate::keys::look(&s.tty, PICK_LINES) {
        crate::keys::Look::Saw { body, .. } => body,
        crate::keys::Look::Withheld { risk, .. } => {
            logging::info(
                "pick_refused_withheld",
                json!({ "session": s.session_id, "risk": risk }),
            );
            return format!(
                "⚠ Màn của {name} có dấu hiệu bí mật nên hub không đọc được chữ — mà không đọc \
                 được thì KHÔNG biết đang đứng ở câu nào, và bấm bừa là chốt hộ Hà một lựa chọn \
                 không lùi lại được. Trả lời trên máy, hoặc `/shot` để tự nhìn."
            );
        }
        crate::keys::Look::Blind { why } => {
            logging::warn(
                "pick_refused_blind",
                json!({ "session": s.session_id, "why": why }),
            );
            return format!("⚠ Không đọc được màn của {name} ({why}) — nên tôi KHÔNG bấm gì cả.");
        }
    };

    let table = crate::keys::ask_table(&body);
    let total = table.as_ref().map(|t| t.answered.len()).unwrap_or(1);
    if q > total {
        return format!(
            "⚠ Bảng của {name} có {total} câu, không có câu {q}. (Bảng một câu thì dùng `/key`.)"
        );
    }
    // Không có thanh tab ⟹ bảng MỘT câu ⟹ không có gì để đi tới; `/pick 1.x`
    // vẫn chạy được và trùng đúng nghĩa với `/key x`.
    let cursor = match crate::keys::cursor_on(&body, &questions) {
        Some(c) => c,
        None if total == 1 => 0,
        None => {
            logging::info(
                "pick_cursor_unknown",
                json!({ "session": s.session_id, "questions": questions.len(), "total": total }),
            );
            return format!(
                "⚠ Đọc được bảng {total} câu của {name} nhưng KHÔNG khớp được câu nào đang mở, \
                 nên tôi không biết phải đi mấy bước — và đi mò thì chốt nhầm câu. `/shot` để \
                 nhìn, rồi `/key <số>` cho câu đang mở."
            );
        }
    };

    let before = table.as_ref().map(|t| t.left());
    let keys = pick_keys(cursor, q - 1, opt);
    let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    if let Err(e) = crate::keys::press_seq(w, &refs) {
        logging::warn(
            "pick_send_failed",
            json!({ "session": s.session_id, "keys": keys, "err": e.to_string() }),
        );
        return format!("⚠ Không gửi được phím tới {name}: {e}");
    }
    logging::info(
        "pick_sent",
        json!({ "session": s.session_id, "window": w, "from": cursor, "to": q - 1,
                "opt": opt, "keys": keys }),
    );

    // ĐỌC LẠI, và chờ vì TUI vẽ sau. Ba nhịp ngắn thay vì một nhịp dài: nếu nó
    // vẽ nhanh thì trả lời nhanh, còn máy đang swap thì vẫn kịp thấy.
    let mut after = None;
    for _ in 0..3 {
        std::thread::sleep(std::time::Duration::from_millis(900));
        if let crate::keys::Look::Saw { body, .. } = crate::keys::look(&s.tty, PICK_LINES) {
            let t = crate::keys::ask_table(&body);
            if t.as_ref().map(|t| t.left()) != before {
                after = t;
                break;
            }
            after = t;
        }
    }
    let label = asking
        .rest
        .get(q.saturating_sub(2))
        .map(|r| r.header.clone())
        .filter(|_| q >= 2)
        .unwrap_or_else(|| asking.header.clone());
    match (before, after.as_ref().map(|t| t.left())) {
        // Ô trống bớt đi: đúng thứ vừa làm, và nói luôn còn mấy câu nữa.
        (Some(b), Some(a)) if a < b => {
            if a == 0 {
                format!(
                    "✅ {name} · câu {q} ({label}) → chọn {opt}. Bảng ĐÃ ĐỦ — bấm `/key enter` để gửi."
                )
            } else {
                format!("✅ {name} · câu {q} ({label}) → chọn {opt}. Còn {a} câu chưa trả lời.")
            }
        }
        // Bảng đã đủ TỪ TRƯỚC: chọn lại một câu đã trả lời thì số ô trống không
        // đổi, và đó là chuyện bình thường — không phải cảnh báo.
        (Some(0), Some(0)) => format!(
            "✅ {name} · câu {q} ({label}) → chọn {opt}. Bảng đã ĐỦ (chọn lại câu đã trả lời) — \
             bấm /send_{sid} để gửi.",
            sid = s.session_id.chars().take(8).collect::<String>()
        ),
        (Some(b), Some(a)) if a == b => format!(
            "⚠ Đã gửi phím tới {name} nhưng bảng KHÔNG đổi (vẫn {b} câu trống). Có thể câu ấy \
             cho chọn nhiều, hoặc phím chưa tới. `/shot` để nhìn trước khi bấm tiếp."
        ),
        // 🔴 Bảng BIẾN MẤT là kết cục TỐT NHẤT, không phải chuyện đáng ngờ: trả
        // lời nốt ô cuối thì `claude` gửi bảng đi và vẽ màn khác. Bản trước gọi
        // đúng cảnh ấy là *"đọc lại KHÔNG thấy bảng đâu"*, nên Hà bấm hai cú
        // ĐÚNG (`['1']` rồi `['right','1']`, log `pick_sent` làm chứng), phiên
        // tfl5 nhận câu trả lời và chạy tiếp — mà tin báo đọc ra như hỏng, và
        // anh nhắn *"bấm rồi nhưng không được"*.
        //
        // Làm đúng rồi báo sai cũng là một lỗi, chỉ hỏng ở khâu cuối: người
        // dùng bấm lại một việc đã xong. Phân biệt bằng thứ đo được — phiên có
        // đang chạy không.
        (_, None)
            if crate::keys::is_busy(
                &crate::keys::screen_of(&s.tty, 8)
                    .map(|x| x.0)
                    .unwrap_or_default(),
            ) =>
        {
            format!("✅ {name} · câu {q} ({label}) → chọn {opt}. Bảng ĐÃ GỬI ĐI — phiên đang chạy tiếp.")
        }
        _ => format!(
            "✅ {name} · câu {q} ({label}) → chọn {opt}. Bảng không còn trên màn — nhiều khả năng \
             nó vừa được gửi đi. `/shot` để nhìn."
        ),
    }
}

/// Bao nhiêu dòng màn cần đọc để thấy CẢ thanh tab lẫn hộp chọn.
///
/// 8 dòng — con số của nhánh "phiên vừa im" — vừa đủ thấy `1. Submit answers /
/// 2. Cancel` mà KHÔNG thấy dòng `You have not answered all questions` nằm cao
/// hơn, tức vừa đủ để báo tin sai một cách tự tin.
const PICK_LINES: usize = 40;

/// Hai luật của dự án nằm gọn trong hàm này, và đó là lý do nó là MỘT hàm chứ
/// không phải hai đoạn giống nhau ở hai chỗ gọi:
/// * **Điều 5** — chữ trên màn rời khỏi máy này y như phần xem trước của phiên,
///   nên phải qua `preview_risk` trước; có dấu hiệu bí mật thì nói là có, và
///   KHÔNG đưa chữ ra.
/// * Màn có **hộp chọn** thì nói thẳng từng lựa chọn: đó chính là thứ người ta
///   mở lên để xem, và số của nó là thứ gõ tiếp được.
pub fn screen_report(s: &crate::sessions::LiveSession, window: i64, lines: usize) -> String {
    // Tên để ĐỌC. 🔴 Hà 2026-08-13, ảnh chụp Telegram: nút và dòng "Đang theo
    // phiên" đã là `[AI/hub]` trong khi ngay dưới nó `/shot` còn in `📷 Màn của
    // projects-d2:` — cùng một phiên, hai cái tên, trong CÙNG một màn hình.
    // `display_name` đã có từ 22c97e9 và chỗ này là chỗ sót: ba lần `s.name`
    // thô, đúng cái tên `claude` tự đặt theo thư mục mở phiên (cả máy mở ở gốc
    // workspace nên phiên nào cũng `projects-xx`, tức cái tên phân biệt được ít
    // nhất trong mọi cái tên có ở đây).
    let what = crate::sessions::shown(s);
    match crate::keys::screen_text(window) {
        Ok(screen) => {
            // 🔴 Hà 2026-08-13 bấm 📷 và nhận *"Màn của [AI/mailler] có thể
            // chứa bí mật (credential_word, credential_word_vi) — không đưa ra
            // ngoài"*. Hai nhãn ấy là nhãn CHỮ: màn hình chỉ **nhắc tới** chữ
            // "mật khẩu"/"token", không hề có giá trị nào. Mà phiên ấy đang bàn
            // về DKIM và quyền ssh, tức nó sẽ nhắc tới mấy chữ đó cả buổi ⟹
            // `/shot` tắt hẳn đúng lúc cần nhất.
            //
            // Cửa này phải cùng luật với `redaction::file_risk`, và lý do đã
            // viết sẵn ở đó: chỗ khác nhau nằm ở NGƯỜI NHẬN và AI CHỌN. Phần
            // xem trước là mảnh chữ **hub tự chọn** đẩy vào một tài liệu trên
            // server — ngờ cả chữ là đúng. Còn `/shot` là **chủ máy gọi đích
            // danh một phiên của chính anh**, trả về buồng chat gác bằng
            // `chat_id`. Anh đang nhìn cái màn ấy nếu ngồi ở máy; chặn chữ ở
            // đây là chặn đúng phép thử cầu nối.
            //
            // GIÁ TRỊ thì vẫn chặn — `credential_literal`, `private_key_block`,
            // `secret_assignment` — vì đó mới là thứ mất đi khi lọt ra ngoài.
            // 🔴 Hà 2026-08-14: *"Tại sao lại bị chặn, hub là cổng làm việc của
            // tôi mà"* → *"Trong tele có thiết lập tự xoá lịch sử tin rồi nên
            // hub không cần tính năng này nữa"*.
            //
            // Cổng này GỠ HẲN, và đây là lý do nó gỡ được chứ không phải nhân
            // nhượng: rủi ro nó chặn là "một giá trị bí mật nằm lại lâu ở nơi
            // khác", mà buồng chat này nay tự xoá — chủ máy đã dựng hàng rào ấy
            // ở tầng dưới, đúng tầng của nó. Còn cái giá thì đã đo được: một
            // dòng dính mẫu là vứt CẢ màn, và màn có bí mật thường đúng là màn
            // đang gỡ chuyện xác thực, tức `/shot` tắt đúng lúc cần nhất.
            //
            // ⚠ KHÔNG suy rộng sang đường khác: nút 📎 gửi NGUYÊN một tệp
            // (`redaction::file_risk` vẫn gác ở đó — một tệp `.env` lọt ra là
            // lọt trọn bộ khoá, khác hẳn vài dòng đang hiện trên màn), và phần
            // xem trước trong ảnh chụp vẫn qua `sessions::preview_risk` vì nó
            // nằm lại trong một tài liệu trên server tfl5 — chỗ mà thiết lập tự
            // xoá của Telegram không với tới.
            let choices = crate::keys::parse_choices(&screen);
            let tail: Vec<&str> = screen
                .lines()
                .filter(|l| !l.trim().is_empty())
                .rev()
                .take(lines.clamp(1, SHOT_LINES_MAX))
                .collect();
            let body: String = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
            // ⛔ KHÔNG in lại lệnh thành chữ: CÁI NÚT ĐÃ LÀ CÂU ẤY RỒI.
            //
            // 🔴 Hà 2026-08-13: *"các nút phải thay vào đúng vị trí nó làm,
            // thay văn bản hiển thị"*. Trước đó `/shot` vừa đính một khối
            // comment liệt kê từng lệnh, VỪA gắn nút cho từng lệnh — cùng một
            // nội dung nói hai lần, mà bản chữ thì dài hơn và không bấm được.
            //
            // Và chính khối chữ ấy đẻ ra con bug anh chụp được: nó nằm lại
            // trong ô nhập, `input_box_text` đọc nó lên, rồi hub dựng nút
            // `⏎ Gửi: # Lệnh thấy trên màn…` — mời gửi lại lời của chính mình.
            // Bỏ nguồn thì cả họ bug ấy hết đường sinh ra.
            //
            // Nút thì vẫn dựng — ở chỗ gọi, từ chính `ack` này
            // (`commands_on_screen`), nên không mất đường bấm nào.
            let quick_note = String::new();
            if choices.is_empty() {
                format!("📷 Màn của {what}:\n\n{body}{quick_note}")
            } else {
                let list: Vec<String> =
                    choices.iter().map(|(n, l)| format!("  {n}. {l}")).collect();
                format!(
                    "📷 {what} đang hỏi — bấm số ở hàng phím để chọn:\n{}\n\n{}{}",
                    list.join("\n"),
                    body,
                    quick_note
                )
            }
        }
        Err(e) => format!(
            "⚠ không đọc được màn: {}",
            crate::exec::truncate(&e.to_string(), 300)
        ),
    }
}

/// Hàng phụ của một phiên — **cùng dữ kiện với thẻ trên trang điện thoại**.
///
/// Giữ đúng bộ ấy là có chủ ý: hai mặt đọc cùng một ảnh chụp, nên hai mặt phải
/// nói cùng một câu. Một con số chỉ có ở một chỗ là một con số không ai đối
/// chiếu được, và tới lúc lệch thì không biết bên nào sai (`fe/index.html:1996`
/// là bản trên trang).
///
/// `im N phút` chỉ hiện với phiên KHÔNG chạy: với phiên đang chạy, "im" là câu
/// sai — nhật ký của nó đứng yên suốt một lượt `cargo test` hai phút.
fn session_meta(s: &crate::sessions::LiveSession, now_ms: i64) -> String {
    let mode = match s.permission_mode.as_deref() {
        Some("auto") => "tự duyệt",
        Some("dontAsk") => "không hỏi",
        Some("default") => "hỏi trước",
        Some(other) => other,
        None => "",
    };
    let kid = if s.pending_subagents > 0 {
        format!("{} subagent", s.pending_subagents)
    } else {
        String::new()
    };
    // Cùng cửa sổ ngữ cảnh với trang: haiku 200k, còn lại 1M.
    let win: u64 = if s.model.as_deref().is_some_and(|m| m.contains("haiku")) {
        200_000
    } else {
        1_000_000
    };
    let pct = if s.context_tokens > 0 {
        (s.context_tokens as f64 / win as f64 * 100.0).round() as u64
    } else {
        0
    };
    let ctx = if pct > 0 {
        format!("ngữ cảnh {pct}%")
    } else {
        String::new()
    };
    let quiet = if s.working {
        String::new()
    } else {
        quiet_for(s.last_activity.as_deref(), now_ms).unwrap_or_default()
    };
    [
        s.activity.clone().unwrap_or_default(),
        quiet,
        kid,
        ctx,
        mode.to_string(),
    ]
    .into_iter()
    .filter(|x| !x.is_empty())
    .collect::<Vec<_>>()
    .join(" · ")
}

/// "im bao lâu rồi", tính từ lượt cuối nhật ký lớn lên.
///
/// Dưới một phút thì KHÔNG nói: "im 0 phút" là một dòng chữ không mang tin, và
/// mỗi dòng thừa đẩy phiên cuối danh sách ra khỏi màn.
fn quiet_for(last_activity: Option<&str>, now_ms: i64) -> Option<String> {
    let dt = chrono::DateTime::parse_from_rfc3339(last_activity?).ok()?;
    let mins = (now_ms - dt.timestamp_millis()) / 60_000;
    match mins {
        m if m < 1 => None,
        m if m < 60 => Some(format!("im {m} phút")),
        m if m < 60 * 24 => Some(format!("im {} tiếng", m / 60)),
        m => Some(format!("im {} ngày", m / (60 * 24))),
    }
}

/// Nhãn của một cái nút phiên. Cùng ba dữ kiện với dòng chữ, gọn hơn để lọt bề
/// ngang một cái nút.
pub fn session_button_label(s: &crate::sessions::LiveSession) -> String {
    // Cùng bộ chấm với `session_list_text` — xem chú thích ở đó về vì sao KHÔNG
    // dùng `▶`/`⏸`/`⏹`.
    let dot = match (s.host.as_str(), s.asking.is_some(), s.working) {
        ("dead", _, _) => "⚫",
        (_, true, _) => "⚠",
        _ if s.error.is_some() => "🔴",
        (_, _, true) => "🟢",
        _ => "🟡",
    };
    // Dự án trước, vì đó là thứ ngón tay đang tìm; tên phiên tự sinh chỉ để phân
    // biệt hai phiên cùng dự án.
    let what = crate::sessions::shown(s);
    // Nguồn đứng ngay trên NÚT nữa, không chỉ trên danh sách chữ: cái nút mới là
    // thứ ngón tay chạm vào, và nó phải nói trước rằng bấm vào một phiên VS Code
    // thì xem được chứ gõ thì không.
    format!(
        "{} {} {} · {}",
        dot,
        source_icon(&s.host),
        crate::exec::truncate(&what, 30),
        s.account
    )
}

/// Tám ký tự đầu của id — đúng thứ `claude stop` nhận, và đúng thứ trang hiện.
fn short_id(session_id: &str) -> &str {
    session_id.split('-').next().unwrap_or(session_id)
}

/// Execute button presses that arrived on a channel, then acknowledge them on
/// that channel. Never propagates: one bad press must not fail the whole poll,
/// but every outcome is logged.
fn execute_commands(db: &Db, cfg: &Config, adapter: &str, commands: &[ChannelCommand]) {
    // 🔴 TỪ ĐÂY TRỞ ĐI LÀ VIỆC CÓ NGƯỜI ĐANG CHỜ. Mọi `osascript`, mọi lần hỏi
    // `claude`, mọi lượt đọc nhật ký sinh ra bên dưới hàm này đều là hệ quả của
    // một ngón tay vừa bấm — nên chúng chạy ở hạng đầy đủ, còn vòng quét định
    // kỳ thì nhường đường (xem `exec::Lane`).
    //
    // Đặt Ở ĐÂY chứ không ở từng chỗ gọi: đường từ cú bấm tới `osascript` xuyên
    // chừng mười lớp, và mười chỗ đánh dấu là mười chỗ để quên. Đây là cái cửa
    // duy nhất mọi mệnh lệnh đều đi qua.
    let _lane = crate::exec::urgent();
    for cmd in commands {
        // ĐỒNG HỒ cho từng lệnh. Hà 2026-08-12: *"bấm vào phiên trên tele vẫn
        // đang phải đợi rất lâu"* — và lúc ấy không ai trả lời được "lâu ở khúc
        // nào", vì log chỉ có lúc nhận và lúc xong. Một con số cho mỗi route thì
        // lần sau câu hỏi ấy tự có đáp án.
        let cmd_started = std::time::Instant::now();
        // Every verb answers for itself. There used to be a second stage below
        // this match — "look the decision up, then approve or reject it" — and
        // a verb that forgot to end with `Some(ack)` fell into it and logged
        // "Không tìm thấy decision #0" as its reply. That whole stage went with
        // the inbox on 2026-08-08; there are no decisions left to look up.
        let answered: Option<String> = match cmd.kind {
            // `/run_<n>` — bản CHỮ của nút `▶`, và là bản đáng tin hơn: dòng
            // lệnh nằm trong sổ chứ không nằm trong nhãn, nên nó không thể bị
            // cắt cụt (xem `keys::BTN_CMD_MAX`). Chạm chữ tô sáng là chạy.
            CommandKind::RunQuick => {
                let n = cmd.arg.trim().parse::<usize>().unwrap_or(usize::MAX);
                match quick_cmd(db, n) {
                    Some((sid, line)) => {
                        logging::info(
                            "run_quick",
                            json!({ "n": n, "session": sid,
                                    "cmd": crate::exec::truncate(&line, 120) }),
                        );
                        // Cùng đường với cái nút: MÁY chạy, PHIÊN đọc.
                        let ack = format!("▶ chạy trong {sid}: {line}");
                        reply_in_channel(db, cfg, adapter, cmd, &ack);
                        if let Some(tg) = crate::telegram::inbox() {
                            tg.push_text(&format!("/runin {sid} {line}"));
                        }
                        Some(ack)
                    }
                    None => Some(
                        "⚠ lệnh gợi ý ấy đã cũ (màn đã đổi). Gõ /shot rồi bấm lại.".to_string(),
                    ),
                }
            }
            CommandKind::Help => {
                // Chữ này SINH TỪ BẢNG (`commands::help_text`), không gõ tay:
                // một lệnh mới không thể ra đời mà thiếu dòng của nó, và
                // không dòng nào còn tả một lệnh đã chết. 🔴 Hà 2026-08-14:
                // *"Tại sao không tạo lib lệnh để map khi nhận"*.
                let ack = crate::commands::help_text();
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
            }
            // Động từ của cả một VÒNG. `Run` được TRẢ LỜI chứ không chạy ở đây,
            // và đó là cố ý: mã này vốn đã đang chạy BÊN TRONG một vòng
            // (`run_once` → `execute_telegram_commands` → `execute_commands`),
            // nên gọi lại vòng là đệ quy vào chính nó. Vòng đang mang lệnh này
            // sẽ làm nốt phần việc ngay sau đây.
            //
            // 🔴 `Ingest` từng đứng chung nhánh này tới 2026-08-14 — nó trả lời
            // *"Đang đọc phòng trong vòng hiện tại"*, một câu nay không còn
            // phòng nào để đúng.
            CommandKind::Run => {
                let what = "Vòng đang chạy ngay bây giờ (lệnh này được xử lý bên trong nó).";
                reply_in_channel(db, cfg, adapter, cmd, what);
                Some(what.to_string())
            }
            CommandKind::Doctor => {
                // 🔴 `/doctor` từng gọi `portal::probe_now` — nó dò SỨC KHOẺ
                // CỦA TFL5 (đăng nhập được không, đẩy doc được không). Kênh ấy
                // đã gỡ ngày 2026-08-14, nên câu hỏi đổi: thứ đáng dò bây giờ
                // là cái máy này và những phiên đang chạy trên nó.
                let live =
                    crate::sessions::snapshot_cached(cfg, std::time::Duration::from_secs(20));
                let jobs = jobs_line().unwrap_or_else(|| "  (không có)".to_string());
                let probe = format!(
                    "🩺 {} phiên đang sống{}\n⚡ lệnh chạy nền:\n{}\n📟 hubd: {}\n{}",
                    live.sessions.len(),
                    if live.blind.is_empty() {
                        String::new()
                    } else {
                        format!(" · {} tài khoản không hỏi được", live.blind.len())
                    },
                    jobs,
                    if std::path::Path::new(&crate::config::expand_home(std::path::Path::new(
                        "~/Library/Application Support/hub/bin/hubd",
                    )),)
                    .exists()
                    {
                        "bản cài có mặt"
                    } else {
                        "⚠ KHÔNG thấy bản cài — launchd đang chạy gì?"
                    },
                    recent_errors_line(db),
                );
                reply_in_channel(db, cfg, adapter, cmd, &probe);
                Some(probe)
            }
            CommandKind::RunIn => {
                // MÁY chạy, PHIÊN đọc — xem `CommandKind::RunIn`.
                let (want, line) = target_and_rest(db, &cmd.arg);
                // Lệnh dựng lại chính hub đi đường `/upgrade`, kể cả khi được
                // gõ tay vào đây: chạy nó qua `/runin` thì hubd bị thay thế
                // giữa lúc đang xử lý, và câu trả lời chết theo — đo được ba
                // lần liền 2026-08-13, xem `remember_quick`.
                if is_self_rebuild(&line) {
                    let ack = match crate::runtime::self_install(cfg) {
                        Ok(msg) => format!(
                            "🔧 {msg}\nĐang khởi động lại hubd… (lệnh này dựng lại chính hub nên nó \
                             đi đường /upgrade — chạy qua /runin thì hub bị thay giữa chừng và câu \
                             trả lời chết theo)"
                        ),
                        Err(e) => format!(
                            "⚠ không dựng lại được (bản đang chạy GIỮ NGUYÊN): {}",
                            crate::exec::truncate(&e.to_string(), 400)
                        ),
                    };
                    let ok = ack.starts_with("🔧");
                    // NÓI TRƯỚC, chết sau. Thứ tự này là cả bản vá.
                    reply_in_channel(db, cfg, adapter, cmd, &ack);
                    if ok {
                        if let Err(e) = crate::runtime::restart_daemon() {
                            logging::error(
                                "self_install_restart_failed",
                                json!({ "err": e.to_string() }),
                            );
                        }
                    }
                    continue;
                }
                let live =
                    crate::sessions::snapshot_cached(cfg, std::time::Duration::from_secs(20));
                let ack = match live
                    .sessions
                    .iter()
                    .find(|s| same_session(&s.session_id, &want))
                {
                    None => format!(
                        "⚠ không thấy phiên '{}' đang chạy — lệnh KHÔNG chạy.",
                        crate::exec::truncate(&want, 40)
                    ),
                    Some(s) => {
                        // 🔴 THƯ MỤC PHẢI LÀ CỦA PHIÊN ẤY. Hà 2026-08-13, chỉ
                        // vào nút `bash scripts/verify-acl-delta-0813.sh`:
                        // *"ví dụ như cái này nó lấy ở đâu thì phải nằm đúng
                        // chỗ đó chứ"*.
                        //
                        // Anh đúng, và đây là một lỗi im lặng đúng nghĩa: dòng
                        // ấy lấy từ màn phiên `[tfl5]`, nơi `scripts/` nghĩa là
                        // `AI/tfl5/scripts/`. Chạy nó ở GỐC workspace thì
                        // `scripts/` là `~/projects/scripts/` — một thư mục có
                        // thật, chứa những tệp khác hẳn. Lệnh sẽ chạy, trả một
                        // mã thoát, và kết quả nói về một thứ không ai hỏi.
                        //
                        // `session_root` đọc dự án từ SỔ và trả `None` khi
                        // không biết — chỗ này TỪ CHỐI thay vì rơi về gốc, vì
                        // rơi về gốc chính là con bug trên.
                        let root = match session_root(db, cfg, &s.session_id) {
                            Some(r) => r,
                            None => {
                                logging::warn(
                                    "runin_no_root",
                                    json!({ "session": s.session_id,
                                            "why": "sổ chưa biết dự án của phiên — không đoán gốc workspace" }),
                                );
                                let msg = format!(
                                    "⚠ chưa biết {} làm ở thư mục nào nên KHÔNG chạy. Một lệnh tương đối chạy nhầm thư mục thì vẫn ra một mã thoát, mà kết quả nói về thứ khác. Dùng đường dẫn tuyệt đối, hoặc chờ hub nhận ra dự án của phiên.",
                                    crate::sessions::shown(s)
                                );
                                reply_in_channel(db, cfg, adapter, cmd, &msg);
                                continue;
                            }
                        };
                        // 🔴 Hà 2026-08-14, ảnh chụp một cú bấm ▶ trên [tfl5]:
                        // *"Có những lệnh sẽ chạy khá lâu nên cần cơ chế theo
                        // dõi riêng thay vì cố định timeout"*. Trên ảnh:
                        // `⏱ quá giờ sau 120.9s — đã giết cả nhóm tiến trình`,
                        // cho một lệnh triển khai.
                        //
                        // Trần 120 giây ấy không đo cái gì cả — nó là một con
                        // số tròn, và với đúng những lệnh đáng bấm từ điện
                        // thoại (build, test, deploy) thì nó bảo đảm GIẾT giữa
                        // chừng. Tệ hơn: giết một lệnh triển khai ở giây thứ
                        // 120 để lại một trạng thái không ai biết là gì.
                        //
                        // Cái sai gốc là CHỜ TẠI CHỖ: chờ thì buộc phải chọn
                        // giữa "chặn kênh chat" và "giết lệnh", mà cả hai đều
                        // sai. Nay lệnh chạy ở luồng riêng, hub trả lời NGAY,
                        // rồi theo dõi và báo lại — xem `watch_long_job`.
                        watch_long_job(
                            cfg.clone(),
                            s.clone(),
                            root.clone(),
                            line.clone(),
                            adapter.to_string(),
                            cmd.chat_id.clone(),
                        );
                        format!(
                            "▶ đang chạy — {}\ntrong {} · hub báo lại khi xong, không còn trần 120 giây.",
                            crate::exec::truncate(&line, 120),
                            crate::sessions::shown(s),
                        )
                    }
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
            }
            CommandKind::Win => {
                // 🔴 TRỐNG = XEM DANH SÁCH. Hà 2026-08-15: *"Đặt là terminal và
                // win thì liệt kê danh sách terminal, có đang chạy gì không"*.
                //
                // Cùng hình dạng với `/session`: động từ trơn hỏi *"đang có
                // những gì"*, động từ kèm tham số mới là *"làm cái này"*. Trước
                // đó `/terminal` trơn trả `None` — tức gõ tên một route rồi
                // nhận lại sự im lặng, đúng cái làm người ta tưởng nó hỏng.
                //
                // `keys::terminal_tabs` đã có sẵn từ 08-13 và trả trọn thứ cần
                // trong MỘT lượt osascript: tty, có bận không, và tiến trình
                // đang chạy. Ghép thêm tên phiên nếu tty ấy khớp một phiên
                // `claude` — vì "cửa sổ này là phiên [tfl5]" mới là câu trả lời
                // người ta cần, không phải "ttys007".
                if cmd.arg.trim().is_empty() {
                    let ack = match crate::keys::terminal_tabs() {
                        Err(e) => format!(
                            "⚠ không hỏi được Terminal: {}",
                            crate::exec::truncate(&e.to_string(), 200)
                        ),
                        Ok(tabs) if tabs.is_empty() => {
                            "Không có cửa sổ Terminal nào đang mở.".to_string()
                        }
                        Ok(tabs) => {
                            let live = crate::sessions::snapshot_cached(
                                cfg,
                                std::time::Duration::from_secs(20),
                            );
                            let mut out =
                                format!("🖥 {} cửa sổ Terminal đang mở:\n", tabs.len());
                            for tb in &tabs {
                                let who = live
                                    .sessions
                                    .iter()
                                    .find(|s| s.tty == tb.tty)
                                    .map(|s| format!(" · {}", crate::sessions::shown(s)));
                                let doing = match tb.cli() {
                                    Some(p) => p.to_string(),
                                    None => "dấu nhắc trống".to_string(),
                                };
                                out.push_str(&format!(
                                    "\n{} {}{}\n    {}",
                                    if tb.busy { "🟢" } else { "⚪" },
                                    tb.tty,
                                    who.unwrap_or_default(),
                                    doing
                                ));
                            }
                            out.push_str(
                                "\n\n🟢 = đang chạy gì đó · ⚪ = rảnh\n\
                                 /terminal <lệnh> để mở một cửa sổ mới có tty.",
                            );
                            out
                        }
                    };
                    reply_in_channel(db, cfg, adapter, cmd, &ack);
                    Some(ack)
                } else {
                // Cửa sổ THẬT, vì thứ thiếu là một cái tty — xem `CommandKind::Win`.
                //
                // 🔴 CHẠY ĐÚNG DÒNG ANH GÕ, không bọc thêm gì. Hà 2026-08-13:
                // *"tại sao lệnh chỉ có 1 dòng mà chèn thêm text vào làm gì?"*.
                // Bản đầu ghép `cd <gốc workspace>; <lệnh>` để cwd khớp `/cmd`
                // — lý do nghe hợp lý, và cái giá là mọi cửa sổ mở ra đều bắt
                // đầu bằng một dòng anh không gõ, dài hơn cả lệnh thật.
                //
                // Phép thử cầu nối trả lời gọn: ngồi trước máy anh mở cửa sổ
                // rồi dán ĐÚNG dòng ấy, không ai tự thêm một `cd` vào trước.
                // Cần thư mục thì gõ `cd` — như ở terminal. Đổi lại, câu trả
                // lời phải NÓI RA cửa sổ mở ở đâu, vì đó là thứ vừa đổi.
                let line = cmd.arg.trim().to_string();
                let ack = if line.is_empty() {
                    "⚠ /win cần một dòng lệnh. Ví dụ: /win sudo -v".to_string()
                } else {
                    match crate::keys::open_window(&line) {
                        Ok((win, tty)) => {
                            logging::info(
                                "win_opened",
                                json!({ "cmd": crate::exec::truncate(&line, 120),
                                        "window": win, "tty": tty }),
                            );
                            format!(
                                "🖥 Đã mở cửa sổ Terminal ({tty}) và chạy đúng dòng này:\n{line}\n\n\
                                 Cửa sổ mở ở thư mục mặc định của shell, không phải gốc workspace — \
                                 cần chỗ khác thì gõ cd. Kết quả nằm TRÊN cửa sổ ấy, không về đây: \
                                 đó là chỗ gõ mật khẩu."
                            )
                        }
                        // Không nuốt: mở cửa sổ hỏng thì người bấm phải biết,
                        // không thì họ ngồi chờ một cái cửa sổ không tồn tại.
                        Err(e) => {
                            let msg = crate::logging::err_chain(&e);
                            logging::error(
                                "win_open_failed",
                                json!({ "cmd": crate::exec::truncate(&line, 120), "err": msg }),
                            );
                            format!(
                                "⚠ chưa mở được cửa sổ: {}",
                                crate::exec::truncate(&msg, 200)
                            )
                        }
                    }
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
                }
            }
            CommandKind::Accounts => {
                // Một ảnh chụp thật, không phải con số nhớ từ lượt trước: câu
                // hỏi "phiên nào đang chạy bằng tài khoản nào" chỉ đúng ở thì
                // hiện tại. Hạn mức thì lấy bản đã đo sẵn (5 phút một lượt),
                // nên lệnh này không đẻ thêm tiến trình `claude` nào.
                let live =
                    crate::sessions::snapshot_cached(cfg, std::time::Duration::from_secs(20));
                let ack =
                    crate::runtime::accounts_say(cfg, &live, chrono::Utc::now().timestamp_millis());
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
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
                let live =
                    crate::sessions::snapshot_cached(cfg, std::time::Duration::from_secs(20));
                // Đóng sổ một phiên VỪA TẮT cũng chạy được — bản bàn giao dựng
                // từ nhật ký, không cần tiến trình (cùng lối với `/ask`).
                let target = live
                    .sessions
                    .iter()
                    .find(|s| same_session(&s.session_id, &want))
                    .cloned()
                    .or_else(|| ended_session(db, &want));
                let ack = match target.as_ref() {
                    None => format!(
                        "⚠ không thấy phiên '{}' đang chạy, và nó cũng không nằm trong sổ phiên \
                         vừa tắt (giữ 24 giờ)",
                        crate::exec::truncate(&want, 12)
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
                //
                // HAI lối gõ, cùng một đường đi (Hà 2026-08-12: *"kiến trúc lại
                // lệnh cho hợp lý, ví dụ: /new -a acc2 -s dwork"*):
                //   `/new -a acc2 -s dwork sửa lịch`   ← cờ, gõ đâu cũng được
                //   `/new dwork @acc2 sửa lịch`        ← vị trí, lối cũ
                // Lối cũ giữ lại vì nó nằm trong tay quen của chủ máy và trong
                // các nút Telegram đã gửi đi; bỏ nó là làm hỏng thứ đang chạy.
                const NEW_FLAGS: &[&str] =
                    &["a", "acc", "account", "s", "p", "project", "duan", "du-an"];
                let (flags, rest) = split_flags(&cmd.arg, NEW_FLAGS);
                let flag_project = ["s", "p", "project", "duan", "du-an"]
                    .iter()
                    .find_map(|k| flags.get(*k))
                    .map(|v| v.trim().to_string());
                let flag_account = ["a", "acc", "account"]
                    .iter()
                    .find_map(|k| flags.get(*k))
                    .map(|v| v.trim().to_string());

                // 🔴 `/new` chỉ cần HAI thứ: tài khoản nào, và gõ gì.
                //
                // Hà 2026-08-13: *"luồng xử lý 1 phiên mới là gõ lệnh vào cli
                // (mở phiên) → gõ text để làm việc vào cli → vì vậy lệnh `new`
                // chỉ cần tham số sử dụng acc nào và text gửi đi là gì"*. Đúng
                // theo đúng phép thử CẦU NỐI: ngồi ở máy thì chủ máy mở một cửa
                // sổ rồi gõ việc — không ai khai báo "dự án" với cái terminal.
                //
                // Bản cũ bóc TỪ ĐẦU TIÊN làm tên dự án và bắt nó phải là một
                // dự án đã biết. Cái giá đo được (09:35 ngày 12-08): Hà gõ
                // `/new Tại sao lại có phiên này chạy…` và nhận về
                // `⚠ không biết dự án 'Tại'` — một câu hỏi bị đọc thành một tên
                // thư mục. Nay cả câu là ĐỀ BÀI; `-s` vẫn còn cho ai muốn chỉ
                // đúng thư mục, nhưng không bắt buộc và không đoán.
                let (name, task) = match flag_project.as_deref() {
                    Some(p) => (p.to_string(), rest.as_str()),
                    None => (String::new(), rest.as_str()),
                };
                let name = name.as_str();
                // `@tài-khoản` đứng ngay sau tên dự án: `/new hub @acc2 việc…`.
                // Không có thì dùng tài khoản mặc định — giữ nguyên cách gõ cũ.
                let (account, task) = match (flag_account, task.trim().strip_prefix('@')) {
                    (Some(a), _) => (Some(a), task),
                    (None, Some(rest)) => {
                        let (acc, rest) =
                            rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                        (Some(acc.trim().to_string()), rest)
                    }
                    (None, None) => (None, task),
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
                    // Không nêu `-s` ⟹ mở ở GỐC workspace, đúng chỗ mọi phiên
                    // trên máy này vẫn mở (và là thư mục duy nhất cả ba tài
                    // khoản đã duyệt MCP).
                    let dir = if name.is_empty() {
                        Some(cfg.workspace_root.clone())
                    } else {
                        dir
                    };
                    match dir {
                        Some(d)
                            if name.is_empty()
                                || known.contains(&name.to_string())
                                || cfg.projects.contains_key(name) =>
                        {
                            // 🔴 KHÔNG CHỜ TẠI CHỖ. Hà 2026-08-14: *"kiểm tra lệnh
                            // new đi đã chạy được cơ chế mới chưa"* — chưa, và đo
                            // được ba lượt gần nhất: **64,7s · 39,4s · 61,5s**.
                            // Suốt chừng ấy `/new` giữ `CMD_LOCK`, nên mọi lệnh
                            // khác của chủ máy xếp hàng phía sau một cái cửa sổ
                            // đang khởi động.
                            //
                            // Thời gian ấy không phải lãng phí: hub chờ nhật ký
                            // phiên sinh ra để biết ID (20 giây), rồi nếu chưa có
                            // thì bấm hộ hộp tin-thư-mục và chờ thêm 20 giây nữa.
                            // Việc đúng, chỗ ngồi chờ thì sai — đúng cùng bệnh với
                            // `/runin` đã chữa sáng nay.
                            //
                            // Nay: mở cửa sổ ở luồng riêng, trả lời NGAY, và báo
                            // lần hai khi phiên chào đời. Con trỏ theo dõi chuyển
                            // ở LƯỢT SAU chứ không phải bây giờ — nên tin đầu phải
                            // nói thẳng điều đó, không thì chữ gõ ngay sau khi bấm
                            // sẽ rơi vào phiên cũ mà không ai biết vì sao.
                            {
                                watch_new_session(
                                    cfg.clone(),
                                    name.to_string(),
                                    d.clone(),
                                    task.to_string(),
                                    account.clone(),
                                    adapter.to_string(),
                                    cmd.chat_id.clone(),
                                );
                                format!(
                                "⌨ Đang mở cửa sổ Terminal{}…\nhub báo lại khi phiên chào đời (thường 15–60 giây, vì nó chờ nhật ký phiên sinh ra để biết id).\n⚠ Con trỏ CHƯA chuyển — gõ chữ lúc này vẫn vào phiên đang theo.",
                                account
                                    .as_deref()
                                    .map(|a| format!(" bằng {a}"))
                                    .unwrap_or_else(|| " bằng tài khoản mặc định".to_string()),
                            )
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
                let mut live =
                    crate::sessions::snapshot_cached(cfg, std::time::Duration::from_secs(20));
                mark_started_by_hub(db, &mut live);
                let ack = match live
                    .sessions
                    .iter()
                    .find(|s| same_session(&s.session_id, &want))
                {
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
            CommandKind::Close => {
                // Cùng cách nhắm đích với `/stop` (trống = phiên đang theo, có
                // id = phiên ấy) — khác ở KẾT CỤC, nên vẫn hỏi lại một câu:
                // đóng một cửa sổ là thứ không lùi lại được.
                let want = cmd.arg.trim().to_string();
                let want = if want.is_empty() {
                    db.cursor_or_log(FOCUS_SESSION_KEY).unwrap_or_default()
                } else {
                    want
                };
                let mut live =
                    crate::sessions::snapshot_cached(cfg, std::time::Duration::from_secs(20));
                mark_started_by_hub(db, &mut live);
                let ack = match live
                    .sessions
                    .iter()
                    .find(|s| same_session(&s.session_id, &want))
                {
                    None if want.is_empty() => {
                        "⚠ chưa theo phiên nào — bấm một phiên rồi /close, hoặc /close <id>."
                            .to_string()
                    }
                    None => format!(
                        "⚠ không thấy phiên '{}' đang chạy",
                        crate::exec::truncate(&want, 40)
                    ),
                    Some(s) => {
                        let what = format!(
                            "Đóng hẳn phiên {} ({})?",
                            crate::sessions::shown(s),
                            s.account
                        );
                        if let Some(refusal) =
                            ask_owner(db, cfg, adapter, cmd, &what, "đóng phiên nào")
                        {
                            refusal
                        } else {
                            match crate::sessions::close_session(cfg, s) {
                                Ok(win) => {
                                    remember_stopped(db, s);
                                    logging::info(
                                        "session_closed",
                                        json!({ "session": s.session_id, "kind": s.kind,
                                                "window": win }),
                                    );
                                    // Nói ĐÚNG cái vừa xảy ra, và ở đây "vừa
                                    // xảy ra" mới là gõ `/exit` — cửa sổ chưa
                                    // đóng, nó vào sổ chờ. Khai "đã đóng" lúc
                                    // này là kể một việc chưa xảy ra, đúng thứ
                                    // luật 3 của dự án cấm.
                                    match win {
                                        None => format!(
                                            "⏹ Đã dừng phiên nền {} — nó không có cửa sổ nào để đóng. Hội thoại vẫn còn.",
                                            crate::sessions::shown(s)
                                        ),
                                        Some(w) => {
                                            let now = chrono::Utc::now().timestamp();
                                            remember_closing(
                                                db,
                                                &s.session_id,
                                                w,
                                                &crate::sessions::shown(s),
                                                now,
                                            );
                                            format!(
                                                "⏳ Đã gõ /exit vào {} — chờ CLI chạy nốt lượt đang dở rồi mới đóng cửa sổ. Kiểm 30 giây một lần, xong tôi báo.",
                                                crate::sessions::shown(s)
                                            )
                                        }
                                    }
                                }
                                Err(e) => format!(
                                    "⚠ chưa đóng được: {}",
                                    crate::exec::truncate(&e.to_string(), 240)
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
                let live =
                    crate::sessions::snapshot_cached(cfg, std::time::Duration::from_secs(20));
                // Đã dừng KHÔNG phải là đã mất: `--resume` nối vào nhật ký, nó
                // không cần tiến trình nào đang sống. Và dừng-rồi-nói-tiếp
                // chính là đường DUY NHẤT — claude từ chối resume một phiên nền
                // đang chạy (đo 2026-08-08).
                let target = live
                    .sessions
                    .iter()
                    .find(|s| same_session(&s.session_id, &want))
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
            // Hub tự dựng lại chính nó. Trả lời TRƯỚC, khởi động lại SAU —
            // bước cuối giết chính tiến trình đang gõ câu trả lời này.
            CommandKind::Upgrade => {
                let ack = match crate::runtime::self_install(cfg) {
                    Ok(msg) => format!("🔧 {msg}\nĐang khởi động lại hubd…"),
                    Err(e) => format!(
                        "⚠ không dựng lại được (bản đang chạy GIỮ NGUYÊN): {}",
                        crate::exec::truncate(&e.to_string(), 400)
                    ),
                };
                let ok = ack.starts_with("🔧");
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                if ok {
                    if let Err(e) = crate::runtime::restart_daemon() {
                        logging::error(
                            "self_install_restart_failed",
                            json!({ "err": e.to_string() }),
                        );
                    }
                }
                Some(ack)
            }
            CommandKind::Type | CommandKind::Key | CommandKind::Shot | CommandKind::Pick => {
                // Gõ vào ĐÚNG cửa sổ của phiên đang theo. Không ghép được cửa
                // sổ thì TỪ CHỐI — gõ vào cửa sổ lạ là gõ vào việc của người
                // khác, và đó là hàng rào duy nhất còn lại ở đường này.
                let (want, typed) = target_and_rest(db, &cmd.arg);
                // ĐƯỜNG NHANH: sổ nói cửa sổ nào, `ps` chứng thực — xem
                // `sessions::window_target_from_book`. Đường cũ dựng lại ảnh
                // chụp (ba lần spawn `claude` 279 MB) chỉ để tra `tty`, và trên
                // máy đang swap nó mất **117–134 giây** rồi còn trả về "không
                // thấy phiên" khi lượt hỏi hết giờ.
                let booked = db
                    .cursor_or_log(WATCH_KEY)
                    .and_then(|v| crate::sessions::window_target_from_book(&v, &want));
                // Chỉ trả tiền ảnh chụp khi sổ KHÔNG trả lời được.
                let live = match &booked {
                    Some(_) => None,
                    None => Some(crate::sessions::snapshot_cached(
                        cfg,
                        std::time::Duration::from_secs(20),
                    )),
                };
                let target = booked.or_else(|| {
                    live.as_ref().and_then(|l| {
                        l.sessions
                            .iter()
                            .find(|s| same_session(&s.session_id, &want))
                            .cloned()
                    })
                });
                // Id của phiên ĐANG được thao tác — giữ TRƯỚC khi `match` nuốt mất
                // `target`. Mọi cái nút dựng bên dưới phải buộc vào phiên này, không
                // phải con trỏ focus lúc bấm (xem `remember_quick`).
                let shot_sid = target
                    .as_ref()
                    .map(|s| s.session_id.clone())
                    .unwrap_or_default();
                // Bảng hỏi đọc từ NHẬT KÝ, giữ lại trước khi `match` nuốt mất
                // `target`. 🔴 Hà 2026-08-14, ảnh chụp `/shot` một phiên đang
                // mở bảng: *"Màn này chưa chọn được gì"* — đúng, vì bộ nút số
                // của `/shot` dựng từ `keys::parse_choices`, mà hàm ấy MÙ với
                // bảng `AskUserQuestion` (mỗi lựa chọn có một dòng mô tả bên
                // dưới, đúng hình dạng luật "liền dòng" loại bỏ — đo được: 0
                // mục trên chính màn ấy). Nhật ký thì đọc ra đủ.
                // Đường NHANH không mang `asking` (sổ cửa sổ không giữ trường
                // ấy) — nên hỏi thẳng nhật ký khi ảnh chụp không trả lời được.
                let shot_asking = target.as_ref().and_then(|s| s.asking.clone()).or_else(|| {
                    (!shot_sid.is_empty())
                        .then(|| crate::sessions::asking_of(cfg, &shot_sid))
                        .flatten()
                });
                let ack = match target {
                    None if want.is_empty() => {
                        "⚠ chưa mở phiên nào. Chạm một phiên rồi gõ.".to_string()
                    }
                    // 🔴 "Không có trong danh sách" ≠ "không tồn tại". Nếu lượt
                    // hỏi vừa rồi MÙ với tài khoản nào đó thì danh sách ấy
                    // thiếu, và nói "không thấy phiên" là khẳng định một điều
                    // hub không biết — đúng con bug đã vá ở cái loa, ở một chỗ
                    // khác. Hà nhận đúng câu ấy về một phiên đang sống.
                    None if live.as_ref().is_some_and(|l| !l.blind.is_empty()) => format!(
                        "⚠ chưa hỏi được danh sách phiên của {} (máy đang chậm, `claude agents` hết giờ) \
                         — nên tôi CHƯA gõ gì cả. Thử lại sau một nhịp.",
                        live.as_ref().map(|l| l.blind.join(", ")).unwrap_or_default()
                    ),
                    None => format!(
                        "⚠ không thấy phiên '{}' trong danh sách",
                        crate::exec::truncate(&want, 40)
                    ),
                    Some(s) => match crate::keys::window_of(&s.tty) {
                        Ok(Some(w)) => {
                            // `/shot` đi đường riêng: nó không gõ gì, chỉ nhìn.
                            if matches!(cmd.kind, CommandKind::Shot) {
                                // Nút "gửi nhanh" chỉ dựng được khi biết màn có
                                // gì — nên đọc màn một lần, dùng cho cả hai.
                                // `/shot 80` — xin nhiều dòng hơn khi thứ cần
                                // nhìn nằm cao hơn cửa sổ mặc định.
                                let n = typed
                                    .trim()
                                    .parse::<usize>()
                                    .unwrap_or(SHOT_LINES);
                                let mut out = screen_report(&s, w, n);
                                // Phiên đang mở bảng hỏi ⟹ viết luôn từng lựa
                                // chọn thành lệnh chạm-được, ngay dưới màn.
                                if let Some(a) = shot_asking.as_ref() {
                                    out.push_str(&ask_command_lines(&shot_sid, a));
                                }
                                out
                            } else if matches!(cmd.kind, CommandKind::Pick) {
                                // Bảng nhiều câu đi đường riêng: nó phải ĐỌC
                                // trước khi gõ (đang đứng ở câu nào), và gõ cả
                                // dãy trong một lượt — xem `pick_answer`.
                                pick_answer(&s, w, &typed)
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
                            // Gõ CHỮ thì KHÔNG soi màn trước.
                            //
                            // Hà 2026-08-12, ba lần một ý: *"nhận lệnh từ tele
                            // thì làm luôn 2 việc là nhập nội dung và bấm enter,
                            // việc gì phải soi"*. Bản trước hỏi màn một lượt để
                            // biết có hộp chọn không — đúng về rủi ro, nhưng
                            // trên máy đang swap thì mỗi lượt hỏi là vài giây,
                            // và cái giá ấy trả cho MỌI câu chat trong khi hộp
                            // chọn là chuyện hiếm. Chủ máy chốt: cứ gõ.
                            //
                            // ⚠ Cái còn lại phải nói thẳng chứ không giấu: nếu
                            // đúng lúc ấy màn đang có hộp chọn thì Enter là
                            // CHỐT một lựa chọn. Đường an toàn khi biết có hộp
                            // chọn vẫn là `/key <số>`, và tin báo "dừng lại
                            // HỎI" vẫn hiện đủ lựa chọn để bấm.
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
                                            crate::sessions::shown(&s)
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
                                            crate::sessions::shown(&s), why
                                        ))
                                    }
                                }
                            } else {
                                None
                            };
                            if let Some(msg) = refusal {
                                msg
                            } else {
                            // 🔴 Hà 2026-08-14, ảnh chụp [dwork]: *"Bấm enter
                            // xong chưa thấy có tác dụng"*. Kiểm log thì hub ĐÃ
                            // gửi phím thật (`keys_typed kind=key`), và đọc màn
                            // ngay sau đó thì ô nhập TRỐNG, phiên đứng nguyên ở
                            // lượt cũ — tức Enter rơi vào một ô rỗng.
                            //
                            // Cái nằm trong ô lúc ấy là GỢI Ý MỜ của TUI (chữ
                            // xám bày lại từ lịch sử), không phải chữ ai gõ. Mà
                            // `contents of tab` bỏ sạch MÀU, nên hub không phân
                            // biệt nổi hai thứ ấy — bài học này đã ghi từ
                            // 08-13, và cái nút ⏎ vẫn hiện ra vì `input_box_text`
                            // đọc gợi ý mờ thành "ô có chữ".
                            //
                            // Chưa phân biệt được thì ĐỪNG khẳng định: chụp màn
                            // trước, so với màn sau, và nếu không đổi gì thì nói
                            // đúng như thế thay vì báo "✓ đã bấm".
                            let before = if is_key {
                                match crate::keys::look(&s.tty, 24) {
                                    crate::keys::Look::Saw { body, .. } => Some(body),
                                    _ => None,
                                }
                            } else {
                                None
                            };
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
                                    // 🔴 KHÔNG đọc một lần rồi kết luận.
                                    //
                                    // Bản cũ ngủ 900ms rồi soi đúng một phát. Hà
                                    // 2026-08-12: *"vừa rồi lại không tự enter
                                    // nên nó chỉ đứng trong ô chat"* · *"hình
                                    // như lúc được lúc không"*. Đó là một CUỘC
                                    // ĐUA, và máy này đang swap 12/13 GB nên nó
                                    // thua thường xuyên: TUI chưa kịp vẽ chữ ⟹
                                    // `still_in_box` = false ⟹ không bắn Enter
                                    // ⟹ chữ hiện ra ngay sau đó và nằm lại
                                    // vĩnh viễn. Cả ba cửa của cú Enter đều đọc
                                    // từ một tấm ảnh chụp quá sớm, nên cửa nào
                                    // cũng "đúng" theo một sự thật chưa xảy ra.
                                    //
                                    // Nay CHỜ tới khi màn nói được một trong hai
                                    // điều: phiên đã bắt đầu chạy/vào hàng chờ
                                    // (xong, không cần Enter), hoặc chữ đã hiện
                                    // trong ô (cần Enter). Hết 4,2 giây mà màn
                                    // vẫn không nói gì thì đừng bịa ra một câu
                                    // chắc chắn — xem chỗ trả lời bên dưới.
                                    // ENTER LIỀN TAY — hộp chọn đã kiểm TRƯỚC
                                    // khi gõ, nên ở đây không còn gì phải đoán.
                                    //
                                    // Vì sao vẫn phải là một cú ghi RỜI: `do
                                    // script` đẩy chữ + dấu xuống dòng trong
                                    // CÙNG một lượt ghi và TUI đọc lượt ấy như
                                    // một cú DÁN, nuốt luôn dấu xuống dòng vào
                                    // nội dung. Hai lượt ghi rời thì pty giữ
                                    // đúng thứ tự, và cú thứ hai tới như một
                                    // phím thật. Nghỉ 400ms để nó không bị gộp
                                    // ngược vào cú dán.
                                    // …và bấm LẠI cho tới khi ô nhập trống.
                                    //
                                    // 🔴 Hà 2026-08-12, ngay sau bản một-phát:
                                    // *"gửi xong im lặng mãi, gửi lần nữa lại
                                    // gộp thành 1 tin rồi enter"*. Đó là chữ ký
                                    // của việc cú Enter bị **gộp ngược vào cú
                                    // dán**: lượt gửi sau tạo một lượt ghi mới,
                                    // Enter tách ra được, và nó gửi luôn cả hai
                                    // đoạn. Tức 400ms là đủ ở máy rảnh và
                                    // KHÔNG đủ ở máy đang swap — một hằng số
                                    // thời gian bao giờ cũng sai với một cái
                                    // máy đang đổi tốc độ.
                                    //
                                    // Nên đừng đặt cược vào một con số: bấm,
                                    // NHÌN, còn chữ thì bấm nữa (tối đa 3 lần,
                                    // giãn dần). Một Enter thừa vào ô TRỐNG thì
                                    // `claude` không làm gì — nên hỏng về phía
                                    // an toàn. Thấy hộp chọn thì DỪNG ngay:
                                    // ở đó Enter là chốt.
                                    let mut sent_enter = false;
                                    if !is_key {
                                        // GÕ RỒI BẤM ENTER. Hết.
                                        //
                                        // 🔴 Hà 2026-08-12, sau khi tôi dựng
                                        // tới bản thứ tư có soi màn: *"không
                                        // hiểu soi kiểu gì, nhận lệnh từ tele
                                        // thì làm luôn 2 việc là nhập nội dung
                                        // và bấm enter, việc gì phải soi"*.
                                        // Đúng — và mọi bản trước đều hỏng vì
                                        // cùng một lý do: chúng treo một QUYẾT
                                        // ĐỊNH vào một tấm ảnh chụp, trên một
                                        // cái máy đang swap nên tấm ảnh ấy tới
                                        // muộn hơn sự thật.
                                        //
                                        // Bấm HAI lần, giãn nhau, và không hỏi
                                        // gì cả. Vì sao hai: `do script` đẩy
                                        // chữ + xuống dòng trong CÙNG một lượt
                                        // ghi nên TUI đọc như cú DÁN và nuốt
                                        // dấu xuống dòng — chữ ký của nó là câu
                                        // Hà tả: *"gửi lần nữa lại gộp thành 1
                                        // tin rồi enter"*. Cú Enter thứ hai vào
                                        // ô TRỐNG thì `claude` không làm gì,
                                        // nên lặp lại là an toàn theo đúng
                                        // nghĩa idempotent.
                                        for wait_ms in [400u64, 1000] {
                                            std::thread::sleep(std::time::Duration::from_millis(
                                                wait_ms,
                                            ));
                                            match crate::keys::press(w, "enter") {
                                                Ok(()) => sent_enter = true,
                                                Err(e) => {
                                                    logging::warn(
                                                        "keys_enter_failed",
                                                        json!({ "session": s.session_id,
                                                                "err": e.to_string() }),
                                                    );
                                                    break;
                                                }
                                            }
                                        }
                                        logging::info(
                                            "keys_enter_sent",
                                            json!({ "session": s.session_id,
                                                    "why": "gõ xong là bấm Enter — không soi, không đoán" }),
                                        );
                                    }
                                    let waited = std::time::Instant::now();
                                    // Không đọc lại được màn thì nói là KHÔNG
                                    // BIẾT. Bản trước rơi về `Landed::Idle`, tức
                                    // trả lời "phiên đang đứng ở dấu nhắc" cho
                                    // một chuyện chưa hề nhìn thấy — cùng họ với
                                    // con bug chốt mũi tên ngay phía trên: đọc
                                    // "mù" thành một khẳng định.
                                    let seen = |tty: &str| match crate::keys::look(tty, 24) {
                                        crate::keys::Look::Saw { body, choices } => {
                                            Some((body, choices))
                                        }
                                        _ => None,
                                    };
                                    // Soi màn CHỈ để báo cáo — không còn
                                    // quyết định nào treo vào nó nữa.
                                    let mut view = None;
                                    for _ in 0..6 {
                                        std::thread::sleep(std::time::Duration::from_millis(500));
                                        view = seen(&s.tty);
                                        if view.is_some() {
                                            break;
                                        }
                                    }
                                    logging::info(
                                        "keys_screen_waited",
                                        json!({ "session": s.session_id,
                                                "ms": waited.elapsed().as_millis() as u64,
                                                "saw": view.is_some(), "enter": sent_enter }),
                                    );
                                    // MỘT DÒNG XÁC NHẬN. Hết.
                                    //
                                    // 🔴 Hà 2026-08-12: *"tôi chỉ cần thông tin
                                    // xác nhận tin nhắn đã được vào hàng chờ
                                    // thành công chưa thôi, cần gì các thông
                                    // tin khác — nếu lỗi mới cần chi tiết"*.
                                    // Câu trả lời cũ dài ba dòng và một nửa nội
                                    // dung là RUỘT của hub (đã gõ bao nhiêu ký
                                    // tự, có phải bấm Enter rời không, màn nói
                                    // gì) — thứ chỉ có ích khi đi tìm lỗi, tức
                                    // đúng chỗ của một dòng log.
                                    let what = view
                                        .as_ref()
                                        .map(|(body, _)| crate::keys::landed(body));
                                    let name = crate::sessions::shown(&s);
                                    if is_key {
                                        let unchanged = match (&before, &view) {
                                            (Some(a), Some((b, _))) => a == b,
                                            _ => false,
                                        };
                                        if unchanged {
                                            logging::info(
                                                "keys_press_no_effect",
                                                json!({ "session": s.session_id,
                                                        "key": typed.trim(),
                                                        "why": "màn không đổi sau khi bấm" }),
                                            );
                                            format!(
                                                "⚠ đã bấm '{}' nhưng màn KHÔNG đổi · {name}\n                                                 Chữ trong ô nhập nhiều khả năng là GỢI Ý MỜ của TUI \
                                                 (hub đọc màn không thấy màu nên không phân biệt được \
                                                 với chữ đã gõ). Muốn gửi câu ấy thì gõ thẳng nó ở đây.",
                                                typed.trim()
                                            )
                                        } else {
                                        format!("✓ đã bấm '{}' · {name}", typed.trim())
                                        }
                                    } else if what == Some(crate::keys::Landed::Queued) {
                                        format!("✓ vào hàng chờ · {name}")
                                    } else {
                                        format!("✓ đã gửi · {name}")
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
                // `/shot` trên Telegram đi kèm NÚT cho từng lệnh thấy trên màn.
                // Đường đi vẫn là một: nút gõ dòng lệnh vào phiên qua `/type`,
                // tức cùng route, cùng sổ (xem `remember_quick`).
                let mut quick = Vec::new();
                // Giữ riêng ĐÚNG các dòng lệnh (không kèm "làm đi"), vì icon
                // trong chữ phải bám đúng dòng sinh ra nó — xem
                // `say_with_command_icons`.
                let mut cmd_lines: Vec<String> = Vec::new();
                if matches!(cmd.kind, CommandKind::Shot) {
                    let mut cmds = crate::keys::commands_on_screen(&ack, 4);
                    let n_cmds = cmds.len();
                    cmd_lines = cmds[..n_cmds].to_vec();
                    // Câu đồng ý nằm CÙNG kho với lệnh (một chỗ nhớ, một chỉ
                    // số), nhưng đi bằng callback KHÁC: `run:` gõ một DÒNG
                    // LỆNH, còn đây là một câu chữ thường.
                    let go = crate::keys::asks_for_go_ahead(&ack);
                    if go {
                        cmds.push("làm đi".to_string());
                    }
                    let stored = remember_quick(db, &shot_sid, &cmds);
                    quick.extend(stored.into_iter().take(n_cmds));
                    if go {
                        quick.push(("✅ Làm đi".to_string(), format!("say:{n_cmds}")));
                    }
                    // …và TỆP thấy trên màn cũng phải mở được ngay tại đây.
                    //
                    // 🔴 Hà 2026-08-13: *"trong nội dung có khá nhiều file
                    // nhưng lại không mở được trên tele, mở nó lại ra trình
                    // duyệt"*. Thứ anh bấm là **link Telegram tự bắt** (nó thấy
                    // `DEPLOY.md` thì đoán là tên miền), không phải nút của
                    // hub — mà nút của hub lúc ấy chỉ gắn ở tin TỰ PHÁT, còn
                    // `/shot` thì không. Cùng một màn, hai luật khác nhau là
                    // thứ người dùng đọc thành "lúc được lúc không".
                    quick.extend(remember_files(
                        db,
                        cfg,
                        &want,
                        &crate::keys::paths_on_screen(&ack, 4),
                    ));
                }
                // Ô nhập đang có sẵn chữ ⟹ một nút GỬI (Hà 2026-08-13: *"có gợi
                // ý nội dung chat cần có cách bấm nhanh để gửi nó"*). Đi đúng
                // route `/key <id> enter` đã có — nút chỉ là phím tắt của một
                // đường đi sẵn, không phải một nhánh xử lý mới.
                if matches!(cmd.kind, CommandKind::Shot) {
                    // 🔴 Hà 2026-08-14: *"Sao có 2 nút làm đi"*. Vì đúng hai
                    // khối cùng dựng nó: khối trên (`say:<n>`, cùng kho với
                    // lệnh) và khối này (`run:0`, một kho riêng). Cả hai đều
                    // "đúng" một mình, cùng đọc một tín hiệu
                    // (`asks_for_go_ahead`), cùng đổ vào một danh sách — và
                    // chẳng chỗ nào hỏi "đã có ai dựng nút này chưa". Khối này
                    // đi, khối trên ở lại: nó nằm cùng chỗ với các nút lệnh nên
                    // thứ tự nút khớp thứ tự chữ trên màn.
                    //
                    // Bảng hỏi thì phải chọn được NGAY TẠI ĐÂY — cùng bộ nút
                    // với tin tự phát (`telegram::choice_buttons`), không đẻ
                    // lối riêng: `pick:<id>:<câu>.<lựa chọn>` cho mọi câu, kèm
                    // `✅ Gửi lựa chọn`.
                    // Bảng hỏi đi bằng CHỮ chạm được, không bằng khối nút — xem
                    // `ask_command_lines`. Phần chữ ấy nối vào cuối `ack` ngay
                    // dưới, chứ không dựng nút nào.
                    // Ô nhập có chữ ⟹ một cái nút GỬI. Đúng một icon.
                    //
                    // 🔴 Hà 2026-08-13, ảnh chụp nút `⏎ Gửi: # Lệnh thấy trên
                    // màn — bấm nút dướ…`: *"sao gợi ý mờ tạo nút lại vô duyên
                    // thế"* → *"không phải bạn tự chèn à"* → *"chỉ cần icon
                    // send là đủ"* → *"focus đúng mục tiêu"*.
                    //
                    // Anh chỉ đúng hai chỗ. Một: dòng trên nhãn là chữ CỦA
                    // CHÍNH HUB (`pipeline.rs` chèn `# Lệnh thấy trên màn…` vào
                    // bản trả lời `/shot`), nên cái nút đang mời gửi lại lời
                    // của chính nó. Hai: bản cũ ĐỌC chữ trong ô rồi GÕ LẠI chữ
                    // ấy — mà cái phân biệt "chữ đã gõ" với "gợi ý mờ" chính là
                    // MÀU, thứ `contents of tab` bỏ sạch. Đọc-rồi-gõ-lại là
                    // dựng một hành động lên trên một phép đo không làm được.
                    //
                    // Nút nay là một CỬ CHỈ, không phải một nội dung: bấm Enter
                    // vào đúng cửa sổ ấy, y như ngón tay chủ máy. hub không cần
                    // biết trong ô có gì — gõ dở, gợi ý đã nhận, hay rỗng — vì
                    // Enter làm đúng một việc ở cả ba ca. Không đọc thì không
                    // đoán sai được.
                    //
                    // Hai cửa giữ nó khỏi bấm nhầm chỗ, cả hai đọc từ CHÍNH màn
                    // vừa chụp: phiên đang chạy thì thôi (chân màn mang `esc to
                    // interrupt`), và có hộp chọn thì thôi — ở đó Enter là CHỐT
                    // một lựa chọn, không phải gửi một câu; hộp chọn đã có bộ
                    // nút số riêng.
                    // 🔴 Hà 2026-08-14, chỉ vào cái nút ấy trên ảnh `/shot` của
                    // một phiên đang mở bảng hỏi: *"1 nút enter để làm gì"*.
                    // Câu hỏi đúng, và câu trả lời là: ở màn ĐÓ nó không được
                    // phép có mặt. Cửa chặn viết đúng ý ("có hộp chọn thì
                    // thôi") nhưng hỏi bằng `parse_choices`, thứ MÙ với bảng
                    // nhiều câu — nên cửa mở, nút hiện ra, và bấm vào là CHỐT
                    // cái lựa chọn đang tô cho một câu chủ máy chưa chọn. Một
                    // phép đo mù ở cổng an toàn thì hỏng về phía nguy hiểm.
                    //
                    // Hỏi cả ba nguồn: hộp chọn trên màn · thanh tab của bảng ·
                    // bảng đọc từ nhật ký (đúng cho cả phiên hub không đọc được
                    // màn).
                    let running = ack.contains("esc to interrupt");
                    let has_choices = !crate::keys::parse_choices(&ack).is_empty()
                        || crate::keys::ask_table(&ack).is_some()
                        || shot_asking.is_some();
                    // 🔴 Hà 2026-08-14: *"Sao ô nhắc trống, cũng không có gợi ý
                    // mờ mà vẫn có nút enter"*. Vì điều kiện trên KHÔNG hỏi
                    // trong ô có gì — chú thích cũ tự bào chữa rằng nút này là
                    // "một CỬ CHỈ, không phải một nội dung", đúng về bản chất
                    // phím và sai về chỗ đặt: một cử chỉ gửi vào ô rỗng thì
                    // không gửi gì cả, nên cái nút chỉ còn là tiếng ồn — và
                    // tiếng ồn trên màn 390px thì đắt.
                    //
                    // Đọc ô nhập CHỈ để quyết định CÓ HIỆN NÚT hay không, không
                    // để dựng nội dung: đó đúng ranh giới mà bài học cũ đặt ra
                    // (không phân biệt được "gợi ý mờ" với "chữ đã gõ" vì màn
                    // đọc về mất màu). Có chữ hay không thì đọc được; chữ ấy
                    // của ai thì không, và ở đây không cần biết.
                    let box_has_text =
                        crate::keys::input_box_text(&ack).is_some_and(|t| !t.trim().is_empty());
                    if !running && !has_choices && box_has_text && !shot_sid.is_empty() {
                        quick.push(("⏎".to_string(), format!("key:{shot_sid}:enter")));
                    }
                    // text đó tới phiên"*) — nhưng nó đứng trên một phép đo hub
                    // KHÔNG làm được.
                    // phân biệt 'chữ đã gõ' với 'gợi ý mờ' CHÍNH LÀ màu"*, mà
                    // `contents of tab` bỏ sạch màu. Rồi kết luận ngược lại —
                    // *"đừng đoán… cứ gửi thẳng chữ ấy đi"* — tức đúng là ĐOÁN,
                    // và đoán về phía nguy: gợi ý mờ là thứ TUI tự bày ra từ
                    // lịch sử, không phải câu chủ máy định gửi. Một cái nút mời
                    // gửi nó đi là mời gửi một câu không ai viết.
                    //
                    // Cùng luật với `keys::look` trả `Blind` thay vì `None`:
                    // không đo được thì NÓI KHÔNG BIẾT, đừng dựng một hành động
                    // lên trên chỗ trống. Muốn gửi chữ ấy thì gõ thẳng nó ở
                    // Telegram — đường ấy có sẵn và không phải đoán gì cả.
                }
                match (quick.is_empty(), crate::telegram::inbox()) {
                    (false, Some(tg)) if adapter == crate::telegram::NAME => {
                        // 🔴 Hà 2026-08-14, ảnh chụp câu trả lời `/shot`: *"Vẫn
                        // thấy hiện nút kiểu cũ là sao"*. Vì đây là đường THỨ BA
                        // dùng cùng cuốn sổ nút, và nó chưa được nối vào máy móc
                        // icon-trong-chữ (hai đường kia: tin tự phát, bản đầy
                        // đủ). Đúng cái hình dạng đã lặp ba lần trong tệp này:
                        // vá một chỗ, quên chỗ bên cạnh, vì không ai bắt ba chỗ
                        // phải giống nhau. Nay CHÚNG GỌI CHUNG MỘT HÀM.
                        say_with_command_icons(
                            tg,
                            &ack,
                            &cmd_lines,
                            &quick,
                            "quick_buttons_failed",
                        );
                    }
                    _ => reply_in_channel(db, cfg, adapter, cmd, &ack),
                }
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
                let live =
                    crate::sessions::snapshot_cached(cfg, std::time::Duration::from_secs(20));
                // Phiên VỪA TẮT vẫn hỏi được: `--resume` chạy trên nhật ký, không
                // cần tiến trình. Đây đúng là ca Hà gặp 16:37 — con trỏ trỏ vào
                // phiên vừa tắt và hub trả lời bằng một ngõ cụt. Xem `ENDED_KEY`.
                let target = live
                    .sessions
                    .iter()
                    .find(|s| same_session(&s.session_id, &want))
                    .cloned()
                    .or_else(|| ended_session(db, &want));
                let from_ended = target.is_some()
                    && !live
                        .sessions
                        .iter()
                        .any(|s| same_session(&s.session_id, &want));
                let ack = match target {
                    None if want.is_empty() => {
                        "⚠ chưa mở phiên nào. Chạm một phiên trên màn Phiên rồi hỏi lại."
                            .to_string()
                    }
                    None => format!(
                        "⚠ không thấy phiên '{}' — nó không còn chạy và cũng không nằm trong \
                         sổ phiên vừa tắt (giữ 24 giờ).\nĐang sống: {}",
                        crate::exec::truncate(&want, 12),
                        if live.sessions.is_empty() {
                            "không có phiên nào".to_string()
                        } else {
                            live.sessions
                                .iter()
                                .map(|s| {
                                    format!(
                                        "{} ({})",
                                        s.name,
                                        &s.session_id[..8.min(s.session_id.len())]
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(" · ")
                        }
                    ),
                    Some(ref s) => match crate::sessions::ask_aside(cfg, s, &asked) {
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
                            // Nói rõ phiên ấy ĐÃ TẮT: người đọc phải biết câu
                            // trả lời dựng từ nhật ký chứ không phải từ một
                            // phiên đang chạy — hai thứ đó khác nhau ở chỗ
                            // "hỏi tiếp được không".
                            let da_tat = if from_ended {
                                "⏹ phiên này đã tắt — trả lời dựng từ nhật ký của nó.\n\n"
                            } else {
                                ""
                            };
                            format!(
                                "{da_tat}💬 Hỏi bên lề phiên {} ({how}):\n\n{}",
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
                // `/session` KHÔNG kèm id = "cho tôi xem có những phiên nào".
                // Câu trả lời cũ ("Chưa theo phiên nào. Chọn một phiên trên màn
                // Phiên.") đẩy người hỏi sang một màn khác — dùng được khi lệnh
                // này chỉ chạy từ chính màn ấy, vô dụng khi nó tới từ Telegram.
                if want.is_empty() {
                    let live =
                        crate::sessions::snapshot_cached(cfg, std::time::Duration::from_secs(20));
                    let focus = db.cursor_or_log(FOCUS_SESSION_KEY).unwrap_or_default();
                    let mut ack = session_list_text(
                        &live.sessions,
                        &focus,
                        chrono::Utc::now().timestamp_millis(),
                    );
                    // Việc đang chạy nền cũng là thứ "máy này đang làm gì", nên
                    // nó thuộc về đúng cái danh sách người ta mở nhiều nhất —
                    // chứ không phải một route `/jobs` thứ hai phải nhớ tên.
                    // Chỉ hiện khi CÓ việc: một dòng "không có việc nào" trên
                    // màn 390px là một dòng trống trả bằng chỗ.
                    if let Some(jobs) = jobs_line() {
                        ack.push_str("\n\n⚡ đang chạy trên máy:\n");
                        ack.push_str(&jobs);
                    }
                    // Trên Telegram mỗi phiên là một NÚT, và gửi ngay tại đây —
                    // để `reply_in_channel` gửi thêm lần nữa là chủ máy nhận hai
                    // tin cùng nội dung, một cái bấm được một cái không.
                    let mut sent = false;
                    if adapter == crate::telegram::NAME {
                        if let Some(tg) = crate::telegram::inbox() {
                            let buttons: Vec<(String, String)> = live
                                .sessions
                                .iter()
                                .take(MAX_SESSION_BUTTONS)
                                .map(|s| {
                                    (session_button_label(s), format!("sess:{}", s.session_id))
                                })
                                .collect();
                            match tg.send_buttons(&ack, &buttons) {
                                Ok(()) => sent = true,
                                // Hỏng thì rơi về đường chữ thường bên dưới,
                                // đừng nuốt: thà một tin không nút còn hơn im.
                                Err(e) => logging::error(
                                    "telegram_ack_failed",
                                    json!({ "err": e, "what": "session_buttons" }),
                                ),
                            }
                        }
                    }
                    if !sent {
                        reply_in_channel(db, cfg, adapter, cmd, &ack);
                    }
                    // Giá trị của NHÁNH này, không phải `return`: `return` ở đây
                    // sẽ bỏ luôn những lệnh còn lại trong cùng một lượt.
                    Some(ack)
                } else {
                    let ack = if want == "-" || want.eq_ignore_ascii_case("off") {
                        match db.set_cursor(FOCUS_SESSION_KEY, "") {
                            Ok(()) => "👁 Đã thôi theo phiên.".to_string(),
                            Err(e) => format!("⚠ không bỏ theo được: {e}"),
                        }
                    } else if let Some((name, account)) = db
                        .cursor_or_log(WATCH_KEY)
                        .and_then(|v| session_name_from_book(&v, want))
                    {
                        // ĐƯỜNG NHANH: sổ đã biết phiên này. Đặt con trỏ rồi chào
                        // ngay — xem `session_name_from_book` để biết vì sao đường
                        // cũ mất 48 giây cho đúng hai chuỗi ký tự.
                        match db.set_cursor(FOCUS_SESSION_KEY, want) {
                            Ok(()) => {
                                let head = format!("👁 Đang theo phiên {name} ({account})");
                                // …và ĐƯA LUÔN MÀN, đừng bắt bấm thêm một lần.
                                //
                                // 🔴 Hà 2026-08-13: *"bấm vào phiên sao không hiện
                                // shot luôn mà nhận thông báo đã vào phiên rồi lại
                                // phải bấm lệnh shot"*. Đây là đảo lại quyết định
                                // 12-08 — và đảo có căn cứ, vì căn cứ cũ đã hết
                                // đúng: hôm ấy bỏ cú chụp vì nó tốn **16 giây** cho
                                // mỗi lần bấm nút. Sau khi `/shot` thôi dựng lại
                                // ảnh chụp phiên (sổ + `ps`), đo lại tối nay:
                                // `command_done Shot` **2,7s · 4,5s**. Cái giá biến
                                // mất thì lý do cũng biến mất.
                                //
                                // Xếp hàng `/shot` chứ không gọi thẳng: cùng route,
                                // cùng sổ, cùng cách dựng nút — chỉ khác là không
                                // phải ngón tay nào bấm.
                                if adapter == crate::telegram::NAME {
                                    if let Some(tg) = crate::telegram::inbox() {
                                        tg.push_text("/shot");
                                    }
                                }
                                head
                            }
                            Err(e) => format!("⚠ không theo được: {e}"),
                        }
                    } else {
                        // Sổ không biết id này: phiên vừa tắt, id gõ tay, hoặc trang
                        // cũ. Lúc ấy mới đáng trả tiền một lượt ảnh chụp — vừa để
                        // xác nhận, vừa để câu từ chối nói được "đang có N phiên".
                        let live = crate::sessions::snapshot_cached(
                            cfg,
                            std::time::Duration::from_secs(20),
                        );
                        // Phiên VỪA DỪNG vẫn phải theo được: màn chi tiết đang mở
                        // chính nó, và `/tell` sau đó cần đúng con trỏ này. Không có
                        // vế dưới thì bấm Dừng xong là màn tự đá mình ra — đo được
                        // 2026-08-09, và nó nuốt luôn cả đường /tell.
                        let target = live
                            .sessions
                            .iter()
                            .find(|s| same_session(&s.session_id, want))
                            .cloned()
                            .or_else(|| stopped_session(db, want));
                        match target {
                            Some(s) => match db.set_cursor(FOCUS_SESSION_KEY, want) {
                                Ok(()) => {
                                    // Nói ĐÚNG cái còn làm được, không nói chung chung.
                                    //
                                    // 🔴 Hà 2026-08-13: bấm vào một phiên nền đã
                                    // chết, hub chào *"đã dừng, vẫn nói tiếp được"*,
                                    // rồi `/shot` ngay sau đó trả *"không có cửa sổ
                                    // terminal để gõ (host: dead)"*. Hai câu của
                                    // cùng một hub, cách nhau vài giây, chọi nhau.
                                    // Câu đầu đúng theo nghĩa hẹp (`/tell` dựng lại
                                    // được phiên nền) nhưng người đọc hiểu thành
                                    // "gõ tiếp được", vì đó là thứ mọi phiên khác
                                    // cho phép.
                                    let how = match (s.pid == 0, s.host.as_str()) {
                                    (_, "dead") => {
                                        " — ĐÃ TẮT: chỉ còn /handover lấy bản bàn giao; gõ thẳng thì không. \
                                         Dọn khỏi danh sách bằng /stop"
                                    }
                                    (true, _) => " — đã dừng, /tell nói tiếp được",
                                    _ => "",
                                };
                                    let head = format!(
                                        "👁 Đang theo phiên {} ({}){}",
                                        s.name, s.account, how
                                    );
                                    // KHÔNG chụp lại màn ở đây (Hà 2026-08-12).
                                    //
                                    // Từ 08-11 câu ack này kèm luôn màn, vì *"bấm
                                    // xong muốn thấy MÀN phiên"*. Đo lại hôm nay
                                    // thì cái giá của nó lộ ra: một cú bấm nút mất
                                    // **42 giây**, trong đó **16 giây** là chính
                                    // bước đọc màn bằng osascript rồi đẩy một ảnh
                                    // chụp mới.
                                    //
                                    // Mà từ hôm nay tin báo đã mang sẵn thông tin
                                    // chốt của lượt cuối (S18) — Hà: *"thông báo đó
                                    // của phiên đã đủ nội dung gần nhất rồi nên
                                    // thông báo đã vào phiên không cần chụp lại
                                    // nữa"*. Trả tiền 16 giây để in lại thứ vừa đọc
                                    // xong ở tin trên là trả cho một bản sao.
                                    //
                                    // Muốn nhìn màn thì `/shot` — một động từ, một
                                    // việc; ack nói ra đường ấy để không ai phải
                                    // đoán.
                                    if adapter == crate::telegram::NAME {
                                        format!("{head}\n(xem màn: /shot)")
                                    } else {
                                        head
                                    }
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
        logging::info(
            "command_done",
            json!({ "kind": format!("{:?}", cmd.kind), "adapter": adapter,
                    "ms": cmd_started.elapsed().as_millis() }),
        );
        if let Some(ack) = answered {
            logging::info(
                "channel_command_handled",
                json!({ "adapter": adapter, "decision_id": cmd.decision_id, "kind": format!("{:?}", cmd.kind), "ack": ack }),
            );
            continue;
        }
    }
}

/// Khoá cho MỘT lượt chạy lệnh tại một thời điểm.
///
/// Từ 2026-08-12 lệnh Telegram chạy ở hai chỗ: đầu mỗi vòng (như cũ) và **ngay
/// lúc bấm** (`run_telegram_now`). Hai chỗ ấy đụng cùng một hàng đợi, cùng cuốn
/// sổ, và cùng một Terminal để gõ phím vào — nên chúng phải xếp hàng. Khoá đặt
/// ở NGUỒN, không ở từng chỗ gọi: chỗ gọi thứ ba sẽ quên.
static CMD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Đang có một luồng chạy-ngay hay chưa — để một tràng nút bấm không đẻ ra
/// mười luồng.
static RUNNING_NOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Chạy lệnh Telegram **ngay**, không đợi vòng đang chạy dở kết thúc.
///
/// 🔴 Đo 2026-08-12, đúng một cú bấm nút "Vào phiên": bấm **18:17:45** → lệnh
/// bắt đầu chạy **18:18:11** → trả lời **18:18:27**. Bốn mươi hai giây, và
/// **26 giây** đầu chỉ là ngồi chờ một vòng đang chạy (vòng ấy 27,4s) rồi chờ
/// đẩy xong ảnh chụp. `waker` đã có từ 08-11 nhưng nó chỉ cắt được GIẤC NGỦ —
/// một vòng đang chạy dở thì không đánh thức được, nó phải chạy hết.
///
/// Vì sao là một luồng riêng chứ không chạy thẳng trong luồng đọc Telegram: chỗ
/// ấy đang giữ đường long-poll `getUpdates`; một lệnh chạy 15 giây ở đó là 15
/// giây không nghe được nút tiếp theo.
pub fn run_telegram_now(cfg: &Config) {
    use std::sync::atomic::Ordering;
    if RUNNING_NOW.swap(true, Ordering::SeqCst) {
        // Đã có luồng đang chạy — nó sẽ vét nốt hàng đợi trước khi thoát.
        return;
    }
    let cfg = cfg.clone();
    let spawned = std::thread::Builder::new()
        .name("telegram-now".into())
        .spawn(move || {
            // Luồng này SINH RA từ một cú bấm, nên cả luồng là việc gấp — kể cả
            // những khúc chạy trước khi tới `execute_commands` (dựng lại ảnh
            // chụp phiên để biết bấm vào phiên nào, chẳng hạn).
            let _lane = crate::exec::urgent();
            loop {
                match Db::open(&cfg.db) {
                    Ok(db) => execute_telegram_commands(&db, &cfg),
                    Err(e) => {
                        logging::error("telegram_now_db_failed", json!({ "err": e.to_string() }));
                        break;
                    }
                }
                // Lệnh mới tới TRONG LÚC đang chạy thì làm nốt ở đây, đừng để
                // nó rơi lại vào chỗ phải đợi trọn một vòng.
                if !crate::telegram::inbox().is_some_and(|i| i.has_pending()) {
                    break;
                }
            }
            RUNNING_NOW.store(false, Ordering::SeqCst);
        });
    if let Err(e) = spawned {
        // Không nuốt: không dựng được luồng thì lệnh vẫn chạy ở đầu vòng sau,
        // chậm chứ không mất — nhưng phải có dòng nói ra vì sao nó chậm.
        RUNNING_NOW.store(false, Ordering::SeqCst);
        logging::error("telegram_now_spawn_failed", json!({ "err": e.to_string() }));
    }
}

/// Chạy những mệnh lệnh vừa gõ trên TELEGRAM.
///
/// Cùng `parse_command`, cùng `execute_commands`, cùng bộ handler với phòng chat
/// — đúng luật 12 của dự án ("một đường, một cuốn sổ"). Khác đúng hai chỗ, và cả
/// hai đều là chuyện của KÊNH chứ không phải của lệnh:
///
/// * **Cổng người:** `chat_id` — `telegram.rs` đã bỏ mọi tin từ buồng khác
///   trước khi tới đây, nên tới được đây tức là chủ máy gõ. Đây là cổng DUY
///   NHẤT từ 2026-08-14: cổng thứ hai (`trust.tfl5_user_tids`, kiểm trong
///   `parse_command`) đi cùng phòng chat — xem `verbs::parse_command`.
/// * **Chỗ trả lời:** ack đi ngược về Telegram (`adapter = "telegram"`).
fn execute_telegram_commands(db: &Db, cfg: &Config) {
    let _guard = CMD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(inbox) = crate::telegram::inbox() else {
        return;
    };
    let pending = inbox.drain();
    if pending.is_empty() {
        return;
    }
    let mut cmds: Vec<ChannelCommand> = Vec::new();
    for item in pending {
        match verbs::parse_command(&item.text) {
            Some((kind, decision_id, arg)) => cmds.push(ChannelCommand {
                quiet: item.quiet,
                kind,
                decision_id,
                arg,
                chat_id: inbox.chat_id().to_string(),
                callback_id: String::new(),
                // Tin đã sinh ra lệnh — chỗ trả lời sẽ thả emoji lên chính nó.
                message_id: item.msg_id,
            }),
            // Không phải lệnh — hai đường rất khác nhau, xem `text_for_session`.
            None => match text_for_session(&item.text) {
                // CHỮ THƯỜNG = gõ thẳng vào phiên đang theo.
                //
                // Hà 2026-08-11, sau khi bấm nút chọn phiên: *"bấm vào mỗi phiên
                // focus vào phiên đó luôn"* — chọn xong thì coi như đang ngồi
                // trong phiên ấy, gõ gì là nó nhận nấy, không phải nhớ thêm một
                // động từ. Câu trả lời cũ ("Chưa hiểu — kênh này nhận LỆNH") bắt
                // người ta gõ `/type` trước mỗi câu, tức bắt nhớ một luật của
                // mã ngay giữa lúc đang làm việc.
                //
                // **Id đi CÙNG mệnh lệnh**, lấy ngay lúc nhận chứ không để
                // handler tự tra con trỏ sau: đó đúng con đường đã gõ nhầm phiên
                // sáng 2026-08-11 (`split_target`).
                Some(text) => {
                    let focus = db.cursor_or_log(FOCUS_SESSION_KEY).unwrap_or_default();
                    if focus.is_empty() {
                        // Không có phiên nào đang theo thì KHÔNG đoán một phiên
                        // để gõ vào: gõ nhầm cửa sổ là thứ không lùi lại được.
                        logging::info("telegram_text_no_focus", json!({ "len": text.len() }));
                        if let Err(e) = inbox.send_text(
                            "Chưa theo phiên nào nên tôi chưa biết gõ vào đâu.\n\
                             Gõ /sessions rồi bấm một phiên — sau đó chữ gõ ở đây đi thẳng \
                             vào phiên ấy.",
                        ) {
                            logging::error("telegram_ack_failed", json!({ "err": e }));
                        }
                    } else {
                        // Nội dung KHÔNG vào log (chữ của chủ máy, log là tệp
                        // nằm lâu) — ghi đủ để truy: phiên nào, dài bao nhiêu.
                        logging::info(
                            "telegram_text_as_typing",
                            json!({ "session": focus, "len": text.len() }),
                        );
                        cmds.push(ChannelCommand {
                            quiet: item.quiet,
                            kind: CommandKind::Type,
                            decision_id: 0,
                            arg: format!("{focus} {text}"),
                            chat_id: inbox.chat_id().to_string(),
                            callback_id: String::new(),
                            message_id: item.msg_id,
                        });
                    }
                }
                // Tự xưng là lệnh mà không có handler: KHÔNG gõ nó vào phiên.
                // Một động từ gõ nhầm (`/sesion`) mà bị bơm vào cửa sổ đang chạy
                // thì hub biến lỗi chính tả thành một lượt gõ thật.
                None => {
                    let head = crate::exec::truncate(item.text.trim(), 40);
                    logging::info("telegram_not_a_command", json!({ "head": head }));
                    if let Err(e) = inbox.send_text(
                        "Chưa hiểu lệnh này — gõ /help để xem danh sách.\n\
                         (Muốn gõ một dòng bắt đầu bằng dấu gạch chéo VÀO phiên thì dùng \
                         /type <dòng đó>.)",
                    ) {
                        logging::error("telegram_ack_failed", json!({ "err": e }));
                    }
                }
            },
        }
    }
    if !cmds.is_empty() {
        logging::info("telegram_commands_run", json!({ "count": cmds.len() }));
        execute_commands(db, cfg, crate::telegram::NAME, &cmds);
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
    // Câu "đã gửi yêu cầu sang Telegram" chỉ có nghĩa với người KHÔNG ở
    // Telegram.
    //
    // 🔴 Cùng ảnh chụp ấy: bấm `/close` trên Telegram và nhận ngay `🔒 Đã gửi
    // yêu cầu xác nhận sang Telegram: …` — hub nói với Telegram rằng nó vừa gửi
    // một thứ sang Telegram, và cái thứ ấy hiện ngay dòng dưới. Một tin chỉ để
    // giới thiệu tin kế tiếp.
    //
    // Với phòng chat tfl5 thì câu ấy vẫn cần: ở đó người đọc KHÔNG thấy hộp
    // xác nhận, nên không nói ra là màn hình đứng im không lý do.
    if cfg.confirm.enabled && adapter != crate::telegram::NAME {
        reply_in_channel(
            db,
            cfg,
            adapter,
            cmd,
            &format!(
                "🔒 Đã gửi yêu cầu xác nhận sang Telegram: {what} Chưa làm gì cho tới khi bấm nút."
            ),
        );
    }
    let verdict = crate::confirm::ask(cfg, what);
    if verdict.allows() {
        None
    } else {
        Some(verdict.refusal(nothing_done))
    }
}

/// Ghi thời gian trả lời lúc RỜI hàm, kể cả khi hàm thoát sớm.
struct AckClock {
    adapter: String,
    at: std::time::Instant,
}
impl Drop for AckClock {
    fn drop(&mut self) {
        logging::info(
            "ack_sent_ms",
            json!({ "adapter": self.adapter, "ms": self.at.elapsed().as_millis() }),
        );
    }
}
fn scopeguard_log(adapter: &str, at: std::time::Instant) -> AckClock {
    AckClock {
        adapter: adapter.to_string(),
        at,
    }
}

/// Câu trả lời này có rút gọn được thành MỘT emoji không — và emoji nào.
///
/// Hàm thuần, kiểm được. Luật hẹp có chủ ý: chỉ những câu **xác nhận trơn** mới
/// đổi được, tức câu mà toàn bộ nội dung là "đã nhận, xong". Mọi câu còn lại —
/// từ chối (`⚠`), báo lỗi, câu có số liệu, câu mời làm bước tiếp — phải giữ
/// nguyên chữ: một mặt cười thay cho một lời từ chối là giấu mất đúng thứ người
/// ta cần đọc.
///
/// Emoji lấy từ BẢNG CỐ ĐỊNH của Telegram (`ReactionTypeEmoji`) — `✓` và `▶`
/// không nằm trong bảng ấy nên không dùng được, dù chúng hợp nghĩa hơn.
/// Emoji RIÊNG của một dự án, sinh từ chính cái tên.
///
/// 🔴 Hà 2026-08-14: *"tự tạo emoji theo tên dự án được không? → add vào để
/// dùng làm phản hồi"*. Được, và nó nói được nhiều hơn một dấu 👍 chung: nhìn
/// cái dấu là biết chữ vừa rơi vào phiên NÀO — thứ đáng biết nhất khi trong máy
/// có bốn phiên và hai trong số đó cùng tên dự án.
///
/// Sinh từ tên chứ không tra bảng tay: dự án mới thêm vào là có dấu ngay, không
/// phải sửa mã. Cùng một tên thì luôn ra cùng một dấu (tổng byte, chia dư) —
/// một cái dấu đổi giữa chừng còn tệ hơn không có dấu, vì người ta học nó rồi
/// bị nó lừa (cùng bài học với nhãn màu đổi sau mỗi lần khởi động).
///
/// Bảng chỉ gồm emoji Telegram CHO PHÉP thả (`ReactionTypeEmoji`) — ngoài bảng
/// ấy thì API từ chối, và một dấu bị từ chối là một tin chữ mọc lại.
pub fn project_emoji(name: &str) -> &'static str {
    // Cố ý chọn những dấu KHÁC HẲN nhau về hình, không phải sắc thái cảm xúc:
    // trên một dòng chat, người ta phân biệt bằng bóng hình chứ không bằng nét
    // mặt.
    const PALETTE: &[&str] = &[
        "🔥", "🎉", "🐳", "🦄", "🍓", "🍌", "🏆", "⚡", "💯", "🌭", "🎄", "👻", "👾", "🎃", "🙈",
        "🗿", "🆒", "💊", "😎", "🕊",
    ];
    let key = name
        .trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_lowercase();
    if key.is_empty() {
        return "👍";
    }
    // FNV-1a chứ không phải tổng byte: bản đầu cộng byte rồi chia dư, và trên
    // đúng bảy cái tên có thật của máy này (`hub · tfl5 · dwork · sdvi ·
    // codetrail · social · anpha1`) nó ra chỉ **năm** dấu — hai cặp trùng nhau,
    // tức mất đúng cái công dụng vừa dựng. Tổng byte không phân biệt được thứ
    // tự chữ, mà tên dự án thì thường cùng bộ chữ cái.
    let mut h: u32 = 0x811c_9dc5;
    for b in key.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    PALETTE[(h as usize) % PALETTE.len()]
}

/// Tên dự án của phiên ĐANG THEO — để dấu thả lên một tấm ảnh nói được nó vừa
/// tới phiên nào.
///
/// `None` khi chưa theo phiên nào hoặc sổ chưa biết dự án: chỗ gọi rơi về dấu
/// chung, chứ không đoán một cái tên.
pub fn project_of_focus(cfg: &Config) -> Option<String> {
    let db = Db::open(&cfg.db).ok()?;
    let id = db.cursor_or_log(FOCUS_SESSION_KEY)?;
    let book = db.cursor_or_log(WATCH_KEY)?;
    let marks: std::collections::BTreeMap<String, crate::watch::Mark> =
        serde_json::from_str(&book).ok()?;
    let folder = marks.get(&id).map(|m| m.d.clone())?;
    let name = folder.trim_matches('/').rsplit('/').next()?.to_string();
    if name.is_empty() {
        return None;
    }
    // Trả về TÊN, không phải dấu: `ack_emoji` tự băm tên: đưa cho nó một cái
    // dấu thì nó băm luôn cái dấu ấy và ra một dấu khác — một lỗi im lặng,
    // suýt nữa đã vào bản cài (bắt được lúc đọc lại kiểu trả về).
    Some(name)
}

/// Tên dự án nằm trong `[...]` của một câu trả lời, nếu có.
fn project_in(ack: &str) -> Option<&str> {
    let i = ack.find('[')?;
    let j = ack[i + 1..].find(']')? + i + 1;
    let name = &ack[i + 1..j];
    (!name.is_empty() && name.len() <= 40).then_some(name)
}

/// Một TRẠNG THÁI mà hub có thể trả lời bằng đúng một dấu.
///
/// 🔴 Hà 2026-08-14: *"viết bộ render tự động emoji các trạng thái cho từng ứng
/// dụng?"*. Đây là cái bảng ấy — một chỗ khai, mọi chỗ đọc.
///
/// Luật chia dấu, và nó không tuỳ hứng: **trạng thái nào mà câu hỏi kế tiếp là
/// "vào phiên nào?" thì mang dấu của DỰ ÁN; còn lại mang dấu của TRẠNG THÁI.**
/// Gõ một câu vào phiên xong, thứ đáng biết là nó rơi vào đâu (bốn phiên đang
/// mở, hai cùng tên dự án). Còn "vào hàng chờ" hay "đang chạy" thì thứ đáng
/// biết là chính cái trạng thái ấy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ack {
    /// Chữ đã tới phiên.
    Sent,
    /// Phiên đang bận nên chữ nằm trong hàng chờ của TUI.
    Queued,
    /// Con trỏ theo dõi vừa chuyển.
    Focused,
    /// Một lệnh dài vừa bắt đầu chạy nền.
    Running,
    /// Đã dừng theo yêu cầu.
    Stopped,
    /// Tệp gửi lên đã ghi xuống đĩa.
    Saved,
    /// Phiên đã được cho xem thứ vừa gửi.
    Seen,
}

/// Bộ sinh dấu: (ứng dụng, trạng thái) → đúng một emoji Telegram thả được.
pub fn ack_emoji(project: Option<&str>, k: Ack) -> &'static str {
    match k {
        // Hai cái này trả lời câu "vào phiên nào" ⟹ mang dấu của dự án.
        Ack::Sent => project.map(project_emoji).unwrap_or("👍"),
        Ack::Seen => project.map(project_emoji).unwrap_or("👀"),
        // Còn lại: trạng thái quan trọng hơn nơi chốn.
        Ack::Queued => "👌",
        Ack::Focused => "👀",
        Ack::Running => "⚡",
        Ack::Stopped => "👌",
        Ack::Saved => "✍",
    }
}

pub fn ack_as_emoji(ack: &str) -> Option<&'static str> {
    let t = ack.trim();
    // 🔴 Hà 2026-08-14: *"Mọi tin tôi gửi phản hồi hết bằng emoji đi"* · *"Vì nó
    // đơn giản là xác nhận thôi không cần thông tin"*. Đó chính là phép thử,
    // và nó cắt cả hai chiều: câu nào chỉ nói "đã nhận, đã làm" thì thành một
    // dấu; câu nào MANG THÔNG TIN — kết quả lệnh, màn hình, danh sách phiên,
    // một lời từ chối kèm lý do — thì giữ nguyên chữ, vì rút nó thành mặt cười
    // là vứt đúng phần người ta cần đọc.
    if t.starts_with('✓') {
        // "vào hàng chờ" ≠ "đã gửi": phiên đang bận thì chữ nằm trong hàng của
        // TUI, và đó là một trạng thái khác, đáng một dấu khác.
        if t.contains("hàng chờ") {
            return Some(ack_emoji(None, Ack::Queued));
        }
        // Gửi được rồi thì dấu nói luôn VÀO ĐÂU (xem `project_emoji`); không
        // đọc ra tên dự án thì rơi về 👍 như Hà chốt.
        return Some(ack_emoji(project_in(t), Ack::Sent));
    }
    // Đổi phiên đang theo: người bấm vừa chọn phiên nào thì tự biết, câu trả
    // lời chỉ xác nhận là con trỏ đã nhúc nhích.
    if t.starts_with('👁') {
        return Some(ack_emoji(None, Ack::Focused));
    }
    // Lệnh dài vừa được nhận và bắt đầu chạy — kết quả sẽ tới sau, bằng chữ.
    if t.starts_with("▶ đang chạy") {
        return Some(ack_emoji(None, Ack::Running));
    }
    // Đã bảo dừng.
    if t.starts_with('⏹') {
        return Some(ack_emoji(None, Ack::Stopped));
    }
    None
}

fn reply_in_channel(db: &Db, _cfg: &Config, adapter: &str, cmd: &ChannelCommand, text: &str) {
    let _ = db;
    // Bước phụ của chính hub thì im — xem `telegram::Incoming::quiet`.
    if cmd.quiet {
        logging::info(
            "command_reply_muted",
            json!({ "kind": format!("{:?}", cmd.kind), "why": "bước phụ do hub xếp hàng" }),
        );
        return;
    }
    // Đồng hồ cho ĐƯỜNG TRẢ LỜI. Với phòng chat tfl5, mỗi câu trả lời là một
    // lần ĐĂNG NHẬP LẠI cộng một websocket mới (`tfl5::send` → `login`), nên nó
    // không hề rẻ như "gửi một dòng chữ" nghe có vẻ.
    let ack_started = std::time::Instant::now();
    let _guard = scopeguard_log(adapter, ack_started);
    // Lệnh gõ từ Telegram thì câu trả lời phải quay về Telegram. Bản trước rơi
    // vào nhánh "adapter lạ" và chỉ ghi log — tức người gõ ngồi nhìn màn hình
    // trống, còn câu trả lời nằm trong một tệp trên máy.
    if adapter == crate::telegram::NAME {
        if let Some(i) = crate::telegram::inbox() {
            // 🔴 Hà 2026-08-14: *"Có thể đổi cách phản hồi tin đã gửi bằng 1
            // emoji trực tiếp vào tin nhắn cho gọn"*.
            //
            // Một câu gõ vào phiên tốn HAI dòng trong buồng chat: câu của chủ
            // máy, rồi `✓ đã gửi · [tên]` của hub — mà dòng thứ hai không mang
            // gì ngoài "đã nhận". Thả emoji lên chính tin ấy nói đúng chừng
            // ấy, không chiếm dòng nào.
            //
            // CHỈ cho câu xác nhận trơn (`ack_as_emoji`): một lời từ chối, một
            // câu hỏi, một báo lỗi thì vẫn phải đọc được thành chữ — đổi nó
            // thành một mặt cười là giấu mất thứ người ta cần biết.
            if let (Some(mid), Some(e)) = (cmd.message_id, ack_as_emoji(text)) {
                match i.react(mid, e) {
                    Ok(()) => {
                        logging::info(
                            "telegram_ack_as_reaction",
                            json!({ "emoji": e, "kind": format!("{:?}", cmd.kind) }),
                        );
                        return;
                    }
                    // Thả không được thì NÓI rồi rơi về chữ — im lặng ở đây là
                    // một câu trả lời biến mất.
                    Err(err) => logging::warn(
                        "telegram_reaction_failed",
                        json!({ "err": err, "fallback": "gửi lại bằng chữ" }),
                    ),
                }
            }
            if let Err(e) = i.send_text(text) {
                logging::error("telegram_ack_failed", json!({ "err": e }));
            }
        }
        return;
    }
    // Kênh lạ: KHÔNG im. Trước 2026-08-14 nhánh này còn tfl5; nay chỉ còn
    // Telegram, nên tới được đây nghĩa là có ai đó thêm một kênh mà quên nối
    // đường trả lời.
    logging::info(
        "channel_command_ack",
        json!({ "adapter": adapter, "ack": text }),
    );
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
/// `/set auto_handover.enabled false` cannot turn a bool into the string
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
    // 🔴 Dòng `runs` nay do CHÍNH VÒNG ghi, 2026-08-14 — trước đó nó là của
    // chặng hỏi vòng, và chặng ấy đi cùng tfl5.
    //
    // Không giữ lại thì `hub status` và khối "lỗi gần đây" của `/doctor` đọc một
    // bảng không ai ghi: mãi mãi rỗng, mãi mãi xanh. Một phép đo không bao giờ
    // đỏ được là phép đo mù — nó tệ hơn không có phép đo nào, vì nó vẫn chiếm
    // chỗ trên màn và vẫn được đọc như một lời cam đoan.
    //
    // Mở sổ TRƯỚC khi làm gì: hàng `ok=NULL` giữa chừng chính là "vòng này chết
    // giữa đường" — thứ duy nhất phân biệt được với "chưa chạy vòng nào".
    let run_id = db.start_run("cycle", "cycle").unwrap_or_else(|e| {
        logging::error("cycle_run_row_failed", json!({ "err": e.to_string() }));
        0
    });
    // Vòng này có sạch không — đo bằng số dòng lỗi nó sinh ra, không bằng việc
    // nó có trả `Err` hay không. Xem `logging::ERRORS`: `run_once` gần như không
    // bao giờ trả `Err` (mọi handler đều tự nuốt lỗi thành một câu trả lời cho
    // người gõ), nên một hàng `runs` chỉ ghi `Ok/Err` của chính nó là một hàng
    // luôn luôn `ok` — và khối "lỗi gần đây" đọc nó sẽ rỗng vĩnh viễn.
    let errors_before = logging::error_count();
    execute_telegram_commands(db, cfg);
    // Dọn tin Telegram quá hạn (Hà 2026-08-12: *"tự xóa tin nhắn cũ hơn 1.5
    // ngày"*). Rẻ khi không có gì tới hạn: một phép so trên một danh sách số.
    crate::telegram::prune_sent(cfg, db);
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
    };
    if run_id != 0 {
        // Đóng sổ KHÔNG được nuốt: một vòng chạy xong mà hàng vẫn `ok=NULL` sẽ
        // đọc ra y hệt một vòng chết giữa đường.
        let n_errors = logging::error_count().saturating_sub(errors_before);
        // Chỉ TÊN sự kiện đi vào hàng này, không bao giờ `fields` — hàng này
        // lên màn điện thoại qua `/doctor`, xem `logging::last_error`.
        let err = (n_errors > 0).then(|| {
            let what = logging::last_error_msg().unwrap_or_else(|| "?".into());
            format!("{n_errors} lỗi trong vòng này, gần nhất: {what}")
        });
        if let Err(e) = db.finish_run(
            run_id,
            RunFinish {
                ok: n_errors == 0,
                n_new: 0,
                err,
                skipped: None,
            },
        ) {
            logging::error("cycle_run_row_unclosed", json!({ "err": e.to_string() }));
        }
    }
    logging::info("cycle_done", serde_json::to_value(&summary)?);
    Ok(summary)
}
