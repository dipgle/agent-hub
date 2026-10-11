//! `/accounts <tên> -l` — đăng nhập lại MỘT tài khoản Claude từ điện thoại.
//!
//! 🔴 Hà 2026-10-11: *"thêm cờ -l để chạy luồng login"*. Ngồi ở máy thì chủ máy gõ
//! `claude2 auth login`, trình duyệt trên máy mở trang đăng nhập, xong là xong. Từ
//! điện thoại thì trình duyệt nằm ở ĐẦU KIA: trang đăng nhập phải đi tới tay anh, và
//! mã trang ấy trả về phải đi ngược về đúng cửa sổ đang chờ. huba làm đúng hai chặng
//! chuyển ấy, không hơn — phép thử cầu nối: cửa sổ Terminal là cửa sổ THẬT, mở ra là
//! thấy, đóng là đóng.
//!
//! 📐 Đo 2026-10-11 trên CLI 2.1.280, thư mục cấu hình TRỐNG (không đụng tài khoản
//! thật nào), trong một cửa sổ Terminal thật:
//! · `claude auth login` gọi `open` với một đường dẫn có `redirect_uri=http://localhost:<cổng>/callback`
//!   (đường TỰ ĐỘNG — chỉ trình duyệt TRÊN MÁY gọi ngược về được), rồi IN RA màn:
//!   `If the browser didn't open, visit: https://claude.com/cai/oauth/authorize?…redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback…`
//!   — đường THỨ HAI, trang hiện mã để chép tay. Đó là đường cho điện thoại.
//! · rồi đứng ở `Paste code here if prompted >`.
//! · `contents` của tab trả đường dẫn trên MỘT dòng logic (500 ký tự, cửa sổ 206 cột).
//! · gõ bằng `do script` (chữ + xuống dòng một lượt) thì ô ấy NHẬN và GỬI ngay — khác
//!   ô nhập của phiên `claude`, nơi một lượt như thế bị đọc thành dán.
//! · mã sai: `Login failed: Request failed with status code 400`, rồi CLI thoát về shell.
//! · câu của lượt THÀNH CÔNG chưa đo được (muốn thấy phải đăng nhập thật) — nên kết quả
//!   báo về là NGUYÊN VĂN dòng sau ô dán mã + một lượt đo `/usage` mới, không đoán chữ.

use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use crate::config::{ClaudeAccountCfg, Config};
use crate::db::Db;
use crate::logging;

/// Dấu nhắc của ô dán mã — nguyên văn CLI 2.1.280.
pub const PASTE_PROMPT: &str = "Paste code here if prompted >";

/// Cửa sổ đăng nhập chờ mã lâu hơn chừng này thì coi là bỏ dở: lượt `-l` sau mở mới.
pub const LOGIN_STALE_MS: i64 = 15 * 60 * 1000;

fn cursor_key(name: &str) -> String {
    format!("login:{name}")
}

/// Dòng lệnh gõ vào cửa sổ mới. Hàm THUẦN.
///
/// Tài khoản có `config_dir` ⟹ đặt `CLAUDE_CONFIG_DIR`; tài khoản mặc định ⟹ `env -u`
/// biến ấy, phòng hồ sơ shell có đặt sẵn (mặc định = biến VẮNG MẶT, xem `account_env`).
/// `--email` điền sẵn đúng hòm thư lên trang đăng nhập, để đăng nhập nhầm tài khoản
/// khó xảy ra hơn.
pub fn login_command(cfg: &Config, acc: &ClaudeAccountCfg, email: Option<&str>) -> String {
    let q = crate::sessions::shell_quote;
    let goc = match acc
        .config_dir
        .as_deref()
        .filter(|d| !d.is_empty())
        .map(|d| crate::config::expand_home(Path::new(d)))
    {
        Some(dir) => format!(
            "CLAUDE_CONFIG_DIR={} {}",
            q(&dir.to_string_lossy()),
            q(&cfg.claude_cli)
        ),
        None => format!("env -u CLAUDE_CONFIG_DIR {}", q(&cfg.claude_cli)),
    };
    match email.filter(|e| !e.is_empty()) {
        Some(e) => format!("{goc} auth login --email {}", q(e)),
        None => format!("{goc} auth login"),
    }
}

/// Đường dẫn đăng nhập in trên màn — đường "trang hiện mã", không phải đường localhost.
/// Lấy lần `visit:` CUỐI. Hàm THUẦN.
pub fn login_url(screen: &str) -> Option<String> {
    let (_, sau) = screen.rsplit_once("visit:")?;
    let url: String = sau
        .trim_start()
        .chars()
        .take_while(|c| !c.is_whitespace())
        .collect();
    url.starts_with("https://").then_some(url)
}

/// Ô dán mã đang CHỜ — dòng có dấu nhắc là dòng chữ cuối và sau `>` chưa có gì.
pub fn waiting_for_code(screen: &str) -> bool {
    screen
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .is_some_and(|l| l.trim_end().ends_with(PASTE_PROMPT))
}

/// Những gì CLI in SAU ô dán mã (lượt gõ mã), gọn về một dòng. Hàm THUẦN.
pub fn after_prompt(screen: &str) -> String {
    let Some((_, sau)) = screen.rsplit_once(PASTE_PROMPT) else {
        return String::new();
    };
    // Dòng dấu nhắc shell (`… % `) là chỗ CLI đã thoát — không phải lời của nó.
    sau.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.ends_with('%') && !l.ends_with('$'))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Màn này có đúng là cửa sổ `auth login` của huba không — CHỐT trước mọi lượt gõ / đóng.
///
/// 🔴 id cửa sổ Terminal được đánh LẠI từ số nhỏ sau khi Terminal khởi động lại (xem
/// `keys::tab_proc_count`), nên id nhớ trong sổ có thể đã thuộc về một cửa sổ khác —
/// gõ `exit` hay một mã vào đó là gõ vào việc của người khác. Dòng lệnh huba gõ lúc mở
/// (`… auth login`) và dòng `visit:` của CLI cùng nằm trên màn thì mới là của mình.
pub fn la_man_dang_nhap(screen: &str) -> bool {
    screen.contains(" auth login") && screen.contains("visit:")
}

/// Mã người dán có lẫn câu chữ thừa không — mã OAuth là MỘT chuỗi liền, không dấu cách.
pub fn code_shape_ok(code: &str) -> bool {
    let c = code.trim();
    c.len() >= 8 && !c.chars().any(char::is_whitespace)
}

/// Bắt đầu (hoặc nhắc lại) một lượt đăng nhập — trả câu cho điện thoại.
pub fn start(cfg: &Config, db: &Db, name: &str) -> String {
    let Some(acc) = cfg
        .claude_accounts_or_ambient()
        .into_iter()
        .find(|a| a.name == name)
    else {
        return unknown(cfg, name);
    };
    let dir = acc
        .config_dir
        .as_deref()
        .filter(|d| !d.is_empty())
        .map(|d| crate::config::expand_home(Path::new(d)));
    let email = crate::quota::identity(dir.as_deref())
        .ok()
        .and_then(|i| i.email);

    // Đã có cửa sổ đang chờ mã cho tài khoản này ⟹ nhắc lại đường dẫn, KHÔNG mở cái thứ hai.
    if let Some((w, at)) = pending(db, name) {
        let tuoi = chrono::Utc::now().timestamp_millis() - at;
        let man = crate::keys::screen_text(w).unwrap_or_default();
        if tuoi < LOGIN_STALE_MS && waiting_for_code(&man) {
            if let Some(url) = login_url(&man) {
                logging::info(
                    "account_login_reminded",
                    json!({ "account": name, "window": w, "tuoi_ms": tuoi }),
                );
                return guide(name, email.as_deref(), &url, w, true);
            }
        }
        cancel(db, name, w, "lượt -l mới thay cửa sổ cũ");
    }

    let cmd = login_command(cfg, &acc, email.as_deref());
    let (w, tty) = match crate::keys::open_window(&cmd) {
        Ok(x) => x,
        Err(e) => {
            logging::error(
                "account_login_window_failed",
                json!({ "account": name, "err": logging::err_chain(&e) }),
            );
            return format!("⚠ không mở được cửa sổ Terminal để đăng nhập {name}: {e}");
        }
    };
    let mut man = String::new();
    for _ in 0..20 {
        std::thread::sleep(Duration::from_secs(1));
        man = crate::keys::screen_text(w).unwrap_or_default();
        if waiting_for_code(&man) && login_url(&man).is_some() {
            break;
        }
    }
    let Some(url) = login_url(&man).filter(|_| waiting_for_code(&man)) else {
        let cuoi: Vec<&str> = man.lines().filter(|l| !l.trim().is_empty()).collect();
        let cuoi = cuoi[cuoi.len().saturating_sub(3)..].join(" · ");
        logging::warn(
            "account_login_no_url",
            json!({ "account": name, "window": w, "tty": tty, "man_cuoi": cuoi }),
        );
        return format!(
            "⚠ đã mở cửa sổ {w} chạy `auth login` cho {name} nhưng sau 20 s màn chưa có đường dẫn \
             đăng nhập + ô dán mã. Màn cuối: «{}»",
            crate::exec::truncate(&cuoi, 300)
        );
    };
    let ghi = json!({ "window": w, "tty": tty, "at_ms": chrono::Utc::now().timestamp_millis() });
    if let Err(e) = db.set_cursor(&cursor_key(name), &ghi.to_string()) {
        logging::error(
            "account_login_cursor_not_saved",
            json!({ "account": name, "err": e.to_string(),
                    "effect": "gửi mã về sẽ không tìm ra cửa sổ này" }),
        );
    }
    logging::info(
        "account_login_started",
        json!({ "account": name, "window": w, "tty": tty, "email_dien_san": email.is_some() }),
    );
    guide(name, email.as_deref(), &url, w, false)
}

/// Gõ mã người dán vào đúng cửa sổ đang chờ, đọc lại kết quả, đo lại, đóng cửa sổ.
pub fn finish(cfg: &Config, db: &Db, name: &str, code: &str) -> String {
    if !cfg
        .claude_accounts_or_ambient()
        .iter()
        .any(|a| a.name == name)
    {
        return unknown(cfg, name);
    }
    if !code_shape_ok(code) {
        return format!(
            "⚠ mã trông không đúng (mã đăng nhập là MỘT chuỗi liền, ≥ 8 ký tự). Gửi lại: \
             /accounts {name} -l <mã>"
        );
    }
    let Some((w, _)) = pending(db, name) else {
        return format!(
            "⚠ không có cửa sổ đăng nhập nào đang chờ mã cho {name}. Mở lại: /accounts {name} -l"
        );
    };
    let man = crate::keys::screen_text(w).unwrap_or_default();
    if !la_man_dang_nhap(&man) || !waiting_for_code(&man) {
        forget(db, name);
        logging::warn(
            "account_login_not_waiting",
            json!({ "account": name, "window": w }),
        );
        return format!(
            "⚠ cửa sổ {w} không còn đứng ở ô dán mã (đã đóng, hoặc CLI đã thoát). Mở lại: \
             /accounts {name} -l"
        );
    }
    if let Err(e) = crate::keys::type_into(w, code.trim()) {
        logging::error(
            "account_login_type_failed",
            json!({ "account": name, "window": w, "err": logging::err_chain(&e) }),
        );
        return format!("⚠ không gõ được mã vào cửa sổ {w}: {e}");
    }
    // Chờ CLI xong việc: ô dán mã biến mất khỏi dòng cuối và có chữ mới sau nó.
    let mut man = String::new();
    for _ in 0..30 {
        std::thread::sleep(Duration::from_secs(1));
        man = crate::keys::screen_text(w).unwrap_or_default();
        if !waiting_for_code(&man) && !after_prompt(&man).is_empty() {
            break;
        }
    }
    let cli_noi = after_prompt(&man);
    // Đo 11/10: mã sai ⟹ `Login failed: …`. Câu của lượt thành công chưa đo được, nên
    // chỉ chữ `fail` đổi dấu — phần còn lại báo NGUYÊN VĂN, không đoán.
    let hong = cli_noi.to_lowercase().contains("fail");
    logging::info(
        "account_login_code_sent",
        json!({ "account": name, "window": w, "cli": crate::exec::truncate(&cli_noi, 200),
                "hong": hong }),
    );
    if cli_noi.is_empty() {
        // CLI chưa nói gì sau 30 s — có thể còn đang đổi mã. Gõ `exit` lúc này là gõ vào
        // chính CLI ấy; để nguyên cửa sổ và sổ, nói ra.
        return format!(
            "⏳ đã gõ mã vào cửa sổ {w} nhưng sau 30 s CLI chưa in gì. Cửa sổ để nguyên; xem: \
             /accounts {name}"
        );
    }
    let dong = match crate::keys::exit_and_close_shell(w, Duration::from_secs(5)) {
        Ok(_) => "đã đóng cửa sổ".to_string(),
        Err(e) => {
            logging::warn(
                "account_login_window_not_closed",
                json!({ "account": name, "window": w, "err": logging::err_chain(&e) }),
            );
            format!("⚠ chưa đóng được cửa sổ {w}: {e}")
        }
    };
    forget(db, name);
    let usage = crate::runtime::usage_lam_moi_mot(cfg, db, name, 0);
    let row = usage.get("accounts").and_then(|m| m.get(name));
    let do_lai = crate::runtime::do_lai_line(row, None, chrono::Utc::now().timestamp_millis())
        .unwrap_or_else(|| "đo lại /usage: không có kết quả\n".to_string());
    format!(
        "{} {name} — CLI nói: «{}»\n{do_lai}{dong}{}",
        if hong { "⚠" } else { "🔑" },
        crate::exec::truncate(&cli_noi, 300),
        if hong {
            format!("\nThử lại: /accounts {name} -l")
        } else {
            String::new()
        }
    )
}

/// Câu dẫn đường gửi về điện thoại. Hàm THUẦN.
pub fn guide(name: &str, email: Option<&str>, url: &str, w: i64, nhac_lai: bool) -> String {
    let ai = match email {
        Some(e) => e.to_string(),
        None => format!("tài khoản {name}"),
    };
    format!(
        "🔑 Đăng nhập lại {name}{}\n\
         1. Mở đường dẫn này trên điện thoại:\n{url}\n\
         2. Đăng nhập ĐÚNG {ai}, rồi chép mã trang hiện ra.\n\
         3. Gửi: /accounts {name} -l <mã>\n\
         (cửa sổ Terminal {w} trên máy đang chờ mã)",
        if nhac_lai {
            " (nhắc lại — cửa sổ cũ vẫn đang chờ)"
        } else {
            ""
        },
    )
}

fn unknown(cfg: &Config, name: &str) -> String {
    format!(
        "⚠ Không có tài khoản '{}'. Máy này khai: {}",
        crate::exec::truncate(name, 24),
        cfg.claude_accounts_or_ambient()
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(" · ")
    )
}

fn pending(db: &Db, name: &str) -> Option<(i64, i64)> {
    let v: Value = serde_json::from_str(&db.cursor_or_log(&cursor_key(name))?).ok()?;
    Some((v.get("window")?.as_i64()?, v.get("at_ms")?.as_i64()?))
}

fn forget(db: &Db, name: &str) {
    if let Err(e) = db.del_cursor(&cursor_key(name)) {
        logging::warn(
            "account_login_cursor_not_cleared",
            json!({ "account": name, "err": e.to_string() }),
        );
    }
}

/// Bỏ một lượt chờ mã: gõ một mã HỎNG cho CLI thoát (đo: mã sai ⟹ thoát về shell),
/// rồi đóng cửa sổ như mọi cửa sổ shell khác của huba.
fn cancel(db: &Db, name: &str, w: i64, vi: &str) {
    let man = crate::keys::screen_text(w).unwrap_or_default();
    if !la_man_dang_nhap(&man) {
        // Không phải cửa sổ của mình (đã đóng, hoặc id đã thuộc về cửa sổ khác) ⟹ KHÔNG đụng.
        logging::info(
            "account_login_forgot_foreign_window",
            json!({ "account": name, "window": w, "vi": vi }),
        );
        forget(db, name);
        return;
    }
    if waiting_for_code(&man) {
        let _ = crate::keys::type_into(w, "huy");
        std::thread::sleep(Duration::from_secs(3));
    }
    let ket = crate::keys::exit_and_close_shell(w, Duration::from_secs(5));
    logging::info(
        "account_login_cancelled",
        json!({ "account": name, "window": w, "vi": vi,
                "dong": ket.as_ref().map(|_| "ok").unwrap_or("hong"),
                "err": ket.err().map(|e| logging::err_chain(&e)) }),
    );
    forget(db, name);
}
