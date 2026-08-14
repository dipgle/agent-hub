//! Structured JSONL logging.
//!
//! Charter rule #1 (no silent failure): every error path must produce a line
//! here AND a durable row (runs / dead_letter) in the DB. A swallowed error
//! that only returns a default is a bug.

use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

const DEBUG: u8 = 10;
const INFO: u8 = 20;
const WARN: u8 = 30;
const ERROR: u8 = 40;

static LEVEL: AtomicU8 = AtomicU8::new(INFO);

/// Đếm dòng mức `error` kể từ lúc tiến trình bật, để một VÒNG biết được nó có
/// sạch hay không.
///
/// 🔴 Sinh ra 2026-08-14, để vá một lỗ do chính lượt gỡ tfl5 mở ra. Bảng `runs`
/// từng được chặng hỏi vòng ghi; chặng ấy đi, `run_once` ghi thay — nhưng vòng
/// thì gần như không bao giờ trả `Err`, nên hàng nào cũng `ok`, nên bất cứ khối
/// nào đọc nó cũng **rỗng vĩnh viễn**: đúng cái phép đo mù mà repo này gọi tên
/// ở `CLAUDE.md` và `OPERATING-CHARTER §2d`.
///
/// ⚠ Bản đầu của chú thích này nói khối ấy là *"lỗi gần đây của `/doctor`"* —
/// **sai**, và sai theo kiểu tệ nhất: nghe hợp lý nên không ai kiểm.
/// `runtime::errors_block` sống trong `runtime::snapshot`, mà hàm ấy có đúng
/// một chỗ gọi là `portal.rs` — tệp đã chết. Tức nó không có người đọc nào.
/// Người đọc THẬT của `runs` lúc ấy chỉ có `hub status` trên CLI. Nay `/doctor`
/// đọc thật, qua `pipeline::recent_errors_line`.
///
/// Vì sao đo bằng NHẬT KÝ chứ không bắt từng handler trả lỗi lên: luật 3 của dự
/// án đã bắt **mọi đường lỗi phải ghi một dòng ở đây**. Nếu luật ấy đúng thì số
/// dòng `error` CHÍNH LÀ số lỗi — không phải một phép xấp xỉ, mà là cùng một
/// mệnh đề đọc từ đầu kia. Còn nếu luật ấy sai ở đâu đó thì cái sai nằm ở chỗ
/// nuốt lỗi, không phải ở đây.
static ERRORS: AtomicU64 = AtomicU64::new(0);

/// Tên sự kiện của dòng `error` gần nhất — **chỉ `msg`, không bao giờ `fields`**.
///
/// Đây là ranh giới có chủ ý, không phải tiết kiệm: `msg` là một hằng chuỗi viết
/// trong mã (`telegram_reaction_failed`), còn `fields` mang dữ liệu chạy thật —
/// đường dẫn, câu lỗi của thư viện, và đã từng mang nguyên khoá bot (xem
/// `redact`). Chuỗi này đi vào một hàng `runs`, rồi từ đó lên màn điện thoại qua
/// `/doctor`; cho `fields` đi cùng là mở lại đúng con đường rò `redact` sinh ra
/// để bịt, chỉ khác là lần này nó chảy qua cơ sở dữ liệu.
fn last_error() -> &'static Mutex<Option<String>> {
    static E: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    E.get_or_init(|| Mutex::new(None))
}

/// Bao nhiêu dòng `error` đã ghi kể từ lúc tiến trình bật. Chỉ tăng.
pub fn error_count() -> u64 {
    ERRORS.load(Ordering::Relaxed)
}

/// Tên sự kiện lỗi gần nhất, nếu có.
pub fn last_error_msg() -> Option<String> {
    last_error().lock().ok().and_then(|g| g.clone())
}

fn log_file() -> &'static Mutex<Option<PathBuf>> {
    static F: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(None))
}

pub fn set_log_file(path: &Path) {
    if let Some(dir) = path.parent() {
        // If this fails the first append fails too, and that path already
        // reports on stderr ("log_file_open_failed") — no silent sink.
        let _ = create_dir_all(dir);
    }
    if let Ok(mut slot) = log_file().lock() {
        *slot = Some(path.to_path_buf());
    }
}

pub fn set_level_from_name(name: &str) {
    let lvl = match name {
        "debug" => DEBUG,
        "info" => INFO,
        "warn" => WARN,
        "error" => ERROR,
        _ => return,
    };
    LEVEL.store(lvl, Ordering::Relaxed);
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn emit(level: u8, level_name: &str, msg: &str, fields: Value) {
    // Đếm TRƯỚC cửa lọc mức, và nói rõ đây là bảo hiểm chứ chưa phải hành vi
    // quan sát được: hôm nay `set_level_from_name` không nhận mức nào cao hơn
    // `error`, nên không cách nào giấu một dòng lỗi qua đường ấy — tức thứ tự
    // này chưa kiểm được từ ngoài. Nó đặt ở đây vì cái ngày ai đó thêm mức
    // `off`, thứ tự ngược lại sẽ biến một cái núm log thành cái núm làm sạch
    // bảng sức khoẻ mà không sửa gì, và không ai nhớ ra để đổi.
    if level >= ERROR {
        ERRORS.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut slot) = last_error().lock() {
            *slot = Some(msg.to_string());
        }
    }
    if level < LEVEL.load(Ordering::Relaxed) {
        return;
    }
    let mut line = json!({ "ts": now_iso(), "level": level_name, "msg": msg });
    if let (Some(obj), Value::Object(extra)) = (line.as_object_mut(), fields) {
        for (k, v) in extra {
            obj.insert(k, v);
        }
    }
    // 🔴 CỬA GÁC ĐẶT Ở ĐÂY, không ở từng chỗ gọi — và đây là lần thứ hai cùng
    // một bí mật chảy ra cùng một đường.
    //
    // `redact` đã có từ 08-11, sau khi vòng đọc Telegram hỏng mạng vài lần và
    // để lại 28 dòng log mang nguyên khoá bot. Nhưng nó là thứ CHỖ GỌI phải
    // nhớ bọc — nên hôm nay, 08-14, một nhánh mới (`Inbox::react`) quên bọc, và
    // token lại nằm nguyên trong `logs/hub.log`:
    //   telegram_reaction_failed err="error sending request for url
    //   (https://api.telegram.org/bot<token>/setMessageReaction)"
    // Chính tôi viết nhánh ấy sáng nay, và chính tôi đã ghi cái bài học "đặt
    // luật ở NGUỒN, vì chỗ gọi thứ mười ba sẽ quên" ba lần trong repo này.
    //
    // Mọi dòng log đều đi qua đúng một hàm — nên gác ở đây thì không nhánh nào
    // quên được nữa, kể cả nhánh viết sau. `redact` chỉ làm việc thật khi chuỗi
    // có `/bot`, nên cái giá là một lần `find` cho mỗi dòng.
    let text = redact(&line.to_string());

    // Every level goes to stderr: stdout is the DATA channel. `hub sessions
    // --json` and `portal-push --dry-run` are meant to be piped into a parser,
    // and a stray `hub_env_loaded` line on stdout breaks them (it did, the
    // first time `--json` was piped, 2026-08-08). Nothing reads these lines
    // from stdout — the console reads the log FILE, and hubd redirects both
    // streams into one file.
    eprintln!("{text}");

    let path = log_file().lock().ok().and_then(|g| g.clone());
    if let Some(path) = path {
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(mut f) => {
                if let Err(e) = writeln!(f, "{text}") {
                    eprintln!(
                        "{{\"level\":\"error\",\"msg\":\"log_file_write_failed\",\"err\":\"{e}\"}}"
                    );
                }
            }
            // Losing the file sink must still be visible on stderr.
            Err(e) => eprintln!(
                "{{\"level\":\"error\",\"msg\":\"log_file_open_failed\",\"err\":\"{e}\"}}"
            ),
        }
    }
}

pub fn debug(msg: &str, fields: Value) {
    emit(DEBUG, "debug", msg, fields);
}
pub fn info(msg: &str, fields: Value) {
    emit(INFO, "info", msg, fields);
}
pub fn warn(msg: &str, fields: Value) {
    emit(WARN, "warn", msg, fields);
}
pub fn error(msg: &str, fields: Value) {
    emit(ERROR, "error", msg, fields);
}

/// Flatten an error chain into one loggable string.
/// Bỏ bí mật ra khỏi một câu lỗi TRƯỚC KHI nó vào log.
///
/// 🔴 Đo 2026-08-11: `reqwest` dựng câu lỗi bằng cách in NGUYÊN CẢ URL, và URL
/// của Telegram mang token trong đường dẫn (`/bot<token>/getUpdates`). Vòng đọc
/// mới hỏng mạng vài lần là **28 dòng log mang nguyên khoá bot**, trong một tệp
/// nằm lâu trên đĩa — đúng thứ luật 4 của dự án cấm ("log TÊN khoá, không bao
/// giờ giá trị"). Một câu lỗi không phải chỗ miễn trừ: nó là chỗ dễ quên nhất.
///
/// Cắt theo HÌNH DẠNG, không theo danh sách khoá đã biết: `/bot<gì đó>/` trong
/// một URL telegram thì luôn là token, kể cả token sau này đổi.
pub fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("/bot") {
        out.push_str(&rest[..i + 4]);
        let after = &rest[i + 4..];
        // Token chạy tới dấu `/` kế tiếp (hoặc hết chuỗi).
        let end = after.find('/').unwrap_or(after.len());
        if end > 0 {
            out.push_str("<token>");
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

pub fn err_chain(e: &anyhow::Error) -> String {
    let mut parts = vec![e.to_string()];
    let mut src = e.source();
    while let Some(s) = src {
        parts.push(s.to_string());
        src = s.source();
    }
    redact(&parts.join(" | "))
}
