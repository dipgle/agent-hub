//! Telegram làm **KÊNH RA LỆNH**, không chỉ cái loa.
//!
//! # Vì sao có tệp này
//!
//! Hà 2026-08-11: *"nếu làm việc hoàn toàn qua kênh tele thì có gửi được nội
//! dung chát không"* — và câu trả lời lúc ấy là **không**: `confirm.rs` chỉ đọc
//! `callback_query` (hai cái nút Xác nhận/Huỷ), chỉ trong lúc đang chờ một câu
//! xác nhận, còn tin nhắn chữ thì bị bỏ qua hoàn toàn. Tức Telegram là cái loa
//! có đúng hai cái nút, mọi mệnh lệnh vẫn phải đi qua phòng chat tfl5.
//!
//! Kèm theo là lỗ hổng thứ hai, cùng gốc: phiên **dừng lại hỏi** thì hub bắn
//! một tin `⚠ … cần bạn chọn` — mà từ Telegram **không chọn được**. Người ta
//! phải mở trang ra bấm. Nay từng lựa chọn thành một cái nút.
//!
//! # Luật của kênh này
//!
//! 1. **ĐÚNG MỘT nơi đọc `getUpdates`.** Telegram giao mỗi update cho người hỏi
//!    trước, và `offset` là con dấu "đã nhận tới đây" dùng chung cho cả bot.
//!    Hai vòng đọc song song thì chúng **ăn mất update của nhau**: một cú bấm
//!    Xác nhận rơi vào vòng đọc lệnh sẽ biến mất, và `confirm::ask` ngồi chờ tới
//!    hết giờ rồi kết luận "không ai bấm" — một câu sai, gửi cho đúng người vừa
//!    bấm. Nên `Inbox` giữ `offset` và cả cờ `busy`; lúc `confirm` đang hỏi thì
//!    vòng đọc **đứng im**, và chính `confirm` nhặt hộ tin chữ vào hàng đợi (bỏ
//!    rơi một mệnh lệnh vì "đang bận hỏi" là một lỗi im lặng).
//! 2. **Chỉ chủ máy ra lệnh được.** Cổng là `chat_id` trong `hub.env` — cùng
//!    luật với `trust.tfl5_user_tids` của phòng chat: được ở trong phòng là
//!    chuyện của Telegram, còn lái cái máy này là chuyện của chủ máy. Tin từ
//!    người khác được LOG rồi bỏ, không im lặng.
//! 3. **Không có bí mật nào đi ra.** Mọi câu trả lời gửi qua đây đi cùng đường
//!    với phòng chat, tức đã qua cổng quét rò rỉ ở nguồn (`sessions::snapshot`,
//!    `gate_preview`). Tệp này không tự bịa thêm nội dung nào.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};

use crate::config::Config;
use crate::logging;

/// Tên kênh trong log và trong `reply_in_channel`.
pub const NAME: &str = "telegram";

/// Một mệnh lệnh vừa tới từ Telegram, chưa chạy.
#[derive(Debug, Clone)]
pub struct Incoming {
    /// Nguyên văn dòng người ta gõ (hoặc dòng lệnh do một cái nút sinh ra).
    pub text: String,
    /// Tin nào đã sinh ra lệnh này — để trả lời bằng một EMOJI thả thẳng lên
    /// nó thay vì đẻ thêm một tin.
    ///
    /// 🔴 Hà 2026-08-14: *"Có thể đổi cách phản hồi tin đã gửi bằng 1 emoji
    /// trực tiếp vào tin nhắn cho gọn"*. Gõ một câu vào phiên hiện nay tốn HAI
    /// tin trong buồng chat: câu của chủ máy, rồi `✓ đã gửi · [tên]` của hub —
    /// mà tin thứ hai không mang thông tin nào ngoài "đã nhận". Thả 👍 lên
    /// chính tin ấy nói đúng chừng ấy, và không chiếm dòng nào.
    pub msg_id: Option<i64>,
    /// Lệnh do CHÍNH hub xếp hàng làm bước phụ ⟹ chạy xong thì im.
    ///
    /// 🔴 Hà 2026-08-13: *"1 thao tác gửi 2 thông báo để làm gì"*. Nút
    /// `⏎ Gửi` xếp hàng hai lệnh (Esc xoá ô, rồi gõ chữ) và mỗi lệnh tự trả
    /// lời một câu — nên một cú bấm ra `✓ đã bấm 'esc'` rồi `✓ đã gửi`. Cú Esc
    /// là RUỘT của hub, không phải việc của người đọc; đúng cùng lý do đã bỏ
    /// dòng "(phải gửi thêm một Enter rời)" hôm qua.
    pub quiet: bool,
}

/// Hàng update chờ thợ, kèm mốc NHẬN của từng cái (để đo thời gian nằm chờ).
type Inflight = (Mutex<VecDeque<(Value, std::time::Instant)>>, Condvar);

/// Hòm thư Telegram dùng chung cho cả tiến trình.
#[derive(Clone)]
pub struct Inbox {
    queue: Arc<Mutex<VecDeque<Incoming>>>,
    /// `update_id` kế tiếp cần đọc. **Một** con dấu cho cả bot — xem luật 1.
    offset: Arc<Mutex<i64>>,
    /// `confirm::ask` đang giữ đường đọc.
    busy: Arc<AtomicBool>,
    /// Update đã NHẬN nhưng chưa xử lý: vòng đọc đẩy vào, luồng thợ lấy ra.
    ///
    /// Vì sao phải có, và vì sao đúng MỘT thợ (2026-08-14): đường đọc
    /// `getUpdates` là đường DUY NHẤT hub nghe được điện thoại (luật 1 của tệp
    /// này), nên mọi giây tiêu ở đó là một giây điếc. Mà `handle_update` có
    /// nhánh tiêu giây thật: tải một tệp đính kèm mất **3,8s và 5,0s** trong
    /// hai lượt đo được của ngày 08-14 (`telegram_update_slow`), chưa kể
    /// `sendMessage` 0,9–2,9s cho mỗi câu trả lời lỗi.
    ///
    /// Một thợ, không phải nhiều: hai update chạy song song là hai bàn tay cùng
    /// gõ vào một Terminal, và thứ tự "Esc xoá ô → gõ chữ" (nút ⏎ Gửi) sẽ đảo.
    /// Nối đuôi là ĐÚNG; nằm trong vòng đọc mới là sai.
    inflight: Arc<Inflight>,
    /// Không dựng được luồng thợ ⟹ vòng đọc tự xử lý. Chậm còn hơn điếc, và
    /// KHÔNG im lặng: `telegram_worker_inline` nói ra vì sao nó chậm.
    inline: Arc<AtomicBool>,
    token: String,
    chat_id: String,
    /// Cấu hình, để `push_text` gọi được `pipeline::run_telegram_now`.
    ///
    /// `Arc` vì `Inbox` bị `clone()` cho luồng đọc và nằm trong một `OnceLock`
    /// toàn cục — chép cả `Config` mỗi lượt bấm nút là chép một cây cấu hình
    /// cho một việc chỉ cần đọc.
    cfg: std::sync::Arc<Config>,
    /// Đánh thức vòng chạy khi có lệnh mới.
    ///
    /// 🔴 Đo 2026-08-11: gõ `/help` lúc 21:31:34, hub chạy nó lúc **21:33:50**
    /// — **2 phút 16 giây**. `execute_telegram_commands` đứng đầu `run_once`,
    /// mà vòng ngủ 120 giây, nên lệnh tới ngay sau lúc vòng vừa đọc hàng đợi
    /// thì nằm chờ trọn một vòng. Phòng chat tfl5 không bị vì socket
    /// `/ws/chat` gọi `wake()`; kênh này lúc đầu thì không có gì gọi.
    /// Một lệnh gõ tay mà đợi hai phút thì người ta gõ lại lần nữa — và lần
    /// thứ hai là một mệnh lệnh THẬT chạy hai lần.
    waker: Option<Arc<crate::live::Waker>>,
}

/// Một cú bấm nút → **đúng dòng lệnh mà ngón tay sẽ gõ**, không phải một nhánh
/// xử lý riêng.
///
/// Đây là chỗ giữ cho Telegram không mọc ra một bộ động từ thứ hai: nút chỉ là
/// cái phím tắt của một ROUTE đã có, nên nó đi vào cùng hàng đợi, cùng
/// `parse_command`, cùng handler, và để lại cùng một vết trong sổ. Hàm thuần —
/// kiểm được mà không cần Telegram.
///
/// * `key:<session_id>:<n>` → `/key <id> <n>` — trả lời hộp chọn của một phiên.
/// * `sess:<session_id>` → `/session <id>` — chọn phiên để theo từ danh sách.
///
/// `ok:`/`no:` (nút xác nhận) KHÔNG thuộc về đây: chúng chỉ có nghĩa trong lúc
/// `confirm::ask` đang chờ, và trả `None` là cách nói "cái này không phải lệnh".
/// Sổ những tin hub đã gửi sang Telegram, để sau này XOÁ được chúng.
///
/// Hà 2026-08-12: *"đã có cơ chế tự xóa tin nhắn cũ hơn 1.5 ngày chưa"* — chưa.
/// Muốn xoá thì phải có `message_id`, mà `message_id` chỉ tồn tại đúng một lần:
/// trong câu trả lời của `sendMessage`. Không nhặt ngay lúc ấy thì tin đã gửi
/// nằm ngoài tầm với vĩnh viễn.
///
/// Ghi thẳng vào sổ (một cursor trong SQLite) chứ không giữ trong bộ nhớ: hub
/// khởi động lại vài lần một ngày, và một bộ đệm trong RAM nghĩa là mỗi lần
/// khởi động lại là một nhúm tin không bao giờ xoá được nữa — đúng loại lỗ hổng
/// im lặng mà một tính năng dọn dẹp không được phép có.
pub const SENT_KEY: &str = "telegram:sent";

/// `message_id` + lúc gửi (epoch giây).
pub type SentMsg = (i64, i64);

/// Nhặt `message_id` từ câu trả lời của `sendMessage` và ghi vào sổ.
///
/// Không có `db` ở đây thì mở lấy một cái: hai đường gửi (`Inbox` và
/// `confirm::tell`) đều không cầm sổ, mà bắt cả hai mang thêm một tham số chỉ
/// để ghi một dòng là bắt mọi chỗ gọi mang theo chi tiết của kênh Telegram.
pub fn remember_sent(cfg: &Config, resp: &Value) {
    let Some(id) = resp
        .get("result")
        .and_then(|r| r.get("message_id"))
        .and_then(Value::as_i64)
    else {
        // Gửi được mà không đọc ra id thì nói thẳng: tin ấy sẽ nằm lại mãi.
        logging::warn("telegram_sent_no_id", json!({}));
        return;
    };
    let now = chrono::Utc::now().timestamp();
    let db = match crate::db::Db::open(&cfg.db) {
        Ok(d) => d,
        Err(e) => {
            logging::error("telegram_sent_db_failed", json!({ "err": e.to_string() }));
            return;
        }
    };
    let mut list: Vec<SentMsg> = db
        .cursor_or_log(SENT_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();
    list.push((id, now));
    // Trần: sổ này chỉ để xoá, mà thứ quá 48 giờ thì không xoá được nữa — giữ
    // vài nghìn dòng là giữ rác.
    if list.len() > 2000 {
        let cut = list.len() - 2000;
        list.drain(..cut);
    }
    match serde_json::to_string(&list) {
        Ok(v) => {
            if let Err(e) = db.set_cursor(SENT_KEY, &v) {
                logging::error("telegram_sent_not_saved", json!({ "err": e.to_string() }));
            }
        }
        Err(e) => logging::error("telegram_sent_not_encodable", json!({ "err": e.to_string() })),
    }
}

/// Trần CỨNG của Telegram: quá 48 giờ thì bot không xoá được tin của chính nó.
///
/// Không phải một lựa chọn của hub — API trả `message can't be deleted`. Nên
/// mọi tin quá hạn này bị **bỏ khỏi sổ** kèm một dòng log, chứ không nằm lại
/// làm hub thử đi thử lại một việc không bao giờ xong.
pub const TELEGRAM_DELETE_WINDOW_SEC: i64 = 48 * 3600;

/// Telegram trả lời nhưng TỪ CHỐI — trả về lý do, `None` nếu bình thường.
///
/// 🔴 Lỗi im lặng bắt được 2026-08-12, đúng lúc Hà đổi bot: token sai thì
/// `getUpdates` vẫn trả **JSON hợp lệ** (`{"ok":false,"description":"Unauthorized"}`),
/// nên `r.json()` THÀNH CÔNG, `result` rỗng, vòng lặp coi như "không có tin
/// nào" và quay lại hỏi ngay — không một dòng log, mà `timeout=20` cũng mất tác
/// dụng vì server đáp tức thì. Kênh chết câm, và bên ngoài nhìn y hệt một buổi
/// chiều không ai nhắn gì.
pub fn poll_rejected(resp: &Value) -> Option<String> {
    if resp.get("ok").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    Some(
        resp.get("description")
            .and_then(Value::as_str)
            .unwrap_or("Telegram từ chối getUpdates (không có mô tả)")
            .to_string(),
    )
}

/// Chia sổ thành ba nhóm — hàm THUẦN, kiểm được mà không cần Telegram.
///
/// Trả về `(đến_hạn_xoá, quá_48h_không_xoá_được)`. Phần quyết định của tính năng
/// này nằm gọn ở đây, nên nó phải kiểm được bằng một cái đồng hồ giả.
pub fn due_for_delete(list: &[SentMsg], now: i64, after_hours: u64) -> (Vec<i64>, Vec<i64>) {
    if after_hours == 0 {
        return (Vec::new(), Vec::new());
    }
    let limit = (after_hours as i64) * 3600;
    let mut due = Vec::new();
    let mut gone = Vec::new();
    for (id, ts) in list {
        let age = now - ts;
        if age >= TELEGRAM_DELETE_WINDOW_SEC {
            gone.push(*id);
        } else if age >= limit {
            due.push(*id);
        }
    }
    (due, gone)
}

/// Xoá những tin hub gửi đã quá hạn — chạy mỗi vòng, rẻ khi không có gì để xoá.
///
/// Hai điều phải nói thẳng vì chúng là giới hạn thật, không phải thiếu sót:
/// * **Chỉ xoá được tin của CHÍNH BOT.** Trong buồng chat riêng, Telegram không
///   cho bot xoá tin của người dùng — câu Hà gõ sẽ nằm lại.
/// * **Chỉ trong 48 giờ.** Tin cũ hơn thì vĩnh viễn không xoá được bằng bot.
pub fn prune_sent(cfg: &Config, db: &crate::db::Db) {
    let hours = cfg.confirm.delete_after_hours;
    if hours == 0 {
        return;
    }
    let Some(inbox) = inbox() else {
        return; // không có kênh Telegram thì không có gì để dọn
    };
    let mut list: Vec<SentMsg> = match db
        .cursor_or_log(SENT_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
    {
        Some(v) => v,
        None => return,
    };
    let now = chrono::Utc::now().timestamp();
    let (due, gone) = due_for_delete(&list, now, hours);
    if due.is_empty() && gone.is_empty() {
        return;
    }
    let (mut deleted, mut failed) = (0usize, 0usize);
    let too_old = gone.len();
    let mut drop_ids: Vec<i64> = gone;
    for id in due {
        match inbox.delete_message(id) {
            Ok(()) => {
                deleted += 1;
                drop_ids.push(id);
            }
            // Telegram từ chối vĩnh viễn (đã xoá tay, hết cửa 48h…) thì bỏ khỏi
            // sổ luôn; lỗi mạng thì GIỮ lại để vòng sau thử tiếp.
            Err(e) if e.contains("can't be deleted") || e.contains("not found") => {
                drop_ids.push(id);
                failed += 1;
            }
            Err(_) => failed += 1,
        }
    }
    list.retain(|(id, _)| !drop_ids.contains(id));
    match serde_json::to_string(&list) {
        Ok(v) => {
            if let Err(e) = db.set_cursor(SENT_KEY, &v) {
                logging::error("telegram_prune_not_saved", json!({ "err": e.to_string() }));
            }
        }
        Err(e) => logging::error("telegram_prune_not_encodable", json!({ "err": e.to_string() })),
    }
    logging::info(
        "telegram_pruned",
        json!({ "deleted": deleted, "too_old_to_delete": too_old, "failed": failed,
                "left": list.len(), "after_hours": hours }),
    );
}

/// Bảng nút cho một phiên đang dừng lại hỏi — hàm THUẦN, kiểm được không cần mạng.
///
/// Tách khỏi `Inbox::ask_choices` vì phần đáng sai là ở đây: mã `callback_data`
/// phải khớp đúng thứ `callback_to_command` giải ra, và cả hai đều phải nằm
/// dưới trần 64 byte của Telegram.
pub fn choice_buttons(
    session_id: &str,
    labels: &[String],
    offer_enter: bool,
    multi: bool,
    rest: &[crate::sessions::Question],
) -> Vec<(String, String)> {
    // 🔴 Hà 2026-08-13: *"có nhiều option thì phải có cơ chế chọn được nhiều"*.
    // Bảng nhiều câu chỉ gửi đi được khi KHÔNG còn ô trống, nên một bộ nút chỉ
    // phục vụ câu đầu là một bộ nút dẫn vào ngõ cụt: bấm xong, bảng vẫn đứng,
    // và trên điện thoại nó trông y hệt lúc hỏng.
    //
    // Nút của bảng nhiều câu mang theo SỐ CÂU (`pick:<id>:<câu>.<lựa chọn>`),
    // vì "câu đang mở" không phải thứ điện thoại biết — và cũng không nên là
    // thứ nó phải biết.
    let table = !rest.is_empty();
    let mut buttons: Vec<(String, String)> = labels
        .iter()
        .enumerate()
        .take(9)
        .map(|(i, l)| {
            // Chọn nhiều thì mỗi nút là một cái CÔNG TẮC, không phải một lá
            // phiếu — nhãn phải nói đúng thế, vì bấm xong màn hình điện thoại
            // không đổi gì và người ta sẽ tưởng nút hỏng.
            let head = if multi {
                format!("☐ {}", i + 1)
            } else if table {
                format!("1▸{}", i + 1)
            } else {
                format!("{}.", i + 1)
            };
            let data = if table {
                format!("pick:{}:1.{}", session_id, i + 1)
            } else {
                format!("key:{}:{}", session_id, i + 1)
            };
            (format!("{head} {}", crate::exec::truncate(l, 60)), data)
        })
        .collect();
    // Các câu sau: cùng khuôn, chỉ đổi số câu. Nhãn mang số câu ở đầu (`2▸1`)
    // để đọc được trong một liếc trên màn 390px — thứ tự đúng bằng thứ tự tab.
    for (qi, q) in rest.iter().enumerate() {
        for (oi, l) in q.options.iter().take(9).enumerate() {
            buttons.push((
                format!("{}▸{} {}", qi + 2, oi + 1, crate::exec::truncate(l, 60)),
                format!("pick:{}:{}.{}", session_id, qi + 2, oi + 1),
            ));
        }
    }
    // 🔴 Hà 2026-08-13, ảnh chụp hộp hỏi của `[codetrail]`: *"option này chọn
    // nhiều chứ không phải chọn 1"*. Với hộp chọn-nhiều, bấm một số chỉ BẬT/TẮT
    // một mục — phiên vẫn đứng đợi dấu Enter. Không có cái nút này thì bấm bao
    // nhiêu cái cũng không xong việc, mà nhìn từ điện thoại y hệt lúc hỏng.
    //
    // Đi bằng đúng route `/key` sẵn có (`key:<id>:enter` → `/key <id> enter`),
    // không đẻ lối riêng cho Telegram.
    // Bảng nhiều câu cũng cần cái nút này, và cần hơn: trả lời đủ mọi ô rồi
    // bảng VẪN đứng đó chờ một dấu Enter (ảnh Hà gửi: `✔ Submit` là một tab
    // riêng). Không có nút thì mọi cú bấm phía trên dẫn tới một bảng đầy đủ mà
    // không ai gửi được — đúng cái ngõ cụt đang vá.
    if multi || table {
        buttons.push((
            "✅ Gửi lựa chọn".to_string(),
            format!("key:{session_id}:enter"),
        ));
    }
    if offer_enter {
        buttons.push(("👁 Vào phiên này".to_string(), format!("sess:{session_id}")));
    }
    buttons
}

/// Gột lớp trang trí Markdown trước khi chữ ra Telegram.
///
/// 🔴 Hà 2026-08-13, gửi lại ảnh chụp chính tin của tôi: *"lệnh ở nội dung bị
/// cắt mất mã"*. Trên ảnh, `**Thử được rồi**` hiện nguyên hai cặp sao,
/// rào khối hiện nguyên ba dấu nháy ngược, và một dòng lệnh nằm trong nháy
/// ngược thì bị mấy ký tự ấy cắt vụn ngay giữa.
///
/// Gốc: hub gửi `sendMessage` **không kèm `parse_mode`**, tức chữ thuần. Mà
/// nguồn chữ là báo cáo của một phiên `claude` — thứ viết bằng Markdown theo
/// bản năng. Hai bên đều đúng phần mình, và người đọc lãnh đủ.
///
/// Vì sao gột chứ không bật `parse_mode`: MarkdownV2 của Telegram đòi thoát
/// **mười tám** ký tự (`_ * [ ] ( ) ~ > # + - = | { } . !`), và một dấu chấm
/// không thoát làm **cả tin bị từ chối** — tức lỗi hiển thị nhỏ đổi thành lỗi
/// mất tin. Với một kênh mang lệnh vận hành thì đó là đổi sai chiều.
///
/// Chỉ gột những gì CHẮC CHẮN là trang trí, và **giữ nguyên nội dung**:
/// dòng rào ba-nháy-ngược biến mất còn các dòng bên trong ở lại (đó thường là
/// chỗ chứa lệnh), cặp `**` và nháy ngược bị bóc. Không đụng tới `_` hay `*`
/// lẻ — chúng xuất hiện trong tên tệp và đường dẫn thật.
pub fn strip_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let t = line.trim_start();
        // Rào khối code: bỏ hẳn dòng ấy, giữ nội dung bên trong.
        if t.starts_with("```") && t.trim_end().len() <= 12 {
            continue;
        }
        let mut s = line.to_string();
        if s.contains("**") {
            s = s.replace("**", "");
        }
        if s.contains('`') {
            s = s.replace('`', "");
        }
        out.push_str(&s);
        out.push('\n');
    }
    // `lines()` bỏ dấu xuống dòng cuối; trả lại đúng hình dạng cũ.
    if !text.ends_with('\n') {
        out.pop();
    }
    out
}

/// `~/x` → `/Users/…/x`. Chữ trên màn hay viết gọn bằng dấu ngã, mà
/// `Path::canonicalize` không hiểu dấu ấy — nó sẽ đi tìm một thư mục tên `~`.
fn shellexpand_home(p: &str) -> String {
    match p.strip_prefix("~/") {
        Some(rest) => match std::env::var("HOME") {
            Ok(h) => format!("{h}/{rest}"),
            Err(_) => p.to_string(),
        },
        None => p.to_string(),
    }
}

pub fn callback_to_command(data: &str) -> Option<String> {
    if let Some(rest) = data.strip_prefix("key:") {
        let (sid, n) = rest.split_once(':')?;
        if sid.is_empty() || n.is_empty() {
            return None;
        }
        return Some(format!("/key {sid} {n}"));
    }
    // `pick:<id>:<câu>.<lựa chọn>` — một cái nút của bảng hỏi NHIỀU CÂU. Nút
    // `key:` cũ chỉ mang được số lựa chọn, tức nó mặc định "câu đang mở là câu
    // người ta muốn trả lời" — đúng với bảng một câu, và sai với bảng nhiều câu
    // đúng vào lúc chuyện đó quan trọng nhất (xem `pipeline` route `Pick`).
    if let Some(rest) = data.strip_prefix("pick:") {
        let (sid, at) = rest.split_once(':')?;
        if sid.is_empty() || at.is_empty() {
            return None;
        }
        return Some(format!("/pick {sid} {at}"));
    }
    // Nút "dựng lại hub". Không mang tham số nào: route `/upgrade` dựng từ MÃ
    // HIỆN TẠI trong cây nguồn, nên một dòng lệnh đi kèm chỉ tạo ảo giác là
    // bấm cái này chạy đúng chữ đang hiện trên màn.
    if data == "upgrade" {
        return Some("/upgrade".to_string());
    }
    if let Some(sid) = data.strip_prefix("sess:") {
        if sid.is_empty() {
            return None;
        }
        return Some(format!("/session {sid}"));
    }
    None
}

/// Hòm thư của tiến trình này.
///
/// Một biến toàn cục, và có lý do: `confirm::ask` bị gọi từ giữa lòng
/// `execute_commands`, còn vòng đọc thì sinh ra ở `main` — luồn một tham số qua
/// mười tầng gọi chỉ để hai chỗ ấy dùng chung một `offset` là làm cho mọi chữ ký
/// hàm mang theo một chi tiết của kênh Telegram. Daemon chỉ có một tiến trình,
/// nên đây là "một cái duy nhất" theo đúng nghĩa đen.
static INBOX: OnceLock<Inbox> = OnceLock::new();

/// Tên công khai của bot, đọc từ `getMe` lúc khởi động.
///
/// 🔴 Hà 2026-08-14: *"thêm 1 cái icon để bấm chạy bên trong text chỗ cuối dòng
/// lệnh"* · *"chứ ko phải đi thay icon"*. Một icon nằm GIỮA CHỮ thì không thể
/// là nút — bàn phím Telegram luôn treo dưới đáy tin. Thứ đặt được vào giữa chữ
/// là một LIÊN KẾT, và liên kết chạy được lệnh chỉ có một dạng: deep link về
/// chính bot, `https://t.me/<bot>?start=<payload>`. Nên cái tên này là điều
/// kiện cần của cả tính năng.
static BOT_USERNAME: OnceLock<String> = OnceLock::new();

/// `<a href="…">▶️</a>` — icon bấm được, đặt ngay sau dòng lệnh.
///
/// `None` khi chưa biết tên bot (chưa kịp `getMe`, hoặc mạng hỏng): chỗ gọi
/// phải rơi về chữ thường, KHÔNG bịa ra một liên kết không bấm được.
///
/// Payload theo đúng luật Telegram: chỉ `A-Za-z0-9_-`, tối đa 64 ký tự. Trùng
/// đúng bộ ký tự mà tên lệnh cho phép, nên `run_0` hay `pick_4963b95c_2_1` đi
/// qua cả hai đường mà không phải mã hoá gì thêm.
pub fn deep_link(payload: &str) -> Option<String> {
    let bot = BOT_USERNAME.get()?;
    if payload.is_empty()
        || payload.len() > 64
        || !payload
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some(format!("https://t.me/{bot}?start={payload}"))
}

/// Chữ thường → chữ an toàn cho `parse_mode=HTML`.
///
/// Telegram chỉ đòi ba ký tự này (tài liệu Bot API, mục *HTML style*) — nhẹ hơn
/// hẳn MarkdownV2, thứ bắt escape MỌI ký tự mã 1–126. Đó cũng là lý do hub gột
/// Markdown suốt từ đầu thay vì bật nó lên.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn inbox() -> Option<&'static Inbox> {
    INBOX.get()
}

impl Inbox {
    /// Dựng hòm thư và chạy vòng đọc nền. `None` khi thiếu bí mật — SKIP-CÓ-LOG,
    /// không phải lỗi (luật 4 của dự án: thiếu khoá thì bỏ qua và nói ra).
    pub fn start(cfg: &Config, waker: Option<Arc<crate::live::Waker>>) -> Option<Inbox> {
        if !cfg.confirm.enabled {
            logging::info(
                "telegram_inbox_off",
                json!({ "why": "confirm.enabled = false" }),
            );
            return None;
        }
        let (token, chat_id) = match (
            crate::config::secret_from_env(&cfg.confirm.bot_token_env),
            crate::config::secret_from_env(&cfg.confirm.chat_id_env),
        ) {
            (Some(t), Some(c)) => (t, c),
            (t, c) => {
                let missing: Vec<&str> = [
                    (t.is_none()).then_some(cfg.confirm.bot_token_env.as_str()),
                    (c.is_none()).then_some(cfg.confirm.chat_id_env.as_str()),
                ]
                .into_iter()
                .flatten()
                .collect();
                logging::warn("telegram_inbox_no_secret", json!({ "keys": missing }));
                return None;
            }
        };
        let inbox = Inbox {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            offset: Arc::new(Mutex::new(0)),
            busy: Arc::new(AtomicBool::new(false)),
            inflight: Arc::new((Mutex::new(VecDeque::new()), Condvar::new())),
            inline: Arc::new(AtomicBool::new(false)),
            token,
            chat_id,
            cfg: std::sync::Arc::new(cfg.clone()),
            waker,
        };
        let _ = INBOX.set(inbox.clone());
        // Luồng THỢ trước, vòng đọc sau: dựng ngược thì có một khoảnh khắc
        // update đã tới mà chưa ai nhặt, và nó nằm im tới update kế tiếp.
        let hand = inbox.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("telegram-work".into())
            .spawn(move || hand.work_forever())
        {
            // Không nuốt: mất thợ thì mọi update chạy ngay trong vòng đọc, tức
            // hub điếc đúng bằng thời gian xử lý. Nói ra, rồi vẫn chạy.
            inbox.inline.store(true, Ordering::SeqCst);
            logging::error(
                "telegram_worker_spawn_failed",
                json!({ "err": e.to_string(),
                        "fallback": "vòng đọc tự xử lý — chậm hơn, không mất lệnh" }),
            );
        }
        let worker = inbox.clone();
        std::thread::Builder::new()
            .name("telegram-inbox".into())
            .spawn(move || worker.read_forever())
            .map_err(|e| {
                logging::error("telegram_inbox_spawn_failed", json!({ "err": e.to_string() }));
            })
            .ok()?;
        logging::info("telegram_inbox_started", json!({ "chat_id_env": cfg.confirm.chat_id_env }));
        // Khai bộ lệnh ở nền: một lượt HTTP, và hỏng thì chỉ mất phần gợi ý —
        // không được phép cản buồng thư khởi động. Nhưng KHÔNG im lặng.
        let reg = inbox.clone();
        std::thread::Builder::new()
            .name("telegram-setcommands".into())
            .spawn(move || {
                if let Err(e) = reg.register_commands() {
                    logging::warn("telegram_commands_register_failed", json!({ "err": e }));
                }
            })
            .map_err(|e| {
                logging::warn("telegram_commands_spawn_failed", json!({ "err": e.to_string() }));
            })
            .ok();
        Some(inbox)
    }

    /// Tải một tệp đính kèm về máy, rồi NÓI cho phiên biết nó nằm đâu.
    ///
    /// 🔴 Hà 2026-08-13: *"thêm cơ chế nhận đính kèm file vào tin nhắn"*. Cây
    /// cầu vốn một chiều ở chỗ này: chữ thì gõ vào phiên được, còn một cái ảnh
    /// chụp lỗi hay một tệp log thì phải ngồi vào máy mới đưa vào được.
    ///
    /// Ba luật, mỗi luật vá một đường hỏng đã biết:
    /// - **tên tệp bị bóc sạch phần thư mục**: `file_name` là chuỗi do NGƯỜI
    ///   GỬI đặt, và một cái tên chứa `..` cùng dấu gạch chéo vẫn hợp lệ với
    ///   Telegram — ghi thẳng nó xuống là để người gửi chọn chỗ ghi;
    /// - **về đúng dự án của phiên đang theo**, không đổ hết vào một chỗ chung:
    ///   phiên đọc tệp bằng đường dẫn tương đối là chuyện thường;
    /// - **hỏng thì NÓI**, đừng im: một tệp gửi đi mà không thấy hồi âm nào là
    ///   thứ khiến người ta gửi lại lần nữa.
    fn take_file(&self, file_id: &str, name: &str, caption: Option<&str>) {
        let client = match self.client() {
            Some(c) => c,
            None => return,
        };
        // 1. Hỏi Telegram đường dẫn tạm của tệp.
        let path = client
            .post(self.api("getFile"))
            .json(&json!({ "file_id": file_id }))
            .send()
            .ok()
            .and_then(|r| r.json::<Value>().ok())
            .and_then(|v| {
                v.pointer("/result/file_path")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
        let Some(path) = path else {
            logging::error("telegram_getfile_failed", json!({ "name": name }));
            let _ = self.send_text("\u{26a0} không hỏi được Telegram đường dẫn của tệp ấy.");
            return;
        };
        // 2. Tải về.
        let url = format!("https://api.telegram.org/file/bot{}/{}", self.token, path);
        let bytes = client
            .get(&url)
            .send()
            .ok()
            .filter(|r| r.status().is_success())
            .and_then(|r| r.bytes().ok());
        let Some(bytes) = bytes else {
            logging::error("telegram_file_download_failed", json!({ "name": name }));
            let _ = self.send_text("\u{26a0} tải tệp về không được (Telegram chỉ cho bot tải tệp ≤ 20 MB).");
            return;
        };
        // 3. Chỗ để: thư mục dự án của phiên đang theo, `.inbox/`.
        let db = crate::db::Db::open(&self.cfg.db).ok();
        let focus = db
            .as_ref()
            .and_then(|db| db.cursor_or_log(crate::pipeline::FOCUS_SESSION_KEY))
            .unwrap_or_default();
        // MỘT hòm thư ở gốc workspace, chia theo mã phiên.
        //
        // Hà 2026-08-13: *"`.inbox` đã lưu theo phiên rồi thì cần gì tách theo
        // thư mục dự án nữa → chuyển ra thư mục gốc để dùng chung"*. Đúng: id
        // phiên đã là duy nhất, thêm một tầng dự án chỉ làm chỗ dọn rác nằm rải
        // ra nhiều nơi — mà dọn rác là toàn bộ lý do chia theo phiên.
        let dir = self.cfg.workspace_root.clone();
        // Xếp theo MÃ PHIÊN (Hà 2026-08-13: *"`.inbox` nên đưa vào theo mã phiên
        // cho dễ dọn rác"*). Tệp gửi cho một phiên chỉ có nghĩa trong đời phiên
        // ấy; đổ chung một chỗ thì sau một tuần không ai biết cái nào còn dùng.
        // Id NGẮN (8 ký tự) — đúng thứ danh sách phiên và `claude stop` đang
        // dùng, nên nhìn thư mục là ghép được với dòng trên màn.
        let short = focus.split('-').next().unwrap_or("").to_string();
        let dir = dir.join(".inbox");
        let dir = if short.is_empty() {
            dir
        } else {
            dir.join(&short)
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            logging::error("telegram_inbox_mkdir_failed", json!({ "err": e.to_string() }));
            let _ = self.send_text(&format!("\u{26a0} không tạo được thư mục nhận tệp: {e}"));
            return;
        }
        // Tên tệp do NGƯỜI GỬI đặt ⟹ chỉ giữ phần tên cuối cùng.
        let safe = std::path::Path::new(name)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty() && *n != "." && *n != "..")
            .unwrap_or("tep-nhan-duoc");
        let dest = dir.join(safe);
        if let Err(e) = std::fs::write(&dest, &bytes) {
            logging::error("telegram_file_write_failed", json!({ "err": e.to_string() }));
            let _ = self.send_text(&format!("\u{26a0} không ghi được tệp: {e}"));
            return;
        }
        logging::info(
            "telegram_file_received",
            json!({ "name": safe, "bytes": bytes.len(), "dir": dir.display().to_string() }),
        );
        let _ = self.send_text(&format!(
            "\u{1f4ce} đã lưu {} ({} KB)",
            dest.display(),
            bytes.len() / 1024
        ));
        // 4. Nói cho phiên biết — đi đúng đường chữ thường đã có.
        let line = match caption.map(str::trim).filter(|c| !c.is_empty()) {
            Some(c) => format!("Tôi vừa gửi một tệp: {} — {}", dest.display(), c),
            None => format!("Tôi vừa gửi một tệp: {}", dest.display()),
        };
        self.push_text(&line);
    }

    fn api(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.token, method)
    }

    /// Khai bộ lệnh với Telegram — gõ `/` là hiện gợi ý, và menu ☰ có nội dung.
    ///
    /// 🔴 Hà 2026-08-14: *"Tại sao không tạo lib lệnh để map khi nhận"*. Câu hỏi
    /// ấy lôi ra một thứ còn tệ hơn ba-nguồn-sự-thật: `setMyCommands` **chưa
    /// từng được gọi**. Cả một tầng giao diện Telegram cho sẵn — danh sách gợi
    /// ý khi gõ `/`, menu ☰ — bỏ không từ đầu, trong khi chủ máy vẫn phải nhớ
    /// tên lệnh trong đầu hoặc mở `/help` ra đọc.
    ///
    /// Danh sách lấy từ `commands::for_telegram()`, tức cùng cái bảng sinh ra
    /// `/help`. Gọi mỗi lần khởi động: rẻ (một lượt HTTP), và nó tự sửa khi
    /// bảng đổi — không có bước "nhớ chạy tay" nào để mà quên.
    pub fn register_commands(&self) -> Result<(), String> {
        let client = self.client().ok_or("không dựng được HTTP client")?;
        let list: Vec<Value> = crate::commands::for_telegram()
            .into_iter()
            .map(|(c, d)| json!({ "command": c, "description": d }))
            .collect();
        let n = list.len();
        let r = client
            .post(self.api("setMyCommands"))
            .json(&json!({ "commands": list }))
            .send()
            .map_err(|e| e.to_string())?;
        let v: Value = r.json().unwrap_or_else(|_| json!({}));
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            logging::info("telegram_commands_registered", json!({ "count": n }));
            Ok(())
        } else {
            Err(v
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("Telegram từ chối setMyCommands")
                .to_string())
        }
    }
}

/// `hub doctor` cho KÊNH CHÍNH — dò thật, không đọc cấu hình rồi đoán.
///
/// 🔴 Hà 2026-08-13: *"nội dung readme.md chưa đúng lắm, bây giờ chủ yếu là kênh
/// telegram"*. Sửa README thì lòi ra chỗ này: `doctor` in mục `channels:` mà
/// trong đó **chỉ có tfl5** — kênh người ta dùng hằng ngày không có một dòng
/// nào. Người mới kéo repo về, làm đúng theo README, vẫn không có cách nào biết
/// bot đã nối được hay chưa; họ sẽ biết bằng cách gõ một lệnh rồi ngồi chờ mãi.
///
/// `getMe` chứ không phải `sendMessage`: một phép dò không được đẻ ra tin nhắn.
/// Và **không bao giờ in token** — chỉ tên biến (luật 4), tên bot, id buồng.
pub fn health(cfg: &Config) -> crate::adapters::Health {
    if !cfg.confirm.enabled {
        return crate::adapters::Health {
            ok: false,
            detail: "tắt (confirm.enabled = false)".into(),
        };
    }
    let (token, chat_id) = match (
        crate::config::secret_from_env(&cfg.confirm.bot_token_env),
        crate::config::secret_from_env(&cfg.confirm.chat_id_env),
    ) {
        (Some(t), Some(c)) => (t, c),
        (t, c) => {
            // Thiếu khoá là SKIP-CÓ-LOG chứ không phải hỏng — nhưng ở đây phải
            // NÓI RA khoá nào thiếu, vì đó chính là việc người đọc phải làm tiếp.
            let missing: Vec<&str> = [
                (t.is_none()).then_some(cfg.confirm.bot_token_env.as_str()),
                (c.is_none()).then_some(cfg.confirm.chat_id_env.as_str()),
            ]
            .into_iter()
            .flatten()
            .collect();
            return crate::adapters::Health {
                ok: false,
                detail: format!("thiếu bí mật trong hub.env: {}", missing.join(", ")),
            };
        }
    };
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return crate::adapters::Health {
                ok: false,
                detail: format!("không dựng nổi HTTP client: {e}"),
            }
        }
    };
    let url = format!("https://api.telegram.org/bot{token}/getMe");
    let resp = match client.get(&url).send().and_then(|r| r.json::<Value>()) {
        Ok(v) => v,
        Err(e) => {
            // Câu lỗi của reqwest có thể mang nguyên URL, tức mang nguyên token.
            let msg = e.to_string().replace(&token, "<token>");
            return crate::adapters::Health {
                ok: false,
                detail: format!("gọi getMe hỏng: {msg}"),
            };
        }
    };
    // `getMe` trả HTTP 200 kèm `{"ok":false}` khi token sai — cùng cái bẫy
    // `poll_rejected` đã ghi: đọc mã HTTP là đọc nhầm chỗ.
    if resp.get("ok").and_then(Value::as_bool) != Some(true) {
        let why = resp
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("Telegram từ chối getMe (không có mô tả)");
        return crate::adapters::Health {
            ok: false,
            detail: format!("token không dùng được: {why}"),
        };
    }
    let who = resp
        .get("result")
        .and_then(|r| r.get("username"))
        .and_then(Value::as_str)
        .unwrap_or("(không tên)");
    crate::adapters::Health {
        ok: true,
        detail: format!(
            "@{who} · buồng {chat_id} · khoá từ {} + {}",
            cfg.confirm.bot_token_env, cfg.confirm.chat_id_env
        ),
    }
}

impl Inbox {
    /// Gửi MỘT file ra Telegram để đọc thẳng trên điện thoại.
    ///
    /// 🔴 Hà 2026-08-13: *"các nội dung có path file thì nên cho click vào nhận
    /// được file để mở trực tiếp trên tele"*. Trước đó cây cầu tệp đi một
    /// chiều: hub nhận được tệp từ Telegram nhưng không gửi ra được cái nào,
    /// nên mọi báo cáo nhắc tới một file đều nhắc tới thứ không mở nổi.
    ///
    /// Ba cửa, và cả ba đều là luật đã có chứ không phải luật mới:
    /// * **Phải nằm trong cây làm việc** — đường dẫn tới từ chữ trên màn, mà
    ///   chữ trên màn thì ai viết cũng được. `/etc/passwd` là một đường dẫn hợp
    ///   lệ về mặt hình dạng.
    /// * **Phải qua `preview_risk`** (luật 5): thứ gì rời khỏi máy này đều bị
    ///   soi, y như phần xem trước của phiên. Đây cũng là lý do chỉ gửi file
    ///   CHỮ — cổng soi không đọc được file nhị phân, mà gửi cái không soi được
    ///   thì cái cổng chỉ còn là hình thức.
    /// * **Trần dung lượng**: Telegram chặn ở 50 MB, nhưng hub chặn sớm hơn
    ///   nhiều — một file 5 MB đọc trên điện thoại là chuyện không xảy ra.
    pub fn send_document(&self, path: &std::path::Path, root: &std::path::Path) -> Result<(), String> {
        const MAX_BYTES: u64 = 5 * 1024 * 1024;

        let real = path.canonicalize().map_err(|e| format!("không mở được: {e}"))?;
        let root_real = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        if !real.starts_with(&root_real) {
            return Err(format!(
                "nằm ngoài cây làm việc ({}) — hub không gửi",
                root_real.display()
            ));
        }
        let meta = std::fs::metadata(&real).map_err(|e| e.to_string())?;
        if !meta.is_file() {
            return Err("không phải một tệp".into());
        }
        if meta.len() > MAX_BYTES {
            return Err(format!(
                "{:.1} MB — quá trần {} MB",
                meta.len() as f64 / 1_048_576.0,
                MAX_BYTES / 1_048_576
            ));
        }
        // Cổng THẬT nằm ở đây, và nó là một câu hỏi về NỘI DUNG chứ không phải
        // về cái tên: `read_to_string` hỏng nghĩa là file không phải UTF-8,
        // tức cổng quét rò không đọc nổi nó — mà thứ không soi được thì không
        // rời khỏi máy này (luật 5). Danh sách đuôi chỉ để khỏi dựng cái nút
        // chắc chắn hỏng; nó không phải hàng rào.
        let body = std::fs::read_to_string(&real)
            .map_err(|_| "không phải file chữ (cổng quét rò không đọc được) nên hub không gửi".to_string())?;
        // Cân riêng cho TỆP — xem `redaction::file_risk` để biết vì sao không
        // dùng lại cân của phần xem trước.
        let risk = crate::redaction::file_risk(&body);
        if !risk.is_empty() {
            logging::warn(
                "telegram_document_withheld",
                json!({ "path": real.display().to_string(), "risk": risk }),
            );
            return Err(format!("giữ lại: có dấu hiệu bí mật ({})", risk.join(", ")));
        }

        let name = real
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        let client = self.client().ok_or("không dựng được HTTP client")?;
        let part = reqwest::blocking::multipart::Part::bytes(body.into_bytes())
            .file_name(name.clone())
            .mime_str("text/plain; charset=utf-8")
            .map_err(|e| e.to_string())?;
        let form = reqwest::blocking::multipart::Form::new()
            .text("chat_id", self.chat_id.clone())
            .text(
                "caption",
                real.strip_prefix(&root_real)
                    .unwrap_or(&real)
                    .display()
                    .to_string(),
            )
            .part("document", part);
        let r = client
            .post(self.api("sendDocument"))
            .multipart(form)
            .send()
            .map_err(|e| e.to_string())?;
        let v: Value = r.json().unwrap_or_else(|_| json!({}));
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            remember_sent(&self.cfg, &v);
            logging::info(
                "telegram_document_sent",
                json!({ "name": name, "bytes": meta.len() }),
            );
            Ok(())
        } else {
            Err(format!(
                "telegram từ chối: {}",
                v.get("description").and_then(Value::as_str).unwrap_or("không rõ")
            ))
        }
    }

    /// `confirm::ask` mượn đường đọc: vòng nền đứng im cho tới khi trả.
    pub fn hold(&self) -> Hold<'_> {
        self.busy.store(true, Ordering::SeqCst);
        Hold { inbox: self }
    }

    pub fn offset_now(&self) -> i64 {
        *self.offset.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_offset(&self, v: i64) {
        let mut o = self.offset.lock().unwrap_or_else(|e| e.into_inner());
        if v > *o {
            *o = v;
        }
    }

    pub fn chat_id(&self) -> &str {
        &self.chat_id
    }

    /// Đưa một dòng lệnh vào hàng đợi — dùng cả từ `confirm` khi nó nhặt hộ.
    pub fn push_text(&self, text: &str) {
        self.push_inner(text, false, None);
    }

    /// Như `push_text`, nhưng NHỚ tin đã sinh ra nó — xem `Incoming::msg_id`.
    pub fn push_text_from(&self, text: &str, msg_id: Option<i64>) {
        self.push_inner(text, false, msg_id);
    }

    /// Xếp hàng một lệnh PHỤ của chính hub — chạy xong không trả lời gì.
    pub fn push_text_quiet(&self, text: &str) {
        self.push_inner(text, true, None);
    }

    fn push_inner(&self, text: &str, quiet: bool, msg_id: Option<i64>) {
        let t = text.trim();
        if t.is_empty() {
            return;
        }
        self.queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(Incoming { text: t.to_string(), quiet, msg_id });
        logging::info(
            "telegram_command_queued",
            json!({ "head": crate::exec::truncate(t, 40) }),
        );
        // Đánh thức NGAY, đừng để lệnh nằm hết giấc ngủ của vòng (xem `waker`).
        if let Some(w) = &self.waker {
            w.wake();
        }
        // …và đánh thức thôi thì CHƯA đủ: `wake()` cắt được giấc ngủ, không cắt
        // được một vòng đang chạy dở (đo 2026-08-12: một cú bấm nút chờ 26 giây
        // đúng vì thế). Chạy thẳng ở đây, trong một luồng riêng, xếp hàng bằng
        // `pipeline::CMD_LOCK`.
        crate::pipeline::run_telegram_now(&self.cfg);
    }

    /// Còn lệnh nào đang chờ không — để luồng chạy-ngay vét nốt trước khi thoát.
    pub fn has_pending(&self) -> bool {
        !self.queue.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
    }

    /// Lấy hết lệnh đang chờ. Vòng chạy gọi mỗi lượt.
    pub fn drain(&self) -> Vec<Incoming> {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        q.drain(..).collect()
    }

    fn client(&self) -> Option<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(40))
            .build()
            .map_err(|e| logging::error("telegram_client_failed", json!({ "err": e.to_string() })))
            .ok()
    }

    fn read_forever(&self) {
        let Some(client) = self.client() else { return };
        // NÓI RA đang cầm bot nào. Không có dòng này thì câu hỏi "hub đã nhận
        // token mới chưa" không trả lời được từ bên ngoài — đúng chỗ Hà mắc
        // 2026-08-12 khi đổi bot: gõ `/start` mà im, và mọi giả thuyết (token
        // sai · chưa khởi động lại · chat_id lệch) đều nghe hợp lý như nhau.
        // Chỉ tên công khai của bot, không bao giờ token (luật §4).
        match client.get(self.api("getMe")).send().and_then(|r| r.json::<Value>()) {
            Ok(v) => match poll_rejected(&v) {
                None => {
                    // GIỮ LẠI tên bot: deep link (`t.me/<bot>?start=…`) là cách
                    // duy nhất đặt một ICON BẤM ĐƯỢC vào giữa dòng chữ, mà nó
                    // đòi đúng cái tên này. Trước đây tên ấy chỉ được in ra log
                    // rồi vứt.
                    if let Some(u) = v
                        .get("result")
                        .and_then(|r| r.get("username"))
                        .and_then(Value::as_str)
                    {
                        let _ = BOT_USERNAME.set(u.to_string());
                    }
                    logging::info(
                        "telegram_bot_identity",
                        json!({
                            "username": v.get("result").and_then(|r| r.get("username")).and_then(Value::as_str),
                            "id": v.get("result").and_then(|r| r.get("id")).and_then(Value::as_i64),
                        }),
                    )
                }
                Some(why) => logging::error("telegram_bot_unusable", json!({ "why": why })),
            },
            Err(e) => logging::warn(
                "telegram_getme_failed",
                json!({ "err": logging::redact(&e.to_string()) }),
            ),
        }
        // Bắt đầu từ mốc HIỆN TẠI, không đọc lại lịch sử: hub vừa khởi động lại
        // mà chạy luôn mấy lệnh gõ từ hôm qua là một kiểu bất ngờ tệ.
        if let Ok(v) = client
            .get(format!("{}?offset=-1&timeout=0", self.api("getUpdates")))
            .send()
            .and_then(|r| r.json::<Value>())
        {
            if let Some(id) = v
                .get("result")
                .and_then(Value::as_array)
                .and_then(|a| a.last())
                .and_then(|u| u.get("update_id"))
                .and_then(Value::as_i64)
            {
                self.set_offset(id + 1);
            }
        }
        loop {
            if self.busy.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(400));
                continue;
            }
            let url = format!(
                "{}?offset={}&timeout=20",
                self.api("getUpdates"),
                self.offset_now()
            );
            let resp = match client.get(&url).send().and_then(|r| r.json::<Value>()) {
                Ok(v) => v,
                Err(e) => {
                    // Mạng chập là chuyện thường; im lặng thử lại thì không ai
                    // biết kênh đang chết. Log rồi lùi một nhịp.
                    logging::warn(
                        "telegram_poll_failed",
                        json!({ "err": logging::redact(&e.to_string()), "source": logging::redact(&format!("{:?}", std::error::Error::source(&e).map(|s| s.to_string()))) }),
                    );
                    std::thread::sleep(Duration::from_secs(3));
                    continue;
                }
            };
            // Trả lời được KHÔNG có nghĩa là trả lời thuận (xem `poll_rejected`).
            if let Some(why) = poll_rejected(&resp) {
                logging::error("telegram_poll_rejected", json!({ "why": why }));
                // Lùi hẳn một nhịp: token sai thì hỏi lại sau 3 giây là gõ cửa
                // Telegram 1200 lần một giờ để nghe cùng một câu từ chối.
                std::thread::sleep(Duration::from_secs(30));
                continue;
            }
            let empty = vec![];
            let updates = resp
                .get("result")
                .and_then(Value::as_array)
                .unwrap_or(&empty);
            for u in updates {
                if let Some(id) = u.get("update_id").and_then(Value::as_i64) {
                    self.set_offset(id + 1);
                }
                self.hand_off(&client, u);
            }
        }
    }

    /// Giao một update cho luồng thợ — và quay lại nghe NGAY.
    ///
    /// Con dấu `offset` đã tiến trước khi tới đây (xem chỗ gọi), nên update này
    /// sẽ không được Telegram giao lại lần nữa: bỏ nó ở đây là mất hẳn một mệnh
    /// lệnh. Vì thế hàng đợi **không có trần** và không bao giờ vứt bớt; cái nó
    /// có là một tiếng kêu khi dài bất thường.
    fn hand_off(&self, client: &reqwest::blocking::Client, u: &Value) {
        if self.inline.load(Ordering::SeqCst) {
            self.handle_update(client, u);
            return;
        }
        let (lock, cv) = &*self.inflight;
        let depth = {
            let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
            q.push_back((u.clone(), std::time::Instant::now()));
            q.len()
        };
        cv.notify_one();
        // Hàng dài = thợ đang tắc (một tệp lớn) hoặc thợ đã chết. Cả hai đều
        // phải nhìn thấy được từ log, vì cả hai trông giống hệt nhau từ điện
        // thoại: gõ rồi không thấy gì.
        if depth > 5 {
            logging::warn(
                "telegram_backlog",
                json!({ "depth": depth,
                        "why": "update đã nhận nhưng thợ chưa xử lý kịp" }),
            );
        }
    }

    /// Luồng thợ: xử lý update NỐI ĐUÔI, ngoài đường đọc.
    fn work_forever(&self) {
        let Some(client) = self.client() else {
            // Thợ không có tay thì trả việc lại cho vòng đọc, đừng đứng im ôm
            // hàng — đó là kiểu hỏng mà từ điện thoại nhìn ra là "hub chết".
            self.inline.store(true, Ordering::SeqCst);
            logging::error(
                "telegram_worker_inline",
                json!({ "why": "không dựng được HTTP client cho luồng thợ" }),
            );
            return;
        };
        loop {
            let (lock, cv) = &*self.inflight;
            let (u, at) = {
                let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
                loop {
                    if let Some(item) = q.pop_front() {
                        break item;
                    }
                    q = cv.wait(q).unwrap_or_else(|e| e.into_inner());
                }
            };
            // 🔴 Phép đo THAY CHO `telegram_update_lag` ở nhánh nút bấm: hai mốc
            // này là đồng hồ của chính hub (nhận → cầm), nên nó đo đúng cái câu
            // hỏi "hub có bị dồn không" mà không phải mượn một dấu thời gian của
            // Telegram rồi đọc sai ý nghĩa của nó.
            let waited = at.elapsed();
            if waited.as_millis() > 2000 {
                logging::warn(
                    "telegram_update_wait",
                    json!({ "ms": waited.as_millis(),
                            "why": "update nằm trong hàng của hub chừng ấy trước khi tới lượt" }),
                );
            }
            self.handle_update(&client, &u);
        }
    }

    fn handle_update(&self, client: &reqwest::blocking::Client, u: &Value) {
        // 🔴 Hà 2026-08-14: *"Gửi 1 lúc hub mới nhận được"* · *"sao tự nhiên
        // phản hồi rất chậm với tele"*. Trước dòng này, câu ấy KHÔNG trả lời
        // được bằng log: hub chỉ ghi lúc nó xếp lệnh vào hàng, không ghi tin
        // được gõ lúc nào — nên "chậm" có thể nằm ở Telegram, ở vòng đọc, ở
        // hàng chờ, hay ở chính lượt làm việc của phiên, và cả bốn nghe hợp lý
        // như nhau. Đúng cái bẫy đã trả giá với `/upgrade` sáng nay.
        //
        // `message.date` là giây UNIX Telegram đóng dấu lúc NHẬN tin, nên hiệu
        // số này đo đúng quãng "gõ xong → hub cầm được", không lẫn phần sau.
        // Chỉ kêu khi quá 3 giây: dưới ngưỡng ấy là long-poll chạy đúng, và một
        // dòng log cho mỗi tin nhắn là tự dựng rác cho mình.
        //
        // 🔴 CHỈ CHO TIN CHỮ — và đây là một phép đo đã từng NÓI DỐI, gỡ ra
        // ngày 2026-08-14 sau khi đọc lại chính những con số nó in.
        //
        // Bản đầu rơi về `callback_query.message.date` khi update là một cú bấm
        // nút. Nhưng `callback_query.message` là **tin nhắn CHỨA cái nút**, nên
        // `date` của nó là lúc *bot gửi tin ấy đi* — không phải lúc ngón tay
        // bấm. Telegram KHÔNG cho biết lúc bấm; không có trường nào mang nó.
        // Tức mỗi cú bấm vào một tin cũ tự sinh ra một "độ trễ" bằng đúng tuổi
        // của tin đó.
        //
        // Bằng chứng, từ `logs/hub.log` cùng ngày — 16/17 dòng `..._lag` là ảo,
        // mỗi dòng quy đúng về một lượt `telegram_buttons_sent`:
        //   190s @08:03:30 · 239s @08:04:19 · 304s @08:05:24 → **cùng một mốc
        //   08:00:20**, là lúc hub gửi danh sách 4 phiên (`buttons_sent 4`).
        //   Ba lần bấm vào cùng một danh sách, và "trễ" leo 190→239→304 chỉ vì
        //   cái danh sách mỗi lúc một cũ. Trong quãng ấy hub vẫn nhận và chạy
        //   6 update khác — nó không hề đứng.
        // Không một tin CHỮ nào của ngày hôm ấy vượt ngưỡng 3 giây.
        //
        // Cái đo được cho một cú bấm nằm ở `telegram_update_wait` (thời gian
        // nằm trong hàng đợi của chính hub) và ở `command_done` — hai mốc hub
        // tự cầm đồng hồ, không phải một mốc mượn của Telegram rồi đọc sai.
        let started = std::time::Instant::now();
        if let Some(sent) = text_sent_at(u) {
            let lag = chrono::Utc::now().timestamp() - sent;
            if lag > 3 {
                logging::warn(
                    "telegram_update_lag",
                    json!({ "sec": lag,
                            "why": "quãng từ lúc Telegram nhận TIN CHỮ tới lúc hub cầm được nó" }),
                );
            }
        }
        // Đo luôn phần hub tự tiêu: một update nặng (tải ảnh đính kèm) giữ luồng
        // thợ, nên update tới sau phải xếp hàng sau nó — nhưng KHÔNG còn chặn
        // đường đọc `getUpdates` (xem `hand_off`).
        let _guard = UpdateTimer(started);
        struct UpdateTimer(std::time::Instant);
        impl Drop for UpdateTimer {
            fn drop(&mut self) {
                let ms = self.0.elapsed().as_millis();
                if ms > 1500 {
                    logging::warn(
                        "telegram_update_slow",
                        json!({ "ms": ms,
                                "why": "luồng thợ tiêu chừng ấy cho MỘT update — update tới sau xếp hàng sau nó" }),
                    );
                }
            }
        }
        // ── Nút bấm ──────────────────────────────────────────────────────────
        if let Some(cb) = u.get("callback_query") {
            let from = cb
                .pointer("/from/id")
                .map(|v| v.to_string())
                .unwrap_or_default();
            let data = cb.get("data").and_then(Value::as_str).unwrap_or("");
            // Trả lời cái nút ngay, nếu không nó quay mãi trên máy người bấm.
            if let Some(cbid) = cb.get("id").and_then(Value::as_str) {
                let _ = client
                    .post(self.api("answerCallbackQuery"))
                    .json(&json!({ "callback_query_id": cbid }))
                    .send();
            }
            if from != self.chat_id {
                logging::warn("telegram_button_from_stranger", json!({ "data": data }));
                return;
            }
            // Nút = phím tắt của một route đã có (xem `callback_to_command`):
            // `key:` trả lời hộp chọn, `sess:` chọn phiên từ danh sách.
            if let Some(cmd) = callback_to_command(data) {
                self.push_text(&cmd);
                return;
            }
            // Nút xác nhận (`ok:`/`no:`) chỉ có nghĩa khi `confirm::ask` đang
            // chờ — mà lúc ấy vòng này đứng im. Tới được đây nghĩa là một cú bấm
            // MUỘN, sau khi câu hỏi đã đóng sổ: nói thẳng, đừng im.
            if data.starts_with("ok:") || data.starts_with("no:") {
                logging::info(
                    "telegram_confirm_button_late",
                    json!({ "why": "câu hỏi đã đóng (hết hạn hoặc đã trả lời)" }),
                );
                let _ = self.send_text(
                    "⌛ Nút này thuộc một câu hỏi đã đóng sổ — hub không làm gì. Gửi lại lệnh nếu vẫn cần.",
                );
                return;
            }
            // `run:<n>` — nút gửi nhanh một lệnh thấy trên màn. Chữ nằm trong
            // sổ chứ không trong nút (trần 64 byte), nên phải tra lại; tra không
            // ra thì NÓI, đừng im — nút cũ bấm lại là chuyện thường.
            // `file:<n>` — gửi thẳng file ấy vào phòng chat để đọc trên điện
            // thoại. Mọi cửa (trong cây làm việc · quét rò · trần dung lượng)
            // nằm trong `send_document`, không nằm ở đây: một cửa đặt ở chỗ gọi
            // là một cửa chỗ gọi thứ hai sẽ quên.
            if let Some(n) = data.strip_prefix("file:").and_then(|n| n.parse::<usize>().ok()) {
                let db = crate::db::Db::open(&self.cfg.db).ok();
                let found = db
                    .as_ref()
                    .and_then(|db| crate::pipeline::quick_file(db, n));
                let msg = match found {
                    Some((sid, p)) => {
                        // Cây thư mục của ĐÚNG phiên đã nhắc tới đường dẫn này.
                        // Không tra ra được thì TỪ CHỐI — xem `session_root`.
                        match db
                            .as_ref()
                            .and_then(|db| crate::pipeline::session_root(db, &self.cfg, &sid))
                        {
                            Some(root) => {
                                let expanded = shellexpand_home(&p);
                                match self
                                    .send_document(std::path::Path::new(&expanded), &root)
                                {
                                    Ok(()) => None,
                                    Err(e) => Some(format!("⚠ chưa gửi được {p} — {e}")),
                                }
                            }
                            None => Some(format!(
                                "⚠ chưa gửi được {p} — không biết phiên ấy làm ở thư mục nào, \
                                 mà hub chỉ gửi file NẰM TRONG thư mục của chính phiên đó."
                            )),
                        }
                    }
                    None => Some("⚠ đường dẫn ấy đã cũ (màn đã đổi). Gõ /shot rồi bấm lại.".into()),
                };
                if let Some(m) = msg {
                    if let Err(e) = self.send_text(&m) {
                        logging::error("telegram_ack_failed", json!({ "err": e }));
                    }
                }
                return;
            }
            // `win:<n>` — cùng cuốn sổ lệnh gợi ý với `run:<n>`, khác chỗ
            // CHẠY: một cửa sổ Terminal thật, vì `!` của TUI không cấp tty
            // (xem `CommandKind::Win`). Một sổ, hai đường ra — không đẻ thêm
            // danh sách lệnh thứ hai để rồi lệch nhau.
            if let Some(n) = data.strip_prefix("win:").and_then(|n| n.parse::<usize>().ok()) {
                match crate::db::Db::open(&self.cfg.db)
                    .ok()
                    .and_then(|db| crate::pipeline::quick_cmd(&db, n))
                {
                    Some((_sid, line)) => {
                        // `/win` mở một cửa sổ MỚI nên không nhắm vào phiên nào
                        // cả — id ở đây chỉ để log truy được nút từ đâu ra.
                        logging::info(
                            "telegram_quick_cmd_window",
                            json!({ "n": n, "from_session": _sid,
                                    "cmd": crate::exec::truncate(&line, 120) }),
                        );
                        self.push_text(&format!("/win {line}"));
                    }
                    None => {
                        if let Err(e) = self.send_text(
                            "⚠ lệnh gợi ý ấy đã cũ (màn đã đổi). Gõ /shot rồi bấm lại.",
                        ) {
                            logging::error("telegram_ack_failed", json!({ "err": e }));
                        }
                    }
                }
                return;
            }
            if let Some(n) = data.strip_prefix("run:").and_then(|n| n.parse::<usize>().ok()) {
                match crate::db::Db::open(&self.cfg.db)
                    .ok()
                    .and_then(|db| crate::pipeline::quick_cmd(&db, n))
                {
                    Some((sid, line)) => {
                        logging::info(
                            "telegram_quick_cmd",
                            json!({ "n": n, "session": sid,
                                    "cmd": crate::exec::truncate(&line, 120) }),
                        );
                        // `!` = chạy TRONG phiên: phiên nhìn thấy kết quả và đi
                        // tiếp được. Đi qua `/type`, tức cùng một đường gõ phím
                        // đã có, không đẻ lối riêng.
                        //
                        // 🔴 Tôi đã suýt định tuyến nhánh này sang `/cmd` cho
                        // các lệnh nằm trong `DENIED_TOOLS`, và Hà chặn đúng
                        // lúc: *"vô lý việc gõ vào phiên là hub làm mà"* ·
                        // *"sao lại chặn được"*. Anh đúng — `--disallowedTools`
                        // gác **lời gọi công cụ của AI**, còn `!<lệnh>` là chế
                        // độ bash của chính TUI, tức đúng cái ngón tay chủ máy
                        // gõ. Hai thứ khác hẳn nhau, và tôi đã lẫn chúng làm
                        // một. Giữ nguyên một đường.
                        // 🔴 KHÔNG còn dấu `!`. Hà 2026-08-13, ảnh chụp màn
                        // phiên tfl5: *"làm gì mà nó không nhận chạy lệnh mà
                        // thành gửi text vào hỏi như bình thường"*. Trên ảnh,
                        // dòng `!bash scripts/verify-acl-delta-0813.sh` vào
                        // phiên như một CÂU HỎI THƯỜNG, và `claude` đọc nó rồi
                        // tự gọi Bash tool để chạy.
                        //
                        // Vì sao: `do script` đẩy chữ + xuống dòng trong CÙNG
                        // một lượt ghi, và TUI đọc lượt ấy như một cú DÁN —
                        // điều tệp này đã ghi từ 08-12 cho chuyện dấu Enter.
                        // Chế độ bash của TUI chỉ bật khi `!` tới như một PHÍM
                        // GÕ đầu tiên vào ô trống; `!` trong một cú dán chỉ là
                        // một ký tự. Tức quy ước `!<lệnh>` của hub **chưa bao
                        // giờ chạy** theo cách nó tự mô tả.
                        //
                        // Và nó không vô hại: nếu có lượt nào `!` bật được chế
                        // độ ấy thì chế độ DÍNH, nên tin nhắn thường tiếp theo
                        // rơi vào ô bash và chạy như lệnh shell — đúng cái
                        // `command not found: Tôi` Hà gặp tối nay.
                        //
                        // Bỏ hẳn `!`: gửi nguyên dòng lệnh cho phiên, đúng như
                        // chủ máy tự gõ. Cần shell THẬT thì đã có `🖥` (cửa sổ
                        // mới, có tty, không tốn hạn mức).
                        //
                        // MANG ID THEO. Hà: *"Nội dung có nút bấm
                        // nhưng bấm xong lại gửi vào phiên khác đang đc chọn"* —
                        // và bằng chứng rơi thẳng vào cuộc trò chuyện: nút
                        // `▶ bash scripts/verify-acl-2026-08-13.sh` của `[tfl5]`
                        // bấm ra dòng `!bash scripts/verify-acl-…` chạy trong
                        // phiên `[hub]`. Tệp ấy nằm ở `AI/tfl5/scripts/`, hub
                        // không có nó.
                        //
                        // `/type` không mang id thì rơi về con trỏ focus
                        // (`target_and_rest`), mà con trỏ ĐỔI ĐƯỢC giữa lúc nút
                        // sinh ra và lúc nút được bấm. Đây đúng con đường đã gõ
                        // nhầm phiên sáng 08-11, và `remember_files` đã vá cho
                        // nút 📎 — một cuốn sổ được vá, cuốn bên cạnh thì không.
                        // MÁY chạy, PHIÊN đọc (Hà 2026-08-13: *"nên gọi lệnh
                        // ở command khác rồi lấy kết quả dán gửi lại vào
                        // phiên"*). Không tốn hạn mức cho một việc `zsh -lc`
                        // làm được, và phiên vẫn thấy kết quả nên đi tiếp được.
                        self.push_text(&format!("/runin {sid} {line}"));
                    }
                    None => {
                        if let Err(e) = self.send_text(
                            "⚠ lệnh gợi ý ấy đã cũ (màn đã đổi). Gõ /shot rồi bấm lại.",
                        ) {
                            logging::error("telegram_ack_failed", json!({ "err": e }));
                        }
                    }
                }
                return;
            }
            // `box:<n>` — gửi chính chữ đang nằm trong ô nhập của phiên.
            //
            // 🔴 Hà 2026-08-13: *"nên chỗ gợi ý đó cần thao tác bấm là gửi luôn
            // text đó tới phiên"*. Hai bước, một cú bấm: **Esc** xoá ô trước,
            // rồi gõ lại nguyên chữ ấy. Xoá trước là bắt buộc vì hub KHÔNG phân
            // biệt được "gợi ý mờ" (ô rỗng thật) với "chữ đã gõ" (ô có chữ) —
            // màn đọc về không mang màu. Không xoá thì ca thứ hai thành
            // `pushpush`.
            if let Some(n) = data.strip_prefix("box:").and_then(|n| n.parse::<usize>().ok()) {
                match crate::db::Db::open(&self.cfg.db)
                    .ok()
                    .and_then(|db| crate::pipeline::quick_cmd(&db, n))
                {
                    Some((sid, line)) => {
                        logging::info(
                            "telegram_box_send",
                            json!({ "n": n, "session": sid,
                                    "text": crate::exec::truncate(&line, 60) }),
                        );
                        // Hai lệnh, đúng thứ tự — `CMD_LOCK` giữ chúng nối đuôi.
                        // Bước phụ ⟹ IM: một cú bấm chỉ nên ra một câu trả lời.
                        // CẢ HAI mang id: nếu chỉ một cái mang thì Esc xoá ô của
                        // phiên này còn chữ lại rơi vào phiên kia.
                        self.push_text_quiet(&format!("/key {sid} esc"));
                        self.push_text(&format!("/type {sid} {line}"));
                    }
                    None => {
                        if let Err(e) =
                            self.send_text("⚠ chữ ấy đã cũ (màn đã đổi). Gõ /shot rồi bấm lại.")
                        {
                            logging::error("telegram_ack_failed", json!({ "err": e }));
                        }
                    }
                }
                return;
            }
            // `say:<n>` — gửi một câu CHỮ THƯỜNG vào phiên đang theo.
            //
            // Khác `run:<n>` ở NỘI DUNG chứ không ở đường đi: `run:` gõ một
            // dòng lệnh cho phiên chạy; `say:` gõ nguyên câu như
            // chủ máy tự gõ. Dùng cho nút "✅ Làm đi" khi phiên đang mời một
            // tiếng "ừ" (Hà 2026-08-13).
            if let Some(n) = data.strip_prefix("say:").and_then(|n| n.parse::<usize>().ok()) {
                match crate::db::Db::open(&self.cfg.db)
                    .ok()
                    .and_then(|db| crate::pipeline::quick_cmd(&db, n))
                {
                    Some((sid, line)) => {
                        logging::info(
                            "telegram_quick_say",
                            json!({ "n": n, "session": sid,
                                    "text": crate::exec::truncate(&line, 60) }),
                        );
                        // Chữ thường cũng phải mang id: cùng lý do với `run:`.
                        self.push_text(&format!("/type {sid} {line}"));
                    }
                    None => {
                        if let Err(e) =
                            self.send_text("⚠ gợi ý ấy đã cũ (màn đã đổi). Gõ /shot rồi bấm lại.")
                        {
                            logging::error("telegram_ack_failed", json!({ "err": e }));
                        }
                    }
                }
                return;
            }
            // `full:<n>` — bản ĐẦY ĐỦ của báo cáo đã bị rút gọn. Telegram chặn
            // ở 4096 ký tự nên cắt thành nhiều tin, cắt theo DÒNG để không đứt
            // giữa câu.
            if let Some(n) = data.strip_prefix("full:").and_then(|n| n.parse::<usize>().ok()) {
                let full = crate::db::Db::open(&self.cfg.db)
                    .ok()
                    .and_then(|db| crate::pipeline::full_report(&db, n));
                match full {
                    Some((sid, sname, text)) => {
                        // Đọc xong một báo cáo dài thì việc kế tiếp gần như luôn
                        // là ĐI VÀO chính phiên ấy — nên hub ĐI LUÔN, không đưa
                        // thêm một cái nút nữa.
                        //
                        // 🔴 Hà 2026-08-13: *"khi bấm xem đầy đủ thì rõ ràng nó
                        // đang ở phiên đúng rồi cần gì có nút vào phiên nữa"*.
                        // Sáng nay chính anh xin cái nút ấy; buổi chiều dùng thật
                        // thì thấy nó là **một cú bấm thừa** — bấm "Xem đầy đủ"
                        // đã là chọn phiên rồi, cái nút chỉ bắt nói lại điều vừa
                        // nói. Cùng một bài học với `sess:` sáng nay (*"bấm vào
                        // phiên sao không hiện shot luôn"*): việc kế tiếp đã
                        // chắc chắn thì đừng bắt bấm thêm.
                        //
                        // Đổi con trỏ là đổi NƠI CHỮ ANH GÕ SẼ ĐI TỚI, nên nó
                        // phải được NÓI RA trong chính tin ấy — và chỉ được nói
                        // khi đã ghi xong sổ. Ghi hỏng mà vẫn in "đang theo" là
                        // đúng loại nói dối làm người ta gõ việc vào nhầm phiên.
                        let db = crate::db::Db::open(&self.cfg.db).ok();
                        let focus = db
                            .as_ref()
                            .and_then(|db| db.cursor_or_log(crate::pipeline::FOCUS_SESSION_KEY))
                            .unwrap_or_default();
                        let moved = if sid.is_empty() || sid == focus {
                            None
                        } else {
                            match db
                                .as_ref()
                                .map(|db| db.set_cursor(crate::pipeline::FOCUS_SESSION_KEY, &sid))
                            {
                                Some(Ok(())) => {
                                    logging::info(
                                        "focus_moved_by_full_report",
                                        json!({ "session": sid, "from": focus,
                                                "why": "bấm Xem đầy đủ = chọn phiên ấy (Hà 2026-08-13)" }),
                                    );
                                    Some(true)
                                }
                                other => {
                                    logging::error(
                                        "focus_move_failed",
                                        json!({ "session": sid, "detail": format!("{other:?}") }),
                                    );
                                    Some(false)
                                }
                            }
                        };
                        let tail = crate::pipeline::full_report_follow_note(&sname, moved);
                        // 🔴 Nút phải theo BẢN ĐẦY ĐỦ nữa. Hà 2026-08-13:
                        // *"Chọn xem đầy đủ lại không có chạy lệnh rồi"*.
                        //
                        // …và một ngày sau, cùng một chỗ, cùng một hình dạng:
                        // *"Có lệnh bash sao lại ko có nút bấm chạy cho nó …
                        // gắn icon bấm được ngay sau chuỗi lệnh đó"*
                        // (2026-08-14). Lần trước vá bằng cách treo NÚT dưới
                        // mảnh cuối; nhưng một khối nút ở đáy một báo cáo dài
                        // bắt người đọc tự ghép "nút nào ứng với dòng nào", và
                        // đúng lúc bấm "Xem đầy đủ" để đọc kỹ thì đường bấm lại
                        // nghèo hơn tin rút gọn (nơi icon ▶️ đã nằm ngay cuối
                        // dòng lệnh từ sáng).
                        //
                        // Nay đi CHUNG máy móc với tin tự phát:
                        // `say_with_command_icons` — cắt ngay sau dòng lệnh,
                        // dán icon vào cuối chính dòng ấy, chia nhỏ cho vừa trần
                        // Telegram. Không còn "📄 (i/n)": mẩu nay cắt theo Ý
                        // (kết bằng một dòng lệnh) chứ không theo số ký tự, nên
                        // một cái đánh số đo bằng độ dài chỉ nói sai.
                        //
                        // 🔴 `commands_in_report`, KHÔNG phải
                        // `commands_on_screen`: chữ này lấy từ nhật ký phiên,
                        // không đi qua bề ngang cửa sổ nào — xem
                        // `keys::BTN_CMD_REPORT_MAX` (chính cái trần 60 đã chặn
                        // dòng `bash …/deploy.sh …` dài 80 ký tự trong ảnh Hà
                        // gửi).
                        let body = format!("{text}{tail}");
                        let cmds = crate::keys::commands_in_report(&text, 3);
                        let mut btns = db
                            .as_ref()
                            .map(|db| {
                                let mut b =
                                    crate::pipeline::remember_quick(db, &sid, &cmds);
                                b.extend(crate::pipeline::remember_files(
                                    db,
                                    &sid,
                                    &crate::keys::paths_on_screen(&text, 4),
                                ));
                                b
                            })
                            .unwrap_or_default();
                        btns.truncate(8);
                        crate::pipeline::say_with_command_icons(
                            self,
                            &body,
                            &cmds,
                            &btns,
                            "telegram_ack_failed",
                        );
                    }
                    None => {
                        if let Err(e) = self
                            .send_text("⚠ bản đầy đủ ấy cũ quá rồi (hub chỉ giữ 8 bản gần nhất).")
                        {
                            logging::error("telegram_ack_failed", json!({ "err": e }));
                        }
                    }
                }
                return;
            }
            logging::info("telegram_button_unknown", json!({ "data": data }));
            return;
        }

        // ── Tin nhắn chữ ────────────────────────────────────────────────────
        let Some(msg) = u.get("message").or_else(|| u.get("edited_message")) else {
            return;
        };
        let from = msg
            .pointer("/chat/id")
            .map(|v| v.to_string())
            .unwrap_or_default();
        // ĐÍNH KÈM: ảnh / tệp gửi kèm tin nhắn (Hà 2026-08-13: *"thêm cơ chế
        // nhận đính kèm file vào tin nhắn"*).
        //
        // Cùng cổng với chữ: chỉ nhận từ đúng `chat_id` của chủ máy. Tệp về
        // `<thư mục dự án của phiên đang theo>/.inbox/`, rồi hub gõ MỘT DÒNG
        // vào phiên nói đường dẫn — tức đi đúng con đường chữ thường đã có,
        // không đẻ lối riêng cho phiên phải học.
        if from == self.chat_id {
            if let Some((file_id, name)) = attachment_of(msg) {
                self.take_file(&file_id, &name, msg.get("caption").and_then(Value::as_str));
                return;
            }
        }
        let Some(text) = msg.get("text").and_then(Value::as_str) else {
            return;
        };
        if from != self.chat_id {
            logging::warn(
                "telegram_message_from_stranger",
                json!({ "head": crate::exec::truncate(text, 40) }),
            );
            return;
        }
        self.push_text_from(text, msg.get("message_id").and_then(Value::as_i64));
        // 🔴 Hà 2026-08-14, ảnh chụp buồng chat sau khi bấm icon ▶️: *"Sao bấm
        // lại thành lệnh start à"*.
        //
        // Đúng, và đó là cái GIÁ của một icon nằm GIỮA CHỮ: Telegram không đặt
        // nút vào giữa chữ được, thứ đặt được là một liên kết
        // `t.me/<bot>?start=run_0`, và bấm nó thì client GỬI `/start run_0` —
        // rồi hiển thị nó rút gọn còn `/start`, giấu mất payload. Nên chủ máy
        // thấy chính mình vừa gõ một câu chẳng nói gì.
        //
        // Không bỏ được cơ chế (bỏ là mất icon trong chữ, quay về khối nút ở
        // đáy — thứ vừa bị chê), nhưng cái vết thì dọn được: trong buồng chat
        // RIÊNG, bot xoá được tin đến (Bot API, `deleteMessage`, trong 48 giờ).
        // Xoá hỏng thì thôi, không phải lỗi chặn việc — nhưng phải có dòng nói
        // ra, đừng im.
        if text == "/start" || text.starts_with("/start ") {
            if let Some(mid) = msg.get("message_id").and_then(Value::as_i64) {
                if let Err(e) = self.delete_message(mid) {
                    logging::info(
                        "telegram_start_echo_kept",
                        json!({ "why": e, "what": "tiếng vọng /start của một cú bấm icon" }),
                    );
                }
            }
        }
    }

    /// Xoá MỘT tin của chính bot. `Err` mang nguyên câu Telegram trả lời, vì
    /// chỗ gọi phải phân biệt "hỏng mạng, thử lại sau" với "không bao giờ xoá
    /// được nữa".
    pub fn delete_message(&self, message_id: i64) -> Result<(), String> {
        let client = self.client().ok_or("không dựng được HTTP client")?;
        let r = client
            .post(self.api("deleteMessage"))
            .json(&json!({ "chat_id": self.chat_id, "message_id": message_id }))
            .send()
            .map_err(|e| e.to_string())?;
        let v: Value = r.json().unwrap_or_else(|_| json!({}));
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(())
        } else {
            Err(v
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("Telegram từ chối deleteMessage")
                .to_string())
        }
    }

    /// Gửi một tin đã dựng sẵn dưới dạng HTML (`parse_mode=HTML`).
    ///
    /// Chỗ gọi tự chịu trách nhiệm escape phần CHỮ (`html_escape`) và chỉ chèn
    /// thẻ mình cố ý — ở đây là `<a href="…">▶️</a>` và `<code>`. Không gột
    /// Markdown: chữ đã qua tay chỗ gọi rồi, gột thêm là sửa cái mình vừa dựng.
    pub fn send_html(&self, html: &str) -> Result<(), String> {
        let client = self.client().ok_or("không dựng được HTTP client")?;
        let r = client
            .post(self.api("sendMessage"))
            .json(&json!({
                "chat_id": self.chat_id,
                "text": html,
                "parse_mode": "HTML",
                // Xem trước liên kết sẽ nở một khung to đùng dưới mỗi tin có
                // icon — đúng thứ không ai muốn khi cái link ấy chỉ là một nút
                // trá hình.
                "link_preview_options": { "is_disabled": true },
            }))
            .send()
            .map_err(|e| e.to_string())?;
        let v: Value = r.json().unwrap_or_else(|_| json!({}));
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            remember_sent(&self.cfg, &v);
            Ok(())
        } else {
            Err(v
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("Telegram từ chối sendMessage (HTML)")
                .to_string())
        }
    }

    /// Thả MỘT emoji lên chính tin của chủ máy, thay cho một câu trả lời.
    ///
    /// 🔴 Hà 2026-08-14: *"Có thể đổi cách phản hồi tin đã gửi bằng 1 emoji
    /// trực tiếp vào tin nhắn cho gọn"*.
    ///
    /// Telegram chỉ nhận emoji trong một BẢNG CỐ ĐỊNH (Bot API,
    /// `ReactionTypeEmoji`) — `✓` hay `▶` không nằm trong đó và sẽ bị từ chối,
    /// nên chỗ gọi phải chọn từ bảng ấy. `Err` mang nguyên câu Telegram trả
    /// lời: chỗ gọi cần phân biệt "phiên bản Telegram không cho thả" với "emoji
    /// sai", và cả hai đều phải rơi về một câu chữ chứ không được im.
    pub fn react(&self, message_id: i64, emoji: &str) -> Result<(), String> {
        let client = self.client().ok_or("không dựng được HTTP client")?;
        let r = client
            .post(self.api("setMessageReaction"))
            .json(&json!({
                "chat_id": self.chat_id,
                "message_id": message_id,
                "reaction": [{ "type": "emoji", "emoji": emoji }],
            }))
            .send()
            .map_err(|e| e.to_string())?;
        let v: Value = r.json().unwrap_or_else(|_| json!({}));
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(())
        } else {
            Err(v
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("Telegram từ chối setMessageReaction")
                .to_string())
        }
    }

    /// Gửi một câu ra Telegram. `Err` chứ không nuốt — chỗ gọi phải log.
    pub fn send_text(&self, text: &str) -> Result<(), String> {
        let client = self.client().ok_or("không dựng được HTTP client")?;
        let r = client
            .post(self.api("sendMessage"))
            .json(&json!({ "chat_id": self.chat_id, "text": strip_markdown(text) }))
            .send()
            .map_err(|e| e.to_string())?;
        let v: Value = r.json().unwrap_or_else(|_| json!({}));
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            remember_sent(&self.cfg, &v);
            Ok(())
        } else {
            Err(v
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("Telegram từ chối sendMessage")
                .to_string())
        }
    }

    /// Gửi câu hỏi của một phiên kèm MỘT NÚT CHO MỖI LỰA CHỌN.
    ///
    /// Hà 2026-08-11: *"cần thêm thông tin mô tả liên quan tới lựa chọn đó mới
    /// hợp lý"* — và bước sau của nó: chọn được ngay tại đây. Nút gửi về
    /// `/key <session_id> <n>`, tức đi đúng con đường mà trang cũng đi.
    /// `offer_enter`: thêm một hàng nút **vào phiên** ở cuối (Hà 2026-08-12 —
    /// *"nếu báo phiên khác phiên đang theo thì thêm nút vào phiên"*). Trả lời
    /// được hộp chọn từ xa là một chuyện; **nhìn phiên ấy đang làm gì** trước
    /// khi trả lời là chuyện khác, và không có nút thì nó đòi gõ tay một uuid.
    pub fn ask_choices(
        &self,
        text: &str,
        session_id: &str,
        labels: &[String],
        offer_enter: bool,
        multi: bool,
        rest: &[crate::sessions::Question],
    ) -> Result<(), String> {
        self.send_buttons(
            text,
            &choice_buttons(session_id, labels, offer_enter, multi, rest),
        )
    }

/// Xếp nút thành hàng: mặc định mỗi nút một hàng, RIÊNG nút file thì gộp.
///
/// 🔴 Hà 2026-08-13, ảnh chụp ba nút 📎 chồng nhau dưới một tin: *"sao không
/// chèn thẳng nút xem file vào nội dung cho gọn thay vì nút độc lập"*.
///
/// Nói thẳng chỗ KHÔNG làm được: Telegram không có nút nằm giữa chữ. Bàn phím
/// `inline_keyboard` luôn là một khối dưới tin, và thứ duy nhất chèn được vào
/// nội dung là một đường dẫn siêu liên kết — mà một đường dẫn thì không gọi
/// được bot để lấy file về. Nên "chèn vào nội dung" không có đường thi hành;
/// phần *"cho gọn"* thì có, và đó là phần thật của yêu cầu.
///
/// Luật cũ — mỗi nút một hàng — viết cho nhãn DÀI (tên phiên kèm trạng thái):
/// xếp ngang là cắt cụt, mà nút đọc không hết thì bấm bằng đoán. Nút file
/// không thuộc ca ấy: nhãn là một tên file, ngắn. Ba nút file ngốn ba hàng
/// trong khi chúng vừa gọn một hàng.
///
/// Gộp theo `callback_data` bắt đầu bằng `file:` chứ không theo vị trí: đó là
/// dấu hiệu của thứ đang xếp, không phải một quy ước đếm-thứ-tự mà chỗ gọi
/// phải nhớ giữ đúng. Ba nút một hàng — bốn thì nhãn bắt đầu bị cắt trên 390px.
pub fn keyboard_rows(buttons: &[(String, String)]) -> Vec<Vec<Value>> {
    let mut rows: Vec<Vec<Value>> = Vec::new();
    for (label, data) in buttons {
        let cell = json!({ "text": label, "callback_data": data });
        let joinable = data.starts_with("file:");
        match rows.last_mut() {
            Some(last)
                if joinable
                    && last.len() < 3
                    && last.first().and_then(|c| c.get("callback_data"))
                        .and_then(Value::as_str)
                        .is_some_and(|d| d.starts_with("file:")) =>
            {
                last.push(cell);
            }
            _ => rows.push(vec![cell]),
        }
    }
    rows
}

    /// Gửi một câu kèm bảng nút — **mỗi nút một hàng**.
    ///
    /// Nhãn của `claude` (và tên phiên kèm trạng thái) thường dài; xếp ngang là
    /// cắt cụt trên màn điện thoại, mà một cái nút đọc không hết thì bấm bằng
    /// đoán. `callback_data` của cả hai đường (`key:` 42 byte, `sess:` 41 byte)
    /// nằm dưới trần 64 byte của Telegram.
    ///
    /// Không có nút nào thì gửi như một câu thường — một bảng phím rỗng là thứ
    /// Telegram từ chối, và đó sẽ là một lỗi nói về API chứ không nói về việc.
    pub fn send_buttons(&self, text: &str, buttons: &[(String, String)]) -> Result<(), String> {
        if buttons.is_empty() {
            return self.send_text(text);
        }
        let client = self.client().ok_or("không dựng được HTTP client")?;
        let keyboard = Self::keyboard_rows(buttons);
        let r = client
            .post(self.api("sendMessage"))
            .json(&json!({
                "chat_id": self.chat_id,
                "text": strip_markdown(text),
                "reply_markup": { "inline_keyboard": keyboard },
            }))
            .send()
            .map_err(|e| e.to_string())?;
        let v: Value = r.json().unwrap_or_else(|_| json!({}));
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            remember_sent(&self.cfg, &v);
            // Ghi SỐ NÚT đã gửi. "Gửi được tin" và "tin ấy có nút bấm" là hai
            // chuyện khác nhau, mà từ máy này không nhìn thấy màn hình điện
            // thoại — không có dòng này thì câu "đã có nút" chỉ là suy luận từ
            // việc không có lỗi.
            logging::info(
                "telegram_buttons_sent",
                json!({ "count": buttons.len() }),
            );
            Ok(())
        } else {
            Err(v
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("Telegram từ chối sendMessage")
                .to_string())
        }
    }
}

/// Dấu thời gian Telegram đóng lúc NHẬN TIN — chỉ có với tin chữ.
///
/// Hàm thuần, tách ra để kiểm được: đây là chỗ một phép đo đã nói dối suốt
/// ngày 2026-08-14 (xem `handle_update`). Một cú bấm nút KHÔNG có mốc nào —
/// `callback_query.message.date` là tuổi của tin chứa cái nút, và trả nó ra ở
/// đây là biến "bấm vào một tin cũ" thành "hub trễ 5 phút".
pub fn text_sent_at(u: &Value) -> Option<i64> {
    u.pointer("/message/date")
        .or_else(|| u.pointer("/edited_message/date"))
        .and_then(Value::as_i64)
}

/// Quyền đọc đang được `confirm::ask` mượn; trả lại khi rời tầm.
pub struct Hold<'a> {
    inbox: &'a Inbox,
}

impl Drop for Hold<'_> {
    fn drop(&mut self) {
        self.inbox.busy.store(false, Ordering::SeqCst);
    }
}

/// Tệp đính kèm của một tin nhắn Telegram, nếu có: `(file_id, tên gợi ý)`.
///
/// Ảnh tới dưới dạng MẢNG nhiều cỡ — lấy cỡ CUỐI (to nhất), vì cỡ đầu là bản
/// xem trước vài KB, gửi cho phiên đọc thì chẳng thấy gì.
fn attachment_of(msg: &Value) -> Option<(String, String)> {
    if let Some(d) = msg.get("document") {
        let id = d.get("file_id").and_then(Value::as_str)?;
        let name = d
            .get("file_name")
            .and_then(Value::as_str)
            .unwrap_or("tep-nhan-duoc");
        return Some((id.to_string(), name.to_string()));
    }
    if let Some(p) = msg.get("photo").and_then(Value::as_array) {
        let biggest = p.last()?;
        let id = biggest.get("file_id").and_then(Value::as_str)?;
        let uid = biggest
            .get("file_unique_id")
            .and_then(Value::as_str)
            .unwrap_or("anh");
        return Some((id.to_string(), format!("anh-{uid}.jpg")));
    }
    for (key, ext) in [("voice", "ogg"), ("audio", "mp3"), ("video", "mp4")] {
        if let Some(v) = msg.get(key) {
            let id = v.get("file_id").and_then(Value::as_str)?;
            let name = v
                .get("file_name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("{key}.{ext}"));
            return Some((id.to_string(), name));
        }
    }
    None
}
