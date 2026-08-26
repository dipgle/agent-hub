//! One cycle of the huba: poll the room for orders, run them, push a snapshot.
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
    /// Bao nhiêu phiên ĐÃ quá ngưỡng ngữ cảnh mà vòng này còn giữ lại.
    ///
    /// Không phải số liệu trang trí: `hubad` đọc nó để rút ngắn giấc ngủ —
    /// xem [`auto_handover`] và `AUTO_WATCH_SLICE`. Nó cũng đi thẳng vào dòng
    /// `cycle_done`, nên "huba có đang canh phiên nào không" đọc được từ log
    /// mà không phải suy từ chỗ khác.
    pub watching: usize,
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

/// `/new <id> [chữ]` — từ đầu là ID MỘT PHIÊN thì đây là lượt MỞ LẠI phiên ấy.
///
/// Trả `(id đầy đủ, tài khoản, phần chữ còn lại)`; `None` nếu từ đầu không phải
/// id, hoặc là id mà huba không biết nó thuộc tài khoản nào.
///
/// 🔴 Thay `/tell`, 2026-08-15. Nhận theo HÌNH DẠNG rồi ĐỐI CHIẾU sổ — không
/// chỉ hình dạng: một chuỗi 8 ký tự hex là hình dạng id, nhưng nếu huba không
/// biết phiên ấy thì nó cũng không biết mở bằng tài khoản nào, mà `--resume`
/// chạy nhầm tài khoản là mở nhầm cả kho phiên. Không biết thì TỪ CHỐI, đừng
/// lặng lẽ rơi về tài khoản mặc định.
fn resume_target(rest: &str, db: &Db) -> Option<(String, String, String)> {
    let (head, tail) = split_target(rest)?;
    if crate::sessions::is_shell_id(&head) {
        return None; // cửa sổ trần không có phiên nào để nối tiếp
    }
    let book = db
        .cursor_or_log(WATCH_KEY)
        .and_then(|v| session_name_from_book(&v, &head));
    if let Some((_, account)) = book.filter(|(_, a)| !a.trim().is_empty()) {
        return Some((head, account, tail));
    }
    let stopped = stopped_session(db, &head).filter(|s| !s.account.trim().is_empty())?;
    let (id, account) = (stopped.session_id.clone(), stopped.account.clone());
    Some((id, account, tail))
}

/// Tên tài khoản gõ TRẦN ở đầu đề bài — `(tài khoản, phần còn lại)`.
///
/// 🔴 Hà 2026-08-15: *"Rõ ràng mở phiên mới dwork là acc3 sau xem lại thành acc1
/// là sao"*. Đo nguyên văn trong `logs/huba.log` lúc 02:14:29Z: anh gõ
/// `/new acc3 dwork`, và huba ghi `new_window_opened task:"[] acc3 dwork"` — tức
/// `acc3` KHÔNG được đọc là tài khoản, nó thành một phần ĐỀ BÀI. Phiên mở trên
/// tài khoản mặc định (acc1) và nhận đúng chuỗi chữ `acc3 dwork` để làm.
///
/// 📌 Danh sách phiên **không nói dối**: phiên ấy thật sự nằm ở acc1. Chỗ hỏng
/// sớm hơn một bước, và đó là chỗ đáng nhớ — câu hỏi "sao xem lại thành acc1"
/// dẫn thẳng tới cái loa, trong khi lỗi nằm ở cái miệng.
///
/// Đây KHÔNG phải nới một cửa đoán. `known` là danh sách huba tự đọc từ cấu hình,
/// và phép so là so KHỚP CẢ CHUỖI — nên "token này có phải tên một tài khoản
/// không" là một câu ĐO ĐƯỢC, không phải một câu đoán ý. Cùng lối nghĩ đã ghi ở
/// `config::looks_like_project`: thay một cái tên viết sẵn bằng một câu hỏi trả
/// lời được.
///
/// Hẹp có chủ ý — chỉ TỪ ĐẦU TIÊN: `acc3` nằm giữa câu là chữ trong đề bài, và
/// nuốt nó đi là giao cho phiên một việc khác việc đã gõ (đúng luật đã viết cho
/// cờ lạ ở điều 7).
pub fn lift_bare_account<'a>(task: &'a str, known: &[String]) -> Option<(&'a str, &'a str)> {
    let t = task.trim();
    let (head, rest) = t.split_once(char::is_whitespace).unwrap_or((t, ""));
    // Đề bài rỗng vẫn hợp lệ trên đường mở cửa sổ (`/new acc3` = mở một cửa sổ
    // acc3 rồi gõ sau) — đúng thứ chủ máy làm khi ngồi ở máy.
    (!head.is_empty() && known.iter().any(|a| a == head)).then_some((head, rest.trim_start()))
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

/// Tin đang được GIM trên đỉnh buồng chat: `{"m":<message_id>,"t":"<chữ>"}`.
///
/// Giữ CẢ chữ chứ không riêng id, vì Telegram từ chối `editMessageText` khi nội
/// dung không đổi một ký tự (xem `edit_html`) — và vòng quét chạy mỗi ~10 giây.
/// Nhớ chữ đã gim là cách duy nhất biết "lần này có gì mới" mà không phải hỏi
/// Telegram. Một khoá cho một sự thật: hai khoá phải giữ đồng bộ là hai khoá sẽ
/// lệch nhau.
pub const PIN_FOLLOWING_KEY: &str = "pin:following";

/// Dòng chữ của tin gim — **icon trạng thái** rồi tới tên phiên, không gì khác.
///
/// 🔴 Hà 2026-08-26: *"bỏ icon hiện tại đi thay thành icon trạng thái làm việc,
/// bỏ text sau icon đó đi"* · *"nút xem màn bỏ text đi, bao cả tin cho dễ bấm"*.
///
/// Icon lấy từ [`crate::sessions::state_of`] — cùng bộ ký hiệu với danh sách
/// phiên, một chỗ quyết định chứ không hai bản chép. CHỮ tình trạng thì bỏ, đúng
/// luật đã áp cho danh sách hôm 25/08: từ 19/08 mỗi tình trạng là một HÌNH khác
/// nhau, nên chữ chỉ là bản sao thứ hai.
///
/// Hàm thuần để kiểm được — cái quyết định "có sửa tin gim không" là phép so
/// chuỗi này, nên nó phải đo được mà không cần mạng.
/// Đọc sổ gim: `(message_id, chữ đã gim)`.
///
/// 🔴 ĐỌC ĐƯỢC CẢ SỔ DẠNG CŨ — số trần, không phải JSON. Sổ này ra đời ngày
/// 25/08 giữ mỗi `message_id`, và **trên máy Hà nó ĐANG giữ một con số**: đo
/// lúc 16:21 ngày 26/08, `sqlite3 data/huba.sqlite` trả
/// `pin:following|12071|2026-08-26T09:23:45Z`.
///
/// Bản đầu của hàm này chỉ hiểu JSON, nên trên đúng cái máy nó sinh ra để phục
/// vụ, nó trả `None` ở mọi lượt: `refresh_pin` về sớm (tin gim ĐỨNG NGUYÊN, cái
/// icon trạng thái thành một lời nói dối treo trên đỉnh buồng chat) và
/// `pin_following` thôi gỡ tin cũ (buồng chat mọc thêm một cái gim mỗi lần đổi
/// phiên). Cả hai đều **câm** — không một dòng log, vì `None` ở đây nghĩa là
/// "chưa gim gì", một trạng thái hợp lệ. Đúng hình dạng luật 3 cấm.
///
/// Chữ trả về là `""` cho sổ cũ: nó khác mọi giá trị `pin_line` sinh ra (dòng
/// nào cũng mở đầu bằng một icon), nên lượt quét kế tiếp thấy "có gì mới", sửa
/// tin gim về đúng dạng rồi ghi lại sổ theo JSON. Tự chuyển hệ, không cần một
/// bước di trú riêng để quên chạy.
pub fn pinned_message(db: &Db) -> Option<(i64, String)> {
    let v = db.cursor_or_log(PIN_FOLLOWING_KEY)?;
    if let Ok(m) = v.trim().parse::<i64>() {
        return Some((m, String::new()));
    }
    let j: serde_json::Value = serde_json::from_str(&v).ok()?;
    Some((
        j.get("m").and_then(serde_json::Value::as_i64)?,
        j.get("t")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
    ))
}

/// Dựng HTML của tin gim: cả dòng nằm trong thẻ, chạm chỗ nào cũng mở màn.
fn pin_html(sid: &str, line: &str) -> Option<String> {
    let href = crate::telegram::deep_link(&format!("shot_{sid}"))?;
    Some(format!(
        "<a href=\"{}\">{}</a>",
        crate::telegram::html_escape(&href),
        crate::telegram::html_escape(line)
    ))
}

/// MỘT CỬA để đặt tin gim: sửa tin đang gim, chỉ gửi tin mới khi chưa có cái nào.
///
/// 🔴 Vì sao phải là "sửa", không phải "gửi rồi gim" — Hà 2026-08-26: *"hiện tại
/// khi chọn vào phiên đã có sẵn pin message rồi"*. Đường cũ gửi một tin chào MỚI
/// mỗi lần đổi phiên rồi gim nó, tức mỗi cú chạm để lại một dòng vĩnh viễn trong
/// buồng chat nói đúng cái điều mà dòng đang gim trên đỉnh đã nói. Tin gim là
/// MỘT chỗ đứng, không phải một cuốn sổ chép tay.
///
/// Trả `Some(message_id)` khi đỉnh buồng chat nay mang đúng `line`; `None` khi
/// không đặt được — và chỗ gọi phải đọc `None` ấy để rơi về đường cũ, chứ không
/// được coi như đã xong (nếu không thì một lượt gim hụt = chủ máy mất luôn câu
/// trả lời cho cú chạm của mình).
///
/// 🔴 `tao_neu_thieu` — **vòng nền KHÔNG được đẻ tin**. Cú chạm của chủ máy thì
/// được: anh vừa hỏi, một tin mới là câu trả lời. Nhưng `refresh_pin` chạy mỗi
/// ~10 giây và không có ai hỏi cả; nếu nó được phép gửi mới thì một lượt
/// `editMessageText` hỏng DAI DẲNG (bot bị siết nhịp, mất quyền sửa) biến thành
/// một tin mới mỗi mười giây — đúng luật 11 của dự án: một cái điện thoại rung
/// mãi là một cái điện thoại bị tắt tiếng, và nó mang theo cả những tin đáng đọc.
fn pin_apply(
    db: &Db,
    tg: &crate::telegram::Inbox,
    sid: &str,
    line: &str,
    tao_neu_thieu: bool,
) -> Option<i64> {
    let html = pin_html(sid, line)?;
    if let Some((mid, cu)) = pinned_message(db) {
        // Chữ y hệt ⟹ không gọi mạng. Telegram từ chối `editMessageText` khi nội
        // dung không đổi một ký tự, nên đây vừa là tiết kiệm vừa là tránh đẻ ra
        // một dòng lỗi giả.
        if cu == line {
            return Some(mid);
        }
        match tg.edit_html(mid, &html, &[]) {
            Ok(_) => {
                pin_book(db, mid, line);
                return Some(mid);
            }
            // Tin gim có thể đã bị xoá (`delete_after_hours`) hoặc chủ máy tự gỡ
            // — GHI rồi đi tiếp xuống đường gửi mới, đừng nuốt và cũng đừng bỏ
            // cuộc: mất tin gim thì dựng lại một cái, không để trống đỉnh.
            Err(e) => logging::info(
                "pin_edit_failed",
                json!({ "message_id": mid, "err": e, "tao_neu_thieu": tao_neu_thieu,
                        "effect": if tao_neu_thieu { "gửi một tin gim mới thay chỗ" }
                                  else { "giữ nguyên tin gim cũ — vòng nền không đẻ tin" } }),
            ),
        }
    }
    if !tao_neu_thieu {
        return None;
    }
    match tg.send_html_report(&html, &[]) {
        Ok(sent) => {
            pin_following(db, tg, sent.message_id, line);
            Some(sent.message_id)
        }
        Err(e) => {
            logging::error(
                "pin_send_failed",
                json!({ "err": e, "effect": "đỉnh buồng chat không có tin gim nào cho phiên này" }),
            );
            None
        }
    }
}

/// Ghi sổ gim. Một chỗ, vì hai bản chép là hai bản sẽ lệch.
fn pin_book(db: &Db, message_id: i64, text: &str) {
    let so = json!({ "m": message_id, "t": text }).to_string();
    if let Err(e) = db.set_cursor(PIN_FOLLOWING_KEY, &so) {
        logging::error(
            "pin_book_failed",
            json!({ "message_id": message_id, "err": e.to_string(),
                    "effect": "đã đặt tin gim nhưng sổ giữ chữ CŨ — vòng sau sẽ sửa lại y hệt" }),
        );
    }
}

/// Tin "⏳ đang quét màn" đang chờ được xoá — giữ đúng `message_id` của nó.
///
/// 🔴 Hà 2026-08-26, ảnh buồng chat: *"chỗ tin phản hồi này chưa hợp lý khi kích
/// chọn vào phiên, chỉ cần hiện thông báo chờ quét màn, sau khi nhận được tin
/// thì xóa nó luôn đi"*.
///
/// Cú chạm vào một phiên xếp luôn `/shot` (xem đường nhanh trong
/// `CommandKind::Session`), và cú chụp ấy tốn vài giây. Khoảng lặng ấy phải có
/// người nói — nhưng nói xong thì đi, vì nó là TRẠNG THÁI TẠM, không phải một
/// sự kiện đáng nằm lại trong sổ hội thoại.
pub const SCAN_NOTICE_KEY: &str = "scan:notice";

/// Đặt dòng "đang quét màn", nhớ id để còn xoá.
fn scan_notice(db: &Db, tg: &crate::telegram::Inbox, ten: &str) {
    // Tin cũ chưa kịp xoá thì dọn trước: hai dòng "đang quét" chồng nhau nói
    // rằng huba đang quét hai lần, mà nó chỉ quét một.
    clear_scan_notice(db, tg);
    match tg.send_html_report(
        &format!("⏳ đang quét màn {}…", crate::telegram::html_escape(ten)),
        &[],
    ) {
        Ok(sent) => {
            if let Err(e) = db.set_cursor(SCAN_NOTICE_KEY, &sent.message_id.to_string()) {
                logging::error(
                    "scan_notice_book_failed",
                    json!({ "message_id": sent.message_id, "err": e.to_string(),
                            "effect": "dòng 'đang quét màn' sẽ NẰM LẠI trong buồng chat" }),
                );
            }
        }
        // Không gửi được thì thôi — nhưng phải nói, vì lúc ấy chủ máy chạm vào
        // phiên và KHÔNG thấy gì phản hồi cho tới khi màn tới.
        Err(e) => logging::warn("scan_notice_failed", json!({ "err": e })),
    }
}

/// Xoá dòng "đang quét màn" nếu còn — gọi ngay khi màn đã tới.
fn clear_scan_notice(db: &Db, tg: &crate::telegram::Inbox) {
    let Some(mid) = db
        .cursor_or_log(SCAN_NOTICE_KEY)
        .and_then(|v| v.trim().parse::<i64>().ok())
    else {
        return;
    };
    // Xoá được hay không thì SỔ CŨNG PHẢI SẠCH: giữ lại một id đã chết nghĩa là
    // mọi lượt sau đều đi xoá đúng cái tin ấy thêm một lần nữa, mỗi lần một dòng
    // lỗi, mãi mãi.
    if let Err(e) = tg.delete_message(mid) {
        logging::info(
            "scan_notice_delete_failed",
            json!({ "message_id": mid, "err": e,
                    "effect": "dòng 'đang quét màn' nằm lại; chủ máy tự xoá được" }),
        );
    }
    if let Err(e) = db.set_cursor(SCAN_NOTICE_KEY, "") {
        logging::error(
            "scan_notice_book_failed",
            json!({ "err": e.to_string(), "effect": "sổ còn giữ một id đã xoá" }),
        );
    }
}

/// Giữ tin gim ĐÚNG VỚI SỰ THẬT — chạy mỗi vòng quét.
///
/// 🔴 Không có hàm này thì cái icon trạng thái là một lời nói dối. Tin gim được
/// viết MỘT LẦN, lúc chủ máy chuyển phiên; mà trạng thái phiên đổi liên tục —
/// `⚡` mười giây sau đã là `💤` hoặc `❓`. Cái gim thì vẫn đứng nguyên trên đỉnh
/// buồng chat, trông như đang sống. Đúng thứ `CLAUDE.md` gọi là phép đo mù:
/// nhìn thì có tin, mà tin ấy không đổi được theo sự thật.
///
/// Ba cửa, theo thứ tự rẻ trước:
/// ① tắt cờ, hoặc chưa theo phiên nào, hoặc chưa gim gì ⟹ về ngay;
/// ② chữ KHÔNG đổi ⟹ về ngay. Vòng quét chạy mỗi ~10 giây, mà Telegram từ chối
///   `editMessageText` khi nội dung y hệt — nện nó mỗi vòng là vừa tốn vừa đẻ
///   một dòng lỗi giả mỗi lần;
/// ③ phiên BIẾN MẤT khỏi danh sách ⟹ KHÔNG sửa gì. Nó có thể chỉ là một lượt
///   `claude agents` mù (xem luật 11b: danh sách rỗng không có nghĩa là phiên
///   chết), và viết "đã tắt" lên đỉnh buồng chat vì một phép hỏi trượt là đúng
///   con bug đã trả giá ngày 12/08 với ba tin báo tử sai trong tám giây.
pub fn refresh_pin(db: &Db, cfg: &Config, live: &crate::sessions::SessionsSnapshot) {
    if !cfg.pin_following {
        return;
    }
    let Some((_, cu)) = pinned_message(db) else {
        return;
    };
    let sid = db.cursor_or_log(FOCUS_SESSION_KEY).unwrap_or_default();
    if sid.trim().is_empty() {
        return;
    }
    let Some(s) = live.sessions.iter().find(|s| s.session_id == sid) else {
        return;
    };
    let moi = pin_line(s);
    if moi == cu {
        return;
    }
    let Some(tg) = crate::telegram::inbox() else {
        return;
    };
    // ĐÚNG MỘT CỬA với đường chạm-vào-phiên: `pin_apply` sửa tin đang gim, và chỉ
    // dựng tin mới khi cái cũ đã biến mất. Trước 26/08 chỗ này có bản chép riêng
    // của phép sửa + ghi sổ; hai bản chép của cùng một sự thật là hai bản sẽ lệch.
    // `false` = vòng nền chỉ SỬA, không đẻ tin mới. Xem `pin_apply`.
    pin_apply(db, tg, &sid, &moi, false);
}

pub fn pin_line(s: &crate::sessions::LiveSession) -> String {
    let (icon, _) = crate::sessions::state_of(s);
    pin_line_from(icon, &crate::sessions::shown(s), &s.account)
}

/// Hình dạng CHUNG của dòng gim: `<icon> <tên>` và `(tài khoản)` nếu biết.
///
/// 🔴 MỘT HÀM, không phải hai chỗ `format!` giống nhau. `refresh_pin` so CHUỖI để
/// biết "lần này có gì mới", nên hai bản chép lệch nhau **một khoảng trắng** là
/// tin gim bị sửa lại ở MỌI vòng quét — mười giây một lần, mãi mãi, và không có
/// gì kêu lên cả. Đường nhanh (chạm vào phiên) không có `LiveSession` nên nó
/// không gọi được `pin_line`; nó gọi thẳng hàm này với icon `👁`, và nhờ vậy hai
/// dòng chỉ có thể khác nhau đúng ở cái icon — đúng phần `refresh_pin` phải sửa.
///
/// Không biết tài khoản thì **đừng mở ngoặc**: một cặp `()` rỗng nói rằng huba
/// biết một điều gì đó rồi bỏ trống (cùng luật `follow_ack_head`).
/// 🔴 VÀ PHẢI CÓ 📷 Ở CUỐI — Hà 2026-08-26: *"pin msg: sao lại mất link xem màn
/// rồi"*.
///
/// Hỏi thẳng Telegram (`getChat`) thì liên kết KHÔNG mất: `text_link offset=0
/// len=20`, tức cả dòng vẫn mở được màn. Thứ mất là **cái nút `📷 Xem màn`** —
/// `refresh_pin` sửa tin bằng `edit_html(mid, html, &[])`, mà `editMessageText`
/// không kèm `reply_markup` thì Telegram XOÁ bàn phím của tin ấy. Cùng lúc 📷 đã
/// nhường chỗ cho icon trạng thái, nên dòng gim còn đúng một màu chữ xanh và
/// không còn gì nói nó dẫn đi đâu.
///
/// Nên 📷 quay lại, nhưng ở CUỐI: đầu dòng là chỗ của icon trạng thái (thứ đọc
/// trong một liếc), còn 📷 là nhãn của HÀNH ĐỘNG. Không dựng lại cái nút — Hà đã
/// bỏ nó ngày 26/08 (*"nút xem màn bỏ text đi để icon và bao hết text của tin
/// gim"*), và một cái nút đứng rời ở đáy thì đích chạm chỉ to bằng chính nó.
pub fn pin_line_from(icon: &str, ten: &str, tai_khoan: &str) -> String {
    if tai_khoan.trim().is_empty() {
        format!("{icon} {ten} 📷")
    } else {
        format!("{icon} {ten} ({tai_khoan}) 📷")
    }
}

/// Tráo cái gim: gỡ tin cũ, gim tin mới, ghi sổ. **Một cửa duy nhất.**
///
/// 🔴 Hà 2026-08-25: *"bật gim tin nhắn thông tin phiên đang đứng trước đi"*.
///
/// Buồng chat cuộn rất nhanh trên điện thoại, nên đỉnh buồng là chỗ duy nhất
/// luôn thấy được câu trả lời cho *"tôi đang đứng ở phiên nào"*.
///
/// Ba điều gói vào một chỗ, để chỗ gọi không phải nhớ cái nào trước cái nào:
/// ① **gỡ cũ TRƯỚC khi gim mới** — không thì đỉnh buồng mọc một chồng gim và
///   cái mới nhất lại không phải cái trên cùng;
/// ② **ghi sổ id mới** ngay cả khi gỡ cái cũ hỏng — tin cũ có thể đã bị xoá,
///   và một lần gỡ hụt không được phép làm mất dấu cái vừa gim;
/// ③ **hỏng thì GHI, đừng nuốt** (luật 3). Bot bị gỡ quyền gim là chuyện có
///   thật; im lặng thì chủ máy chỉ thấy "gim không lên" mà không biết vì sao.
pub fn pin_following(db: &Db, tg: &crate::telegram::Inbox, message_id: i64, text: &str) {
    if let Some(cu) = pinned_message(db).map(|(m, _)| m) {
        if cu != message_id {
            if let Err(e) = tg.unpin(cu) {
                // Không phải lỗi nặng: tin cũ có thể đã bị `delete_after_hours`
                // dọn đi. Ghi lại rồi đi tiếp — cái gim mới quan trọng hơn.
                logging::info("pin_unpin_failed", json!({ "message_id": cu, "err": e }));
            }
        }
    }
    match tg.pin(message_id) {
        Ok(()) => pin_book(db, message_id, text),
        Err(e) => logging::error(
            "pin_failed",
            json!({ "message_id": message_id, "err": e,
                    "effect": "câu 'đang theo phiên nào' không lên đỉnh buồng chat" }),
        ),
    }
}

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
/// Hậu quả không dừng ở một câu trả lời lạc: huba đã **gõ thật** vào cửa sổ của
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
    // Id NGẮN (8 ký tự hex) cũng là một cái tên phiên thật: huba in nó khắp nơi
    // (`f7612183`), và một lệnh tự tô sáng thì BẮT BUỘC phải dùng nó — tên lệnh
    // chỉ được 32 ký tự, một uuid đầy đủ đã 36. Hẹp có chủ ý: đúng 8, toàn hex,
    // và phải có chữ đi sau — `/type deadbeef` trống thì vẫn là chữ gõ vào
    // phiên, không phải một lệnh nhắm vào phiên `deadbeef`.
    let short_id =
        head.len() == 8 && head.chars().all(|c| c.is_ascii_hexdigit()) && !rest.is_empty();
    // 🔴 CỬA SỔ TRẦN cũng là một mục tiêu, 2026-08-15.
    //
    // Hà bấm cửa sổ `ttys002` rồi gõ `ls`, và cái shell nhận được nguyên
    // `win-ttys002 Ls` — `zsh: command not found: win-ttys002`. Đo trong log:
    // `telegram_text_as_typing len=2` mà `keys_typed **len=14**`, tức id bị dán
    // vào ĐẦU chữ. Vì đường gõ dựng `/type <id> <chữ>` rồi hàm này tách lại —
    // và nó chỉ biết hình dạng uuid, nên `win-ttys002` không phải id ⟹ cả chuỗi
    // thành chữ để gõ.
    //
    // Đúng con bệnh của cả ngày hôm nay: `is_shell_id` vừa dựng xong ở
    // `sessions`, mà chỗ này thì chưa ai bảo. Nên hỏi CHUNG một chỗ thay vì so
    // chuỗi lần nữa.
    //
    // Đòi `rest` không rỗng, cùng luật với id ngắn: `/type win-ttys002` trơn
    // vẫn là chữ gõ vào phiên đang theo, không phải một lệnh nhắm vào cửa sổ ấy.
    //
    // Đòi ĐÚNG hình dạng một tên tty (`ttys000`), không phải "chữ gì cũng được
    // sau `win-`". Bản đầu của tôi nới tới mức ấy và bài kiểm bắt ngay: câu
    // *"win-win thế nào rồi"* bị nuốt mất từ đầu — đúng cái bẫy chú thích trên
    // đã ghi (*"bắt nhầm thì chữ đầu của câu người ta gõ bị nuốt mất"*).
    let shell_id = crate::sessions::is_shell_id(head)
        && {
            let tail = &head[crate::sessions::SHELL_ID_PREFIX.len()..];
            tail.strip_prefix("tty")
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric()))
        }
        && !rest.is_empty();
    (full_uuid || short_id || shell_id).then(|| (head.to_string(), rest.to_string()))
}

/// Hai chuỗi này có chỉ vào CÙNG một phiên không?
///
/// `want` được phép là **8 ký tự đầu** của id — dạng huba in ra khắp nơi, và là
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
/// 🔴 Hà 2026-08-12, đọc đúng tin `⏹ huba-67 (033059d8) đã tắt — cửa sổ ấy nay
/// đang chạy phiên huba-ec.` kèm một cái nút: *"tại sao 1 phiên đã tắt mà vẫn
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
    // 🔴 ĐANG THEO PHIÊN ẤY THÌ ĐỔI NÚT, ĐỪNG BỎ NÚT — Hà 2026-08-19, ảnh một
    // tin `🟡 🟩 [tfl5] dừng, đang chờ bạn — sau 4 phút chạy`: *"Nhận được thông
    // báo này nhưng không có nút vào phiên"*.
    //
    // Bản trước trả `None` ở đây, và cái lý do nghe rất hợp lý: "vào phiên" là
    // một cú bấm KHÔNG làm gì khi anh đã ở trong phiên ấy rồi. Đúng về cái nút,
    // sai về cái TIN: tin ấy vừa nói *"đang chờ bạn"* — tức nó vừa dựng ra đúng
    // một việc để làm, rồi không đưa đường nào để làm việc đó. Ngồi ở máy thì
    // anh liếc sang cửa sổ; từ điện thoại, thứ tương đương là XEM MÀN.
    //
    // Nên luật của rule 14 giữ nguyên (*"một cái nút phải dẫn tới chỗ có thật"*)
    // và đọc kỹ hơn một nhịp: đích không có thật thì đổi đích, chứ đừng để tin
    // trần. `shot:` là route đã có, cùng đường với nút `📷 Xem màn` ở chỗ khác —
    // không đẻ lối riêng.
    if target == focused {
        return Some(("📷 Xem màn".to_string(), format!("shot:{target}")));
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
/// nó đi vào đúng một dòng: dựng lại ảnh chụp phiên **chỉ để lấy `s.name` và
/// `s.account`** cho câu chào. (Bản vá hôm ấy là một cái đệm 20 giây; nó đã bị
/// gỡ 2026-08-15 cùng lúc ảnh chụp thôi spawn `claude` — xem bia mộ
/// `snapshot_cached` trong `sessions.rs`.)
///
/// Mà hai thứ ấy huba đã nhớ sẵn: `Mark::n` (tên) và `Mark::a` (tài khoản), ghi
/// mỗi vòng chính vì lúc phiên biến mất thì không còn chỗ nào hỏi nữa. Đọc sổ
/// là một lượt đọc SQLite.
///
/// Cái giá phải nói thẳng: sổ cũ hơn ảnh chụp đúng **một vòng**. Với câu "đang
/// theo phiên nào" thì đó là cái giá đúng — một cái tên trễ một vòng vẫn là cái
/// tên ấy, còn 48 giây im lặng thì người ta bấm lần hai.
/// Câu chào của đường CHẬM — nhánh chạy khi sổ chưa biết phiên này.
///
/// 🔴 Tách thành hàm riêng 2026-08-15, và lý do tách chính là con bug: Hà bấm
/// đúng cái nút `🟪 [huba]` và nhận về *"👁 Đang theo phiên projects-67 (acc1)"*
/// — *"rõ ràng vào huba mà chỉ báo thế này"*. Câu chào ấy có HAI đường; đường
/// nhanh (`session_name_from_book`) trả nhãn đúng từ 08-12, còn đường này in
/// `s.name` THÔ. Cả máy mở phiên ở gốc workspace nên `claude` đặt tên phiên nào
/// cũng `projects-xx` — đúng cái tên phân biệt được ÍT NHẤT trong mọi cái tên
/// có ở đây, và nó là thứ duy nhất câu này in ra.
///
/// Bản chép tay THỨ TƯ của cùng một luật (ba bản trước ở `screen_report`, vá
/// 08-13). Nằm trong một `format!` giữa một `match` sáu tầng thì không cửa nào
/// bắt được nó — nên nay nó là một hàm, và có một bài kiểm ĐỎ ĐƯỢC đứng canh.
///
/// 📌 Và đây là nhánh hay chạy nhất ngay sau một lượt `hubad` khởi động lại (sổ
/// còn rỗng), tức lỗi hiện ra đúng lúc chủ máy hay bấm nút nhất.
pub fn follow_ack_head(s: &crate::sessions::LiveSession, how: &str) -> String {
    // Cửa sổ trần không có tài khoản — in `()` rỗng là một cặp ngoặc nói rằng
    // huba biết một điều gì đó rồi bỏ trống. Không biết thì đừng mở ngoặc.
    let who = if s.account.trim().is_empty() {
        String::new()
    } else {
        format!(" ({})", s.account)
    };
    format!("👁 {}{}{}", crate::sessions::shown(s), who, how)
}

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

/// Cất sổ theo dõi. `false` = KHÔNG cất được, và chỗ gọi phải im luôn lượt ấy.
fn save_watch_book(db: &Db, next: &BTreeMap<String, crate::watch::Mark>) -> bool {
    match serde_json::to_string(next) {
        Ok(v) => match db.set_cursor(WATCH_KEY, &v) {
            Ok(()) => true,
            Err(e) => {
                logging::error("watch_state_save_failed", json!({ "err": e.to_string() }));
                false
            }
        },
        Err(e) => {
            logging::error("watch_state_encode_failed", json!({ "err": e.to_string() }));
            false
        }
    }
}

/// Sổ cũ hơn mốc này ⟹ huba đã KHÔNG nhìn trong lúc ấy, nên không được nói "vừa".
///
/// Đặt bằng 10 phút: vòng chạy ~2 phút, nên một sổ quá năm vòng nghĩa là huba
/// vắng mặt chứ không phải thế giới đứng im.
const WATCH_BOOK_STALE_SEC: i64 = 600;

/// Sổ lượt trước có ĐỦ TƯƠI để kết luận "vừa xong / vừa tắt" không.
///
/// Tách thành hàm thuần vì đây là một QUYẾT ĐỊNH, không phải một phép đo: nó
/// nói *huba có tư cách để tuyên bố gì không*. Ba ca, ba lý do khác nhau:
/// sổ rỗng thì cứ đi đường thường (`watch::changes` vốn đã im ở lượt đầu);
/// đọc được mốc và còn mới thì nói; **không đọc được mốc thì IM** — không biết
/// mình đã nhìn hay chưa là chưa đủ tư cách để báo một cái chết.
pub fn watch_book_usable(prev_len: usize, age_sec: Option<i64>) -> bool {
    prev_len == 0 || age_sec.is_some_and(|a| a <= WATCH_BOOK_STALE_SEC)
}

pub fn announce_changes(db: &Db, cfg: &Config, snap: &crate::sessions::SessionsSnapshot) {
    let live = &snap.sessions;
    let prev: BTreeMap<String, crate::watch::Mark> = db
        .cursor_or_log(WATCH_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();

    // 🔴 SỔ QUÁ CŨ = CHƯA TỪNG NHÌN, không phải "mọi thứ vừa đổi".
    //
    // Luật 11 đã có sẵn một nửa của câu này: *nói KHÔNG gì cả ở lượt đầu sau khi
    // khởi động lại* — nhưng nó đo bằng "sổ rỗng", mà sổ có thể **đầy và ôi**.
    // Đúng ca đang có trên máy lúc viết dòng này: `watch:sessions` ghi lần cuối
    // 14/08 13:11:24 (đúng phút cái loa mất chỗ gọi), giữ hai phiên chết từ hôm
    // kia. Lượt so đầu tiên sau bản vá sẽ thấy hai phiên ấy vắng mặt và bắn hai
    // tin *"⏹ đã tắt"* về hai cái chết đã cũ hơn một ngày.
    //
    // Cùng một họ với ba tin sai ngày 12/08 (`blind`): huba chỉ được kết luận
    // "vừa tắt" khi nó THẬT SỰ đang nhìn ở lượt trước. Nên: nạp lại sổ, im
    // lặng, và NÓI RA trong log rằng lượt này cố ý không có tin.
    let age = db
        .cursor_written_at(WATCH_KEY)
        .map(|t| chrono::Utc::now().timestamp() - t);
    if !watch_book_usable(prev.len(), age) {
        let (_, next) =
            crate::watch::changes(&prev, live, chrono::Utc::now().timestamp(), &snap.blind);
        save_watch_book(db, &next);
        logging::warn(
            "watch_book_stale_muted",
            json!({ "age_sec": age, "was": prev.len(), "now": live.len(),
                    "why": "sổ cũ hơn 10 phút ⟹ huba đã không nhìn trong khoảng ấy; nạp lại và KHÔNG nói gì (luật 11)" }),
        );
        return;
    }
    // Tài khoản nào KHÔNG liệt kê được phiên ở lượt này đi thẳng vào phép so:
    // vắng mặt trong một danh sách hỏng không phải là một cái chết. Trước
    // 2026-08-12 hàm này chỉ nhận `sessions`, nên `notes` — chỗ duy nhất ghi
    // chuyện tài khoản hỏng — không tới được đây, và ba phiên còn sống bị báo
    // tắt trong 8 giây.
    let (changes, next) =
        crate::watch::changes(&prev, live, chrono::Utc::now().timestamp(), &snap.blind);
    // Phiên đang theo — để biết tin nào cần kèm nút "vào phiên".
    let focused = db.cursor_or_log(FOCUS_SESSION_KEY).unwrap_or_default();
    // Phiên do CHÍNH huba đóng sổ thì cái chết của nó KHÔNG phải tin.
    //
    // 🔴 Hà 2026-08-13, đọc đúng tin ấy: *"sao lại có thông báo này: ⏹
    // projects-fb · AI/huba (76534706) đã tắt hẳn — nó đang chạy dở, nên xem
    // lại"*. Log cùng lúc: `auto_handover_firing` 00:09:15 →
    // `handover_window_opened` 00:09:49 → tin báo tử 00:10:07. Tức huba vừa cố ý
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
    // …và phiên huba ĐANG ĐÓNG theo lệnh `/close` cũng vậy — cùng một lý do,
    // một cuốn sổ khác.
    //
    // 🔴 Hà 2026-08-13, đếm tin sau đúng MỘT cú `/close`: *"Đóng 1 phiên mà lắm
    // thông báo thế"*. Trên ảnh có `⏳ Đã gõ /exit … chờ CLI chạy nốt`, rồi
    // `⚫ [mailler] đã tắt (thoát CLI, cửa sổ terminal còn mở)`, rồi `⏹ Đã đóng
    // hẳn [mailler] … (chờ 24s)`. Tin giữa là cái loa nhìn thấy phiên biến mất
    // và báo động — về đúng việc huba vừa cố ý làm, ba mươi giây trước. Nó còn
    // mâu thuẫn với tin sau nó ("cửa sổ còn mở" rồi "cửa sổ đã đóng"), nên
    // người đọc phải tự ghép hai câu mới ra một sự thật.
    let closing: Vec<String> = db
        .cursor_or_log(CLOSING_KEY)
        .and_then(|v| serde_json::from_str::<BTreeMap<String, Closing>>(&v).ok())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();

    // Ghi sổ TRƯỚC khi nói: nói xong mới ghi mà sập giữa chừng thì lượt sau nói
    // lại y hệt. Thà lỡ một lời báo còn hơn một cái loa lặp.
    if !save_watch_book(db, &next) {
        return;
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
        // chính huba tạo, và log kèm số tệp — xoá tệp của người khác mà im lặng
        // là thứ không ai tha thứ lần thứ hai.
        if matches!(c, crate::watch::Change::Ended { .. }) {
            clean_inbox(cfg, &id, row.map(|r| r.folder.as_str()).unwrap_or(""));
        }
        // huba vừa tự đóng sổ phiên này ⟹ cái chết của nó là KẾ HOẠCH, không
        // phải tin. Xem `handed_over` ở đầu hàm.
        if matches!(c, crate::watch::Change::Ended { .. })
            && (handed_over.iter().any(|d| d == &id) || closing.iter().any(|d| d == &id))
        {
            logging::info(
                "session_end_muted",
                json!({ "session": id,
                        "why": "huba vừa tự đóng sổ phiên này — cái chết của nó là kế hoạch" }),
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
            // huba từng nói "cửa sổ ấy nay đang chạy phiên khác" về hai phiên
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
        // dừng lại chờ anh. Thêm một khe mù nữa: huba chỉ NHÌN mỗi ~139 giây
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
        // Lựa chọn lấy từ NHẬT KÝ trước (đầy đủ, có cả với phiên huba không đọc
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
            // 🔴 HAI, không phải ba. Hà 2026-08-14, ảnh chụp một tin mang ba
            // nút lệnh: *"sao lắm nút lệnh thế"*. Một báo cáo dài nhắc tới
            // nhiều lệnh, nhưng thứ chủ máy cần bấm ngay thì gần như luôn là
            // câu chốt — những cái còn lại chỉ là chữ trong lời kể. Ba nút gần
            // giống nhau không cho thêm lựa chọn nào, chúng bắt người đọc dừng
            // lại đoán xem cái nào mới đúng.
            //
            // 🔴 2026-08-15 — MỘT nguồn cho cả hai đường. Trước đây đường này
            // đọc `scan` (chữ báo cáo) còn `/shot` đọc màn; hai nguồn, hai bộ
            // luật, và chúng lệch nhau đúng theo kiểu tệp này đã đặt tên nhiều
            // lần. Nay cả hai hỏi `sessions::commands_of`, tức cùng nhật ký,
            // cùng luật — và đường này được thêm nhánh **lệnh bị cổng quyền từ
            // chối**, thứ `scan` không bao giờ nhìn thấy: phiên dừng lượt vì
            // không được phép chạy, và đó đúng là lúc việc rơi sang chủ máy.
            //
            // Không thấy sổ ⟹ rơi về `scan`: chữ ấy CŨNG từ nhật ký (`long`),
            // nên đây không phải rơi về màn.
            let from_log = crate::sessions::commands_of(cfg, &id, 2);
            if from_log.is_empty() {
                // Rơi về `scan` thì KHÔNG có thư mục nào đo được — để trống, và
                // `root_for_command` sẽ rơi tiếp về gốc dự án. Bịa một thư mục ở
                // đây là dựng lại đúng con bug vừa vá.
                crate::keys::commands_in_report(scan, 2)
                    .into_iter()
                    .map(|line| crate::sessions::Cmd {
                        line,
                        cwd: String::new(),
                    })
                    .collect()
            } else {
                from_log
            }
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
                // 🔴 ☑ NGAY TẠI DÒNG LỰA CHỌN — Hà 2026-08-17, ảnh một tin tự
                // phát có bốn lựa chọn và bốn cái nút `☐ 1 Khô` `☐ 2 Bí d`… ở
                // đáy: *"Sao không chèn icon thẳng vào các lựa chọn mà chèn phía
                // dưới"*.
                //
                // Đúng, và đây là đường CUỐI CÙNG còn dựng khối nút ở đáy cho
                // thứ đã có chỗ đứng trong chữ. `/shot` chèn ☑ vào chính dòng
                // của lựa chọn từ 16/08; tin tự phát thì không, nên cùng một
                // hộp hỏi cho hai hình dạng khác nhau tuỳ nó tới bằng đường nào
                // — và bản ở đáy còn tệ hơn: nhãn bị Telegram cắt ở 52 ký tự
                // nên `Không xoá gì` đọc thành `1 Khô`.
                //
                // Nút vẫn dựng như cũ rồi truyền vào cùng cửa: `session_layout`
                // tự bỏ những nút `key:` đã thành ☑ trong chữ, và giữ lại
                // `✅ Gửi lựa chọn` cùng nút của các câu sau — thứ KHÔNG có chỗ
                // neo nào trong chữ để mà chèn.
                let btns = crate::telegram::choice_buttons(&id, opts, enter.is_some(), multi, rest);
                let data = SessionData {
                    sid: id.clone(),
                    cmds: crate::sessions::lines_of(&cmds),
                    // Bảng nhiều câu ⟹ mã `"1.<n>"` (câu 1) để ☑ đi bằng
                    // `pick_`; hộp một câu ⟹ mã `"<n>"`, đi bằng `k_`.
                    choices: opts
                        .iter()
                        .enumerate()
                        .map(|(i, l)| {
                            let n = i + 1;
                            let code = if rest.is_empty() {
                                n.to_string()
                            } else {
                                format!("1.{n}")
                            };
                            (code, l.clone())
                        })
                        .collect(),
                    ..Default::default()
                };
                say_session_data(tg, &text, &btns, "session_change_telegram_failed", &data);
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
                say_with_command_icons(
                    tg,
                    &text,
                    &crate::sessions::lines_of(&cmds),
                    &b,
                    "session_change_telegram_failed",
                );
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

/// Khối dán NGƯỢC vào phiên sau khi huba chạy hộ một lệnh — ngắn nhất có thể.
///
/// 🔴 Hà 2026-08-16, ảnh chụp đúng khối này: *"tại sao lại có một mớ text không
/// cần thiết này"* · *"quá tốn context"*. Bản cũ mở đầu bằng một câu **90 ký
/// tự** kể ruột huba — *"huba đã chạy hộ lệnh này trên máy — cwd
/// /Users/hanguyen/projects/dwork, KHÔNG có tty"* — và khối này **nằm lại trong
/// nhật ký phiên vĩnh viễn**, tức nó ngốn ngữ cảnh của chính phiên ấy ở mọi
/// lượt về sau. Phiên cần đúng hai điều: lệnh nào, và ra gì.
///
/// Hai thứ KHÔNG cắt, vì cả hai đều load-bearing:
/// * `[huba chạy hộ]` — thiếu nó thì phiên đọc khối này như thể CHÍNH NÓ vừa
///   chạy lệnh, rồi kể lại như việc mình đã làm.
/// * `$ <lệnh>` — một báo cáo thường nhắc vài lệnh; không có dòng này thì kết
///   quả không biết thuộc về lệnh nào.
///
/// Còn "không qua tty" chỉ nói khi lệnh HỎNG: lúc ấy nó là một lý do (`sudo`,
/// `ssh -t`, `passwd` chết ở dòng hỏi mật khẩu), lúc thành công thì nó là một
/// mẩu tin không ai dùng.
pub fn runin_block(line: &str, report: &str, failed: bool) -> String {
    if failed {
        format!("[huba chạy hộ · không qua tty]\n$ {line}\n{report}")
    } else {
        format!("[huba chạy hộ]\n$ {line}\n{report}")
    }
}

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
/// Khác một chỗ: `STOPPED_KEY` chỉ nhớ phiên do CHÍNH huba dừng, còn sổ này nhớ
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

/// The session huba stopped most recently, kept whole so `/tell` can resume it.
///
/// `/stop` answers "hội thoại vẫn còn — nói tiếp bằng /tell", and that promise used to
/// break on the very next command: `claude agents` drops a stopped background
/// session from its list within seconds, and `/tell` gated on that list, so the
/// reply was "không thấy phiên đang chạy nữa" for the session huba had just
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
/// Trả về **số phiên đã quá ngưỡng mà vòng này còn giữ lại** — thứ `hubad` đọc
/// để thôi ngủ đủ hai phút.
///
/// 🔴 VÌ SAO CON SỐ NÀY TỒN TẠI — Hà 2026-08-23: *"sao nó đủ điều kiện chuyển
/// phiên mới nhưng không tự chuyển, cho đến khi tôi chạy một lệnh bất kỳ mới
/// vào luồng chuyển phiên"*.
///
/// Tôi trả lời sai lần đầu ("chờ tối đa 4–5 phút"). Số đo trên `huba.log` từ
/// 20/08 (26 phiên chạm ngưỡng) nói khác: **trung vị 15 phút**, 14/24 ca chờ
/// quá 10 phút, ca lâu nhất **205 phút**, và **2 phiên chưa bao giờ chuyển**
/// (`93faab89`: 30 lượt kiểm, `Busy` cả 30).
///
/// Gốc là phép LẤY MẪU, không phải một cửa nào sai. Điều kiện nổ đòi `!busy`
/// **và** `idle ≥ idle_sec` cùng đúng tại ĐÚNG khoảnh khắc vòng chạy qua — mà
/// vòng chỉ chạy mỗi `poll_interval_sec` (đo: trung vị **124s**). Một phiên
/// rảnh 150 giây rồi làm tiếp chỉ mở ra một khe hợp lệ 30 giây; lưới lấy mẫu
/// 124 giây bắt được khe ấy chừng một phần tư số lần. Khe càng hẹp, xác suất
/// càng thấp — nên phiên bận theo từng đợt ngắn có thể **không bao giờ** rơi
/// đúng mẫu, đúng như `93faab89`.
///
/// Và đó cũng là lý do "gõ một lệnh thì nó chuyển" nghe như mê tín mà lại
/// đúng: lệnh Telegram đánh thức vòng ngay (waker, `hubad.rs`), tức **thêm một
/// mẫu** ngoài lưới. Đo được: **8/24 lượt nổ cưỡi lên một vòng do lệnh đánh
/// thức** — một phần ba, đủ dày để nhận ra bằng mắt.
///
/// Bản vá không đụng cửa nào (chúng đúng cả — không đóng sổ phiên đang chạy,
/// đang hỏi, hay vừa gõ xong): nó **lấy mẫu dày lên đúng lúc cần**. Còn phiên
/// quá ngưỡng đang bị giữ ⟹ vòng sau ngủ ngắn. Rẻ, vì phép đọc màn tốn kém chỉ
/// chạy cho phiên đã quá ngưỡng (xem `screen_of` bên dưới), mà số ấy thường là
/// 0 hoặc 1.
fn auto_handover(db: &Db, cfg: &Config, live: &crate::sessions::SessionsSnapshot) -> usize {
    if !cfg.auto_handover.enabled {
        return 0;
    }
    let mut watching = 0usize;
    let done: Vec<String> = db
        .cursor_or_log(AUTO_DONE_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    // 🔴 Ngữ cảnh LÚC BÀN GIAO của từng phiên — để "đã bàn giao" thôi là một
    // bản án chung thân.
    //
    // Hà 2026-08-15: *"cả 2 phiên hiện tại đều đang gần full rồi, vậy mà huba
    // không tắt để mở phiên mới"*. Log: 14/08 13:15 bàn giao nổ ở 67%, mở xong
    // phiên kế nhiệm, nhưng phiên cũ đang chạy dở nên không đóng được — rồi id
    // ấy vào sổ `AUTO_DONE_KEY` và **1.791 lượt kiểm sau đó đều trả
    // `AlreadyDone`**, trong khi phiên cũ phình tiếp từ 67% lên 80%.
    //
    // Cái sổ ấy trả lời đúng câu "đã bàn giao chưa", nhưng câu cần hỏi là "còn
    // cần bàn giao nữa không". Nay: đã bàn giao mà ngữ cảnh vẫn leo thêm một
    // mốc thì hỏi lại từ đầu.
    let done_at: std::collections::BTreeMap<String, u8> = db
        .cursor_or_log(AUTO_PCT_KEY)
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
        // chỉ cần một lần rảnh là huba thay cửa sổ vô tận. Gốc đã vá (phiên mới
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
            watching += 1;
            continue;
        }
        let why = auto_handover_why(
            pct,
            cfg.auto_handover.at_percent,
            // "Đã bàn giao" chỉ còn tính khi ngữ cảnh CHƯA leo thêm một mốc
            // kể từ lần ấy — xem `done_at` và `AUTO_RETRY_STEP`.
            // 🔴 KHÔNG NHỚ ĐÃ BÀN GIAO Ở % NÀO ⟹ HỎI LẠI, đừng khoá chặt hơn.
            //
            // Bản cũ rơi về `unwrap_or(at_percent)`, tức thiếu dữ liệu thì mốc
            // hỏi-lại thành `ngưỡng + 10` = 70%. Một phiên bàn giao hụt ở 61%
            // nằm im tới 70% mà không ai biết vì sao — và "không ai biết vì
            // sao" là vì cái mốc ấy KHÔNG phải số đo nào cả, nó là một giá trị
            // mặc định đội lốt số đo.
            //
            // Luật 11b của dự án nói đúng ca này: *một phép đo hỏng không phải
            // một sự thật về thế giới*. Quên mất mốc cũ thì điều đã biết chỉ
            // còn "từng bàn giao", chưa đủ để giữ lại — nên thả cho các cửa
            // sau (`Busy`, `Asking`, `TooFresh`) quyết, đúng như phiên chưa
            // từng bàn giao lần nào.
            already_handed_over(&s.session_id, pct, &done, &done_at),
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
                // Đếm ĐÚNG những phiên đã quá ngưỡng mà còn bị giữ — không đếm
                // `NotFull`, vì phiên dưới ngưỡng không có gì để canh và đếm nó
                // vào đây là bắt daemon thức suốt ngày cho một việc không có.
                watching += 1;
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
                // Ghi luôn ngữ cảnh lúc này, để lần sau biết nó đã leo thêm
                // bao nhiêu kể từ lần bàn giao ấy.
                {
                    let mut at: std::collections::BTreeMap<String, u8> = db
                        .cursor_or_log(AUTO_PCT_KEY)
                        .and_then(|v| serde_json::from_str(&v).ok())
                        .unwrap_or_default();
                    at.insert(s.session_id.clone(), pct);
                    // 🔴 CẮT THEO SỔ `done`, KHÔNG CẮT THEO THỨ TỰ KHOÁ.
                    //
                    // Hà 2026-08-24: *"Trong danh sách phiên tôi thấy có 1 phiên
                    // 64% rồi tại sao chưa tự chuyển, tôi thấy vấn đề này chạy
                    // không được ổn định"*. Anh mô tả đúng cả triệu chứng lẫn
                    // tính chất: nó KHÔNG ổn định, và cái quyết định phiên nào
                    // hỏng là **thứ tự chữ cái của uuid**.
                    //
                    // Bản cũ cắt bằng `at.keys().next()` — khoá NHỎ NHẤT của
                    // `BTreeMap`, tức uuid xếp trước theo bảng chữ cái, chẳng
                    // liên quan gì tới tuổi. Chú thích ngay trên nó viết "nhớ 50
                    // phiên gần nhất"; mã thì nhớ 50 phiên có uuid LỚN NHẤT.
                    //
                    // Đo được trên DB thật lúc phát hiện: `auto_handover:pct`
                    // mở đầu bằng khoá `5a7f2f4a` — **mọi khoá bắt đầu bằng 0–4
                    // đã bị xoá sạch**, trong khi `auto_handover:done` (một
                    // `Vec`, cắt từ đầu nên đúng là cũ-trước) vẫn giữ chúng.
                    // Phiên `1ad3e613` rơi đúng khe ấy: có trong `done`, mất
                    // trong `pct` ⟹ `AlreadyDone` với mốc sai, đứng im ở 63%
                    // suốt nhiều giờ.
                    //
                    // Gốc sâu hơn một tầng: **hai cuốn sổ cho một sự thật, cắt
                    // bằng hai luật khác nhau** thì sớm muộn cũng lệch. Nay
                    // cuốn `pct` bám hẳn vào `done` — cùng danh sách, nên không
                    // còn hai luật để mà lệch.
                    at.retain(|k, _| next.contains(k));
                    if let Ok(v) = serde_json::to_string(&at) {
                        let _ = db.set_cursor(AUTO_PCT_KEY, &v);
                    }
                }
                if let Err(e) =
                    db.record_spend("auto_handover", &h.new_session_id, h.cost_usd, &s.name)
                {
                    logging::error("spend_record_failed", json!({ "err": e.to_string() }));
                }
                // …rồi MỞ phiên mới và ĐÓNG phiên cũ (Hà chốt 2026-08-12, cách
                // A): *"tự chủ động đóng phiên rồi mở phiên mới luôn"*. Trước
                // đó huba dừng ở chỗ đưa một dòng `claude --resume …` cho chủ
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
                            // 🔴 ĐÓNG HỤT THÌ GIAO CHO SỔ ĐÓNG, đừng bỏ đó.
                            //
                            // Hà 2026-08-15: *"cả 2 phiên hiện tại đều đang gần
                            // full rồi, vậy mà huba không tắt để mở phiên mới"*.
                            // Log kể đúng chuyện đã xảy ra: 14/08 13:15 bàn giao
                            // nổ ở 67%, 13:16 mở xong phiên kế nhiệm, rồi
                            // `handover_old_window_not_closed` — *"đã gõ /exit
                            // nhưng phiên vẫn đang chạy dở sau 30 giây"*. Sau đó
                            // KHÔNG AI thử lại: cờ `AlreadyDone` bật, 1.791 lượt
                            // kiểm tiếp theo đều dừng ở đó, và phiên cũ cứ phình
                            // từ 67% lên 80% với hai cửa sổ cùng sống.
                            //
                            // Gốc là một định nghĩa sai: bàn giao coi là XONG khi
                            // mở được phiên mới, trong khi việc chưa xong chừng
                            // nào phiên cũ còn sống. Máy móc để làm nốt thì đã
                            // có sẵn từ `/close`: sổ đóng ngó lại mỗi 30 giây,
                            // nhắc ra chat mỗi 2 phút, bỏ cuộc ở phút thứ 10 và
                            // NÓI RA. Đưa cửa sổ hụt vào đúng cuốn sổ ấy.
                            if w.closed_err.is_some() {
                                if let Ok(Some(old_w)) = crate::keys::window_of(&s.tty) {
                                    remember_closing(
                                        db,
                                        &s.session_id,
                                        old_w,
                                        &crate::sessions::shown(s),
                                        chrono::Utc::now().timestamp(),
                                    );
                                    logging::info(
                                        "handover_close_deferred",
                                        json!({ "session": s.session_id, "window": old_w,
                                                "why": "phiên cũ còn chạy dở — sổ đóng sẽ ngó lại" }),
                                    );
                                }
                            }
                            HandoverMove::Opened {
                                tty: &w.tty,
                                new_id,
                                closed_err: w.closed_err.as_deref(),
                            }
                        }
                        // Phiên mới chưa chào đời ⟹ con trỏ KHÔNG chuyển: nó
                        // phải trỏ vào một phiên gõ được, mà ở đây chưa có phiên
                        // nào cả — và cửa sổ cũ thì huba đã giữ lại.
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
                // `tfl5_chat_sent`, KHÔNG có một dòng telegram nào — tức huba tự
                // đóng cửa sổ đang làm việc của chủ máy rồi báo vào đúng cái
                // phòng anh không mở. Mà đây là tin duy nhất trong cả huba xảy
                // ra khi **không ai bấm gì**: bỏ sót nó là bỏ sót đúng lúc cần
                // nhất. `announce_changes` đã đi hai mồm từ đầu; chỗ này quên.
                //
                // Nút thì gắn có điều kiện — luật 14: chỉ trỏ vào phiên còn
                // sống, và ở đây phiên mới sống là điều kiện của chính nhánh
                // `Opened`. Ngoại lệ có chủ ý so với `enter_button` (nó bỏ nút
                // khi target == phiên đang theo): ở đây huba VỪA tự chuyển con
                // trỏ sang phiên mới, nên luật ấy sẽ gỡ nút trong 100% trường
                // hợp — và từ 0af884c, bấm vào phiên là thấy luôn màn, tức nút
                // này là đường ngắn nhất để nhìn tận mắt cái cửa sổ huba vừa mở.
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
                    // Không có nút (hoặc không có inbox — `huba once` chạy tay):
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
    watching
}

/// Sổ những `(phiên, lệnh)` đã tự chạy — để không chạy lại cùng một dòng.
pub const AUTORUN_DONE_KEY: &str = "auto_run:done";

// 🪦 `autorun_allows` + `SHELL_JOINERS` GỠ 2026-08-24 — Hà chọn **mức 2**:
// *"Chỉ dấu, bỏ allow"*, sau khi hỏi *"Tại sao lại cần allow làm gì vậy?"*.
//
// Cổng ấy sinh ra vì `auto_run` ĐOÁN theo hình dạng (`commands_in_report`),
// nên nó phải tự chặn lại thứ chính nó vừa đoán bừa. Nay nguồn đổi hẳn: chỉ
// chạy dòng phiên **CỐ Ý đánh dấu** bằng [`crate::keys::RUN_MARK`]. Không còn
// phép đoán thì không còn thứ để mà chặn — giữ cả hai là dựng hai cổng cho một
// câu hỏi, đúng hình dạng lỗi `CLAUDE.md` §7 đã ghi.
//
// Cái KHÔNG mất theo: dấu chỉ nói *"mô hình cố ý bảo chạy"*, không nói *"chủ
// máy cho phép"*. Hà biết và chọn thế. Ghi ở đây để lần sau không ai "vá cho
// an toàn" bằng cách lặng lẽ dựng lại một danh sách.

/// Tự bấm hộ nút `▶️` cho phiên đang ĐỨNG CHỜ — trả về số lệnh đã xếp hàng.
///
/// 🔴 Hà 2026-08-23: *"luồng kiểm tra phiên dừng lại chờ sẽ quét nội dung trả về
/// có lệnh cần tôi chạy thì sẽ chạy luôn lệnh đó, kết quả chạy được sẽ gửi vào
/// hàng chờ của phiên đó luôn"*.
///
/// Không tự dựng đường chạy mới: nó xếp đúng `/runin <phiên> <lệnh>` vào hàng
/// đợi của huba — **cùng một dòng chữ mà nút `▶️` xếp** (xem `RunQuick`). Nhờ
/// thế phần chạy-và-dán-ngược chỉ có MỘT bản: hubad chạy bằng `/bin/zsh -lc`
/// rồi gõ khối `[huba chạy hộ]` vào ô nhập của chính phiên ấy. Dựng bản thứ hai
/// là hẹn ngày hai bản nói khác nhau — hình dạng lỗi đã lặp nhiều lần ở tệp này.
///
/// Bốn hàng rào, và không cái nào thừa:
/// ① **Chỉ phiên `đứng chờ`** — hỏi `sessions::state_of`, chỗ duy nhất quyết
///    định tình trạng, nên nó tự loại phiên đang chạy / đang hỏi / dừng vì lỗi /
///    đã tắt / còn lệnh nền. Chép lại điều kiện ở đây là mở đường cho hai chỗ
///    trả lời khác nhau về cùng một phiên.
/// ② **Chỉ dòng phiên ĐÃ ĐÁNH DẤU** ([`crate::keys::marked_commands`]) — không
///    đoán theo hình dạng. Đây là chỗ thay cho danh sách cho phép của bản đầu;
///    xem bia mộ ngay trên hàm này để biết vì sao đổi.
/// ③ **Một lệnh mỗi phiên mỗi vòng** — màn thường in cả một khối nhiều dòng;
///    bắn cả khối là mất quyền can thiệp giữa chừng, đúng lý do `auto_handover`
///    cũng chỉ làm một phiên mỗi vòng.
/// ④ **Sổ đã-chạy theo `(phiên, lệnh)`** — không có nó thì dòng lệnh vẫn nằm
///    nguyên trong `last_text` ở vòng sau, và huba chạy lại nó mãi mãi. Đây là
///    hàng rào dễ quên nhất vì nó chỉ lộ ra ở vòng thứ hai.
fn auto_run(db: &Db, cfg: &Config, live: &crate::sessions::SessionsSnapshot) -> usize {
    if !cfg.auto_run.enabled {
        return 0;
    }
    // 🔴 SỔ THEO PHIÊN, KHÔNG PHẢI MỘT DANH SÁCH CHUNG CÓ TRẦN.
    //
    // Bản đầu giữ một `Vec` chung trần 200, cắt từ đầu. Một phiên ồn ào bắn đủ
    // 200 lượt là đẩy văng mã của phiên im lặng — mà `last_text` của phiên im
    // lặng thì KHÔNG đổi, dòng lệnh vẫn nằm nguyên ở đó, nên vòng sau bắn lại
    // nó. Đúng cái hàng rào ④ tuyên bố chống được.
    //
    // Nay khoá theo phiên và dọn theo SỰ SỐNG: phiên còn thì sổ của nó còn
    // nguyên; phiên chết thì cả mục biến mất cùng lúc với thứ sinh ra nó.
    let mut done: std::collections::BTreeMap<String, Vec<String>> = db
        .cursor_or_log(AUTORUN_DONE_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    let alive: std::collections::BTreeSet<&str> = live
        .sessions
        .iter()
        .filter(|s| s.host != "dead")
        .map(|s| s.session_id.as_str())
        .collect();
    let before = done.clone();
    done.retain(|k, _| alive.contains(k.as_str()));
    let mut fired = 0usize;
    for s in &live.sessions {
        // 🔴 `ST_WAIT` là nhánh MẶC ĐỊNH của `state_of`, tức "không chứng minh
        // được là bận" — không phải "đã chứng minh được là rảnh". Lối vào tệ
        // nhất là `host == "unknown"`: `state_of` chỉ chặn `"dead"`, mà
        // `host_of` trả `"unknown"` khi phép dò `ps` KHÔNG CHẠY ĐƯỢC. Tức đúng
        // lúc huba mù nhất thì phiên đọc ra "đứng chờ".
        //
        // `pending_for_display` trong cùng tệp `sessions.rs` đã gộp
        // `"dead" | "unknown"` từ lâu; `state_of` thì chưa. Ở đây đòi thêm cho
        // đủ: mù thì KHÔNG bắn.
        if s.host == "unknown" || crate::sessions::state_of(s).0 != crate::sessions::ST_WAIT {
            continue;
        }
        let Some(text) = s.last_text.as_deref() else {
            continue;
        };
        let seen = done.entry(s.session_id.clone()).or_default();
        let Some(line) = crate::keys::marked_commands(text, 3)
            .into_iter()
            .find(|l| !seen.contains(&quick_token(&s.session_id, l)))
        else {
            continue;
        };
        // 🔴 GHI SỔ LỆNH TRƯỚC KHI XẾP HÀNG — nhưng là cuốn sổ `QUICK_KEY`, thứ
        // mang THƯ MỤC của dòng lệnh. Nút `▶️` không chỉ xếp một dòng chữ: nó
        // gọi `remember_quick` trước, và `root_for_command` tra chính cuốn ấy.
        // Bỏ bước này là đường tự chạy đi vòng qua bản vá 13/08 — dòng
        // `bash scripts/x.sh` sẽ chạy ở GỐC workspace, nơi `scripts/` là một
        // thư mục CÓ THẬT chứa những tệp khác hẳn.
        let cmd = crate::sessions::Cmd {
            line: line.clone(),
            cwd: quick_cwd(db, &s.session_id, &line),
        };
        remember_quick(db, &s.session_id, std::slice::from_ref(&cmd));
        match crate::telegram::inbox() {
            Some(tg) => {
                logging::info(
                    "auto_run_firing",
                    json!({ "session": s.session_id, "name": s.name, "cwd": cmd.cwd,
                            "cmd": crate::exec::truncate(&line, 120) }),
                );
                // 🔴 KHÔNG `quiet`. Bản đầu dùng `push_text_quiet`, và
                // `reply_in_channel` nuốt MỌI câu trả lời của lượt ấy — cả kết
                // quả lẫn `⚠ không thấy phiên…`. Tức một cỗ máy tự thi hành
                // lệnh shell chạy hoàn toàn im với chủ máy, bằng chứng duy nhất
                // là một dòng log trong một tệp trên máy.
                //
                // Tôi tự viết cách đó sáu dòng rằng "một cỗ máy tự chạy mà lặng
                // lẽ là thứ không ai phát hiện ra là đã hỏng" — rồi làm ngược
                // lại ở đúng chỗ quan trọng nhất: không phải lúc nó KHÔNG làm
                // gì, mà lúc nó CÓ làm.
                tg.push_text(&format!("/runin {} {}", s.session_id, line));
                // …và CHỈ ghi sổ khi đã xếp được hàng. Ghi trước thì lượt nào
                // chưa có hòm thư sẽ đóng dấu "đã chạy" cho một lệnh chưa hề
                // chạy — và vì `quick_token` là hằng số, nó sẽ KHÔNG BAO GIỜ
                // chạy nữa. Đổi một lần-chạy-lặp (thấy được) lấy một lần-mất-
                // hẳn-im-lặng (không thấy được) là đổi sai phía.
                done.entry(s.session_id.clone())
                    .or_default()
                    .push(quick_token(&s.session_id, &line));
                fired += 1;
            }
            None => logging::warn(
                "auto_run_no_inbox",
                json!({ "session": s.session_id,
                        "why": "chưa có hòm thư Telegram — lệnh KHÔNG xếp hàng và KHÔNG vào sổ" }),
            ),
        }
        // MỘT lệnh mỗi VÒNG cho cả máy, không phải mỗi phiên. Chú thích bản đầu
        // hứa "một lệnh mỗi phiên mỗi vòng" rồi dẫn `auto_handover` làm chỗ dựa
        // — nhưng `auto_handover` `break` ở vòng NGOÀI, kèm đúng lý do: *"làm
        // hàng loạt trong một nhịp là thứ không ai kịp can"*. Tám phiên đứng
        // chờ thì bản cũ bắn tám lệnh trong một nhịp.
        break;
    }
    // Dọn sổ của phiên đã chết, và giữ mỗi phiên tối đa 50 mã.
    for v in done.values_mut() {
        if v.len() > 50 {
            let cut = v.len() - 50;
            v.drain(..cut);
        }
    }
    if done != before {
        if let Ok(v) = serde_json::to_string(&done) {
            let _ = db.set_cursor(AUTORUN_DONE_KEY, &v);
        }
    }
    fired
}

/// Phiên này đã bàn giao rồi VÀ chưa leo thêm một mốc ⟹ thôi hỏi lại.
///
/// 🔴 TÁCH RA THÀNH HÀM THUẦN 2026-08-25, theo luật §13 vừa thêm vào
/// `CLAUDE.md` (*phép đo phải đổi được trạng thái*). Bài kiểm trước dựng lại
/// phép quyết định này **bên trong chính nó**, nên nó xanh kể cả khi mã sản
/// xuất hỏng — một cổng không bao giờ đỏ được là một cổng không có. Nay bài
/// kiểm gọi đúng hàm mà `auto_handover` gọi.
///
/// Hai vế, và vế thứ hai là bản vá của ca `1ad3e613`: **quên mốc cũ ⟹ HỎI
/// LẠI**, không khoá chặt hơn. Bản trước rơi về `unwrap_or(at_percent)`, tức
/// thiếu dữ liệu thì mốc hỏi-lại thành `ngưỡng + 10` — một giá trị mặc định
/// đội lốt số đo (luật 11b: một phép đo hỏng không phải một sự thật về thế giới).
pub fn already_handed_over(
    sid: &str,
    pct: u8,
    done: &[String],
    done_at: &std::collections::BTreeMap<String, u8>,
) -> bool {
    done.iter().any(|d| d == sid)
        && done_at
            .get(sid)
            .is_some_and(|d| pct < d.saturating_add(AUTO_RETRY_STEP))
}

/// Ngủ bao lâu khi CÒN phiên quá ngưỡng đang bị giữ.
///
/// Suy từ `idle_sec` chứ không gõ cứng một con số, vì hai thứ ấy là **cùng một
/// bài toán lấy mẫu**: cửa nổ đòi phiên im ít nhất `idle_sec`, nên khe hợp lệ
/// hẹp nhất mà ta còn muốn bắt cũng cỡ ấy. Lấy mẫu thưa hơn khe thì bắt hụt —
/// đó đúng là chuyện đã xảy ra với lưới 124 giây (xem [`auto_handover`]). Chia
/// sáu để có ít nhất vài mẫu rơi vào trong khe, chứ không phải một mẫu may rủi.
///
/// Cận dưới 15 giây: dày hơn nữa thì mỗi mẫu là một lượt `osascript` đọc màn,
/// và cái giá ấy có thật (đọc màn cho mọi phiên mỗi vòng đã từng kéo một vòng
/// từ 18 lên 90 giây, đo 2026-08-10).
/// Cận trên là chính `poll_interval_sec`: hàm này chỉ được phép **rút ngắn**
/// giấc ngủ, không bao giờ kéo dài nó.
pub fn watch_slice_sec(cfg: &Config) -> u64 {
    let need = cfg.auto_handover.idle_sec.max(1);
    (need / 6).clamp(15, cfg.poll_interval_sec.max(15))
}

/// Phiên nào đã được huba tự đóng sổ rồi — để không đóng hai lần.
pub const AUTO_DONE_KEY: &str = "auto_handover:done";

/// Ngữ cảnh lúc bàn giao của từng phiên (`sid` → `%`).
pub const AUTO_PCT_KEY: &str = "auto_handover:pct";

/// Leo thêm chừng này phần trăm kể từ lần bàn giao trước thì HỎI LẠI.
///
/// Mười điểm: đủ rộng để một lượt trả lời dài không tự châm ngòi, đủ hẹp để một
/// phiên đã bàn giao hụt không kịp bò từ 67% lên 80% trong im lặng — đúng quãng
/// đã xảy ra thật ngày 2026-08-14.
const AUTO_RETRY_STEP: u8 = 10;

/// Chuyện gì THẬT SỰ xảy ra khi huba thay cửa sổ — ba kết cục, không gộp.
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
    /// id sau 12 giây) — nên huba **giữ nguyên cửa sổ cũ**. `asking` là hộp chọn
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

/// Câu huba nói khi nó vừa TỰ thay cửa sổ làm việc của chủ máy.
///
/// Thuần, và tách ra làm hàm riêng vì đây là tin nhắn khó nhất trong cả huba: nó
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
                     ✅ Cửa sổ CŨ huba GIỮ NGUYÊN — không mất gì, phiên cũ vẫn ở đó. \
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

/// Ids of the sessions THIS huba started, newest last.
///
/// Nothing in `claude agents` says who opened a session: a background row looks
/// the same whether huba ran `/new` from the phone or someone typed `claude --bg`
/// in a window. The phone needs the difference — those are the rows it can stop
/// and talk to — so huba writes down what it starts instead of guessing.
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

/// Stamp `started_by_hub` on the rows huba opened.
///
/// Lives here rather than in `sessions` because it needs the book; every
/// surface that shows sessions (portal snapshot, `huba sessions`) calls it, so
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

/// The session huba stopped a moment ago, if it is the one being asked for.
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
                // trục trặc của huba sống ở mức `warn` và cố ý không lên đây.
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
            "⚠ không đọc được sổ vòng chạy — xem logs/huba.log".to_string()
        }
    }
}

// 🔴 ĐÃ BỎ CẢ CHẶNG HỎI VÒNG (`ADAPTER_NAMES`, `adapter_enabled`,
// `poll_adapter`, `ingest`), 2026-08-14, cùng lượt gỡ tfl5.
//
// Chặng ấy tồn tại để hỏi phòng chat: một vòng lặp qua danh sách kênh, mỗi kênh
// một dòng `runs`, đọc con trỏ, và ghi con trỏ SAU khi lệnh đã chạy. Sau khi
// phòng đóng, danh sách còn đúng một tên và `poll_adapter` trả `unknown adapter`
// cho chính cái tên ấy — tức `/ingest` lẫn `huba ingest` chỉ còn đúng một câu trả
// lời khả dĩ: *"disabled in config"*. Đó đúng là thứ luật riêng của dự án cấm:
// **một động từ phân tích được mà không có việc gì để làm**.
//
// Telegram không hỏi vòng, nó ĐẨY TỚI: `telegram::Inbox` giữ một luồng riêng
// chạy `getUpdates`, xếp tin vào hộp, rồi đánh thức vòng bằng `Waker`. Không có
// con trỏ nào để tiến ở đây, vì `getUpdates` tự tiến bằng `offset` của nó.
//
// Đi theo nó là bảng `runs`: không còn ai ghi. Xem `run_once` — nay chính nó ghi
// một dòng cho mỗi vòng, để `huba status` và khối "lỗi gần đây" của `/doctor` còn
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
/// mà tiêu chí gốc của huba gọi tên: ngồi trước máy thì `claude agents` là thấy,
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

/// Bề ngang MỘT dòng chữ Telegram trên màn điện thoại của chủ máy, đo bằng CỘT.
///
/// 🔴 Vì sao hằng số này phải có, và vì sao nó là thứ bản gọn lần một thiếu.
/// Ngày 2026-08-22 danh sách phiên được cắt từ 3,1 xuống **1,9 dòng/phiên** —
/// đo bằng `\n`. Hà mở lên và nói *"Chưa làm gọn danh sách phiên à"*. Cả hai
/// đều đúng: số dòng LOGIC đã giảm, còn thứ Hà nhìn là số dòng SAU KHI XUỐNG
/// DÒNG. Lượt `/session` lúc 21:09 (lấy nguyên văn từ `logs/huba.log`) dài
/// **671 ký tự cho 6 phiên = 112 ký tự/phiên**, tức mỗi phiên vẫn ăn 3–4 dòng
/// trên màn. Đếm `\n` cho một khung tự xuống dòng là một **phép đo mù** —
/// đúng họ với `OPERATING-CHARTER.md` §2d.
///
/// ⚠ **38 là ƯỚC LƯỢNG, không phải số đo.** Suy ra: màn 390pt, bong bóng tin
/// ~300pt, cỡ chữ hệ thống 16pt ⟹ ~8pt/ký tự ⟹ ~37. Chưa đếm trên ảnh chụp
/// thật lần nào. Sai số ở đây chỉ làm hàng ngắn hơn hoặc dài hơn một dòng, chứ
/// không làm mất dữ kiện nào — nhưng khi có ảnh chụp thì sửa CHỖ NÀY, đừng đi
/// cắt thêm ở từng chỗ vẽ.
const PHONE_COLS: usize = 38;

/// Trần cho MỘT phiên: hai dòng nhìn thấy. Hà 2026-08-22: *"gom lại thành 1
/// khối thôi"* — một khối đọc được trên điện thoại là hai dòng, không phải hai
/// `\n`.
const ROW_COLS: usize = PHONE_COLS * 2;

/// Đích chạm của một hàng phiên — thay cho cái nút lặp lại hàng ấy ở đáy tin.
///
/// 🔴 Hà 2026-08-22, ảnh 21:36: *"Vẫn đang hiện cả danh sách lẫn nút thừa
/// thãi"*. Nút Telegram cao CỐ ĐỊNH, không co theo nhãn, nên rút gọn nhãn không
/// lấy lại được pixel nào — sáu nút vẫn ăn gần nửa màn. Đường duy nhất là bỏ
/// nút và đưa đích chạm LÊN chính hàng chữ (`verbs.rs`, payload `s_<uuid>`).
const TAP: &str = "👉";

/// Tên phiên co lại tới đâu thì dừng. Dưới mức này thì cái tên thôi phân biệt
/// được hai hàng, mà phân biệt được mới là việc của nó — thà tràn sang dòng thứ
/// ba còn hơn in ra một cái tên không chỉ vào đâu.
const NAME_FLOOR: usize = 16;

/// Bề ngang đo bằng CỘT, không bằng ký tự.
///
/// `chars().count()` nói `🟪` và `a` bằng nhau; trên màn thì không. Bảng này cố
/// ý **ước lượng THỪA** (mọi thứ từ U+2000 trở lên tính 2 cột, kể cả `…` vốn
/// chỉ 1): đoán thừa thì hàng ngắn hơn dự tính, đoán thiếu thì hàng tràn dòng —
/// mà tràn dòng đúng là thứ cần dẹp. Dấu ghép tiếng Việt (U+0300–U+036F) và
/// biến thể emoji (U+FE00–U+FE0F, U+200D) không chiếm cột nào.
fn cols(s: &str) -> usize {
    s.chars()
        .map(|c| match c as u32 {
            0x0300..=0x036F | 0xFE00..=0xFE0F | 0x200D => 0,
            n if n >= 0x2000 => 2,
            _ => 1,
        })
        .sum()
}

/// Cắt theo CỘT (xem [`cols`]), không theo ký tự — `exec::truncate` đếm ký tự
/// nên một cái tên mở đầu bằng ô màu dự án luôn dài hơn nó tưởng một cột.
fn cut_to_cols(s: &str, budget: usize) -> String {
    if cols(s) <= budget {
        return s.to_string();
    }
    // Chừa một cột cho dấu `…`: một cái tên bị cắt mà không nói là bị cắt thì
    // đọc lên như một cái tên khác.
    let room = budget.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = cols(&c.to_string());
        if used + w > room {
            break;
        }
        out.push(c);
        used += w;
    }
    format!("{}…", out.trim_end())
}

/// Gột KHUNG của TUI khỏi chữ sắp gửi đi.
///
/// 🔴 Hà 2026-08-23, ảnh một tin toàn gạch ngang: *"sao nội dung tin không cắt
/// bỏ các ký tự thừa thãi này đi, để làm gì?"*.
///
/// Đo trên bản chụp màn THẬT đang nằm trong kho (`tests/fixtures/
/// shot-screen-2026-08-18.txt`): 17 dòng, trong đó **2 dòng là vạch `─` dài 97
/// ký tự**. Trên màn ~38 cột mỗi vạch ấy nở thành ba dòng, tức 6 trong 17 dòng
/// của tin là gạch — nhiều hơn cả phần chữ ở nhiều lượt `/shot`.
///
/// ⚠ CHỈ GỘT Ở TẦNG HIỂN THỊ, và đó là ràng buộc thật chứ không phải lời dặn
/// suông: `keys::box_start` NEO vào chính mấy vạch ấy để tìm ô nhập của
/// `claude` (xem chú thích của nó — bản trước neo `╭`, và nó đã trượt ở mọi
/// lượt đọc từ khi `claude` bỏ khung). Gột TRƯỚC lúc phân tích là làm mù chính
/// chỗ đọc ô nhập.
///
/// Hai phép cắt, và không phép nào đụng vào chữ:
/// ① dòng mà bỏ khoảng trắng đi thì CHỈ CÒN ký tự khung ⟹ bỏ cả dòng;
/// ② dòng có chữ thật thì chỉ tỉa khung ở HAI ĐẦU (viền dọc, và những đoạn
///    `───` trang trí ôm lấy một tiêu đề).
pub fn strip_box_rules(text: &str) -> String {
    // U+2500–U+257F là khối "Box Drawing" của Unicode; U+2580–U+259F là "Block
    // Elements" (▏▕█▄), thứ TUI cũng dùng để vẽ viền và thanh cuộn.
    fn la_khung(c: char) -> bool {
        matches!(c as u32, 0x2500..=0x259F)
    }
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let co_chu = line.chars().any(|c| !c.is_whitespace() && !la_khung(c));
        if !co_chu {
            // Dòng chỉ có khung (hoặc rỗng): giữ nhiều nhất MỘT dòng trống, để
            // đoạn văn không dính liền nhau sau khi mấy vạch biến mất.
            if !matches!(out.last(), Some(l) if l.is_empty()) {
                out.push(String::new());
            }
            continue;
        }
        let tia = line
            .trim_matches(|c: char| la_khung(c) || c.is_whitespace())
            .to_string();
        out.push(tia);
    }
    while matches!(out.first(), Some(l) if l.is_empty()) {
        out.remove(0);
    }
    while matches!(out.last(), Some(l) if l.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// Nhãn Remote Control mà TUI `claude` căn phải ở thanh trạng thái.
///
/// Lấy từ CHÍNH bản `claude` đang cài, không phải từ trí nhớ (2026-08-23):
/// `…:"/rc reconnecting",color:"warning"};if(r||t)return{label:"/rc active",…`
/// và `let e5l = v7r.label==="/rc active" && !ggD ? "/rc" : v7r.label`.
const RC_HINTS: [&str; 4] = ["/rc reconnecting", "/rc active", "/rc failed", "/rc"];

/// Gột GỢI Ý BÀN PHÍM căn phải khỏi chữ sắp gửi đi điện thoại.
///
/// 🔴 Hà 2026-08-23: *"Sao cuối tin gửi tele lại có / rc"*.
///
/// Nó không phải chữ của huba — huba chuyển tiếp nguyên văn màn, và `/rc` nằm
/// sẵn ở mép phải thanh trạng thái của phiên:
///
/// ```text
///   ⏵⏵ auto mode on (shift+tab to cycle) · ← 2 agents        …150 dấu cách…   /rc
/// ```
///
/// Đếm trên 20.000 dòng log gần nhất: `/rc active` ×29, `/rc` ×21, `/rc failed`
/// ×12 — luôn ở cuối dòng chế độ quyền, luôn sau một dải cách dài.
///
/// Nó là gợi ý cho NGƯỜI NGỒI TRƯỚC MÁY (bấm để nối Remote Control). Trên điện
/// thoại nó vô nghĩa hai lần: không bấm được, và dải cách căn phải của nó nở
/// thành một hàng trống trên màn 38 cột.
///
/// ⚠ NEO VÀO CHUỖI, KHÔNG NEO VÀO "ĐOẠN CĂN PHẢI". Cắt mọi thứ sau một dải
/// cách dài thì gọn hơn thật, nhưng nó ăn cả bảng kẻ cột và cả dòng `…  +35
/// lines` của công cụ — chữ THẬT mà người đọc cần. Cùng bài học với
/// [`strip_box_rules`]: nới phạm vi là tự xoá nội dung của mình. Muốn dẹp gợi ý
/// khác thì thêm vào [`RC_HINTS`], nơi mỗi dòng là một chuỗi đã đo.
pub fn strip_keyboard_hints(text: &str) -> String {
    let cut_one = |line: &str| -> Option<String> {
        let t = line.trim_end();
        let head = RC_HINTS.iter().find_map(|h| t.strip_suffix(h))?;
        // Phải có DẢI CÁCH ngăn: `.../rc` trong một đường dẫn hay một câu thì
        // không phải cái nhãn ấy, và cắt nó là cắt vào chữ.
        head.ends_with("  ").then(|| head.trim_end().to_string())
    };
    text.lines()
        .map(|l| cut_one(l).unwrap_or_else(|| l.to_string()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `[dùng Bash]` KHÔNG phải lời — nó là dấu vết công cụ.
///
/// 🔴 Đo trên lượt `/session` 21:09 ngày 22/08: **4 trong 6** dòng `💬` là
/// `[dùng Bash]` hoặc `[dùng Read]`. Dòng `💬` sinh ra để trả lời *"phiên nào
/// đáng mở ra"* (Hà 2026-08-12); khi hàng ngay trên nó đã in `⚡` kèm động từ
/// đang chạy thì `[dùng Bash]` không thêm được bit nào — nó chỉ là bốn dòng đẩy
/// hai phiên cuối ra khỏi màn. Cùng một phép cắt với `· tự duyệt` của bản gọn
/// lần một: thứ lặp lại ở gần hết mọi hàng thì không phân biệt được gì.
///
/// Lượt nào có CHỮ thật lẫn với dấu vết (`"Đang xem… [dùng Read]"`) thì giữ —
/// chỗ này chỉ dẹp hàng KHÔNG có gì ngoài dấu vết.
fn only_tool_marks(s: &str) -> bool {
    let mut rest = s.to_string();
    while let Some(i) = rest.find("[dùng ") {
        let Some(j) = rest[i..].find(']') else { break };
        rest.replace_range(i..i + j + 1, "");
    }
    rest.trim().is_empty()
}

/// [`session_list_text`] dựng thành HTML, với **CẢ HÀNG là một đích chạm**.
///
/// 🔴 Hà 2026-08-22, ngay sau lượt bỏ nút: *"Nút nhỏ quá rất khó bấm"*. Lượt ấy
/// đổi sáu cái nút rộng hết bề ngang lấy sáu cái icon `👉` rộng chừng hai chục
/// pixel — nhỏ hơn cả đầu ngón tay, tức mới đi được nửa đường: đúng là hết
/// trùng lặp, nhưng cái bấm được thì teo lại. Thứ có bề rộng ĐÚNG BẰNG cái nút
/// vừa bỏ là cả cái hàng, nên cả hàng đi vào trong `<a>`.
///
/// Nhận diện hàng bằng MÃ NGẮN, không bằng tên: hai phiên cùng một dự án giống
/// nhau tới ba chục cột đầu (`[dwork]·Tiếp dwork…` · `[dwork]·Tiếp tục DS04…`),
/// nên nhận theo tên sẽ dán đích chạm của phiên này lên hàng của phiên kia —
/// cùng họ với lỗi `text.find(nhãn)` mà `telegram::Link` đã phải bỏ. Tám ký tự
/// hex thì duy nhất trong đúng cái danh sách này, và ĐÃ nằm sẵn cuối mỗi hàng.
///
/// Trả về thêm SỐ HÀNG bọc được: chỗ gọi phải so nó với số hàng thật, vì nửa
/// danh sách bấm được nửa không thì ngón tay học sai một lần rồi thôi tin cả
/// cái danh sách.
///
/// Dòng KHÔNG phải hàng phiên (tiêu đề, `💬`, dòng chân) vẫn đi qua
/// [`tame_auto_links`] y như đường `html_with_links`: Telegram tự biến `docs/…`
/// hay `update.sh` thành liên kết ra web, và một câu cuối mọc link lạ thì đọc
/// như huba vừa gửi cái gì đó ra ngoài.
pub fn session_list_html(text: &str, sessions: &[crate::sessions::LiveSession]) -> (String, usize) {
    let taps: Vec<(String, String)> = sessions
        .iter()
        .take(MAX_SESSION_BUTTONS)
        .filter_map(|s| {
            let href = crate::telegram::deep_link(&format!("s_{}", s.session_id))?;
            Some((short_id(&s.session_id).to_string(), href))
        })
        .collect();
    tap_rows_html(text, &taps)
}

/// Danh sách tab của Chrome, cùng bố cục với danh sách phiên.
///
/// Cùng bố cục là có chủ ý, không phải lười: hai danh sách này nằm trong CÙNG
/// một buồng chat, và một ngón tay vừa học "chạm cả hàng" ở `/session` thì phải
/// dùng lại được cái đã học ở `/web`. Khoá tra cứu (`<cửa sổ>.<tab>`) đứng
/// CUỐI, đúng luật của hàng phiên: nó chỉ được đọc lúc sắp gõ một lệnh nữa.
pub fn web_list_text(tabs: &[crate::browser::Tab]) -> String {
    if tabs.is_empty() {
        return "Chrome đang mở nhưng không có tab nào.".to_string();
    }
    let mut out = format!("🌐 {} tab đang mở\n", tabs.len());
    for t in tabs {
        let eye = if t.active { "👁 " } else { "" };
        let key = format!("{}.{}", t.win, t.idx);
        let host = web_host(&t.url);
        let tail = format!(" · {host} · {key}");
        let room = ROW_COLS
            .saturating_sub(cols(eye) + cols(TAP) + 1 + cols(&tail))
            .max(NAME_FLOOR);
        let title = if t.title.trim().is_empty() {
            "(chưa có tiêu đề)"
        } else {
            t.title.trim()
        };
        out.push_str(&format!("{eye}{}{tail}\n", cut_to_cols(title, room)));
    }
    out
}

/// Tên miền, phần người ta thật sự đọc để biết mình đang ở đâu.
///
/// Đường dẫn đầy đủ thì dài hơn cả tiêu đề và gần như luôn bị cắt — mà nửa đầu
/// của một URL bị cắt (`https://mail.google.com/mail/u/0/#inb…`) không nói thêm
/// gì so với tên miền, chỉ tốn chỗ của tiêu đề.
pub fn web_host(url: &str) -> String {
    let s = url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let s = s.split('/').next().unwrap_or(s);
    s.trim_start_matches("www.").to_string()
}

/// Đích chạm cho từng hàng tab — neo vào khoá `· <cửa sổ>.<tab>` ở cuối hàng.
///
/// Neo mang cả dấu ngăn ô chứ không phải mỗi `1.2`: một tiêu đề trang có thể
/// chứa `1.2` (số phiên bản), và neo khớp nhầm thì cú chạm của hàng này rơi vào
/// hàng kia — cùng cái bẫy `text.find(nhãn)` mà `telegram::Link` đã phải bỏ.
pub fn web_taps(tabs: &[crate::browser::Tab]) -> Vec<(String, String)> {
    tabs.iter()
        .filter_map(|t| {
            let href = crate::telegram::deep_link(&format!("wb_{}_{}", t.win, t.idx))?;
            Some((format!(" · {}.{}", t.win, t.idx), href))
        })
        .collect()
}

/// Trần chữ cho một trang web gửi về Telegram.
///
/// Cắt thì phải NÓI — một trang cắt im lặng đọc lên y hệt một trang ngắn, và
/// người đọc sẽ kết luận sai về nội dung chứ không kết luận sai về huba.
const WEB_TEXT_MAX: usize = 3500;

/// `/web` — một chỗ quyết định cho cả bốn dạng tham số.
///
/// Trả về (chữ, các đích chạm). Tách khỏi chỗ gửi để kiểm được bằng test: mọi
/// nhánh ở đây là chữ vào, chữ ra.
pub fn web_route(want: &str) -> (String, Vec<(String, String)>) {
    let want = want.trim();
    // `<cửa sổ>.<tab>` — chuyển sang một tab đã liệt kê.
    if let Some((w, t)) = want.split_once('.') {
        let so = |x: &str| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit());
        if so(w) && so(t) {
            let (w, t) = (w.parse().unwrap_or(0), t.parse().unwrap_or(0));
            return match crate::browser::chon(w, t) {
                Ok(tab) => (
                    format!("👁 Đang xem {} · {}", tab.title, web_host(&tab.url)),
                    Vec::new(),
                ),
                Err(e) => (format!("⚠ {e}"), Vec::new()),
            };
        }
    }
    match want {
        "" => match crate::browser::tabs() {
            Ok(tabs) => (web_list_text(&tabs), web_taps(&tabs)),
            Err(e) => (format!("⚠ {e}"), Vec::new()),
        },
        "doc" | "đọc" | "text" | "chu" | "chữ" => match crate::browser::chu_trang() {
            Ok(chu) => {
                let chu = chu.trim();
                if chu.chars().count() > WEB_TEXT_MAX {
                    let cut: String = chu.chars().take(WEB_TEXT_MAX).collect();
                    (
                        format!(
                            "{cut}\n\n… cắt ở {WEB_TEXT_MAX} ký tự (trang dài {} ký tự).",
                            chu.chars().count()
                        ),
                        Vec::new(),
                    )
                } else if chu.is_empty() {
                    (
                        "Trang này không có chữ nào đọc được.".to_string(),
                        Vec::new(),
                    )
                } else {
                    (chu.to_string(), Vec::new())
                }
            }
            Err(e) => (format!("⚠ {e}"), Vec::new()),
        },
        url => match crate::browser::mo(url) {
            Ok(tab) => (
                format!("🌐 Đã mở {} · {}", tab.title, web_host(&tab.url)),
                Vec::new(),
            ),
            Err(e) => (format!("⚠ {e}"), Vec::new()),
        },
    }
}

/// Lõi dùng chung: bọc CẢ HÀNG mang một cái neo vào trong `<a>`.
///
/// Tách ra khỏi [`session_list_html`] khi `/web` cần đúng bố cục ấy cho danh
/// sách tab (2026-08-23). Chép bản thứ hai thì hai danh sách sẽ lệch nhau ở
/// lượt sửa đầu tiên — cùng bài học `session_button_label` đã trả giá: một lời
/// hứa "cùng bộ" giữ bằng tay thì gãy ngay lượt đổi đầu tiên.
///
/// `taps` là các cặp (NEO, địa chỉ). Neo phải duy nhất trong đúng cái danh sách
/// ấy; hàng không mang neo nào thì đi qua [`tame_auto_links`] như chữ thường.
pub fn tap_rows_html(text: &str, taps: &[(String, String)]) -> (String, usize) {
    let mut out = String::new();
    let mut wrapped = 0usize;
    for line in text.lines() {
        match taps.iter().find(|(sid, _)| line.contains(sid.as_str())) {
            Some((_, href)) => {
                // `👉` nằm TRONG `<a>`: nó là dấu hiệu "chạm được", nên nó phải
                // chạm được. Để ngoài là dựng lại đúng cái đích tí xíu vừa bỏ.
                out.push_str(&format!(
                    "<a href=\"{}\">{} {}</a>",
                    crate::telegram::html_escape(href),
                    TAP,
                    crate::telegram::html_escape(line)
                ));
                wrapped += 1;
            }
            None => out.push_str(&tame_auto_links(&crate::telegram::html_escape(line))),
        }
        out.push('\n');
    }
    if !text.ends_with('\n') {
        out.pop();
    }
    (out, wrapped)
}

pub fn session_list_text(
    sessions: &[crate::sessions::LiveSession],
    focus: &str,
    now_ms: i64,
) -> String {
    if sessions.is_empty() {
        return "Không có phiên nào đang sống.".to_string();
    }
    // 🔴 GOM MỖI PHIÊN THÀNH MỘT KHỐI — Hà 2026-08-22: *"chỉnh lại nội dung
    // lệnh session cho gọn đi, gom lại thành 1 khối thôi"*.
    //
    // Bản trước tiêu **ba dòng** cho một phiên: hàng đầu, rồi hai hàng phụ thụt
    // vào bốn dấu cách. Bảy phiên ⟹ 22 dòng, và trên màn 390px thì phiên cuối
    // nằm ngoài tầm nhìn — đúng thứ `MAX_SESSION_BUTTONS` sinh ra để tránh, mà
    // lại tự gây ra bằng chiều dọc thay vì bằng số hàng.
    //
    // Ba phép cắt, và cả ba đều đo được chứ không phải gu thẩm mỹ:
    // ① hàng phụ *tình trạng* gộp thẳng vào hàng đầu — nó vốn là phần tiếp của
    //    cùng một câu, tách ra chỉ tốn một dòng và bốn dấu cách;
    // ② chế độ quyền lên ĐẦU DANH SÁCH khi cả danh sách giống nhau (đo 22/08:
    //    **7/8 phiên cùng `auto`**) — xem `session_meta`;
    // ③ bỏ thụt đầu dòng: nó là thứ chia một phiên thành ba khối con, đúng cái
    //    Hà bảo gom lại.
    //
    // KHÔNG cắt: nhãn tình trạng (Hà 12/08 — bốn tình trạng phải phân biệt
    // được, và một cái icon trần thì phải học thuộc mới đọc nổi), động từ đang
    // chạy (Hà 10/08 — *"ui chưa thể hiện được phiên đang làm gì"*), câu cuối
    // 💬 (thứ nói phiên nào ĐÁNG mở ra), và dấu 👁.
    //
    // Chế độ quyền chỉ gom lên đầu khi MỌI phiên có khai chế độ đều khai giống
    // nhau. Hàng không khai gì (cửa sổ Terminal trần) vẫn không in gì — nó
    // không có chế độ, chứ không phải thiếu dữ liệu.
    let shown_rows: Vec<&crate::sessions::LiveSession> =
        sessions.iter().take(MAX_SESSION_BUTTONS).collect();
    let modes: std::collections::BTreeSet<&str> = shown_rows
        .iter()
        .map(|s| permission_label(s))
        .filter(|m| !m.is_empty())
        .collect();
    let one_mode = (modes.len() == 1).then(|| *modes.iter().next().unwrap());
    let mut out = format!(
        "📋 {} phiên đang sống{}\n",
        sessions.len(),
        one_mode.map(|m| format!(" · đều {m}")).unwrap_or_default()
    );
    for s in shown_rows {
        let eye = if !focus.is_empty() && s.session_id == focus {
            "👁"
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
        // 🔴 MỘT CHỖ QUYẾT ĐỊNH TÌNH TRẠNG — `sessions::state_of`. Bảng `match`
        // từng nằm ngay đây, và nó thiếu đúng một bậc: phiên đang chờ mà còn
        // lệnh chạy NỀN thì đọc ra "đứng chờ", im về nửa còn lại (Hà 2026-08-19:
        // *"vẫn đang có shell đang chạy nhưng danh sách nút phiên thể hiện đã
        // dừng"*). Đưa bảng ấy xuống `sessions.rs` để tin tự phát dùng CÙNG một
        // bộ ký hiệu — hai màn nói khác nhau về một trạng thái thì không ai đối
        // chiếu được.
        // 🪦 Bảng `match` bốn nhánh từng nằm ngay đây, kèm bài học 13/08 về
        // `▶ ⏸ ⏹` — bài học ấy đi theo sang `sessions::state_of` và vẫn là ràng
        // buộc; chỉ chỗ đứng đổi.
        let (icon, label) = crate::sessions::state_of(s);
        // 🔴 ĐỘNG TỪ THAY CHỖ CHỮ "đang chạy" — Hà 2026-08-22, sau bản gọn lần
        // một: *"Chưa làm gọn danh sách phiên à"*. Hàng của một phiên đang chạy
        // in CẢ HAI vế: `⚡ đang chạy · Drizzling… 16m14s`. Vế sau nói đúng điều
        // vế trước nói, và nói kỹ hơn — kèm đồng hồ. Đó là một vế viết hai lần,
        // 14 cột mỗi hàng, trên **4/6 hàng** của lượt đo 21:09.
        //
        // Ba tình trạng kia KHÔNG có động từ (đứng chờ · dừng lại HỎI · đã tắt)
        // nên chữ của chúng ở nguyên chỗ cũ: luật Hà 2026-08-12 — *"bốn tình
        // trạng phải phân biệt được"*, icon trần thì phải học thuộc mới đọc nổi
        // — không mất chỗ nào. Và hàng đang chạy MÀ KHÔNG có động từ (phiên vừa
        // bắt đầu, `activity` rỗng) vẫn in `⚡ đang chạy`, chứ không rơi về một
        // cái icon câm.
        let verb = s.activity.as_deref().unwrap_or_default().trim().to_string();
        let verb_moved = icon == crate::sessions::ST_RUN && !verb.is_empty();
        // 🔴 ICON TÌNH TRẠNG ĐỨNG TRƯỚC TÊN, chữ ở lại ô của nó — Hà 2026-08-22:
        // *"Chuyển icon trạng thái lên đứng trước tên phiên sẽ dễ nhìn hơn"*.
        //
        // Đây cũng đúng thứ cái NÚT vẫn làm từ đầu (`session_button_label`:
        // `{tình trạng} {nguồn} {tên} · {tài khoản}`), nên lượt bỏ nút đã lấy
        // mất một cách đọc mà chưa trả lại: mắt quét DỌC mép trái tìm phiên nào
        // đang chạy, mà icon thì nằm ở ô thứ hai — lệch một quãng khác nhau ở
        // mỗi hàng vì tên dài ngắn khác nhau. Đưa nó lên cột đầu thì cả sáu
        // icon xếp thành một cột thẳng.
        //
        // CHỮ vẫn ở ô thứ hai, không đi theo icon: luật Hà 2026-08-12 (bốn tình
        // trạng phải đọc được bằng chữ) không đổi, và gộp cả cụm lên đầu thì
        // tên phiên — thứ ngón tay đang tìm — bị đẩy ra sau một quãng dài ngắn
        // tuỳ tình trạng.
        // 🔴 CHỮ TÌNH TRẠNG ĐI RA, ICON Ở LẠI — Hà 2026-08-25: *"text trạng
        // thái không cần vì có icon rồi"* · *"'đứng chờ' bỏ đi"*.
        //
        // Đây LẬT một luật cũ của chính anh (2026-08-12: *"bốn tình trạng phải
        // phân biệt được"*), và lật đúng: hồi ấy icon là bốn CHẤM TRÒN khác
        // MÀU (`🟢 🟡 🔴 ⚫`) — không phân biệt nổi trên màn 390px, nên chữ phải
        // gánh. Ngày 19/08 bộ ký hiệu đổi sang bốn HÌNH khác nhau
        // (`⚡ 💤 ❓ ❌ 🪦 🌀`, xem `sessions::ST_*`), mỗi hình tự nói nghĩa của
        // nó. Từ lúc ấy chữ thành bản sao thứ hai — chỉ chưa ai gỡ.
        //
        // Động từ thì Ở LẠI: `Embellishing… 2m10s` không nói lại cái icon, nó
        // nói phiên đang làm GÌ (Hà 2026-08-10: *"ui chưa thể hiện được phiên
        // đang làm gì"*). Phiên đang chạy mà chưa có động từ thì ô này rỗng —
        // icon `⚡` đã đủ, không cần một chữ "đang chạy" chen vào.
        let _ = label;
        let run = if verb_moved {
            verb.clone()
        } else {
            String::new()
        };
        // Dự án ĐANG LÀM đứng trước tên: tên phiên do `claude` tự đặt
        // ("projects-ff") không nói được gì, còn `cwd` thì giống hệt nhau ở mọi
        // dòng trên máy này — xem `sessions::folder_from_tail`.
        // Nhãn dự án thay cho tên tự sinh — xem `sessions::display_name`.
        // Thứ tự đọc: ai · làm gì · rồi mới tới hai cái khoá tra cứu (tài khoản,
        // id) — chúng chỉ được đọc lúc sắp GÕ một lệnh nữa, nên đứng cuối.
        let meta = session_meta(s, now_ms, one_mode.is_none(), verb_moved);
        // Ghép bằng cách LỌC rồi `join`, không nối chuỗi tay: một cửa sổ
        // Terminal trần không có tài khoản, và bản nối tay in ra
        // `💤 đứng chờ ·  · win` — hai dấu chấm ôm khoảng trắng, đúng thứ
        // "gọn" vừa đi ra để dẹp.
        //
        // 🔴 NGÂN SÁCH CỘT, KHÔNG PHẢI NGÂN SÁCH `\n` — xem [`ROW_COLS`]. Đuôi
        // hàng (tình trạng · số đo · tài khoản · id) dựng TRƯỚC và không ai cắt
        // nó: mỗi ô ở đấy là một dữ kiện đã có người hỏi tới. Thứ co lại là cái
        // TÊN, vì nó là ô duy nhất có phần đuôi thừa đọc được — `[dwork]·Hoàn
        // tất A-DDOC và chờ phản hồi ph…` dài 43 cột, và ba chữ cuối của nó
        // không nói thêm gì mà đẩy cả hàng sang dòng thứ ba.
        //
        // Nhờ vậy hàng dài ra bao nhiêu thì tên co lại bấy nhiêu — chứ không
        // phải mỗi lần thêm một ô lại đi cắt tay một chỗ khác rồi quên.
        let tail: Vec<String> = [
            run,
            meta,
            s.account.clone(),
            short_id(&s.session_id).to_string(),
        ]
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .collect();
        // 🔴 `⌨` ĐI RA KHỎI HÀNG, các nguồn KHÁC ở lại — Hà 2026-08-23: *"bỏ icon
        // bàn phím đi thay thành các icon tình trạng vào đó"*.
        //
        // Đo trên ảnh 21:36: **5/6 hàng** mang đúng một ký tự `⌨`, nên nó không
        // phân biệt được gì — cùng lý do `· tự duyệt` và `⚡ đang chạy` đã đi ra.
        // Cái giá của nó không chỉ là hai cột: nó chen vào GIỮA icon tình trạng
        // và tên phiên, đúng chỗ vừa dọn ra để hai thứ ấy đứng cạnh nhau.
        //
        // `🌙` (nền) · `💻` (VS Code) · `🔌` (rời tty) thì Ở LẠI: chúng là NGOẠI
        // LỆ, và mỗi cái trả lời một câu người ta sắp hỏi — gõ vào được không.
        // Bỏ cả cột nguồn là bỏ luôn câu trả lời ấy; bỏ MẶC ĐỊNH thì câu trả lời
        // chỉ hiện đúng lúc nó mang tin. Cột icon tình trạng vẫn thẳng vì nó
        // đứng TRƯỚC, không phải sau.
        let src = source_icon(&s.host);
        let head = match src {
            "⌨" => format!("{eye}{icon} "),
            _ => format!("{eye}{icon} {src} "),
        };
        // Chừa chỗ cho đích chạm `👉 ` mà `session_tap_anchors` chèn vào đầu
        // dòng lúc dựng HTML: nó không nằm trong chuỗi này (bài kiểm và các kênh
        // khác đọc bản chữ thuần), nhưng nó CHIẾM CỘT trên màn Telegram — không
        // trừ ra thì đúng những hàng vừa khít lại tràn sang dòng thứ ba.
        let used = cols(&head) + cols(TAP) + 1 + tail.iter().map(|p| cols(p) + 3).sum::<usize>();
        let room = ROW_COLS.saturating_sub(used).max(NAME_FLOOR);
        let what = cut_to_cols(&crate::sessions::shown(s), room);
        let mut parts = vec![format!("{head}{what}")];
        parts.extend(tail);
        out.push_str(&format!("{}\n", parts.join(" · ")));
        // Phiên đang hỏi thì CÂU HỎI thay chỗ câu cuối: câu cuối của nó chính là
        // lời dẫn vào câu hỏi, còn thứ người đọc cần là hỏi gì và chọn được gì.
        if let Some(a) = &s.asking {
            let head = if a.header.is_empty() { "" } else { &a.header };
            out.push_str(&format!(
                "⚠ {}{}\n",
                if head.is_empty() {
                    String::new()
                } else {
                    format!("{head}: ")
                },
                crate::exec::truncate(&a.question, 120)
            ));
            for (i, o) in a.options.iter().take(9).enumerate() {
                out.push_str(&format!("  {}. {}\n", i + 1, crate::exec::truncate(o, 60)));
            }
            continue;
        }
        if let Some(said) = &s.last_text {
            let said = said.replace(['\n', '\r'], " ");
            let said = said.trim();
            if !said.is_empty() && !only_tool_marks(said) {
                // Cùng ngân sách với hàng phiên: câu cuối cũng là chữ trên cùng
                // một cái màn, và 74 KÝ TỰ ở bản trước đọc ra 3 dòng vì `💬` +
                // ô màu dự án + dấu nháy mã đều là ký tự HAI CỘT.
                out.push_str(&format!(
                    "💬 {}\n",
                    cut_to_cols(said, ROW_COLS.saturating_sub(cols("💬 ")))
                ));
            }
        }
    }
    if sessions.len() > MAX_SESSION_BUTTONS {
        // Cắt bớt mà im lặng thì danh sách này nói dối về số phiên đang chạy.
        out.push_str(&format!(
            "…còn {} phiên nữa chưa liệt kê — dùng /session <id>\n",
            sessions.len() - MAX_SESSION_BUTTONS
        ));
    }
    if focus.is_empty() {
        out.push_str("Chưa theo phiên nào — chạm một hàng để theo.");
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
/// kèm Enter — huba biến cái gõ nhầm thành một lượt gõ thật.
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
/// phải huba đọc nhầm nguồn — nó đọc ĐÚNG màn thật (`contents of selected tab`,
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
    /// Lúc huba ẨN cửa sổ vì `close` không ăn (epoch giây; 0 = chưa ẩn lần nào).
    ///
    /// Mục KHÔNG rời sổ khi bị ẩn — xem `hidden_next`. Trước 17/08 nó rời sổ
    /// ngay, nên một cửa sổ ẩn là một cửa sổ khuất mắt VĨNH VIỄN: nó rời khỏi
    /// mọi danh sách của huba nên không ai quay lại đóng nó nữa.
    #[serde(default)]
    pub h: i64,
    /// Lần thử đóng LẠI gần nhất sau khi ẩn (epoch giây; 0 = chưa thử lần nào).
    #[serde(default)]
    pub r: i64,
}

/// Bao lâu ngó lại một lần. Hà nói thẳng con số này.
const CLOSE_CHECK_SEC: i64 = 30;

/// Chờ tới đây mà cửa sổ vẫn bận thì THÔI CHỜ IM — nói ra và trả quyền quyết
/// định lại cho chủ máy.
///
/// 🔴 Hà 2026-08-14: *"Rõ ràng phiên dwork dừng rồi, tôi gửi lệnh close rồi 1h
/// hay lại xem shot nó vẫn ở đó"*. Đọc log đúng như thế: `/close` lúc 11:06:19,
/// huba gõ `/exit`, rồi `close_still_busy` đều đặn 30 giây một lần — 20s · 60s ·
/// 133s · 167s · 204s · 247s… và **không một dòng nào ra tới Telegram**. Câu hứa
/// gửi đi lúc đầu là *"Kiểm 30 giây một lần, xong tôi báo"*, nên im lặng ở đây
/// đọc thành "đang chạy êm", trong khi sự thật là huba đang chờ một điều kiện có
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

/// Cửa sổ đã ẩn thì bao lâu thử đóng lại một lần.
///
/// Thưa hơn hẳn nhịp 30 giây của phần còn lại, vì đây là việc của một cửa sổ
/// RÁC — không ai đang chờ nó, và mỗi lượt thử là một lần `osascript` chen vào
/// hàng đợi Apple Event chung với những việc có người chờ.
const CLOSE_HIDDEN_RETRY_SEC: i64 = 300;

/// Thử lại tới bao giờ thì thôi.
///
/// Sáu tiếng, và con số ấy đo từ chính lần hỏng: 17/08 lúc 10:20Z Terminal từ
/// chối đóng năm cửa sổ; tới 14:1xZ — **gần bốn tiếng sau** — đúng những cửa sổ
/// ấy đóng ngay lượt đầu. Một trần nửa tiếng sẽ bỏ cuộc trước khi cơ hội tới,
/// tức xây một cái máy thử-lại rồi tự tắt nó đúng lúc cần.
const CLOSE_HIDDEN_GIVE_UP_SEC: i64 = 6 * 3600;

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
            h: 0,
            r: 0,
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
/// **chưa có id phiên** nên KHÔNG route nào của huba với tới được: `/key`,
/// `/type`, `/shot`, `/close` đều nhắm bằng id. Một cửa sổ huba tự mở, rồi huba
/// tự mất đường vào.
///
/// Nên phép bấm hộ phải sống trong VÒNG CHẠY, không sống trong một lời gọi hàm
/// — cùng bài học với `CLOSING_KEY`: việc kéo dài thì phải có người ngó lại.
/// Không cần sổ: dấu hiệu nằm ngay trên màn, và `trust_dialog_choice` chỉ khớp
/// ĐÚNG hộp ấy (đúng hai lựa chọn, đúng chữ *"trust this folder"*). Màn nào
/// không phải hộp ấy thì hàm không bấm gì cả.
///
/// Quét MỌI tab, không riêng tab huba mở: hộp này hỏi một lần cho mỗi cặp tài
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
        if !tab.is_claude() {
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

/// Một lượt hỏi trong sổ chờ đóng trả lời được gì, thì làm gì tiếp.
///
/// Tách rời vì đây là chỗ DUY NHẤT có phán đoán trong `close_pending_tick`;
/// phần còn lại là `osascript` và Telegram, thứ không bài kiểm nào chạm tới
/// được. Ba mệnh đề nó phải giữ, cả ba đều đã trả giá:
///
/// - **Cửa sổ không còn là việc XONG**, không phải huba mù. Gộp hai thứ ấy làm
///   một là lỗi đã đo được: 190 dòng `close_check_failed` trong 5 tiếng cho một
///   cửa sổ đóng từ lâu (xem `keys::tab_state`).
/// - **Hỏi không được thì GIỮ trong sổ** — luật `Look::Blind`, không đổi.
/// - Nhưng giữ mà im thì đúng bằng cái vừa xảy ra, nên **mù quá lâu cũng phải
///   có tiếng nói**: cùng cái trần đã dùng cho "còn bận quá lâu"
///   (`CLOSE_GIVE_UP_SEC`), cùng lý lẽ — huba thôi canh thì phải nói là thôi,
///   chứ không lặng lẽ hỏi tới vô tận.
pub fn close_step(seen: Option<crate::keys::TabState>, waited_sec: i64) -> CloseStep {
    match seen {
        Some(crate::keys::TabState::Gone) => CloseStep::Gone,
        Some(crate::keys::TabState::Idle) => CloseStep::Close,
        Some(crate::keys::TabState::Busy) if waited_sec >= CLOSE_GIVE_UP_SEC => {
            CloseStep::GiveUpBusy
        }
        Some(crate::keys::TabState::Busy) => CloseStep::Wait,
        None if waited_sec >= CLOSE_GIVE_UP_SEC => CloseStep::GiveUpBlind,
        None => CloseStep::Blind,
    }
}

/// Mục ĐÃ ẨN thì lượt này làm gì.
///
/// 🔴 Vì sao có vòng thử lại (đo 2026-08-17, và nó BÁC một giả thuyết tôi vừa
/// nêu ra): lúc 10:20Z năm cửa sổ từ chối `close` — chạy êm, `osascript` trả 0,
/// cửa sổ đứng nguyên — nên huba ẩn chúng đi và nói đúng là đã ẩn. Gần bốn tiếng
/// sau, gọi tay lên ĐÚNG những cửa sổ ấy, ĐÚNG lệnh ấy: `2151` · `2153` · `2156`
/// đều đóng ngay lượt đầu (`1/false` → `0/false`), trong khi vẫn đang ẩn. Giả
/// thuyết "cửa sổ ẩn không nhận `close`" bị chính phép thử A/B ấy bác bỏ.
///
/// Nên lời từ chối kia là **nhất thời**, không phải thuộc tính của mấy cửa sổ
/// đó — và cái chữa một lời từ chối nhất thời là thử lại. Bỏ mục khỏi sổ ngay
/// khi ẩn (bản cũ) là bảo đảm không bao giờ có ai quay lại: cửa sổ ẩn rời khỏi
/// mọi danh sách của huba, nên nó thành rác vô hình, đúng thứ đã đếm được năm cái
/// chỉ trong một ngày.
pub fn hidden_next(hidden_at: i64, last_retry: i64, now: i64) -> HiddenNext {
    if hidden_at == 0 {
        return HiddenNext::NotHidden;
    }
    if now - hidden_at >= CLOSE_HIDDEN_GIVE_UP_SEC {
        return HiddenNext::GiveUp;
    }
    if now - last_retry.max(hidden_at) >= CLOSE_HIDDEN_RETRY_SEC {
        return HiddenNext::Retry;
    }
    HiddenNext::Wait
}

/// Bốn nước đi của một mục đã ẩn — xem [`hidden_next`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HiddenNext {
    /// Mục chưa từng bị ẩn: đường thường, không phải việc của hàm này.
    NotHidden,
    /// Chưa tới nhịp thử lại.
    Wait,
    /// Thử đóng lại lượt này.
    Retry,
    /// Thử đủ lâu rồi: nói một câu rồi buông.
    GiveUp,
}

/// Sáu nước đi của một mục trong sổ chờ đóng — xem [`close_step`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseStep {
    /// Còn bận, chưa tới trần: chờ tiếp (và nhắc thưa thớt).
    Wait,
    /// Còn bận quá lâu: nói ra, trả quyền quyết định lại cho chủ máy.
    GiveUpBusy,
    /// Rảnh rồi: đóng.
    Close,
    /// Cửa sổ không còn — xong, dù không phải huba làm.
    Gone,
    /// Hỏi không được: giữ trong sổ, hỏi lại lượt sau.
    Blind,
    /// Hỏi mãi không được: nói ra rồi bỏ sổ.
    GiveUpBlind,
}

/// Mỗi vòng: cửa sổ nào hết bận thì đóng, còn bận thì CHỜ TIẾP.
///
/// Bốn kết cục, cả bốn đều nói ra: đóng được · cửa sổ không còn (ai đó đã đóng
/// tay, hoặc `claude` thoát rồi Terminal tự dọn) · còn bận quá lâu · hỏi không
/// được. Ca cuối **giữ nguyên trong sổ** — không hỏi được ≠ không còn, đúng luật
/// `Look::Blind` — cho tới trần `CLOSE_GIVE_UP_SEC` thì nói một câu rồi buông.
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
        let waited = now - c.t;
        // Mục ĐÃ ẨN đi đường riêng: nhịp thưa hơn (không ai đang chờ một cửa sổ
        // rác), một cổng an toàn riêng, và câu nói khác — việc ở đây không còn
        // là "chờ CLI thoát" mà là "dọn nốt cái vỏ".
        if c.h != 0 {
            match hidden_next(c.h, c.r, now) {
                HiddenNext::NotHidden | HiddenNext::Wait => continue,
                HiddenNext::GiveUp => {
                    logging::warn(
                        "close_hidden_gave_up",
                        json!({ "session": id, "window": c.w, "hidden_sec": now - c.h }),
                    );
                    say_closed(cfg, &format!(
                        "⚠ {} — huba thử đóng lại cửa sổ đã ẩn suốt {} tiếng mà Terminal vẫn không chịu, nên thôi. \
                         Nó vẫn khuất mắt và khuất khỏi mọi danh sách của huba; ⌘W khi anh ngồi máy là hết hẳn.",
                        c.n,
                        (now - c.h) / 3600
                    ));
                    done.push(id.clone());
                    continue;
                }
                HiddenNext::Retry => {
                    c.r = now;
                    // 🔴 Cổng an toàn của lượt đóng LẠI: chỉ đóng cửa sổ KHÔNG
                    // CÒN TIẾN TRÌNH nào. id cửa sổ Terminal đánh lại từ số nhỏ
                    // sau khi Terminal khởi động lại, nên một mục cũ có thể trỏ
                    // vào cửa sổ MỚI — và `busy = false` thì một cửa sổ vừa mở,
                    // đang ở dấu nhắc, cũng thoả. Số tiến trình thì không: phiên
                    // thật luôn còn ít nhất cái shell.
                    match crate::keys::tab_process_count(c.w) {
                        Ok(None) => {
                            logging::info(
                                "close_hidden_window_gone",
                                json!({ "session": id, "window": c.w, "hidden_sec": now - c.h }),
                            );
                            say_closed(
                                cfg,
                                &format!("⏹ Cửa sổ đã ẩn của {} nay không còn — hết hẳn.", c.n),
                            );
                            done.push(id.clone());
                        }
                        Ok(Some(0)) => match crate::keys::close_hidden_again(c.w) {
                            Ok(true) => {
                                logging::info(
                                    "close_hidden_retry_worked",
                                    json!({ "session": id, "window": c.w, "hidden_sec": now - c.h }),
                                );
                                say_closed(cfg, &format!(
                                    "⏹ Đóng hẳn được cửa sổ của {} rồi — Terminal từ chối lúc nãy, thử lại sau {} phút thì ăn.",
                                    c.n,
                                    (now - c.h) / 60
                                ));
                                done.push(id.clone());
                            }
                            // Vẫn chưa chịu: im, vì đã nói một lần rồi. Còn
                            // trong sổ nghĩa là còn người ngó lại.
                            Ok(false) => logging::info(
                                "close_hidden_retry_refused",
                                json!({ "session": id, "window": c.w, "hidden_sec": now - c.h }),
                            ),
                            Err(e) => logging::warn(
                                "close_hidden_retry_failed",
                                json!({ "session": id, "window": c.w,
                                        "err": crate::logging::err_chain(&e) }),
                            ),
                        },
                        Ok(Some(n)) => {
                            // Cửa sổ ấy nay có người ở. Đóng nó là đóng thứ của
                            // người khác — buông, và nói ra chứ không im.
                            logging::warn(
                                "close_hidden_now_occupied",
                                json!({ "session": id, "window": c.w, "procs": n }),
                            );
                            say_closed(cfg, &format!(
                                "⚠ Cửa sổ đã ẩn của {} nay đang chạy thứ khác ({} tiến trình) — huba KHÔNG đóng nó nữa.",
                                c.n, n
                            ));
                            done.push(id.clone());
                        }
                        Err(e) => logging::warn(
                            "close_hidden_check_failed",
                            json!({ "session": id, "window": c.w,
                                    "err": crate::logging::err_chain(&e) }),
                        ),
                    }
                    continue;
                }
            }
        }
        // Hỏi MỘT lượt, giữ lại cả câu trả lời lẫn việc không trả lời được —
        // rồi mới phán. Phán đoán nằm trong `close_step`, đo được bằng bài kiểm.
        let seen = match crate::keys::tab_state(c.w) {
            Ok(s) => Some(s),
            Err(e) => {
                logging::warn(
                    "close_check_failed",
                    json!({ "session": id, "window": c.w, "waited_sec": waited,
                            "err": crate::logging::err_chain(&e) }),
                );
                None
            }
        };
        match close_step(seen, waited) {
            CloseStep::Wait | CloseStep::GiveUpBusy => {
                logging::info(
                    "close_still_busy",
                    json!({ "session": id, "window": c.w, "waited_sec": waited }),
                );
                // 🔴 NHÌN TRƯỚC KHI PHÁN — Hà 2026-08-19, ảnh một cửa sổ đứng im
                // sau khi chuyển phiên: *"Chuyển phiên xong phiên cũ bị kẹt như
                // này làm sao qua được"*. "Tab còn bận" có ít nhất bốn nghĩa
                // (xem `sessions::ExitBox`) và tới hôm ấy huba chỉ biết một, nên
                // nó chờ mười phút một cái hộp đang đợi ĐÚNG MỘT phím, rồi bỏ
                // cuộc kèm một lý do bịa: *"CLI đang chạy dở một lượt"*.
                let vuong = match crate::sessions::answer_exit_dialog(c.w) {
                    crate::sessions::ExitBox::Answered(n, tasks) => {
                        let stopped = if tasks.is_empty() {
                            String::new()
                        } else {
                            format!("\nDừng theo: {}", tasks.join(" · "))
                        };
                        say_closed(cfg, &format!(
                            "⌨ {} đang đứng ở hộp *lệnh nền còn chạy* của claude — huba bấm {n} \
                             (thoát và dừng lệnh nền) rồi đóng nốt cửa sổ.{stopped}",
                            c.n
                        ));
                        // Đồng hồ chờ tính lại từ đây: huba vừa RA LẠI lệnh
                        // thoát, nên bỏ cuộc theo cái mốc cũ là bỏ cuộc ngay
                        // sau khi vừa gỡ được cái kẹt.
                        c.t = now;
                        continue;
                    }
                    crate::sessions::ExitBox::Other(k) => Some(format!(
                        "cửa sổ đang chờ anh trả lời một hộp chọn {k} mục — huba KHÔNG trả lời thay anh. \
                         Bấm /key <số> (hoặc /pick <câu>.<lựa chọn>) rồi huba đóng nốt"
                    )),
                    crate::sessions::ExitBox::Blind(why) => Some(format!(
                        "huba không đọc được màn cửa sổ ấy ({why}), nên không biết nó bận vì chạy hay vì đang hỏi"
                    )),
                    crate::sessions::ExitBox::None => None,
                };
                // Hết kiên nhẫn thì NÓI, và trả quyền quyết định lại: huba không
                // tự đóng cứng một cửa sổ đang chạy dở — đóng khi còn tiến trình
                // sống làm Terminal bật hộp thoại "terminate running processes?",
                // mà một hộp thoại thì khoá mọi lệnh tự động sau nó (bài học
                // 08-11, xem `keys::close_window`).
                // Lý do đọc ĐƯỢC thì nói lý do đọc được; đọc không ra thì nói
                // câu cũ — nó là phỏng đoán đúng cho ca thường gặp, và nay nó
                // chỉ còn được nói khi màn thật sự không có hộp nào.
                let vi_sao = vuong.unwrap_or_else(|| {
                    "cửa sổ còn bận, tức CLI đang chạy dở một lượt và `/exit` nằm trong hàng chờ của nó".to_string()
                });
                if waited >= CLOSE_GIVE_UP_SEC {
                    say_closed(cfg, &format!(
                        "⚠ {} vẫn chưa đóng được sau {} phút — {vi_sao}.\nhub THÔI chờ (không tự đóng \
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
                    // mà im mười phút thì người ta đi kiểm tay, đúng cái huba
                    // sinh ra để khỏi phải làm.
                    say_closed(cfg, &format!(
                        "⏳ {} chưa đóng được sau {} phút — {vi_sao}. huba vẫn chờ (bỏ cuộc ở phút thứ {}).",
                        c.n,
                        waited / 60,
                        CLOSE_GIVE_UP_SEC / 60
                    ));
                }
            }
            CloseStep::Close => {
                match crate::keys::close_window(c.w) {
                    Ok(what) => {
                        logging::info(
                            "close_done",
                            json!({ "session": id, "window": c.w, "waited_sec": now - c.t,
                                    "hidden": matches!(what, crate::keys::Closed::Hidden) }),
                        );
                        // Ẩn KHÔNG phải đóng, nên câu báo cũng khác — xem
                        // `keys::close_window`.
                        say_closed(cfg, &match what {
                            // 🔴 Hà 2026-08-19: *"sao nội dung lại thừa và mâu thuẫn nhau thế"*.
                            // Câu cũ nói MỘT sự kiện ba lần — *"Đã đóng hẳn"* + *"CLI chạy nốt rồi
                            // thoát"* + *"cửa sổ terminal đã đóng"* — vì nó được viết để đối lập với
                            // nhánh `Hidden` ngay dưới. Nhưng người đọc chỉ nhận MỘT dòng, không thấy
                            // nhánh kia, nên chỗ đối lập ấy đọc ra thành lặp. Nhánh này chỉ có đúng
                            // một tin: cửa sổ không còn nữa. Nhánh `Hidden` tự nói phần khác biệt
                            // của nó ("Terminal KHÔNG đóng cửa sổ ấy"), nên không cần vế nào ở đây
                            // đứng ra so sánh hộ.
                            crate::keys::Closed::Gone => format!(
                                "⏹ {} đã thoát — cửa sổ đã đóng (chờ {}s).",
                                c.n,
                                now - c.t
                            ),
                            crate::keys::Closed::Hidden => format!(
                                "⏹ {} đã thoát CLI (chờ {}s), nhưng Terminal KHÔNG đóng cửa sổ ấy — huba đã ẩn nó đi. \
                                 Nó rời khỏi mọi danh sách của huba, và cứ {} phút huba thử đóng lại một lượt \
                                 (lời từ chối kiểu này đo được là nhất thời). ⌘W khi anh ngồi máy là hết ngay.",
                                c.n,
                                now - c.t,
                                CLOSE_HIDDEN_RETRY_SEC / 60
                            ),
                        });
                        // Ẩn thì GIỮ TRONG SỔ: cửa sổ ẩn rời khỏi mọi danh sách
                        // của huba, nên bỏ mục đi là bảo đảm không ai quay lại
                        // đóng nó nữa — xem `hidden_next`.
                        if matches!(what, crate::keys::Closed::Hidden) {
                            c.h = now;
                            continue;
                        }
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
            CloseStep::Gone => {
                // Cửa sổ ấy không còn — huba không phải là người đóng, nhưng câu
                // hỏi "đóng xong chưa" đã có câu trả lời, nên đóng sổ và NÓI.
                // Trước 17/08 ca này rơi vào nhánh `Err` (`selected tab` của một
                // cửa sổ 0 tab ép sang chữ là lỗi -1700) và nằm lại trong sổ
                // mãi mãi — xem `keys::tab_state`.
                logging::info(
                    "close_window_gone",
                    json!({ "session": id, "window": c.w, "waited_sec": waited,
                            "seen": "tab_state" }),
                );
                say_closed(cfg, &format!(
                    "⏹ {} đã thoát — cửa sổ ấy không còn nữa (Terminal tự dọn khi shell thoát, hoặc anh đã đóng tay). huba đóng sổ chờ.",
                    c.n
                ));
                done.push(id.clone());
            }
            CloseStep::Blind => {
                // KHÔNG bỏ khỏi sổ: hỏi không được là huba mù, không phải cửa sổ
                // đã đóng. Bỏ đi là im lặng đánh rơi việc. (Dòng warn đã ghi ở
                // chỗ hỏi, kèm `waited_sec`.)
            }
            CloseStep::GiveUpBlind => {
                logging::warn(
                    "close_gave_up_blind",
                    json!({ "session": id, "window": c.w, "waited_sec": waited }),
                );
                say_closed(cfg, &format!(
                    "⚠ {} — {} phút liền huba hỏi Terminal mà không lần nào biết được cửa sổ ấy còn bận hay không, \
                     nên huba THÔI canh. Cửa sổ có thể vẫn còn: ⌘W khi anh ngồi máy, hoặc /terminal để xem lại. \
                     Lý do từng lượt nằm ở log `close_check_failed`.",
                    c.n,
                    waited / 60
                ));
                done.push(id.clone());
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

/// Tên tệp phiên ghi vào scratchpad của chính nó để nhờ huba chạy một lệnh.
pub const RUNIN_INBOX_NAME: &str = "huba-run.txt";

/// Đọc hòm thư của MỌI phiên: `<gốc>/claude-*/…/<id phiên>/scratchpad/huba-run.txt`.
///
/// 🔴 Hà 2026-08-24: *"Phiên a nhận được lệnh chạy → gửi lệnh runin cho huba →
/// huba đẩy vào hàng chờ để chạy → chạy xong lấy kết quả dán vào hàng chờ của
/// phiên a"*, sau khi hỏi *"phải có hướng dẫn để phiên tự lấy đúng id mà huba
/// đang quản lý"*.
///
/// 📌 **Không cần gửi id kèm, và đó là cả thiết kế.** Hai ràng buộc có sẵn ghép
/// đúng vào nhau:
/// ① luật workspace: một phiên chỉ được GHI trong thư mục của chính nó — nên
///    hòm thư không thể đặt trong cây của huba;
/// ② scratchpad của mỗi phiên **mang sẵn id trong đường dẫn**
///    (`…/claude-501/<slug>/<id phiên>/scratchpad`) — đo 2026-08-24: 4/4 thư mục
///    lấy mẫu đều có nhật ký `.jsonl` trùng tên.
///
/// Ghép lại: phiên ghi vào đúng chỗ nó được phép ghi, và **chính chỗ ấy khai
/// hộ nó là ai**. Một id gõ tay thì gõ sai được; một id đọc từ đường dẫn thì
/// không.
///
/// Trả `(id phiên, dòng lệnh, đường dẫn tệp)`. Không chạy gì, không xoá gì —
/// tách thuần để kiểm được mà không cần một cái máy đang chạy `claude`.
/// Chuỗi này có mang hình dạng một uuid phiên không.
///
/// Cùng phép thử với nhánh `full_uuid` của [`split_target`] — tách ra thành hàm
/// để hai chỗ không tự so chuỗi lần nữa mỗi chỗ một kiểu.
pub fn looks_like_uuid(s: &str) -> bool {
    s.len() >= 32
        && s.matches('-').count() == 4
        && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

pub fn scan_session_inboxes(root: &std::path::Path) -> Vec<(String, String, std::path::PathBuf)> {
    let mut out = Vec::new();
    let Ok(tmp) = std::fs::read_dir(root) else {
        return out;
    };
    for claude_dir in tmp.flatten() {
        // `/private/tmp/claude-<uid>` — chỉ nhận đúng hình dạng ấy, đừng đi
        // duyệt cả `/private/tmp`.
        if !claude_dir
            .file_name()
            .to_string_lossy()
            .starts_with("claude-")
        {
            continue;
        }
        let Ok(slugs) = std::fs::read_dir(claude_dir.path()) else {
            continue;
        };
        for slug in slugs.flatten() {
            let Ok(sessions) = std::fs::read_dir(slug.path()) else {
                continue;
            };
            for sess in sessions.flatten() {
                let sid = sess.file_name().to_string_lossy().to_string();
                // Chỉ nhận thư mục mang hình dạng uuid: đó là thứ khai id, và
                // nhận bừa là dán kết quả vào một phiên không tồn tại.
                if !looks_like_uuid(&sid) {
                    continue;
                }
                let f = sess.path().join("scratchpad").join(RUNIN_INBOX_NAME);
                let Ok(body) = std::fs::read_to_string(&f) else {
                    continue;
                };
                // Dòng đầu KHÔNG rỗng là lệnh; phần còn lại coi như ghi chú của
                // phiên. Một tệp nhiều dòng thì lấy dòng đầu chứ không nối —
                // nối là tự dựng lại đúng phép đoán vừa bỏ.
                let Some(cmd) = body.lines().map(str::trim).find(|l| !l.is_empty()) else {
                    continue;
                };
                out.push((sid, cmd.to_string(), f));
            }
        }
    }
    out
}

/// Gốc chứa scratchpad của mọi phiên — nơi đặt hòm thư.
const SCRATCH_ROOT: &str = "/private/tmp";

/// Nhận thư của các phiên rồi xếp vào hàng chờ. Chạy mỗi vòng, rẻ khi không có.
///
/// Đường đi đúng như Hà mô tả: *"Phiên a nhận được lệnh chạy → gửi lệnh runin
/// cho huba → huba đẩy vào hàng chờ để chạy → chạy xong lấy kết quả dán vào
/// hàng chờ của phiên a"*.
///
/// Không dựng đường chạy mới: nó xếp `/runin <phiên> <lệnh>` — **cùng dòng chữ
/// mà nút `▶️` xếp** — nên phần chạy-và-dán-ngược vẫn chỉ có một bản, kèm cả sổ
/// gõ-lại khi Terminal bận (xem [`RUNIN_PENDING_KEY`]).
///
/// 🔴 ĐỔI TÊN TỆP TRƯỚC KHI CHẠY, không phải sau. Một cú chết giữa hai bước là
/// khác nhau hoàn toàn: đổi tên trước thì mất một lệnh (thấy được — tệp `.taken`
/// nằm đó), đổi tên sau thì **chạy lại một lệnh đã chạy** (không thấy được, và
/// với một lệnh không idempotent thì không lùi được). Cùng lý lẽ đã chọn cho
/// `auto_run` hôm qua, chọn ngược phía vì ở đó "mất" là im còn ở đây "mất" là
/// một tệp nhìn thấy được.
fn runin_inbox_tick(db: &Db, cfg: &Config) -> usize {
    let _ = db;
    let mut taken = 0usize;
    for (sid, cmd, path) in scan_session_inboxes(std::path::Path::new(SCRATCH_ROOT)) {
        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let moved = path.with_extension(format!("taken-{stamp}"));
        if let Err(e) = std::fs::rename(&path, &moved) {
            // Không đổi tên được thì KHÔNG chạy: chạy mà không cầm được tệp là
            // hẹn giờ chạy lại nó ở mọi vòng sau.
            logging::error(
                "runin_inbox_not_claimed",
                json!({ "session": sid, "file": path.display().to_string(),
                        "err": e.to_string(), "effect": "lệnh KHÔNG chạy" }),
            );
            continue;
        }
        let alive = crate::sessions::snapshot(cfg)
            .sessions
            .iter()
            .any(|s| s.session_id == sid && s.host != "dead");
        if !alive {
            logging::warn(
                "runin_inbox_session_gone",
                json!({ "session": sid, "cmd": crate::exec::truncate(&cmd, 120),
                        "effect": "không còn phiên nào để dán kết quả vào — bỏ" }),
            );
            continue;
        }
        match crate::telegram::inbox() {
            Some(tg) => {
                logging::info(
                    "runin_inbox_queued",
                    json!({ "session": sid, "cmd": crate::exec::truncate(&cmd, 120) }),
                );
                // 🔴 QUIET: phiên tự nhờ thì kết quả vào phiên là đủ. Xem
                // tham số `quiet` của `watch_long_job` để biết vì sao — 21 lượt
                // hòm thư trong một buổi là 21 tin Hà không hỏi mà vẫn nhận.
                tg.push_text_quiet(&format!("/runin {sid} {cmd}"));
                taken += 1;
            }
            None => logging::warn(
                "runin_inbox_no_inbox",
                json!({ "session": sid, "file": moved.display().to_string(),
                        "why": "chưa có hòm thư Telegram — đổi tên tệp lại để chạy vòng sau" }),
            ),
        }
    }
    taken
}

/// Kết quả `/runin` đã chạy xong mà CHƯA dán được vào phiên.
///
/// 🔴 Hà 2026-08-23: *"Sao chạy lệnh xong lại báo không dán đc vào phiên vì quá
/// 20s"*.
///
/// Lệnh chạy xong thật, kết quả có thật — thứ hỏng là cú `osascript` gõ nó vào
/// cửa sổ Terminal, hết hạn ở [`crate::keys`]`::OSA_TIMEOUT` = 20s. Đo trên
/// `logs/huba.log` ngày 23/08: **386 lượt `osascript quá 20s`**, rải khắp mọi
/// phép hỏi Terminal (`terminal_probe_failed` ×111, `window_of_from_cache`
/// ×98, `keys_screen_read_failed` ×32). Tức không phải khối kết quả quá to —
/// **Terminal không trả lời kịp**, và mọi thứ hỏi nó đều dính.
///
/// Bản cũ tới đây thì bỏ cuộc: in một câu `⚠` ra Telegram kèm 600 ký tự đầu
/// của kết quả, rồi **đánh rơi phần còn lại**. Phiên không bao giờ biết lệnh đã
/// chạy — mà nó là bên duy nhất cần biết, vì nó là bên đang chờ để đi tiếp.
/// Một cú hết hạn 20 giây không phải một sự thật vĩnh viễn về thế giới; nó là
/// một lần hỏi trượt.
///
/// Nên: giữ lại và **gõ lại ở vòng sau**, cùng khuôn với `close_pending_tick`
/// và `trust_dialog_tick` — hai chỗ đã học đúng bài này rồi.
pub const RUNIN_PENDING_KEY: &str = "runin:pending";

/// Bao lâu thì thử dán lại một lần.
const RUNIN_RETRY_SEC: i64 = 30;

/// Thử tới bao lâu thì thôi — và khi thôi thì phải NÓI TO.
///
/// 15 phút: đủ dài cho một cơn Terminal treo (hộp thoại modal chờ người bấm là
/// ca dài nhất đã gặp), đủ ngắn để kết quả còn liên quan tới thứ phiên đang
/// làm. Quá hạn thì kết quả đi ra Telegram NGUYÊN VẸN hơn — im lặng bỏ là đúng
/// cái lỗi đang vá.
const RUNIN_GIVE_UP_SEC: i64 = 900;

/// Một kết quả đang chờ được gõ vào phiên.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingRunin {
    /// Phiên nhận.
    pub s: String,
    /// Dòng lệnh đã chạy — để câu báo nói được nó là kết quả của cái gì.
    pub l: String,
    /// Khối chữ sẽ gõ vào ô nhập.
    pub b: String,
    /// Lần thử gần nhất (giây epoch).
    pub c: i64,
    /// Lần hỏng đầu tiên (giây epoch) — mốc để tính hạn bỏ cuộc.
    pub t: i64,
}

fn runin_pending_book(db: &Db) -> Vec<PendingRunin> {
    db.cursor_or_log(RUNIN_PENDING_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}

fn save_runin_pending(db: &Db, book: &[PendingRunin]) {
    match serde_json::to_string(book) {
        Ok(v) => {
            if let Err(e) = db.set_cursor(RUNIN_PENDING_KEY, &v) {
                logging::error("runin_pending_not_saved", json!({ "err": e.to_string() }));
            }
        }
        Err(e) => logging::error("runin_pending_not_saved", json!({ "err": e.to_string() })),
    }
}

/// Xếp một kết quả vào sổ chờ dán.
pub fn remember_runin_pending(db: &Db, sid: &str, line: &str, block: &str, now: i64) {
    let mut book = runin_pending_book(db);
    // Cùng phiên + cùng lệnh thì thay chỗ cũ: hai bản của một kết quả dán vào
    // một ô nhập là hai lần cùng một chuyện, và bản sau mới là bản đúng.
    book.retain(|p| !(p.s == sid && p.l == line));
    book.push(PendingRunin {
        s: sid.to_string(),
        l: line.to_string(),
        b: block.to_string(),
        c: now,
        t: now,
    });
    // Trần 20: sổ này giữ CẢ khối kết quả (tới `CMD_OUT_MAX`), nên nó nặng hơn
    // hẳn mấy cuốn sổ mã băm bên cạnh.
    while book.len() > 20 {
        book.remove(0);
    }
    logging::warn(
        "runin_pending_queued",
        json!({ "session": sid, "cmd": crate::exec::truncate(line, 120),
                "why": "chưa dán được vào phiên — sẽ gõ lại ở vòng sau" }),
    );
    save_runin_pending(db, &book);
}

/// Như [`remember_runin_pending`], nhưng TỰ MỞ kết nối DB từ cấu hình.
///
/// Luồng chạy nền của `/runin` (`watch_long_job`) chỉ nuốt `Config`, không nuốt
/// `Db` — `Db` giữ một `Connection` của SQLite, thứ không đưa qua biên luồng
/// được. Mở một kết nối riêng ngay tại đây rẻ và đúng: SQLite cho nhiều kết
/// nối trên cùng tệp, và luồng này chỉ ghi đúng một dòng.
///
/// Mở không được thì **NÓI TO**: mất đường này là kết quả rơi hẳn, đúng cái lỗi
/// đang vá — im ở đây là vá xong rồi tự đào lại cái hố cũ.
pub fn remember_runin_pending_for(cfg: &Config, sid: &str, line: &str, block: &str, now: i64) {
    match Db::open(&cfg.db) {
        Ok(db) => remember_runin_pending(&db, sid, line, block, now),
        Err(e) => logging::error(
            "runin_pending_db_failed",
            json!({ "session": sid, "err": e.to_string(),
                    "effect": "kết quả KHÔNG vào được sổ chờ — nó chỉ còn trong tin Telegram" }),
        ),
    }
}

/// Gõ lại những kết quả còn nợ. Chạy mỗi vòng, rẻ khi sổ rỗng.
pub fn runin_pending_tick(db: &Db, cfg: &Config, now: i64) {
    let mut book = runin_pending_book(db);
    if book.is_empty() {
        return;
    }
    let mut keep: Vec<PendingRunin> = Vec::new();
    let mut changed = false;
    for mut p in book.drain(..) {
        if now - p.c < RUNIN_RETRY_SEC {
            keep.push(p);
            continue;
        }
        p.c = now;
        changed = true;
        // Phiên còn sống không, và cửa sổ nào — hỏi lại mỗi lượt, vì cái tty
        // của một phiên đổi được (nó có thể đã được thay cửa sổ bởi chính
        // `auto_handover` trong lúc chờ).
        let tty = crate::sessions::snapshot(cfg)
            .sessions
            .iter()
            .find(|s| s.session_id == p.s && s.host != "dead")
            .map(|s| s.tty.clone());
        let typed = match tty {
            Some(t) if !t.is_empty() => match crate::keys::window_of(&t) {
                Ok(Some(w)) => crate::keys::type_and_send(w, &p.b).map(|d| Some(format!("{d:?}"))),
                Ok(None) => Ok(None),
                Err(e) => Err(e),
            },
            // Phiên đã chết trong lúc chờ: không còn ai để dán vào, và giữ tiếp
            // là giữ rác. Nói ra rồi bỏ.
            _ => {
                logging::warn(
                    "runin_pending_session_gone",
                    json!({ "session": p.s, "cmd": crate::exec::truncate(&p.l, 120) }),
                );
                say_closed(
                    cfg,
                    &format!(
                        "⚠ Phiên nhận kết quả đã tắt trước khi dán được. Kết quả của lệnh:\n\n$ {}\n{}",
                        p.l,
                        crate::exec::truncate(&p.b, 1200)
                    ),
                );
                continue;
            }
        };
        match typed {
            Ok(Some(landed)) => {
                logging::info(
                    "runin_pending_delivered",
                    json!({ "session": p.s, "landed": landed, "waited_sec": now - p.t }),
                );
                say_closed(
                    cfg,
                    &format!(
                        "✅ Đã dán được kết quả vào phiên sau {} giây chờ Terminal.\n$ {}",
                        now - p.t,
                        p.l
                    ),
                );
                continue;
            }
            Ok(None) | Err(_) => {
                if now - p.t >= RUNIN_GIVE_UP_SEC {
                    let why = match &typed {
                        Err(e) => crate::exec::truncate(&e.to_string(), 160),
                        _ => "phiên không có cửa sổ terminal".to_string(),
                    };
                    logging::error(
                        "runin_pending_gave_up",
                        json!({ "session": p.s, "waited_sec": now - p.t, "err": why }),
                    );
                    // Bỏ cuộc thì kết quả ĐI RA NGUYÊN VẸN HƠN, không phải 600
                    // ký tự như bản cũ: đây là lần cuối nó được nhìn thấy.
                    say_closed(
                        cfg,
                        &format!(
                            "⚠ Thử dán vào phiên suốt {} phút không được ({why}). Kết quả:\n\n$ {}\n{}",
                            (now - p.t) / 60,
                            p.l,
                            crate::exec::truncate(&p.b, 1200)
                        ),
                    );
                    continue;
                }
                keep.push(p);
            }
        }
    }
    if changed || keep.len() != runin_pending_book(db).len() {
        save_runin_pending(db, &keep);
    }
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

/// Đường dẫn file huba vừa nhắc tới trên màn — để nút `file:<n>` tìm lại được.
pub const FILES_KEY: &str = "quick:files";

/// Tin đang mang BẢNG lựa chọn của mỗi phiên — để cú bấm sau sửa đúng nó.
pub const PANEL_KEY: &str = "panel:msg";

/// `message_id` của bảng đang mở cho phiên ấy, nếu có.
pub fn panel_id(db: &Db, sid: &str) -> Option<i64> {
    let v = db.cursor_or_log(PANEL_KEY)?;
    let map: std::collections::BTreeMap<String, i64> = serde_json::from_str(&v).ok()?;
    map.get(sid).copied()
}

/// Ghi lại tin vừa mang bảng. `None` ⟹ xoá, vì lần sau không có gì để sửa.
fn remember_panel(db: &Db, sid: &str, message_id: Option<i64>) {
    let mut map: std::collections::BTreeMap<String, i64> = db
        .cursor_or_log(PANEL_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    match message_id {
        Some(id) => {
            map.insert(sid.to_string(), id);
        }
        None => {
            map.remove(sid);
        }
    }
    if let Ok(v) = serde_json::to_string(&map) {
        if let Err(e) = db.set_cursor(PANEL_KEY, &v) {
            logging::error("panel_not_saved", json!({ "err": e.to_string() }));
        }
    }
}

/// Điểm dùng của từng loại lệnh, để xếp menu ☰ — xem [`menu_reorder_if_needed`].
pub const MENU_KEY: &str = "menu:usage";
/// Thứ tự menu đã khai với Telegram lần gần nhất (để biết khi nào phải khai lại).
pub const MENU_ORDER_KEY: &str = "menu:order";

/// Sau bao lâu thì một lượt dùng chỉ còn đáng NỬA.
///
/// 🔴 Hà 2026-08-17, ngay sau khi hỏi menu có tự xếp theo tần suất được không:
/// *"Nó tần suất phải gắn cả thời gian thì mới phản ánh đúng nó đc dùng nhiều
/// thật hay chỉ là trong quá khứ"*.
///
/// Đúng, và đó là khác biệt giữa một cái đếm và một thước đo: đếm thuần thì một
/// lệnh dùng 200 lần hồi tháng trước đứng đầu menu mãi mãi, kể cả khi nó chết
/// hẳn — cùng hình dạng với `/win` và `/project` (0 lượt từ 26/07 mà vẫn nằm
/// trong bảng tới 15/08). Bảy ngày là cỡ một nhịp làm việc: đủ dài để một lệnh
/// dùng hằng ngày không tụt hạng vì nghỉ cuối tuần, đủ ngắn để thói quen tháng
/// trước không quyết định menu tháng này.
pub const MENU_HALF_LIFE_MS: i64 = 7 * 24 * 3_600_000;

/// Điểm cũ, nhìn từ HÔM NAY: mỗi `half_life_ms` trôi qua thì còn một nửa.
///
/// Hàm thuần để đo được: nó là toàn bộ phần "gắn thời gian" của tần suất, và
/// một phép đo tính sai chỗ này thì menu xếp sai mà không ai thấy.
pub fn decayed(score: f64, last_ms: i64, now_ms: i64, half_life_ms: i64) -> f64 {
    if half_life_ms <= 0 || score <= 0.0 {
        return 0.0;
    }
    // Đồng hồ chạy lùi (đổi giờ hệ thống, sổ chép từ máy khác) ⟹ coi như vừa
    // dùng: thà giữ nguyên điểm còn hơn nhân nó lên bằng một số mũ dương.
    let elapsed = (now_ms - last_ms).max(0) as f64;
    score * 0.5f64.powf(elapsed / half_life_ms as f64)
}

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
/// Những đường dẫn CÒN LẠI sau cửa "phải là tệp thật, nằm trong cây của phiên".
///
/// Tách ra vì hai chỗ cần ĐÚNG danh sách này theo ĐÚNG thứ tự ấy: cái nút ở đáy
/// (`remember_files`) và liên kết 📎 giữa chữ (`file_anchors`). Hai phép lọc
/// chép tay là hai chỉ số lệch nhau, và lệch chỉ số ở đây nghĩa là bấm 📎 trên
/// tên tệp này lại tải về tệp khác.
fn kept_paths(db: &Db, cfg: &Config, session_id: &str, paths: &[String]) -> Vec<String> {
    match session_root(db, cfg, session_id) {
        Some(root) => {
            // 🔴 MỘT TỆP MỘT NÚT — Hà 2026-08-19, ảnh tin `[tfl5]` có **hai** nút
            // `📎 docs/du-toan.md` giống hệt nhau: *"Sao lịch sử lẫn lộn các
            // phiên thế"*. Cùng một tệp được nhắc hai lần trong một tin (một
            // lần trong câu văn, một lần trong dòng lệnh `mv`) là chuyện
            // thường; hai cái nút đưa về CÙNG một tệp thì cái thứ hai không nói
            // thêm gì, chỉ tốn một hàng bàn phím và làm người đọc tưởng có hai
            // tệp khác nhau.
            //
            // Trùng nhau tính theo ĐƯỜNG ĐÃ GIẢI, không theo chuỗi: `docs/x.md`
            // và `~/projects/AI/tfl5/docs/x.md` là hai chuỗi khác nhau trỏ vào
            // đúng một tệp. Giữ lần nhắc ĐẦU để thứ tự nút vẫn theo thứ tự đọc.
            // 🔴 ĐẾM RIÊNG HAI CỚ RỤNG. Dòng log dưới đây có từ trước lượt khử
            // trùng (2026-08-20), nên nó khai đúng MỘT cớ — "không phải tệp có
            // thật, hoặc ngoài workspace" — cho cả những lần rụng vì TRÙNG. Một
            // dòng nhật ký khai sai nguyên nhân đắt hơn một dòng không có: nó
            // gửi người đọc đi tìm một cái tệp không hề thiếu, đúng lúc họ mở
            // log ra vì thấy ít nút hơn số tệp trong tin.
            let mut da_co: Vec<std::path::PathBuf> = Vec::new();
            let mut kept: Vec<String> = Vec::new();
            let mut trung = 0usize;
            for p in paths {
                let Some(that) = sendable_file(p, &root, &cfg.workspace_root) else {
                    continue;
                };
                if da_co.contains(&that) {
                    trung += 1;
                    continue;
                }
                da_co.push(that);
                kept.push(p.clone());
            }
            if kept.len() < paths.len() {
                logging::info(
                    "quick_files_filtered",
                    json!({ "kept": kept.len(), "seen": paths.len(),
                            "dup": trung, "unsendable": paths.len() - kept.len() - trung,
                            "why": "dup = cùng MỘT tệp được nhắc nhiều lần (một tệp một nút); \
                                    unsendable = không phải tệp có thật, hoặc nằm ngoài workspace" }),
                );
            }
            kept
        }
        // 🔴 Không tra được thư mục phiên thì CHỈ giữ đường tuyệt đối. Đường
        // tương đối lúc ấy không giải được — giữ nó lại là dựng một cái nút mà
        // cú bấm chắc chắn trả "không biết phiên ấy làm ở thư mục nào", tức một
        // lời hứa suông (cùng bài học với `📎 com.dipgle.hubd.plist` 14/08).
        None => paths
            .iter()
            .filter(|p| p.starts_with('/') || p.starts_with("~/"))
            .cloned()
            .collect(),
    }
}

pub fn remember_files(
    db: &Db,
    cfg: &Config,
    session_id: &str,
    paths: &[String],
) -> Vec<(String, String)> {
    // 🔴 MỘT CÁI TÊN KHÔNG PHẢI MỘT TỆP. Hà 2026-08-14, ảnh chụp một tin có nút
    // 📎 `com.dipgle.hubd.plist`: *"Com.dipgle.hubd.plist đâu phải là file"*.
    // Đúng — đó là một cái tên nhắc giữa câu văn của chính huba, và tệp thật thì
    // nằm ở `~/Library/LaunchAgents`, ngoài cây làm việc của phiên.
    //
    // Cửa "chỉ gửi tệp NẰM TRONG thư mục phiên" vốn đã có, nhưng nó đặt ở lúc
    // BẤM (`send_document`). Nên cái nút vẫn mọc ra, vẫn mời bấm, và chỉ trả
    // lời "chưa gửi được" sau khi người ta bấm — tức huba dựng một lời hứa rồi
    // để người dùng đi phát hiện hộ rằng nó rỗng. Hỏi ngay lúc DỰNG thì rẻ
    // (một lần `stat`) và cái nút không tồn tại nếu không có gì để mở.
    //
    // Không tra được thư mục phiên ⟹ giữ nguyên như cũ (dựng nút): thà một nút
    // có thể hỏng còn hơn im lặng nuốt mọi nút vì một cuốn sổ chưa kịp ghi.
    let paths = kept_paths(db, cfg, session_id, paths);
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

/// Neo 📎 cho chữ: `(đường dẫn ĐÚNG NHƯ NÓ HIỆN trong chữ, chỉ số trong sổ)`.
///
/// Cùng thứ tự, cùng phép lọc với [`remember_files`] — hai danh sách phải sinh
/// ra từ MỘT lần gọi, nếu không chỉ số lệch và cái liên kết 📎 sẽ mở một tệp
/// khác. Đó là lý do hàm này nhận lại `paths` đã lọc thay vì tự lọc lần nữa.
pub fn file_anchors(
    db: &Db,
    cfg: &Config,
    session_id: &str,
    paths: &[String],
) -> Vec<(String, usize)> {
    kept_paths(db, cfg, session_id, paths)
        .into_iter()
        .take(4)
        .enumerate()
        .map(|(i, p)| (p, i))
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
/// Đường dẫn này có phải một TỆP huba được phép gửi đi không — và ở đâu.
///
/// 🔴 Hà 2026-08-16: *"Rõ ràng trong nội dung có file .html nhưng lại không có
/// nút để tải được về"*. Tệp hôm ấy là
/// `~/projects/AI/tcc/danh-gia-tccbrowser.html` — CÓ THẬT, 28 KB — và chính
/// phiên đã nói vì sao nó nằm ở `tcc/` chứ không trong `tcc/browser/`: *"để
/// không làm bẩn cây git của kho công khai"*. Đặt tệp ra ngoài thư mục phiên là
/// một việc ĐÚNG và thường xuyên; cửa cũ (`starts_with(thư mục phiên)`) đọc nó
/// thành *"một cái tên nhắc trong câu văn"* rồi im lặng bỏ mất cái nút.
///
/// Hàng rào không mất, chỉ lùi ra một tầng — `workspace_root`. Gửi tệp là đưa
/// nội dung RỜI KHỎI MÁY (luật 5), nên `~/.ssh`, `~/Library`, `/etc` vẫn nằm
/// ngoài. Và cửa `is_file` vẫn chặn ca gốc: `com.dipgle.hubd.plist` nhắc giữa
/// câu văn thì không có tệp nào để mà gửi.
///
/// Tách thuần để KIỂM ĐƯỢC, và để hai chỗ dùng chung MỘT luật: lúc dựng nút
/// (`remember_files`) và lúc bấm (`telegram::send_document`). Hai cửa lệch nhau
/// thì nút mọc ra rồi bấm vào báo "nằm ngoài cây làm việc".
pub fn sendable_file(
    p: &str,
    root: &std::path::Path,
    workspace: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let expanded = match p.strip_prefix("~/") {
        Some(rest) => std::env::var("HOME")
            .map(|h| std::path::PathBuf::from(h).join(rest))
            .unwrap_or_else(|_| std::path::PathBuf::from(p)),
        None => std::path::PathBuf::from(p),
    };
    let full = if expanded.is_absolute() {
        expanded.clone()
    } else {
        root.join(&expanded)
    };
    let inside = full.starts_with(root) || full.starts_with(workspace);
    if full.is_file() && inside {
        return Some(full);
    }
    // 🔴 KHÔNG THẤY Ở CHỖ ĐOÁN THÌ ĐI TÌM — Hà 2026-08-17: *"phải tìm được file
    // ở đĩa"*.
    //
    // Ba hình dạng đều có thật trên một màn `/shot`, và cả ba đều trượt phép
    // `root.join(...)`: tên trần (`TODO.md`), đường dẫn bị cửa sổ **bẻ đôi**
    // (`docs/` ở cuối dòng, tên tệp ở dòng sau), và đường tính từ một thư mục
    // con chứ không từ gốc dự án. Phiên thì biết rõ nó làm ở đâu, nên câu hỏi
    // *"tệp này nằm chỗ nào trong cây ấy"* trả lời được bằng một lượt quét.
    if expanded.is_absolute() {
        return None;
    }
    let name = expanded.file_name()?.to_str()?;
    find_one_in_tree(root, name)
}

/// Bỏ qua khi quét: nặng, và không chứa thứ ai đó nhắc tới trong một báo cáo.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".tmp",
    "dist",
    "build",
    ".cargo",
    ".next",
    "vendor",
];

/// Tìm **đúng một** tệp tên `name` trong cây `root`. Nhiều khớp ⟹ `None`.
///
/// Nhiều khớp là ca phải TỪ CHỐI chứ không phải chọn bừa: `docs/README.md` và
/// `web/README.md` là hai tệp khác nhau, và gửi nhầm cái thứ hai thì người đọc
/// không có cách nào biết mình đang đọc sai tệp. Không có nút thì còn thấy được
/// là không có; một nút gửi sai tệp thì im lặng và trông y như đúng.
///
/// Trần độ sâu và trần số mục là hàng rào thời gian: hàm này chạy trong lượt trả
/// lời một cú bấm, nên nó phải kết thúc kể cả khi ai đó trỏ vào một cây khổng lồ.
fn find_one_in_tree(root: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    const MAX_DEPTH: usize = 6;
    const MAX_ENTRIES: usize = 20_000;
    let mut seen = 0usize;
    let mut hit: Option<std::path::PathBuf> = None;
    let mut queue = std::collections::VecDeque::from([(root.to_path_buf(), 0usize)]);
    while let Some((dir, depth)) = queue.pop_front() {
        if depth > MAX_DEPTH || seen > MAX_ENTRIES {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                logging::info(
                    "file_search_capped",
                    json!({ "root": root.display().to_string(), "name": name,
                            "why": "cây quá lớn — dừng quét, không dựng nút" }),
                );
                return None;
            }
            let p = e.path();
            let Some(base) = p.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if p.is_dir() {
                if !base.starts_with('.') && !SKIP_DIRS.contains(&base) {
                    queue.push_back((p, depth + 1));
                }
            } else if base == name {
                if hit.is_some() {
                    // Hai tệp cùng tên: không đoán.
                    logging::info(
                        "file_search_ambiguous",
                        json!({ "name": name, "root": root.display().to_string() }),
                    );
                    return None;
                }
                hit = Some(p);
            }
        }
    }
    hit
}

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

/// Thư mục mà sổ nút đã ghi cho ĐÚNG dòng lệnh này của ĐÚNG phiên này.
///
/// Tra theo nội dung chứ không theo chỉ số, vì đường đi từ cú bấm tới đây là
/// một chuỗi chữ (`/runin <id> <lệnh>`) và thêm một tham số nữa vào đó là đổi
/// hình dạng một mệnh lệnh đang chạy tốt. Sổ chỉ giữ tối đa 4 dòng cho một
/// phiên nên tra theo nội dung là chính xác, không phải xấp xỉ.
pub fn quick_cwd(db: &Db, session_id: &str, line: &str) -> String {
    let Some(v) = db.cursor_or_log(QUICK_KEY) else {
        return String::new();
    };
    let Ok(st) = serde_json::from_str::<serde_json::Value>(&v) else {
        return String::new();
    };
    if st.get("s").and_then(|s| s.as_str()) != Some(session_id) {
        return String::new();
    }
    st.get("c")
        .and_then(|c| c.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|r| serde_json::from_value::<crate::sessions::Cmd>(r.clone()).ok())
                .find(|c| c.line.trim() == line.trim())
                .map(|c| c.cwd)
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// Thư mục để CHẠY một lệnh lấy từ phiên — thứ nhật ký đã ghi, không phải nhãn.
///
/// 🔴 Hà 2026-08-15, ảnh chụp hai nút `▶ git add …`: *"Như này có đúng không"*.
/// Không đúng — `runin_ran code=128`, *"fatal: not a git repository"*. Và khi
/// tôi định vá bằng cách **dò thư mục** (thử gốc dự án, không thấy thì tụt
/// xuống tìm thư mục con nào chứa những đường tương đối), anh chặn đúng lúc:
/// *"Có vẻ chưa đúng ngữ cảnh thật"* · *"nhật ký là sao, sao không đọc trực
/// tiếp trong phiên?"*.
///
/// Anh đúng cả hai vế. Con số thật nằm sẵn trong chính bản ghi mà dòng lệnh
/// được đọc ra: `cwd = "/Users/hanguyen/projects/dwork/dev"` — trong khi
/// `session_root` dựng từ NHÃN (`Mark::d = "dwork"`) chỉ ra `<workspace>/dwork`.
/// Lệch đúng một bậc, và một bậc là đủ để `git` trả 128 — hoặc tệ hơn: một lệnh
/// khác CHẠY THẬT trên những tệp không ai hỏi (bài học `scripts/` 13/08).
///
/// Nên thứ tự là: **thư mục của chính lệnh ấy** → nhãn dự án (đường cũ, cho nút
/// sinh ra trước bản vá) → từ chối. Không có bậc nào là phép đoán.
fn root_for_command(
    db: &Db,
    cfg: &Config,
    session_id: &str,
    cmd_cwd: &str,
) -> Option<std::path::PathBuf> {
    let from_log = std::path::Path::new(cmd_cwd.trim());
    if !cmd_cwd.trim().is_empty() && from_log.is_dir() {
        return Some(from_log.to_path_buf());
    }
    if !cmd_cwd.trim().is_empty() {
        logging::warn(
            "runin_logged_cwd_gone",
            json!({ "session": session_id, "cwd": cmd_cwd,
                    "why": "nhật ký khai thư mục ấy nhưng nay không còn — rơi về gốc dự án" }),
        );
    }
    session_root(db, cfg, session_id)
}

/// Bao nhiêu nút được nhớ. 40 ≈ vài chục tin gần nhất — đủ để một cái nút nằm
/// trên màn hình cả buổi vẫn tra ra đúng việc của nó, mà sổ không phình.
const QUICK_KEEP: usize = 40;

/// Mã của một nút: sinh từ CHÍNH `(phiên, dòng lệnh)`, không từ thứ tự.
///
/// Thứ tự là thứ đổi theo tin mới; cặp `(phiên, lệnh)` thì không. Cùng một lệnh
/// của cùng một phiên luôn ra cùng một mã, nên bấm lại một nút cũ là chạy lại
/// đúng việc ấy chứ không phải một việc khác trùng chỗ.
///
/// FNV-1a 32-bit, in ra 8 chữ số hex: đủ ngắn cho `callback_data` (Telegram cho
/// 64 byte) và đủ thưa cho 40 mục. Đây KHÔNG phải hàm băm bảo mật và không cần
/// là: giá trị nó khoá đã nằm trong sổ của chính huba, mã chỉ để tra.
pub fn quick_token(session_id: &str, line: &str) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for b in session_id
        .as_bytes()
        .iter()
        .chain(b"\n")
        .chain(line.as_bytes())
    {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    format!("{h:08x}")
}

/// Sổ nút hiện tại, đọc được cả hình dạng cũ lẫn mới.
fn quick_book(db: &Db) -> serde_json::Map<String, Value> {
    db.cursor_or_log(QUICK_KEY)
        .and_then(|v| serde_json::from_str::<Value>(&v).ok())
        .and_then(|st| st.get("e").and_then(|e| e.as_object()).cloned())
        .unwrap_or_default()
}

/// Giữ `QUICK_KEEP` mục, và LUÔN giữ những mã vừa ghi.
///
/// Không có thứ tự thời gian trong một `Map`, nên khi phải cắt thì cắt những mã
/// KHÔNG thuộc lượt này — thà bỏ nút cũ còn hơn bỏ nút vừa gửi đi.
fn trim_quick_book(book: &mut serde_json::Map<String, Value>, keep: &[String]) {
    if book.len() <= QUICK_KEEP {
        return;
    }
    let mut drop: Vec<String> = book.keys().filter(|k| !keep.contains(k)).cloned().collect();
    let excess = book.len() - QUICK_KEEP;
    drop.truncate(excess);
    for k in drop {
        book.remove(&k);
    }
}

pub fn remember_quick(
    db: &Db,
    session_id: &str,
    cmds: &[crate::sessions::Cmd],
) -> Vec<(String, String)> {
    if cmds.is_empty() {
        return Vec::new();
    }
    // 🔴 Sổ phải nhớ PHIÊN NÀO đã sinh ra cái nút, không chỉ nhớ dòng lệnh.
    //
    // Hà 2026-08-13: *"Sao bấm nút được tạo phiên này lại gửi vào phiên đang
    // chọn thế"* — và bằng chứng rơi thẳng vào cuộc trò chuyện: một tin của
    // `[tfl5]` mang nút `▶ bash scripts/verify-acl-2026-08-13.sh`, anh bấm, và
    // dòng `!bash scripts/verify-acl-2026-08-13.sh` hiện ra trong phiên `[huba]`
    // — phiên đang được theo. Tệp ấy nằm ở `AI/tfl5/scripts/`, huba không có nó.
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
    // 🔴 …và MỖI NÚT phải tự nhận ra mình. Hà 2026-08-16, ảnh chụp tin của
    // `[social]`: *"bấm các nút lệnh này lại nhảy thành nút lệnh chạy của phiên
    // games phía sau"* · *"nó lại nhận cái cuối cùng trong phiên chat"*.
    //
    // Anh tả đúng cơ chế. Nút chỉ mang **số thứ tự** (`run:0`, `run:1`), mà sổ
    // này có **đúng một ô**: mỗi tin mới ghi đè cả ô. Nên `run:0` không nghĩa là
    // "lệnh đầu của tin ấy" mà là "lệnh đầu của tin GẦN NHẤT" — một cái nút cũ
    // nằm trên màn hình vẫn bấm được, và nó chạy việc của phiên khác.
    //
    // Bản vá 13/08 (nhớ `s` = phiên nào) chữa nửa vấn đề: nó chặn được cảnh gõ
    // vào phiên đang-theo, nhưng `s` cũng chỉ có một ô, nên nút cũ vẫn mượn
    // luôn cả phiên của tin mới. *Một khoá dùng chung thì mọi bản vá đứng trên
    // nó đều chỉ đúng cho hàng cuối cùng.*
    //
    // Nay mỗi lệnh mang một MÃ riêng (`run:<mã>`), sinh từ chính `(phiên,
    // lệnh)`; sổ giữ 40 mã gần nhất. Nút cũ hoặc tra ra đúng việc của nó, hoặc
    // tra không thấy và huba nói thẳng — không còn cửa nào để nó chạy nhầm việc.
    let mut book = quick_book(db);
    let mut tokens: Vec<String> = Vec::new();
    for c in cmds {
        let tok = quick_token(session_id, &c.line);
        book.insert(
            tok.clone(),
            json!({ "s": session_id, "l": c.line, "d": c.cwd }),
        );
        tokens.push(tok);
    }
    trim_quick_book(&mut book, &tokens);
    if let Ok(v) = serde_json::to_string(&json!({ "v": 2, "e": book })) {
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
        .zip(tokens)
        .take(3)
        .flat_map(|(c, tok)| {
            // MỘT lệnh, MỘT nút, và nhãn đúng là lệnh ấy.
            //
            // 🔴 Hà 2026-08-13, ảnh chụp sáu nút dưới một tin: *"sao vẫn ra một
            // đống nút ở đây?"*, rồi nói thẳng cái cần: *"tôi đâu cần thông tin
            // chạy ở đâu làm gì, tôi chỉ cần biết nút đó chạy cái gì và huba phải
            // quản lý được đúng phiên đúng luồng"*.
            //
            // Hai nút mỗi lệnh là bắt người bấm chọn hộ một quyết định KỸ THUẬT
            // (chạy ở đâu) mà họ không có dữ kiện để chọn, và nó nhân đôi chiều
            // dài bảng phím. huba biết đường nào chạy được từ điện thoại
            // (`/runin`) nên huba chọn. Cần cửa sổ thật có tty thì gõ `/win`.
            // 🔴 Hà 2026-08-13: *"Nút chưa chèn vào đúng chỗ của nó"* · *"Bấm
            // vẫn chưa chạy được"*. Đo trong log: ba cú bấm
            // (16:29:39 · 16:30:55 · 16:31:26Z) đều xếp `/runin … ./huba
            // self-install`, và **không cú nào có dòng `runin_ran`** — trong
            // khi bản cài đổi lúc 16:31:37Z, tức lệnh CHẠY XONG. Nó chạy được;
            // thứ không về là lời báo.
            //
            // Gốc: lệnh ấy khởi động lại chính hubad, nên tiến trình đang xử lý
            // lệnh bị thay thế TRƯỚC khi kịp ghi log và gửi tin. Từ điện thoại
            // nhìn y hệt một cái nút hỏng — nên Hà bấm lại, và cài thêm hai
            // lần nữa. Đây là "lỗi im lặng" đúng nghĩa, chỉ khác chỗ: không
            // phải một `Err` bị nuốt, mà là **cái mồm bị giết giữa câu**.
            //
            // Đường đúng đã có sẵn: route `/upgrade` báo TRƯỚC rồi mới restart
            // (`CommandKind::Upgrade`). Nên nút phải trỏ vào đó — đúng nghĩa
            // "chèn vào đúng chỗ của nó".
            if is_self_rebuild(&c.line) {
                return [(
                    format!("🔧 {}", crate::exec::truncate(&c.line, 52)),
                    "upgrade".to_string(),
                )];
            }
            [(
                format!("▶ {}", crate::exec::truncate(&c.line, 52)),
                format!("run:{tok}"),
            )]
        })
        .collect()
}

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
/// Neo này có phải một DÒNG LỆNH không — hỏi lại đúng cái hàng rào đã dựng ra
/// danh sách lệnh, chứ không dựng thêm một phép đoán thứ hai.
///
/// Cùng lý do `sessions::closing_needs_confirm` nằm một chỗ: hai chỗ cùng trả
/// lời một câu hỏi thì tới lúc chúng lệch nhau, không ai biết bên nào sai.
fn is_a_command(anchor: &str) -> bool {
    crate::keys::commands_in_report(anchor.trim(), 1).len() == 1
}

/// Cắt một dòng thành `(trước, phần NEO, sau)` — cùng phép so với
/// [`line_carries`], nên hai hàm không bao giờ trả lời lệch nhau.
///
/// Không tìm thấy ⟹ `(cả dòng, "", "")`, tức không bọc gì. Đó là kết cục đúng:
/// bọc nhầm một khúc chữ vào `<code>` là đổi nghĩa một câu người khác viết.
fn split_at_anchor<'a>(line: &'a str, cmd: &str) -> (&'a str, &'a str, &'a str) {
    let c = cmd.trim();
    if let Some(at) = line.find(c) {
        return (&line[..at], &line[at..at + c.len()], &line[at + c.len()..]);
    }
    // Nhánh khớp-phần-đầu (xem `line_carries`): dòng này mang NỬA ĐẦU của một
    // lệnh bị cửa sổ bẻ. Bọc từ chỗ khớp tới hết dòng — phần còn lại của dòng
    // chính là phần đuôi của lệnh, không phải chữ của ai khác.
    let head: String = c.chars().take(40).collect();
    if head.chars().count() >= 12 {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&head) {
            let at = line.len() - trimmed.len();
            return (&line[..at], &line[at..], "");
        }
    }
    (line, "", "")
}

fn line_carries(line: &str, cmd: &str) -> bool {
    let c = cmd.trim();
    if line.contains(c) {
        return true;
    }
    // Nhánh khớp theo PHẦN ĐẦU: một lệnh dài bị CỬA SỔ bẻ làm đôi thì không
    // dòng nào chứa trọn nó (`tests/telegram.rs::an_icon_still_finds_the_line_
    // the_window_broke_in_two`, sự cố tfl5 14/08). Màn vẫn là nguồn của `/shot`,
    // nên nhánh này còn việc để làm — bản 16/08 của tôi gỡ hẳn nó và bài kiểm ấy
    // đỏ ngay, đúng như nó sinh ra để làm.
    //
    // 🔴 Nhưng phải NEO ĐẦU DÒNG. Không có ràng buộc ấy, nhánh này đi khớp vào
    // giữa một câu văn: Hà 2026-08-16, ảnh chụp `[mailler]` có **một icon ▶️
    // đứng trơ một mình** sau câu *"…đã thử bash -n, cũng bị chặn. Lần chạy của
    // anh sẽ là lần đầu."*. Một dòng lệnh bị bẻ thì nửa của nó BẮT ĐẦU một dòng;
    // một lời nhắc tới lệnh thì nằm lọt giữa câu. Đó là chỗ phân biệt được, và
    // nó không tốn gì.
    //
    // Cắt theo ranh giới ký tự, không theo byte: đường dẫn có dấu tiếng Việt là
    // chuyện có thật trong workspace này.
    let head: String = c.chars().take(40).collect();
    if head.chars().count() < 12 {
        return false;
    }
    if line.trim_start().starts_with(&head) {
        return true;
    }
    // 🔴 DÒNG LỰA CHỌN mở đầu bằng SỐ THỨ TỰ, nên nhãn của nó không đứng ở đầu
    // dòng — Hà 2026-08-24, ảnh bảng năm lựa chọn của `[social]`: *"Bắt kiểu gì
    // mà option cái được cái không"*. Chỉ 4 và 5 có `☑`, đúng hai cái nhãn NGẮN
    // trùng khít cả dòng (`Type something.` · `Chat about this`); ba cái đầu
    // mang thêm một đoạn mô tả nên không dòng nào chứa TRỌN nhãn, và nhánh
    // `starts_with` ở trên thì trượt vì dòng mở đầu bằng `1. `.
    //
    // Bóc phần dẫn rồi hỏi lại — vẫn NEO ĐẦU DÒNG, nên không mở cửa cho phép
    // khớp vào giữa một câu văn (lý do nhánh trên có ràng buộc ấy, xem ca
    // `[mailler]` 16/08).
    let bare = line.trim_start().trim_start_matches(|c: char| {
        matches!(c, '☐' | '☒' | '☑' | '✔' | '❯' | '•' | '*' | '-') || c.is_whitespace()
    });
    // `1. ` · `12. ` — số thứ tự do TUI in ra, không thuộc về nhãn.
    let bare = match bare.find(". ") {
        Some(i) if i > 0 && i <= 2 && bare[..i].chars().all(|c| c.is_ascii_digit()) => {
            &bare[i + 2..]
        }
        _ => bare,
    };
    if bare.starts_with(&head) {
        return true;
    }
    // …và CHIỀU NGƯỢC LẠI, thứ mới là ca của Hà: dòng trên màn ngắn HƠN nhãn.
    //
    // `AskUserQuestion` cho mỗi lựa chọn một `label` và một `description`; màn
    // in tiêu đề ở dòng đầu rồi xuống dòng in mô tả. Nếu neo mang cả hai thì
    // không dòng nào chứa trọn nó, và `head` (40 ký tự) còn DÀI HƠN cả dòng —
    // nên mọi phép `starts_with` theo chiều kia đều trượt.
    //
    // Hỏi ngược: cả dòng (đã bóc số thứ tự) có phải phần MỞ ĐẦU của nhãn
    // không. Vẫn neo đầu, vẫn đòi ≥12 ký tự, nên một câu văn ngắn không tự nhận
    // vơ một lựa chọn.
    let bare = bare.trim_end();
    bare.chars().count() >= 12 && c.starts_with(bare)
}

/// Chèn liên kết chạy vào NGAY SAU dòng lệnh, rồi ghép cả tin thành một chuỗi
/// HTML. Trả `(html, số liên kết đã chèn, chỉ số những lệnh không có liên kết)`.
///
/// 🔴 Hà 2026-08-16, sau khi tôi cãi rằng icon phải nằm cuối một mẩu: *"cái tele
/// nhận được là text dù trước đó là gì thì bạn vẫn phải đọc từng phần, vậy
/// trước khi gửi đã biết từng phần rồi đương nhiên biết luôn khối lệnh nên chèn
/// luôn link vào khối lệnh rồi mới ghép tất cả gửi đi"*.
///
/// 🪦 `command_slices` — bản trước CẮT tin thành nhiều mẩu, mỗi mẩu kết thúc
/// ngay sau một dòng lệnh, để cái icon "rơi đúng chỗ". Nó ra đời hồi icon còn
/// là một NÚT (`inline_keyboard` luôn treo dưới đáy một tin, nên muốn nút nằm
/// giữa chữ thì phải cắt chữ thành nhiều tin) — và nó sống sót qua cả lượt đổi
/// nút thành LIÊN KẾT, dù liên kết thì đặt được vào bất cứ đâu trong chuỗi. Cái
/// giá của việc sống sót ấy: một câu trả lời có 3 lệnh nổ thành 4-5 tin, mẩu
/// chỉ chứa rào ``` gửi ra tin rỗng (`message text is empty`, đo 08-14), và bộ
/// nút phải đi thêm một tin nữa mang mỗi chữ "⤵".
///
/// Nay: một lượt duyệt dòng, dòng nào mang lệnh thì dán liên kết vào cuối chính
/// nó, ghép lại — MỘT tin. Chữ vào đây phải là chữ ĐÃ GỘT markdown, tức đúng
/// chữ Telegram sẽ hiển thị, vì phép định vị dòng lệnh chỉ đúng trên chữ ấy.
///
/// Mỗi lệnh khớp ĐÚNG MỘT LẦN, ở dòng đầu tiên chứa nó: một báo cáo hay nhắc
/// lại cùng một lệnh ở phần tóm tắt, và hai cái icon giống hệt nhau cho cùng
/// một việc là mời người ta bấm hai lần.
pub fn html_with_command_links(
    shown: &str,
    cmds: &[String],
    link_of: &dyn Fn(usize) -> Option<(String, String)>,
) -> (String, usize, Vec<usize>) {
    let anchors: Vec<(String, Vec<(String, String)>)> = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| (c.clone(), link_of(i).into_iter().collect()))
        .collect();
    html_with_links(shown, &anchors)
}

/// Chữ đang nằm trên DÒNG DẤU NHẮC của ảnh màn — neo cho hai nút ⏎/⌫.
///
/// 🔴 Hà 2026-08-16, ảnh chụp lúc 08:01: *"sao lại chèn 2 nút vào cuối thế này,
/// ko hiểu nổi bạn đang làm cái trò gì nữa"* — hai nút dán vào dòng *"Lệnh phiên
/// chạy không được (cổng quyền chặn):"* thay vì dòng `❯ chạy deploy đi`.
///
/// Vì bản đầu hỏi `keys::input_box_text`, mà hàm ấy đọc MỘT MÀN: không thấy
/// khung `╭` thì nó lùi về "4 dòng không rỗng cuối". Ở đây thứ nó nhận không
/// phải một màn — nó là cả TIN, và bốn dòng cuối của tin là chữ huba tự viết
/// thêm. Một hàm đúng, hỏi ở sai chỗ, vẫn ra một câu trả lời sai.
///
/// Nên hỏi bằng dấu hiệu có thật trên ảnh màn: dòng dấu nhắc CUỐI CÙNG còn mang
/// chữ. Khung của `claude` vẽ bằng `───` nên `╭` không có ở đây, nhưng `❯` thì
/// luôn có.
/// `pub` để bài kiểm chạm được: hai cổng trong hàm này là thứ đứng giữa một cú
/// bấm `⏎ Gửi` và một lựa chọn bị xác nhận nhầm, nên chúng phải đo được từ
/// ngoài — chứ không chỉ qua đường vòng của cả một tin đã dựng.
/// Hộp chọn trên màn có thuộc về MỘT CÂU của bảng nhiều câu không — hàm thuần.
///
/// 🔴 Hà 2026-08-18: *"Màn phiên onghut đang chờ chọn với nhiều tab, lấy đó làm
/// chuẩn để test trường hợp nhiều option"*. Lấy đúng màn ấy đo ra một lỗi:
/// bước **Review your answers** (mọi câu đã trả lời, còn mỗi
/// `1. Submit answers · 2. Cancel`) vẫn được dựng nút bằng mã `pick_<sid>_1_<n>`
/// — đường của bảng nhiều câu, thứ gửi *mũi tên rồi số* để đi tới câu số 1.
/// Nhưng ở màn ấy không còn câu nào để đi tới; hai mục kia là hộp chọn ĐƠN của
/// bước xác nhận. Bấm là gửi một dãy phím vào một màn không hiểu nó.
///
/// Gốc: `table` hỏi NHẬT KÝ (*"bảng này có nhiều câu không"*) rồi dùng câu trả
/// lời ấy cho một chuyện khác hẳn (*"màn đang đứng ở đâu"*). Nhật ký trả lời
/// đúng câu của nó, và vẫn sai chỗ này — cùng họ với `is_busy` bị hỏi hộ câu
/// "phiên có đang chạy không" sáng nay.
///
/// Nên hỏi cả hai, mỗi bên một câu: nhật ký nói bảng có nhiều câu, MÀN nói còn ô
/// trống hay không. Không đọc được thanh tab ⟹ giữ nguyên câu trả lời của nhật
/// ký (mù không lật ngược bằng chứng đã có).
pub fn multi_question_screen(has_more_questions: bool, screen: &str) -> bool {
    if !has_more_questions {
        return false;
    }
    match crate::keys::ask_table(screen) {
        // Hết ô trống ⟹ đang ở bước Review/Submit, không còn câu nào để dời tới.
        Some(t) => t.left() > 0,
        None => true,
    }
}

pub fn prompt_line_text(shown: &str) -> Option<String> {
    // 🔴 `❯` MANG HAI NGHĨA — Hà 2026-08-16, ảnh 11:31: hai nút ⏎/⌫ dán vào dòng
    // *"1. Đổi tên file ở gốc repo"*, rồi `/type clear` trả *"đã bấm 'clear'
    // nhưng màn KHÔNG đổi"*.
    //
    // Cả hai là một lỗi: khi màn đang mở HỘP CHỌN, `❯` là con trỏ đang trỏ vào
    // một lựa chọn, KHÔNG phải dấu nhắc ô nhập — và ô nhập lúc ấy trống, nên
    // chẳng có gì để gửi hay xoá. Đây đúng thứ Hà gọi tên: *"mọi dữ liệu từ
    // phiên phải được định nghĩa"* — cùng một ký tự, hai nghĩa, và phải phân
    // biệt bằng ngữ cảnh chứ không bằng hình dạng.
    if crate::keys::has_chooser_footer(shown) {
        return None;
    }
    // 🔴 CỔNG THỨ HAI, và nó phải có vì cổng trên đọc DÒNG CHÂN — một chuỗi do
    // CLI vẽ ra, mà CLI đổi chuỗi ấy lúc nào không ai báo. Đúng chuyện đã xảy
    // ra 2026-08-16: hộp bật auto mode dùng *"Enter to confirm"* thay cho
    // *"Enter to select"*, cổng trên mù, và dòng `❯ 1. Set it up` đi thẳng vào
    // đây thành "chữ trong ô nhập".
    //
    // Cổng này hỏi một câu KHÁC HẲN, nên nó không mù cùng lúc: dòng `❯` ấy có
    // phải chính là một lựa chọn đang hiện trên màn không? `parse_choices` đọc
    // được hộp ấy kể cả khi dòng chân lạ — nên hai cổng hỏng độc lập với nhau.
    // Một cái mù thì cái kia vẫn đứng.
    let choices = crate::keys::parse_choices(shown);
    // 🔴 CHỈ ĐỌC TRONG Ô NHẬP, không quét ngược cả màn — Hà 2026-08-17: *"Text
    // lên hàng đợi rồi vẫn chèn 2 nút vào làm gì"* · *"bắt ký tự ô chát chờ
    // cuối cùng thôi chứ"*.
    //
    // Màn ấy có `❯ làm 8 lỗi walk đi…` nằm trong phần HỘI THOẠI (câu đã gửi,
    // phiên đang chạy nó) và một ô nhập TRỐNG ở cuối. Vòng quét ngược bỏ qua ô
    // trống (dưới 4 ký tự) rồi đi tiếp lên trên, gặp câu cũ và dựng ⏎/⌫ cho nó
    // — tức mời gửi lại một câu vừa gửi xong.
    //
    // `box_region` là khối đóng khung cuối cùng, đúng thứ `still_in_box` đã
    // dùng từ 12/08 cho cùng câu hỏi. Hai hàm hỏi "ô nhập có gì" mà đọc hai
    // vùng khác nhau thì tới lúc chúng lệch, không ai biết bên nào sai.
    let shown = crate::keys::box_region(shown);
    shown.lines().rev().find_map(|l| {
        let t = l.trim();
        let rest = t.strip_prefix('❯').or_else(|| t.strip_prefix('>'))?.trim();
        // `❯ 1. Set it up` — con trỏ hộp chọn, KHÔNG phải ô nhập. So với đúng
        // bảng lựa chọn vừa đọc, không đoán theo hình dạng "có số ở đầu": một
        // câu người ta gõ hoàn toàn có thể mở đầu bằng "1. ".
        let is_cursor_on_a_choice = choices.iter().any(|(n, label)| {
            rest.strip_prefix(&format!("{n}."))
                .map(|r| r.trim() == label.trim())
                .unwrap_or(false)
        });
        if is_cursor_on_a_choice {
            return None;
        }
        // Đủ dài để `line_carries` bám được, và để không khớp nhầm một dấu
        // nhắc trống có một ký tự trang trí.
        (rest.chars().count() >= 4).then(|| rest.to_string())
    })
}

/// Bản tổng quát: mỗi NEO là một chuỗi cần bám, kèm các liên kết dán sau dòng
/// chứa nó. Một dòng có thể mang NHIỀU nút.
///
/// 🔴 Hà 2026-08-16: *"chèn vào đúng chỗ gõ lệnh đó nếu có text 2 nút là được"*
/// · *"1 nút enter 1 nút xóa"*. Ô nhập của phiên hiện ra trong tin `/shot` như
/// một dòng chữ; nút gửi/xoá phải nằm NGAY TẠI dòng ấy, chỗ mắt đang nhìn —
/// không phải dưới đáy tin, nơi không có gì nói chúng thuộc về cái gì.
///
/// Trả `(html, số liên kết đã chèn, chỉ số neo không chèn được)`.
/// Các liên kết của MỘT neo, mượn từ bảng neo — `(href, nhãn)`.
type Links<'a> = Vec<&'a (String, String)>;

/// Chữ bắt đầu bằng `/` trong câu của PHIÊN không phải lệnh của huba — bọc lại.
///
/// 🔴 Hà 2026-08-17: *"`/healthz` bị Telegram tô xanh thành lệnh bot — bấm nhầm
/// là gửi lệnh rác cho huba"*. Telegram tự nhận mọi `/<chữ>` đứng đầu một từ là
/// một lệnh bot và biến nó thành đích chạm: chạm vào là **gửi ngay** chữ ấy cho
/// bot. Nên một dòng `curl …/healthz` của phiên, hay một đường dẫn `/Users/…`,
/// đều mọc ra một cái bẫy — chạm nhầm là huba nhận một lệnh nó không hiểu.
///
/// Vì sao `<code>`: Telegram không tự nối liên kết bên trong nó. Đó không phải
/// suy đoán — cùng cơ chế ấy đã ĐO được ngày 16/08, khi `deploy.sh` với
/// `update.sh` trong một dòng lệnh bị tự biến thành liên kết web (`.sh` là TLD
/// có thật) và cách chữa là bọc `<code>` (xem [`html_with_links`]).
///
/// Lệnh THẬT của huba thì giữ nguyên màu — đó là đích chạm có ích, và bảng route
/// (`commands::lookup`) là chỗ duy nhất biết cái nào thật. Chép tay danh sách ấy
/// ở đây là dựng bản thứ hai sẽ lệch ngay lần thêm route sau.
///
/// 🔴 `@` cũng vậy — Hà 2026-08-17, ảnh `/shot` `[dwork]`: hai dòng
/// `printf '@update-be …'` hiện ra với `@update` **xanh như một mention**. Cùng
/// một cái bẫy, khác ký tự: Telegram tự nhận `@<tên>` là tài khoản và biến nó
/// thành đích chạm dẫn tới một tài khoản không tồn tại. `@update-be` là tên
/// trong THƯ VIỆN LỆNH của dwork (`.runner-commands`), không phải người.
///
/// Đầu vào phải là chữ ĐÃ escape HTML — hàm chỉ thêm thẻ `<code>`, không escape
/// hộ ai, để không có chỗ nào escape hai lần.
pub fn tame_auto_links(escaped: &str) -> String {
    let mut out = String::with_capacity(escaped.len());
    let mut rest = escaped;
    let mut at_word_start = true;
    while let Some(k) = rest.find(['/', '@']) {
        let (before, from_slash) = rest.split_at(k);
        out.push_str(before);
        // Ranh giới đo theo ĐÚNG cái Telegram làm, và hai ký tự có hai luật —
        // ảnh Hà gửi 17/08 cho thấy cả hai trong CÙNG một dòng:
        //   printf '@update-be dci/config/holiday/\n' > ~/projects/…/up.cmd
        // `@update` đứng sau dấu nháy: TÔ XANH. `~/projects` đứng sau `~`, và
        // `holiday/` giữa từ: KHÔNG tô. Nên:
        // · `/` chỉ thành lệnh khi mở đầu một từ (đầu tin hoặc sau khoảng trắng);
        // · `@` thành mention sau BẤT KỲ ký tự nào không phải chữ-số — trừ thư
        //   điện tử, nơi nó đứng ngay sau chữ.
        let prev = before.chars().last();
        let starts_word = match (rest[k..].starts_with('@'), prev) {
            (_, None) => at_word_start,
            (true, Some(c)) => !c.is_alphanumeric() && c != '_' && c != '.' && c != '-',
            (false, Some(c)) => c.is_whitespace() || c == '(' || c == '[',
        };
        let end = from_slash
            .find(char::is_whitespace)
            .unwrap_or(from_slash.len());
        let word = &from_slash[..end];
        // Dấu câu dính đuôi không thuộc về cái lệnh. `;` KHÔNG cắt: chữ đã
        // escape nên `&amp;` kết thúc bằng nó, cắt là vỡ thực thể.
        let core = word.trim_end_matches(['.', ',', ':', ')', ']', '!', '?', '"', '\'']);
        let mark = core.chars().next().unwrap_or(' ');
        let name: String = core
            .chars()
            .skip(1)
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let is_hub_route = mark == '/'
            && !name.is_empty()
            && core.len() == name.len() + 1
            && crate::commands::lookup(&name).is_some();
        // Tên tài khoản Telegram tối thiểu 5 ký tự — ngắn hơn thì nó không nối
        // liên kết, và bọc bừa là biến một chữ thường thành thứ trông như mã.
        let worth_taming = match mark {
            '@' => name.chars().count() >= 5,
            _ => core.chars().count() > 1,
        };
        if starts_word && worth_taming && !is_hub_route {
            out.push_str("<code>");
            out.push_str(core);
            out.push_str("</code>");
            out.push_str(&word[core.len()..]);
        } else {
            out.push_str(word);
        }
        rest = &from_slash[end..];
        at_word_start = false;
    }
    out.push_str(rest);
    out
}

pub fn html_with_links(
    shown: &str,
    anchors: &[(String, Vec<(String, String)>)],
) -> (String, usize, Vec<usize>) {
    html_with_links_last(shown, anchors, &[])
}

/// Như [`html_with_links`], nhưng vài cái neo được khai là **bám lần khớp CUỐI**.
///
/// 🔴 Hà 2026-08-25, ảnh một tin `/shot` có `❯ ssh vps-a "curl -s http://…"`:
/// *"sao ô chờ gợi ý mờ lại không có nút enter"*. Log của chính huba nói ra thủ
/// phạm: `box_anchor_ambiguous {hits: 4}` — chuỗi trong ô nhập trùng với 4 dòng
/// trên màn, vì phiên vừa chạy đúng lệnh ấy nên nó còn nằm trong phần hội thoại
/// phía trên. `session_layout` thấy mập mờ nên bỏ neo, nút rơi xuống đáy tin.
///
/// Nhưng cái mập mờ ấy là BÁO ĐỘNG GIẢ. Ô nhập không phải "một chỗ nào đó trên
/// màn có chuỗi này" — theo đúng định nghĩa của [`prompt_line_text`], nó là
/// **dòng dấu nhắc CUỐI CÙNG còn mang chữ**. Màn cuộn từ trên xuống và ô nhập
/// nằm dưới đáy, nên mọi bản trùng đều ở PHÍA TRÊN nó. Bám lần khớp cuối là
/// bám đúng ô nhập, không cần đoán và không cần bỏ cuộc.
///
/// (Khu chữ huba tự nối thêm nay đặt lên TRƯỚC màn — xem `said_missing_head` —
/// nên nó không đẻ ra bản trùng nào nằm DƯỚI ô nhập.)
///
/// Chỉ áp cho neo được khai trong `neo_cuoi`. Dòng lệnh, lựa chọn, tab, tệp vẫn
/// bám lần đầu như cũ: chúng có thể xuất hiện nhiều lần một cách chính đáng, và
/// "lần đầu" là thứ mắt đọc tới trước.
pub fn html_with_links_last(
    shown: &str,
    anchors: &[(String, Vec<(String, String)>)],
    neo_cuoi: &[usize],
) -> (String, usize, Vec<usize>) {
    // Với mỗi neo "bám cuối", tính sẵn dòng NÀO là lần khớp cuối của nó. Tính
    // một lần ở đây thay vì hỏi lại trong vòng lặp: cùng một câu hỏi hỏi hai
    // lần là hai câu trả lời có cơ hội lệch nhau.
    let dong_cuoi: Vec<Option<usize>> = anchors
        .iter()
        .enumerate()
        .map(|(i, (a, _))| {
            if !neo_cuoi.contains(&i) || a.trim().is_empty() {
                return None;
            }
            shown
                .lines()
                .enumerate()
                .filter(|(_, l)| line_carries(l, a))
                .map(|(n, _)| n)
                .last()
        })
        .collect();
    let mut used = vec![false; anchors.len()];
    let mut html = String::new();
    let mut linked = 0usize;
    let mut unlinked: Vec<usize> = Vec::new();
    for (so_dong, line) in shown.lines().enumerate() {
        let hit = anchors.iter().enumerate().find(|(i, (a, _))| {
            !used[*i]
                && !a.trim().is_empty()
                && line_carries(line, a)
                // Neo "bám cuối" chỉ ăn ĐÚNG dòng cuối của nó; các dòng trùng
                // phía trên đi qua như chữ thường.
                && dong_cuoi[*i].is_none_or(|d| d == so_dong)
        });
        // 🔴 NEO PHẢI NHÌN THẤY ĐƯỢC — Hà 2026-08-16, ảnh chụp tin của
        // `[mailler]` có hai dòng lệnh liền nhau: *"chỗ này tại sao chỉ rend
        // được một lệnh, mà không biết lệnh đó ăn 1 dòng hay cả 2?"*.
        //
        // Một icon ▶️ dán vào cuối một dòng chữ thường thì không nói được nó
        // thuộc về đoạn nào — mắt phải tự đoán ranh giới. Bọc đúng phần LỆNH
        // trong `<code>` là vẽ ra cái ranh giới ấy, ngay tại chỗ.
        //
        // Và nó vá luôn một lỗi thứ hai nhìn thấy trong cùng ảnh: Telegram TỰ
        // biến `deploy.sh` với `update.sh` thành liên kết (`.sh` là một TLD có
        // thật), nên một dòng lệnh hiện ra với hai đường dẫn xanh dẫn ra web.
        // Trong `<code>` thì Telegram không tự nối liên kết nữa.
        //
        // ⚠ CHỈ bọc khi neo là một DÒNG LỆNH. Neo còn có hai loại khác — nhãn
        // lựa chọn và chữ trong ô nhập — và bọc *"Set it up"* vào `<code>` là
        // biến một câu tiếng Anh thành thứ trông như mã. Câu hỏi "đây có phải
        // lệnh không" đã có đúng MỘT chỗ trả lời (`keys::commands_in_report`),
        // nên hỏi lại chính nó, đừng đoán theo hình dạng lần thứ hai.
        // Neo này có phải một DÒNG LỆNH không — quyết định cái bọc khi không
        // dựng được liên kết (`<code>` chỉ dành cho lệnh; bọc *"Set it up"* vào
        // `<code>` là biến một câu tiếng Anh thành thứ trông như mã).
        let anchor_is_cmd = matches!(hit, Some((_, (a, _))) if is_a_command(a));
        // 🔴 …và NEO CHIẾM TRỌN DÒNG thì cũng bọc cả dòng — Hà 2026-08-25:
        // *"chỉnh nốt nút enter chỗ ô chờ gợi ý bao chọn cả text cho dễ bấm"*.
        //
        // Đúng ca của chữ đang nằm trong ô nhập: neo LÀ cả dòng ấy, mà đích
        // chạm thì đang to bằng đúng hai ký tự của `⏎`. Cùng một lỗi với dòng
        // lệnh hôm 23/08, chỉ khác chỗ.
        //
        // Điều kiện hẹp có chủ ý — neo phải bằng ĐÚNG cả dòng: một đường dẫn
        // nằm giữa câu (nút 📎) hay một nhãn lựa chọn nằm sau số thứ tự thì
        // không khớp, nên chúng giữ nguyên hình dạng cũ. Nới ra là đổi hình
        // dạng của những chỗ chưa ai hỏi.
        // Dấu nhắc của TUI (`❯ `, `$ `, `> `…) đứng TRƯỚC chữ, nên "cả dòng"
        // phải hiểu là "cả dòng sau khi bóc dấu nhắc" — bài kiểm bắt đúng chỗ
        // này ngay lượt đầu. Phần bóc ra vẫn nằm ngoài thẻ `<a>`, vì
        // `split_at_anchor` giao nó lại làm `head`.
        let bare_line = line
            .trim()
            .trim_start_matches(|c: char| {
                matches!(c, '❯' | '$' | '>' | '⏵' | '%' | '•' | '*' | '☐' | '☑' | '☒')
                    || c.is_whitespace()
            })
            .trim();
        let anchor_is_whole_line = matches!(
            hit,
            Some((_, (a, _))) if a.trim() == bare_line && a.trim().chars().count() >= 4
        );
        // 🔴 …VÀ ĐƯỜNG DẪN TỆP NẰM GIỮA CÂU — Hà 2026-08-25, ảnh một tin có hai
        // lần `FEATURE-GAPS.md`: *"icon tải tệp gắn không đúng chỗ trong nội
        // dung tin vậy, và cũng chưa bao text đường dẫn file"*.
        //
        // Hai lời ấy là MỘT gốc, không phải hai lỗi. Đường dẫn giữa câu không
        // phải lệnh (`is_a_command` đúng khi nói không) và không chiếm trọn
        // dòng, nên nó rơi vào nhánh `_ => (line, "", "")`: `cmd_part` rỗng ⟹
        // không thẻ `<a>` nào bọc lấy tên tệp ⟹ cái 📎 bị đẩy xuống danh sách
        // `after`, tức dán vào **cuối dòng nguồn**. Trên màn 390px Telegram bẻ
        // lại đoạn văn, nên "cuối dòng nguồn" hiện ra giữa câu — đúng chỗ Hà
        // thấy nó đứng sau *"thuộc về"* và sau *"commit"*.
        //
        // Và nó kéo theo cái thứ ba không ai thấy: `tame_auto_links` chỉ soi
        // `/` với `@` (xem hàm ấy), nên một tên tệp trần đi qua nguyên vẹn và
        // **Telegram tự nối liên kết** — `.md` là TLD thật của Moldova, y hệt
        // con bug `.sh` ngày 16/08. Chữ xanh trong ảnh không phải nút tải của
        // huba; nó trỏ ra một tên miền ngoài. Bọc `<a>` chặn sẵn cả ca ấy, nên
        // một phép vá đóng cả ba triệu chứng.
        //
        // Đây chính là ca lượt trước CỐ Ý để lại (*"một đường dẫn nằm giữa câu
        // (nút 📎) … giữ nguyên hình dạng cũ. Nới ra là đổi hình dạng của những
        // chỗ chưa ai hỏi"*). Nay đã có người hỏi, nên nới đúng một ca ấy.
        //
        // Hỏi bằng CHÍNH cái đã đặt neo xuống (`session_layout` gắn icon `📎`
        // cho mọi neo lấy từ `data.files`), không tự dựng lại phép "chuỗi này có
        // phải đường dẫn không" — hai bản chép của cùng một câu hỏi là hai chỉ
        // số lệch nhau, đúng thứ `remember_files`/`file_anchors` đã trả giá.
        let anchor_is_file =
            matches!(hit, Some((_, (_, links))) if links.iter().any(|(_, i)| i.trim() == "📎"));
        let (head, cmd_part, tail) = match hit {
            Some((_, (a, _))) if anchor_is_cmd || anchor_is_whole_line || anchor_is_file => {
                split_at_anchor(line, a)
            }
            _ => (line, "", ""),
        };
        // 🔴 Icon mở đầu bằng TAB ⟹ chèn TRƯỚC dòng, không phải sau — Hà
        // 2026-08-17: *"Chèn phía trước số mỗi dòng"*.
        //
        // Với lựa chọn thì đúng chỗ của ☑ là đầu dòng, ngay trước `1.`: mắt
        // chạy dọc cột số để chọn, nên đích chạm phải nằm trên cùng cột ấy. Dán
        // vào cuối nhãn thì mỗi dòng một chỗ khác nhau, và với nhãn dài thì nó
        // rơi tận cuối câu.
        //
        // Cùng quy ước với `\n` (xuống hẳn một dòng) đã có từ 16/08: một ký tự
        // điều khiển ở đầu nhãn nói VỊ TRÍ, không phải nội dung.
        let (before, mut after): (Links, Links) = match hit {
            Some((_, (_, links))) => links.iter().partition(|(_, i)| i.starts_with('\t')),
            None => (Vec::new(), Vec::new()),
        };
        // 🔴 CẢ DÒNG LỆNH LÀ ĐÍCH CHẠM, KHÔNG PHẢI MỖI CÁI EMOJI — Hà
        // 2026-08-23: *"Tại sao các nút chạy khối lệnh không bao cả khối như
        // cách làm ở danh sách phiên cho dễ bấm"*.
        //
        // Anh chỉ đúng: hàng phiên đã bọc cả hàng vào `<a>` hôm qua
        // ([`tap_rows_html`]), còn dòng lệnh vẫn để đích chạm to đúng bằng hai
        // ký tự của cái emoji. Cùng một lỗi, vá một chỗ, sót chỗ bên cạnh.
        //
        // Vì sao KHÔNG bọc `<a>` ra ngoài `<code>` — cách hiển nhiên nhất:
        // Telegram NUỐT cái link. Đo thật 2026-08-23 06:10Z
        // (`tests/cmd_block_tap_live.rs`): gửi `<a href="…"><code>lệnh</code></a>`
        // thì `entities` trả về đúng một `code`, không `text_link` nào phủ nó.
        // Khối trông y hệt mà bấm không ăn — một lỗi im tiếng, đúng loại phải
        // đo mới thấy vì hai kết cục nhìn từ ngoài giống hệt nhau.
        //
        // Nên `<a>` THAY `<code>`, không bọc ngoài nó. Đo cùng lượt: link phủ
        // trọn 20 ký tự dòng lệnh, và `gate.sh` **không** bị Telegram tự nối
        // thành liên kết web dù đã bỏ `<code>` — thẻ `<a>` bọc ngoài chặn sẵn,
        // nên con bug 16/08 (`.sh` là TLD thật) không quay lại.
        //
        // Icon đi VÀO TRONG thẻ, đúng bài học của `tap_rows_html`: nó là dấu
        // hiệu "chạm được", nên chính nó phải chạm được — để ngoài là dựng lại
        // đúng cái đích tí xíu vừa bỏ.
        //
        // ⚠ Chỉ đổi khi THẬT có link. Không dựng được liên kết (chưa biết tên
        // bot) thì `<code>` ở lại — nó vẫn đang giữ hai việc của lượt 16/08: vẽ
        // ranh giới "lệnh ăn 1 dòng hay 2", và chặn tự-nối-liên-kết. Bỏ nó ở
        // nhánh không có link là đánh đổi lấy không gì cả.
        // Hai vế hỏi hai chuyện khác nhau — *không có chữ để bọc* và *không có
        // liên kết để bọc bằng* — nhưng cùng ra một câu trả lời, nên gộp
        // (clippy `if_same_then_else`). Tách ra chỉ nói được điều mà hai chữ
        // trong điều kiện đã nói.
        let cmd_link = if cmd_part.is_empty() || after.is_empty() {
            None
        } else {
            Some(after.remove(0))
        };
        for (href, icon) in &before {
            html.push_str(&format!(
                "<a href=\"{}\">{}</a> ",
                crate::telegram::html_escape(href),
                icon.trim_start_matches('\t')
            ));
            linked += 1;
        }
        html.push_str(&tame_auto_links(&crate::telegram::html_escape(head)));
        if !cmd_part.is_empty() {
            match &cmd_link {
                Some((href, icon)) => {
                    html.push_str(&format!(
                        "<a href=\"{}\">{} {}</a>",
                        crate::telegram::html_escape(href),
                        icon.trim_start_matches(['\t', '\n']),
                        crate::telegram::html_escape(cmd_part)
                    ));
                    linked += 1;
                }
                // Không dựng được liên kết: LỆNH thì giữ `<code>` (ranh giới +
                // chặn tự-nối-liên-kết), còn chữ thường thì để nguyên là chữ.
                None if anchor_is_cmd => html.push_str(&format!(
                    "<code>{}</code>",
                    crate::telegram::html_escape(cmd_part)
                )),
                None => html.push_str(&tame_auto_links(&crate::telegram::html_escape(cmd_part))),
            }
        }
        html.push_str(&tame_auto_links(&crate::telegram::html_escape(tail)));
        if let Some((i, (_, links))) = hit {
            used[i] = true;
            if links.is_empty() {
                // Không dựng được liên kết (chưa biết tên bot) ⟹ neo ấy phải
                // rơi xuống một cái nút ở đáy. Nói ra, đừng đánh rơi im lặng:
                // một dòng lệnh không có đường bấm là một cây cầu hụt nhịp.
                unlinked.push(i);
            }
            for (href, icon) in &after {
                // Nhãn mở đầu bằng xuống dòng ⟹ nút ấy xuống HẲN một dòng.
                //
                // 🔴 Hà 2026-08-16: *"2 nút enter và clear gần nhau quá dễ bấm
                // nhầm"*. Trên màn điện thoại hai icon cách nhau một dấu cách là
                // hai đích chạm chồng lên nhau — mà một bên GỬI còn bên kia
                // XOÁ, tức bấm nhầm là mất chữ vừa gõ hoặc gửi thứ chưa định
                // gửi. Cả hai đều không lùi lại được.
                let (sep, label) = match icon.strip_prefix('\n') {
                    Some(rest) => ("\n", rest),
                    None => (" ", icon.as_str()),
                };
                html.push_str(&format!(
                    "{sep}<a href=\"{}\">{}</a>",
                    crate::telegram::html_escape(href),
                    label
                ));
                linked += 1;
            }
        }
        html.push('\n');
    }
    // 🔴 NEO KHÔNG BÁM ĐƯỢC DÒNG NÀO THÌ PHẢI NÓI RA.
    //
    // Bản trước chỉ báo `unlinked` cho neo ĐÃ bám mà không dựng nổi liên kết.
    // Neo không tìm thấy dòng nào thì rơi hoàn toàn im lặng — và đó đúng là
    // hình dạng của ca Hà chụp 24/08 (`☑` mất ở ba lựa chọn đầu, không một dòng
    // log nào). Luật 3 của dự án cấm đúng chuyện này.
    //
    // ⚠ CHỈ GHI LOG, KHÔNG đụng vào `unlinked`. Cuốn ấy có hợp đồng riêng —
    // *neo ĐÃ khớp một dòng mà không dựng nổi liên kết* — và hai bài kiểm khoá
    // đúng nghĩa ấy (`command_anchors::prose_that_merely_mentions…` và
    // `…falls_to_a_bottom_button`). Nhét thêm nghĩa thứ hai vào một cuốn sổ là
    // cách chắc chắn để hai chỗ đọc nó hiểu khác nhau; chỗ gọi tự lo đường lùi.
    for (i, (a, _)) in anchors.iter().enumerate() {
        if !used[i] && !a.trim().is_empty() {
            logging::warn(
                "anchor_found_no_line",
                json!({ "anchor": crate::exec::truncate(a, 80),
                        "effect": "không chèn được vào chữ — neo này không thành đích chạm" }),
            );
        }
    }
    (html, linked, unlinked)
}

/// Trần chữ cho MỘT tin Telegram. Luật của Telegram là 4096; chừa lại chỗ cho
/// cái icon và thẻ `<a>` bọc nó.
const TG_TEXT_MAX: usize = 3500;

/// Bao nhiêu ký tự của một dòng lệnh được HIỆN ở khu "lấy từ nhật ký".
///
/// Không phải trần của thứ được CHẠY — nút tra dòng gốc trong sổ. 90 là cỡ hai
/// hàng trên màn 390px: đủ để nhận ra lệnh nào, chưa đủ để thành bức tường.
const SHOWN_CMD_MAX: usize = 90;

/// Trần AN TOÀN cho một lệnh chạy nền: một giờ.
///
/// Không phải "thời gian một lệnh được phép chạy" — đó là câu hỏi không ai trả
/// lời đúng được từ trước, và trần 120 giây cũ chính là một câu trả lời sai.
/// Đây là cái phanh cuối: một tiến trình treo một tiếng thì nó treo thật, và bỏ
/// nó chạy tới sáng là bỏ lại một thứ đang giữ tài nguyên mà không ai nhớ.
const LONG_JOB_MAX_SEC: u64 = 3600;

/// Bao lâu thì nhắc một lần rằng lệnh vẫn đang chạy.
const LONG_JOB_TICK_SEC: u64 = 90;

/// Nhịp hỏi Terminal xem cửa sổ ấy đã chạy xong chưa (nút 🖥).
///
/// Hỏi bằng `busy of tab` — chính Terminal trả lời về tab của nó — chứ không
/// đoán bằng `ps` hay `sleep`: `ps` mất trước khi shell kịp in dấu nhắc, và một
/// `sleep` cố định thì hoặc cắt ngang một lệnh dài, hoặc bắt chờ vô cớ.
const TERM_JOB_POLL_SEC: u64 = 3;

/// Bao nhiêu dòng cuối của cửa sổ được lấy làm "kết quả" gửi về Telegram.
const TERM_JOB_TAIL_LINES: usize = 60;

/// 🖥 Chạy trong CỬA SỔ TERMINAL riêng, rồi **kết quả về Telegram**.
///
/// 🔴 Hà 2026-08-16: *"lệnh chạy phải có 2 nút: 1 là chạy xong lấy kết quả đưa
/// vào phiên, 1 nút là chạy terminal được kết quả gửi về tele"*. Hai nút ấy
/// khác nhau ở ĐÍCH ĐẾN của kết quả, không phải ở chỗ chạy — và trước lượt này
/// nút 🖥 làm đúng nửa việc: mở cửa sổ, gõ lệnh, rồi bỏ đó. Kết quả nằm lại
/// trên một màn hình mà người đang cầm điện thoại không nhìn thấy, tức cái nút
/// chỉ dùng được khi chủ máy đang ngồi trước máy — đúng lúc anh không cần huba.
///
/// Không giữ `Db` nên tin trả về đi qua cửa định dạng với bảng dữ liệu rỗng:
/// chữ hiện GIỐNG mọi tin khác, chỉ không mang nút của phiên nào (cửa sổ trần
/// không thuộc phiên nào — đó là định nghĩa của nó).
fn watch_terminal_job(w: i64, tty: String, line: String) {
    // Bản sao cho nhánh "không dựng được luồng": luồng nuốt bản gốc, mà đúng ca
    // ấy mới cần tên cửa sổ để nói ra (cùng hình dạng với `watch_long_job`).
    let fallback_tty = tty.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("term-job-{tty}"))
        .spawn(move || {
            let _lane = crate::exec::urgent();
            // Gõ xong, Terminal cần một nhịp mới báo `busy`. Hỏi ngay lập tức
            // thì đọc được trạng thái TRƯỚC lệnh và kết luận "xong" cho một
            // lệnh còn chưa bắt đầu.
            std::thread::sleep(std::time::Duration::from_secs(2));
            let started = std::time::Instant::now();
            loop {
                match crate::keys::tab_state(w) {
                    Ok(crate::keys::TabState::Idle) => break,
                    Ok(crate::keys::TabState::Busy) => {}
                    // Cửa sổ đóng giữa chừng: kết quả đi theo nó. Nói ĐÚNG chừng
                    // ấy — "cửa sổ không còn" là thứ đo được, còn "lệnh chạy tới
                    // đâu" thì không, và đoán hộ ở đây là bịa.
                    Ok(crate::keys::TabState::Gone) => {
                        logging::warn(
                            "term_job_window_gone",
                            json!({ "tty": tty, "sec": started.elapsed().as_secs(),
                                    "cmd": crate::exec::truncate(&line, 120) }),
                        );
                        say_term_result(&format!(
                            "🖥 cửa sổ {tty} đã đóng khi lệnh còn đang chạy — huba không đọc được kết quả, và không biết lệnh chạy tới đâu.\n$ {line}"
                        ));
                        return;
                    }
                    // Terminal câm (quyền bị rút, osascript chết): NÓI RA, đừng
                    // im — im ở đây là một cú bấm không bao giờ có câu trả lời.
                    Err(e) => {
                        let why = crate::logging::err_chain(&e);
                        logging::warn(
                            "term_job_watch_failed",
                            json!({ "tty": tty, "err": why, "cmd": crate::exec::truncate(&line, 120) }),
                        );
                        say_term_result(&format!(
                            "🖥 mất dấu cửa sổ {tty} khi đang chờ lệnh chạy xong ({why}).\n$ {line}"
                        ));
                        return;
                    }
                }
                if started.elapsed().as_secs() >= LONG_JOB_MAX_SEC {
                    logging::warn(
                        "term_job_still_running",
                        json!({ "tty": tty, "sec": started.elapsed().as_secs() }),
                    );
                    say_term_result(&format!(
                        "🖥 vẫn đang chạy sau {} phút trong cửa sổ {tty} — huba thôi canh, cửa sổ vẫn còn đó.\n$ {line}",
                        started.elapsed().as_secs() / 60
                    ));
                    return;
                }
                std::thread::sleep(std::time::Duration::from_secs(TERM_JOB_POLL_SEC));
            }
            // Đọc màn CỦA CHÍNH cửa sổ ấy, rồi cắt từ dòng lệnh trở xuống: phần
            // trên nó là những gì có sẵn trước khi huba gõ vào, không phải kết
            // quả của lệnh này.
            let body = match crate::keys::screen_of(&tty, TERM_JOB_TAIL_LINES) {
                Some((body, _)) => body,
                None => {
                    logging::warn("term_job_screen_unreadable", json!({ "tty": tty }));
                    say_term_result(&format!(
                        "🖥 lệnh chạy xong trong cửa sổ {tty} nhưng huba KHÔNG đọc được màn của nó.\n$ {line}"
                    ));
                    return;
                }
            };
            let out = tail_after_command(&body, &line);
            logging::info(
                "term_job_done",
                json!({ "tty": tty, "sec": started.elapsed().as_secs(),
                        "cmd": crate::exec::truncate(&line, 120), "out_len": out.len() }),
            );
            // 🔴 CHẠY XONG THÌ DỌN CỬA SỔ ĐI — Hà 2026-08-23, ảnh một cửa sổ
            // `ttys001` nằm lại sau lượt `git push` đã báo kết quả xong: *"tại
            // sao chạy lệnh ở cửa sổ xong lại không tự tắt đi"*.
            //
            // Cửa sổ này sinh ra để chạy ĐÚNG MỘT lệnh, và kết quả của nó vừa
            // đi ra Telegram. Để lại là để lại rác: mỗi nút 🖥 một cửa sổ, và
            // chính danh sách `/terminal` sẽ dài ra vì thứ huba tự bày.
            //
            // Đóng ở ĐÂY, sau khi đã ĐỌC màn: đóng trước thì kết quả đi theo
            // cửa sổ. Và chỉ tới được dòng này khi vòng lặp trên đã thấy tab
            // `Idle` — tức không còn tiến trình nào chạy, đúng điều kiện
            // `close_window` đòi (hộp thoại *"terminate running processes?"* của
            // Terminal chặn MỌI lệnh tự động sau nó, xem `CLAUDE.md` §13).
            let dong = match crate::keys::exit_and_close_shell(
                w,
                std::time::Duration::from_secs(10),
            ) {
                Ok(crate::keys::Closed::Gone) => " · đã đóng cửa sổ".to_string(),
                Ok(crate::keys::Closed::Hidden) => {
                    // Ẩn KHÔNG phải đóng, và nói dối chỗ này thì lần sau người
                    // ta đi tìm một cửa sổ không còn trong tầm mắt.
                    " · cửa sổ ẨN chứ chưa đóng được (⌘W khi ngồi máy)".to_string()
                }
                Err(e) => {
                    logging::warn(
                        "term_job_close_failed",
                        json!({ "tty": tty, "err": crate::logging::err_chain(&e) }),
                    );
                    " · KHÔNG đóng được cửa sổ, nó vẫn còn đó".to_string()
                }
            };
            say_term_result(&format!(
                "🖥 xong trong cửa sổ {tty} ({} giây){dong}:\n$ {}\n{}",
                started.elapsed().as_secs(),
                line,
                crate::exec::truncate(&out, CMD_OUT_MAX)
            ));
        });
    if let Err(e) = spawned {
        // Không dựng được luồng canh thì lệnh VẪN CHẠY trong cửa sổ — nói đúng
        // như vậy, đừng để người ta ngồi chờ một tin không bao giờ tới.
        logging::error("term_job_spawn_failed", json!({ "err": e.to_string() }));
        say_term_result(&format!(
            "⚠ lệnh đang chạy trong cửa sổ {fallback_tty} nhưng huba KHÔNG canh được để báo kết quả — xem trong cửa sổ ấy."
        ));
    }
}

/// Phần màn SAU dòng lệnh vừa gõ — tức kết quả của chính nó.
///
/// Không thấy dòng lệnh (màn đã cuộn qua, lệnh quá dài bị bẻ đôi) thì trả cả
/// khúc đang có: thà thừa vài dòng ngữ cảnh còn hơn trả về chuỗi rỗng và để
/// người đọc tưởng lệnh không in ra gì.
pub fn tail_after_command(screen: &str, line: &str) -> String {
    let needle = line.trim();
    let mut lines: Vec<&str> = screen.lines().collect();
    if let Some(i) = lines.iter().rposition(|l| l.contains(needle)) {
        lines = lines.split_off(i + 1);
    }
    lines.join("\n").trim().to_string()
}

/// Tin của nút 🖥 — cùng cửa định dạng với mọi tin khác.
fn say_term_result(text: &str) {
    match crate::telegram::inbox() {
        Some(tg) => say_session_data(
            tg,
            text,
            &[],
            "term_job_ack_failed",
            &SessionData::default(),
        ),
        None => logging::info("term_job_ack_dropped", json!({ "ack": text })),
    }
}

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

/// Bóc `cd <thư mục>` + `&&` hoặc `;` ở đầu.
///
/// Trả `(tiền tố giữ nguyên, phần lệnh thật)`.
fn boc_cd(line: &str) -> (String, &str) {
    let t = line.trim_start();
    let Some(rest) = t.strip_prefix("cd ") else {
        return (String::new(), t);
    };
    if let Some((dir, tail)) = rest.split_once("&&") {
        return (format!("cd {dir}&&"), tail.trim_start());
    }
    if let Some((dir, tail)) = rest.split_once(';') {
        return (format!("cd {dir};"), tail.trim_start());
    }
    (String::new(), t)
}

/// Cờ của `ssh` có ăn theo một giá trị đứng sau — phải nhảy qua cả hai để không
/// đọc nhầm giá trị ấy thành tên host.
const SSH_CO_GIA_TRI: &[&str] = &[
    "-p", "-i", "-o", "-l", "-F", "-J", "-b", "-c", "-E", "-L", "-R", "-D", "-W", "-e", "-m",
];

/// Kế hoạch bơm mật khẩu `sudo` qua **stdin** cho một dòng lệnh.
///
/// `Some((host, dòng ĐEM CHẠY))` — `host` rỗng là máy này. `None` = dòng này
/// không cần mật khẩu, và khi ấy chỗ gọi KHÔNG được đụng gì vào nó.
///
/// 🔴 Hà 2026-08-25: *"trường hợp chạy ssh xong có yc mật khẩu thì với lệnh chạy
/// từ tele sẽ làm thế nào?"*; rồi 26/08 đặt tên khoá `HUB_VPS_A_SUDO_PASSWORD`
/// — chính cái tên ấy lộ ra rằng ca anh cần là `sudo` **ở đầu kia của ssh**,
/// trong khi bản đầu của tôi chỉ phủ `sudo` cục bộ và còn có một bài kiểm khẳng
/// định ca của anh BỊ LOẠI. Đọc hụt câu hỏi, không phải mã sai.
///
/// Vì sao ca xa lại giải được: `ssh` từ chối đọc mật khẩu **của chính nó** từ
/// stdin, nhưng nó **chuyển tiếp stdin cho lệnh chạy ở đầu kia**. Nên
/// `ssh vps-a "sudo -S -p '' …"` thì `sudo` trên vps-a đọc được — không cần PTY,
/// không cần đoán lời nhắc, không cần `sshpass`.
///
/// Đo được trước khi vá: `hubad` chạy tty `??`, và tiến trình không có terminal
/// điều khiển thì mở `/dev/tty` ra `[Errno 6]`. Mà đó đúng là chỗ `sudo` mở để
/// hỏi. Nên nút ▶️ gặp `sudo` là hỏng ngay — không treo, nhưng vẫn là việc ngồi
/// ở máy làm được mà từ xa thì không.
///
/// 🔴 CỔNG HẸP CÓ CHỦ Ý — `sudo` phải là lệnh ĐẦU TIÊN (cục bộ), hoặc lệnh đầu
/// tiên của phần chạy ở xa. `cat /etc/hosts && sudo reboot` thì KHÔNG: bơm mật
/// khẩu vào stdin của chuỗi ấy là đưa nó cho `cat` đọc trước. Hẹp thì cùng lắm
/// mất một ca; rộng thì mất bí mật, và hai hướng hỏng ấy không cân nhau.
pub fn sudo_stdin_plan(line: &str) -> Option<(String, String)> {
    let (dau, than) = boc_cd(line);

    // ① `sudo` ngay trên máy này.
    if let Some(sau) = la_sudo(than) {
        return Some((String::new(), format!("{dau}sudo -S -p '' {sau}")));
    }

    // ② `sudo` ở ĐẦU KIA của một lệnh `ssh`.
    let rest = than.strip_prefix("ssh ")?;
    let b = rest.as_bytes();
    let mut i = 0usize;
    let mut host: Option<&str> = None;
    while i < rest.len() {
        while i < rest.len() && b[i] == b' ' {
            i += 1;
        }
        if i >= rest.len() {
            break;
        }
        let dau_tok = i;
        while i < rest.len() && b[i] != b' ' {
            i += 1;
        }
        let tok = &rest[dau_tok..i];
        if tok.starts_with('-') {
            if SSH_CO_GIA_TRI.contains(&tok) {
                while i < rest.len() && b[i] == b' ' {
                    i += 1;
                }
                while i < rest.len() && b[i] != b' ' {
                    i += 1;
                }
            }
            continue;
        }
        host = Some(tok);
        break;
    }
    let host = host?;
    let truoc = rest[..i].trim_end();
    let lenh_xa = rest[i..].trim();

    // Lệnh chạy ở xa thường nằm trong một cặp nháy — viết lại phần BÊN TRONG,
    // giữ nguyên cặp nháy, vì bỏ nó đi là đổi cách shell ở đầu kia tách tham số.
    let q = lenh_xa.chars().next().filter(|c| *c == '"' || *c == '\'');
    let trong = match q {
        Some(c) if lenh_xa.len() >= 2 && lenh_xa.ends_with(c) => &lenh_xa[1..lenh_xa.len() - 1],
        _ => lenh_xa,
    };
    let sau = la_sudo(trong)?;
    let nhay = q.map(String::from).unwrap_or_default();
    Some((
        host.to_string(),
        format!("{dau}ssh {truoc} {nhay}sudo -S -p '' {sau}{nhay}"),
    ))
}

/// Phần đứng sau `sudo` nếu chuỗi này MỞ ĐẦU bằng đúng lệnh `sudo`.
///
/// Tách ra để cả hai nhánh (cục bộ và ở xa) hỏi CÙNG một câu — hai bản chép của
/// cùng phép so chuỗi là hai câu trả lời có cơ hội lệch nhau.
fn la_sudo(t: &str) -> Option<&str> {
    let t = t.trim();
    let sau = t.strip_prefix("sudo")?;
    if sau.is_empty() || sau.starts_with(' ') {
        Some(sau.trim_start())
    } else {
        None
    }
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
    // Lượt này do PHIÊN tự nhờ (hòm thư) chứ không do chủ máy bấm ⟹ kết quả
    // vào phiên là đủ, đừng dội thêm một tin ra Telegram.
    //
    // 🔴 Hà 2026-08-25: *"Sao cứ có tin nhắn này ✅ Đã chạy trên máy rồi dán
    // kết quả vào [dwork/A-DDOC]…"*. Đo cùng lúc: **21 lượt hòm thư trong một
    // buổi**, mỗi lượt một tin. Phiên A-DDOC đang DÙNG tính năng ấy đúng như
    // thiết kế — nó nhờ chạy, nó đọc kết quả — nhưng Hà thì không hỏi gì mà
    // vẫn nhận đủ 21 tin.
    //
    // Cửa `quiet` đã có sẵn cho việc này (`push_text_quiet` → `Incoming.quiet`
    // → `reply_in_channel` im), nhưng đường `/runin` trả lời bằng `say_back`,
    // và `say_back` KHÔNG hỏi `quiet` — nên cờ ấy chưa từng với tới đây.
    quiet: bool,
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
            // 🔴 MẬT KHẨU `sudo` ĐI BẰNG STDIN, KHÔNG BAO GIỜ QUA `argv`.
            //
            // huba HIỆN dòng lệnh ra Telegram, đặt nó làm nhãn nút và ghi vào sổ
            // (`remember_quick`). Một mật khẩu trong `argv` vì thế không chỉ rời
            // khỏi máy — nó rời khỏi máy KÈM CẢ CÁCH DÙNG, và nằm lại vĩnh viễn
            // trong lịch sử buồng chat. `RunOpts.input` bơm thẳng vào stdin của
            // tiến trình con (`exec.rs`), nên nó không đi qua chuỗi nào cả.
            //
            // Không khai `sudo_password_env`, hoặc biến ấy rỗng ⟹ KHÔNG bơm gì
            // và cũng KHÔNG viết lại dòng lệnh: lệnh chạy y như hôm qua và hỏng
            // y như hôm qua, có thông báo. Tắt phải là tắt hẳn, không phải một
            // nửa đường mới.
            // Tra bảng theo HOST. Không khớp host nào ⟹ không bơm gì và cũng
            // KHÔNG viết lại dòng lệnh: lệnh chạy y như hôm qua và hỏng y như
            // hôm qua, có thông báo. Tắt phải là tắt hẳn, không phải nửa đường.
            //
            // `user@host` thì thử cả chuỗi đầy đủ TRƯỚC, rồi mới tới phần sau
            // `@`: một máy có thể cần mật khẩu khác nhau cho hai tài khoản, và
            // đoán gộp là đưa mật khẩu của người này cho phiên của người kia.
            let ke_hoach = sudo_stdin_plan(&line);
            let mat_khau = ke_hoach.as_ref().and_then(|(host, _)| {
                let ten = cfg.sudo_password_env.get(host.as_str()).or_else(|| {
                    host.split_once('@')
                        .and_then(|(_, h)| cfg.sudo_password_env.get(h))
                })?;
                let co = crate::config::secret_from_env(ten.trim());
                logging::info(
                    "sudo_stdin_gate",
                    // TÊN khoá và TÊN host — không bao giờ giá trị (luật 4).
                    json!({ "host": host, "env": ten.trim(), "co_gia_tri": co.is_some() }),
                );
                co
            });
            let chay = match (&ke_hoach, &mat_khau) {
                (Some((_, viet_lai)), Some(_)) => viet_lai.clone(),
                _ => line.clone(),
            };
            let out = crate::exec::run(
                "/bin/zsh",
                &["-lc", &chay],
                crate::exec::RunOpts {
                    cwd: Some(root.as_path()),
                    timeout: Some(std::time::Duration::from_secs(LONG_JOB_MAX_SEC)),
                    pid_out: Some(tx),
                    input: mat_khau.map(|p| format!("{p}\n")),
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
                    // 🔴 Khối dán vào phiên phải NGẮN NHẤT có thể — Hà
                    // 2026-08-16: *"tại sao lại có một mớ text không cần thiết
                    // này"* · *"quá tốn context"*.
                    //
                    // Bản cũ mở đầu bằng một câu **90 ký tự** kể ruột huba
                    // (*"huba đã chạy hộ lệnh này trên máy — cwd …, KHÔNG có
                    // tty"*), và nó nằm lại trong nhật ký phiên VĨNH VIỄN, tức
                    // ngốn ngữ cảnh của chính phiên ấy ở mọi lượt sau. Phiên
                    // cần đúng hai điều: **lệnh nào** và **ra gì**.
                    //
                    // Ba chữ `[huba chạy hộ]` vẫn phải giữ, và đây là phần
                    // load-bearing chứ không phải lịch sự: thiếu nó thì phiên
                    // đọc khối này như thể CHÍNH NÓ vừa chạy lệnh — rồi kể lại
                    // như việc mình đã làm.
                    //
                    // Còn "không có tty" chỉ nói khi lệnh HỎNG: lúc ấy nó là
                    // một lý do (sudo/ssh chết ở dòng hỏi mật khẩu); lúc thành
                    // công thì nó là một mẩu tin không ai dùng.
                    let block = runin_block(&line, &report, r.timed_out || r.code != Some(0));
                    // 🔴 GỠ 2026-08-16 cổng "kết quả có dấu hiệu bí mật thì
                    // KHÔNG dán vào phiên". Nút ▶️ có đúng một việc: chạy rồi
                    // đưa kết quả vào phiên (Hà 16/08: *"1 là chạy xong lấy kết
                    // quả đưa vào phiên"*) — giữ kết quả lại là bỏ dở đúng cái
                    // việc ấy, và bỏ dở im lặng ngay lúc lệnh vừa in ra thứ
                    // đáng đọc nhất. Ghi dấu hiệu vào log, chữ đi tiếp.
                    crate::sessions::note_preview_risk("runin_block", &block);
                    {
                        match crate::keys::window_of(&s.tty) {
                            // Cú Enter rời nay nằm ở MỘT chỗ (`keys::type_and_send`).
                            // Bản cũ chép tay vòng lặp ấy vào đây và nuốt lỗi
                            // bằng `let _ = press(…)` — tức nếu Enter không gửi
                            // được thì huba vẫn in "✅ đã chạy", không một dòng
                            // log nào. Đúng hình dạng luật 3 cấm.
                            Ok(Some(w)) => match crate::keys::type_and_send(w, &block) {
                                Ok(crate::keys::Delivered::Gone) => {
                                    format!(
                                        "✅ Đã chạy trên máy rồi dán kết quả vào {}:\n$ {}\n{}",
                                        crate::sessions::shown(&s),
                                        line,
                                        crate::exec::truncate(&report, 400)
                                    )
                                }
                                // 🔴 DÁN ĐƯỢC KHÁC GỬI ĐƯỢC — Hà 2026-08-19,
                                // ảnh ô nhập `[dwork]` mang nguyên khối này:
                                // *"nội dung sao bị chèn lung tung ở đâu vào ô
                                // chat"*. Nó nằm đó **một tiếng**, trong khi
                                // huba đã nói "✅ đã dán vào phiên" từ đầu — vì
                                // cả ba đường ra của `type_and_send` cùng trả
                                // `Ok(())`. Nay ba đường ba câu, và câu này nói
                                // rõ chữ đang Ở ĐÂU cùng cách gỡ.
                                Ok(other) => {
                                    logging::warn(
                                        "runin_block_left_in_box",
                                        json!({ "session": s.session_id, "n": n,
                                                "landed": format!("{other:?}"),
                                                "effect": "kết quả nằm trong ô nhập của phiên, CHƯA gửi" }),
                                    );
                                    // Cùng bẫy với nhánh `Bảng ĐÃ ĐỦ` (xem chú
                                    // thích ở `pick_answer`): `/key enter` vẽ ra
                                    // một lệnh chạm được, nhưng chạm chỉ gửi
                                    // token `/key` — chữ sau dấu cách rơi mất,
                                    // và huba đáp *"Chưa hiểu lệnh này"* về đúng
                                    // cái nó vừa mời bấm. `send_`/`clr_` là hai
                                    // liên kết TỰ MANG phiên, đã có sẵn trong
                                    // `verbs.rs` cho đúng việc này.
                                    format!(
                                        "⚠ Đã chạy xong, nhưng kết quả còn NẰM TRONG Ô NHẬP của {} — chưa gửi.\n\
                                         Bấm /send_{sid} để gửi, hoặc /clr_{sid} để xoá đi.\n\n$ {}\n{}",
                                        crate::sessions::shown(&s),
                                        line,
                                        crate::exec::truncate(&report, 400),
                                        sid = s.session_id.chars().take(8).collect::<String>()
                                    )
                                }
                                // 🔴 KHÔNG BỎ CUỘC Ở ĐÂY — xem [`RUNIN_PENDING_KEY`].
                                //
                                // Hà 2026-08-23: *"Sao chạy lệnh xong lại báo
                                // không dán đc vào phiên vì quá 20s"*. Lệnh
                                // chạy xong thật; thứ hết hạn là cú `osascript`
                                // hỏi Terminal — 386 lượt trong một ngày. Một
                                // lần hỏi trượt không phải một sự thật vĩnh
                                // viễn, nên giữ kết quả lại và gõ lại vòng sau.
                                Err(e) => {
                                    remember_runin_pending_for(
                                        &cfg,
                                        &s.session_id,
                                        &line,
                                        &block,
                                        chrono::Utc::now().timestamp(),
                                    );
                                    format!(
                                        "⏳ Đã chạy xong. Terminal chưa nhận được ({}) — huba sẽ gõ lại, \
                                         báo anh khi vào được phiên.\n\n$ {}\n{}",
                                        crate::exec::truncate(&e.to_string(), 160),
                                        line,
                                        crate::exec::truncate(&report, 600)
                                    )
                                }
                            },
                            _ => {
                                remember_runin_pending_for(
                                    &cfg,
                                    &s.session_id,
                                    &line,
                                    &block,
                                    chrono::Utc::now().timestamp(),
                                );
                                format!(
                                    "⏳ Đã chạy xong. Chưa tìm ra cửa sổ của {} — huba sẽ gõ lại, \
                                     báo anh khi vào được phiên.\n\n$ {}\n{}",
                                    crate::sessions::shown(&s),
                                    line,
                                    crate::exec::truncate(&report, 600)
                                )
                            }
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
            if quiet {
                // Im với Telegram, KHÔNG im với sổ: một cỗ máy chạy lệnh mà
                // không để lại dấu nào là thứ không ai kiểm được sau này.
                logging::info(
                    "runin_ack_quiet",
                    json!({ "session": s.session_id,
                            "ack": crate::exec::truncate(&ack, 200) }),
                );
            } else {
                say_back(&cfg, &adapter, &chat_id, &ack);
            }
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
/// Đề bài của MỘT lượt mở phiên — gom lại thay vì tám tham số rời.
///
/// Tám thứ đi cùng nhau qua ba tầng hàm thì thứ tự của chúng là một cái bẫy: đổi
/// chỗ `task` với `account` là trình dịch vẫn nhận (cùng kiểu `String`), và phiên
/// mở ra chạy nhầm tài khoản với một đề bài là tên tài khoản. Cùng họ với con bug
/// `[] acc3 dwork` sáng nay.
pub struct NewSession {
    pub cfg: Config,
    pub name: String,
    pub dir: std::path::PathBuf,
    pub task: String,
    pub account: Option<String>,
    /// `Some(id)` ⟹ MỞ LẠI phiên ấy (`claude --resume`), không mở phiên mới.
    pub resume: Option<String>,
    pub adapter: String,
    pub chat_id: String,
}

fn watch_new_session(job: NewSession) {
    let NewSession {
        cfg,
        name,
        dir,
        task,
        account,
        resume,
        adapter,
        chat_id,
    } = job;
    let (fb_cfg, fb_adapter, fb_chat) = (cfg.clone(), adapter.clone(), chat_id.clone());
    let spawned = std::thread::Builder::new()
        .name("new-session".into())
        .spawn(move || {
            let _lane = crate::exec::urgent();
            let started =
                crate::sessions::start_background(
                    &cfg,
                    &name,
                    &dir,
                    &task,
                    account.as_deref(),
                    resume.as_deref(),
                );
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
                    // 🔴 KHÔNG in một chỗ trống — Hà 2026-08-16, ảnh chụp lúc
                    // 15:34: *"Đã mở ⌨ cửa sổ Terminal cho ."*. `/new acc1
                    // <đề bài>` không khai dự án, nên `s.project` rỗng và câu
                    // chào để lại đúng một dấu chấm lửng lơ. Cùng họ với cặp
                    // ngoặc `()` trong câu hỏi đóng cửa sổ trần, vá cùng ngày:
                    // huba khai một dữ kiện mà chính nó biết là không có.
                    let cho = if s.project.trim().is_empty() {
                        String::new()
                    } else {
                        format!(" cho {}", s.project)
                    };
                    // 🔴 BỎ câu *"Nó chạy không hỏi ai"* — Hà 2026-08-16: *"khi
                    // tạo phiên mới sao chèn thêm câu 'nó chạy không hỏi ai'
                    // vào làm gì?"*.
                    //
                    // Nó đúng về sự thật (phiên huba mở chạy `--permission-mode
                    // auto`) nhưng sai về chỗ đứng: nó nói MỘT TÍNH CHẤT CỐ
                    // ĐỊNH của mọi phiên huba mở, lặp lại nguyên văn ở mọi lần
                    // mở, cho đúng người đã dựng ra tính chất ấy. Cùng lý do
                    // luật 11 cấm nói TRẠNG THÁI trong một vòng lặp: một cảnh
                    // báo kêu ở mọi lượt thì không còn là cảnh báo, nó là nhiễu
                    // — và nhiễu ăn mất chỗ của câu đáng đọc trong cùng tin.
                    // Cái rào thật không nằm ở câu này mà ở `DENIED_TOOLS`
                    // (không đẩy/xoá/ssh/sudo/deploy), và nó không đổi theo
                    // việc có in dòng chữ hay không.
                    // 🔴 ĐƯỜNG LUI PHẢI NÓI RA — Hà 2026-08-19: *"phiên lại báo
                    // không có cửa sổ là sao"*. Lệnh của anh đúng; huba thử mở
                    // cửa sổ, osascript hết giờ, nên nó lui về `--bg`. Câu chào
                    // cũ chỉ đổi hai chữ (`⌨` → `🌙`) rồi im về ba thứ người
                    // đọc cần: huba ĐÃ THỬ, vì sao trượt, và cái giá.
                    let lui = match (s.window, s.fallback_why.as_deref()) {
                        (false, Some(why)) => format!(
                            "\n\n⚠ Định mở cửa sổ Terminal nhưng KHÔNG mở được ({why}) — nên nó là \
                             phiên nền: không gõ thẳng vào được, `/shot` không có màn để chụp. \
                             Nói tiếp bằng /tell, tắt bằng /stop. Mở lại kiểu cửa sổ thì thử /new \
                             lần nữa khi máy đỡ bận."
                        ),
                        _ => String::new(),
                    };
                    format!(
                        "🚀 Đã mở {}{}.\nPhiên {} — đang chạy trên máy.{}\n\n🎯 Nay đang theo phiên này: gõ thẳng câu hỏi ở đây là vào nó. Tắt bằng /stop.",
                        cua_so,
                        cho,
                        &s.session_id[..8.min(s.session_id.len())],
                        lui
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
            // Tin này mang KẾT QUẢ một lệnh của phiên (`$ …` + đầu ra), nên nó
            // đi qua cùng bộ định dạng với `/shot` — không có phiên nào để gắn
            // action ở luồng này (không cầm `Db`), nhưng chữ vẫn phải hiện
            // GIỐNG mọi tin khác: cùng phép gột markdown, cùng cách cắt tin.
            // Hà 2026-08-16: *"mọi thứ nhìn thấy ở tele phải đồng nhất"*.
            say_session_data(i, text, &[], "telegram_ack_failed", &SessionData::default());
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
/// Mọi thứ từ phiên đi ra Telegram đều phải được ĐỊNH NGHĨA rồi mới gắn action.
///
/// 🔴 Hà 2026-08-16: *"Mọi dữ liệu từ phiên trước khi gửi lên tele đều phải được
/// định nghĩa, chèn action phù hợp"* — sau một buổi tôi vá từng ca lẻ (dòng
/// lệnh, ô nhập, lựa chọn, tệp) ở bốn chỗ khác nhau, và ca thứ năm lại hỏng.
///
/// Đây là bảng ấy: mỗi loại dữ liệu → neo (chuỗi có thật trong tin) → action.
/// Thêm một loại mới thì thêm một dòng ở đây, không đi sửa bốn chỗ.
#[derive(Debug, Clone, Default)]
pub struct SessionData {
    /// Mã phiên — mọi action phải tự mang nó (bấm lại tin cũ vẫn đúng phiên).
    pub sid: String,
    /// Dòng lệnh phiên nhắc tới → ▶️ chạy.
    pub cmds: Vec<String>,
    /// Lựa chọn đang hiện trên màn → ☑ bấm chọn, NGAY TẠI dòng của nó.
    /// `(mã lựa chọn, nhãn)` — mã là `"3"` cho hộp một câu, `"1.3"` cho bảng
    /// nhiều câu (câu 1, lựa chọn 3). Xem `session_layout`.
    pub choices: Vec<(String, String)>,
    /// Chữ đang nằm trong ô nhập → ⏎ gửi · ⌫ xoá.
    pub box_text: Option<String>,
    /// Màn có dòng `Submit` (hộp CHỌN NHIỀU) → ✅ gửi bảng, ngay tại dòng ấy.
    ///
    /// 🔴 Hà 2026-08-17, sau khi ☑ đã bám đúng từng dòng: *"Bấm chọn được rồi,
    /// chưa bấm được submit"*. Dòng `Submit` là một dòng THẬT trên màn, không
    /// mang số nên không có `k_`/`pick_` nào trỏ tới — nó cần đích chạm riêng.
    pub submit: bool,
    /// Tệp phiên nhắc tới → 📎 tải về, NGAY TẠI tên tệp trong chữ.
    ///
    /// `(chuỗi neo — đường dẫn đúng như nó hiện trong chữ, chỉ số trong sổ tệp)`.
    /// Hà 2026-08-16: *"chưa chèn link tải file xuất hiện trong nội dung phiên
    /// gửi lên tele"*.
    pub files: Vec<(String, usize)>,
    /// Thanh tab của bảng hỏi nhiều câu → ↪ sang tab ấy, NGAY TẠI nhãn của nó.
    ///
    /// `(số tab đếm từ 1, nhãn, đã trả lời chưa)`.
    ///
    /// 🔴 Hà 2026-08-19: *"Sao không chèn nút trực tiếp ở phần nội dung lại đi
    /// chèn thêm nút ở cuối"*. Bản đầu của tôi ném ba cái nút xuống đáy tin, và
    /// lý do thì thuần là lý do của MÃ: `html_with_links` gắn mỗi dòng một neo,
    /// mà thanh tab là MỘT dòng mang ba nhãn. Nên tôi đi đường dễ — đúng thứ
    /// `CLAUDE.md` đã cấm hai lần (*"nút chọn phải chèn ngay tại các dòng chọn"*
    /// · *"Hạn chế dùng khối nút ở cuối tin"*).
    ///
    /// Cách đúng không phải là sửa bộ gắn neo cho nó gắn được nhiều neo một
    /// dòng, mà là **bẻ thanh tab thành mỗi tab một dòng** — xem
    /// [`split_tab_bar`]. Trên màn 390px nó vốn đã tự xuống dòng lộn xộn rồi.
    pub tabs: Vec<(usize, String, bool)>,
}

impl SessionData {
    fn short(&self) -> String {
        // 🔴 Id CỬA SỔ TRẦN đi nguyên — cắt 8 ký tự là cắt mất số tty, tức cắt
        // mất chính cái phân biệt cửa sổ này với cửa sổ khác (`win-ttys002` ⟹
        // `win-ttys`). Xem `verbs::sid_ok`: hai cái ☑ vẽ ra rồi không đường nào
        // nhận, Hà 2026-08-19 *"Sao khong bam chon được"*.
        if crate::sessions::is_shell_id(&self.sid) {
            return self.sid.clone();
        }
        self.sid.chars().take(8).collect()
    }
}

/// Số dòng lệnh huba nhặt từ MỘT lượt phiên — **không còn trần** (2026-08-16).
///
/// 🪦 Từng là 4, rồi 12, và Hà gỡ hẳn: *"Bỏ hẳn trần cắt lệnh đi"*. Nó sinh ra
/// hồi mỗi lệnh là một cái NÚT ở đáy tin — mười nút thì bàn phím Telegram thành
/// một bức tường và nhãn bị cắt ở 52 ký tự. Từ khi icon nằm GIỮA CHỮ, mỗi lệnh
/// mang icon trên chính dòng của nó, không chiếm thêm chỗ nào: cái giá đã biến
/// mất, chỉ còn cái trần ở lại, và nó cắt IM LẶNG từ đầu danh sách (đúng ảnh
/// *"bảy dòng lệnh, ba icon"*).
pub const CMD_LINES_MAX: usize = usize::MAX;

/// 🔴 MỘT CỬA cho mọi tin **mang nội dung phiên** ra Telegram.
///
/// Hà 2026-08-16: *"lệnh `/shot` hay phản hồi tự động gửi về tele đều phải qua
/// định dạng trước khi gửi → cái nhận được ở tele phải thao tác được với các
/// lệnh link của phiên đó"* · *"mọi thứ nhìn thấy ở tele phải đồng nhất"* ·
/// *"dành cho nội dung lấy từ phiên thôi"*.
///
/// Đo được cái hỏng: chỉ `/shot` và tin tự phát đi qua bộ định dạng
/// (`say_session_data`), còn ack của **mọi route khác** đi bằng `send_text`
/// trần — nên cùng một câu của phiên, cùng một dòng lệnh trong đó, khi thì bấm
/// được khi thì không, tuỳ nó ra bằng cửa nào. Người đọc không có cách nào biết
/// trước, nên phải thử — và thử hụt thì tin ấy coi như chữ chết.
///
/// Cửa này chỉ dành cho chữ CỦA PHIÊN. Tin thuần của huba ("không mở được cửa
/// sổ", `/help`, danh sách tài khoản) đi đường thường: không có phiên nào để
/// gắn action, gắn bừa thì nút trỏ vào chỗ trống.
pub fn say_from_session(
    db: &Db,
    cfg: &Config,
    tg: &crate::telegram::Inbox,
    sid: &str,
    text: &str,
    extra: &[(String, String)],
    log_key: &str,
) {
    say_from_session_with(db, cfg, tg, sid, text, extra, &[], log_key)
}

/// Như trên, nhưng chỗ gọi khai thêm LỰA CHỌN đang hiện trên màn → ☑ ngay tại
/// dòng của nó.
///
/// Dùng cho bản *"Xem đầy đủ"*, thứ nay mang theo cả khúc màn cuối — xem
/// `screen_tail`.
#[allow(clippy::too_many_arguments)]
pub fn say_from_session_with(
    db: &Db,
    cfg: &Config,
    tg: &crate::telegram::Inbox,
    sid: &str,
    text: &str,
    extra: &[(String, String)],
    choices: &[(String, String)],
    log_key: &str,
) {
    let cmds = cmds_of_text(cfg, sid, text);
    let mut buttons = remember_quick(db, sid, &cmds);
    // Tệp NHẮC TỚI trong chính câu này phải mở được ngay tại tên nó — cùng luật
    // với `/shot`, vì "lúc được lúc không" là thứ người đọc không đoán nổi.
    //
    // Dò trên phần THÂN, không kể ô nhập: chữ chưa gửi thì đích chạm của nó là
    // ⏎/⌫, không phải 📎 (xem `keys::body_before_box`).
    let seen_paths = paths_not_in_commands(
        text,
        &crate::keys::paths_on_screen(&crate::keys::body_before_box(text), 4),
        &cmds,
    );
    let files = file_anchors(db, cfg, sid, &seen_paths);
    if !files.is_empty() {
        // Sổ tệp phải được ghi thì cú bấm sau mới tra ra đường dẫn; nút ở đáy
        // là tác dụng phụ đáng giữ (một liên kết không dựng được thì vẫn còn
        // đường bấm).
        buttons.extend(remember_files(db, cfg, sid, &seen_paths));
    }
    buttons.extend(extra.iter().cloned());
    let data = SessionData {
        sid: sid.to_string(),
        cmds: crate::sessions::lines_of(&cmds),
        files,
        choices: choices.to_vec(),
        ..Default::default()
    };
    say_session_data(tg, text, &buttons, log_key, &data);
}

/// Ghi một lượt dùng, rồi khai lại menu ☰ NẾU thứ tự đổi.
///
/// Điểm của mỗi loại lệnh suy giảm theo thời gian (xem [`decayed`]) và cộng 1
/// cho lượt vừa chạy. Khai lại chỉ khi thứ tự khác lần trước: Telegram không có
/// cách nào "sửa một dòng", mỗi lần khai là gửi cả danh sách, nên gửi mỗi lượt
/// bấm là tốn một lượt HTTP cho một cái menu y hệt.
/// Phải hơn kẻ đứng trên BAO NHIÊU thì mới được vượt mặt.
///
/// 🔴 Hà 2026-08-19: *"Sắp xếp ưu tiên menu đang theo flow nào mà tôi thấy cứ
/// nhảy loạn lên"*. Flow thì đúng — tần suất có suy giảm theo thời gian, chính
/// thứ anh đặt hôm 17/08 — nhưng nó thiếu cái hãm, nên **hai lệnh sát điểm nhau
/// đổi chỗ sau MỖI lượt bấm**.
///
/// 📐 Đo trên sổ thật (`cursors.menu:usage`, 19/08): `Session` **257,6** ·
/// `Shot` **241,2** — hơn nhau **6,8%**. Mà bấm một phiên là chạy `/session`
/// rồi `/shot` liền nhau, mỗi lượt +1 cho một bên, nên hai đứa **thay nhau dẫn
/// đầu vĩnh viễn**. Log nói đúng điều đó: 48 lượt xếp lại trong hai ngày, và
/// riêng cặp 1↔2 lật **bốn lần trong 13 phút** (08:34:35 → 08:34:38 → 08:45:17
/// → 08:45:21), có lần **cách nhau 3 giây**. Cặp `Type` 100,7 / `Key` 97,3
/// (3,5%) là cặp thứ hai đang chờ tới lượt.
///
/// 1,25 ⟹ muốn vượt `Session` thì `Shot` phải đạt ~322 điểm, tức hơn hẳn vài
/// chục lượt dùng chứ không phải một cú bấm. Nó KHÔNG đóng băng menu: một lệnh
/// thật sự đang được dùng nhiều hơn vẫn leo, chỉ là leo vì đang được dùng nhiều
/// hơn, không phải vì vừa được bấm sau.
pub const MENU_LEAD_MARGIN: f64 = 1.25;

/// Thứ tự menu ĐÃ HÃM: giữ nguyên thứ tự đang có, trừ chỗ kẻ dưới hơn hẳn kẻ trên.
///
/// Hàm thuần, và cố ý thế: đây là toàn bộ phần "có nên đổi chỗ không", nên nó
/// phải kiểm được bằng đúng những con số đã làm menu nhảy — xem
/// `tests/menu_order.rs`.
///
/// Đi từ thứ tự CŨ chứ không từ bảng điểm: cái người dùng đang nhớ là thứ tự cũ,
/// nên nó là điểm xuất phát, còn điểm số chỉ được phép đẩy từng nấc. Lệnh mới
/// (chưa từng có trong thứ tự cũ) xếp cuối theo điểm của nó — điểm 0 thì đứng
/// cuối, đúng chỗ.
pub fn menu_settled_order(
    prev: &[String],
    scored: &[(&'static str, &'static str, u64)],
) -> Vec<(&'static str, &'static str)> {
    let mut order: Vec<(&'static str, &'static str, u64)> = Vec::with_capacity(scored.len());
    for name in prev {
        if let Some(row) = scored.iter().find(|(n, _, _)| n == name) {
            order.push(*row);
        }
    }
    for row in scored {
        if !order.iter().any(|(n, _, _)| *n == row.0) {
            order.push(*row);
        }
    }
    // Nổi bọt từng nấc một, và chỉ khi hơn đủ biên. Từng nấc là có chủ: một lệnh
    // vừa sống lại thì leo dần, mắt còn theo kịp — nhảy tám bậc một lượt
    // (`accounts` 12→8, đo 18/08 01:57) thì lần sau tìm nó ở đâu cũng sai.
    let n = order.len();
    for _ in 0..n {
        let mut moved = false;
        for i in 1..n {
            let (up, down) = (order[i - 1].2 as f64, order[i].2 as f64);
            if down > up * MENU_LEAD_MARGIN {
                order.swap(i - 1, i);
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    order.into_iter().map(|(n, h, _)| (n, h)).collect()
}

pub fn menu_reorder_if_needed(db: &Db, kind: CommandKind, now_ms: i64) {
    let mut book: std::collections::BTreeMap<String, (f64, i64)> = db
        .cursor_or_log(MENU_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    let key = format!("{kind:?}");
    let cur = book.get(&key).copied().unwrap_or((0.0, now_ms));
    let score = decayed(cur.0, cur.1, now_ms, MENU_HALF_LIFE_MS) + 1.0;
    book.insert(key, (score, now_ms));
    if let Ok(v) = serde_json::to_string(&book) {
        if let Err(e) = db.set_cursor(MENU_KEY, &v) {
            logging::error("menu_usage_not_saved", json!({ "err": e.to_string() }));
            return;
        }
    }
    let scored = crate::commands::for_telegram_scored(|r| {
        let k = format!("{:?}", r.kind);
        let (s, t) = book.get(&k).copied().unwrap_or((0.0, now_ms));
        // Xếp bằng số nguyên để thứ tự không nhảy vì sai số dấu phẩy động; nhân
        // 1000 giữ đủ phân giải cho những điểm đã mờ gần hết.
        (decayed(s, t, now_ms, MENU_HALF_LIFE_MS) * 1000.0) as u64
    });
    let stored = db.cursor_or_log(MENU_ORDER_KEY);
    let prev: Vec<String> = stored
        .as_deref()
        .map(|s| s.split(',').map(str::to_string).collect())
        .unwrap_or_default();
    let rows = menu_settled_order(&prev, &scored);
    let order: Vec<&str> = rows.iter().map(|(n, _)| *n).collect();
    let joined = order.join(",");
    if stored.as_deref() == Some(joined.as_str()) {
        return;
    }
    let Some(tg) = crate::telegram::inbox() else {
        return;
    };
    match tg.register_command_list(rows) {
        Ok(()) => {
            if let Err(e) = db.set_cursor(MENU_ORDER_KEY, &joined) {
                logging::error("menu_order_not_saved", json!({ "err": e.to_string() }));
            }
            logging::info("menu_reordered", json!({ "order": order }));
        }
        // Không khai được thì THÔI, và nói ra: menu cũ vẫn dùng được, đây là
        // tiện nghi chứ không phải đường đi của một mệnh lệnh nào.
        Err(e) => logging::warn("menu_reorder_failed", json!({ "err": e })),
    }
}

/// Khúc CUỐI của màn phiên — gồm cả ô nhập — kèm lựa chọn đang hiện.
///
/// 🔴 Hà 2026-08-17: *"Xem đầy đủ gắn thêm phía cuối nội dung cuối của shot bao
/// gồm ô chờ nhập để biết đang gợi ý gì còn thao tác nhanh luôn"*.
///
/// Hai thứ ấy vốn ở hai nguồn khác nhau, và đó là lý do chúng khác nhau: bản
/// đầy đủ là NGUYÊN VĂN lượt cuối trong nhật ký (không bẻ dòng, không cắt),
/// còn ô nhập với hộp chọn chỉ tồn tại trên MÀN. Nên bản đầy đủ không thể tự
/// biết ô nhập đang có gì — phải đi đọc màn, ngay lúc bấm.
///
/// `None` khi phiên không còn cửa sổ, hoặc không đọc được màn: lúc ấy bản đầy đủ
/// vẫn gửi được, chỉ thiếu phần đuôi — thà thiếu một khúc còn hơn nuốt cả tin.
pub fn screen_tail(
    cfg: &Config,
    sid: &str,
    lines: usize,
) -> Option<(String, Vec<(String, String)>)> {
    let live = crate::sessions::snapshot(cfg);
    let s = live
        .sessions
        .iter()
        .find(|s| same_session(&s.session_id, sid))?;
    if !crate::sessions::is_real_tty(&s.tty) {
        return None;
    }
    let (body, choices) = crate::keys::screen_of(&s.tty, lines)?;
    // Hộp trên màn lúc này là hộp MỘT CÂU đang mở, nên mã đi bằng `k_` — bảng
    // nhiều câu có đường riêng (`pick_`) và nó được dựng ở chỗ biết số câu.
    let choices = choices
        .into_iter()
        .map(|(n, l)| (n.to_string(), l))
        .collect();
    Some((body, choices))
}

/// 🔴 CHỈ những lệnh CÓ MẶT trong chính câu này — cửa định dạng không thêm nội
/// dung.
///
/// `session_layout` cố ý nối thêm khu *"Lệnh phiên chạy không được (cổng quyền
/// chặn)"* cho lệnh nó không tìm thấy trong chữ. Đúng cho `/shot`: ảnh màn
/// thiếu đúng dòng bị cổng quyền chặn, và dòng ấy là thứ đáng đọc nhất. Sai cho
/// mọi câu khác — một cái ack hai dòng sẽ mọc thêm cả một danh sách lệnh không
/// ai hỏi, đúng thứ Hà đã chê sáng 16/08 (*"một mớ text không cần thiết"* ·
/// *"quá tốn context"*).
pub fn cmds_present_in(text: &str, cmds: Vec<crate::sessions::Cmd>) -> Vec<crate::sessions::Cmd> {
    cmds.into_iter()
        .filter(|c| text.contains(c.line.as_str()))
        .collect()
}

/// MỘT nguồn lệnh cho mọi tin mang chữ phiên — nhật ký **và** chính chữ ấy.
///
/// 🔴 Hà 2026-08-16, ảnh chụp một bản *"Xem đầy đủ"*: *"Bấm xem đầy đủ sao lại
/// khác lệnh shot, chưa gắn được nút chạy lệnh"*. Đúng, và lý do là hai đường
/// hỏi hai nguồn khác nhau: `/shot` đọc NHẬT KÝ (`commands_of` — lệnh phiên
/// THẬT SỰ đã chạy, kèm thư mục), còn bản đầy đủ đọc CHỮ
/// (`keys::commands_in_report` — lệnh phiên mới chỉ VIẾT RA, `cwd` rỗng), lại
/// còn kèm trần 3. Cùng một màn, hai bộ nút khác nhau.
///
/// Hai nguồn ấy KHÔNG thay được cho nhau, nên gộp chứ không chọn: nhật ký biết
/// thư mục nhưng mù với lệnh phiên chỉ đề xuất; chữ thì ngược lại. Nhật ký đi
/// trước để `cwd` của nó thắng khi cùng một dòng xuất hiện ở cả hai.
pub fn cmds_of_text(cfg: &Config, sid: &str, text: &str) -> Vec<crate::sessions::Cmd> {
    let mut out = cmds_present_in(text, crate::sessions::commands_of(cfg, sid, CMD_LINES_MAX));
    add_prose_cmds(cfg, sid, text, &mut out);
    out
}

/// Bộ lệnh cho một ẢNH MÀN (`/shot`) — cùng ba nguồn, **không lọc theo màn**.
///
/// Khác `cmds_of_text` đúng một điểm, và điểm ấy có chủ: lệnh trong nhật ký mà
/// KHÔNG có trên màn vẫn được giữ, để `session_layout` viết thêm nó vào cuối tin
/// dưới nhãn *"Lệnh phiên chạy không được (cổng quyền chặn)"*. Đó là ca dòng
/// lệnh bị cổng quyền từ chối: phiên không hề viết nó ra lời, nên bỏ nó đi là
/// giấu mất đúng dòng đáng đọc nhất.
pub fn cmds_for_screen(cfg: &Config, sid: &str, screen: &str) -> Vec<crate::sessions::Cmd> {
    let mut out = crate::sessions::commands_of(cfg, sid, CMD_LINES_MAX);
    add_prose_cmds(cfg, sid, screen, &mut out);
    out
}

/// Thêm lệnh bóc từ CHỮ: chữ đang hiện, rồi nguyên văn lượt cuối trong nhật ký.
fn add_prose_cmds(cfg: &Config, sid: &str, text: &str, out: &mut Vec<crate::sessions::Cmd>) {
    let add = |line: String, out: &mut Vec<crate::sessions::Cmd>| {
        if line.trim().is_empty() || out.iter().any(|c| c.line == line) {
            return;
        }
        // 🔴 MẢNH BỊ CỬA SỔ BẺ KHÔNG PHẢI MỘT LỆNH THỨ HAI — 2026-08-17.
        //
        // Cùng một dòng có thể tới đây hai lần với hai độ dài: nhật ký giữ
        // nguyên văn, còn MÀN thì bị cửa sổ bẻ, nên nửa đầu của nó cũng lọt qua
        // `commands_in_report`. Thêm cả hai là dựng ra hai cái nút cho một việc
        // — và cái nút mọc từ mảnh cụt chạy một lệnh KHÁC HẲN: `printf '…\n'`
        // thiếu mất `> …/up-holiday.cmd` chỉ in ra màn, không xếp việc nào cả.
        // Một cái nút chạy sai thứ nó ghi trên mình là thứ tệ hơn không có nút.
        if out.iter().any(|c| c.line.starts_with(&line)) {
            return;
        }
        // Ngược chiều: bản dài hơn tới sau thì nó THAY mảnh cụt — nhưng chỉ thay
        // mảnh bóc từ chữ (`cwd` rỗng), không đụng dòng của nhật ký (nó mang
        // theo thư mục, thứ chữ không có).
        if let Some(i) = out
            .iter()
            .position(|c| c.cwd.is_empty() && line.starts_with(&c.line))
        {
            out[i].line = line;
            return;
        }
        out.push(crate::sessions::Cmd {
            line,
            cwd: String::new(),
        });
    };
    for line in crate::keys::commands_in_report(text, CMD_LINES_MAX) {
        add(line, out);
    }
    // 🔴 NGUỒN THỨ BA: nguyên văn lượt cuối trong NHẬT KÝ — Hà 2026-08-17, ảnh
    // `/shot` `[codetrail]`: *"Có lệnh trong nội dung nhưng không có nút, sao xử
    // lý mãi không xong vấn đề này thế, có cần dùng ai để bóc tách không"*.
    //
    // Dòng ấy là `git -C ~/… add … && git -C ~/… commit -m "…"`, dài hơn bề
    // ngang cửa sổ nên MÀN bẻ nó làm bốn. Bóc lệnh từ màn thì không dòng nào còn
    // là một lệnh; đó không phải chuyện đoán giỏi hay dở, mà là nguồn đã hỏng
    // trước khi đọc. Nhật ký giữ nguyên văn, nên câu trả lời cho *"có cần dùng
    // AI để bóc tách không"* là KHÔNG: đọc đúng nguồn thì một phép so chuỗi là
    // đủ, tất định và không tốn hạn mức nào.
    //
    // Màn vẫn là chỗ để BÁM (`line_carries` khớp cả phần đầu cho dòng bị bẻ);
    // lệnh nào không bám được thì `session_layout` viết thêm nó vào cuối tin.
    //
    // 🔴 …NHƯNG NGUỒN NÀY PHẢI LỌC THEO CHÍNH CHỮ ĐANG ĐỊNH DẠNG — Hà 2026-08-17,
    // ảnh ba cái ack liên tiếp, cái nào cũng mọc thêm một khu *"Lệnh của phiên,
    // không thấy trên màn"* dài bốn dòng: `open -W -g "/Applications/Docker.app…`,
    // `docker exec`, `cargo clippy …`, `node --check`.
    //
    // Chúng là lệnh trong LƯỢT NÓI CUỐI của phiên, không phải trong cái ack hai
    // dòng vừa gửi. Không lọc thì mỗi lời đáp "▶ đang chạy — …" lại kéo theo cả
    // sổ lệnh của phiên, đúng thứ `cmds_present_in` sinh ra để chặn và đúng thứ
    // Hà đã chê từ 16/08 (*"một mớ text không cần thiết"*).
    //
    // Lọc bằng `line_carries` chứ không `contains`: với `/shot`, cửa sổ bẻ đôi
    // một lệnh dài nên màn chỉ mang NỬA ĐẦU — mà nửa đầu ấy vẫn là "có mặt".
    // Cùng phép đo với `session_layout`, để hai bên không nói ngược nhau.
    if let Some(said) = crate::sessions::last_say_by_id(cfg, sid, crate::sessions::SAY_MAX) {
        for line in crate::keys::commands_in_report(&said, CMD_LINES_MAX) {
            if text.lines().any(|l| line_carries(l, &line)) {
                add(line, out);
            }
        }
    }
}

/// Bỏ những đường dẫn NẰM TRONG một dòng lệnh — chúng đã có đích chạm riêng.
///
/// 🔴 Hà 2026-08-16, đọc chính tin tôi vừa gửi (`rm ~/…/probe_prompt_anchor.rs`
/// kèm một nút 📎 `probe_prompt_anchor.rs`): *"Mà dòng lệnh lại gắn nút tải file
/// là sao"*. Vì `paths_on_screen` quét cả dòng lệnh, thấy một đường dẫn hợp lệ
/// và dựng nút tải — trên đúng cái tệp mà dòng lệnh ấy bảo XOÁ.
///
/// Một dòng, một ý định: dòng lệnh thì đích chạm là ▶️/🖥, còn 📎 dành cho tệp
/// được nhắc tới như một tệp để đọc.
///
/// 🔴 Và phép lọc phải hỏi theo DÒNG, không theo đường dẫn — Hà 2026-08-18, ảnh
/// chụp một tin `/shot` của phiên dwork: *"Nội dung này có file html nhưng lại
/// không có nút tải"*. Tin ấy nhắc tệp hai lần, đúng hình dạng mọi phiên viết
/// xong báo cáo đều dùng:
///
/// ```text
/// **`~/projects/dwork/dev/docs/bao-cao/bao-cao-ra-soat-2026-08-18.html`**  ← tệp
/// open ~/projects/dwork/dev/docs/bao-cao/bao-cao-ra-soat-2026-08-18.html   ← lệnh
/// ```
///
/// Bản đầu hỏi *"đường dẫn này có nằm trong dòng lệnh nào không"*, nên lần nhắc
/// ĐỘC LẬP ở trên mất nút chỉ vì bên dưới có một dòng lệnh nhắc lại cùng tệp.
/// Câu hỏi đúng là *"có dòng nào nhắc tệp này mà KHÔNG phải dòng lệnh không"* —
/// giữ nguyên ca 16/08 (đường dẫn chỉ xuất hiện trong `rm …` thì vẫn không mọc
/// nút tải), mà không nuốt lần nhắc kia.
///
/// Đo "dòng này có phải dòng lệnh không" bằng [`line_carries`], cùng cái thước
/// `session_layout` dùng để gắn icon — hai phép đo khác nhau ở đây nghĩa là nút
/// mọc ở dòng mà icon không mọc.
pub fn paths_not_in_commands(
    text: &str,
    paths: &[String],
    cmds: &[crate::sessions::Cmd],
) -> Vec<String> {
    paths
        .iter()
        .filter(|p| {
            text.lines()
                .any(|l| l.contains(p.as_str()) && !cmds.iter().any(|c| line_carries(l, &c.line)))
        })
        .cloned()
        .collect()
}

/// Tin này có NỘI DUNG để định dạng không — MỘT câu hỏi, MỘT chỗ trả lời.
///
/// 🔴 Hai lượt liền tôi sửa một chỗ rồi làm hỏng chỗ bên cạnh, và cả hai lần
/// đều vì điều kiện "đi cửa nào" nằm rải trong thân hàm, mỗi lần sửa lại đổi
/// một mảnh:
/// · điều kiện `!quick.is_empty()` ⟹ gỡ hai nút trống là mất luôn liên kết `⏎`
///   giữa chữ (*"Lại mất nút gửi nhanh gợi ý mờ rồi"*);
/// · rồi "luôn đi qua cửa" ⟹ ack `✓ vào hàng chờ` thôi thả emoji, quay lại
///   chiếm một dòng chữ (*"Chỉnh thành phản hồi bằng emoji rồi cơ mà"*).
///
/// Câu hỏi đúng không phải *"có nút không"* mà *"tin này có mang chữ của phiên
/// không"*. Câu xác nhận trơn (`ack_as_emoji` nhận ra được) thì không: nó chỉ
/// nói "đã nhận", và một dấu thả lên tin gốc nói đúng chừng ấy mà không chiếm
/// dòng nào (Hà 2026-08-14: *"Vì nó đơn giản là xác nhận thôi không cần thông
/// tin"*).
pub fn needs_formatting(ack: &str) -> bool {
    ack_as_emoji(ack).is_none()
}

/// Trả lời một cú bấm/lệnh mà nội dung là CHỮ CỦA PHIÊN — qua đúng cửa trên.
///
/// Rơi về `reply_in_channel` khi không có phiên nào để gắn (`sid` rỗng), khi
/// kênh không phải Telegram, hoặc khi câu ấy chỉ là một xác nhận trơn
/// (`needs_formatting`) — ba ca không có nội dung phiên nào để định dạng.
fn reply_from_session(
    db: &Db,
    cfg: &Config,
    adapter: &str,
    cmd: &ChannelCommand,
    sid: &str,
    text: &str,
) {
    if adapter == crate::telegram::NAME && !sid.is_empty() && needs_formatting(text) {
        if let Some(tg) = crate::telegram::inbox() {
            say_from_session(db, cfg, tg, sid, text, &[], "session_ack_failed");
            return;
        }
    }
    reply_in_channel(db, cfg, adapter, cmd, text);
}

pub fn say_with_command_icons(
    tg: &crate::telegram::Inbox,
    text: &str,
    cmds: &[String],
    buttons: &[(String, String)],
    log_key: &str,
) {
    say_session_data(
        tg,
        text,
        buttons,
        log_key,
        &SessionData {
            cmds: cmds.to_vec(),
            ..Default::default()
        },
    )
}

/// Dựng HTML từ bảng dữ liệu — phần thuần của [`say_session_data`], kiểm được.
///
/// 🔴 Câu trên đã SAI trong đúng một ngày, và cái sai ấy là loại nguy hiểm nhất:
/// hàm này từng là một **bản chép tay** của khúc dựng neo trong `say_session_data`
/// — chỉ có nhánh lựa chọn, không có cổng `key_sid`, không có ô nhập, không có
/// dòng lệnh. Nên `tests/choice_links_live.rs` — bài kiểm gửi tin THẬT để chứng
/// minh nút ☑ nằm đúng dòng option — đo bản chép, và sẽ vẫn xanh sau khi ai đó
/// làm hỏng đường thật. Một phép đo không thể đỏ vì sản phẩm hỏng là một phép đo
/// mù (`OPERATING-CHARTER.md` §2d).
///
/// Nay cả hai đi qua [`session_layout`]. Muốn thấy nó đỏ được: bỏ nhánh lựa chọn
/// trong `session_layout` là bài kiểm live đỏ ngay.
pub fn render_session_data(text: &str, data: &SessionData) -> String {
    let l = session_layout(text, data, &[]);
    html_with_links_last(&l.shown, &l.anchors, &l.neo_cuoi).0
}

/// Chữ Telegram sẽ hiển thị, kèm bảng neo của nó.
///
/// Đây là chỗ DUY NHẤT quyết định "loại dữ liệu nào của phiên được gắn action
/// vào đâu" — bảng trong [`SessionData`] đọc thành mã ở đây, một lần.
struct Layout {
    /// Đã gột markdown, đã nối thêm khu "lệnh chạy không được" nếu có.
    shown: String,
    /// `(chuỗi neo, [(đường dẫn, biểu tượng)])` — neo phải có thật trong `shown`.
    anchors: Vec<(String, Vec<(String, String)>)>,
    /// Nút lệnh, cùng thứ tự với `data.cmds` — chỗ gọi cần để dựng đường lùi.
    cmd_btns: Vec<(String, String)>,
    /// Nút còn lại, đã trừ ⏎/⌫ nếu hai cái ấy đã vào được giữa chữ.
    rest_btns: Vec<(String, String)>,
    /// Chỉ số những neo phải bám lần khớp **CUỐI** — xem [`html_with_links_last`].
    ///
    /// Hiện chỉ có ô nhập: nó là dòng dấu nhắc cuối cùng theo đúng định nghĩa,
    /// nên mọi bản trùng đều nằm phía trên nó.
    neo_cuoi: Vec<usize>,
}

/// Thanh tab MỘT dòng → mỗi tab MỘT dòng.
///
/// 🔴 Hà 2026-08-19: *"Sao không chèn nút trực tiếp ở phần nội dung lại đi chèn
/// thêm nút ở cuối"*. Đích chạm phải nằm tại nhãn, mà bộ gắn neo
/// (`html_with_links`) gắn **mỗi dòng một neo** — nên chừng nào ba nhãn còn nằm
/// chung một dòng thì chỉ một cái bám được, và hai cái kia rơi xuống đáy tin.
///
/// Bẻ dòng là phép ĐỊNH DẠNG, không phải thêm nội dung: từng chữ ở đây đều là
/// chữ TUI vẽ ra, chỉ đổi chỗ xuống dòng. Và trên màn 390px thì thanh ấy vốn đã
/// tự gãy lung tung — `←  ☒ RPC pool  ☐ NativeAssets v3  ☐ Việc tiếp  ✔ Submit
/// →` đọc trên điện thoại không ra hàng nào cả.
///
/// Hai mũi tên `←`/`→` bỏ đi cùng lúc: chúng là chỉ dẫn BÀN PHÍM (*"bấm trái
/// phải để đi"*), đúng khi ngồi trước máy và vô nghĩa khi mỗi tab đã có một chỗ
/// để chạm.
fn split_tab_bar(text: &str, tabs: &[(usize, String, bool)]) -> String {
    if tabs.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 64);
    for line in text.lines() {
        let is_bar = line.contains('←')
            && line.contains('→')
            && tabs
                .iter()
                .any(|(_, label, _)| line.contains(label.as_str()));
        if !is_bar {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        for (_, label, answered) in tabs {
            out.push_str(if *answered { "☒ " } else { "☐ " });
            out.push_str(label);
            out.push('\n');
        }
        // Nút gửi của bảng giữ nguyên chữ `Submit` để cái neo ✅ (đọc chính chữ
        // ấy trên màn) còn chỗ bám — xem `data.submit`.
        if line.contains("Submit") {
            out.push_str("✔ Submit\n");
        }
    }
    out
}

fn session_layout(text: &str, data: &SessionData, buttons: &[(String, String)]) -> Layout {
    let cmds = &data.cmds[..];
    // 🔴 HAI KHU VỰC, và ranh giới nằm ngay trong hàm này — Hà 2026-08-16:
    // *"không phân biệt được khu vực à"*.
    //
    // `text` vào đây là ẢNH MÀN của phiên. Vài dòng nữa hàm tự NỐI THÊM một khu
    // thứ hai ("Lệnh phiên chạy không được…"). Neo cho hai nút ⏎/⌫ phải đo trên
    // khu THỨ NHẤT, và phải đo TRƯỚC khi trộn — bản 08:01 đi tìm lại dấu nhắc
    // sau khi đã trộn, nên nó bám vào dòng chữ huba tự viết. Không cần quét
    // ngược: chỗ này biết ranh giới, vì chính nó vẽ ra ranh giới ấy.
    // Chỗ gọi ĐO ĐƯỢC ô nhập trên ảnh màn gốc thì lời nó nói thắng: tới đây
    // `text` có thể đã mang thêm khu chữ do huba nối (*"🗣 Lời cuối nó nói"*,
    // *"Lệnh phiên chạy không được"*), và đọc "khối khung cuối cùng" trên chuỗi
    // đã trộn là đọc nhầm khu — đúng lỗi 18/08 làm mất hai nút ⏎/⌫.
    let box_anchor = data
        .box_text
        .clone()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| prompt_line_text(&crate::telegram::strip_markdown(text)));
    // 🔴 HỎI ĐÚNG CÂU HỎI MÀ CHỖ NEO SẼ HỎI — Hà 2026-08-17: *"Dòng lệnh in hai
    // biến thể trùng nhau trong cùng một tin"*.
    //
    // "Có trên màn không" ở đây từng đo bằng `text.contains(cmd)`, còn chỗ chèn
    // liên kết (`html_with_links`) đo bằng `line_carries` — thứ khớp cả DÒNG BỊ
    // CỬA SỔ BẺ ĐÔI. Hai phép đo, một câu hỏi: với một lệnh dài, `contains` nói
    // "không thấy" nên hàm này chép nguyên văn nó xuống cuối tin, trong khi
    // `line_carries` vẫn bám được vào nửa đầu trên màn và gắn ▶️ ở đó. Kết quả
    // đúng như Hà đọc: MỘT lệnh, hai biến thể, hai chỗ bấm, trong một tin.
    //
    // Và cái nhãn cũ (*"cổng quyền chặn"*) nói một NGUYÊN NHÂN mà huba không đo
    // được: dòng vắng mặt vì bị chặn, vì màn đã cuộn qua, hay vì nó chỉ nằm
    // trong nhật ký — ba chuyện khác nhau. Nói đúng thứ biết chắc.
    let missing: Vec<&String> = cmds
        .iter()
        .filter(|c| !text.lines().any(|l| line_carries(l, c)))
        .collect();
    let text = if missing.is_empty() {
        text.to_string()
    } else {
        let mut t = text.trim_end().to_string();
        t.push_str("\n\nLệnh của phiên, không thấy trên màn (lấy từ nhật ký):\n");
        // 🔴 CẮT NGẮN CHỖ HIỆN, KHÔNG CẮT CHỖ CHẠY — Hà 2026-08-19, ảnh một
        // `/shot` của `[dwork]`: *"Tin này hiển thị loạn thế"*.
        //
        // Khu này in NGUYÊN VĂN từng dòng lệnh, mà lệnh của phiên dwork là
        // những chuỗi 200+ ký tự (`cd … && git rebase … 2>&1 | tail -15; echo
        // "REBASE_EXIT=$?"; git status --short | head -10`). Trên màn 390px mỗi
        // dòng ấy gãy thành sáu bảy hàng, bốn dòng lệnh thành một bức tường —
        // và cái tin còn bị cắt làm hai phần vì quá dài.
        //
        // Cắt được là nhờ ĐÍCH CHẠM KHÔNG ĐỌC CHỮ NÀY: `run_<mã>` tra dòng lệnh
        // trong SỔ (`remember_quick`), nên nút vẫn chạy trọn vẹn dòng gốc. Chữ
        // ở đây chỉ để NHẬN RA nó là lệnh nào — và `…` là lời khai rằng còn nữa,
        // không phải một dòng lệnh khác.
        //
        // ⚠ Đừng đem phép cắt này sang khu neo trong màn: ở đó chuỗi hiện RA
        // chính là chuỗi `line_carries` đi tìm, cắt nó là neo mất chỗ bám.
        for c in &missing {
            t.push_str(&crate::exec::truncate(c, SHOWN_CMD_MAX));
            t.push('\n');
        }
        t
    };
    // Bẻ thanh tab SAU khi mọi phép đo đã xong (neo ⏎/⌫ ở trên đo trên chữ gốc)
    // và TRƯỚC khi gắn neo — đây là bước định dạng cuối cùng.
    let text = split_tab_bar(&text, &data.tabs);
    let text = text.as_str();

    let is_cmd_btn = |d: &str| d.starts_with("run:") || d == "upgrade";
    let cmd_btns: Vec<(String, String)> = buttons
        .iter()
        .filter(|(_, d)| is_cmd_btn(d))
        .cloned()
        .collect();
    let mut rest_btns: Vec<(String, String)> = buttons
        .iter()
        .filter(|(_, d)| !is_cmd_btn(d))
        .cloned()
        .collect();
    // 🔴 Gột markdown TRƯỚC, rồi mới định vị dòng lệnh. Đây là chữ Telegram sẽ
    // hiển thị, nên nó là chữ duy nhất phép định vị được phép đọc: bản trước
    // cắt trên chữ CHƯA gột rồi mới gột từng mẩu, và một mẩu chỉ chứa dòng rào
    // ``` gột xong là RỖNG — Telegram trả `message text is empty` (đo thật
    // 2026-08-14 08:58:32). Đo ở đúng chỗ Telegram đo thì ca ấy không dựng lên
    // được nữa.
    // 🔴 GỘT KHUNG Ở ĐÂY, KHÔNG SỚM HƠN MỘT DÒNG NÀO — Hà 2026-08-23: *"sao nội
    // dung tin không cắt bỏ các ký tự thừa thãi này đi, để làm gì?"*.
    //
    // Đúng chỗ này vì hai vế, và cả hai đều phải đúng cùng lúc:
    // ① MỌI tin mang chữ của phiên đều chảy qua `session_layout` (nó chỉ có một
    //    chỗ gọi thật: `say_session_data_at`), và cả hai nhánh thoát — có liên
    //    kết hay không — đều dựng từ CÙNG biến `shown` này;
    // ② mọi phép dò khung đã xong TRƯỚC dòng này: `box_anchor` (qua
    //    `prompt_line_text` → `keys::box_region`, thứ NEO vào chính mấy vạch
    //    ấy) và `split_tab_bar` (đòi `←` với `→`) đều đọc `text` ở trên. Gột
    //    sớm hơn là làm mù đúng chỗ đọc ô nhập — cái đã trả giá một lần bằng
    //    khối kết quả nằm lại trong ô nhập hơn một tiếng.
    //
    // ⚠ Và chỉ chạm khối Box Drawing: `☐ ☒ ✔ ↪ ❯ ← →` là dấu huba TỰ CHÈN vào
    // chuỗi này ở ngay bước trên (`split_tab_bar`, neo `\t☑`) — chúng nằm ngoài
    // khối ấy, nên bộ gột không với tới. Nới phạm vi là tự xoá nút của mình.
    // Gợi ý bàn phím đi TRƯỚC khung: nhãn `/rc` neo vào cuối dòng, mà
    // `strip_box_rules` tỉa hai đầu dòng — chạy sau nó thì cái neo vẫn còn,
    // nhưng đặt đúng thứ tự đọc (cắt gợi ý → gột khung → gộp dòng trống) thì
    // không ai phải suy xem bước nào làm hỏng neo của bước nào.
    let shown = strip_box_rules(&strip_keyboard_hints(&crate::telegram::strip_markdown(
        text,
    )));

    // `run:<i>` → payload `run_<i>`; nút cài lại huba → `upgrade`. Cùng bộ ký tự
    // mà tên lệnh cho phép, nên payload đi thẳng, không mã hoá gì thêm.
    // 🔴 HAI ĐÍCH CHẠM CHO MỘT DÒNG LỆNH — Hà 2026-08-16: *"kiếm 1 cái icon
    // terminal để biết nó là bấm chạy terminal riêng chứ không phải chạy xong
    // rồi gửi ngược vào phiên, nên tách thành 2 nút này để người dùng chủ động
    // chọn"*.
    //
    // `▶️` = huba chạy bằng `/bin/zsh -lc`, chờ xong, dán bản tóm tắt vào phiên.
    // `🖥` = mở một cửa sổ Terminal và gõ lệnh vào đó, rồi chuyển con trỏ sang.
    // Hai thứ khác nhau ở CÁI CÒN LẠI sau khi chạy, nên chúng phải là hai đích
    // chạm chứ không phải một mặc định huba tự chọn hộ.
    //
    // ⚠ Nút cài lại huba (`upgrade`) chỉ có MỘT đường: nó khởi động lại chính
    // hubad, và làm việc ấy trong một cửa sổ rời thì cái mồm báo tin chết giữa
    // câu — đúng con bug đã trả giá ngày 16/08 (xem `quick_buttons`).
    let link_of = |i: usize| -> Vec<(String, String)> {
        let Some((_, data)) = cmd_btns.get(i) else {
            return Vec::new();
        };
        if data == "upgrade" {
            return crate::telegram::deep_link("upgrade")
                .map(|href| (href, "🔧".to_string()))
                .into_iter()
                .collect();
        }
        let tok = data.trim_start_matches("run:");
        // Nhãn cho cửa THỨ HAI, vì nó là cái duy nhất còn nằm ngoài thẻ `<a>`
        // bọc dòng lệnh — mà một emoji trần là đích chạm rộng đúng 2 ký tự,
        // đúng thứ Hà vừa kêu. `▶️` thì không cần nhãn: nó đã đi vào TRONG thẻ
        // cùng cả dòng lệnh (xem `html_with_links`).
        [("run_", "▶️"), ("term_", "🖥 cửa sổ")]
            .iter()
            .filter_map(|(p, icon)| {
                crate::telegram::deep_link(&format!("{p}{tok}"))
                    .map(|href| (href, icon.to_string()))
            })
            .collect()
    };
    // 🔴 Ô NHẬP CŨNG LÀ MỘT NEO — Hà 2026-08-16: *"chèn vào đúng chỗ gõ lệnh đó
    // nếu có text 2 nút là được"* · *"1 nút enter 1 nút xóa"* · *"còn lăn tăn nó
    // là text mờ hay tỏ thì thêm 1 nút xóa bên cạnh nữa để tự thao tác"*.
    //
    // Chữ đang nằm trong ô nhập hiện ra trong tin như một dòng bình thường; hai
    // nút của nó phải nằm NGAY TẠI dòng ấy. Và vì huba không phân biệt được chữ
    // thật với gợi ý mờ (đọc màn về là chữ mất màu), nó KHÔNG đoán: đưa cả hai
    // đường ra cạnh nhau, người đang nhìn màn hình quyết.
    //
    // Mã phiên nằm trong chính liên kết (`send_<sid>` / `clr_<sid>`), không lấy
    // theo con trỏ — bấm lại một tin cũ vẫn chạm đúng phiên của nó.
    let mut neo_cuoi: Vec<usize> = Vec::new();
    let mut anchors: Vec<(String, Vec<(String, String)>)> = cmds
        .iter()
        .enumerate()
        .map(|(i, c)| (c.clone(), link_of(i)))
        .collect();
    let key_sid = if data.sid.is_empty() {
        buttons.iter().find_map(|(_, d)| {
            d.strip_prefix("key:")
                .and_then(|r| r.split_once(':'))
                .map(|(sid, _)| sid.to_string())
        })
    } else {
        Some(data.sid.clone())
    };
    // 🔴 LỰA CHỌN → ☑ NGAY TẠI DÒNG CỦA CHÍNH NÓ. Hà 2026-08-16: *"nút chọn phải
    // chèn ngay tại các dòng chọn tại chính chỗ option chứ không phải ném thêm
    // xuống cuối"*. Neo là NHÃN lựa chọn — chuỗi đang hiện ngay trên màn.
    if key_sid.is_some() {
        let short = data.short();
        for (n, label) in &data.choices {
            // 🔴 BẢNG NHIỀU CÂU đi bằng `pick_`, hộp một câu đi bằng `k_`. Cùng
            // một dòng lựa chọn trên màn, hai đường gửi khác nhau — xem
            // `CLAUDE.md §7` (`/key <số>` là ngõ cụt với bảng nhiều câu).
            //
            // Chỗ gọi khai `n` là "số lựa chọn" hay "câu.lựa chọn" bằng chính
            // hình dạng chuỗi: có dấu chấm ⟹ bảng nhiều câu.
            let payload = match n.split_once('.') {
                Some((q, o)) => format!("pick_{short}_{q}_{o}"),
                None => format!("k_{short}_{n}"),
            };
            if let Some(href) = crate::telegram::deep_link(&payload) {
                // TAB ở đầu nhãn = chèn TRƯỚC dòng, tức trước `1.` (Hà 17/08:
                // *"Chèn phía trước số mỗi dòng"*) — xem `html_with_links`.
                anchors.push((label.clone(), vec![(href, "\t☑".to_string())]));
            }
        }
    }
    // 🔴 MỖI TAB MỘT ĐÍCH CHẠM, NGAY TẠI NHÃN CỦA NÓ — Hà 2026-08-19: *"Sao
    // không chèn nút trực tiếp ở phần nội dung lại đi chèn thêm nút ở cuối"*.
    //
    // Bám được là nhờ [`split_tab_bar`] vừa bẻ thanh tab thành mỗi tab một
    // dòng; trước đó ba nhãn nằm chung một dòng nên chỉ một cái có chỗ neo.
    if !data.tabs.is_empty() {
        let short = data.short();
        for (n, label, _) in &data.tabs {
            if let Some(href) = crate::telegram::deep_link(&format!("tab_{short}_{n}")) {
                anchors.push((label.clone(), vec![(href, "\t↪".to_string())]));
            }
        }
    }
    // …và lựa chọn đã có ☑ trong chữ thì THÔI nằm ở đáy. Một việc, một chỗ bấm:
    // hai đường cho cùng một lựa chọn thì cái ở đáy chỉ mang con số trần, còn bị
    // Telegram cắt nhãn ở 52 ký tự (`☐ 1 Khô`) — đúng thứ Hà đọc được 17/08.
    //
    // Luật này nằm ở ĐÂY chứ không ở từng chỗ gọi: `/shot` đã chép tay nó một
    // lần, và tin tự phát thì quên — cùng hình dạng "vá một chỗ, sót chỗ bên
    // cạnh" đã lặp nhiều lần trong tệp này. Giữ `:enter`/`:clear` (ô nhập, không
    // phải lựa chọn) và `pick:` (bảng nhiều câu — các câu sau không có neo nào
    // trong chữ để mà chèn).
    if !data.choices.is_empty() {
        rest_btns.retain(|(_, d)| {
            !d.starts_with("key:") || d.ends_with(":enter") || d.ends_with(":clear")
        });
    }
    // …và tab đã có ↪ trong chữ thì THÔI nằm ở đáy. Cùng luật, cùng lý do: hai
    // đường cho một việc thì cái ở đáy chỉ là tiếng ồn, và nhãn của nó bị
    // Telegram cắt ở 52 ký tự.
    if !data.tabs.is_empty() {
        rest_btns.retain(|(_, d)| !d.starts_with("tab:"));
    }
    // ✅ NGAY TẠI DÒNG `Submit` — đường gửi của hộp CHỌN NHIỀU.
    //
    // Neo là chính chữ `Submit` trên màn. Không bám được (màn đổi, chữ khác) ⟹
    // `html_with_links` báo `unlinked` và chỗ gọi vẫn còn nút ở đáy: mất một
    // đích chạm thì phải thấy được, không được im.
    if let (true, Some(sid)) = (data.submit, key_sid.as_ref()) {
        let short: String = sid.chars().take(8).collect();
        if let Some(href) = crate::telegram::deep_link(&format!("send_{short}")) {
            anchors.push(("Submit".to_string(), vec![(href, "\t✅".to_string())]));
        }
    }
    // 📎 NGAY TẠI TÊN TỆP. Neo là đường dẫn đúng như nó hiện trong chữ, nên chỗ
    // gọi phải lấy nó từ chính chữ ấy (`keys::paths_on_screen`), không tự dựng
    // lại từ đường dẫn tuyệt đối — dựng lại là neo không bám được và liên kết
    // rơi xuống đáy tin, đúng chỗ nó vừa được gỡ khỏi.
    for (path, n) in &data.files {
        if let Some(href) = crate::telegram::deep_link(&format!("f_{n}")) {
            anchors.push((path.clone(), vec![(href, "📎".to_string())]));
        }
    }
    if let (Some(sid), Some(box_text)) = (&key_sid, box_anchor) {
        let short: String = sid.chars().take(8).collect();
        // 🔴 CHỈ CÒN ⏎ — Hà 2026-08-25: *"nút xóa ô nhập không cần thiết vì có
        // lệnh xóa rồi"*. Hai đích chạm cạnh nhau, một bên GỬI một bên XOÁ, cả
        // hai đều không lùi lại được — bỏ được cái nào là bớt một cú bấm nhầm
        // không sửa được. Đường xoá vẫn còn nguyên bằng lệnh gõ.
        let links: Vec<(String, String)> = [("send_", "⏎")]
            .iter()
            .filter_map(|(p, icon)| {
                crate::telegram::deep_link(&format!("{p}{short}"))
                    .map(|href| (href, icon.to_string()))
            })
            .collect();
        // 🔴 NEO PHẢI CHỈ ĐÚNG MỘT CHỖ. Hà 2026-08-18, ảnh chụp một tin `/shot`:
        // *"Sao lại chèn lệnh /clear vào ô chat"* — hai nút ⏎/⌫ dán vào GIỮA
        // đoạn văn, ngay sau chữ `/clean` trong một câu tôi viết, chứ không nằm
        // ở dòng ô nhập dưới đáy.
        //
        // Vì sao: ô nhập lúc ấy chứa đúng `/clean`, và neo là CHUỖI ấy —
        // `html_with_links` duyệt từ dòng đầu nên nó bám vào chỗ khớp ĐẦU TIÊN.
        // Chữ trong ô nhập càng ngắn thì càng dễ trùng với chữ đang bàn về nó,
        // và phiên `[huba]` thì nói về lệnh của huba suốt.
        //
        // Khớp nhiều dòng ⟹ KHÔNG neo giữa chữ, để hai cái nút ở đáy tin (đường
        // lùi vẫn còn nguyên). Thà nút đứng xa một chút còn hơn nút chỉ sai chỗ:
        // ⌫ ở nhầm dòng mời người ta xoá một thứ không phải ô nhập.
        // 🔴 KHÔNG BỎ CUỘC NỮA — BÁM LẦN KHỚP CUỐI. Hà 2026-08-25, ảnh một tin
        // có `❯ ssh vps-a "curl -s http://…"` mà không nút ⏎: *"sao ô chờ gợi ý
        // mờ lại không có nút enter"*.
        //
        // Log của chính huba đã nói: `box_anchor_ambiguous {hits: 4}` — chuỗi
        // trong ô nhập trùng 4 dòng, vì phiên vừa chạy đúng lệnh ấy nên nó còn
        // nằm trong phần hội thoại phía trên. Cửa cũ (`hits == 1`) vì thế đóng,
        // và nút rơi xuống đáy tin — nơi nó không nói được nó thuộc dòng nào.
        //
        // Cái mập mờ ấy là BÁO ĐỘNG GIẢ, và chỗ này biết thế: ô nhập không phải
        // "một chỗ nào đó có chuỗi này", nó là **dòng dấu nhắc cuối cùng** —
        // đúng định nghĩa `prompt_line_text` dùng để đọc ra `box_text` ngay ở
        // trên. Màn cuộn từ trên xuống nên mọi bản trùng đều nằm PHÍA TRÊN.
        //
        // Tức bản vá 18/08 né hậu quả (thấy trùng thì bỏ neo) trong khi dữ kiện
        // để trị gốc đã nằm sẵn trong tay: huba đo được vị trí, rồi vứt vị trí
        // đi và đưa `html_with_links` một CHUỖI để dò lại từ đầu tin. Nay khai
        // thẳng "neo này bám lần khớp cuối" (`html_with_links_last`).
        //
        // Cái KHÔNG đổi: luật 18/08 vẫn đúng nguyên văn — *"neo nhầm dòng thì cú
        // Enter đi vào một dòng KHÔNG phải ô nhập"*. Bám cuối là cách THOẢ nó,
        // không phải cách bỏ nó.
        let hits = shown
            .lines()
            .filter(|l| line_carries(l, box_text.trim()))
            .count();
        if hits > 1 {
            logging::info(
                "box_anchor_repeated",
                json!({ "hits": hits, "chars": box_text.trim().chars().count(),
                        "why": "chữ trong ô nhập trùng chỗ khác trên màn — neo bám DÒNG CUỐI, không bỏ cuộc" }),
            );
        }
        if links.len() == 1 && !box_text.trim().is_empty() {
            neo_cuoi.push(anchors.len());
            anchors.push((box_text.trim().to_string(), links));
            // …và bỏ cái nút ⏎/⌫ trơn ở đáy: hai đường cho cùng một việc, mà
            // cái ở đáy là cái không nói được nó thuộc về dòng nào.
            //
            // 🔴 CHỈ hai cái ấy. Bản đầu viết `!d.starts_with("key:")` và nó
            // quét sạch **nút số 1–9** — chúng cũng đi bằng `key:<id>:<n>`. Đo
            // được ngay lượt `/shot` thật đầu tiên sau khi cài: tin ra
            // `text_links=3, buttons=0`, tức hộp chọn 5 lựa chọn mà không một
            // nút nào. Hà: *"Sao lại đẩy mớ option vào ô nhập chát thế này?"* —
            // đúng, cả mớ option thành chữ vì cái lọc này ăn mất nút của chúng.
            rest_btns.retain(|(_, d)| !d.ends_with(":enter"));
        }
        // …và nút xoá ở đáy đi theo cái ⌫ vừa gỡ, KHÔNG phụ thuộc neo có bám
        // được hay không: Hà bảo nó thừa, nên nó thừa ở cả hai chỗ.
        rest_btns.retain(|(_, d)| !d.ends_with(":clear"));
    }
    Layout {
        shown,
        anchors,
        cmd_btns,
        rest_btns,
        neo_cuoi,
    }
}

/// Như trên, nhưng chỗ gọi khai RÕ từng loại dữ liệu của phiên.
pub fn say_session_data(
    tg: &crate::telegram::Inbox,
    text: &str,
    buttons: &[(String, String)],
    log_key: &str,
    data: &SessionData,
) {
    let _ = say_session_data_at(tg, text, buttons, log_key, data, None);
}

/// Như trên, nhưng SỬA một tin đã có (`edit`) thay vì gửi tin mới — và trả về
/// `message_id` của tin mang bảng, để lần bấm sau sửa đúng nó.
///
/// 🔴 Hà 2026-08-17: *"Khi bấm ở phản hồi nên sửa tin tại phản hồi đó luôn không
/// cần gửi 1 tin mới"*. Một hộp năm ô là năm cú bấm; mỗi cú một tin thì buồng
/// chat có năm bảng gần giống hệt nhau và bảng ĐÚNG là cái cuối — người đọc phải
/// cuộn để biết mình đang nhìn trạng thái nào.
///
/// Sửa được thì trả lại chính `message_id` ấy. Sửa hỏng (tin quá cũ, Telegram
/// từ chối) ⟹ NÓI ra trong log rồi gửi tin mới: mất chỗ sửa còn hơn mất câu trả
/// lời.
pub fn say_session_data_at(
    tg: &crate::telegram::Inbox,
    text: &str,
    buttons: &[(String, String)],
    log_key: &str,
    data: &SessionData,
    edit: Option<i64>,
) -> Option<i64> {
    // 🔴 MỘT KIỂU THÔI — Hà 2026-08-16: *"tại sao huba chèn lệnh này mà ở các
    // phiên khác lại chèn kiểu button? sao không dùng giống link này?"*
    //
    // Hai kiểu ấy không phải hai lựa chọn thiết kế, chúng là **một kiểu và một
    // đường lùi**: icon `▶️` gắn được vào chữ chỉ khi huba TÌM THẤY dòng lệnh
    // trong tin (`command_slices` cắt tin ngay sau dòng ấy để icon rơi đúng
    // chỗ). Lệnh nào không nằm trong chữ thì không có gì để gắn vào, nên nó rơi
    // xuống một hàng nút ở đáy — đúng cảnh trong ảnh của `[social]`.
    //
    // Và lệnh KHÔNG nằm trong chữ là chuyện thường, không phải ngoại lệ: nhánh
    // *lệnh bị cổng quyền từ chối* đọc lệnh từ NHẬT KÝ, còn phiên thì không hề
    // viết nó ra lời. Nút ở đáy khi ấy vừa lạc chỗ vừa **cắt cụt nhãn ở 52 ký
    // tự**, tức người bấm không đọc được thứ mình sắp chạy.
    //
    // Nên: lệnh nào chưa có trong chữ thì huba **viết thêm nó vào cuối tin**, rồi
    // gắn icon như mọi lệnh khác. Một kiểu duy nhất, và nguyên văn dòng lệnh
    // luôn hiện ra.
    //
    // Toàn bộ khúc dựng neo nằm ở [`session_layout`] — DÙNG CHUNG với
    // `render_session_data`, để bài kiểm gửi tin thật đo đúng đường này.
    let Layout {
        shown,
        anchors,
        cmd_btns,
        rest_btns,
        neo_cuoi,
    } = session_layout(text, data, buttons);
    let (html, linked, unlinked) = html_with_links_last(&shown, &anchors, &neo_cuoi);

    // Lệnh nào không dựng được liên kết thì rơi về NÚT ở đáy — đường lùi cũ,
    // giữ nguyên: một liên kết không bấm được thì tệ hơn một cái nút.
    let mut row: Vec<(String, String)> = unlinked
        .iter()
        .filter_map(|i| cmd_btns.get(*i))
        .map(|(_, data)| {
            let icon = if data == "upgrade" { "🔧" } else { "▶" };
            (icon.to_string(), data.clone())
        })
        .collect();
    row.extend(rest_btns.clone());

    if linked == 0 {
        // Không chèn được liên kết nào: chữ thuần + nút như cũ. Không đi đường
        // HTML ở đây — chữ này chưa qua tay ai, và bật `parse_mode` cho một
        // chuỗi không cố ý mang thẻ là mời Telegram từ chối cả tin.
        //
        // 🔴 TRỪ khi trong chữ có một `/<chữ>` mà Telegram sẽ tô thành lệnh bot:
        // ca ấy phải đi đường HTML, vì cái bẫy nằm ở chỗ KHÔNG có liên kết nào
        // để mà đi cửa kia — đúng cái tin trơn Hà chạm nhầm. Escape rồi chỉ thêm
        // `<code>`, nên chuỗi vẫn là chuỗi cũ về mặt nội dung.
        let escaped = crate::telegram::html_escape(&shown);
        let tamed = tame_auto_links(&escaped);
        if tamed != escaped {
            let parts = split_for_telegram(&tamed);
            let last = parts.len().saturating_sub(1);
            for (n, p) in parts.into_iter().enumerate() {
                let sent = if n == last && !buttons.is_empty() {
                    tg.send_html_report(&p, buttons).map(|_| ())
                } else {
                    tg.send_html(&p)
                };
                if let Err(e) = sent {
                    logging::error(log_key, json!({ "err": e, "slice": n, "html": true }));
                }
            }
            return None;
        }
        let parts = split_for_telegram(&shown);
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
        return None;
    }

    // MỘT tin — trừ khi dài quá trần Telegram, và lúc ấy cắt theo DÒNG nên
    // không thẻ nào bị đứt đôi (liên kết luôn nằm gọn trong dòng lệnh của nó).
    // Bàn phím đi cùng tin CUỐI, trong cùng một lời gọi.
    let mut chunks = split_for_telegram(&html);
    let tail = chunks.pop().unwrap_or_default();
    let chunks_len = chunks.len();
    for (k, head) in chunks.into_iter().enumerate() {
        if let Err(e) = tg.send_html(&head) {
            logging::error(log_key, json!({ "err": e, "chunk": k }));
        }
    }
    // Sửa tại chỗ khi chỗ gọi biết tin nào đang mang bảng — chỉ làm được với tin
    // MỘT MẨU: bảng bị cắt đôi thì sửa mẩu cuối để lại mẩu đầu nói chuyện cũ.
    if let (Some(mid), true) = (edit, chunks_len == 0) {
        match tg.edit_html(mid, &tail, &row) {
            Ok(_) => return Some(mid),
            Err(e) => logging::info(
                "telegram_edit_fell_back_to_new",
                json!({ "message_id": mid, "why": e }),
            ),
        }
    }
    match tg.send_html_report(&tail, &row) {
        Ok(sent) => Some(sent.message_id),
        Err(e) => {
            logging::error(
                log_key,
                json!({ "err": e, "chunk": "tail", "linked": linked }),
            );
            None
        }
    }
}

/// Cắt chữ cho vừa MỘT tin Telegram — theo DÒNG, để không đứt giữa câu.
///
/// Luôn trả về ít nhất một mẩu (có thể rỗng), nên chỗ gọi `pop()` được mà không
/// phải kiểm rỗng.
///
/// 🔴 CẮT LÀM ĐÔI THÌ PHẢI NÓI RA — Hà 2026-08-17: *"Tin dài bị cắt làm hai mẩu,
/// mẩu sau không có dấu nối nên đọc như tin lạc"*.
///
/// Buồng chat không có khái niệm "trang 2": hai tin rời nằm cạnh nhau, và mẩu
/// sau bắt đầu giữa câu, thường là giữa một danh sách. Người đọc trên điện thoại
/// gặp nó trước khi kịp cuộn lên, nên nó đọc như một tin của chuyện khác — tệ
/// nhất đúng lúc tin dài, tức đúng lúc nội dung đáng đọc.
///
/// Dấu nối là chữ THUẦN, không thẻ: mẩu này đi cả đường HTML lẫn đường chữ trần,
/// và một cái thẻ lọt vào đường chữ trần thì Telegram in ra nguyên con.
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
    // Mẩu rỗng là một tin Telegram từ chối (`message text is empty`) — giữ lại
    // đúng một mẩu để hợp đồng "luôn có ít nhất một" không đổi.
    if parts.iter().any(|p| !p.trim().is_empty()) {
        parts.retain(|p| !p.trim().is_empty());
    } else {
        parts.truncate(1);
    }
    let n = parts.len();
    if n < 2 {
        return parts;
    }
    parts
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            let mut out = String::new();
            if i > 0 {
                out.push_str(&format!("⋯ mẩu {}/{n}, nối tiếp tin trên\n", i + 1));
            }
            out.push_str(&p);
            if i + 1 < n {
                out.push_str(&format!("⋯ còn mẩu {}/{n} ở tin dưới\n", i + 2));
            }
            out
        })
        .collect()
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
/// `skip_current` = câu ĐANG HIỆN trên màn đã có ☑ ngay tại dòng của nó, nên
/// đừng liệt kê lại nó ở cuối tin.
///
/// 🔴 Hà 2026-08-17, ảnh một `/shot` có đủ bốn lựa chọn trên màn RỒI lại thêm
/// bốn dòng `/pick_…` ở cuối: *"Sao không chèn trực tiếp vào văn bản lại đi chèn
/// thêm xuống cuối"*. Khu chữ ấy ra đời khi chưa có cách chèn vào giữa dòng; nay
/// có rồi thì nó thành bản sao thứ hai của cùng một danh sách, dài gấp đôi và
/// bắt mắt đọc hai lần.
///
/// Cái nó CÒN việc để làm: các câu SAU của một bảng nhiều câu — chúng chưa hiện
/// trên màn nên không có dòng nào để neo — và dòng `/send_…`.
pub fn ask_command_lines(
    session_id: &str,
    a: &crate::sessions::Asking,
    skip_current: bool,
) -> String {
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
        if options.is_empty() || (skip_current && qi == 0) {
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
        // 09:06, Telegram gửi mỗi `/key`, huba trả *"Chưa hiểu lệnh này"*. Tôi
        // tự viết ra luật "chạm chỉ gửi lại token lệnh, chữ sau dấu cách rơi
        // mất" ở ngay tệp bên cạnh, rồi dẫm đúng vào nó ở dòng này.
        //
        // Tham số phải nằm TRONG tên: `/send_<8 ký tự đầu id>`.
        out.push_str(&format!("\n\nTrả lời hết rồi gửi: /send_{sid}"));
    }
    out
}

/// Dòng lệnh này có phải là "huba dựng lại chính huba" không?
///
/// Hàng rào HẸP có chủ ý — đây là danh sách những đường cài lại hubad trên máy
/// này (`./huba self-install` là bản Rust, `install_update.sh` là bản shell nó
/// thay thế). Nới rộng bằng cách bắt mọi thứ có chữ "install" thì `npm install`
/// cũng thành "dựng lại huba", và người bấm nhận một câu trả lời nói về chuyện
/// khác hẳn.
///
/// 🔴 Tên cũ `deploy/install.sh` VẪN nhận, và đây không phải sự nhân nhượng:
/// nó còn nằm trong những tin Telegram đã gửi đi, trong sổ lệnh gợi ý, và
/// trong ngón tay chủ máy. Một cái nút cũ bấm vào mà huba không nhận ra thì tệ
/// hơn một dòng thừa ở đây.
pub fn is_self_rebuild(cmd: &str) -> bool {
    let c = cmd.trim();
    c.contains("huba self-install")
        || c.contains("install_update.sh")
        || c.contains("deploy/install.sh")
}

/// Lệnh gợi ý thứ `n`, kèm PHIÊN đã sinh ra nó — cái nút chỉ mang con số.
///
/// Trả `None` khi sổ cũ (dạng mảng trần, chưa có tên phiên): thà bắt bấm lại
/// `/shot` còn hơn gõ một dòng lệnh vào một phiên đoán bừa.
pub fn quick_cmd(db: &Db, key: &str) -> Option<(String, crate::sessions::Cmd)> {
    let v = db.cursor_or_log(QUICK_KEY)?;
    let st: serde_json::Value = serde_json::from_str(&v).ok()?;

    // Sổ MỚI: tra bằng mã, và mã mang theo cả phiên lẫn thư mục của nó.
    if let Some(e) = st.get("e").and_then(|e| e.as_object()) {
        let row = e.get(key)?;
        return Some((
            row.get("s")?.as_str()?.to_string(),
            crate::sessions::Cmd {
                line: row.get("l")?.as_str()?.to_string(),
                cwd: row
                    .get("d")
                    .and_then(|d| d.as_str())
                    .unwrap_or_default()
                    .to_string(),
            },
        ));
    }

    // Sổ CŨ (một ô, tra bằng số thứ tự) — chỉ còn để những cái nút đã gửi đi
    // trước bản vá không chết câm. Nó mang đúng khuyết tật Hà bắt được
    // (`run:0` = "lệnh đầu của tin GẦN NHẤT"), nên nó sẽ tự hết khi 40 mã mới
    // đẩy hết chỗ; không nới thêm đường nào cho nó.
    let n: usize = key.parse().ok()?;
    let sid = st.get("s")?.as_str()?.to_string();
    let raw = st.get("c")?.as_array()?.get(n)?;
    let cmd = match raw {
        serde_json::Value::String(s) => crate::sessions::Cmd {
            line: s.clone(),
            cwd: String::new(),
        },
        v => serde_json::from_value(v.clone()).ok()?,
    };
    logging::warn(
        "quick_cmd_legacy_index",
        json!({ "n": n, "session": sid,
                "why": "nút cũ tra bằng số thứ tự — trỏ vào tin GẦN NHẤT, không phải tin sinh ra nó" }),
    );
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

/// `/tab <n>` — đưa con trỏ sang tab thứ `n` của bảng hỏi, KHÔNG chọn gì.
///
/// 🔴 Hà 2026-08-19: *"muốn chuyển tab thì bấm phím phải trái, giờ qua tele thì
/// có nút bấm ở chính tab để nhận như click chuột"*.
///
/// Ba việc nó KHÔNG làm, mỗi việc là một cái bẫy đã đo:
/// * **Không đi bằng `do script`.** Lượt ghi ấy kèm một CR không tắt được, và
///   trên hộp chọn CR là một cú CHỐT — 2026-08-19 một cú Enter lạc đã chốt
///   `☐ RPC pool` → `☒` trên đúng bảng này. Phím ngang đi bằng
///   [`crate::cgkeys`], không kèm gì.
/// * **Không đoán con trỏ đang ở đâu.** Tab hiện hành vẽ bằng màu, mà đọc màn về
///   chỉ có chữ trần. Nên nó về mép trái rồi đếm — xem `keys::tab_keys`.
/// * **Không tin là bảng có thật.** Không đọc ra thanh tab ⟹ từ chối, vì gửi
///   mũi tên vào một màn không phải bảng hỏi là gõ vào việc của người khác.
fn tab_move(s: &crate::sessions::LiveSession, w: i64, arg: &str) -> Result<(), String> {
    let name = crate::sessions::shown(s);
    let n: usize = arg.trim().parse().map_err(|_| {
        format!(
            "⚠ `/tab` cần một con số, ví dụ `/tab 2` — nhận được '{}'",
            crate::exec::truncate(arg.trim(), 40)
        )
    })?;
    let body = match crate::keys::look(&s.tty, PICK_LINES) {
        crate::keys::Look::Saw { body, .. } => body,
        crate::keys::Look::Blind { why } => {
            logging::warn(
                "tab_refused_blind",
                json!({ "session": s.session_id, "why": why }),
            );
            return Err(format!(
                "⚠ Không đọc được màn của {name} ({why}) — nên tôi KHÔNG bấm gì cả."
            ));
        }
    };
    // 🔴 ĐỌC CÙNG BỀ NGANG VỚI LÚC DỰNG NÚT. Cái nút `tab n` đi ra điện thoại
    // từ ảnh chụp, mà ảnh chụp NỚI cửa sổ khi màn bị mép cắt (`shot_grew_window`
    // ngay dưới); đường này thì đọc bằng `look` → `screen_text`, không nới. Nên
    // chấm cú bấm trên bản đọc hẹp là có ngày trả lời *"bảng chỉ có 2 câu, không
    // có câu 3"* về đúng cái nút huba vừa tự dựng ra.
    //
    // `want` lấy con số lớn hơn giữa sổ phiên và chính số tab vừa bấm — cả hai
    // đều là mệnh đề của huba, không phải số bịa. Xem `keys::tab_bar_cut`.
    let tu_so = s.asking.as_ref().map(|a| a.rest.len() + 1).unwrap_or(0);
    let Some(table) = crate::keys::ask_table_wide(&body, w, tu_so.max(n)).0 else {
        return Err(format!(
            "⚠ Màn của {name} không có bảng hỏi nhiều câu nào đang mở, nên không có tab để sang."
        ));
    };
    let tabs = table.answered.len();
    if n > tabs {
        return Err(format!(
            "⚠ Bảng của {name} có {tabs} câu, không có câu {n}. (`/tab 0` là bước gửi.)"
        ));
    }
    let keys = crate::keys::tab_keys(tabs, n);
    if let Err(e) = crate::keys::send_bare(w, &keys) {
        logging::warn(
            "tab_send_failed",
            json!({ "session": s.session_id, "keys": keys, "err": e.to_string() }),
        );
        return Err(format!(
            "⚠ Không đi sang tab {n} được: {}",
            crate::exec::truncate(&e.to_string(), 300)
        ));
    }
    logging::info(
        "tab_moved",
        json!({ "session": s.session_id, "to": n, "tabs": tabs, "keys": keys.len() }),
    );
    // TUI vẽ sau khi nhận phím; đọc lại ngay là đọc màn cũ rồi báo "không đổi".
    std::thread::sleep(std::time::Duration::from_millis(1200));
    Ok(())
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
        // 🪦 Nhánh `Withheld` gỡ 2026-08-16: nó từ chối cú bấm bằng câu *"màn có
        // dấu hiệu bí mật nên huba không đọc được chữ"* — về đúng cái màn mà
        // `/shot` vừa gửi nguyên lên điện thoại. Xem bia mộ trong `keys::Look`.
        crate::keys::Look::Blind { why } => {
            logging::warn(
                "pick_refused_blind",
                json!({ "session": s.session_id, "why": why }),
            );
            return format!("⚠ Không đọc được màn của {name} ({why}) — nên tôi KHÔNG bấm gì cả.");
        }
    };

    // Cùng cửa nới với `/tab`: bản đọc hẹp có thể cắt mất tab, và ở đây cái giá
    // là từ chối một cú bấm hợp lệ bằng câu *"bảng chỉ có N câu"*. `want` lấy
    // con số lớn hơn giữa sổ phiên và chính câu vừa bấm — xem `keys::tab_bar_cut`.
    let (table, rong) = crate::keys::ask_table_wide(&body, w, questions.len().max(q));
    // Nới rồi thì chấm con trỏ trên CHÍNH cái màn vừa đọc ra bảng: đếm ô trống
    // trên bản rộng mà tìm con trỏ trên bản hẹp là hỏi hai cái màn khác nhau.
    let mut da_noi = rong.is_some();
    let body = rong.unwrap_or(body);
    let total = table.as_ref().map(|t| t.answered.len()).unwrap_or(1);
    if q > total {
        return format!(
            "⚠ Bảng của {name} có {total} câu, không có câu {q}. (Bảng một câu thì dùng `/key`.)"
        );
    }
    // Không có thanh tab ⟹ bảng MỘT câu ⟹ không có gì để đi tới; `/pick 1.x`
    // vẫn chạy được và trùng đúng nghĩa với `/key x`.
    // 🔴 KHỚP HỤT THÌ NHÌN RỘNG HƠN TRƯỚC KHI BỎ CUỘC.
    //
    // `cursor_on` tìm NGUYÊN VĂN câu hỏi (lấy từ nhật ký) trong chữ trên màn, vì
    // câu ĐANG MỞ là câu duy nhất CLI in đủ ra dưới thanh tab. Ở `24×80` một bảng
    // nhiều câu với lựa chọn dài (mỗi lựa chọn bẻ đôi ở cột 80) đẩy dòng câu hỏi
    // cuộn khỏi mép trên — và lúc ấy huba từ chối một cú bấm hoàn toàn hợp lệ.
    //
    // Cửa nới **đã có** (`ask_table_wide`) nhưng nó hỏi câu khác: chỉ nới khi
    // THANH TAB bị cắt. Tab đủ mà câu hỏi thiếu thì cửa ấy đóng. Hai triệu chứng
    // của cùng một cái màn hẹp, mới có một cái được canh.
    //
    // ⚠ Và đây KHÔNG phải thủ phạm của lượt 16:45 ngày 26/08 (ảnh Hà gửi). Log
    // của chính lượt ấy nói `questions:1 total:2` — sổ thiếu câu, không phải màn
    // thiếu chữ; xem lời từ chối bên dưới, nơi hai trạng thái ấy được tách ra.
    // Ghi rõ chỗ này để người đọc sau đừng tưởng cửa nới đã trị được ca đó.
    //
    // Nới rồi thì `da_noi = true`: phần đọc-lại-sau-khi-gõ ở dưới BẮT BUỘC dùng
    // cùng bề ngang, nếu không thì đếm ô trống trên hai cái màn khác nhau và trả
    // lời sai, tự tin, về đúng cú bấm vừa rồi.
    let mut vi_tri = crate::keys::cursor_on(&body, &questions);
    if vi_tri.is_none() && total > 1 && !da_noi {
        match crate::keys::screen_text_tall(w, crate::keys::GROW_ASK) {
            Ok(rong2) if !rong2.trim().is_empty() => {
                vi_tri = crate::keys::cursor_on(&rong2, &questions);
                logging::info(
                    "pick_cursor_regrown",
                    json!({ "session": s.session_id, "questions": questions.len(),
                            "khop": vi_tri.is_some() }),
                );
                // Chỉ cần ghi nhớ ĐÃ NỚI: `body` xong việc ngay sau phép khớp
                // này, còn `da_noi` thì đi tiếp xuống phần đọc-lại-sau-khi-gõ và
                // quyết định bề ngang của nó.
                if vi_tri.is_some() {
                    da_noi = true;
                }
            }
            // Không nới được thì nói ra rồi rơi về câu từ chối bên dưới — đừng
            // im, vì lúc ấy lời từ chối sẽ đổ cho "không khớp" trong khi thủ
            // phạm là cửa sổ không nới được.
            Ok(_) => logging::warn(
                "pick_cursor_grow_empty",
                json!({ "window": w, "effect": "nới xong đọc ra rỗng — chấm trên bản hẹp" }),
            ),
            Err(e) => logging::warn(
                "pick_cursor_grow_failed",
                json!({ "window": w, "err": logging::err_chain(&e),
                        "effect": "không nới được cửa sổ — chấm trên bản hẹp" }),
            ),
        }
    }
    let cursor = match vi_tri {
        Some(c) => c,
        None if total == 1 => 0,
        None => {
            logging::info(
                "pick_cursor_unknown",
                json!({ "session": s.session_id, "questions": questions.len(), "total": total }),
            );
            // 🔴 NÓI RA CÁI LỆCH, ĐỪNG NÓI "KHÔNG KHỚP" — Hà 2026-08-26, ảnh
            // buồng chat 16:45: *"chưa chọn được, có phải do chưa vào phiên
            // không?"*. Câu cũ để anh đoán, và đoán ra một nguyên nhân sai (chưa
            // vào phiên), vì nó giấu đúng con số phân biệt được hai chuyện.
            //
            // Log của chính hai lượt ấy: `pick_cursor_unknown questions:1
            // total:2`. Sổ nhật ký của huba mới có MỘT câu trong khi thanh tab
            // trên màn đã có HAI — bảng vừa mở, `AskUserQuestion` chưa kịp vào
            // `.jsonl` (xem `CLAUDE.md` §13: hộp đang treo chưa được ghi). Nên
            // `cursor_on` chỉ dò được câu huba biết, mà câu đang mở là câu kia.
            // Đo được: hai phút sau, sổ đã đủ hai câu và `/pick` chạy đúng.
            //
            // Hai trạng thái khác hẳn nhau, và cách chữa cũng khác:
            // * sổ THIẾU câu ⟹ đợi một nhịp rồi `/pick` lại, hoặc `/key <số>`;
            // * sổ ĐỦ mà vẫn không khớp ⟹ chữ câu hỏi không có trên màn (đã nới
            //   hết cỡ ở trên mà vẫn thiếu) ⟹ `/key <số>` là đường chắc.
            //
            // KHÔNG đoán bừa vị trí con trỏ trong cả hai ca: mũi tên đi kèm một
            // CR nên đi mò là chốt nhầm hộ chủ máy, và cái đó không lùi lại được.
            let so_sach = questions.len();
            // 🔴 LỜI TỪ CHỐI PHẢI BẤM ĐƯỢC — Hà 2026-08-26: *"thao tác vẫn chưa
            // được mượt và thông minh khi có nhiều lựa chọn"*.
            //
            // huba không biết câu đang mở là câu NÀO, nhưng nó đọc được các LỰA
            // CHỌN đang hiện — và số của chúng là số CLI tự đánh cho đúng câu ấy,
            // nên `/k_<phiên>_<số>` chạm phát nào trúng phát đó. Bảo người ta gõ
            // `/key <số>` thì vừa bắt gõ tay vừa là một ngõ cụt: Telegram vẽ
            // `/key` thành lệnh chạm được, mà chạm chỉ gửi token `/key` — chữ sau
            // dấu cách rơi mất (cùng bẫy đã trị ở nhánh `Bảng ĐÃ ĐỦ`).
            //
            // Đọc được lựa chọn nào thì mời đúng chừng ấy. Không đọc được cái nào
            // thì nói thẳng là không có gì để mời, đừng bịa ra một dãy số.
            let short: String = s.session_id.chars().take(8).collect();
            let mut moi = String::new();
            for (n, nhan) in crate::keys::parse_choices(&body) {
                moi.push_str(&format!(
                    "\n/k_{short}_{n} {}",
                    crate::exec::truncate(nhan.trim(), 60)
                ));
            }
            let duong_ra = if moi.is_empty() {
                "\n/shot để nhìn màn, rồi /key <số> cho câu đang mở.".to_string()
            } else {
                format!("\nChạm thẳng lựa chọn của câu ĐANG MỞ:{moi}")
            };
            return if so_sach < total {
                format!(
                    "⚠ Bảng của {name} có {total} câu trên màn, nhưng sổ của tôi mới ghi được \
                     {so_sach} — bảng vừa mở, nhật ký phiên chưa kịp có nó. Nên tôi KHÔNG biết \
                     câu đang mở là câu nào, và đi mò thì chốt nhầm câu.{duong_ra}"
                )
            } else {
                format!(
                    "⚠ Đọc được bảng {total} câu của {name}, sổ cũng đủ {so_sach} câu, nhưng chữ \
                     của câu ĐANG MỞ không có trên màn (đã nới hết cỡ mà vẫn thiếu) — nên tôi \
                     không biết phải đi mấy bước, và đi mò thì chốt nhầm câu.{duong_ra}"
                )
            };
        }
    };

    let before = table.as_ref().map(|t| t.left());
    // 🔴 CẢ DÃY VÀO **MỘT** LƯỢT GHI, và đây là chỗ luật ấy sinh ra (13/08):
    // bảng nhiều câu đi bằng mũi tên + số, mà mỗi lượt ghi tự kèm một CR — nên
    // tách ra nhiều lượt là rải Enter vào giữa đường, tức CHỐT hộ chủ máy đúng
    // cái câu con trỏ đang đứng trước khi tới được câu anh bấm. Gộp lại thì cả
    // dãy chỉ còn đúng một dấu, và nó nằm sau con số.
    let keys = pick_keys(cursor, q - 1, opt);
    if let Err(e) = crate::keys::press_writes(w, std::slice::from_ref(&keys)) {
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
    let mut seen_body = String::new();
    for _ in 0..3 {
        std::thread::sleep(std::time::Duration::from_millis(900));
        // Đọc lại BẰNG ĐÚNG BỀ NGANG của lần đọc trước. `before` đếm trên bản
        // rộng mà `after` đếm trên bản hẹp là so hai cái màn khác nhau: số ô
        // trống lệch đi rồi rơi vào nhánh *"bảng KHÔNG đổi"* hoặc *"bảng biến
        // mất"* — một câu sai, tự tin, về đúng cú bấm vừa rồi.
        let doc = if da_noi {
            crate::keys::screen_text_tall(w, crate::keys::GROW_ASK).ok()
        } else {
            match crate::keys::look(&s.tty, PICK_LINES) {
                crate::keys::Look::Saw { body, .. } => Some(body),
                crate::keys::Look::Blind { .. } => None,
            }
        };
        if let Some(body) = doc {
            let t = crate::keys::ask_table(&body);
            seen_body = body;
            if t.as_ref().map(|t| t.left()) != before {
                after = t;
                break;
            }
            after = t;
        }
    }
    // 🔴 HỘP CHỌN MỘT CÂU KHÔNG PHẢI MỘT BẢNG — Hà 2026-08-17, ảnh bốn cú
    // `/pick` liên tiếp trên `[dwork]`, mỗi cú trả lời *"Bảng không còn trên màn
    // — nhiều khả năng nó vừa được gửi đi"* trong khi bảng vẫn nằm nguyên đó:
    // *"Ko qua nổi màn này"*.
    //
    // `ask_table` đọc THANH TAB của bảng NHIỀU CÂU (`←  ☒ …  ✔ Submit  →`). Một
    // hộp CHỌN NHIỀU chỉ có một câu thì không có thanh ấy, nên nó trả `None` —
    // và cả hai đầu `before`/`after` cùng `None` rơi thẳng vào nhánh cuối, thứ
    // đọc `None` thành "bảng biến mất". Một phép đo trả lời câu hỏi KHÁC với
    // câu đang hỏi, và nó sai theo hướng tệ nhất: bảo người ta rằng việc đã
    // xong trong khi họ còn phải bấm tiếp.
    //
    // Hỏi thẳng thứ đang cần biết: màn CÒN hộp chọn không.
    let still_choosing = !crate::keys::parse_choices(&seen_body).is_empty();
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
                // 🔴 ĐƯỜNG GỬI PHẢI BẤM ĐƯỢC — Hà 2026-08-26, ảnh buồng chat
                // 16:48: *"thao tác vẫn chưa được mượt và thông minh khi có
                // nhiều lựa chọn"*. Câu này bảo *"bấm `/key enter` để gửi"*,
                // Telegram vẽ `/key` thành một lệnh chạm được, Hà chạm — và
                // client chỉ gửi đúng token `/key`, chữ `enter` sau dấu cách
                // RƠI MẤT. huba tự trả lời chính mình: *"Chưa hiểu lệnh này"*.
                //
                // Đúng cái bẫy `verbs.rs` đã ghi khi dựng `send_<id>`: *"`/key`
                // có tham số đứng sau dấu cách, mà chạm thì chỉ gửi lại token
                // lệnh"*. Nhánh `(Some(0), Some(0))` ngay dưới đã dùng
                // `/send_{sid}` từ trước; nhánh này bị bỏ quên — một luật sửa ở
                // một chỗ trong hai chỗ cùng hình dạng.
                format!(
                    "✅ {name} · câu {q} ({label}) → chọn {opt}. Bảng ĐÃ ĐỦ — bấm /send_{sid} để gửi.",
                    sid = s.session_id.chars().take(8).collect::<String>()
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
        // Không có THANH TAB nhưng màn vẫn còn hộp chọn ⟹ hộp một câu (thường
        // là CHỌN NHIỀU): bấm được tiếp, và phải nói ra đường gửi.
        _ if still_choosing => format!(
            "✅ {name} → chọn {opt}. Hộp vẫn mở: bấm thêm lựa chọn, xong thì /send_{sid} để gửi.",
            sid = s.session_id.chars().take(8).collect::<String>()
        ),
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
pub struct ScreenReport {
    /// Chữ gửi lên điện thoại.
    pub text: String,
    /// Hộp chọn ĐO ĐƯỢC trên màn — đo một lần, dùng cho cả chữ lẫn nút.
    ///
    /// 🔴 Vì sao phải trả ra chứ không để chỗ gọi tự đo lại, 2026-08-15. Hà,
    /// ảnh chụp một `/shot` của `[dwork]` đang mở hộp khảo sát 4 lựa chọn:
    /// *"Có lựa chọn nhưng không thấy nút"*. Chữ thì ĐÚNG — tin mở bằng
    /// *"đang hỏi — bấm số ở hàng phím để chọn"* kèm đủ 4 dòng — mà nút thì
    /// không có, và cái nút `⏎` (thứ phải BIẾN MẤT khi có hộp chọn) lại có.
    ///
    /// Gốc: chỗ gọi hỏi `parse_choices(&ack)`, tức đo trên **chữ huba vừa
    /// viết ra**, mà chữ ấy chép lại nguyên hộp chọn lên đầu tin. Màn thành
    /// `1,2,3,4` rồi lại `1,2,3,4`, và luật "số phải liên tiếp từ 1" — luật
    /// đúng, dựng để một đoạn văn có đánh số không bị đọc thành hộp chọn —
    /// thấy `1` ở vị trí thứ 5 nên trả về RỖNG. huba tự làm mù phép đo của
    /// chính nó bằng đầu ra của chính nó.
    ///
    /// Cùng một họ với `??` đọc thành cửa sổ và với `⏎ Gửi: # Lệnh thấy trên
    /// màn…`: huba đọc lại lời của mình rồi tưởng là của người khác. Cách chữa
    /// cũng cùng một kiểu — **một phép đo, một chỗ**, và chỗ ấy là nơi còn
    /// cầm màn GỐC.
    pub choices: Vec<(usize, String)>,
}

/// `said` = lời cuối theo NHẬT KÝ, nếu chỗ gọi có. Nó không đi vào tin; nó là
/// cái thước để biết màn có đang thiếu chữ không, và chỉ khi thiếu mới nới cửa
/// sổ của chủ máy lên (đụng vào cửa sổ ai đó đang nhìn thì phải có cớ).
pub fn screen_report(
    s: &crate::sessions::LiveSession,
    window: i64,
    lines: usize,
    said: Option<&str>,
) -> ScreenReport {
    // Tên để ĐỌC. 🔴 Hà 2026-08-13, ảnh chụp Telegram: nút và dòng "Đang theo
    // phiên" đã là `[AI/huba]` trong khi ngay dưới nó `/shot` còn in `📷 Màn của
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
            // xem trước là mảnh chữ **huba tự chọn** đẩy vào một tài liệu trên
            // server — ngờ cả chữ là đúng. Còn `/shot` là **chủ máy gọi đích
            // danh một phiên của chính anh**, trả về buồng chat gác bằng
            // `chat_id`. Anh đang nhìn cái màn ấy nếu ngồi ở máy; chặn chữ ở
            // đây là chặn đúng phép thử cầu nối.
            //
            // GIÁ TRỊ thì vẫn chặn — `credential_literal`, `private_key_block`,
            // `secret_assignment` — vì đó mới là thứ mất đi khi lọt ra ngoài.
            // 🔴 Hà 2026-08-14: *"Tại sao lại bị chặn, huba là cổng làm việc của
            // tôi mà"* → *"Trong tele có thiết lập tự xoá lịch sử tin rồi nên
            // huba không cần tính năng này nữa"*.
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
            // 🔴 HỘP BỊ MÉP MÀN CẮT ⟹ ĐỌC LẠI BẰNG CỬA SỔ CAO — Hà 2026-08-19:
            // *"phải có cách khác để mọi thứ trong phiên phải thể hiện đúng đủ
            // khi gửi giống như một bản sao hoàn hảo chứ?"*
            //
            // Dấu hiệu nhận ra "bị cắt" là thứ đo được, không phải phỏng đoán:
            // danh sách lựa chọn đọc ra mà số ĐẦU không phải 1 ⟹ mấy mục trên
            // đã cuộn khỏi mép. Chỉ lúc ấy mới nới, vì nới là đụng vào cửa sổ
            // của chủ máy và tốn thêm ~1,5 giây (xem `keys::screen_text_tall`).
            let mut screen = screen;
            let mut choices = crate::keys::parse_choices(&screen);
            let mut grew = false;
            let choices_cut = choices.first().is_some_and(|(n, _)| *n > 1);
            // 🔴 VĂN XUÔI CŨNG BỊ MÉP CẮT, và cho tới 20/08 không có gì nhận ra
            // — Hà: *"Tốt nhất là tự kiểm và cuộn lên để lấy đầy đủ và đúng
            // nhất"*. Cửa nới cũ chỉ mở cho hộp chọn, vì hộp chọn có một dấu
            // hiệu tự tố cáo (số đầu không phải 1). Một câu trả lời dài thì
            // không có dấu hiệu nào cả: nó chỉ đơn giản bắt đầu giữa câu.
            //
            // Thước đo là NHẬT KÝ — hỏi `said_shown_on_screen` xem chữ phiên
            // vừa nói có nằm trên màn không. Đo thật trên cửa sổ phiên `[huba]`
            // 20/08: 24×80 ⟹ 1081 ký tự, nới HẾT CỠ ⟹ 61×206 và **3943** ký
            // tự, đầu màn lùi về phần đã trôi. Cuộn lên được thật, gấp 3,6 lần.
            let prose_cut =
                said.is_some_and(|t| !crate::sessions::said_shown_on_screen(t, &screen));
            if choices_cut || prose_cut {
                match crate::keys::screen_text_tall(window, crate::keys::GROW_ASK) {
                    Ok(rong) if !rong.trim().is_empty() => {
                        let them = crate::keys::parse_choices(&rong);
                        // Chỉ nhận bản rộng khi nó THẬT SỰ hơn: nới xong mà vẫn
                        // cụt (hộp dài hơn cả 60 dòng) thì đừng đổi lấy một bản
                        // đọc khác cũng cụt.
                        //
                        // Hai ca hỏi hai câu khác nhau, vì "hơn" ở hai ca là hai
                        // thứ: hộp chọn hơn khi đã thấy lựa chọn `1.`, còn văn
                        // xuôi hơn khi đọc ra NHIỀU CHỮ HƠN — nó không có mốc
                        // nào để mà "đủ".
                        let het_cut = them.first().is_some_and(|(n, _)| *n == 1);
                        let dai_hon = rong.chars().count() > screen.chars().count();
                        logging::info(
                            "shot_grew_window",
                            json!({ "window": window, "xin": crate::keys::GROW_ASK,
                                    "choices_before": choices.len(), "choices_after": them.len(),
                                    "chars_before": screen.chars().count(),
                                    "chars_after": rong.chars().count(),
                                    "why": if choices_cut { "hộp chọn bị mép màn cắt" }
                                           else { "lời cuối của phiên không hiện trọn trên màn" },
                                    "taken": (choices_cut && het_cut) || (prose_cut && dai_hon) }),
                        );
                        if (choices_cut && het_cut) || (prose_cut && dai_hon) {
                            screen = rong;
                            choices = them;
                            grew = true;
                        }
                    }
                    Ok(_) => logging::warn(
                        "shot_grow_empty",
                        json!({ "window": window,
                                "effect": "nới cửa sổ xong đọc ra rỗng — giữ bản đọc cũ" }),
                    ),
                    Err(e) => logging::warn(
                        "shot_grow_failed",
                        json!({ "window": window, "err": crate::logging::err_chain(&e),
                                "effect": "không nới được cửa sổ — tin vẫn đi, nhưng thiếu phần đã cuộn khỏi mép" }),
                    ),
                }
            }
            // 🔴 NỚI RỒI MÀ VĂN XUÔI VẪN THIẾU ⟹ CUỘN. Hà 2026-08-20: *"Chỉ cần
            // focus tới cửa sổ di chuột tới khung nhìn cuộn chuột là được"*.
            //
            // Đứng SAU cửa nới, không thay nó, vì hai thứ lấy hai kiểu: nới là
            // MỘT lượt đọc rẻ, đụng trần 61×206 rồi thôi; cuộn thì đi ngược bao
            // xa tuỳ ý nhưng tốn một lượt đọc mỗi nấc và động vào cửa sổ chủ máy
            // đang nhìn. Rẻ trước, đắt sau — và ca thường gặp (lượt trả lời hơi
            // dài hơn khung) đã xong ở cửa trên rồi.
            //
            // KHÔNG cuộn khi có hộp chọn: bánh xe không chốt được gì, nhưng cuộn
            // một hộp đang treo ra khỏi khung thì `parse_choices` đọc trượt, và
            // tin sẽ mất đúng thứ đáng bấm.
            if !grew && prose_cut && choices.is_empty() {
                // Trần THẤP có chủ ý (10 nấc ≈ 80 dòng ngược). Đo 20/08: mỗi nấc tốn
                // ~4 giây khi TUI đang vẽ, nên 24 nấc là một phút rưỡi cho người
                // đang cầm điện thoại. Cuộn lo phần MÀN, nhật ký lo phần đuôi —
                // và nhật ký thì không có trần nào.
                match crate::keys::screen_scrollback(window, 10, |du| {
                    said.is_some_and(|t| crate::sessions::said_shown_on_screen(t, du))
                }) {
                    Ok(du) if du.chars().count() > screen.chars().count() => {
                        logging::info(
                            "shot_scrolled",
                            json!({ "window": window,
                                    "chars_before": screen.chars().count(),
                                    "chars_after": du.chars().count(),
                                    "why": "nới hết cỡ rồi vẫn thiếu lời cuối — cuộn ngược" }),
                        );
                        screen = du;
                        grew = true;
                    }
                    Ok(_) => logging::warn(
                        "shot_scroll_no_gain",
                        json!({ "window": window,
                                "effect": "cuộn xong không thêm chữ nào — giữ bản đọc cũ" }),
                    ),
                    Err(e) => logging::warn(
                        "shot_scroll_failed",
                        json!({ "window": window, "err": crate::logging::err_chain(&e),
                                "effect": "không cuộn được — tin vẫn đi, phần trên vẫn thiếu" }),
                    ),
                }
            }
            // Nới cửa sổ xong mà vẫn cắt theo trần cũ (40 dòng) thì công cốc:
            // bản rộng đọc ra 61 đoạn, `SHOT_LINES` là 40, và phần bị cắt lại
            // đúng là phần vừa lấy được. Đọc rộng ⟹ lấy trọn, trần vẫn là
            // `SHOT_LINES_MAX`.
            let lines = if grew { SHOT_LINES_MAX } else { lines };
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
            // trong ô nhập, `input_box_text` đọc nó lên, rồi huba dựng nút
            // `⏎ Gửi: # Lệnh thấy trên màn…` — mời gửi lại lời của chính mình.
            // Bỏ nguồn thì cả họ bug ấy hết đường sinh ra.
            //
            // Nút thì vẫn dựng — ở chỗ gọi, từ chính `ack` này
            // (`commands_on_screen`), nên không mất đường bấm nào.
            let quick_note = String::new();
            // 🔴 THÔI CHÉP DANH SÁCH LỰA CHỌN LÊN ĐẦU TIN — Hà 2026-08-17:
            // *"Bảo ko chèn ở dưới thì lại chèn lên đầu làm ăn kiểu gì thế"*.
            //
            // Khối ấy là bản sao thứ hai của thứ đã nằm ngay bên dưới, trong
            // chính ảnh màn. Nó ra đời khi huba chưa chèn được gì vào giữa chữ:
            // hồi ấy phải kể lại danh sách thì mới nói được "bấm số nào". Nay ☑
            // nằm ngay tại dòng của mỗi lựa chọn, nên bản chép không những
            // thừa — nó còn CƯỚP MẤT chỗ neo: `html_with_links` bám dòng ĐẦU
            // TIÊN khớp nhãn, mà bản chép nằm trên màn thật, nên mọi cái ☑ dán
            // hết vào bản chép và màn thật thì trơ ra. Đúng thứ Hà nhìn thấy:
            // "chèn lên đầu".
            //
            // Cùng một bài học đã trả giá cho khối chữ lệnh (15/08) và cho khu
            // `/pick_…` ở cuối tin (17/08, nửa tiếng trước): nội dung đã có mặt
            // trên màn thì đừng kể lại, hãy gắn action VÀO CHÍNH NÓ.
            // …nhưng PHẢI nói khi màn đã CẮT MẤT phần đầu của hộp chọn. Đây
            // không phải chép lại nội dung — nội dung ấy không có trên màn để
            // mà chép — mà là nói ra một chỗ huba đang mù, đúng luật 13.
            //
            // 🔴 Hà 2026-08-19, `/shot` phiên `[tcc/amm]`: *"đọc không hiểu
            // luôn"*. Hộp sáu lựa chọn, mỗi cái bốn dòng mô tả, cao hơn cửa sổ
            // 80×24 ⟹ màn mở đầu bằng đuôi mô tả của lựa chọn 1 và số của nó thì
            // không còn. Không có dòng này thì tin đọc như thể hộp chỉ có năm
            // mục bắt đầu từ số 2.
            let cut_note = match choices.first() {
                Some((n, _)) if *n > 1 => format!(
                    "\n\n⚠ Hộp chọn CAO HƠN cửa sổ: {} lựa chọn đầu đã cuộn khỏi mép trên, huba không đọc được. \
                     Số vẫn bấm đúng (/key <số>); muốn thấy cả hộp thì nới cửa sổ terminal cao lên.",
                    n - 1
                ),
                _ => String::new(),
            };
            let text = format!("📷 Màn của {what}:\n\n{body}{quick_note}{cut_note}");
            ScreenReport { text, choices }
        }
        Err(e) => ScreenReport {
            text: format!(
                "⚠ không đọc được màn: {}",
                crate::exec::truncate(&e.to_string(), 300)
            ),
            // Không đọc được màn ≠ màn không có hộp chọn. Để rỗng ở đây là
            // đúng, và cửa dùng nó phải hiểu đúng như thế — xem chỗ gọi.
            choices: Vec::new(),
        },
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
/// Chế độ quyền của một phiên, thành chữ. `""` = phiên không khai chế độ nào
/// (cửa sổ Terminal trần chẳng hạn — nó không có chế độ, chứ không phải thiếu
/// dữ liệu).
fn permission_label(s: &crate::sessions::LiveSession) -> &'static str {
    match s.permission_mode.as_deref() {
        Some("auto") => "tự duyệt",
        Some("dontAsk") => "không hỏi",
        Some("default") => "hỏi trước",
        Some(_) => "khác",
        None => "",
    }
}

/// `mode_inline` = có in chế độ quyền vào ngay hàng này không.
///
/// 🔴 Đo 2026-08-22 trên máy thật: **7/8 phiên cùng `auto`**, nên `· tự duyệt`
/// in ở gần như mọi hàng và không phân biệt được gì — nó chỉ đẩy phiên cuối
/// danh sách ra khỏi màn (cùng lý do `quiet_for` không nói "im 0 phút"). Khi cả
/// danh sách chung một chế độ thì [`session_list_text`] nói MỘT LẦN ở đầu; chỉ
/// khi các phiên khác nhau chế độ thì con chữ ấy mới mang tin, và lúc ấy nó
/// quay lại từng hàng.
fn session_meta(
    s: &crate::sessions::LiveSession,
    now_ms: i64,
    mode_inline: bool,
    // Động từ đã lên đứng cạnh `⚡` ở ô tình trạng (xem `session_list_text`) —
    // in lại ở đây là in hai lần cùng một chữ trên cùng một hàng.
    verb_moved: bool,
) -> String {
    let mode = if mode_inline { permission_label(s) } else { "" };
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
    // `%` không cần chú thích: nó đứng cạnh tên phiên và một con số phần trăm
    // trong danh sách phiên chỉ có thể là ngữ cảnh. Chín ký tự × mỗi hàng là
    // một phiên bị đẩy khỏi màn điện thoại.
    let ctx = if pct > 0 {
        format!("{pct}%")
    } else {
        String::new()
    };
    let quiet = if s.working {
        String::new()
    } else {
        quiet_for(s.last_activity.as_deref(), now_ms).unwrap_or_default()
    };
    let activity = if verb_moved {
        String::new()
    } else {
        s.activity.clone().unwrap_or_default()
    };
    [activity, quiet, kid, ctx, mode.to_string()]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// "im bao lâu rồi", tính từ lượt cuối nhật ký lớn lên.
///
/// Dưới một phút thì KHÔNG nói: "im 0 phút" là một dòng chữ không mang tin, và
/// mỗi dòng thừa đẩy phiên cuối danh sách ra khỏi màn.
///
/// 🔴 VIẾT GỌN 2026-08-25 — Hà, kèm ảnh danh sách: *"ví dụ 'đứng chờ' bỏ đi,
/// 'im 1 tiếng' thì viết gọn thành 1h là được"*.
///
/// `im 1 tiếng` = 11 cột, `1h` = 2. Trên sáu hàng là **54 cột** lấy lại, và
/// mỗi cột lấy lại là một cột trả về cho TÊN phiên — ô duy nhất bị `cut_to_cols`
/// cắt cụt (xem `ROW_COLS`). Chữ "im" cũng đi: ô này đứng cạnh icon tình trạng
/// `💤`, nên nó chỉ nhắc lại điều cái icon vừa nói.
fn quiet_for(last_activity: Option<&str>, now_ms: i64) -> Option<String> {
    let dt = chrono::DateTime::parse_from_rfc3339(last_activity?).ok()?;
    let mins = (now_ms - dt.timestamp_millis()) / 60_000;
    match mins {
        m if m < 1 => None,
        m if m < 60 => Some(format!("{m}p")),
        m if m < 60 * 24 => Some(format!("{}h", m / 60)),
        m => Some(format!("{}n", m / (60 * 24))),
    }
}

/// Nhãn của một cái nút phiên. Cùng ba dữ kiện với dòng chữ, gọn hơn để lọt bề
/// ngang một cái nút.
pub fn session_button_label(s: &crate::sessions::LiveSession) -> String {
    // 🔴 CÙNG BỘ VỚI DÒNG CHỮ, và nay là cùng một HÀM — Hà 2026-08-19, ảnh danh
    // sách sau bản vá icon: dòng chữ đã là `💤 đứng chờ` trong khi mấy cái NÚT
    // ngay dưới vẫn `🟡`. Chú thích cũ ở đây viết *"cùng bộ chấm với
    // `session_list_text`"* — một lời hứa bằng chữ, giữ bằng tay, và nó gãy
    // ngay lượt đổi đầu tiên. Bản chép thứ hai của một bảng thì không bao giờ
    // là "cùng bộ"; nó chỉ là bộ giống nhau CHO TỚI KHI ai đó sửa một bên.
    let dot = crate::sessions::state_of(s).0;
    // Dự án trước, vì đó là thứ ngón tay đang tìm; sau dấu `·` là VIỆC phiên
    // đang làm, chỉ hiện khi có hai phiên cùng dự án (`sessions::label_sessions`).
    let what = crate::sessions::shown(s);
    // Nguồn đứng ngay trên NÚT nữa, không chỉ trên danh sách chữ: cái nút mới là
    // thứ ngón tay chạm vào, và nó phải nói trước rằng bấm vào một phiên VS Code
    // thì xem được chứ gõ thì không.
    // 🔴 30 → 48 ký tự, 2026-08-19. Con số cũ ra đời khi cái nhãn dài nhất có
    // thể là `[dwork]·08a90086` (16 ký tự) — nó chưa bao giờ cắt gì cả, nên
    // không ai biết nó chật. Nay nhãn mang việc đang làm, và 30 cắt đúng giữa
    // câu: `🟩 [dwork]·Tiếp tục DS04 q…`.
    //
    // 48 vì nút `sess:` đứng MỘT MÌNH một hàng (`telegram::keyboard_rows` chỉ
    // gộp `file:` và `key:`), nên bề ngang không phải chia với ai; Telegram tự
    // xuống dòng trong nút, và hai dòng trên một màn điện thoại vẫn đọc được —
    // trong khi một câu cụt thì không đọc được ở bất cứ bề ngang nào.
    format!(
        "{} {} {} · {}",
        dot,
        source_icon(&s.host),
        crate::exec::truncate(&what, 48),
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
                let n = cmd.arg.trim();
                match quick_cmd(db, n).map(|(sid, c)| (sid, c.line)) {
                    Some((sid, line)) => {
                        logging::info(
                            "run_quick",
                            json!({ "n": n, "session": sid,
                                    "cmd": crate::exec::truncate(&line, 120) }),
                        );
                        // Cùng đường với cái nút: MÁY chạy, PHIÊN đọc.
                        let ack = format!("▶ chạy trong {sid}: {line}");
                        reply_from_session(db, cfg, adapter, cmd, &sid, &ack);
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
            CommandKind::RunInTerminal => {
                // 🖥 CÙNG dòng lệnh của `RunQuick`, khác chỗ CHẠY. Xem
                // `adapters::CommandKind::RunInTerminal` để biết vì sao hai kiểu.
                let n = cmd.arg.trim();
                let ack = match quick_cmd(db, n).map(|(sid, c)| (sid, c.line, c.cwd)) {
                    None => "⚠ lệnh gợi ý ấy đã cũ (màn đã đổi). Gõ /shot rồi bấm lại.".to_string(),
                    Some((sid, line, cwd)) => {
                        // Mở cửa sổ ở ĐÚNG thư mục sổ đã ghi cho dòng lệnh này.
                        // Một lệnh tương đối (`bash ./gate.sh`) chạy ở thư mục
                        // khác là chạy một thứ khác — hoặc không chạy gì.
                        match crate::sessions::open_bare_terminal() {
                            Err(e) => {
                                let why = crate::logging::err_chain(&e);
                                logging::error("term_run_open_failed", json!({ "err": why }));
                                format!("⚠ chưa mở được cửa sổ: {why}")
                            }
                            Ok((w, tty)) => {
                                let full = if cwd.trim().is_empty() {
                                    line.clone()
                                } else {
                                    format!(
                                        "cd {} && {line}",
                                        crate::sessions::shell_quote(cwd.trim())
                                    )
                                };
                                logging::info(
                                    "term_run",
                                    json!({ "n": n, "session": sid, "tty": tty,
                                            "cmd": crate::exec::truncate(&full, 120) }),
                                );
                                // Chuyển con trỏ sang cửa sổ ấy — cùng luật với
                                // `/new` trần: mở xong thì tay người ta phải ở
                                // đó, không phải ở phiên cũ.
                                let id = format!("{}{tty}", crate::sessions::SHELL_ID_PREFIX);
                                if let Err(e) = db.set_cursor(FOCUS_SESSION_KEY, &id) {
                                    logging::error(
                                        "focus_after_term_run_failed",
                                        json!({ "err": e.to_string(), "id": id }),
                                    );
                                }
                                match crate::keys::type_and_send(w, &full) {
                                    Ok(crate::keys::Delivered::StillInBox) => {
                                        logging::warn(
                                            "term_run_left_in_box",
                                            json!({ "tty": tty,
                                                    "effect": "lệnh đã vào ô nhập của cửa sổ mới nhưng chưa gửi được" }),
                                        );
                                        format!(
                                            "⚠ Đã mở cửa sổ ({tty}) và gõ lệnh vào, nhưng nó VẪN nằm trong ô nhập — \
                                             chưa chạy. Bấm Enter trong cửa sổ ấy, hoặc /key enter."
                                        )
                                    }
                                    Ok(_) => {
                                        // 🔴 ĐÍCH của nút này là TELEGRAM — Hà
                                        // 2026-08-16: *"1 nút là chạy terminal
                                        // được kết quả gửi về tele"*. Canh ở
                                        // luồng riêng: chờ tại chỗ thì khoá cả
                                        // vòng chạy (xem `watch_long_job`).
                                        watch_terminal_job(w, tty.clone(), full.clone());
                                        format!(
                                            "🖥 Đang chạy trong cửa sổ riêng ({tty}) — báo lại khi xong.\n\
                                             /shot để nhìn màn."
                                        )
                                    }
                                    // `open` xong mà gõ hỏng thì cửa sổ vẫn ở
                                    // đó và TRỐNG: nói đúng như vậy, đừng khai
                                    // là đã chạy.
                                    Err(e) => {
                                        let why = crate::logging::err_chain(&e);
                                        logging::error(
                                            "term_run_type_failed",
                                            json!({ "tty": tty, "err": why }),
                                        );
                                        format!(
                                            "⚠ Đã mở cửa sổ ({tty}) nhưng CHƯA gõ được lệnh vào: {why}"
                                        )
                                    }
                                }
                            }
                        }
                    }
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
            }
            // 📎 Liên kết tải tệp nằm GIỮA CHỮ — cùng sổ, cùng cửa với cái nút
            // `file:<n>` ở đáy tin (xem `telegram::Inbox::send_quick_file`).
            CommandKind::SendFile => {
                let n = cmd.arg.trim().parse::<usize>().ok();
                let ack = match (n, crate::telegram::inbox()) {
                    (Some(n), Some(tg)) => match tg.send_quick_file(n) {
                        // Tệp đã đi rồi thì không nói thêm câu nào: chính cái
                        // tệp hiện ra trong buồng chat là câu trả lời.
                        None => {
                            logging::info("quick_file_sent", json!({ "n": n, "via": "deep_link" }));
                            continue;
                        }
                        Some(why) => why,
                    },
                    (None, _) => {
                        logging::warn("quick_file_bad_index", json!({ "arg": cmd.arg }));
                        "⚠ liên kết tệp ấy hỏng (không đọc được chỉ số).".to_string()
                    }
                    (_, None) => "⚠ chưa nối được Telegram để gửi tệp.".to_string(),
                };
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
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
                let live = crate::sessions::snapshot(cfg);
                let jobs = jobs_line().unwrap_or_else(|| "  (không có)".to_string());
                // 🔴 QUYỀN TRỢ NĂNG PHẢI ĐỌC ĐƯỢC TỪ ĐIỆN THOẠI — Hà 2026-08-19:
                // *"Bật trợ năng là sao, sao tin nhắn tôi không thấy chi tiết
                // về thông tin này"*.
                //
                // Trước đó câu trả lời chỉ nằm trong lời từ chối của `/tab`, tức
                // muốn biết đã cấp quyền chưa thì phải đi bấm một cái nút CẦN
                // quyền ấy rồi đọc lỗi. Đó là bắt người ta thử cửa để biết cửa
                // khoá — trong khi `/doctor` sinh ra đúng để trả lời "cái gì
                // đang chạy được, cái gì không".
                //
                // Và nó phải hỏi ở ĐÂY: `AXIsProcessTrusted` trả lời về TIẾN
                // TRÌNH ĐANG HỎI, mà lệnh Telegram chạy bên trong `hubad` — đúng
                // tiến trình cần quyền. Hỏi từ `huba` (CLI) là hỏi về một chương
                // trình khác, và nhận một câu trả lời đúng cho câu hỏi sai.
                let keys_line = if crate::cgkeys::trusted() {
                    "🔑 phím rời (Trợ năng): đã cấp — nút ↪ chuyển tab chạy được"
                } else {
                    "🔑 phím rời (Trợ năng): CHƯA cấp — Cài đặt Hệ thống ▸ \
                     Quyền riêng tư & Bảo mật ▸ Trợ năng ▸ bật `hubad`. \
                     Không có nó thì nút ↪ chuyển tab không đi đâu cả."
                };
                let probe = format!(
                    "🩺 {} phiên đang sống{}\n⚡ lệnh chạy nền:\n{}\n📟 hubad: {}\n{keys_line}\n{}",
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
                // Lệnh dựng lại chính huba đi đường `/upgrade`, kể cả khi được
                // gõ tay vào đây: chạy nó qua `/runin` thì hubad bị thay thế
                // giữa lúc đang xử lý, và câu trả lời chết theo — đo được ba
                // lần liền 2026-08-13, xem `remember_quick`.
                if is_self_rebuild(&line) {
                    let ack = match crate::runtime::self_install(cfg) {
                        Ok(msg) => format!(
                            "🔧 {msg}\nĐang khởi động lại hubad… (lệnh này dựng lại chính huba nên nó \
                             đi đường /upgrade — chạy qua /runin thì huba bị thay giữa chừng và câu \
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
                let live = crate::sessions::snapshot(cfg);
                // Phiên mà câu trả lời này NÓI VỀ — cửa định dạng cần nó để gắn
                // action vào đúng phiên (rỗng ⟹ không có phiên nào, đi đường
                // thường).
                let mut ack_sid = String::new();
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
                        // Thư mục ĐÚNG là thư mục nhật ký đã ghi cho chính dòng
                        // lệnh này — xem `root_for_command`. Nhãn dự án chỉ là
                        // đường lùi cho nút sinh ra trước bản vá 15/08.
                        let logged = quick_cwd(db, &s.session_id, &line);
                        let root = match root_for_command(db, cfg, &s.session_id, &logged) {
                            Some(r) => r,
                            None => {
                                logging::warn(
                                    "runin_no_root",
                                    json!({ "session": s.session_id,
                                            "why": "sổ chưa biết dự án của phiên — không đoán gốc workspace" }),
                                );
                                let msg = format!(
                                    "⚠ chưa biết {} làm ở thư mục nào nên KHÔNG chạy. Một lệnh tương đối chạy nhầm thư mục thì vẫn ra một mã thoát, mà kết quả nói về thứ khác. Dùng đường dẫn tuyệt đối, hoặc chờ huba nhận ra dự án của phiên.",
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
                        // sai. Nay lệnh chạy ở luồng riêng, huba trả lời NGAY,
                        // rồi theo dõi và báo lại — xem `watch_long_job`.
                        watch_long_job(
                            cfg.clone(),
                            s.clone(),
                            root.clone(),
                            line.clone(),
                            adapter.to_string(),
                            cmd.chat_id.clone(),
                            cmd.quiet,
                        );
                        ack_sid = s.session_id.clone();
                        // 🔴 Câu này KHÔNG kể ruột huba — Hà 2026-08-16: *"Tại
                        // sao để báo trần 120s làm gì"*. Bản cũ khoe *"không
                        // còn trần 120 giây"*: một cái trần **đã bị gỡ**, tức
                        // huba đang khoe với người đọc rằng nó vừa sửa một chỗ
                        // hỏng của chính nó. Cùng lỗi đã sửa 12/08 cho `/type`
                        // (*"chỉ cần báo đã gõ được thôi cần gì báo đã gửi
                        // enter rời"*): người đọc hỏi *việc chạy chưa*, không
                        // hỏi huba xoay xở thế nào.
                        format!(
                            "▶ đang chạy — {}\ntrong {} · báo lại khi xong.",
                            crate::exec::truncate(&line, 120),
                            crate::sessions::shown(s),
                        )
                    }
                };
                reply_from_session(db, cfg, adapter, cmd, &ack_sid, &ack);
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
                    // 🔴 CHỈ cửa sổ TRẦN, và MỖI CÁI MỘT NÚT. Hà 2026-08-15:
                    // *"`/terminal` luôn liệt kê terminal trống → bấm chọn thì
                    // làm việc được với nó (giống như session)"*.
                    //
                    // "Giống như session" đọc theo nghĩa đen, và đó là chỗ tiết
                    // kiệm được cả một nhánh: nút gửi `sess:<id>` — ĐÚNG callback
                    // của `/session` — vì cửa sổ trần đã có id sẵn (`win-<tty>`,
                    // `sessions::add_shell_windows`, có từ trước). Nên con trỏ,
                    // câu chào, `/type`, `/shot` chạy y hệt phiên CLI, không một
                    // dòng xử lý riêng nào.
                    //
                    // Đó cũng là câu trả lời cho *"chưa kế thừa được các lệnh"*:
                    // thêm một hạng mục tiêu mà KHÔNG thêm một đường đi.
                    let (ack, buttons, inline_html) = match crate::keys::terminal_tabs() {
                        Err(e) => (
                            format!(
                                "⚠ không hỏi được Terminal: {}",
                                crate::exec::truncate(&e.to_string(), 200)
                            ),
                            Vec::new(),
                            None,
                        ),
                        Ok(tabs) => {
                            // Có CLI chạy ⟹ KHÔNG phải cửa sổ trần. Gõ lệnh
                            // shell vào giữa một chương trình đang chạy là thứ
                            // không lùi lại được — cùng luật với
                            // `add_shell_windows`, và cố ý đọc từ cùng một chỗ
                            // (`tab.cli()`) để hai danh sách không bao giờ lệch.
                            let bare: Vec<_> = tabs.iter().filter(|t| t.cli().is_none()).collect();
                            if bare.is_empty() {
                                // 🔴 Đếm ĐÚNG thứ đang nói tới. Bản đầu in
                                // `tabs.len()` kèm chữ "đang chạy CLI" — mà đó
                                // là TỔNG số tab, nên khi phép đo hỏng và trả
                                // rỗng, câu trả lời thành *"0 cửa sổ đang chạy
                                // CLI"* trên một cái máy đang mở sáu cửa sổ.
                                // Một con số dán nhầm nhãn còn tệ hơn không có
                                // số: nó nghe như một phép đo.
                                let msg = if tabs.is_empty() {
                                    "Không có cửa sổ Terminal nào đang mở.\nMở một cái: /new"
                                        .to_string()
                                } else {
                                    format!(
                                        "Không có cửa sổ Terminal trần nào — cả {} cửa sổ đều đang chạy CLI (xem /session).\n\
                                         Mở một cái: /new",
                                        tabs.len()
                                    )
                                };
                                (msg, Vec::new(), None)
                            } else {
                                let mut out = format!("🖥 {} cửa sổ Terminal trần:\n", bare.len());
                                let mut btns: Vec<(String, String)> = Vec::new();
                                // Bản HTML đi song song: mỗi cửa sổ MỘT dòng,
                                // hai đích chạm nằm ngay trên dòng ấy. `out`
                                // (chữ trần + nút đáy) ở lại làm đường lui.
                                let mut html =
                                    format!("🖥 {} cửa sổ Terminal trần:\n\n", bare.len());
                                for tb in &bare {
                                    // 🔴 KHÔNG in "dấu nhắc trống" cho MỌI hàng
                                    // — Hà 2026-08-16: *"trạng thái có đang chạy
                                    // giở gì không"*.
                                    //
                                    // Bản cũ dán đúng một câu ấy vào cả danh
                                    // sách, kể cả hàng `busy = true`. Mà "trần"
                                    // ở đây chỉ nghĩa là KHÔNG chạy CLI trợ lý
                                    // (`tab.cli()`); một cửa sổ đang `tail -f`,
                                    // đang build, đang mở `vim` thì vẫn vào danh
                                    // sách này — và đóng nó là mất việc đang
                                    // chạy. Nói "dấu nhắc trống" ở đó là huba
                                    // khai một trạng thái nó chưa từng hỏi.
                                    //
                                    // `procs` là thứ Terminal đã trả về trong
                                    // cùng lượt dò, nên câu này không tốn thêm
                                    // một lượt osascript nào.
                                    let doing = if tb.busy {
                                        let names: Vec<&str> = tb
                                            .procs
                                            .iter()
                                            .map(|p| p.trim())
                                            .filter(|p| {
                                                !p.is_empty()
                                                    && *p != "login"
                                                    && !p.trim_start_matches('-').eq("zsh")
                                                    && !p.trim_start_matches('-').eq("bash")
                                            })
                                            .collect();
                                        if names.is_empty() {
                                            "đang chạy gì đó".to_string()
                                        } else {
                                            format!("đang chạy: {}", names.join(", "))
                                        }
                                    } else {
                                        "dấu nhắc trống".to_string()
                                    };
                                    out.push_str(&format!(
                                        "\n{} {}\n    {doing}",
                                        if tb.busy { "🟢" } else { "⚪" },
                                        tb.tty
                                    ));
                                    // 🔴 MỘT CỬA SỔ = MỘT DÒNG, và hai đích chạm
                                    // nằm NGAY TRÊN dòng ấy — Hà 2026-08-17:
                                    // *"danh sách đó mỗi cái và nút nằm trên 1
                                    // dòng"*, sau khi 8 cửa sổ đẻ ra 16 cái nút
                                    // xếp dọc, hai cái một cặp giống hệt nhau.
                                    //
                                    // Cái nút chỉ nằm được dưới đáy tin; thứ đặt
                                    // được giữa chữ là LIÊN KẾT. Cùng cách ☑ bám
                                    // dòng lựa chọn và ▶️ bám dòng lệnh.
                                    let mark = if tb.busy { "🟢" } else { "⚪" };
                                    let open = crate::telegram::deep_link(&format!("w_{}", tb.tty));
                                    let shut =
                                        crate::telegram::deep_link(&format!("wx_{}", tb.tty));
                                    match (open, shut) {
                                        (Some(o), Some(c)) => html.push_str(&format!(
                                            "{mark} <code>{}</code> · <a href=\"{}\">🖥 vào</a> · <a href=\"{}\">⏹ đóng</a> — {}\n",
                                            crate::telegram::html_escape(&tb.tty),
                                            crate::telegram::html_escape(&o),
                                            crate::telegram::html_escape(&c),
                                            crate::telegram::html_escape(&doing),
                                        )),
                                        // Chưa biết tên bot ⟹ không dựng được
                                        // liên kết. Rơi về khối nút cũ chứ không
                                        // im: mất chỗ bấm là mất cả tính năng.
                                        _ => {
                                            let id = format!(
                                                "{}{}",
                                                crate::sessions::SHELL_ID_PREFIX,
                                                tb.tty
                                            );
                                            btns.push((
                                                format!("🖥 {}", tb.tty),
                                                format!("sess:{id}"),
                                            ));
                                            btns.push((
                                                format!("⏹ {}", tb.tty),
                                                format!("close:{id}"),
                                            ));
                                        }
                                    }
                                }
                                let foot = "\n🖥 vào để làm việc với cửa sổ · ⏹ đóng nó · /new mở cửa sổ mới\n\
                                     Cửa sổ đang chạy dở (🟢) thì huba hỏi lại trước khi đóng.";
                                out.push('\n');
                                out.push_str(foot);
                                html.push_str(&crate::telegram::html_escape(foot));
                                // Chỉ đi đường HTML khi MỌI hàng dựng được liên
                                // kết: nửa nọ nửa kia thì cùng một danh sách có
                                // hàng bấm được hàng không, đúng thứ khó đoán
                                // nhất cho người đọc.
                                let inline = btns.is_empty().then_some(html);
                                (out, btns, inline)
                            }
                        }
                    };
                    // Gửi ngay tại đây khi có nút — để `reply_in_channel` gửi
                    // thêm lần nữa là chủ máy nhận hai tin cùng nội dung, một
                    // cái bấm được một cái không. Cùng hình dạng với `/session`.
                    let mut sent = false;
                    if adapter == crate::telegram::NAME {
                        if let Some(tg) = crate::telegram::inbox() {
                            // Đích chạm nằm TRONG chữ thì gửi bản HTML; không
                            // dựng được liên kết thì mới tới khối nút ở đáy.
                            match (&inline_html, buttons.is_empty()) {
                                (Some(h), _) => match tg.send_html(h) {
                                    Ok(()) => sent = true,
                                    Err(e) => logging::error(
                                        "telegram_ack_failed",
                                        json!({ "err": e, "what": "terminal_inline" }),
                                    ),
                                },
                                (None, false) => match tg.send_buttons(&ack, &buttons) {
                                    Ok(()) => sent = true,
                                    // Hỏng thì rơi về đường chữ thường, đừng
                                    // nuốt: thà một tin không nút còn hơn im.
                                    Err(e) => logging::error(
                                        "telegram_ack_failed",
                                        json!({ "err": e, "what": "terminal_buttons" }),
                                    ),
                                },
                                (None, true) => {}
                            }
                        }
                    }
                    if !sent {
                        reply_in_channel(db, cfg, adapter, cmd, &ack);
                    }
                    Some(ack)
                } else {
                    // 🔴 `/terminal <lệnh>` ĐÃ BỎ, 2026-08-15. Hà chốt lại vai
                    // của ba động từ, và nó gọn hơn hẳn bản cũ:
                    //
                    //   /new        → động từ MỞ duy nhất (trần → +CLI → +chữ)
                    //   /terminal   → liệt kê cửa sổ TRẦN, không chạy gì
                    //   /session    → liệt kê cửa sổ đang chạy CLI
                    //
                    // Bản cũ để `/terminal` vừa LIỆT KÊ vừa MỞ, tuỳ có tham số
                    // hay không — hai việc khác hẳn nhau đội chung một tên, và
                    // `/new` thì mở một kiểu khác nữa. Ba đường mở, ba chỗ
                    // chép tay cùng một chặng.
                    //
                    // Lối thoát hiểm (sudo/ssh/passwd cần tty thật) KHÔNG mất,
                    // và còn khá hơn: `/new` trần cho một cửa sổ, cửa sổ ấy lên
                    // danh sách `/terminal` dưới id `win-<tty>`, rồi `/type` gõ
                    // vào nó và `/shot` đọc lại. Bản cũ chạy được một dòng rồi
                    // câm — chính câu trả lời của nó thừa nhận: *"kết quả nằm
                    // TRÊN cửa sổ ấy, không về đây"*.
                    let ack = format!(
                        "⚠ `/terminal` nay chỉ LIỆT KÊ cửa sổ trần.\n\
                         Mở cửa sổ mới: `/new`  ·  rồi gõ vào nó: `/type <win-ttysNNN> {}`",
                        crate::exec::truncate(cmd.arg.trim(), 60)
                    );
                    reply_in_channel(db, cfg, adapter, cmd, &ack);
                    Some(ack)
                }
            }
            CommandKind::Accounts => {
                // Một ảnh chụp thật, không phải con số nhớ từ lượt trước: câu
                // hỏi "phiên nào đang chạy bằng tài khoản nào" chỉ đúng ở thì
                // hiện tại. Hạn mức thì lấy bản đã đo sẵn (5 phút một lượt),
                // nên lệnh này không đẻ thêm tiến trình `claude` nào.
                let live = crate::sessions::snapshot(cfg);
                let ack =
                    crate::runtime::accounts_say(cfg, &live, chrono::Utc::now().timestamp_millis());
                reply_in_channel(db, cfg, adapter, cmd, &ack);
                Some(ack)
            }
            CommandKind::Web => {
                // 🔴 Hà 2026-08-23: *"Cổng điều khiển browser thế nào rồi"*.
                // Đúng một "gap" theo phép thử cầu nối: ngồi ở máy thì mở
                // Chrome là một cú click, từ điện thoại thì trước lượt này
                // không có đường nào.
                // HAI ĐỘNG CƠ, HAI CÂU HỎI KHÁC NHAU — không phải hai bản chép.
                //
                // `/web may` hỏi *"cái Chrome ĐANG MỞ TRÊN MÁY có gì"*: đường
                // AppleScript, thấy đúng phiên đăng nhập của chủ máy, và cần
                // quyền Tự động hoá chỉ cấp được khi ngồi trước máy.
                //
                // Mọi dạng còn lại đi bằng TRÌNH DUYỆT CỦA HUBA (Playwright,
                // `crate::web`): hồ sơ riêng, chạy ẩn, **không quyền macOS
                // nào** — nên nó là thứ dùng được từ điện thoại, đúng câu Hà
                // hỏi 23/08: *"Sao khong dùng playwright"*.
                let arg = cmd.arg.trim();
                // 🔴 MẶC ĐỊNH LÀ TRÌNH DUYỆT THẬT — Hà 2026-08-23, sau khi về
                // tới máy: *"tôi ngồi máy rồi chuyển qua điều khiển trình duyệt
                // thật đi"*. Bản ẩn (Playwright) lui về sau chữ `an`: nó là
                // đường cho lúc Ở XA, khi không ai tích được ô quyền Tự động
                // hoá; ngồi ở máy thì thứ đáng lái là cái trình duyệt đang mở
                // trước mặt, với đúng phiên đăng nhập của nó.
                let an = if arg == "an" || arg == "ẩn" {
                    Some("")
                } else {
                    arg.strip_prefix("an ").or_else(|| arg.strip_prefix("ẩn "))
                };
                let mut sent = false;
                let ack = if an.is_none() {
                    // Chrome TRÊN MÁY: danh sách tab, mỗi hàng một đích chạm —
                    // cùng bố cục danh sách phiên, dùng chung `tap_rows_html`.
                    let (ack, taps) = web_route(arg);
                    // 🔴 NÓI TRƯỚC KHI NÓ NÓI DỐI. Bản ẩn của huba chạy từ cùng
                    // một bundle `com.google.Chrome`, nên khi nó còn sống thì
                    // Apple Events trỏ vào NÓ — đo 23/08: `tabs()` đọc ra đúng
                    // một tab của bản ẩn, giết bản ẩn đi thì Chrome thật trả về
                    // 0 cửa sổ. Không phân biệt được từ trong AppleScript, nên
                    // chỗ duy nhất trung thực được là ĐÂY.
                    let ack = match crate::web::an_dang_chay(&cfg.hub_home) {
                        Some(pid) => format!(
                            "⚠ Trình duyệt ẩn của huba đang chạy (pid {pid}), mà Apple Events \
                             không phân biệt được nó với Chrome của bạn — danh sách dưới đây có \
                             thể là của NÓ. Gõ `/web an tắt` rồi gọi lại.\n{ack}"
                        ),
                        None => ack,
                    };
                    if adapter == crate::telegram::NAME && !taps.is_empty() {
                        if let Some(tg) = crate::telegram::inbox() {
                            let (html, linked) =
                                tap_rows_html(&crate::telegram::strip_markdown(&ack), &taps);
                            // Cùng luật all-or-nothing với danh sách phiên: nửa
                            // danh sách bấm được nửa không thì ngón tay học sai
                            // một lần rồi thôi tin cả cái danh sách.
                            if linked == taps.len() {
                                match tg.send_html(&html) {
                                    Ok(()) => {
                                        sent = true;
                                        logging::info(
                                            "web_taps_sent",
                                            json!({ "rows": taps.len(), "linked": linked }),
                                        );
                                    }
                                    Err(e) => logging::error(
                                        "telegram_ack_failed",
                                        json!({ "err": e, "what": "web_taps" }),
                                    ),
                                }
                            }
                        }
                    }
                    ack
                } else {
                    let (ack, anh) = crate::web::route(&cfg.hub_home, an.unwrap_or(""));
                    if adapter == crate::telegram::NAME {
                        if let (Some(tg), Some(path)) = (crate::telegram::inbox(), anh.as_ref()) {
                            // Ảnh TRƯỚC, vì thứ người cầm điện thoại cần đầu
                            // tiên là NHÌN THẤY trang. Gửi hỏng thì rơi về chữ.
                            match tg.send_photo(path, &ack) {
                                Ok(()) => sent = true,
                                Err(e) => logging::error(
                                    "telegram_ack_failed",
                                    json!({ "err": e, "what": "web_shot" }),
                                ),
                            }
                        }
                    }
                    ack
                };
                if !sent {
                    reply_in_channel(db, cfg, adapter, cmd, &ack);
                }
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
                let live = crate::sessions::snapshot(cfg);
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
                // Bản bàn giao là CHỮ CỦA PHIÊN — đi qua cửa định dạng, nên
                // lệnh `cd … && claude --resume …` trong đó bấm được như mọi
                // dòng lệnh khác.
                let ack_sid = target
                    .as_ref()
                    .map(|s| s.session_id.clone())
                    .unwrap_or_default();
                reply_from_session(db, cfg, adapter, cmd, &ack_sid, &ack);
                Some(ack)
            }
            CommandKind::New => {
                // `<dự án> <việc>` — the project decides the folder, and only a
                // folder huba already knows about is accepted: a typo must not
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
                // Tài khoản lạ thì TỪ CHỐI, đừng lặng lẽ rơi về mặc định: mở
                // phiên nhầm tài khoản là mở nhầm cả kho phiên.
                let known_accounts: Vec<String> = cfg
                    .claude_accounts_or_ambient()
                    .iter()
                    .map(|a| a.name.clone())
                    .collect();
                // `@tài-khoản` đứng ngay sau tên dự án: `/new huba @acc2 việc…`.
                // Không có thì dùng tài khoản mặc định — giữ nguyên cách gõ cũ.
                let (account, task) = match (flag_account, task.trim().strip_prefix('@')) {
                    (Some(a), _) => (Some(a), task),
                    (None, Some(rest)) => {
                        let (acc, rest) =
                            rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                        (Some(acc.trim().to_string()), rest)
                    }
                    // 🔴 TÊN TÀI KHOẢN GÕ TRẦN ở đầu dòng, 2026-08-15.
                    //
                    // Hà: *"Rõ ràng mở phiên mới dwork là acc3 sau xem lại thành
                    // acc1 là sao"*. Đo nguyên văn trong log (02:14:29Z):
                    // `/new acc3 dwork` ⟹ `new_window_opened task:"[] acc3
                    // dwork"`, tài khoản mặc định. Tức `acc3` không được đọc là
                    // tài khoản, nó thành ĐỀ BÀI — phiên mở trên acc1 và được
                    // giao đúng chuỗi chữ `acc3 dwork` để làm.
                    //
                    // Danh sách phiên KHÔNG nói dối: phiên ấy thật sự nằm ở
                    // acc1. Chỗ hỏng nằm ở đây, sớm hơn một bước.
                    //
                    // Đây KHÔNG phải nới cửa đoán: `known_accounts` là danh
                    // sách huba tự đọc từ cấu hình, nên "token này có phải tên
                    // một tài khoản không" là một phép ĐO, khớp chính xác cả
                    // chuỗi. Cùng lối nghĩ đã ghi ở `looks_like_project`: thay
                    // một cái tên viết sẵn bằng một câu hỏi trả lời được.
                    //
                    // Vẫn ghi log, vì đây là lượt huba SỬA chữ chủ máy gõ — và
                    // luật của tệp này là mọi lượt như thế phải kiểm được.
                    (None, None) => match lift_bare_account(task, &known_accounts) {
                        Some((acc, rest)) => {
                            logging::info(
                                "new_bare_account_lifted",
                                json!({ "account": acc, "task": rest }),
                            );
                            (Some(acc.to_string()), rest)
                        }
                        None => (None, task),
                    },
                };
                let bad_account = account
                    .as_ref()
                    .filter(|a| !known_accounts.contains(a))
                    .cloned();
                // 🔴 `/new <id>` = MỞ LẠI một phiên đã tắt — thay cho `/tell`.
                //
                // Hà 2026-08-15: *"lệnh tell là không cần thiết?"* · *"vì trên
                // tele tôi chỉ gõ text bình thường thôi"*. Đúng, và đường cũ
                // sai sâu hơn một động từ thừa: `sessions::tell` chạy
                // `claude -p --resume`, tức MỘT LƯỢT rồi thôi, không cửa sổ, có
                // tiêu hạn mức — thứ chủ máy không bao giờ tự làm khi ngồi ở
                // máy. Nay nó về đúng động từ MỞ, và mở ra một phiên SỐNG: gõ
                // tiếp được, `/shot` nhìn được, miễn phí.
                //
                // Tài khoản KHÔNG đoán: `--resume` chạy nhầm tài khoản là mở
                // nhầm cả kho phiên. Hỏi sổ (`Mark::a`) rồi tới phiên vừa dừng;
                // không nơi nào biết thì TỪ CHỐI và nói ra, thay vì im lặng rơi
                // về mặc định.
                let resume = resume_target(&rest, db);
                let known = known_projects(cfg);
                let dir = crate::config::project_dir(cfg, name);
                // 🔴 `/new` TRẦN = một cửa sổ Terminal trần, không dựng CLI.
                //
                // Hà 2026-08-15: *"nếu `/new` để trống thì nó sẽ chỉ khởi tạo
                // terminal, như vậy không cần lệnh terminal nữa"*. Một động từ
                // MỞ, ba mức — mỗi tham số thêm một bước:
                //   /new              → cửa sổ trần
                //   /new acc3         → + dựng CLI đúng tài khoản
                //   /new acc3 <chữ>   → + gõ đề bài ⟹ đã khởi tạo xong một phiên
                //
                // Cửa sổ trần KHÔNG mất tăm: nó lên danh sách `/terminal` dưới
                // id `win-<tty>` (`sessions::add_shell_windows`, có sẵn từ
                // trước), nên `/type` gõ được và `/shot` đọc lại được — tức lối
                // thoát hiểm cho sudo/ssh/passwd còn nguyên, và còn hai chiều
                // chứ không một chiều như `/terminal <lệnh>` cũ.
                let ack = if account.is_none() && name.is_empty() && task.trim().is_empty() {
                    match crate::sessions::open_bare_terminal() {
                        // Cửa sổ trần mở ở `~`; phiên CLI mới mở ở gốc
                        // workspace. Hai chỗ khác nhau, và câu trả lời phải nói
                        // ra — đó là thứ vừa đổi so với một cửa sổ chủ máy tự mở.
                        // 🔴 CHUYỂN CON TRỎ NGAY, và nói ngắn — Hà 2026-08-16:
                        // *"sao lệnh new trống xong giải thích dài dòng thế?
                        // tạo xong thì focus vào terminal đấy luôn rồi còn
                        // chứ?"* · *"gõ lệnh vào tele rồi gửi là xong"* ·
                        // *"gửi lệnh /shot là xong"*.
                        //
                        // Anh đúng ở cả hai vế, và vế thứ nhất là một lỗi thật
                        // chứ không phải chuyện câu chữ: bản cũ mở cửa sổ rồi
                        // KHÔNG chuyển con trỏ, nên nó phải đi dạy hai cú pháp
                        // dài (`/type win-<tty> …`, `/shot win-<tty>`) để bù.
                        // Mở một cửa sổ rồi bắt người ta gõ id của nó vào mọi
                        // câu sau đó thì không phải cây cầu — ngồi ở máy, mở
                        // xong là con trỏ đã ở đó.
                        //
                        // Chuyển được ngay vì tty CÓ SẴN lúc này; phiên CLI thì
                        // không (id chưa sinh ra, phải chờ nhật ký), và đó mới
                        // là lý do câu chào của nhánh kia nói *"con trỏ CHƯA
                        // chuyển"*. Hai nhánh khác nhau ở dữ kiện, không ở luật.
                        Ok((_w, tty)) => {
                            let id = format!("{}{tty}", crate::sessions::SHELL_ID_PREFIX);
                            if let Err(e) = db.set_cursor(FOCUS_SESSION_KEY, &id) {
                                // Không nuốt: con trỏ không chuyển mà câu chào
                                // vẫn khoe "gõ ở đây là vào nó" thì chữ tiếp
                                // theo của chủ máy đi vào phiên CŨ.
                                logging::error(
                                    "focus_after_bare_terminal_failed",
                                    json!({ "err": e.to_string(), "id": id }),
                                );
                                format!(
                                    "🖥 Đã mở cửa sổ Terminal trần ({tty}) ở ~\n\
                                     ⚠ nhưng con trỏ CHƯA chuyển được sang nó — bấm nó trong /terminal trước khi gõ."
                                )
                            } else {
                                // 🔴 In THẲNG danh sách tài khoản — Hà
                                // 2026-08-16: *"đoạn này giở hơi hơn, ghi luôn
                                // danh sách tài khoản có thể chạy ra cho nhẹ"*.
                                // Bản cũ viết một câu giải thích kèm ĐÚNG MỘT
                                // ví dụ gõ cứng (`/new acc3`) và một đường dẫn
                                // dài — tức bắt người đọc suy ra cái danh sách
                                // mà huba đang cầm sẵn trong tay.
                                let accs = cfg
                                    .claude_accounts_or_ambient()
                                    .iter()
                                    .map(|a| a.name.clone())
                                    .collect::<Vec<_>>()
                                    .join(" · ");
                                format!(
                                    "🖥 Đã mở cửa sổ Terminal trần ({tty}) ở ~ — đang theo nó.\n\
                                     Gõ chữ ở đây là chạy trong nó · /shot để nhìn màn.\n\
                                     Phiên CLI: /new {accs}"
                                )
                            }
                        }
                        // Không nuốt: mở cửa sổ hỏng thì người bấm phải biết,
                        // không thì họ ngồi chờ một cái cửa sổ không tồn tại.
                        Err(e) => {
                            let msg = crate::logging::err_chain(&e);
                            logging::error("bare_terminal_open_failed", json!({ "err": msg }));
                            format!(
                                "⚠ chưa mở được cửa sổ: {}",
                                crate::exec::truncate(&msg, 200)
                            )
                        }
                    }
                } else if let Some(a) = bad_account {
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
                            // Thời gian ấy không phải lãng phí: huba chờ nhật ký
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
                                // `/new <id>` ⟹ nối tiếp phiên ấy, và tài khoản
                                // do CHÍNH PHIÊN quyết, không phải cờ `-a`.
                                let (resume_id, acc, task) = match resume.clone() {
                                    Some((id, acc, tail)) => (Some(id), Some(acc), tail),
                                    None => (None, account.clone(), task.to_string()),
                                };
                                watch_new_session(NewSession {
                                    cfg: cfg.clone(),
                                    name: name.to_string(),
                                    dir: d.clone(),
                                    task,
                                    account: acc.clone(),
                                    resume: resume_id.clone(),
                                    adapter: adapter.to_string(),
                                    chat_id: cmd.chat_id.clone(),
                                });
                                match resume_id {
                                    Some(id) => format!(
                                        "⌨ Đang MỞ LẠI phiên {} bằng {}…\nCửa sổ Terminal thật, hội thoại cũ nối tiếp — gõ được, /shot nhìn được.\n⚠ Con trỏ CHƯA chuyển — gõ chữ lúc này vẫn vào phiên đang theo.",
                                        &id[..8.min(id.len())],
                                        acc.as_deref().unwrap_or("tài khoản mặc định")
                                    ),
                                    None => format!(
                                "⌨ Đang mở cửa sổ Terminal{}…\nhub báo lại khi phiên chào đời (thường 15–60 giây, vì nó chờ nhật ký phiên sinh ra để biết id).\n⚠ Con trỏ CHƯA chuyển — gõ chữ lúc này vẫn vào phiên đang theo.",
                                account
                                    .as_deref()
                                    .map(|a| format!(" bằng {a}"))
                                    .unwrap_or_else(|| " bằng tài khoản mặc định".to_string()),
                            ),
                                }
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
                // riêng của huba và do `mark_started_by_hub` dán vào. Thiếu bước
                // này thì mọi phiên đều "không phải của huba", và từ 2026-08-11
                // — khi `/new` mở cửa sổ thật — nó biến thành lỗi nhìn thấy
                // được: huba mở được cửa sổ rồi từ chối đóng chính nó, với câu
                // *"chỉ dừng được phiên do huba mở"*. Nhánh phiên nền không lộ
                // vì nó xét `kind`, không xét quyền sở hữu.
                let mut live = crate::sessions::snapshot(cfg);
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
                                            "⏹ Đã dừng phiên {}. Hội thoại vẫn còn — mở lại bằng `/new {}` để nói tiếp.",
                                            crate::sessions::shown(s),
                                            &s.session_id[..8.min(s.session_id.len())]
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
                let mut live = crate::sessions::snapshot(cfg);
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
                        // Cửa sổ TRẦN không thuộc tài khoản nào, nên `s.account`
                        // rỗng — và bản cũ vẫn in cặp ngoặc, ra
                        // *"Đóng hẳn phiên ⬜ cửa sổ ttys005 ()?"*. Hà đọc câu ấy
                        // sáu lần liên tiếp lúc 12:25–12:29 ngày 16/08 khi dọn
                        // mấy cửa sổ trần. Ngoặc rỗng không phải lỗi chính tả:
                        // nó là chỗ huba khai một dữ kiện mà chính nó biết là
                        // không có.
                        let what = if s.account.trim().is_empty() {
                            format!("Đóng hẳn {}?", crate::sessions::shown(s))
                        } else {
                            format!(
                                "Đóng hẳn phiên {} ({})?",
                                crate::sessions::shown(s),
                                s.account
                            )
                        };
                        // Luật "có cần hỏi lại không" nằm ở
                        // `sessions::closing_needs_confirm` — một chỗ, kiểm được.
                        let refusal = if crate::sessions::closing_needs_confirm(s) {
                            ask_owner(db, cfg, adapter, cmd, &what, "đóng phiên nào")
                        } else {
                            logging::info(
                                "close_without_asking",
                                json!({ "session": s.session_id,
                                        "why": "cửa sổ trần đang ở dấu nhắc trống — không có việc chạy dở để mất" }),
                            );
                            None
                        };
                        if let Some(refusal) = refusal {
                            refusal
                        } else {
                            match crate::sessions::close_session(cfg, s) {
                                Ok(win) => {
                                    remember_stopped(db, s);
                                    logging::info(
                                        "session_closed",
                                        json!({ "session": s.session_id, "kind": s.kind,
                                                "window": match win {
                                                    crate::sessions::Closing::Background => None,
                                                    crate::sessions::Closing::Closed(w)
                                                    | crate::sessions::Closing::Hidden(w)
                                                    | crate::sessions::Closing::Exiting(w) => Some(w),
                                                } }),
                                    );
                                    // Nói ĐÚNG cái vừa xảy ra, và ở đây "vừa
                                    // xảy ra" mới là gõ `/exit` — cửa sổ chưa
                                    // đóng, nó vào sổ chờ. Khai "đã đóng" lúc
                                    // này là kể một việc chưa xảy ra, đúng thứ
                                    // luật 3 của dự án cấm.
                                    match win {
                                        crate::sessions::Closing::Background => format!(
                                            "⏹ Đã dừng phiên nền {} — nó không có cửa sổ nào để đóng. Hội thoại vẫn còn.",
                                            crate::sessions::shown(s)
                                        ),
                                        // …và ca này thì ĐÃ đóng thật, đã kiểm
                                        // bằng số tab chứ không bằng mã trả về
                                        // (xem `keys::window_gone`). Cửa sổ trần
                                        // không có CLI nào để chờ, nên không có
                                        // gì phải hẹn.
                                        crate::sessions::Closing::Closed(_) => format!(
                                            "⏹ Đã đóng {} — cửa sổ trần, shell đã thoát từ trước nên không có gì để chờ.",
                                            crate::sessions::shown(s)
                                        ),
                                        // Ẩn ≠ đóng, và chủ máy phải biết đúng
                                        // cái vừa xảy ra với máy của mình.
                                        crate::sessions::Closing::Hidden(_) => format!(
                                            "⏹ Terminal KHÔNG chịu đóng {} (lỗi của nó, huba đã thử đủ cách) — nên huba ẩn cửa sổ ấy đi. \
                                             Nó biến mất khỏi mọi danh sách của huba; ⌘W khi anh ngồi máy là hết hẳn.",
                                            crate::sessions::shown(s)
                                        ),
                                        crate::sessions::Closing::Exiting(w) => {
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
            // Huba tự dựng lại chính nó. Trả lời TRƯỚC, khởi động lại SAU —
            // bước cuối giết chính tiến trình đang gõ câu trả lời này.
            CommandKind::Upgrade => {
                let ack = match crate::runtime::self_install(cfg) {
                    Ok(msg) => format!("🔧 {msg}\nĐang khởi động lại hubad…"),
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
            // `/anh` đi CHUNG khối này với `/shot`: cùng cách tìm phiên, cùng
            // cách tra cửa sổ, chỉ khác cái nó làm khi đã có cửa sổ trong tay.
            CommandKind::Type
            | CommandKind::Key
            | CommandKind::Shot
            | CommandKind::Photo
            | CommandKind::Front
            | CommandKind::Clean
            | CommandKind::Clear
            | CommandKind::Tab
            | CommandKind::Pick => {
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
                    None => Some(crate::sessions::snapshot(cfg)),
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
                // Hộp chọn đọc từ MÀN GỐC, giữ lại để dựng nút. Đo ở đây, dùng
                // ở dưới — chứ KHÔNG đo lại trên `ack`: `ack` có chữ của chính
                // huba, và chính chỗ ấy đã làm hỏng phép đo (xem `ScreenReport`).
                let mut shot_choices: Vec<(String, String)> = Vec::new();
                // Chữ trong ô nhập, đo trên ẢNH MÀN trước khi huba nối thêm khu
                // nào — xem chỗ gán bên dưới.
                let mut shot_box: Option<String> = None;
                // Màn có dòng `Submit` ⟹ gắn ✅ vào đó. Điền ở nhánh `/shot` lẫn
                // nhánh bấm phím, vì cả hai đều trả về một bảng bấm được.
                let mut shot_submit = false;
                let ack = match target {
                    None if want.is_empty() => {
                        "⚠ chưa mở phiên nào. Chạm một phiên rồi gõ.".to_string()
                    }
                    // 🔴 "Không có trong danh sách" ≠ "không tồn tại". Nếu lượt
                    // hỏi vừa rồi MÙ với tài khoản nào đó thì danh sách ấy
                    // thiếu, và nói "không thấy phiên" là khẳng định một điều
                    // huba không biết — đúng con bug đã vá ở cái loa, ở một chỗ
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
                            // 🔴 `/tab <n>` chạy TRƯỚC cả khối dưới, rồi để
                            // nhánh `/shot` vẽ lại màn — vì thứ chứng minh nó
                            // chạy được là chính cái màn ĐÃ ĐỔI (Hà 2026-08-19:
                            // *"khi shot nó phải thay đổi"*), không phải một câu
                            // "đã gửi phím".
                            //
                            // `typed` bị dọn về rỗng sau khi đi: nhánh `/shot`
                            // đọc nó như SỐ DÒNG (`/shot 80`), nên để nguyên số
                            // tab ở đó là xin một ảnh màn 2 dòng.
                            let (typed, tab_err) = if matches!(cmd.kind, CommandKind::Tab) {
                                match tab_move(&s, w, &typed) {
                                    Ok(()) => (String::new(), None),
                                    Err(e) => (String::new(), Some(e)),
                                }
                            } else {
                                (typed, None)
                            };
                            if let Some(msg) = tab_err {
                                msg
                            } else
                            // 🔴 `/anh` — ẢNH THẬT, và nó cũng chỉ NHÌN.
                            //
                            // Hà 2026-08-17: *"Thêm lệnh chụp ảnh màn hình để
                            // tôi xem thực sự đang có gì trên màn hình"* ·
                            // *"Focus tới phiên thật"*. Câu sau là cả thiết kế:
                            // chụp bừa cả màn chỉ nói "máy đang mở gì đó", nên
                            // đưa ĐÚNG cửa sổ phiên ra trước rồi mới bấm máy.
                            if matches!(cmd.kind, CommandKind::Photo) {
                                let path = std::env::temp_dir()
                                    .join(format!("huba-man-hinh-{}.png", std::process::id()));
                                match crate::keys::photograph_window(w, &path) {
                                    Ok(()) => match crate::telegram::inbox() {
                                        Some(tg) => {
                                            let cap = format!(
                                                "📸 {} — ảnh thật, cửa sổ đã đưa ra trước",
                                                crate::sessions::shown(&s)
                                            );
                                            let out = match tg.send_photo(&path, &cap) {
                                                Ok(()) => format!(
                                                    "📸 Ảnh màn hình của {} ở trên.",
                                                    crate::sessions::shown(&s)
                                                ),
                                                Err(e) => format!(
                                                    "⚠ chụp được nhưng KHÔNG gửi được ảnh: {}",
                                                    crate::exec::truncate(&e, 200)
                                                ),
                                            };
                                            // Ảnh nằm trong thư mục tạm và đã đi
                                            // rồi — giữ lại là để lại một tấm
                                            // ảnh màn hình của chủ máy trên đĩa.
                                            let _ = std::fs::remove_file(&path);
                                            out
                                        }
                                        None => "⚠ chưa có kênh Telegram nào để gửi ảnh".to_string(),
                                    },
                                    Err(e) => format!(
                                        "⚠ {}",
                                        crate::exec::truncate(&e.to_string(), 420)
                                    ),
                                }
                            } else if matches!(cmd.kind, CommandKind::Clean) {
                                // 🔴 `/clean` — dọn HÀNG CHỜ, và nói đúng con số.
                                //
                                // Hà 2026-08-18: *"Thêm lệnh clean xóa hết ở
                                // chờ"*. Câu trả lời mang ba trạng thái khác
                                // nhau vì việc của người đọc khác nhau: sạch rồi
                                // · vừa dọn xong N tin · dọn không được (và lúc
                                // ấy phải nói còn bao nhiêu, chứ không phải một
                                // câu "xong" cho một việc chưa xong).
                                // 🔴 DỌN NỐT Ô NHẬP — Hà 2026-08-26: *"Sửa lại
                                // lệnh clean … để cùng có tác dụng xóa text ở ô
                                // chat"*.
                                //
                                // Thứ tự bắt buộc: hàng chờ TRƯỚC, ô nhập SAU.
                                // `clear_queue` bấm `↑` để lôi từng tin trong
                                // hàng chờ NGƯỢC VÀO ô nhập rồi xoá — nên tin
                                // cuối cùng được lôi ra nằm lại đúng trong ô.
                                // Chính nó là thứ đổ chữ vào cái ô mà anh thấy
                                // vẫn còn. Xoá ô trước là xoá một cái ô sắp được
                                // đổ đầy trở lại.
                                let don = crate::keys::clear_queue(w);
                                let o_sach = matches!(crate::keys::clear_box(w), Ok(true));
                                let con_o = if o_sach {
                                    String::new()
                                } else {
                                    " ⚠ ô nhập vẫn còn chữ — gõ `/clear` lần nữa.".to_string()
                                };
                                match don {
                                    Ok((0, 0)) => format!(
                                        "🧹 {} không có tin nào trong hàng chờ; ô nhập đã sạch.{con_o}",
                                        crate::sessions::shown(&s)
                                    ),
                                    Ok((removed, 0)) => format!(
                                        "🧹 Đã xoá {removed} tin khỏi hàng chờ của {} — hàng chờ trống, ô nhập đã sạch. \
                                         Lượt đang chạy KHÔNG bị cắt (muốn cắt thì `/key esc`).{con_o}",
                                        crate::sessions::shown(&s)
                                    ),
                                    Ok((removed, left)) => format!(
                                        "⚠ mới xoá được {removed} tin, còn {left} nằm lại trong hàng chờ của {} \
                                         — phím ↑ thôi lấy được tin ra (xem log `clean_queue_stuck`). \
                                         Gõ `/clean` lần nữa, hoặc dọn tay ở máy.",
                                        crate::sessions::shown(&s)
                                    ),
                                    Err(e) => format!(
                                        "⚠ không dọn được hàng chờ: {}",
                                        crate::exec::truncate(&e.to_string(), 200)
                                    ),
                                }
                            } else if matches!(cmd.kind, CommandKind::Clear) {
                                // 🔴 `/clear` — CHỈ ô nhập, hàng chờ giữ nguyên.
                                //
                                // Hà 2026-08-26: *"thêm lệnh clear để cùng có
                                // tác dụng xóa text ở ô chat"*. Phép xoá vốn đã
                                // có, nhưng nấp sau `/key clear` — một lệnh nói
                                // về PHÍM, trong khi đây không phải một phím mà
                                // là "xoá đúng bấy nhiêu ký tự đang có".
                                //
                                // Giữ riêng với `/clean` vì hậu quả khác nhau:
                                // ở đây chỉ mất chữ CHƯA gửi, còn `/clean` mất
                                // cả tin đã xếp hàng chờ chạy — thứ không lấy
                                // lại được.
                                match crate::keys::clear_box(w) {
                                    Ok(true) => format!(
                                        "🧽 Đã xoá ô nhập của {}. Hàng chờ giữ nguyên (muốn dọn cả thì `/clean`).",
                                        crate::sessions::shown(&s)
                                    ),
                                    Ok(false) => {
                                        logging::warn(
                                            "keys_clear_incomplete",
                                            json!({ "session": s.session_id,
                                                    "effect": "ô nhập vẫn còn chữ sau khi xoá — không khai là đã sạch" }),
                                        );
                                        format!(
                                            "⚠ ô nhập của {} vẫn còn chữ sau khi xoá — gõ `/clear` lần nữa, \
                                             hoặc xoá tay ở máy.",
                                            crate::sessions::shown(&s)
                                        )
                                    }
                                    Err(e) => format!(
                                        "⚠ không xoá được ô nhập: {}",
                                        crate::exec::truncate(&e.to_string(), 200)
                                    ),
                                }
                            } else if matches!(cmd.kind, CommandKind::Front) {
                                // 🔴 `/front` — chỉ ĐƯA RA TRƯỚC MẶT. Không
                                // chụp, không gõ, không gửi phím nào.
                                //
                                // Hà 2026-08-22: *"vậy muốn một phiên nổi lên
                                // thì làm thế nào"*, sau khi gõ `/focus` và
                                // không thấy gì. Trước route này đường duy nhất
                                // là `/anh` — tức phải trả giá một tấm PNG qua
                                // Telegram và cần quyền Screen Recording, chỉ để
                                // làm cái việc mà ngồi ở máy là một cú click.
                                //
                                // KHÔNG tin mã thoát của `osascript`: nó trả 0
                                // khi câu lệnh chạy xong, không khi cửa sổ đã ra
                                // trước. Hỏi lại chính Terminal xem ai đang
                                // đứng trước, rồi mới nói.
                                match crate::keys::bring_to_front(w) {
                                    Ok(()) => {
                                        // TUI + WindowServer cần một nhịp mới
                                        // sắp xong; đọc ngay là đọc thứ tự cũ
                                        // rồi báo hỏng cho một lượt ĐÚNG.
                                        std::thread::sleep(
                                            std::time::Duration::from_millis(400),
                                        );
                                        let front = crate::keys::front_window();
                                        logging::info(
                                            "front_window_raised",
                                            json!({ "session": s.session_id, "window": w,
                                                    "front": front.as_ref().ok().and_then(|f| *f) }),
                                        );
                                        match front {
                                            Ok(Some(f)) if f == w => format!(
                                                "🪟 {} đã ra trước mặt.",
                                                crate::sessions::shown(&s)
                                            ),
                                            // Đổi rồi mà không phải cửa sổ mình
                                            // xin ⟹ nói ra, đừng gắn dấu ✅ lên
                                            // một việc chưa chắc.
                                            Ok(Some(f)) => format!(
                                                "⚠ đã gọi cửa sổ của {} ra trước, nhưng Terminal nói \
                                                 cửa sổ đang đứng trước là {f} chứ không phải {w}. \
                                                 Có thể một hộp thoại của macOS đang đè lên.",
                                                crate::sessions::shown(&s)
                                            ),
                                            Ok(None) => format!(
                                                "⚠ đã gọi cửa sổ của {} ra trước, nhưng Terminal báo \
                                                 KHÔNG có cửa sổ nào — nên tôi chưa xác nhận được.",
                                                crate::sessions::shown(&s)
                                            ),
                                            // Hỏi lại không được thì việc kia
                                            // vẫn có thể đã xong — nói đúng thế,
                                            // không nói "hỏng".
                                            Err(e) => format!(
                                                "🪟 đã gọi cửa sổ của {} ra trước, nhưng KHÔNG kiểm lại được ({}) \
                                                 — nhìn màn hình giúp tôi.",
                                                crate::sessions::shown(&s),
                                                crate::exec::truncate(&e.to_string(), 120)
                                            ),
                                        }
                                    }
                                    Err(e) => {
                                        logging::warn(
                                            "front_window_failed",
                                            json!({ "session": s.session_id, "window": w,
                                                    "err": crate::logging::err_chain(&e) }),
                                        );
                                        format!(
                                            "⚠ không đưa được cửa sổ của {} ra trước: {}",
                                            crate::sessions::shown(&s),
                                            crate::exec::truncate(&e.to_string(), 200)
                                        )
                                    }
                                }
                            } else if matches!(cmd.kind, CommandKind::Shot | CommandKind::Tab) {
                                // Nút "gửi nhanh" chỉ dựng được khi biết màn có
                                // gì — nên đọc màn một lần, dùng cho cả hai.
                                // `/shot 80` — xin nhiều dòng hơn khi thứ cần
                                // nhìn nằm cao hơn cửa sổ mặc định.
                                let n = typed
                                    .trim()
                                    .parse::<usize>()
                                    .unwrap_or(SHOT_LINES);
                                // Lời cuối theo nhật ký, lấy TRƯỚC khi đọc màn:
                                // nó vừa là thước đo "màn hiện trọn chưa" (để
                                // `screen_report` quyết định có nới cửa sổ
                                // không), vừa là đường lùi khi nới hết cỡ vẫn
                                // thiếu — và "hết cỡ" là có thật: Terminal kẹp
                                // ở 61×206 trên máy này, xin 999 cũng chừng ấy.
                                // Một lượt dài hơn khung ấy thì chỉ nhật ký mới
                                // giữ được trọn.
                                let said = crate::sessions::last_say_by_id(
                                    cfg,
                                    &shot_sid,
                                    crate::sessions::SAY_MAX,
                                )
                                .map(|t| t.trim().to_string())
                                .filter(|t| !t.is_empty());
                                let rep = screen_report(&s, w, n, said.as_deref());
                                // 🔴 ☑ CHÈN THẲNG VÀO DÒNG LỰA CHỌN — Hà
                                // 2026-08-17: *"Sao không chèn trực tiếp vào văn
                                // bản lại đi chèn thêm xuống cuối"*.
                                //
                                // Bảng nhiều câu thì mã mang cả số câu (`1.<n>`)
                                // để ☑ đi bằng `pick_`; hộp một câu đi bằng
                                // `k_`. Chỉ CÂU ĐANG HIỆN mới chèn được — các
                                // câu sau chưa có mặt trên màn, nên chúng vẫn
                                // cần khu chữ ở cuối (xem `ask_command_lines`).
                                // Nhật ký nói bảng có nhiều câu; MÀN nói còn ô
                                // trống hay đã sang bước Review — xem
                                // `multi_question_screen`.
                                let table = multi_question_screen(
                                    shot_asking.as_ref().is_some_and(|a| !a.rest.is_empty()),
                                    &rep.text,
                                );
                                shot_choices = rep
                                    .choices
                                    .into_iter()
                                    .map(|(n, l)| {
                                        let code =
                                            if table { format!("1.{n}") } else { n.to_string() };
                                        (code, l)
                                    })
                                    .collect();
                                let mut out = rep.text;
                                // 🔴 MÀN KHÔNG CÓ LỜI NÀO CỦA PHIÊN ⟹ BÙ BẰNG
                                // NHẬT KÝ. Hà 2026-08-17, ảnh `/shot` của
                                // `[AI/onghut]` ra nguyên một tệp mã kèm
                                // *"… +35 lines"*: *"Sao phiên này hiện như vậy,
                                // biết đằng nào làm tiếp"*.
                                //
                                // `/shot` là ẢNH của 40 dòng cuối cửa sổ, và nó
                                // trung thực: phiên vừa in một tệp nên phần nó
                                // NÓI bị đẩy lên trên khung. Ảnh đúng mà vô
                                // dụng — người ở xa không có cách nào cuộn.
                                //
                                // Nhật ký thì giữ nguyên lời cuối. Chỉ bù khi
                                // màn thật sự không có lời nào (`⏺` là dấu
                                // `claude` in trước mỗi câu của nó) và cũng
                                // không có hộp chọn — bù mọi lượt là chép lại
                                // thứ đã nằm ngay trên màn, đúng cái vừa gỡ đi
                                // hai lần hôm nay.
                                // 🔴 ĐO Ô NHẬP TRÊN ẢNH MÀN, TRƯỚC MỌI THỨ HUBA
                                // NỐI THÊM. Hà 2026-08-18, ảnh chụp tin `/shot`:
                                // *"ô chat có gợi ý tại sao lại không có nút
                                // bấm, sao cứ update lại mất vài thứ"*.
                                //
                                // Đo được: tin ấy đi ra với `text_links=0`,
                                // trong khi tin `/shot` của một phiên khác cùng
                                // phút có `text_links=2`. Khác nhau đúng một
                                // chỗ: phiên này màn chỉ là đầu ra lệnh nên huba
                                // NỐI THÊM khối *"🗣 Lời cuối nó nói"* ở dưới —
                                // và `prompt_line_text` đọc "khối đóng khung
                                // CUỐI CÙNG", tức từ lúc ấy nó đọc phần văn
                                // xuôi huba tự viết chứ không đọc ô nhập nữa.
                                //
                                // Cùng một bài học đã ghi trong `session_layout`
                                // (*"phải đo TRƯỚC khi trộn"*) — nhưng chỗ trộn
                                // thứ hai nằm ở ĐÂY, nên luật ấy phải đi theo.
                                // `SessionData.box_text` vốn đã khai sẵn cho
                                // đúng việc này mà chưa ai nối vào.
                                shot_box = prompt_line_text(&crate::telegram::strip_markdown(&out));
                                // 🔴 HỎI NHẬT KÝ TRƯỚC, RỒI ĐỐI CHIẾU VỚI MÀN —
                                // không hỏi màn xem nó "có lời nào không".
                                // Phép cũ (`out.contains('⏺')`) đọc một dấu
                                // hiệu nằm ở ĐẦU lượt, tức đúng thứ cuộn đi
                                // trước nhất; xem `sessions::said_shown_on_screen`
                                // cho ảnh chụp 20/08 nơi nó khai ngược sự thật.
                                //
                                // Hộp chọn thì vẫn KHÔNG bù: lúc ấy việc của
                                // tin là để bấm, mà lời cuối trong nhật ký
                                // thường là câu TRƯỚC câu hỏi (câu hỏi đi bằng
                                // `tool_use`, không phải văn xuôi) — bù vào là
                                // đẩy thứ đáng bấm xuống dưới một đoạn cũ.
                                if shot_choices.is_empty() {
                                    if let Some(said) = said.as_deref() {
                                        // Hỏi LẠI trên chuỗi CUỐI CÙNG, tức bản
                                        // đã nới nếu có nới: chỉ bù khi cuộn hết
                                        // cỡ rồi màn vẫn thiếu. Không có bước
                                        // này thì mọi lượt nới thành công vẫn
                                        // kèm một bản chép — đúng cái đã gỡ đi
                                        // hai lần trong ngày 17/08.
                                        // 🔴 GHÉP NỐI, KHÔNG CHÈN XUỐNG CUỐI —
                                        // Hà 2026-08-25: *"tại sao mục này lại
                                        // không tự viết thuật toán xử lý để ghép
                                        // nối luôn với màn hình chính lại cứ chèn
                                        // thêm xuống cuối tin, nên nhiều thông
                                        // tin bị trùng nhau rất dài"*.
                                        //
                                        // Hai chỗ đổi, và cái thứ hai mới là
                                        // cái anh kêu:
                                        // ① CHỈ phần màn thiếu
                                        //   (`said_missing_head`), không nguyên
                                        //   văn cả lượt — màn cuộn mất từ trên
                                        //   xuống nên cái đuôi vẫn đang nằm đó,
                                        //   và bản cũ chép đè lên chính nó;
                                        // ② đặt lên TRƯỚC màn, không sau. Đây là
                                        //   thứ tự thời gian thật (phiên nói
                                        //   trước, lệnh in ra sau), nên đọc một
                                        //   mạch từ trên xuống là đúng mạch. Đặt
                                        //   sau thì người đọc phải nhảy xuống
                                        //   cuối rồi ngược lên.
                                        //
                                        // Và nó AN TOÀN hơn cho phép đo ô nhập:
                                        // con bug 18/08 (`prompt_line_text` đọc
                                        // "khối đóng khung CUỐI CÙNG" rồi bám vào
                                        // chữ huba tự viết) sinh ra vì khối này
                                        // nối vào ĐUÔI. Nối lên đầu thì ô nhập
                                        // vẫn là khối cuối, đúng như màn gốc.
                                        // `shot_box` đo trước vẫn giữ nguyên —
                                        // hai hàng rào tốt hơn một.
                                        //
                                        // KHÔNG cắt 600 như bản 20/08: đường gửi
                                        // đã cắt theo dòng cho vừa trần Telegram
                                        // (`split_for_telegram`), nên cắt thêm ở
                                        // đây chỉ để mất chữ — mà mất chữ đúng
                                        // lúc màn đã mất chữ là mất hai lần.
                                        if let Some(head) =
                                            crate::sessions::said_missing_head(said, &out)
                                        {
                                            let whole = head.trim() == said.trim();
                                            let nhan = if whole {
                                                "🗣 Màn không mang lời nào của lượt này. Nguyên văn lấy từ nhật ký:"
                                            } else {
                                                "🗣 Phần đầu lượt này đã cuộn khỏi màn — nối lại từ nhật ký:"
                                            };
                                            out = format!("{nhan}\n{head}\n\n{out}");
                                        }
                                    }
                                }
                                // Câu ĐANG HIỆN đã có ☑ ngay tại dòng của nó,
                                // nên khu chữ ở cuối chỉ còn việc với các câu
                                // SAU — và với dòng `/send_…`.
                                if let Some(a) = shot_asking.as_ref() {
                                    out.push_str(&ask_command_lines(&shot_sid, a, true));
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
                            // 🔴 BẤM SỐ THÌ PHẢI CÓ HỘP CHỌN ĐỂ MÀ BẤM — Hà
                            // 2026-08-17: *"Bấm nhiều lần 1 lựa chọn"*, kèm ảnh
                            // bốn cú bấm `1` vào `[onghut]`: hai cú đầu huba đáp
                            // *"✓ đã bấm '1'"*, hai cú sau *"màn KHÔNG đổi"*.
                            //
                            // Nhật ký 12:06–12:07 cho thấy phím tới đúng cửa sổ
                            // (2214), mà `/shot` cùng lúc ra **mã nguồn**: bảng
                            // trong tin là bảng CŨ, phiên đã đi tiếp từ lâu.
                            //
                            // Lần ấy KHÔNG có lượt rác nào được đẻ ra — kiểm
                            // transcript của phiên: lượt `user` cuối cùng là
                            // 12:00:06, còn bốn cú bấm rơi vào 12:06–12:07. Nói
                            // đúng chừng ấy, đừng kể một hậu quả chưa xảy ra.
                            //
                            // Nhưng RỦI RO thì có thật và đó là lý do có cổng
                            // này: `do script` luôn kèm một CR, nên một con số
                            // gửi vào màn không có hộp chọn có thể đi làm một
                            // lượt chat trong phiên của chủ máy — và một cú bấm
                            // đẻ ra lượt rác thì tệ hơn hẳn cú bấm không làm gì.
                            //
                            // Nên: nhìn màn TRƯỚC, và chỉ gửi con số khi màn
                            // đang thật sự có hộp chọn. Không đọc được màn thì
                            // cũng KHÔNG gửi — cùng luật với `arrow_verdict`,
                            // vì "không đọc được" không phải "không có hộp".
                            let digit = is_key
                                && typed.trim().len() == 1
                                && typed.trim().chars().all(|c| c.is_ascii_digit());
                            let refusal = if digit {
                                match crate::keys::look(&s.tty, 24) {
                                    crate::keys::Look::Saw { body, .. }
                                        if !crate::keys::parse_choices(&body).is_empty() =>
                                    {
                                        None
                                    }
                                    crate::keys::Look::Saw { .. } => {
                                        logging::info(
                                            "keys_choice_refused",
                                            json!({ "session": s.session_id, "key": typed.trim(),
                                                    "why": "màn không có hộp chọn — bảng trong tin đã cũ" }),
                                        );
                                        Some(format!(
                                            "⚠ {} lúc này KHÔNG có hộp chọn nào trên màn, nên tôi không gửi số \
                                             '{}' — bảng trong tin là bảng cũ, phiên đã đi tiếp. Gửi số vào đó \
                                             là đẻ ra một lượt chat rác trong phiên. /shot để nhìn màn hiện tại.",
                                            crate::sessions::shown(&s),
                                            typed.trim()
                                        ))
                                    }
                                    crate::keys::Look::Blind { why } => {
                                        logging::warn(
                                            "keys_choice_refused",
                                            json!({ "session": s.session_id, "key": typed.trim(),
                                                    "why": "blind", "detail": why }),
                                        );
                                        Some(format!(
                                            "⚠ Lúc này tôi KHÔNG đọc được màn của {} ({}), nên KHÔNG gửi số '{}'. \
                                             Không đọc được không có nghĩa là đang có hộp chọn — mà nếu không có \
                                             thì con số ấy đi làm một lượt chat trong phiên.",
                                            crate::sessions::shown(&s),
                                            why,
                                            typed.trim()
                                        ))
                                    }
                                }
                            } else if is_key && arrow {
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
                            // xong chưa thấy có tác dụng"*. Kiểm log thì huba ĐÃ
                            // gửi phím thật (`keys_typed kind=key`), và đọc màn
                            // ngay sau đó thì ô nhập TRỐNG, phiên đứng nguyên ở
                            // lượt cũ — tức Enter rơi vào một ô rỗng.
                            //
                            // Cái nằm trong ô lúc ấy là GỢI Ý MỜ của TUI (chữ
                            // xám bày lại từ lịch sử), không phải chữ ai gõ. Mà
                            // `contents of tab` bỏ sạch MÀU, nên huba không phân
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
                                // `clear` không phải một PHÍM — nó là "xoá đúng
                                // bấy nhiêu ký tự đang có", nên nó phải đọc màn
                                // trước. Xem `keys::clear_box`.
                                if typed.trim() == "clear" {
                                    crate::keys::clear_box(w).map(|sạch| {
                                        if !sạch {
                                            logging::warn(
                                                "keys_clear_incomplete",
                                                json!({ "session": s.session_id,
                                                        "effect": "ô nhập vẫn còn chữ sau khi xoá — không khai là đã sạch" }),
                                            );
                                        }
                                    })
                                } else if typed.trim() == "enter" {
                                    // 🔴 ENTER TRẦN KHÔNG GỬI ĐƯỢC HỘP CHỌN
                                    // NHIỀU — Hà 2026-08-17, sau khi bấm đủ bốn
                                    // lựa chọn rồi `/send_…`: *"Ko qua nổi màn
                                    // này"*.
                                    //
                                    // Trong hộp ấy, Enter tác động lên DÒNG CON
                                    // TRỎ ĐANG ĐỨNG (bật/tắt đúng ô vừa chọn),
                                    // còn thứ gửi bảng đi là một dòng riêng tên
                                    // `Submit` — không mang số nên bấm số không
                                    // tới được. Nên `/send` phải ĐI TỚI đó rồi
                                    // mới Enter; xem `keys::submit_keys`, và cả
                                    // ba cửa của nó đo trên chính màn ấy.
                                    //
                                    // 🔴 Và `Submit` KHÔNG ở phía DƯỚI: đo
                                    // 17/08, đi `↓` tới nó thì con trỏ quấn về
                                    // mục 1 rồi Enter lật mất một ô. Nó là tab
                                    // bên PHẢI trên thanh `← ☒ … ✔ Submit →`,
                                    // nên phải biết đang đứng ở CÂU nào —
                                    // đọc từ màn, không đếm phím.
                                    let at_q = {
                                        let asking = s.asking.clone().unwrap_or_default();
                                        let questions: Vec<String> =
                                            std::iter::once(asking.question.clone())
                                                .chain(
                                                    asking.rest.iter().map(|r| r.question.clone()),
                                                )
                                                .collect();
                                        before
                                            .as_deref()
                                            .and_then(|scr| crate::keys::cursor_on(scr, &questions))
                                            .unwrap_or(0)
                                    };
                                    match before
                                        .as_deref()
                                        .and_then(|scr| crate::keys::submit_plan(scr, at_q))
                                    {
                                        Some(plan) => {
                                            logging::info(
                                                "keys_submit_plan",
                                                json!({ "session": s.session_id, "plan": plan }),
                                            );
                                            crate::keys::press_writes(w, &plan)
                                        }
                                        None => crate::keys::press(w, "enter"),
                                    }
                                } else {
                                    // 🔴 SỐ KHÔNG ĂN TRONG HỘP CHỌN NHIỀU — Hà
                                    // 2026-08-17, ảnh `/shot` sau khi bấm: *"Bấm
                                    // xong xem lại vẫn đứng im"*. Log ghi *"đã
                                    // bấm '1'"*, mà không một ô `[ ]` nào đổi.
                                    //
                                    // Hộp chọn MỘT nhận phím số (đường này chạy
                                    // từ 13/08); hộp CHỌN NHIỀU thì không —
                                    // dòng chân của nó chỉ khai `Enter to select
                                    // · ↑/↓ to navigate`. Nhận nó bằng ô `[ ]`
                                    // trên nhãn (Hà: *"chèn [] là tương ứng chọn
                                    // được nhiều à"*) rồi ĐI TỚI mục ấy và Enter.
                                    let as_item = typed
                                        .trim()
                                        .parse::<usize>()
                                        .ok()
                                        .zip(before.as_deref())
                                        .and_then(|(n, scr)| crate::keys::checkbox_plan(scr, n));
                                    match as_item {
                                        Some(plan) => {
                                            logging::info(
                                                "keys_checkbox_plan",
                                                json!({ "session": s.session_id, "n": typed.trim(),
                                                        "plan": plan }),
                                            );
                                            crate::keys::press_writes(w, &plan)
                                        }
                                        None => crate::keys::press(w, typed.trim()),
                                    }
                                }
                            } else {
                                crate::keys::type_into(w, &typed)
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
                                    // 🔴 THÔI ĐẶT CƯỢC TRƯỚC BAO NHIÊU CÚ ENTER.
                                    //
                                    // Hà 2026-08-15, nhìn cửa sổ shell sau một
                                    // lệnh: *"sao có tới 5 cái enter?"*. Đếm ra
                                    // đúng phép tính chứ không phải ngẫu nhiên:
                                    // `press(enter)` gửi một ký tự **và `do
                                    // script` tự chèn thêm một dấu nữa** (luật
                                    // 13: cái đó không tắt được). Nên MỖI cú
                                    // Enter đáng hai. Một lệnh + hai cú cố định
                                    // = 1+2+2 = năm dấu nhắc.
                                    //
                                    // (Ký tự ấy là CR từ 2026-08-16; trước đó
                                    // là LF, và LF chính là thứ chèn dòng trống
                                    // vào ô nhập của `claude` thay vì gửi —
                                    // xem `keys::key_payload`.)
                                    //
                                    // Vòng `[400ms, 1000ms]` cũ bấm hai lần VÔ
                                    // ĐIỀU KIỆN, vì hồi 12-08 chưa ai đọc lại
                                    // màn nên phải bấm thừa cho chắc. Nay có
                                    // phép đo (`Landed::InBox`), nên bấm THEO
                                    // NHU CẦU: gõ xong, nhìn, còn chữ thì mới
                                    // bấm. Ca thường của một shell là **không
                                    // cú nào** — dấu xuống dòng của `do script`
                                    // đã đủ gửi.
                                    //
                                    // Đây cũng là cách duy nhất đúng cho CẢ HAI
                                    // hạng cửa sổ: shell nuốt một dòng trống
                                    // thành một dấu nhắc (nhìn thấy được), còn
                                    // TUI của `claude` thì bỏ qua nó. Một hằng
                                    // số không thể đúng cho cả hai.
                                    let mut sent_enter = false;
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
                                    // Soi màn để BÁO CÁO — và, nếu chữ còn nằm
                                    // trong ô, để BẤM LẠI.
                                    //
                                    // 🔴 Hà 2026-08-15, ảnh chụp ô nhập mang hai
                                    // tin dính liền: *"sao nội dung lại bị lặp
                                    // thế này"*. Hai Enter cố định (400ms +
                                    // 1000ms) là một CON SỐ ĐẶT CƯỢC, và chú
                                    // thích ngay trên đã nói đúng từ 12-08:
                                    // *"400ms là đủ ở máy rảnh và KHÔNG đủ ở máy
                                    // đang swap — một hằng số thời gian bao giờ
                                    // cũng sai với một cái máy đang đổi tốc độ …
                                    // đừng đặt cược vào một con số: bấm, NHÌN,
                                    // còn chữ thì bấm nữa"*. Chú thích tả đúng
                                    // thiết kế; **mã thì chưa bao giờ làm phần
                                    // "NHÌN"**. Nay làm.
                                    //
                                    // Không mâu thuẫn với *"việc gì phải soi"*
                                    // của Hà: anh cấm soi TRƯỚC KHI gõ (một cửa
                                    // chặn), còn đây là đọc lại SAU khi gõ để
                                    // nói cho đúng — và Enter vào ô TRỐNG thì
                                    // `claude` không làm gì, nên bấm thừa là an
                                    // toàn theo đúng nghĩa idempotent.
                                    let mut view = None;
                                    let mut extra_enter = 0u32;
                                    for round in 0..3 {
                                        for _ in 0..6 {
                                            std::thread::sleep(
                                                std::time::Duration::from_millis(500),
                                            );
                                            view = seen(&s.tty);
                                            if view.is_some() {
                                                break;
                                            }
                                        }
                                        if is_key {
                                            break;
                                        }
                                        let stuck = view.as_ref().is_some_and(|(body, _)| {
                                            crate::keys::still_in_box(body, &typed)
                                        });
                                        if !stuck || round == 2 {
                                            break;
                                        }
                                        // Có hộp chọn thì DỪNG: ở đó Enter là
                                        // CHỐT một lựa chọn, không phải gửi một
                                        // câu — thứ không lùi lại được.
                                        if view.as_ref().is_some_and(|(_, ch)| !ch.is_empty()) {
                                            logging::warn(
                                                "keys_enter_retry_stopped",
                                                json!({ "session": s.session_id,
                                                        "why": "màn có hộp chọn — Enter ở đó là CHỐT" }),
                                            );
                                            break;
                                        }
                                        if let Err(e) = crate::keys::press(w, "enter") {
                                            logging::warn(
                                                "keys_enter_failed",
                                                json!({ "session": s.session_id,
                                                        "round": round, "err": e.to_string() }),
                                            );
                                            break;
                                        }
                                        extra_enter += 1;
                                        sent_enter = true;
                                        logging::info(
                                            "keys_enter_again",
                                            json!({ "session": s.session_id, "round": round,
                                                    "why": "chữ vẫn nằm trong ô nhập" }),
                                        );
                                        view = None;
                                    }
                                    if extra_enter > 0 {
                                        logging::info(
                                            "keys_enter_extra",
                                            json!({ "session": s.session_id, "count": extra_enter }),
                                        );
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
                                    // dung là RUỘT của huba (đã gõ bao nhiêu ký
                                    // tự, có phải bấm Enter rời không, màn nói
                                    // gì) — thứ chỉ có ích khi đi tìm lỗi, tức
                                    // đúng chỗ của một dòng log.
                                    let what = view
                                        .as_ref()
                                        .map(|(body, _)| crate::keys::landed(body, &typed));
                                    let name = crate::sessions::shown(&s);
                                    if is_key {
                                        let unchanged = match (&before, &view) {
                                            (Some(a), Some((b, _))) => a == b,
                                            _ => false,
                                        };
                                        // 🔴 MỘT TRẠNG THÁI ĐÃ GỌI ĐƯỢC TÊN THÌ
                                        // PHẢI CÓ HÀNH ĐỘNG ĐI KÈM.
                                        //
                                        // Hà 2026-08-16, ảnh chụp 11:54: *"Bấm
                                        // nút enter không nhận, chỗ ô chat có
                                        // gợi ý, phải bấm nút right trước thì nó
                                        // mới điền text theo gợi ý"*. Câu trả
                                        // lời cũ ngay dưới đây chẩn ĐÚNG bệnh
                                        // rồi trả việc lại cho chủ máy ("gõ
                                        // thẳng nó ở đây") — tức cây cầu dừng
                                        // đúng chỗ nó vừa chỉ ra vấn đề.
                                        //
                                        // Thứ tự Enter-TRƯỚC-rồi-`→` là bằng
                                        // chứng chứ không phải thói quen: xem
                                        // `keys::ghost_verdict`.
                                        let mut ghost = false;
                                        // NHẬN được gợi ý vào ô ≠ ĐÃ GỬI. Hai
                                        // kết cục khác nhau thì hai biến, vì
                                        // câu báo cho chủ máy khác hẳn nhau —
                                        // gộp lại là chỗ đẻ ra câu "✓ đã gửi"
                                        // cho một chữ còn nằm nguyên trong ô.
                                        let mut ghost_accepted = false;
                                        // Đã THẬT SỰ bấm `→` hay chưa — câu báo
                                        // phải nói đúng cái đã làm, không nói
                                        // cái lẽ ra làm.
                                        let mut tried_right = false;
                                        if let Some(v) = crate::keys::ghost_verdict(
                                            &typed,
                                            unchanged,
                                            &crate::keys::look(&s.tty, 24),
                                        ) {
                                            match v {
                                                crate::keys::Arrow::Send => {
                                                    match crate::keys::press(w, "right") {
                                                        Ok(()) => {
                                                            tried_right = true;
                                                            // 🔴 `→` NHẬN gợi ý, và
                                                            // CHỈ có thế. Bản cũ
                                                            // viết ở đây rằng cú CR
                                                            // đi kèm `do script`
                                                            // gửi luôn — sai, và
                                                            // luật 13 của dự án đã
                                                            // ghi đúng lý do từ
                                                            // 12/08: chữ và dấu
                                                            // xuống dòng vào TUI
                                                            // trong CÙNG một lượt
                                                            // ghi thì `claude` đọc
                                                            // cả cụm như một cú
                                                            // DÁN, dấu xuống dòng
                                                            // rơi vào nội dung chứ
                                                            // không kết thúc nó.
                                                            //
                                                            // Hà 2026-08-16: *"ô
                                                            // nhập đang là gợi ý
                                                            // mờ, bấm nút enter nó
                                                            // hiện thành text xong
                                                            // phải bấm lại nút
                                                            // enter lần nữa nó mới
                                                            // gửi vào hàng đợi"* —
                                                            // đúng từng nhịp. Và
                                                            // huba thì báo *"✓ đã
                                                            // gửi"* ngay ở nhịp
                                                            // đầu, vì nó chấm bằng
                                                            // "màn có đổi không"
                                                            // (đổi thật: chữ mờ vừa
                                                            // thành chữ tỏ) chứ
                                                            // không bằng "câu ấy đi
                                                            // chưa".
                                                            let mut after = None;
                                                            for _ in 0..6 {
                                                                std::thread::sleep(
                                                                    std::time::Duration::from_millis(500),
                                                                );
                                                                after = seen(&s.tty);
                                                                if after.is_some() {
                                                                    break;
                                                                }
                                                            }
                                                            // Gợi ý đã được NHẬN
                                                            // vào ô hay chưa — đây
                                                            // mới là thứ `→` hứa.
                                                            let accepted = match (&before, &after) {
                                                                (Some(a), Some((b, _))) => a != b,
                                                                _ => false,
                                                            };
                                                            ghost_accepted = accepted;
                                                            // …rồi ENTER RỜI, đúng
                                                            // thuốc đã có sẵn cho
                                                            // ca này (`still_in_box`
                                                            // + `press enter`), và
                                                            // chỉ bấm khi ô nhập
                                                            // THẬT SỰ còn giữ chữ.
                                                            let in_box = after
                                                                .as_ref()
                                                                .and_then(|(b, _)| {
                                                                    crate::keys::input_box_text(b)
                                                                })
                                                                .unwrap_or_default();
                                                            if accepted && !in_box.trim().is_empty()
                                                            {
                                                                match crate::keys::press(w, "enter")
                                                                {
                                                                    Ok(()) => {
                                                                        std::thread::sleep(
                                                                            std::time::Duration::from_millis(900),
                                                                        );
                                                                        let done = seen(&s.tty);
                                                                        // Khai theo Ô
                                                                        // NHẬP, không
                                                                        // theo "màn có
                                                                        // đổi": gửi
                                                                        // xong thì
                                                                        // `claude` in
                                                                        // lại câu ấy ở
                                                                        // phần hội
                                                                        // thoại, nên
                                                                        // "màn có chữ"
                                                                        // không nói
                                                                        // được gì.
                                                                        ghost = done
                                                                            .as_ref()
                                                                            .map(|(b, _)| {
                                                                                !crate::keys::still_in_box(b, &in_box)
                                                                            })
                                                                            .unwrap_or(false);
                                                                        logging::info(
                                                                            "keys_ghost_enter_sent",
                                                                            json!({ "session": s.session_id,
                                                                                    "left_the_box": ghost,
                                                                                    "saw_after": done.is_some() }),
                                                                        );
                                                                    }
                                                                    Err(e) => logging::warn(
                                                                        "keys_ghost_enter_failed",
                                                                        json!({ "session": s.session_id,
                                                                                "err": e.to_string() }),
                                                                    ),
                                                                }
                                                            } else {
                                                                // Ô đã trống ngay sau
                                                                // `→` ⟹ TUI ấy gửi
                                                                // luôn. Không bắn
                                                                // thêm Enter: một
                                                                // Enter thừa không
                                                                // lùi lại được.
                                                                ghost = accepted;
                                                            }
                                                            logging::info(
                                                                "keys_ghost_accepted",
                                                                json!({ "session": s.session_id,
                                                                        "accepted": accepted,
                                                                        "worked": ghost,
                                                                        "saw_after": after.is_some() }),
                                                            );
                                                        }
                                                        Err(e) => logging::warn(
                                                            "keys_ghost_right_failed",
                                                            json!({ "session": s.session_id,
                                                                    "err": e.to_string() }),
                                                        ),
                                                    }
                                                }
                                                crate::keys::Arrow::RefuseDialog => logging::info(
                                                    "keys_ghost_refused",
                                                    json!({ "session": s.session_id,
                                                            "why": "màn có hộp chọn — → ở đó là CHỐT" }),
                                                ),
                                                crate::keys::Arrow::RefuseBlind(why) => {
                                                    logging::warn(
                                                        "keys_ghost_refused",
                                                        json!({ "session": s.session_id,
                                                                "why": "blind", "detail": why }),
                                                    )
                                                }
                                            }
                                        }
                                        if ghost {
                                            // 🔴 XONG THÌ CHỈ NÓI XONG — Hà
                                            // 2026-08-16: *"Xác nhận xử xong là
                                            // được giải thích dài dòng làm gì"*.
                                            //
                                            // Bản cũ kể lại cả cách xoay xở
                                            // ("ô đang là gợi ý mờ nên Enter
                                            // trơn không ăn, tôi bấm → rồi một
                                            // Enter rời"). Đúng sự thật, sai chỗ
                                            // đứng: người đọc hỏi *câu của tôi
                                            // đi chưa*, không hỏi huba vượt qua
                                            // cái gì. Cùng lỗi đã sửa 12/08 cho
                                            // `/type` (*"chỉ cần báo đã gõ được
                                            // thôi cần gì báo đã gửi enter
                                            // rời"*) và 16/08 cho `/runin`
                                            // (*"Tại sao để báo trần 120s làm
                                            // gì"*) — ba lần cùng một hình dạng.
                                            //
                                            // Đường đi vẫn ghi đủ trong log
                                            // (`keys_ghost_accepted`), chỗ dành
                                            // cho người đi gỡ lỗi. Và câu này
                                            // nay đúng dạng `ack_as_emoji` nhận
                                            // ra, nên nó về thành một dấu thả
                                            // lên tin gốc, không chiếm dòng nào.
                                            format!("✓ đã gửi · {name}")
                                        } else if ghost_accepted {
                                            // 🔴 Nhịp giữa, và nó PHẢI có tên
                                            // riêng: gợi ý đã vào ô thành chữ
                                            // tỏ, nhưng cú Enter rời không đưa
                                            // được nó đi. Khai "đã gửi" ở đây
                                            // là đúng cái Hà bắt được; khai
                                            // "màn KHÔNG đổi" cũng sai, vì màn
                                            // đổi thật. Nói đúng nhịp đang
                                            // đứng, kèm việc chủ máy làm tiếp.
                                            format!(
                                                "⚠ CHƯA gửi · {name}\n\
                                                 Gợi ý mờ đã được nhận vào ô nhập (chữ nay là chữ thật), \
                                                 nhưng cú Enter rời chưa đưa nó đi. Bấm ⏎ lần nữa."
                                            )
                                        } else if unchanged {
                                            logging::info(
                                                "keys_press_no_effect",
                                                json!({ "session": s.session_id,
                                                        "key": typed.trim(),
                                                        "why": "màn không đổi sau khi bấm" }),
                                            );
                                            format!(
                                                "⚠ đã bấm '{}'{} nhưng màn KHÔNG đổi · {name}\n                                                 Chữ trong ô nhập nhiều khả năng là GỢI Ý MỜ của TUI \
                                                 (huba đọc màn không thấy màu nên không phân biệt được \
                                                 với chữ đã gõ). Muốn gửi câu ấy thì gõ thẳng nó ở đây.",
                                                typed.trim(),
                                                if tried_right { " rồi bấm → để nhận gợi ý" } else { "" }
                                            )
                                        } else {
                                            // 🔴 NÓI KẾT QUẢ, KHÔNG NÓI HÀNH
                                            // ĐỘNG — Hà 2026-08-17: *"Bấm chọn
                                            // hết nhưng shot lại thiếu… Phản hồi
                                            // về là bấm rồi mà"*.
                                            //
                                            // `✓ đã bấm '3'` chỉ khai rằng phím
                                            // rời khỏi huba. Với hộp CHỌN NHIỀU,
                                            // thứ người ở xa cần biết là mấy ô
                                            // đang tick — và con số ấy bắt được
                                            // cả ca phím tới nơi nhưng rơi vào
                                            // mục khác, thứ mà "đã bấm" che mất.
                                            // 🔴 ĐỌC MÀN ĐÃ CHỜ, VÀ CHỜ TỚI KHI
                                            // NÓ ĐỔI — Hà 2026-08-17, ảnh năm
                                            // cú bấm liền: `2/5 · 2/5 · 2/5 ·
                                            // 0/5 · 4/5`. Con số nhảy vì bản
                                            // đầu của tôi đọc màn NGAY sau khi
                                            // gửi phím, tức đọc trước lúc TUI
                                            // vẽ xong — đúng cái bẫy `keys_
                                            // screen_waited` sinh ra để tránh,
                                            // mà tôi lại đi đọc một lượt riêng
                                            // bên cạnh nó.
                                            let mut ticked = view
                                                .as_ref()
                                                .map(|(b, _)| crate::keys::ticked(b))
                                                .unwrap_or((0, 0));
                                            let was = before
                                                .as_deref()
                                                .map(crate::keys::ticked)
                                                .unwrap_or((0, 0));
                                            // Chưa thấy đổi thì đọc lại vài
                                            // nhịp: một cú toggle phải làm con
                                            // số nhúc nhích, và nếu nó không
                                            // nhúc nhích thật thì ba nhịp nữa
                                            // cũng vậy — không có vòng lặp vô
                                            // hạn nào ở đây.
                                            for _ in 0..3 {
                                                if ticked != was || ticked.1 == 0 {
                                                    break;
                                                }
                                                std::thread::sleep(
                                                    std::time::Duration::from_millis(400),
                                                );
                                                if let Some((b, _)) =
                                                    crate::keys::screen_of(&s.tty, 40)
                                                {
                                                    ticked = crate::keys::ticked(&b);
                                                }
                                            }
                                            let (on, all) = ticked;
                                            if all > 0 {
                                                // 🔴 TRẢ VỀ CẢ BẢNG, BẤM ĐƯỢC
                                                // NGAY — Hà 2026-08-17: *"Phản
                                                // hồi nên thêm ô đã tích hay
                                                // chưa và cho phép bấm được
                                                // luôn"*.
                                                //
                                                // Một con số `4/5` nói được
                                                // "có gì đó đã đổi" nhưng
                                                // không nói ô NÀO, nên sau mỗi
                                                // cú bấm vẫn phải `/shot` để
                                                // nhìn — tức cây cầu bắt người
                                                // ta đi thêm một nhịp cho thứ
                                                // huba vừa đọc xong.
                                                //
                                                // Chữ dựng từ MÀN ĐÃ CHỜ, và
                                                // `shot_choices`/`shot_submit`
                                                // được điền ngay tại đây nên
                                                // cửa định dạng gắn ☑ vào từng
                                                // dòng và ✅ vào dòng Submit —
                                                // cùng bộ máy với `/shot`,
                                                // không đẻ nhánh nào.
                                                let screen = view
                                                    .as_ref()
                                                    .map(|(b, _)| b.clone())
                                                    .unwrap_or_default();
                                                let opts = crate::keys::parse_choices(&screen);
                                                // Cùng một câu hỏi, cùng một
                                                // chỗ trả lời — xem
                                                // `multi_question_screen`.
                                                let table = multi_question_screen(
                                                    shot_asking
                                                        .as_ref()
                                                        .is_some_and(|a| !a.rest.is_empty()),
                                                    &screen,
                                                );
                                                shot_choices = opts
                                                    .iter()
                                                    .map(|(n, l)| {
                                                        let code = if table {
                                                            format!("1.{n}")
                                                        } else {
                                                            n.to_string()
                                                        };
                                                        (code, l.clone())
                                                    })
                                                    .collect();
                                                shot_submit = crate::keys::has_submit(&screen);
                                                let lines: Vec<String> = opts
                                                    .iter()
                                                    .map(|(n, l)| format!("{n}. {l}"))
                                                    .collect();
                                                let submit_line =
                                                    if shot_submit { "\nSubmit" } else { "" };
                                                // 🔴 ĐẾM XEM MẤY Ô ĐỔI, KHÔNG
                                                // CHỈ ĐẾM MẤY Ô ĐANG BẬT — Hà
                                                // 2026-08-17: *"Bấm cái nọ mất
                                                // cái kia ảo lắm"*.
                                                //
                                                // Hôm ấy huba bấm mục 1 và làm
                                                // mất dấu mục 2, rồi vẫn ack
                                                // `3/5` xanh rờn: tổng số ô bật
                                                // KHÔNG đổi khi một ô lật lên
                                                // còn một ô lật xuống, nên phép
                                                // đo cũ mù đúng cái ca này. Một
                                                // cú bấm lành làm đổi ĐÚNG MỘT
                                                // ô — nên đổi khác thế là nói ra,
                                                // ngay trong tin, chứ không đợi
                                                // chủ máy tự soi ảnh.
                                                let flipped = before
                                                    .as_deref()
                                                    .map(|b| {
                                                        crate::keys::ticks_changed(b, &screen)
                                                    })
                                                    .unwrap_or_default();
                                                let odd = if flipped.is_empty() {
                                                    // Không đổi gì cũng là một
                                                    // kết quả, và là kết quả
                                                    // hay bị nuốt nhất: bảng in
                                                    // ra y như cũ thì đọc như
                                                    // "xong rồi".
                                                    logging::warn(
                                                        "keys_no_toggle",
                                                        json!({ "session": s.session_id,
                                                                "typed": typed.trim(),
                                                                "why": "bấm xong mà không ô nào đổi dấu" }),
                                                    );
                                                    if typed.trim() == "enter" {
                                                        "\n⚠ Bảng VẪN mở — chưa chốt được. \
                                                         Bấm ✅ Submit lần nữa, hoặc /shot để nhìn."
                                                            .to_string()
                                                    } else {
                                                        "\n⚠ Không ô nào đổi dấu — cú bấm ấy \
                                                         không tới được mục nào."
                                                            .to_string()
                                                    }
                                                } else if flipped.len() > 1 {
                                                    logging::warn(
                                                        "keys_multi_toggle",
                                                        json!({ "session": s.session_id,
                                                                "flipped": flipped,
                                                                "why": "một cú bấm mà nhiều ô đổi dấu — phím rơi sai chỗ" }),
                                                    );
                                                    format!(
                                                        "\n⚠ Một cú bấm mà {} ô đổi dấu (mục {}). \
                                                         Bảng trên là thứ màn ĐANG có — sửa lại bằng \
                                                         cách bấm chính những ô ấy.",
                                                        flipped.len(),
                                                        flipped
                                                            .iter()
                                                            .map(usize::to_string)
                                                            .collect::<Vec<_>>()
                                                            .join(", ")
                                                    )
                                                } else {
                                                    String::new()
                                                };
                                                format!(
                                                    "✓ {name} — {on}/{all} ô đang chọn\n{}{submit_line}{odd}",
                                                    lines.join("\n")
                                                )
                                            } else {
                                                // 🔴 "Đã bấm" chỉ khai rằng phím
                                                // rời khỏi huba. Với hộp chọn MỘT
                                                // thì KẾT QUẢ đo được là: bảng
                                                // còn hay đã đóng.
                                                //
                                                // Hà 2026-08-17 bấm `1` bốn lượt
                                                // vào một bảng đã cũ; hai lượt
                                                // đầu huba đáp `✓ đã bấm '1'` —
                                                // xanh, vì phép đo cũ chỉ hỏi
                                                // "màn có đổi gì không", mà một
                                                // phiên đang chạy thì màn luôn
                                                // đổi (đồng hồ, con quay).
                                                let n_before = before
                                                    .as_deref()
                                                    .map(|b| crate::keys::parse_choices(b).len())
                                                    .unwrap_or(0);
                                                let n_after = view
                                                    .as_ref()
                                                    .map(|(b, _)| {
                                                        crate::keys::parse_choices(b).len()
                                                    })
                                                    .unwrap_or(0);
                                                match (n_before, n_after) {
                                                    (b, 0) if b > 0 => format!(
                                                        "✓ đã chọn '{}' — bảng đã đóng · {name}",
                                                        typed.trim()
                                                    ),
                                                    (b, a) if b > 0 && a == b => format!(
                                                        "⚠ đã gửi '{}' mà bảng vẫn còn nguyên {b} lựa chọn · {name}\n\
                                                         Hộp này có thể không nhận phím số — /shot để nhìn.",
                                                        typed.trim()
                                                    ),
                                                    _ => format!(
                                                        "✓ đã bấm '{}' · {name}",
                                                        typed.trim()
                                                    ),
                                                }
                                            }
                                        }
                                    } else if what == Some(crate::keys::Landed::Queued) {
                                        format!("✓ vào hàng chờ · {name}")
                                    } else if what == Some(crate::keys::Landed::InBox) {
                                        // 🔴 NÓI THẬT. Đây là ca đã gửi cho Hà
                                        // một câu `✓ đã gửi` sai (08-15), và
                                        // cái giá không dừng ở một dòng chữ:
                                        // anh gõ tin tiếp theo, nó nối đuôi vào
                                        // đúng ô ấy, rồi cả hai đi làm MỘT tin.
                                        // Một lời khen sai đắt hơn một lời báo
                                        // lỗi, vì người đọc dựa vào nó để làm
                                        // việc tiếp theo.
                                        logging::warn(
                                            "keys_text_stuck_in_box",
                                            json!({ "session": s.session_id,
                                                    "why": "gõ xong, bấm Enter nhiều lượt, chữ vẫn nằm trong ô" }),
                                        );
                                        format!(
                                            "⚠ chữ VẪN NẰM trong ô nhập · {name}\n\
                                             Đã bấm Enter nhiều lượt mà nó chưa đi. ĐỪNG gõ tiếp — \
                                             tin sau sẽ nối vào đuôi tin này rồi đi làm một tin. \
                                             Bấm gửi: /key {} enter",
                                            s.session_id
                                        )
                                    } else if what.is_none() {
                                        // Không đọc được màn ⟹ KHÔNG BIẾT. Bản
                                        // cũ gộp ca này vào "✓ đã gửi", tức
                                        // khẳng định một chuyện chưa hề nhìn
                                        // thấy — cùng họ với `keys::look` trả
                                        // `Blind` thay vì `None`.
                                        format!(
                                            "✓ đã gõ · {name}\n\
                                             (chưa đọc lại được màn nên chưa xác nhận nó ĐÃ ĐI — /shot để nhìn)"
                                        )
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
                        // Phiên nền không có cửa sổ nào.
                        //
                        // 🔴 KHÔNG CÓ MÀN ≠ KHÔNG CÓ GÌ ĐỂ ĐƯA RA — Hà
                        // 2026-08-19, bấm 📷 *Xem màn* trên `Tiếp Onghut` rồi
                        // nhận đúng một câu về việc GÕ: *"rõ ràng đang chạy mà
                        // lại báo không có cửa sổ"*.
                        //
                        // Hai chỗ sai, và chỗ thứ hai mới là chỗ đau:
                        // ① câu trả lời nói về `gõ` trong khi anh hỏi `xem` —
                        //    một lời từ chối **lạc route**;
                        // ② nó từ chối trống không, trong khi ngồi trước máy thì
                        //    phiên nền ĐỌC ĐƯỢC: nhật ký `.jsonl` của nó nằm
                        //    ngay đó, và `/ask`·`/handover` (chạy trên fork của
                        //    nhật ký) vẫn với tới được. Đúng thứ CLAUDE.md gọi
                        //    là **cầu một chiều**.
                        //
                        // Đo trên chính hàng ấy (`watch:sessions` + `ps`):
                        // `tty '??' · host detached · kind bg · pid 35114 SỐNG`,
                        // tiến trình là `claude.exe --session-id … --fork-session`
                        // treo dưới một `--bg-pty-host` — tức phiên thật, chạy
                        // thật, chỉ không có cửa sổ Terminal nào.
                        Ok(None) => {
                            let ten = crate::sessions::shown(&s);
                            if matches!(cmd.kind, CommandKind::Shot | CommandKind::Photo) {
                                match crate::sessions::last_say_by_id(
                                    cfg,
                                    &s.session_id,
                                    crate::sessions::SAY_MAX,
                                )
                                .map(|t| t.trim().to_string())
                                .filter(|t| !t.is_empty())
                                {
                                    Some(said) => format!(
                                        "🗣 {ten} là phiên NỀN — không có cửa sổ Terminal nào để chụp. \
                                         Đây là lời cuối của nó, lấy từ nhật ký:\n\n{}",
                                        crate::exec::truncate(&said, 1200)
                                    ),
                                    // Nhật ký rỗng là một sự thật khác hẳn "không
                                    // đọc được nhật ký", và khác hẳn "phiên chết".
                                    // Nói đúng cái nào đang đúng.
                                    None => format!(
                                        "🗣 {ten} là phiên NỀN (host: {}) — không có màn để chụp, và nhật ký của nó \
                                         CHƯA có dòng nào (phiên vừa dựng, hoặc lượt đầu chưa xong).\n\
                                         /ask <câu hỏi> vẫn hỏi được nó — đường ấy chạy trên nhật ký, không cần cửa sổ.",
                                        s.host
                                    ),
                                }
                            } else {
                                format!(
                                    "⚠ {ten} là phiên NỀN (host: {}) — không có cửa sổ Terminal để gõ vào.\n\
                                     Đọc thì vẫn được: 📷 /shot lấy lời cuối từ nhật ký, /ask hỏi bên lề.",
                                    s.host
                                )
                            }
                        }
                        Err(e) => format!(
                            "⚠ không tìm được cửa sổ: {}",
                            crate::exec::truncate(&e.to_string(), 200)
                        ),
                    },
                };
                // `/shot` trên Telegram đi kèm NÚT cho từng lệnh của lượt cuối.
                // Đường đi vẫn là một: nút gõ dòng lệnh vào phiên qua `/type`,
                // tức cùng route, cùng sổ (xem `remember_quick`).
                let mut quick = Vec::new();
                // Giữ riêng ĐÚNG các dòng lệnh (không kèm "làm đi"), vì icon
                // trong chữ phải bám đúng dòng sinh ra nó — xem
                // `say_with_command_icons`.
                let mut cmd_lines: Vec<String> = Vec::new();
                // Tệp thấy trong chữ → neo cho liên kết 📎 (xem `file_anchors`).
                let mut shot_files: Vec<(String, usize)> = Vec::new();
                if matches!(cmd.kind, CommandKind::Shot | CommandKind::Tab) {
                    // 🔴 2026-08-15 — NGUỒN đổi: sổ, không phải màn.
                    //
                    // `ack` ở đây là `contents of selected tab`, tức chữ đã đi
                    // qua một cửa sổ: bẻ theo bề ngang, cắt bằng `…`. Đọc lệnh
                    // từ đó là đoán, và mọi cái nút đã chạy sai đều sinh ra ở
                    // đúng chỗ đoán ấy. Nay `ack` chỉ còn để NHÌN; lệnh lấy
                    // nguyên văn từ nhật ký của chính phiên này.
                    //
                    // Không có sổ ⟹ 0 nút, KHÔNG rơi về màn: rơi về là giữ lại
                    // đúng cái vừa gỡ, và giữ nó ở đúng lúc huba mù nhất.
                    // `commands_of` ghi một dòng log cho ca ấy.
                    // 🔴 TRẦN 4 → 12, và lý do nới là lý do CŨ ĐÃ HẾT HIỆU LỰC.
                    //
                    // Hà 2026-08-16, ảnh chụp `/shot` của `[dwork]` có BẢY dòng
                    // lệnh và đúng BA icon: *"phân tích mãi không được nội dung
                    // để bóc tách lệnh như này, thiếu quá nhiều"*. Trước đó là
                    // ảnh `[mailler]`: bốn dòng, ba icon. Cùng một thủ phạm —
                    // `commands_of` giữ phần CUỐI rồi cắt phần đầu, im lặng
                    // (nay có `cmds_truncated`).
                    //
                    // Trần nhỏ sinh ra hồi mỗi lệnh là một cái NÚT ở đáy tin:
                    // mười nút thì bàn phím Telegram thành một bức tường, và
                    // nhãn nút bị cắt ở 52 ký tự nên không đọc được mình sắp
                    // chạy gì. Từ khi icon nằm GIỮA CHỮ (`html_with_links`),
                    // mỗi lệnh mang icon của nó ngay trên dòng của nó — thêm
                    // một lệnh là thêm một icon ở một dòng vốn đã có, không
                    // chiếm thêm chỗ nào. Cái giá đã biến mất; cái trần thì ở
                    // lại. 12 là chặn-chuyện-vô-lý, không phải chắt lọc.
                    // Ba nguồn, một chỗ — xem `cmds_for_screen`. Nguồn nhật ký
                    // văn xuôi là thứ cứu được lệnh dài bị cửa sổ bẻ làm bốn.
                    let cmds = cmds_for_screen(cfg, &shot_sid, &ack);
                    let n_cmds = cmds.len();
                    cmd_lines = crate::sessions::lines_of(&cmds[..n_cmds]);
                    // 🪦 Nút "✅ làm đi" — GỠ 2026-08-16, Hà: *"1 xóa nút đó đi
                    // không cần nữa"*.
                    //
                    // Nó sinh ra từ chỗ huba ĐOÁN ý một màn: bảy cụm chữ
                    // (*"nói một tiếng"*, *"có muốn"*…) đọc thành "phiên đang
                    // mời", rồi dựng một nút gửi hai chữ đồng ý vào phiên. Một
                    // mệnh lệnh không lùi được, dựng trên một phép so chuỗi —
                    // và cùng tín hiệu ấy đã có lần dựng ra HAI nút một lúc
                    // (*"Sao có 2 nút làm đi"*, 14/08). Câu trả lời vẫn đi được
                    // bằng `/tell` hoặc `/type`, và khi tự gõ thì câu ấy nói
                    // đúng thứ chủ máy muốn nói, không phải hai chữ huba đoán.
                    let stored = remember_quick(db, &shot_sid, &cmds);
                    quick.extend(stored.into_iter().take(n_cmds));
                    // 🪦 Dòng "⛔ N dòng lệnh xoá/ghi đè — huba cố ý KHÔNG dựng
                    // nút" sống đúng nửa tiếng (16/08, 16:45→17:15). Hà đọc nó
                    // trên điện thoại: *"cái này thằng nào tạo ra, thằng nào
                    // chặn thế, tôi có yêu cầu vậy à"*. Không ai yêu cầu — tôi
                    // viết nó ra để giải thích một cái chặn mà chính huba tự
                    // dựng, thay vì hỏi xem cái chặn ấy có nên tồn tại không.
                    //
                    // Nay lệnh xoá CÓ nút (xem `keys::destructive`), nên không
                    // còn gì để giải thích.
                    //
                    // 📌 Và nó kịp gây một lỗi trong nửa tiếng ấy, đáng ghi:
                    // nối chữ của huba vào `ack` làm hỏng phép đo NGAY DƯỚI —
                    // `input_box_text(&ack)` đọc dòng vừa nối thành "chữ đang
                    // nằm trong ô nhập", nên hai nút ⏎/⌫ mọc ra ở đáy một tin
                    // có ô nhập rỗng (*"lại chèn 2 cái nút ở cuối, làm quanh
                    // làm quẩn mãi"*). Đúng cái bẫy đã ghi ngay tại
                    // `prompt_line_text`: `ack` không phải một MÀN, nó là cả
                    // TIN — và mấy dòng cuối của tin là chữ huba tự viết.
                    // …và TỆP thấy trên màn cũng phải mở được ngay tại đây.
                    //
                    // 🔴 Hà 2026-08-13: *"trong nội dung có khá nhiều file
                    // nhưng lại không mở được trên tele, mở nó lại ra trình
                    // duyệt"*. Thứ anh bấm là **link Telegram tự bắt** (nó thấy
                    // `DEPLOY.md` thì đoán là tên miền), không phải nút của
                    // huba — mà nút của huba lúc ấy chỉ gắn ở tin TỰ PHÁT, còn
                    // `/shot` thì không. Cùng một màn, hai luật khác nhau là
                    // thứ người dùng đọc thành "lúc được lúc không".
                    // Đường dẫn nằm TRONG một dòng lệnh thì thôi — dòng ấy đã có
                    // ▶️/🖥 của nó (xem `paths_not_in_commands`).
                    let seen_paths = paths_not_in_commands(
                        &ack,
                        &crate::keys::paths_on_screen(&crate::keys::body_before_box(&ack), 4),
                        &cmds_of_text(cfg, &want, &ack),
                    );
                    quick.extend(remember_files(db, cfg, &want, &seen_paths));
                    // …và ĐÍCH CHẠM NẰM NGAY TẠI TÊN TỆP trong chữ, không chỉ ở
                    // đáy tin (Hà 2026-08-16: *"chưa chèn link tải file xuất
                    // hiện trong nội dung phiên gửi lên tele"*). Cùng một lần
                    // lọc với cái nút ở trên, nên chỉ số không lệch được.
                    shot_files = file_anchors(db, cfg, &want, &seen_paths);
                }
                // Ô nhập đang có sẵn chữ ⟹ một nút GỬI (Hà 2026-08-13: *"có gợi
                // ý nội dung chat cần có cách bấm nhanh để gửi nó"*). Đi đúng
                // route `/key <id> enter` đã có — nút chỉ là phím tắt của một
                // đường đi sẵn, không phải một nhánh xử lý mới.
                if matches!(cmd.kind, CommandKind::Shot | CommandKind::Tab) {
                    // 🔴 Hà 2026-08-14: *"Sao có 2 nút làm đi"*. Vì đúng hai
                    // khối cùng dựng nó: khối trên (`say:<n>`, cùng kho với
                    // lệnh) và khối này (`run:0`, một kho riêng). Cả hai đều
                    // "đúng" một mình, cùng đọc một tín hiệu đoán-chữ, cùng đổ
                    // vào một danh sách — và chẳng chỗ nào hỏi "đã có ai dựng
                    // nút này chưa". Khối này đi trước (14/08); khối trên đi
                    // nốt 16/08 khi Hà bỏ hẳn cái nút.
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
                    // CHÍNH HUBA (`pipeline.rs` chèn `# Lệnh thấy trên màn…` vào
                    // bản trả lời `/shot`), nên cái nút đang mời gửi lại lời
                    // của chính nó. Hai: bản cũ ĐỌC chữ trong ô rồi GÕ LẠI chữ
                    // ấy — mà cái phân biệt "chữ đã gõ" với "gợi ý mờ" chính là
                    // MÀU, thứ `contents of tab` bỏ sạch. Đọc-rồi-gõ-lại là
                    // dựng một hành động lên trên một phép đo không làm được.
                    //
                    // Nút nay là một CỬ CHỈ, không phải một nội dung: bấm Enter
                    // vào đúng cửa sổ ấy, y như ngón tay chủ máy. huba không cần
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
                    // bảng đọc từ nhật ký (đúng cho cả phiên huba không đọc được
                    // màn).
                    // 🪦 `running` — cửa thứ ba của hai nút ⏎/⌫ ở đáy ("phiên
                    // đang chạy thì Ctrl+C là NGẮT lượt, không phải xoá ô"), đi
                    // cùng chúng 2026-08-16.
                    // 🪦 `has_choices` — cửa an toàn của hai nút ⏎/⌫ ở đáy, đi
                    // cùng chúng ngày 2026-08-16 (bia mộ bên dưới). Phép đo
                    // đúng của nó vẫn được ghi lại ở đây vì nó là một bài học
                    // chứ không phải một dòng mã: đo hộp chọn trên MÀN GỐC
                    // (`shot_choices`), KHÔNG `parse_choices(&ack)` — `ack`
                    // chép hộp chọn lên đầu tin nên đo trên nó ra
                    // `1,2,3,4,1,2,3,4`, luật "liên tiếp từ 1" trả rỗng, và
                    // cửa an toàn MỞ đúng lúc nó phải đóng.
                    // Hộp chọn trên màn ⟹ MỖI LỰA CHỌN MỘT CÁI NÚT.
                    //
                    // 🔴 Hà 2026-08-15, ảnh chụp `/shot` của `[dwork]` đang mở
                    // hộp khảo sát bốn lựa chọn: *"Có lựa chọn nhưng không thấy
                    // nút"*. Tin ấy mở đầu bằng đúng câu *"đang hỏi — bấm số ở
                    // hàng phím để chọn"* và liệt kê đủ bốn dòng — tức huba NHÌN
                    // THẤY hộp chọn, viết ra một lời hứa, rồi không giữ: cả
                    // route `/shot` chưa bao giờ dựng nút số. Đường duy nhất có
                    // nút là bảng `AskUserQuestion` đọc từ nhật ký
                    // (`ask_command_lines`), mà hộp này không phải bảng ấy —
                    // nó là hộp khảo sát của chính CLI, không có trong sổ.
                    //
                    // Đi đúng route sẵn có `/key <id> <số>`, không đẻ lối
                    // riêng: đó chính là đường CLAUDE.md ghi cho hộp MỘT CÂU,
                    // và hộp này một câu ("Question 1 of 3" là ba hộp nối
                    // tiếp, không phải một bảng ba cột).
                    //
                    // Nhãn chỉ mang SỐ: bốn dòng chữ đã nằm ngay trên đầu nút,
                    // nguyên văn — nhắc lại trong nhãn vừa thừa vừa bị cắt.
                    // 🔴 CỔNG NÀY TẮT NHẦM CẢ HỘP MỘT CÂU — Hà 2026-08-16:
                    // *"Màn có option nhưng không có bảng chọn"* · *"Option để
                    // chọn không chọn được"* · *"Màn option của mailler đã được
                    // đâu"*.
                    //
                    // Đo trên chính lượt `/shot` sau bản cài 10:44: tin mở đầu
                    // bằng *"đang hỏi — bấm số ở hàng phím để chọn"* rồi liệt kê
                    // đủ 5 dòng, mà log cùng lúc ghi `telegram_buttons_sent
                    // count=1` — tức huba VIẾT RA lời hứa rồi không giữ, đúng
                    // hình dạng đã bắt được ngày 08-15 và vá ở chỗ khác.
                    //
                    // Gốc là `shot_asking.is_none()`: đọc được bảng hỏi từ nhật
                    // ký thì tắt hẳn nhánh nút số, để dành đường `/pick`. Nhưng
                    // `/pick` chỉ CẦN cho bảng NHIỀU CÂU (xem CLAUDE.md §7:
                    // `/key <số>` là ngõ cụt ở đó vì các câu sau nằm sau một
                    // phím mũi tên). Bảng MỘT câu thì `/key` đủ, và tắt nó đi là
                    // đổi một đường đi được lấy một đường không có.
                    let nhieu_cau = shot_asking.as_ref().is_some_and(|a| !a.rest.is_empty());
                    if !nhieu_cau && !shot_sid.is_empty() {
                        for (n, _) in shot_choices.iter().take(9) {
                            quick.push((n.to_string(), format!("key:{shot_sid}:{n}")));
                        }
                    }
                    // 🔴 THANH TAB → MỖI TAB MỘT NÚT (Hà 2026-08-19: *"muốn
                    // chuyển tab thì bấm phím phải trái, giờ qua tele thì có nút
                    // bấm ở chính tab để nhận như click chuột"*).
                    //
                    // Nguồn là THANH TAB đọc từ màn, không phải nhật ký — và đó
                    // là cả điểm mấu chốt: một bảng hỏi ĐANG TREO **chưa được
                    // ghi vào nhật ký** (đo 2026-08-19: nhật ký 3,59 MB của phiên
                    // amm có **0** lần `AskUserQuestion` trong khi bảng nằm sờ sờ
                    // trên màn). Nên mọi thứ dựng trên `shot_asking` đều mù đúng
                    // lúc chủ máy cần nhất; màn thì không mù.
                    //
                    // Nhãn mang sẵn `☐`/`☒` để biết tab nào còn trống — chính
                    // con số quyết định bảng có gửi đi được hay chưa.
                    if !shot_sid.is_empty() {
                        if let Some(t) = crate::keys::ask_table(&ack) {
                            let short: String = shot_sid.chars().take(8).collect();
                            for (i, h) in t.headers.iter().enumerate().take(8) {
                                let mark = if t.answered.get(i).copied().unwrap_or(false) {
                                    "☒"
                                } else {
                                    "☐"
                                };
                                quick.push((
                                    format!("{mark} {}", crate::exec::truncate(h, 18)),
                                    format!("tab:{short}:{}", i + 1),
                                ));
                            }
                        }
                    }
                    // 🔴 Hà 2026-08-14: *"Sao ô nhắc trống, cũng không có gợi ý
                    // mờ mà vẫn có nút enter"*. Vì điều kiện trên KHÔNG hỏi
                    // trong ô có gì — chú thích cũ tự bào chữa rằng nút này là
                    // "một CỬ CHỈ, không phải một nội dung", đúng về bản chất
                    // phím và sai về chỗ đặt: một cử chỉ gửi vào ô rỗng thì
                    // không gửi gì cả, nên cái nút chỉ còn là tiếng ồn — và
                    // tiếng ồn trên màn 390px thì đắt.
                    //
                    // 🪦 HAI NÚT ⏎/⌫ Ở ĐÁY TIN — gỡ 2026-08-16. Hà, gửi kèm ảnh
                    // một `/shot` cửa sổ trần: *"2 cái nút ⏎ ⌫ trống ở cuối vẫn
                    // còn kìa"* → *"Bỏ 2 nút trống đó đi"*.
                    //
                    // Chúng đã sống qua ba lượt vá vì cùng một lý do: mỗi lượt
                    // chỉ siết thêm ĐIỀU KIỆN hiện nút (không chạy · không có
                    // hộp chọn · ô có chữ), mà điều kiện cuối đứng trên một
                    // phép đo huba làm không nổi — `input_box_text` đọc màn
                    // shell rồi tưởng dấu nhắc `hanguyen@… %` là chữ trong ô.
                    // Nên nút mọc ra ở đúng chỗ không có gì để gửi, và ở dạng
                    // TRỐNG: nhãn chỉ có một icon, không nói nó làm gì.
                    //
                    // Đường thật vẫn còn và tốt hơn hẳn: khi ô có chữ thật,
                    // `session_layout` chèn `⏎` NGAY SAU chữ ấy và `⌫ xoá ô
                    // nhập` xuống dòng dưới — có nhãn, nằm cạnh thứ nó tác
                    // động, và chỉ dựng được khi phép định vị tìm thấy dòng ô
                    // nhập thật. Hai đường cho một việc thì bỏ đường mù.
                    // text đó tới phiên"*) — nhưng nó đứng trên một phép đo huba
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
                // 🔴 CỬA KHÔNG ĐƯỢC PHỤ THUỘC VÀO "CÓ NÚT HAY KHÔNG" — Hà
                // 2026-08-16, ngay sau khi tôi gỡ hai nút ⏎/⌫ trống: *"Lại mất
                // nút gửi nhanh gợi ý mờ rồi, làm cái nọ hỏng cái kia thế"*.
                //
                // Điều kiện cũ là `!quick.is_empty()`, và nó SAI từ trước — chỉ
                // là hai cái nút trống luôn có mặt nên không ai thấy. Gỡ chúng
                // đi thì `quick` rỗng, tin rơi xuống `reply_in_channel` (chữ
                // trần), và mất luôn thứ KHÔNG phải nút: liên kết `⏎` chèn
                // giữa chữ ngay tại dòng ô nhập. Đo được trong log 14:34:40Z —
                // lượt `/shot` ấy không có một dòng `telegram_html_sent` nào.
                //
                // Cửa hỏi đúng một câu: *đây có phải chữ của phiên đi ra
                // Telegram không*. Có nút hay không là chuyện của bên trong.
                // 🔴 …NHƯNG câu XÁC NHẬN TRƠN vẫn đi bằng emoji — Hà 2026-08-16,
                // ngay sau bản trên: *"Chỉnh thành phản hồi bằng emoji rồi cơ
                // mà"*. Đúng: `✓ vào hàng chờ · [huba]` là ack của `/type`, và
                // từ 14/08 nó được thả thành một dấu lên chính tin chủ máy vừa
                // gõ (*"Vì nó đơn giản là xác nhận thôi không cần thông tin"*).
                // Bản vá "luôn đi qua cửa định dạng" ở trên nuốt mất nhánh ấy —
                // hai lượt liền tôi sửa một chỗ và làm hỏng chỗ bên cạnh, vì
                // đều quên hỏi *tin này có nội dung để định dạng không*.
                //
                // Cửa định dạng dành cho tin MANG CHỮ CỦA PHIÊN. Một câu chỉ
                // nói "đã nhận" thì không có gì để gắn action, và rút nó thành
                // một dấu là đúng ý nó.
                match crate::telegram::inbox() {
                    Some(tg) if adapter == crate::telegram::NAME && needs_formatting(&ack) => {
                        // 🔴 Hà 2026-08-14, ảnh chụp câu trả lời `/shot`: *"Vẫn
                        // thấy hiện nút kiểu cũ là sao"*. Vì đây là đường THỨ BA
                        // dùng cùng cuốn sổ nút, và nó chưa được nối vào máy móc
                        // icon-trong-chữ (hai đường kia: tin tự phát, bản đầy
                        // đủ). Đúng cái hình dạng đã lặp ba lần trong tệp này:
                        // vá một chỗ, quên chỗ bên cạnh, vì không ai bắt ba chỗ
                        // phải giống nhau. Nay CHÚNG GỌI CHUNG MỘT HÀM.
                        // Khai RÕ từng loại dữ liệu của phiên, rồi để một chỗ
                        // duy nhất gắn action — xem `SessionData`.
                        let data = SessionData {
                            sid: shot_sid.clone(),
                            cmds: cmd_lines.clone(),
                            choices: shot_choices.clone(),
                            // Màn có dòng `Submit` ⟹ gắn ✅ ngay tại đó (Hà
                            // 17/08: *"chưa bấm được submit"*). Hỏi bằng chính
                            // hàm dựng chuỗi phím, nên "thấy Submit" ở hai chỗ
                            // là MỘT phép đo.
                            submit: shot_submit || crate::keys::has_submit(&ack),
                            // Đo trên ảnh màn, không đo lại trên `ack` — `ack`
                            // đã mang cả khu chữ huba tự nối.
                            box_text: shot_box.clone(),
                            files: shot_files.clone(),
                            // Thanh tab đọc từ MÀN, không từ nhật ký: bảng đang
                            // TREO chưa được ghi vào nhật ký (đo 2026-08-19 —
                            // 3,59 MB nhật ký phiên amm có 0 lần
                            // `AskUserQuestion` trong khi bảng nằm trên màn),
                            // nên mọi thứ dựng trên `asking` đều mù đúng lúc
                            // chủ máy cần nhất.
                            tabs: crate::keys::ask_table(&ack)
                                .map(|t| {
                                    t.headers
                                        .iter()
                                        .enumerate()
                                        .map(|(i, h)| {
                                            (
                                                i + 1,
                                                h.clone(),
                                                t.answered.get(i).copied().unwrap_or(false),
                                            )
                                        })
                                        .collect()
                                })
                                .unwrap_or_default(),
                        };
                        // Lựa chọn nào đã thành ☑ trong chữ thì thôi nằm ở đáy:
                        // hai đường cho một việc, và cái ở đáy chỉ mang con số
                        // trần, không nói nó chọn cái gì.
                        let quick: Vec<(String, String)> = if data.choices.is_empty() {
                            quick.clone()
                        } else {
                            quick
                                .iter()
                                .filter(|(_, d)| {
                                    !d.starts_with("key:")
                                        || d.ends_with(":enter")
                                        || d.ends_with(":clear")
                                })
                                .cloned()
                                .collect()
                        };
                        // 🔴 SỬA TẠI CHỖ khi đây là một BẢNG của cùng phiên —
                        // Hà 2026-08-17: *"Khi bấm ở phản hồi nên sửa tin tại
                        // phản hồi đó luôn không cần gửi 1 tin mới"*.
                        //
                        // Chỉ áp cho bảng lựa chọn: nó là thứ người ta bấm
                        // nhiều lần liên tiếp, và mỗi lần một tin thì buồng
                        // chat đầy những bản gần giống nhau mà bản đúng là cái
                        // cuối. Còn `/shot` là ẢNH của một thời điểm — sửa đè
                        // lên ảnh cũ là xoá mất thứ chủ máy có thể đang đối
                        // chiếu, nên nó vẫn gửi tin mới.
                        let panel = if data.choices.is_empty() {
                            None
                        } else {
                            panel_id(db, &shot_sid)
                        };
                        let sent = say_session_data_at(
                            tg,
                            &ack,
                            &quick,
                            "quick_buttons_failed",
                            &data,
                            panel.filter(|_| matches!(cmd.kind, CommandKind::Key)),
                        );
                        if !data.choices.is_empty() {
                            remember_panel(db, &shot_sid, sent);
                        }
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
                let live = crate::sessions::snapshot(cfg);
                // Phiên VỪA TẮT vẫn hỏi được: `--resume` chạy trên nhật ký, không
                // cần tiến trình. Đây đúng là ca Hà gặp 16:37 — con trỏ trỏ vào
                // phiên vừa tắt và huba trả lời bằng một ngõ cụt. Xem `ENDED_KEY`.
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
                // Câu trả lời bên lề là chữ CỦA PHIÊN ấy — qua cửa định dạng.
                let ack_sid = target
                    .as_ref()
                    .map(|s| s.session_id.clone())
                    .unwrap_or_default();
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
                reply_from_session(db, cfg, adapter, cmd, &ack_sid, &ack);
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
                    let live = crate::sessions::snapshot(cfg);
                    let focus = db.cursor_or_log(FOCUS_SESSION_KEY).unwrap_or_default();
                    // 🔴 CHỈ cửa sổ đang chạy CLI — Hà 2026-08-15: *"lệnh session
                    // liệt kê cửa sổ đang chạy cli"*, và `/terminal` nhận phần
                    // còn lại (*"liệt kê terminal thuần không chạy gì"*).
                    //
                    // Trước lượt này ảnh chụp trộn cả hai vào MỘT danh sách
                    // (`sessions::add_shell_windows`), nên `/session` trả về hai
                    // hạng vật khác hẳn nhau — một cái gõ chữ vào là nói với một
                    // phiên `claude`, một cái gõ vào là chạy lệnh shell. Cùng
                    // một danh sách, hai nghĩa của cú bấm: đó đúng là chỗ sinh
                    // ra cảm giác rối.
                    let cli_rows: Vec<crate::sessions::LiveSession> = live
                        .sessions
                        .iter()
                        .filter(|s| s.host != "shell")
                        .cloned()
                        .collect();
                    let live = crate::sessions::SessionsSnapshot {
                        sessions: cli_rows,
                        ..live
                    };
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
                    // 🔴 ĐÍCH CHẠM NẰM TRÊN CHÍNH HÀNG, KHÔNG PHẢI MỘT NÚT LẶP
                    // LẠI HÀNG ẤY — Hà 2026-08-22, ảnh 21:36: *"Vẫn đang hiện
                    // cả danh sách lẫn nút thừa thãi"*. Cái nút chép lại đúng
                    // bốn thứ hàng chữ vừa nói (icon tình trạng · nguồn · tên ·
                    // tài khoản) và không thêm gì ngoài việc bấm được. Rút ngắn
                    // NHÃN không cứu được: Telegram cho nút một chiều cao cố
                    // định, nên sáu nút vẫn ăn chừng ấy màn hình dù nhãn còn ba
                    // chữ. Đường duy nhất là bỏ nút, và đưa cái bấm được lên
                    // hàng (`👉`, payload `s_<uuid>` — cùng lệnh nút vẫn gửi).
                    let mut sent = false;
                    if adapter == crate::telegram::NAME {
                        if let Some(tg) = crate::telegram::inbox() {
                            let rows = live.sessions.len().min(MAX_SESSION_BUTTONS);
                            let (html, linked) = session_list_html(
                                &crate::telegram::strip_markdown(&ack),
                                &live.sessions,
                            );
                            // MỌI hàng phải có đích chạm hoặc KHÔNG hàng nào:
                            // nửa danh sách bấm được nửa không là thứ tệ hơn cả
                            // hai lựa chọn, vì ngón tay học sai một lần rồi thôi
                            // tin cả cái danh sách.
                            if rows > 0 && linked == rows {
                                match tg.send_html(&html) {
                                    Ok(()) => {
                                        sent = true;
                                        logging::info(
                                            "session_taps_sent",
                                            json!({ "rows": rows, "linked": linked }),
                                        );
                                    }
                                    Err(e) => logging::error(
                                        "telegram_ack_failed",
                                        json!({ "err": e, "what": "session_taps" }),
                                    ),
                                }
                            }
                            // Đường lùi: chưa biết tên bot ⟹ không dựng được
                            // deep link ⟹ danh sách không có chỗ nào bấm. Lúc ấy
                            // cái nút vẫn hơn một tin chỉ để đọc.
                            if !sent {
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
                    }
                    if !sent {
                        reply_in_channel(db, cfg, adapter, cmd, &ack);
                    }
                    // 🔴 KHÔNG đi qua `reply_from_session` ở đây, và đó là chủ
                    // ý: tin này nói về NHIỀU phiên: gắn action theo một sid là
                    // gắn sai phiên cho phần lớn các dòng. Đích chạm của mỗi
                    // hàng (`s_<uuid>`) đã tự mang phiên của nó.
                    // Giá trị của NHÁNH này, không phải `return`: `return` ở đây
                    // sẽ bỏ luôn những lệnh còn lại trong cùng một lượt.
                    Some(ack)
                } else {
                    // Đường nhanh tự lo phần nói năng của nó (sửa tin gim + một
                    // dòng "đang quét màn" sống ngắn), nên chỗ gửi ở cuối phải
                    // biết mà ĐỪNG gửi lại. Cờ chứ không phải `return`: `return`
                    // ở đây bỏ luôn những lệnh còn lại trong cùng một lượt.
                    let mut tu_tra_loi = false;
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
                                // 🔴 BỎ CHỮ "ĐANG THEO PHIÊN" — Hà 2026-08-26: *"Chỉnh tin gim bỏ
                                // text 'đang theo phiên' đi"*. Tin này được GIM
                                // lên đỉnh buồng chat, nên nó không cần tự giới
                                // thiệu: chỗ nó nằm đã nói nó là gì. Cái còn lại
                                // là thứ ngón tay đang tìm — TÊN phiên.
                                //
                                // `👁 ` ở đầu vẫn giữ, nhưng chỉ làm DẤU NHẬN
                                // BIẾT cho chỗ gửi (nó bóc ra rồi thay bằng 📷).
                                // CÙNG một hàm với `pin_line`, nên hai dòng chỉ
                                // có thể khác nhau đúng ở cái icon — xem
                                // `pin_line_from` để biết vì sao chỗ này không
                                // được phép có một bản `format!` riêng.
                                let head = pin_line_from("👁", &name, &account);
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
                                        // 🔴 SỬA TIN GIM, ĐỪNG GỬI TIN CHÀO MỚI —
                                        // Hà 2026-08-26: *"chỗ tin phản hồi này
                                        // chưa hợp lý khi kích chọn vào phiên …
                                        // hiện tại khi chọn vào phiên đã có sẵn
                                        // pin message rồi"*. Dòng đang gim trên
                                        // đỉnh nói đúng cái điều tin chào định
                                        // nói, nên tin chào chỉ là một dòng thừa
                                        // nằm lại vĩnh viễn sau mỗi cú chạm.
                                        //
                                        // Icon ở đây là `👁`, KHÔNG phải icon
                                        // trạng thái: đường nhanh cố ý không dựng
                                        // ảnh chụp (xem `session_name_from_book`
                                        // — đường cũ mất 48 giây cho đúng hai
                                        // chuỗi ký tự), nên nó không có
                                        // `LiveSession` để hỏi `state_of`.
                                        // `refresh_pin` thay đúng icon ấy ở vòng
                                        // quét kế; giữa hai nhịp thì TÊN phiên —
                                        // thứ ngón tay đang tìm — đã đúng rồi.
                                        //
                                        // Gim hụt ⟹ `tu_tra_loi` giữ nguyên
                                        // `false` ⟹ rơi về đường gửi cũ ở cuối.
                                        // Không có vế ấy thì một lượt gim hỏng =
                                        // cú chạm của chủ máy không có hồi âm nào.
                                        if cfg.pin_following
                                            && pin_apply(db, tg, want, &head, true).is_some()
                                        {
                                            // Cú chụp tốn vài giây (`command_done
                                            // Shot` đo được 2,7s · 4,5s). Khoảng
                                            // lặng ấy phải có người nói — rồi đi,
                                            // vì nó là TRẠNG THÁI TẠM.
                                            scan_notice(db, tg, &name);
                                            tu_tra_loi = true;
                                        }
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
                        let live = crate::sessions::snapshot(cfg);
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
                                    // chết, huba chào *"đã dừng, vẫn nói tiếp được"*,
                                    // rồi `/shot` ngay sau đó trả *"không có cửa sổ
                                    // terminal để gõ (host: dead)"*. Hai câu của
                                    // cùng một huba, cách nhau vài giây, chọi nhau.
                                    // Câu đầu đúng theo nghĩa hẹp (`/tell` dựng lại
                                    // được phiên nền) nhưng người đọc hiểu thành
                                    // "gõ tiếp được", vì đó là thứ mọi phiên khác
                                    // cho phép.
                                    let how = match (s.pid == 0, s.host.as_str()) {
                                    // 🔴 Cửa sổ TRẦN không phải một phiên đã
                                    // dừng. Bản cũ rơi vào nhánh `pid == 0` và
                                    // trả lời *"đã dừng, /tell nói tiếp được"* —
                                    // mà `/tell` chỉ nói được với một phiên
                                    // `claude`. Một câu mời đi vào ngõ cụt.
                                    (_, "shell") => " — dấu nhắc trống: gõ chữ ở đây là chạy lệnh shell",
                                    (_, "dead") => {
                                        " — ĐÃ TẮT: chỉ còn /handover lấy bản bàn giao; gõ thẳng thì không. \
                                         Dọn khỏi danh sách bằng /stop"
                                    }
                                    (true, _) => " — đã dừng; /new <id> mở lại nó trong một cửa sổ",
                                    _ => "",
                                };
                                    let head = follow_ack_head(&s, how);
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
                                    // 🔴 MỘT CÁI NÚT, KHÔNG PHẢI MỘT CÁI TÊN —
                                    // Hà 2026-08-17: *"Khi bấm vào 1 phiên từ
                                    // danh sách lại không có nút xem chi tiết,
                                    // sau đó lại càng không xem được"*. Dòng
                                    // `(xem màn: /shot)` bảo người ta tự gõ lại
                                    // một cái tên lệnh trên điện thoại — đúng
                                    // nhịp cuối mà cây cầu bỏ dở. Nút gắn ngay
                                    // dưới câu chào (xem `shot:` trong
                                    // `telegram::button_command`).
                                    head
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
                    // Câu chào đi kèm NÚT 📷 khi vừa chọn được một phiên: bấm
                    // là thấy màn, không phải gõ lại tên lệnh. Lời từ chối thì
                    // không có nút — không có phiên nào để mà xem.
                    let shot_btn: Vec<(String, String)> = if ack.starts_with('👁') {
                        vec![("📷 Xem màn".to_string(), format!("shot:{want}"))]
                    } else {
                        Vec::new()
                    };
                    match (shot_btn.is_empty(), crate::telegram::inbox()) {
                        // Đường nhanh đã sửa tin gim + đặt dòng "đang quét màn"
                        // rồi: gửi thêm ở đây là đẻ lại đúng cái tin thừa vừa bỏ.
                        _ if tu_tra_loi => {}
                        (false, Some(tg)) if adapter == crate::telegram::NAME => {
                            // 🔴 CẢ DÒNG LÀ ĐÍCH CHẠM, ICON ĐI VÀO TRONG — Hà
                            // 2026-08-26: *"nút xem màn bỏ text đi để icon và
                            // bao hết text của tin gim"*.
                            //
                            // Cái nút bàn phím `shot:<id>` đứng RỜI ở đáy tin
                            // nên không bọc được chữ; đích chạm to đúng bằng
                            // cái emoji. Cùng lỗi đã vá cho dòng lệnh (23/08),
                            // ô nhập (25/08) và tên tệp (25/08) — đây là chỗ
                            // thứ tư của cùng một luật.
                            //
                            // `ack` mở đầu bằng `👁 ` (dấu nhận biết của nhánh
                            // này). Bóc nó ra rồi bọc phần còn lại vào thẻ với
                            // icon 📷 — một icon, không phải hai.
                            let than = ack.strip_prefix("👁 ").unwrap_or(&ack);
                            let lien_ket = crate::telegram::deep_link(&format!("shot_{want}"));
                            let gui = match &lien_ket {
                                // Không dựng được liên kết (chưa biết tên bot)
                                // ⟹ rơi về NÚT cũ. Đường lùi phải còn: mất cái
                                // nút là mất đường xem màn, không chỉ mất đẹp.
                                None => tg.send_buttons_id(&ack, &shot_btn),
                                Some(href) => tg
                                    .send_html_report(
                                        &format!(
                                            "<a href=\"{}\">📷 {}</a>",
                                            crate::telegram::html_escape(href),
                                            crate::telegram::html_escape(than)
                                        ),
                                        &[],
                                    )
                                    .map(|s| Some(s.message_id)),
                            };
                            match gui {
                                // Gim tin ấy lên đỉnh buồng chat. Chỉ gim khi
                                // Telegram TRẢ VỀ id — gim một id đoán bừa là
                                // gim nhầm tin của người khác.
                                Ok(Some(mid)) if cfg.pin_following => {
                                    pin_following(db, tg, mid, than);
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    logging::error(
                                        "telegram_ack_failed",
                                        json!({ "err": e, "what": "follow_ack_shot_button" }),
                                    );
                                    reply_in_channel(db, cfg, adapter, cmd, &ack);
                                }
                            }
                        }
                        _ => reply_in_channel(db, cfg, adapter, cmd, &ack),
                    }
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
        // Màn đã tới ⟹ dòng "đang quét màn" hết việc (xem `SCAN_NOTICE_KEY`).
        //
        // Đặt ở ĐÂY, sau khi nhánh phía trên đã gửi câu trả lời của nó, nên thứ
        // tự trên buồng chat là: ảnh màn tới, dòng chờ biến mất. Và tính CẢ nhánh
        // trả lời bằng một lời TỪ CHỐI ("phiên nền — không có màn nào để chụp"):
        // với chủ máy thì tin đã tới, còn một dòng "đang quét" treo lại sau một
        // câu từ chối là huba nói dối về việc nó đang làm.
        if matches!(
            cmd.kind,
            CommandKind::Shot | CommandKind::Photo | CommandKind::Tab
        ) {
            if let Some(tg) = crate::telegram::inbox() {
                clear_scan_notice(db, tg);
            }
        }
        logging::info(
            "command_done",
            json!({ "kind": format!("{:?}", cmd.kind), "adapter": adapter,
                    "ms": cmd_started.elapsed().as_millis() }),
        );
        // Menu ☰ xếp theo cái thật sự đang được dùng — xem `menu_reorder_if_needed`.
        menu_reorder_if_needed(db, cmd.kind, chrono::Utc::now().timestamp_millis());
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
                // thì huba biến lỗi chính tả thành một lượt gõ thật.
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
    // yêu cầu xác nhận sang Telegram: …` — huba nói với Telegram rằng nó vừa gửi
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
/// Bộ emoji Telegram cho phép thả lên một tin nhắn (Bot API, `ReactionTypeEmoji`).
///
/// Chép vào mã vì nó là một HỢP ĐỒNG với máy chủ Telegram, không phải một lựa
/// chọn thẩm mỹ: thả một emoji ngoài bộ này thì `setMessageReaction` trả
/// `REACTION_INVALID`, và cái giá là một dòng chữ thừa trong buồng chat chứ
/// không phải một dấu xấu.
pub const REACTIONS: &[&str] = &[
    "👍", "👎", "❤", "🔥", "🥰", "👏", "😁", "🤔", "🤯", "😱", "🤬", "😢", "🎉", "🤩", "🤮", "💩",
    "🙏", "👌", "🕊", "🤡", "🥱", "🥴", "😍", "🐳", "🌚", "🌭", "💯", "🤣", "⚡", "🍌", "🏆", "💔",
    "🤨", "😐", "🍓", "🍾", "💋", "🖕", "😈", "😴", "😭", "🤓", "👻", "👀", "🎃", "🙈", "😇", "😨",
    "🤝", "✍", "🤗", "🫡", "🎅", "🎄", "☃", "💅", "🤪", "🗿", "🆒", "💘", "🙉", "🦄", "😘", "💊",
    "🙊", "😎", "👾", "😡",
];

pub fn project_emoji(name: &str) -> &'static str {
    // Cố ý chọn những dấu KHÁC HẲN nhau về hình, không phải sắc thái cảm xúc:
    // trên một dòng chat, người ta phân biệt bằng bóng hình chứ không bằng nét
    // mặt. Và dấu này hay đi MỘT MÌNH — `telegram_ack_as_reaction` thả nó lên
    // tin nhắn, không kèm tên dự án — nên hai dự án chung một dấu là mất trắng
    // thông tin, không phải một va chạm nhỏ.
    // 🔴 CHỈ ĐƯỢC LẤY TỪ BỘ TELEGRAM CHO PHÉP THẢ — xem [`REACTIONS`].
    //
    // Lượt nở bảng 20 → 59 ô ngày 2026-08-20 đã quên đúng điều ấy: 39 ô thêm
    // vào (🌵 🍄 🐙 🦉 🧲 🪃 …) là emoji hợp lệ, nhưng **không nằm trong bộ
    // reaction của Telegram**. Hậu quả đo được ngày 23/08: `setMessageReaction`
    // trả `Bad Request: REACTION_INVALID` **11 lần trong một buổi**, huba rơi về
    // đường chữ, và Hà nhìn thấy đúng cái hệ quả ấy: *"sao phản hồi đã gửi của
    // tin nhắn cứ nhảy đi nhảy lại khi làm có các cập nhật mới thế"* — dòng chữ
    // ấy khi thì bị sửa tại chỗ (nằm nguyên chỗ cũ), khi thì gửi mới (xuống
    // đáy), nên nó *nhảy*.
    //
    // Bảng nở ra để tránh hai dự án trùng dấu; nhưng một cái dấu KHÔNG THẢ ĐƯỢC
    // thì không phân biệt được gì cả — nó chỉ đổi một va chạm hiếm lấy một lời
    // từ chối chắc chắn. 34 ô dưới đây đều thả được, và đều khác hẳn nhau về
    // BÓNG HÌNH chứ không phải sắc thái nét mặt.
    // 🔴 ĐÚNG 59 Ô, và con số ấy ĐO ĐƯỢC chứ không phải chọn cho đẹp. Băm
    // FNV-1a của 15 tên dự án thật trên máy này, chia lấy dư cho mọi độ dài từ
    // 30 tới 69: **chỉ 59 và 65 là không có hai tên nào chung một ô**. Bảng cũ
    // may mà đúng 59; lượt siết bộ dấu (23/08) co nó xuống 34 rồi 58 và cả hai
    // lần đều làm đỏ `a_project_always_gets_the_same_mark` — bài kiểm ấy làm
    // đúng việc của nó.
    //
    // Thêm dự án mới mà đỏ ở đây thì ĐỪNG hạ chuẩn: đo lại độ dài nào không
    // đụng, rồi thêm/bớt ô cho vừa — và ô thêm vào phải nằm trong [`REACTIONS`].
    const PALETTE: &[&str] = &[
        "👍", "❤", "🔥", "🥰", "👏", "😁", "🤔", "🤯", "😱", "🎉", "🤩", "🙏", "👌", "🕊", "🤡",
        "🥱", "🥴", "😍", "🐳", "🌚", "🌭", "💯", "🤣", "⚡", "🍌", "🏆", "🤨", "😐", "🍓", "🍾",
        "💋", "😈", "😴", "🤓", "👻", "👀", "🎃", "🙈", "😇", "😨", "🤝", "✍", "🤗", "🫡", "🎅",
        "🎄", "☃", "💅", "🤪", "🗿", "🆒", "💘", "🙉", "🦄", "😘", "💊", "🙊", "😎", "👾",
    ];
    let key = name
        .trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_lowercase();
    if key.is_empty() {
        return "👍";
    }
    // FNV-1a chứ không phải tổng byte: tổng byte không phân biệt được thứ tự
    // chữ, mà tên dự án thì thường cùng bộ chữ cái.
    //
    // 🔴 BẢNG NỞ TỪ 20 LÊN 59 Ô, 2026-08-20, và lý do đáng chép lại vì nó là
    // một cái bẫy đo lường. Bài kiểm cũ đòi "7 tên ⟹ ít nhất 6 dấu khác nhau",
    // tức nó DUNG TÚNG sẵn một cặp trùng — và có một cặp trùng thật:
    // `dwork` và `social` cùng ra 😎, đã thế từ lâu. Lượt đổi tên
    // `hub`→`huba` thêm cặp thứ hai (`huba` đụng `sdvi` ở 👻) làm bài kiểm đỏ,
    // và chỉ lúc ấy mới lòi ra cặp cũ. Một ngưỡng "gần đúng là được" không
    // canh gì cả; nó chỉ hoãn ngày người ta biết.
    //
    // Đo trên 15 cái tên có thật của máy này: 20 ô cho 12 dấu, 59 ô cho đủ 15.
    // NÓI THẲNG GIỚI HẠN: băm-theo-tên KHÔNG bảo đảm phân biệt được — nó không
    // biết những tên khác tồn tại. 59 là con số đủ rộng để roster hôm nay sạch,
    // không phải một chứng minh. Thêm dự án thứ 16 vẫn có thể đụng (~25%), và
    // `a_project_always_gets_the_same_mark` sẽ đỏ ngay lúc đó — ĐỪNG nới ngưỡng
    // bài kiểm khi gặp: cách chữa thật là gán dấu theo CẢ DANH SÁCH dự án
    // (băm lấy ô ưa thích rồi dò sang ô trống kế tiếp), thứ cần biết roster nên
    // phải đổi chữ ký hàm — một việc riêng, không nhét vào lượt đổi tên.
    //
    // Giá đã trả một lần: mọi dự án đổi dấu, vì `% PALETTE.len()` đổi. Trả ở
    // đây là rẻ nhất — `huba` vừa đổi tên nên dấu của nó đằng nào cũng đổi.
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

/// Một TRẠNG THÁI mà huba có thể trả lời bằng đúng một dấu.
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
    // 🔴 NHIỀU DÒNG ⟹ KHÔNG PHẢI XÁC NHẬN TRƠN. Một cái dấu thả lên tin gốc nói
    // được đúng "đã nhận"; nó không nói thay được một BẢNG. Từ 17/08 ack của một
    // cú bấm trong hộp chọn mang cả danh sách ô đã tích (Hà: *"Phản hồi nên thêm
    // ô đã tích hay chưa và cho phép bấm được luôn"*), và câu ấy vẫn mở đầu bằng
    // `✓` — thiếu cổng này thì nó bị rút thành một mặt cười, mất sạch bảng.
    if t.lines().count() > 1 {
        return None;
    }
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
    // Bước phụ của chính huba thì im — xem `telegram::Incoming::quiet`.
    if cmd.quiet {
        logging::info(
            "command_reply_muted",
            json!({ "kind": format!("{:?}", cmd.kind), "why": "bước phụ do huba xếp hàng" }),
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
            // máy, rồi `✓ đã gửi · [tên]` của huba — mà dòng thứ hai không mang
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
            // 🔴 Không thả được dấu thì câu xác nhận vẫn phải đọc được — nhưng
            // nó KHÔNG cần một dòng mới mỗi lần. Cú bấm đi qua một liên kết
            // trong chữ (`t.me/<bot>?start=k_…`) không có tin nào để thả dấu
            // lên: tiếng vọng `/start` bị huba dọn ngay khi nhận. Đo ngày 17/08:
            // **73 dòng `✓ đã gửi · …`** cho 73 cú bấm phím, xếp dọc buồng chat,
            // đúng thứ Hà bảo bỏ (*"Có thể đổi cách phản hồi … cho gọn"*).
            // `send_ack` gộp câu giống hệt vào chính tin trước (`×N`), và chỉ
            // gộp khi nó còn là tin cuối.
            let sent = if ack_as_emoji(text).is_some() {
                i.send_ack(text)
            } else {
                i.send_text(text)
            };
            if let Err(e) = sent {
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
    // Không giữ lại thì `huba status` và khối "lỗi gần đây" của `/doctor` đọc một
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
    // 🔴 BA VIỆC MỖI VÒNG, và cả ba đã CHẾT CÂM từ 2026-08-14 — đây là bản vá.
    //
    // Chúng từng được gọi từ `portal::build_inner`, tệp đẩy ảnh chụp lên trang
    // tfl5. Trang chết ngày 14/08 (`cf20874`), `lib.rs` bỏ `mod portal`, và ba
    // cỗ máy này mất chỗ gọi DUY NHẤT của chúng — không một cảnh báo nào, vì
    // `pub fn` trong `pub mod` thì trình dịch không kêu "không ai gọi".
    //
    // Đo trên `logs/huba.log` (không phải suy luận): `session_change` 439 lượt,
    // **lần cuối 14/08 13:10:40**; `close_still_busy`/`close_done` lần cuối
    // 14/08 11:17–11:20; `trust_dialog_answered` lần cuối 14/08 07:58. Sau đó
    // đúng 0 lượt trong hơn một ngày. Hậu quả nhìn thấy được: luật 11 (*"huba
    // chỉ nói khi có THAY ĐỔI"*) không nói được câu nào nữa, và sổ
    // `closing:windows` ngồi trong DB với **hai** hàng `c: 0` — tức chưa từng
    // được ngó lại một lần — trong khi Hà nhìn thấy cửa sổ phiên cũ còn nguyên
    // và hỏi *"tại sao chuyển phiên mới rồi mà phiên cũ vẫn còn cửa sổ chưa
    // đóng?"*.
    //
    // 📌 Đây là bản LẶP LẠI của con bug đã ghi trong `CLAUDE.md` (`errors_block`
    // sống trong `runtime::snapshot`, chỗ gọi duy nhất là `portal.rs`) — cùng
    // một commit, cùng một hình dạng, ba lần nữa. Bài học không phải "nhớ kiểm
    // chỗ gọi": mà là **gỡ một tệp thì phải đi hỏi từng hàm nó gọi xem còn ai
    // gọi không** — `grep -L` chứ không phải trí nhớ.
    //
    // MỘT ảnh chụp cho cả vòng: bốn việc dưới đây hỏi cùng một câu ("máy đang
    // chạy gì"), dựng hai lần là hai câu trả lời lệch nhau — mà cái loa thì so
    // hai lượt ảnh chụp để quyết định có nói hay không.
    let mut live = crate::sessions::snapshot(cfg);
    mark_started_by_hub(db, &mut live);
    announce_changes(db, cfg, &live);
    // Giữ tin gim đúng với sự thật — xem `refresh_pin`. Đặt ngay sau cái loa vì
    // dùng CHUNG một ảnh chụp: dựng hai lần là hai câu trả lời lệch nhau.
    refresh_pin(db, cfg, &live);
    let now_sec = chrono::Utc::now().timestamp();
    // Cửa sổ đang chờ đóng: ngó lại một lượt (rẻ — một câu AppleScript mỗi cửa
    // sổ, và chỉ khi đã qua 30 giây). Phải bám VÒNG CHẠY chứ không đứng chờ
    // trong lượt lệnh: chờ tại chỗ là giữ `CMD_LOCK` (xem `CLOSING_KEY`).
    close_pending_tick(db, cfg, now_sec);
    // Cửa sổ kẹt ở hộp tin-thư-mục: chưa có id phiên nên không route nào với
    // tới — phải có người ngó lại mỗi vòng (xem `trust_dialog_tick`).
    trust_dialog_tick(now_sec);
    // Kết quả `/runin` đã chạy xong mà chưa gõ vào phiên được: gõ lại. Đặt
    // cạnh hai cái tick trên vì cùng một hình dạng việc — thứ hỏng vì Terminal
    // bận một lúc, không hỏng vì sai.
    runin_pending_tick(db, cfg, now_sec);
    // …và nhận thư của chính các phiên: chúng tự xếp lệnh vào scratchpad của
    // mình, huba đọc id từ đường dẫn nên không phiên nào phải khai id.
    // 🔴 RÚT LỆNH LẦN NỮA, ngay sau phép chụp — Hà 2026-08-25: *"sau khi gửi
    // lệnh session đợi rất lâu mới nhận được tin phản hồi, nó không phải là
    // luồng độc lập à?"*.
    //
    // Không, và đây là chỗ thấy rõ nhất: lệnh chỉ được rút ở ĐẦU vòng
    // (`execute_telegram_commands` bên trên). Lệnh nào tới giữa lúc vòng đang
    // chụp ảnh phiên hay đọc màn thì nằm chờ hết cả vòng rồi mới tới lượt.
    // Đo trên log: từ lúc tin vào hàng tới lúc trả lời, **trung vị 10,3 giây,
    // lâu nhất 73,6 giây** (72 lượt `/start`, tức chạm liên kết trong chữ).
    //
    // Rút thêm một nhát ở đây cắt đúng khúc đắt nhất ra khỏi thời gian chờ.
    // Rẻ khi rỗng: một lần khoá, một lần `drain`, rồi trả về ngay.
    //
    // ⚠ Đổi lại, `live` bên dưới có thể cũ đi một lệnh. Chấp nhận được vì mọi
    // chỗ dùng nó đều tự kiểm lại với máy thật: `auto_handover` đọc màn tươi
    // trước khi quyết, còn hai tick `runin_*` tự chụp lại ảnh phiên của chúng.
    execute_telegram_commands(db, cfg);
    runin_inbox_tick(db, cfg);
    let watching = auto_handover(db, cfg, &live);
    // …và phiên nào đang đứng chờ một lệnh CHỦ MÁY phải gõ thì gõ hộ, nếu lệnh
    // ấy nằm trong danh sách cho phép. Đứng SAU `auto_handover` có chủ ý: đóng
    // sổ là việc đổi cả cửa sổ, còn đây chỉ là gõ một dòng — thứ nặng tay hơn
    // được nhìn phiên ở trạng thái chưa ai đụng vào.
    auto_run(db, cfg, &live);
    // No triage, and nothing to flush. huba used to spend money on its own here:
    // every line typed in the room went through a `claude -p` call to be sorted
    // into an inbox, and a daily ceiling existed to stop that from running away.
    // The inbox is gone (2026-08-08) and the room now carries orders, not mail
    // — so the only thing that costs money is a button the owner presses
    // (`/ask`, `/handover`, `/new`, `/tell`). huba no longer spends unwatched,
    // which is why the ceiling that guarded it is gone too.
    let summary = CycleSummary {
        ms: started.elapsed().as_millis(),
        watching,
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
