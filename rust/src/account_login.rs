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
//! · mã có `#` mà sai: `Login failed: Request failed with status code 400`, rồi CLI THOÁT.
//! · mã THIẾU `#` (lượt thật của Hà 01:05Z): `Invalid code. Please make sure the full code
//!   was copied.` — và CLI **KHÔNG thoát**, vẫn chờ mã khác, KHÔNG in lại dấu nhắc. Bản đầu
//!   đọc dòng cuối màn để biết "còn chờ không", nên tưởng CLI đã xong, gõ `exit` (CLI nhận
//!   làm mã thứ hai), đóng không được ⟹ cửa sổ bị ẨN cùng một `auth login` mồ côi, và câu
//!   báo về nói "đã đóng". Nay "còn chờ" chấm bằng TIẾN TRÌNH trên tty ([`cli_pid`]).
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

/// Mã người dán có lẫn câu chữ thừa không — mã OAuth là MỘT chuỗi liền, không dấu cách,
/// và có `#` (đo 11/10: thiếu `#` thì CLI từ chối ngay *"Invalid code … full code"*).
pub fn code_shape_ok(code: &str) -> bool {
    let c = code.trim();
    c.len() >= 8 && c.contains('#') && !c.chars().any(char::is_whitespace)
}

/// Câu trả lời khi mã người dán chắc chắn hỏng — `None` = hình dạng được, đem gõ. THUẦN.
pub fn code_shape_why(name: &str, code: &str) -> Option<String> {
    let c = code.trim();
    if c.starts_with("http://") || c.starts_with("https://") {
        return Some(format!(
            "⚠ đó là một ĐƯỜNG DẪN, không phải mã. Mở đường dẫn đăng nhập, đăng nhập xong trang \
             hiện ra một MÃ (có dấu #) — chép mã ấy rồi gửi: /accounts {name} -l <mã>"
        ));
    }
    (!code_shape_ok(c)).then(|| {
        format!(
            "⚠ mã trông không đủ: mã đăng nhập là MỘT chuỗi liền, ≥ 8 ký tự, CÓ dấu # ở giữa \
             (thiếu phần sau # thì CLI từ chối). Chép ĐỦ rồi gửi: /accounts {name} -l <mã>"
        )
    })
}

/// pid của `claude … auth login` trong bảng `ps -t <tty> -o pid=,command=`. Hàm THUẦN.
pub fn login_pid(ps_out: &str) -> Option<i32> {
    ps_out
        .lines()
        .find(|l| l.contains(" auth login"))
        .and_then(|l| l.split_whitespace().next())
        .and_then(|p| p.parse().ok())
}

/// Chữ CLI in THÊM sau lượt gõ: phần sau dấu nhắc của màn sau, bỏ phần đã có từ trước
/// (CLI không in lại dấu nhắc sau `Invalid code`, nên các câu cũ nằm cùng một khúc). THUẦN.
pub fn new_after(truoc: &str, sau: &str) -> String {
    let a = after_prompt(truoc);
    let b = after_prompt(sau);
    match b.strip_prefix(a.as_str()) {
        Some(r) => r.trim_start_matches(" · ").trim().to_string(),
        None => b,
    }
}

/// Kết cục MỘT lượt gõ mã.
#[derive(Debug, PartialEq, Eq)]
pub enum Ket {
    /// CLI trả lời mà VẪN CHẠY — nó còn chờ mã khác (đo: `Invalid code …`).
    ChuaNhan(String),
    /// CLI đã thoát kèm lời hỏng (`Login failed …`).
    Hong(String),
    /// CLI đã thoát, lời không mang dấu hỏng — báo NGUYÊN VĂN, không tự xưng "thành công".
    Xong(String),
    /// CLI vẫn chạy và chưa in gì.
    ChuaNoi,
}

/// Chấm kết cục bằng TIẾN TRÌNH + chữ mới, không bằng dòng cuối màn. Hàm THUẦN.
pub fn ket_cuc(moi: &str, cli_con_song: bool) -> Ket {
    let l = moi.to_lowercase();
    let hong = l.contains("fail") || l.contains("invalid") || l.contains("error");
    match (cli_con_song, moi.is_empty(), hong) {
        (true, true, _) => Ket::ChuaNoi,
        (true, false, _) => Ket::ChuaNhan(moi.to_string()),
        (false, _, true) => Ket::Hong(moi.to_string()),
        (false, _, false) => Ket::Xong(if moi.is_empty() {
            "(CLI thoát, không in gì)".to_string()
        } else {
            moi.to_string()
        }),
    }
}

/// pid của `auth login` đang chạy trên `tty` — `None` = CLI đã thoát (hoặc hỏi `ps` hỏng).
fn cli_pid(tty: &str) -> Option<i32> {
    let t = tty.trim_start_matches("/dev/");
    let out = crate::exec::run(
        "ps",
        &["-t", t, "-o", "pid=,command="],
        crate::exec::RunOpts {
            timeout: Some(Duration::from_secs(5)),
            ..Default::default()
        },
    )
    .ok()?;
    login_pid(&out.stdout)
}

/// Đóng cửa sổ shell (CLI đã thoát) và nói ĐÚNG cái đã xảy ra: đóng ≠ chỉ ẩn được.
fn dong_cua_so(name: &str, w: i64) -> String {
    match crate::keys::exit_and_close_shell(w, Duration::from_secs(5)) {
        Ok(crate::keys::Closed::Gone) => "đã đóng cửa sổ".to_string(),
        Ok(crate::keys::Closed::Hidden) => {
            format!("⚠ cửa sổ {w} không đóng được, chỉ ẨN được (còn trong menu Window)")
        }
        Err(e) => {
            logging::warn(
                "account_login_window_not_closed",
                json!({ "account": name, "window": w, "err": logging::err_chain(&e) }),
            );
            format!("⚠ chưa đóng được cửa sổ {w}: {e}")
        }
    }
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
    if let Some((w, at, tty)) = pending(db, name) {
        let tuoi = chrono::Utc::now().timestamp_millis() - at;
        let man = crate::keys::screen_text(w).unwrap_or_default();
        if tuoi < LOGIN_STALE_MS && la_man_dang_nhap(&man) && cli_pid(&tty).is_some() {
            if let Some(url) = login_url(&man) {
                logging::info(
                    "account_login_reminded",
                    json!({ "account": name, "window": w, "tuoi_ms": tuoi }),
                );
                return guide(name, email.as_deref(), &url, w, true);
            }
        }
        cancel(db, name, w, &tty, "lượt -l mới thay cửa sổ cũ");
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
    // `email` = hòm thư điền sẵn — lượt gửi mã đối chiếu với tài khoản THẬT vừa vào.
    let ghi = json!({ "window": w, "tty": tty, "at_ms": chrono::Utc::now().timestamp_millis(),
                      "email": email });
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

/// Gõ mã người dán vào đúng cửa sổ đang chờ, đọc lại kết quả; CLI đã thoát thì đo lại
/// và đóng cửa sổ, còn chờ thì để nguyên và nói ra.
pub fn finish(cfg: &Config, db: &Db, name: &str, code: &str) -> String {
    if !cfg
        .claude_accounts_or_ambient()
        .iter()
        .any(|a| a.name == name)
    {
        return unknown(cfg, name);
    }
    if let Some(why) = code_shape_why(name, code) {
        return why;
    }
    let Some((w, _, tty)) = pending(db, name) else {
        return format!(
            "⚠ không có cửa sổ đăng nhập nào đang chờ mã cho {name}. Mở lại: /accounts {name} -l"
        );
    };
    let truoc = crate::keys::screen_text(w).unwrap_or_default();
    if !la_man_dang_nhap(&truoc) || cli_pid(&tty).is_none() {
        forget(db, name);
        logging::warn(
            "account_login_not_waiting",
            json!({ "account": name, "window": w, "tty": tty }),
        );
        return format!(
            "⚠ cửa sổ {w} không còn chờ mã (đã đóng, hoặc CLI đã thoát). Mở lại: /accounts {name} -l"
        );
    }
    if let Err(e) = crate::keys::type_into(w, code.trim()) {
        logging::error(
            "account_login_type_failed",
            json!({ "account": name, "window": w, "err": logging::err_chain(&e) }),
        );
        return format!("⚠ không gõ được mã vào cửa sổ {w}: {e}");
    }
    let mut moi = String::new();
    let mut song = true;
    for _ in 0..30 {
        std::thread::sleep(Duration::from_secs(1));
        moi = new_after(&truoc, &crate::keys::screen_text(w).unwrap_or_default());
        song = cli_pid(&tty).is_some();
        if !song {
            break;
        }
        if !moi.is_empty() {
            // Đã trả lời mà còn chạy — cho thêm 3 s để thoát hẳn trước khi chấm "còn chờ".
            for _ in 0..3 {
                std::thread::sleep(Duration::from_secs(1));
                if cli_pid(&tty).is_none() {
                    song = false;
                    break;
                }
            }
            moi = new_after(&truoc, &crate::keys::screen_text(w).unwrap_or_default());
            break;
        }
    }
    let ket = ket_cuc(&moi, song);
    logging::info(
        "account_login_code_sent",
        json!({ "account": name, "window": w, "cli_con_song": song,
                "cli": crate::exec::truncate(&moi, 200), "ket": format!("{ket:?}").split('(').next() }),
    );
    match ket {
        Ket::ChuaNhan(t) => format!(
            "⚠ {name} — CLI CHƯA nhận mã: «{}»\nCLI vẫn đang chờ trong cửa sổ {w}. Chép ĐỦ mã \
             (có dấu #) rồi gửi: /accounts {name} -l <mã>",
            crate::exec::truncate(&t, 300)
        ),
        Ket::ChuaNoi => format!(
            "⏳ đã gõ mã vào cửa sổ {w} nhưng sau 30 s CLI chưa in gì và vẫn chạy. Cửa sổ để \
             nguyên; xem: /accounts {name}"
        ),
        Ket::Hong(t) => {
            let dong = dong_cua_so(name, w);
            forget(db, name);
            format!(
                "⚠ {name} — CLI nói: «{}»\n{dong}\nThử lại: /accounts {name} -l",
                crate::exec::truncate(&t, 300)
            )
        }
        Ket::Xong(t) => {
            let mong = pending_email(db, name);
            let dong = dong_cua_so(name, w);
            forget(db, name);
            let usage = crate::runtime::usage_lam_moi_mot(cfg, db, name, 0);
            let row = usage.get("accounts").and_then(|m| m.get(name));
            let do_lai =
                crate::runtime::do_lai_line(row, None, chrono::Utc::now().timestamp_millis())
                    .unwrap_or_else(|| "đo lại /usage: không có kết quả\n".to_string());
            // «Login successful» chỉ nói CLI đã nhận mã, không nói ĐÃ VÀO TÀI KHOẢN NÀO —
            // đọc lại sổ của chính tài khoản ấy và của mọi tài khoản khác.
            let ids = danh_tinh_tat_ca(cfg);
            let moi = ids
                .iter()
                .find(|(n, _)| n == name)
                .and_then(|(_, e)| e.clone());
            let trung = moi
                .as_deref()
                .map(|m| trung_voi(&ids, name, m))
                .unwrap_or_default();
            let khop = match (mong.as_deref(), moi.as_deref()) {
                (Some(a), Some(b)) => Some(a.eq_ignore_ascii_case(b)),
                _ => None,
            };
            // Không ghi email vào log — chỉ cờ và tên tài khoản.
            let ghi = if khop == Some(false) || !trung.is_empty() || moi.is_none() {
                logging::warn
            } else {
                logging::info
            };
            ghi(
                "account_login_identity",
                json!({ "account": name, "khop_dien_san": khop, "co_email": moi.is_some(),
                        "trung": trung }),
            );
            let ai = canh_bao_danh_tinh(name, mong.as_deref(), moi.as_deref(), &trung);
            format!(
                "🔑 {name} — CLI nói: «{}»\n{ai}{do_lai}{dong}",
                crate::exec::truncate(&t, 300)
            )
        }
    }
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

/// Hòm thư đã điền sẵn (`--email`) lúc mở lượt đăng nhập đang chờ — `None` khi không điền.
fn pending_email(db: &Db, name: &str) -> Option<String> {
    let v: Value = serde_json::from_str(&db.cursor_or_log(&cursor_key(name))?).ok()?;
    v.get("email")?
        .as_str()
        .filter(|e| !e.is_empty())
        .map(str::to_string)
}

/// Email của MỌI tài khoản khai trong cấu hình, đọc từ sổ `.claude.json` của từng cái.
///
/// 🔑 Tài khoản chạy bằng API key thì trả NHÃN key (`API key …WXYZ`) thay cho email: sổ
/// `.claude.json` của nó vẫn còn đăng nhập gói cũ (acc5 21/10: trogiup, trùng acc3) mà
/// CLI không dùng nữa — in email ấy là nói sai tài khoản đang chạy, và báo trùng oan.
pub fn danh_tinh_tat_ca(cfg: &Config) -> Vec<(String, Option<String>)> {
    cfg.claude_accounts_or_ambient()
        .iter()
        .map(|a| {
            if let Some(nhan) = crate::api_key::nhan(&crate::api_key::thu_muc(a)) {
                return (a.name.clone(), Some(nhan));
            }
            let dir = a
                .config_dir
                .as_deref()
                .filter(|d| !d.is_empty())
                .map(|d| crate::config::expand_home(Path::new(d)));
            (
                a.name.clone(),
                crate::quota::identity(dir.as_deref())
                    .ok()
                    .and_then(|i| i.email),
            )
        })
        .collect()
}

/// Các tài khoản KHÁC `ten` đang đăng nhập cùng `email` (không phân biệt hoa thường). Hàm THUẦN.
pub fn trung_voi(ids: &[(String, Option<String>)], ten: &str, email: &str) -> Vec<String> {
    ids.iter()
        .filter(|(n, e)| n != ten && e.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(email)))
        .map(|(n, _)| n.clone())
        .collect()
}

/// Các nhóm ≥2 tài khoản cùng một email — `(email, [tên…])`, theo thứ tự khai. Hàm THUẦN.
pub fn nhom_trung(ids: &[(String, Option<String>)]) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for (n, e) in ids {
        let Some(e) = e.as_deref().filter(|e| !e.is_empty()) else {
            continue;
        };
        match out.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(e)) {
            Some((_, v)) => v.push(n.clone()),
            None => out.push((e.to_string(), vec![n.clone()])),
        }
    }
    out.retain(|(_, v)| v.len() > 1);
    out
}

/// Dòng cảnh báo trùng cho `/accounts` — `None` khi không có nhóm trùng nào. Hàm THUẦN.
///
/// 🔴 Đo 2026-10-19 (Hà: *"Tại sao acc3 và acc5 lại là 1"*): `~/.claude-acc3` và
/// `~/.claude-acc5` cùng `accountUuid 29829ac3` — một tài khoản, một hạn mức. Hai hàng
/// in số y hệt nhau, và không câu nào nói vì sao.
pub fn trung_line(nhom: &[(String, Vec<String>)]) -> Option<String> {
    if nhom.is_empty() {
        return None;
    }
    let mut s = String::new();
    for (email, ten) in nhom {
        s.push_str(&format!(
            "⚠ {} đăng nhập CÙNG MỘT tài khoản ({email}) — chung hạn mức: số của hàng này là \
             số của hàng kia. Cái nào đăng nhập nhầm thì: /accounts <tên> -l\n",
            ten.join(" · ")
        ));
    }
    Some(s)
}

/// Câu thêm vào sau một lượt «Login successful» — LUÔN nói tài khoản vừa vào là ai,
/// và kêu khi nó không phải người đã điền sẵn hoặc trùng tài khoản khác. Hàm THUẦN.
///
/// 🔴 Đo 2026-10-19: lượt `/accounts acc5 -l` ngày 13/10 00:20Z điền sẵn
/// `help.me002022@gmail.com` (`login_hint`), Hà duyệt trên điện thoại, CLI in
/// «Login successful.» — và acc5 nay là `trogiup.gdk@gmail.com`, đúng tài khoản của
/// acc3. `login_hint` chỉ là GỢI Ý: trang dùng tài khoản đang mở trên trình duyệt.
/// huba đáp *"CLI nói: «Login successful.»"* rồi thôi, nên sáu ngày không ai biết.
pub fn canh_bao_danh_tinh(
    ten: &str,
    mong: Option<&str>,
    moi: Option<&str>,
    trung: &[String],
) -> String {
    let Some(moi) = moi.filter(|m| !m.is_empty()) else {
        return format!(
            "⚠ sổ của {ten} không ghi email nào — CHƯA xác nhận được {ten} vừa đăng nhập vào \
             tài khoản nào. Xem: /accounts {ten}\n"
        );
    };
    let mut s = format!("tài khoản: {moi}\n");
    let sai_nguoi = mong.filter(|m| !m.is_empty() && !m.eq_ignore_ascii_case(moi));
    if let Some(m) = sai_nguoi {
        s.push_str(&format!(
            "⚠ KHÔNG phải {m} như đã điền sẵn — trang đăng nhập dùng tài khoản đang mở trên \
             trình duyệt điện thoại, chữ điền sẵn chỉ là gợi ý.\n"
        ));
    }
    if !trung.is_empty() {
        s.push_str(&format!(
            "⚠ {moi} cũng đang là {0} ⟹ {ten} và {0} là MỘT tài khoản, chung hạn mức.\n",
            trung.join(" · ")
        ));
    }
    if sai_nguoi.is_some() {
        s.push_str(&format!(
            "Sửa: đăng xuất {moi} khỏi claude.ai trên trình duyệt điện thoại (hoặc mở đường dẫn \
             trong tab ẩn danh), rồi /accounts {ten} -l\n"
        ));
    } else if !trung.is_empty() {
        s.push_str(&format!(
            "Cái nào nhầm thì đăng xuất {moi} trên trình duyệt điện thoại rồi /accounts <tên> -l\n"
        ));
    }
    s
}

fn pending(db: &Db, name: &str) -> Option<(i64, i64, String)> {
    let v: Value = serde_json::from_str(&db.cursor_or_log(&cursor_key(name))?).ok()?;
    Some((
        v.get("window")?.as_i64()?,
        v.get("at_ms")?.as_i64()?,
        v.get("tty")?.as_str()?.to_string(),
    ))
}

fn forget(db: &Db, name: &str) {
    if let Err(e) = db.del_cursor(&cursor_key(name)) {
        logging::warn(
            "account_login_cursor_not_cleared",
            json!({ "account": name, "err": e.to_string() }),
        );
    }
}

/// Bỏ một lượt chờ mã: TẮT đúng tiến trình `auth login` của cửa sổ ấy (không gõ gì vào
/// nó — đo 11/10: chữ gõ vào bị nhận làm mã), rồi đóng cửa sổ shell như mọi cửa sổ khác.
fn cancel(db: &Db, name: &str, w: i64, tty: &str, vi: &str) {
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
    if let Some(pid) = cli_pid(tty) {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
        for _ in 0..6 {
            std::thread::sleep(Duration::from_millis(500));
            if cli_pid(tty).is_none() {
                break;
            }
        }
    }
    let dong = dong_cua_so(name, w);
    logging::info(
        "account_login_cancelled",
        json!({ "account": name, "window": w, "vi": vi, "dong": dong }),
    );
    forget(db, name);
}
