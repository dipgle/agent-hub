//! `huba setup` — trang cấu hình chạy NGAY TRÊN MÁY cài huba.
//!
//! Hà 2026-08-13, khi bàn chuyện mở repo công khai: *"cần kiến trúc rõ ràng để
//! ai kéo về cấu hình đơn giản là dùng được (dành cho người không rành kỹ
//! thuật)"* → *"đóng gói huba thành app và cài trên máy có ui để cấu hình biến
//! môi trường"*.
//!
//! Tôi đã trả lời sai một lần và cần ghi lại cho khỏi lặp: tôi viện luật *"cấu
//! hình không giữ bí mật"* để nói KHÔNG với một trang cấu hình. Hà chỉ đúng chỗ
//! sai — *"ui là của huba để quản lý kết nối trên máy cài đặt, đâu liên quan tới
//! bảo mật"*. Luật ấy nói về **nơi bí mật NẰM** (`huba.env`, chmod 600, không
//! bao giờ vào git, không bao giờ vào ảnh chụp trạng thái), chứ không nói về
//! **cái gì ghi ra nó**. Một trang chạy trên chính máy ấy là một trình soạn
//! thảo dễ dùng hơn `vi`, không hơn không kém.
//!
//! Nó cũng là NGOẠI LỆ có căn cứ cho luật 12 (*"trang điện thoại là UI duy
//! nhất"*): luật ấy sinh ra để chặn một bảng điều khiển vận hành thứ hai. Còn
//! đây là bước MỒI — lúc này chưa có khoá bot thì chưa có buồng chat nào để mà
//! ra lệnh. Không có bước mồi thì không có cái UI kia.
//!
//! Bốn ranh giới, và cả bốn đều là quyết định chứ không phải mặc định:
//! * **Chỉ 127.0.0.1**, không bao giờ `0.0.0.0` — trang này không có việc gì ở
//!   ngoài máy.
//! * **Vé một lần trong URL**: một tiến trình khác trên cùng máy vẫn gọi được
//!   cổng loopback, nên cổng không phải là hàng rào; cái vé mới là.
//! * **Không bao giờ hiện lại giá trị đã lưu** — trang chỉ nói khoá ấy *đã có*
//!   hay *chưa có*. Đọc file 600 rồi bơm ngược ra HTTP là tự tay dựng đúng cái
//!   đường rò mà file 600 sinh ra để chặn.
//! * **Ghi rồi mới đổi tên** (`huba.env.tmp` → `huba.env`), và đặt quyền 600
//!   TRƯỚC khi đổi tên, để không có một khoảnh khắc nào file mật nằm đó với
//!   quyền mặc định.

use anyhow::{anyhow, Result};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::logging;

/// Những khoá trang này biết hỏi. Thứ tự chính là thứ tự hiện trên trang.
///
/// `secret` quyết định hai thứ: ô nhập là `password`, và giá trị KHÔNG bao giờ
/// được đọc ngược ra trang.
struct Field {
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    secret: bool,
    required: bool,
}

// 🔴 Ba ô tfl5 (`HUB_TFL5_USER`, `HUB_TFL5_PASSWORD`, `HUB_TFL5_ALICE_PASSWORD`)
// đã bỏ 2026-08-14 cùng cái kênh. Hai ô còn lại nay là BẮT BUỘC, và đó là thay
// đổi có nghĩa chứ không phải dọn dẹp: khi còn phòng chat thì Telegram là kênh
// phụ, *"bỏ trống cũng chạy"*. Nay nó là kênh DUY NHẤT — thiếu khoá thì huba
// không có mồm nào để nghe, nên trang phải nói thẳng ngay lúc nhập, thay vì để
// người ta lưu xong rồi ngồi đợi một con bot không bao giờ trả lời.
const FIELDS: &[Field] = &[
    Field {
        key: "HUB_TELEGRAM_BOT_TOKEN",
        label: "Token bot Telegram",
        hint: "Xin ở @BotFather. Đây là kênh DUY NHẤT của huba — thiếu khoá này thì không ra lệnh được từ điện thoại.",
        secret: true,
        required: true,
    },
    Field {
        key: "HUB_TELEGRAM_CHAT_ID",
        label: "Chat ID Telegram — để trống, huba tự dò",
        hint: "Nhắn BẤT CỨ GÌ cho bot của bạn rồi bấm Lưu: huba hỏi Telegram và tự điền. Chỉ gõ tay khi bạn muốn chỉ định một buồng khác. Đây cũng là CỔNG: chỉ buồng chat này ra lệnh được cho huba.",
        secret: false,
        // 🔴 THÔI BẮT BUỘC 2026-08-25 — Hà: *"chỉ cần nhập token thì có cơ chế
        // tự quét id chứ, bắt người dùng đi lấy id thành phức tạp"*. Đúng: ô
        // này từng buộc chủ máy tự mở `api.telegram.org/bot<TOKEN>/getUpdates`
        // rồi đọc `message.chat.id` bằng mắt — việc huba làm hộ được, vì nó đã
        // có token và đã nói chuyện với đúng cái API ấy suốt ngày.
        required: false,
    },
];

/// Mở trang cấu hình rồi CHỜ tới khi lưu xong (hoặc chủ máy đóng bằng Ctrl-C).
pub fn serve(hub_home: &Path) -> Result<()> {
    let env_path = hub_home.join("huba.env");
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    // Vé một lần. `SystemTime` + địa chỉ con trỏ là đủ cho một cái vé sống vài
    // phút trên máy của chính mình; đây không phải khoá phiên đăng nhập.
    let ticket = format!(
        "{:x}{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        &listener as *const _ as usize
    );
    let url = format!("http://127.0.0.1:{port}/?t={ticket}");

    logging::info(
        "setup_started",
        json!({ "port": port, "env": env_path.display().to_string() }),
    );
    println!("\n  Mở trang cấu hình huba:\n\n    {url}\n");
    println!("  (chỉ máy này vào được — trang đóng ngay sau khi bạn bấm Lưu)\n");
    open_window(&url);

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(e) => {
                logging::warn("setup_accept_failed", json!({ "err": e.to_string() }));
                continue;
            }
        };
        match handle(&mut stream, &ticket, &env_path) {
            Ok(true) => {
                logging::info(
                    "setup_saved",
                    json!({ "env": env_path.display().to_string() }),
                );
                println!("  ✅ Đã ghi {} (chmod 600).", env_path.display());
                println!("  Kiểm tra thật:  ./huba doctor");
                return Ok(());
            }
            Ok(false) => {}
            Err(e) => logging::warn("setup_request_failed", json!({ "err": e.to_string() })),
        }
    }
    Err(anyhow!("cổng cấu hình đóng trước khi lưu được gì"))
}

/// Mở trang cấu hình như một CỬA SỔ ỨNG DỤNG, không phải một tab lẫn trong bầy.
///
/// 🔴 Hà 2026-08-13: *"khi chạy ứng dụng thì hiện luôn fe hay phải vào trình
/// duyệt để thao tác"*. Câu hỏi đúng chỗ: một trang cấu hình mở ra thành tab
/// thứ ba mươi, cạnh Gmail và mười cái tab tài liệu, thì người không rành kỹ
/// thuật sẽ lạc mất nó ngay.
///
/// Chrome/Edge có `--app=<url>`: cùng một trang, nhưng thành cửa sổ riêng,
/// không thanh địa chỉ, không tab — nhìn và dùng như một ứng dụng. Không có
/// trình duyệt nào trong danh sách thì rơi về `open` thường; và `open` hỏng nốt
/// thì cũng KHÔNG làm hỏng cả lệnh, vì đường dẫn đã in ra màn hình rồi.
fn open_window(url: &str) {
    const APP_BROWSERS: &[&str] = &[
        "Google Chrome",
        "Microsoft Edge",
        "Brave Browser",
        "Chromium",
    ];
    for b in APP_BROWSERS {
        let ok = std::process::Command::new("open")
            .args(["-na", b, "--args", &format!("--app={url}")])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            logging::info("setup_window_opened", json!({ "as": "app", "browser": b }));
            return;
        }
    }
    match std::process::Command::new("open").arg(url).status() {
        Ok(s) if s.success() => {
            logging::info("setup_window_opened", json!({ "as": "browser-tab" }));
        }
        other => logging::warn(
            "setup_open_failed",
            json!({ "detail": format!("{other:?}"), "url_printed": true }),
        ),
    }
}

/// Trả `true` khi vừa lưu xong — chỗ gọi lấy đó làm dấu dừng.
fn handle(stream: &mut TcpStream, ticket: &str, env_path: &Path) -> Result<bool> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();

    let mut len = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
    }

    // Cái vé, không phải cái cổng, mới là hàng rào — mọi tiến trình trên máy đều
    // gọi được loopback.
    if !target.contains(&format!("t={ticket}")) {
        respond(
            stream,
            "403 Forbidden",
            "text/plain; charset=utf-8",
            b"ve khong dung",
        )?;
        return Ok(false);
    }

    if method == "POST" {
        let mut body = vec![0u8; len];
        reader.read_exact(&mut body)?;
        let mut form = parse_form(&String::from_utf8_lossy(&body));
        let dò = auto_chat_id(&mut form);
        let written = save_env(env_path, &form)?;
        let page = saved_page(&written, &dò);
        respond(
            stream,
            "200 OK",
            "text/html; charset=utf-8",
            page.as_bytes(),
        )?;
        return Ok(true);
    }

    let have = existing_keys(env_path);
    let page = form_page(ticket, &have);
    respond(
        stream,
        "200 OK",
        "text/html; charset=utf-8",
        page.as_bytes(),
    )?;
    Ok(false)
}

fn respond(stream: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) -> Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

/// Khoá nào ĐÃ có giá trị trong `huba.env` — **chỉ tên khoá, không lấy giá trị**.
pub fn existing_keys(env_path: &Path) -> Vec<String> {
    read_env(env_path)
        .into_iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .map(|(k, _)| k)
        .collect()
}

/// Đọc `huba.env` thành cặp khoá–giá trị. Dòng chú thích và dòng rỗng bỏ qua.
fn read_env(env_path: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(env_path) else {
        return out;
    };
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            out.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    out
}

/// Trộn giá trị mới vào file cũ rồi ghi lại — **giữ nguyên khoá lạ**.
///
/// Giữ khoá lạ là có chủ ý: `huba.env` là file của CHỦ MÁY, không phải file của
/// trang này. Ghi đè sạch nghĩa là một trang cấu hình biết 5 khoá sẽ lặng lẽ
/// xoá khoá thứ 6 mà ai đó thêm tay.
///
/// Ô để trống ⟹ **không đụng** tới giá trị đang có (đó cũng là lý do trang
/// không cần đọc giá trị cũ ra để hiện lại).
fn save_env(env_path: &Path, form: &BTreeMap<String, String>) -> Result<Vec<String>> {
    let mut merged = read_env(env_path);
    let mut written = Vec::new();
    for f in FIELDS {
        if let Some(v) = form.get(f.key) {
            if !v.trim().is_empty() {
                merged.insert(f.key.to_string(), v.trim().to_string());
                written.push(f.key.to_string());
            }
        }
    }
    let mut text = String::from(
        "# huba — bí mật của máy này. Ghi bởi `huba setup`.\n\
         # Chỉ TÊN khoá được ghi vào log, không bao giờ ghi giá trị.\n\n",
    );
    for (k, v) in &merged {
        text.push_str(&format!("{k}={v}\n"));
    }

    // Ghi ra file tạm, đặt quyền 600 TRƯỚC, rồi mới đổi tên: không để tồn tại
    // một khoảnh khắc nào mà file mật nằm đó với quyền mặc định.
    let tmp: PathBuf = env_path.with_extension("env.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(&tmp, env_path)?;
    Ok(written)
}

/// `a=1&b=2` với mã hoá phần trăm → cặp khoá–giá trị.
fn parse_form(body: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for pair in body.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        out.insert(percent_decode(k), percent_decode(v));
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.replace('+', " ").into_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

const STYLE: &str = "body{font:16px/1.6 -apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;\
max-width:44rem;margin:3rem auto;padding:0 1.25rem;color:#1a1a1a;background:#fbfbfa}\
h1{font-size:1.5rem;margin:0 0 .25rem}p.lead{color:#555;margin:0 0 2rem}\
label{display:block;font-weight:600;margin:1.5rem 0 .25rem}\
small{display:block;color:#666;font-weight:400;margin-bottom:.4rem}\
input{width:100%;padding:.6rem .7rem;font-size:1rem;border:1px solid #ccc;border-radius:8px;\
background:#fff;box-sizing:border-box}\
.have{color:#0a7d33;font-weight:600;font-size:.85rem}\
button{margin-top:2rem;padding:.7rem 1.4rem;font-size:1rem;font-weight:600;border:0;\
border-radius:8px;background:#1a6dd4;color:#fff;cursor:pointer}\
code{background:#eee;padding:.1rem .3rem;border-radius:4px;font-size:.9em}\
.note{background:#fff8e1;border-left:3px solid #e0a800;padding:.8rem 1rem;margin:2rem 0;\
border-radius:0 8px 8px 0}";

fn form_page(ticket: &str, have: &[String]) -> String {
    let mut inputs = String::new();
    for f in FIELDS {
        let filled = have.iter().any(|k| k == f.key);
        let state = if filled {
            "<span class=\"have\">✓ đã có — để trống nếu không đổi</span>"
        } else if f.required {
            "<span style=\"color:#c0392b;font-size:.85rem\">bắt buộc</span>"
        } else {
            "<span style=\"color:#888;font-size:.85rem\">không bắt buộc</span>"
        };
        inputs.push_str(&format!(
            "<label>{} {}</label><small>{}</small>\
             <input type=\"{}\" name=\"{}\" autocomplete=\"off\" spellcheck=\"false\">",
            esc(f.label),
            state,
            esc(f.hint),
            if f.secret { "password" } else { "text" },
            f.key
        ));
    }
    format!(
        "<!doctype html><html lang=\"vi\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>Cấu hình huba</title><style>{STYLE}</style></head><body>\
         <h1>Cấu hình huba</h1>\
         <p class=\"lead\">Trang này chỉ chạy trên máy của bạn. Bấm Lưu là nó ghi thẳng vào \
         <code>huba.env</code> (chmod 600) rồi tự đóng — không có gì đi ra ngoài.</p>\
         <div class=\"note\"><b>Một bot Telegram cho riêng bạn.</b> Đừng dùng chung bot với người \
         khác: tin huba gửi đi mang <i>chữ đang hiện trên màn</i> phiên của bạn, và <code>chat_id</code> \
         dưới đây là cổng DUY NHẤT — đúng một phép so ấy là thứ ngăn người lạ chạy lệnh bằng shell \
         của bạn.</div>\
         <form method=\"POST\" action=\"/save?t={ticket}\">{inputs}<button>Lưu</button></form>\
         </body></html>"
    )
}

/// Điền hộ `HUB_TELEGRAM_CHAT_ID` khi chủ máy để trống — trả câu để in ra trang.
///
/// 🔴 Hà 2026-08-25: *"chỉ cần nhập token thì có cơ chế tự quét id chứ, bắt
/// người dùng đi lấy id thành phức tạp"*.
///
/// Ba nhánh, và cả ba đều NÓI RA, vì đây là lúc duy nhất chủ máy còn đang nhìn
/// màn hình cài đặt: dò được (in cả bot lẫn ai đã nhắn, để nhìn một cái là biết
/// đúng buồng chưa) · token đúng mà chưa ai nhắn (bảo nhắn rồi Lưu lại) · hỏi
/// không được (in nguyên văn lý do — token sai và hubad-đang-giữ-đường là hai
/// chuyện khác hẳn nhau).
///
/// KHÔNG đè lên giá trị chủ máy tự gõ: gõ tay nghĩa là cố ý chỉ định một buồng
/// khác, và huba đoán đè lên một lựa chọn có chủ ý là sai.
fn auto_chat_id(form: &mut std::collections::BTreeMap<String, String>) -> String {
    let token = form
        .get("HUB_TELEGRAM_BOT_TOKEN")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let da_go = form
        .get("HUB_TELEGRAM_CHAT_ID")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if token.is_empty() || da_go {
        return String::new();
    }
    match crate::telegram::probe_token(&token) {
        Ok((bot, Some((id, who)))) => {
            form.insert("HUB_TELEGRAM_CHAT_ID".to_string(), id.to_string());
            logging::info(
                "setup_chat_id_detected",
                json!({ "bot": bot, "chat_id": id }),
            );
            format!(
                "<p>🔎 Đã tự dò: bot <code>@{}</code>, buồng chat <code>{}</code> (tin gần nhất từ {}).                  Không đúng buồng thì gõ tay rồi Lưu lại.</p>",
                esc(&bot),
                id,
                esc(&who)
            )
        }
        Ok((bot, None)) => format!(
            "<p>⚠ Token đúng (bot <code>@{}</code>) nhưng <b>chưa ai nhắn cho nó</b>, nên chưa có              buồng nào để dò. Mở Telegram, nhắn một câu bất kỳ cho bot, rồi bấm Lưu lại.</p>",
            esc(&bot)
        ),
        Err(e) => format!("<p>⚠ Chưa dò được chat id: {}</p>", esc(&e)),
    }
}

fn saved_page(written: &[String], dò: &str) -> String {
    let list = if written.is_empty() {
        "<p>Không có ô nào được điền — file cũ giữ nguyên.</p>".to_string()
    } else {
        format!(
            "<p>Đã ghi {} khoá:</p><ul>{}</ul>",
            written.len(),
            written
                .iter()
                .map(|k| format!("<li><code>{}</code></li>", esc(k)))
                .collect::<String>()
        )
    };
    format!(
        "<!doctype html><html lang=\"vi\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>Đã lưu</title><style>{STYLE}</style></head><body>\
         <h1>✅ Đã lưu</h1>{list}{dò}\
         <p>Bước tiếp theo, chạy ở terminal:</p>\
         <p><code>./huba doctor</code> — kiểm tra thật: hỏi Telegram, tìm claude CLI, đọc thư mục dự án.</p>\
         <p><code>./huba self-install</code> — cài daemon để huba tự chạy cùng máy.</p>\
         <p style=\"color:#666\">Đóng tab này được rồi.</p></body></html>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_box_never_wipes_a_key_that_is_already_there() {
        let dir = std::env::temp_dir().join(format!("hubsetup{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("huba.env");
        std::fs::write(
            &p,
            "HUB_TELEGRAM_CHAT_ID=8110\nHUB_TELEGRAM_BOT_TOKEN=cu\nNGUOI_KHAC_THEM=giu\n",
        )
        .unwrap();

        let mut form = BTreeMap::new();
        form.insert("HUB_TELEGRAM_BOT_TOKEN".to_string(), "moi".to_string());
        form.insert("HUB_TELEGRAM_CHAT_ID".to_string(), "   ".to_string()); // để trống
        let written = save_env(&p, &form).unwrap();

        let after = read_env(&p);
        assert_eq!(written, vec!["HUB_TELEGRAM_BOT_TOKEN"]);
        assert_eq!(
            after.get("HUB_TELEGRAM_BOT_TOKEN").unwrap(),
            "moi",
            "phải nhận giá trị mới"
        );
        assert_eq!(
            after.get("HUB_TELEGRAM_CHAT_ID").unwrap(),
            "8110",
            "ô trống KHÔNG được xoá khoá cũ"
        );
        assert_eq!(
            after.get("NGUOI_KHAC_THEM").unwrap(),
            "giu",
            "khoá lạ do chủ máy tự thêm phải còn nguyên"
        );

        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "file mật phải là 600, đo được {mode:o}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_page_never_shows_a_value_it_read_back() {
        let dir = std::env::temp_dir().join(format!("hubsetup2{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("huba.env");
        std::fs::write(&p, "HUB_TELEGRAM_BOT_TOKEN=khoa-that-cua-ha\n").unwrap();

        let have = existing_keys(&p);
        assert_eq!(
            have,
            vec!["HUB_TELEGRAM_BOT_TOKEN"],
            "chỉ TÊN khoá được lấy ra"
        );
        let page = form_page("ve", &have);
        assert!(
            !page.contains("khoa-that-cua-ha"),
            "giá trị bí mật lọt ra trang HTTP"
        );
        assert!(page.contains("✓ đã có"), "phải nói khoá ấy đã có");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_password_with_punctuation_survives_the_form_encoding() {
        let form =
            parse_form("HUB_TELEGRAM_BOT_TOKEN=a%26b%3Dc%20d%2B&HUB_TELEGRAM_CHAT_ID=b%C3%B4t");
        assert_eq!(form.get("HUB_TELEGRAM_BOT_TOKEN").unwrap(), "a&b=c d+");
        assert_eq!(form.get("HUB_TELEGRAM_CHAT_ID").unwrap(), "bôt");
    }
}
