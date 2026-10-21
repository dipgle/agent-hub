//! `/accounts <tên> -k <key>` — cho MỘT tài khoản Claude chạy bằng API key thay cho
//! gói đăng nhập (Hà 2026-10-21: *"Tôi có api key và muốn dùng cho acc5"* → *"Tạo
//! thêm lệnh chuyển sang sử dụng api"*). `-k xoa` trả nó về gói.
//!
//! 📐 Đo 2026-10-21 trên mã CLI 2.1.280 (`function d_` · `Jc` · `Wc` trong
//! `claude.exe`) và trên một thư mục cấu hình tạm:
//! · `ANTHROPIC_API_KEY` trong môi trường: phiên TƯƠNG TÁC chỉ dùng khi key đã được
//!   duyệt trong `customApiKeyResponses`, chưa duyệt thì CLI HỎI lúc khởi động — một
//!   cửa sổ huba mở sẽ đứng ở hộp ấy. Không dùng đường này.
//! · `apiKeyHelper` trong `<thư mục cấu hình>/settings.json`: thắng cả đăng nhập gói
//!   đang có trong thư mục ấy, không hỏi gì. `claude auth status` đọc ra
//!   `"authMethod": "api_key_helper"`, `"apiKeySource": "apiKeyHelper"` (mã 0); bỏ
//!   helper ⟹ `loggedIn: false`, mã 1. Đây là đường huba dùng.
//! · `auth status` KHÔNG kiểm key với máy chủ — một key GIẢ vẫn qua. Nên key được hỏi
//!   máy chủ TRƯỚC khi cất ([`kiem_voi_may_chu`], `GET /v1/models`: không tốn token).
//!
//! Vì sao một tệp quyền 600 trong chính thư mục cấu hình mà không phải Keychain: cùng
//! mức bảo vệ với `huba.env` (luật 4) và `.claude.json` của CLI, thử được bằng thư mục
//! tạm, và đi theo thư mục tài khoản — xoá tài khoản là xoá cả key.
//!
//! Ba điều mô-đun này KHÔNG làm, mỗi điều là một luật:
//! · không ghi key vào log, vào câu trả lời, vào lỗi — chỉ 4 ký tự cuối ([`duoi`]);
//! · không cất key khi máy chủ không xác nhận được (sai hay không kiểm được đều dừng);
//! · không để thư mục ở trạng thái nửa vời: CLI không xác nhận ⟹ trả nguyên byte cũ.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::config::{ClaudeAccountCfg, Config};
use crate::logging;

/// Tên tệp key huba cất trong thư mục cấu hình của tài khoản.
pub const TEP_KEY: &str = "huba-api-key";

/// Thư mục cấu hình của tài khoản: `config_dir`, hoặc `~/.claude` cho tài khoản mặc
/// định (không `config_dir` ⟹ CLI chạy khi KHÔNG có `CLAUDE_CONFIG_DIR`).
pub fn thu_muc(acc: &ClaudeAccountCfg) -> PathBuf {
    match acc.config_dir.as_deref().filter(|d| !d.trim().is_empty()) {
        Some(d) => crate::config::expand_home(Path::new(d)),
        None => crate::config::expand_home(Path::new("~/.claude")),
    }
}

/// Dòng `apiKeyHelper` huba ghi: đọc tệp key của chính thư mục ấy.
pub fn helper_cua_huba(dir: &Path) -> String {
    format!(
        "cat {}",
        crate::sessions::shell_quote(&dir.join(TEP_KEY).to_string_lossy())
    )
}

/// `…WXYZ` — đủ để chủ máy nhận ra key nào, không đủ để dùng.
pub fn duoi(key: &str) -> String {
    let k: Vec<char> = key.trim().chars().collect();
    let tu = k.len().saturating_sub(4);
    format!("…{}", k[tu..].iter().collect::<String>())
}

/// `None` = đúng hình dạng API key của Anthropic; `Some(lý do)` = sai. Hàm THUẦN.
///
/// Cùng phép thử CLI dùng (`sk-ant-` rồi `api` ở ký tự 7–9): key admin
/// (`sk-ant-admin…`) hay mã OAuth không gọi được API hội thoại.
pub fn hinh_dang_sai(key: &str) -> Option<String> {
    let k = key.trim();
    // Bản đã che (`logging::che_api_key`) — tới đây chỉ bằng đường chạy lại từ sổ
    // `tin_den` sau một lần huba khởi động lại giữa chừng.
    if k.starts_with("sk-ant-…") {
        return Some(
            "key này là bản ĐÃ CHE trong sổ (huba khởi động lại giữa chừng) — gửi lại lệnh".into(),
        );
    }
    if k.chars().any(char::is_whitespace) {
        return Some("key có khoảng trắng ở giữa — dán thiếu hay thừa một đoạn".into());
    }
    if !k.starts_with("sk-ant-api") {
        return Some(format!(
            "không phải API key Anthropic (phải mở đầu bằng sk-ant-api, key này đuôi {})",
            duoi(k)
        ));
    }
    if k.len() < 40 {
        return Some(format!("key quá ngắn ({} ký tự) — dán thiếu", k.len()));
    }
    None
}

/// Máy chủ nói gì về một key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KiemKey {
    /// `200` — key gọi được API.
    Dung,
    /// Máy chủ TỪ CHỐI (`401`/`403`): key sai, bị thu hồi, hay không có quyền.
    Sai(String),
    /// Không hỏi được (mạng, hết giờ, mã lạ). Không phải "sai" — nhưng cũng không cất.
    KhongKiemDuoc(String),
}

/// Hỏi máy chủ Anthropic key có dùng được không — `GET /v1/models`, KHÔNG tốn token
/// (luật 8: huba không tự tiêu gì).
pub fn kiem_voi_may_chu(key: &str) -> KiemKey {
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
    {
        Ok(c) => c,
        Err(e) => return KiemKey::KhongKiemDuoc(format!("không dựng được HTTP client: {e}")),
    };
    let r = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", key.trim())
        .header("anthropic-version", "2023-06-01")
        .send();
    match r {
        Err(e) => KiemKey::KhongKiemDuoc(format!(
            "không gọi được api.anthropic.com: {}",
            // `reqwest` có thể in URL, không bao giờ in header — nhưng cắt cho chắc.
            crate::exec::truncate(&e.to_string(), 160)
        )),
        Ok(resp) => {
            let ma = resp.status().as_u16();
            let than: Value = resp.json().unwrap_or(Value::Null);
            let loi = than
                .pointer("/error/message")
                .and_then(Value::as_str)
                .map(|m| crate::exec::truncate(m, 160))
                .unwrap_or_default();
            match ma {
                200 => KiemKey::Dung,
                401 | 403 => KiemKey::Sai(format!("máy chủ trả {ma} {loi}").trim().to_string()),
                _ => KiemKey::KhongKiemDuoc(format!("máy chủ trả {ma} {loi}").trim().to_string()),
            }
        }
    }
}

/// Tài khoản này đang chạy bằng API key không — và nói bằng chữ để đặt vào chỗ email
/// trên `/accounts`. `None` = không có `apiKeyHelper` (dùng gói như thường).
///
/// Nhãn chứa đuôi key, nên hai tài khoản dùng CÙNG một key gom về cùng một nhóm trùng
/// ([`crate::account_login::nhom_trung`]) — đúng, vì chúng chung một hoá đơn.
pub fn nhan(dir: &Path) -> Option<String> {
    let helper = doc_settings(dir)
        .ok()
        .flatten()?
        .get("apiKeyHelper")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|h| !h.trim().is_empty())?;
    if helper != helper_cua_huba(dir) {
        return Some("API key (apiKeyHelper ngoài huba)".into());
    }
    Some(match std::fs::read_to_string(dir.join(TEP_KEY)) {
        Ok(k) if !k.trim().is_empty() => format!("API key {}", duoi(&k)),
        _ => format!("API key — ⚠ tệp {TEP_KEY} không đọc được, phiên sẽ không gọi được API"),
    })
}

/// `settings.json` của thư mục: `Ok(None)` = chưa có tệp; `Err` = có mà hỏng JSON.
fn doc_settings(dir: &Path) -> Result<Option<serde_json::Map<String, Value>>, String> {
    let p = dir.join("settings.json");
    match std::fs::read_to_string(&p) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("không đọc được {}: {e}", p.display())),
        Ok(s) => match serde_json::from_str::<Value>(&s) {
            Ok(Value::Object(m)) => Ok(Some(m)),
            Ok(_) => Err(format!("{} không phải một object JSON", p.display())),
            Err(e) => Err(format!("{} hỏng JSON: {e}", p.display())),
        },
    }
}

/// Ghi qua tệp tạm rồi đổi tên — một cú cắt điện không để lại tệp nửa chừng. Tệp key
/// được tạo với quyền 600 NGAY LÚC TẠO, không phải tạo xong mới hạ quyền.
fn ghi_nguyen_tu(p: &Path, noi_dung: &[u8], rieng: bool) -> Result<(), String> {
    use std::io::Write;
    let tam = p.with_extension("huba-tmp");
    // Tệp tạm sót lại từ một lượt chết giữa chừng giữ QUYỀN CŨ của nó — `mode()` chỉ
    // áp lúc TẠO. Xoá trước để tệp key luôn sinh ra với 600.
    let _ = std::fs::remove_file(&tam);
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if rieng {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = rieng;
    let mut f = o
        .open(&tam)
        .map_err(|e| format!("không tạo được {}: {e}", tam.display()))?;
    f.write_all(noi_dung)
        .and_then(|_| f.sync_all())
        .map_err(|e| format!("không ghi được {}: {e}", tam.display()))?;
    std::fs::rename(&tam, p).map_err(|e| format!("không đổi tên vào {}: {e}", p.display()))
}

/// Trạng thái cũ của hai tệp — để trả NGUYÊN BYTE khi CLI không xác nhận.
struct TruocKhi {
    settings: Option<Vec<u8>>,
    key: Option<Vec<u8>>,
}

fn chup(dir: &Path) -> TruocKhi {
    TruocKhi {
        settings: std::fs::read(dir.join("settings.json")).ok(),
        key: std::fs::read(dir.join(TEP_KEY)).ok(),
    }
}

fn tra_lai(dir: &Path, t: &TruocKhi) -> Vec<String> {
    let mut hong = Vec::new();
    for (ten, cu, rieng) in [
        ("settings.json", &t.settings, false),
        (TEP_KEY, &t.key, true),
    ] {
        let p = dir.join(ten);
        let r = match cu {
            Some(b) => ghi_nguyen_tu(&p, b, rieng),
            None => match std::fs::remove_file(&p) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("không xoá được {}: {e}", p.display())),
            },
        };
        if let Err(e) = r {
            hong.push(e);
        }
    }
    hong
}

/// `claude auth status` bằng ĐÚNG tài khoản ấy — CLI tự khai nó đang xác thực bằng gì.
pub fn auth_status(cfg: &Config, acc: &ClaudeAccountCfg) -> Result<Value, String> {
    let out = crate::exec::run(
        &cfg.claude_cli,
        &["auth", "status"],
        crate::exec::RunOpts {
            timeout: Some(Duration::from_secs(40)),
            env: crate::runtime::account_env(acc),
            ..Default::default()
        },
    )
    .map_err(|e| format!("không chạy được `claude auth status`: {e}"))?;
    if out.timed_out {
        return Err("`claude auth status` quá 40 s".into());
    }
    serde_json::from_str::<Value>(out.stdout.trim()).map_err(|_| {
        format!(
            "`claude auth status` (mã {:?}) không trả JSON: {}",
            out.code,
            crate::exec::truncate(out.stdout.trim(), 160)
        )
    })
}

fn tim(cfg: &Config, ten: &str) -> Result<ClaudeAccountCfg, String> {
    let all = cfg.claude_accounts_or_ambient();
    all.iter().find(|a| a.name == ten).cloned().ok_or_else(|| {
        format!(
            "⚠ không có tài khoản '{}'. Máy này khai: {}",
            crate::exec::truncate(ten, 24),
            all.iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(" · ")
        )
    })
}

/// `/accounts <tên> -k <key>` — kiểm key, cất, gắn `apiKeyHelper`, rồi bắt CLI xác
/// nhận. `kiem` = [`kiem_voi_may_chu`] (bài kiểm truyền bản giả). Trả câu cho chủ máy;
/// câu ấy KHÔNG BAO GIỜ chứa key.
pub fn dat(cfg: &Config, ten: &str, key: &str, kiem: &dyn Fn(&str) -> KiemKey) -> String {
    if cfg!(windows) {
        return "⚠ -k chưa làm cho Windows (apiKeyHelper ở đây dùng `cat`).".into();
    }
    let acc = match tim(cfg, ten) {
        Ok(a) => a,
        Err(m) => return m,
    };
    let key = key.trim();
    if let Some(vi_sao) = hinh_dang_sai(key) {
        logging::warn(
            "account_api_key_refused",
            json!({ "account": ten, "why": "hình dạng", "duoi": duoi(key) }),
        );
        return format!("⚠ {ten}: CHƯA đổi gì — {vi_sao}.");
    }
    match kiem(key) {
        KiemKey::Dung => {}
        KiemKey::Sai(m) => {
            logging::warn(
                "account_api_key_refused",
                json!({ "account": ten, "why": "máy chủ từ chối", "duoi": duoi(key), "err": m }),
            );
            return format!(
                "⚠ {ten}: CHƯA đổi gì — máy chủ từ chối key {}: {m}",
                duoi(key)
            );
        }
        KiemKey::KhongKiemDuoc(m) => {
            logging::warn(
                "account_api_key_refused",
                json!({ "account": ten, "why": "không kiểm được", "duoi": duoi(key), "err": m }),
            );
            return format!(
                "⚠ {ten}: CHƯA đổi gì — chưa kiểm được key {} với máy chủ ({m}). Gửi lại sau.",
                duoi(key)
            );
        }
    }
    let dir = thu_muc(&acc);
    if !dir.is_dir() {
        return format!(
            "⚠ {ten}: CHƯA đổi gì — thư mục cấu hình {} không có.",
            dir.display()
        );
    }
    // Hỏng JSON thì DỪNG: ghi đè một tệp chủ máy tự sửa là xoá mất phần anh viết.
    let mut settings = match doc_settings(&dir) {
        Ok(m) => m.unwrap_or_default(),
        Err(e) => return format!("⚠ {ten}: CHƯA đổi gì — {e}"),
    };
    let truoc = chup(&dir);
    let helper_cu = settings
        .get("apiKeyHelper")
        .and_then(Value::as_str)
        .map(str::to_string);
    settings.insert("apiKeyHelper".into(), Value::String(helper_cua_huba(&dir)));
    let ghi = ghi_nguyen_tu(&dir.join(TEP_KEY), key.as_bytes(), true).and_then(|_| {
        let mut s = serde_json::to_string_pretty(&Value::Object(settings))
            .map_err(|e| format!("không dựng được settings.json: {e}"))?;
        s.push('\n');
        ghi_nguyen_tu(&dir.join("settings.json"), s.as_bytes(), false)
    });
    let xac_nhan = ghi.and_then(|_| auth_status(cfg, &acc)).and_then(|v| {
        let cach = v.get("authMethod").and_then(Value::as_str).unwrap_or("");
        let nguon = v.get("apiKeySource").and_then(Value::as_str).unwrap_or("");
        if cach == "api_key_helper" && nguon == "apiKeyHelper" {
            Ok(())
        } else {
            Err(format!(
                "CLI không nhận: authMethod={cach:?} apiKeySource={nguon:?}"
            ))
        }
    });
    if let Err(e) = xac_nhan {
        let tra = tra_lai(&dir, &truoc);
        logging::error(
            "account_api_key_set_failed",
            json!({ "account": ten, "duoi": duoi(key), "err": e, "tra_lai_hong": tra }),
        );
        return if tra.is_empty() {
            format!("⚠ {ten}: CHƯA đổi gì — {e}. Đã trả hai tệp về nguyên trạng.")
        } else {
            format!(
                "⚠ {ten}: HỎNG giữa chừng — {e}. Trả lại cũng hỏng: {}. Xem {}",
                tra.join(" · "),
                dir.display()
            )
        };
    }
    logging::info(
        "account_api_key_set",
        json!({ "account": ten, "duoi": duoi(key),
                "thay_helper_cu": helper_cu.is_some_and(|h| h != helper_cua_huba(&dir)) }),
    );
    let khoa = if acc.locked {
        format!(
            "🔒 {ten} vẫn KHOÁ ⟹ huba không tự dùng nó; mở phiên: /new -a {ten} (có hộp xác nhận)."
        )
    } else {
        format!(
            "huba KHÔNG tự chọn tài khoản chạy bằng API key (tốn tiền theo token) — chỉ khi \
             anh gõ /new -a {ten}."
        )
    };
    format!(
        "🔑 {ten} nay chạy bằng API key {} — máy chủ nhận key, CLI xác nhận \
         authMethod=api_key_helper.\n\
         Đăng nhập gói cũ trong thư mục vẫn còn nhưng KHÔNG được dùng. Phiên {ten} ĐANG mở \
         vẫn chạy bằng cách cũ tới khi mở lại.\n\
         ⚠ Tính tiền theo token. {khoa}\n\
         Bỏ: /accounts {ten} -k xoa",
        duoi(key)
    )
}

/// `/accounts <tên> -k xoa` — gỡ `apiKeyHelper` của huba và tệp key, trả tài khoản về
/// gói. Helper KHÔNG phải của huba thì không đụng (chủ máy tự đặt).
pub fn go(cfg: &Config, ten: &str) -> String {
    let acc = match tim(cfg, ten) {
        Ok(a) => a,
        Err(m) => return m,
    };
    let dir = thu_muc(&acc);
    let mut settings = match doc_settings(&dir) {
        Ok(m) => m.unwrap_or_default(),
        Err(e) => return format!("⚠ {ten}: CHƯA đổi gì — {e}"),
    };
    match settings.get("apiKeyHelper").and_then(Value::as_str) {
        None => return format!("{ten} không chạy bằng API key — không có gì để bỏ."),
        Some(h) if h != helper_cua_huba(&dir) => {
            return format!(
                "⚠ {ten}: apiKeyHelper trong {} KHÔNG phải của huba — không đụng. Sửa tay tệp ấy.",
                dir.join("settings.json").display()
            )
        }
        Some(_) => {}
    }
    settings.remove("apiKeyHelper");
    let mut s = match serde_json::to_string_pretty(&Value::Object(settings)) {
        Ok(s) => s,
        Err(e) => return format!("⚠ {ten}: CHƯA đổi gì — {e}"),
    };
    s.push('\n');
    if let Err(e) = ghi_nguyen_tu(&dir.join("settings.json"), s.as_bytes(), false) {
        logging::error(
            "account_api_key_clear_failed",
            json!({ "account": ten, "err": e }),
        );
        return format!("⚠ {ten}: CHƯA đổi gì — {e}");
    }
    let xoa_key = match std::fs::remove_file(dir.join(TEP_KEY)) {
        Ok(()) => String::new(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => format!("\n⚠ tệp key chưa xoá được: {e}"),
    };
    let nay = match auth_status(cfg, &acc) {
        Ok(v) => format!(
            "CLI nay khai authMethod={}",
            v.get("authMethod").and_then(Value::as_str).unwrap_or("?")
        ),
        Err(e) => format!("chưa hỏi được CLI: {e}"),
    };
    logging::info(
        "account_api_key_cleared",
        json!({ "account": ten, "nay": nay }),
    );
    format!("{ten} thôi dùng API key — {nay}. Chưa đăng nhập gói thì: /accounts {ten} -l{xoa_key}")
}
