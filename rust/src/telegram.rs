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
use std::sync::{Arc, Mutex, OnceLock};
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
}

/// Hòm thư Telegram dùng chung cho cả tiến trình.
#[derive(Clone)]
pub struct Inbox {
    queue: Arc<Mutex<VecDeque<Incoming>>>,
    /// `update_id` kế tiếp cần đọc. **Một** con dấu cho cả bot — xem luật 1.
    offset: Arc<Mutex<i64>>,
    /// `confirm::ask` đang giữ đường đọc.
    busy: Arc<AtomicBool>,
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
) -> Vec<(String, String)> {
    let mut buttons: Vec<(String, String)> = labels
        .iter()
        .enumerate()
        .take(9)
        .map(|(i, l)| {
            (
                format!("{}. {}", i + 1, crate::exec::truncate(l, 60)),
                format!("key:{}:{}", session_id, i + 1),
            )
        })
        .collect();
    if offer_enter {
        buttons.push(("👁 Vào phiên này".to_string(), format!("sess:{session_id}")));
    }
    buttons
}

pub fn callback_to_command(data: &str) -> Option<String> {
    if let Some(rest) = data.strip_prefix("key:") {
        let (sid, n) = rest.split_once(':')?;
        if sid.is_empty() || n.is_empty() {
            return None;
        }
        return Some(format!("/key {sid} {n}"));
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
            token,
            chat_id,
            cfg: std::sync::Arc::new(cfg.clone()),
            waker,
        };
        let _ = INBOX.set(inbox.clone());
        let worker = inbox.clone();
        std::thread::Builder::new()
            .name("telegram-inbox".into())
            .spawn(move || worker.read_forever())
            .map_err(|e| {
                logging::error("telegram_inbox_spawn_failed", json!({ "err": e.to_string() }));
            })
            .ok()?;
        logging::info("telegram_inbox_started", json!({ "chat_id_env": cfg.confirm.chat_id_env }));
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
        let folder = db
            .as_ref()
            .and_then(|db| db.cursor_or_log(crate::pipeline::WATCH_KEY))
            .and_then(|v| crate::pipeline::session_folder_from_book(&v, &focus))
            .unwrap_or_default();
        let mut dir = self.cfg.workspace_root.clone();
        if !folder.is_empty() {
            dir = dir.join(&folder);
        }
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
        let t = text.trim();
        if t.is_empty() {
            return;
        }
        self.queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(Incoming { text: t.to_string() });
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
                None => logging::info(
                    "telegram_bot_identity",
                    json!({
                        "username": v.get("result").and_then(|r| r.get("username")).and_then(Value::as_str),
                        "id": v.get("result").and_then(|r| r.get("id")).and_then(Value::as_i64),
                    }),
                ),
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
                self.handle_update(&client, u);
            }
        }
    }

    fn handle_update(&self, client: &reqwest::blocking::Client, u: &Value) {
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
            if let Some(n) = data.strip_prefix("run:").and_then(|n| n.parse::<usize>().ok()) {
                match crate::db::Db::open(&self.cfg.db)
                    .ok()
                    .and_then(|db| crate::pipeline::quick_cmd(&db, n))
                {
                    Some(line) => {
                        logging::info(
                            "telegram_quick_cmd",
                            json!({ "n": n, "cmd": crate::exec::truncate(&line, 120) }),
                        );
                        // `!` = chạy TRONG phiên: phiên nhìn thấy kết quả và đi
                        // tiếp được. Đi qua `/type`, tức cùng một đường gõ phím
                        // đã có, không đẻ lối riêng.
                        self.push_text(&format!("/type !{line}"));
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
            // `full:<n>` — bản ĐẦY ĐỦ của báo cáo đã bị rút gọn. Telegram chặn
            // ở 4096 ký tự nên cắt thành nhiều tin, cắt theo DÒNG để không đứt
            // giữa câu.
            if let Some(n) = data.strip_prefix("full:").and_then(|n| n.parse::<usize>().ok()) {
                let full = crate::db::Db::open(&self.cfg.db)
                    .ok()
                    .and_then(|db| crate::pipeline::full_report(&db, n));
                match full {
                    Some((sid, sname, text)) => {
                        let mut chunk = String::new();
                        let mut parts: Vec<String> = Vec::new();
                        for line in text.lines() {
                            if chunk.len() + line.len() + 1 > 3500 {
                                parts.push(std::mem::take(&mut chunk));
                            }
                            chunk.push_str(line);
                            chunk.push('\n');
                        }
                        if !chunk.trim().is_empty() {
                            parts.push(chunk);
                        }
                        // Đọc xong một báo cáo dài thì việc kế tiếp gần như luôn
                        // là ĐI VÀO chính phiên ấy (Hà 2026-08-13). Nút đi kèm
                        // TIN CUỐI, và chỉ khi đó không phải phiên đang theo —
                        // đúng luật của `pipeline::enter_button`.
                        let focus = crate::db::Db::open(&self.cfg.db)
                            .ok()
                            .and_then(|db| db.cursor_or_log(crate::pipeline::FOCUS_SESSION_KEY))
                            .unwrap_or_default();
                        let enter = (!sid.is_empty() && sid != focus).then(|| {
                            (
                                format!("👁 Vào phiên {}", crate::exec::truncate(&sname, 24)),
                                format!("sess:{sid}"),
                            )
                        });
                        let total = parts.len();
                        for (i, p) in parts.into_iter().enumerate() {
                            let head = if total > 1 {
                                format!("📄 ({}/{})\n", i + 1, total)
                            } else {
                                String::new()
                            };
                            let body = format!("{head}{p}");
                            let last = i + 1 == total;
                            let sent = match (&enter, last) {
                                (Some(b), true) => self.send_buttons(&body, std::slice::from_ref(b)),
                                _ => self.send_text(&body),
                            };
                            if let Err(e) = sent {
                                logging::error("telegram_ack_failed", json!({ "err": e }));
                                break;
                            }
                        }
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
        self.push_text(text);
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

    /// Gửi một câu ra Telegram. `Err` chứ không nuốt — chỗ gọi phải log.
    pub fn send_text(&self, text: &str) -> Result<(), String> {
        let client = self.client().ok_or("không dựng được HTTP client")?;
        let r = client
            .post(self.api("sendMessage"))
            .json(&json!({ "chat_id": self.chat_id, "text": text }))
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
    ) -> Result<(), String> {
        self.send_buttons(text, &choice_buttons(session_id, labels, offer_enter))
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
        let keyboard: Vec<Vec<Value>> = buttons
            .iter()
            .map(|(label, data)| vec![json!({ "text": label, "callback_data": data })])
            .collect();
        let r = client
            .post(self.api("sendMessage"))
            .json(&json!({
                "chat_id": self.chat_id,
                "text": text,
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
