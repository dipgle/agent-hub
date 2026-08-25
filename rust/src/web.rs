//! Trình duyệt của huba — lái bằng Playwright, qua `web.mjs`.
//!
//! 🔴 Hà 2026-08-23: *"Sao khong dùng playwright"*, ngay sau *"Tôi ko ngồi
//! máy"*. Đường AppleScript ([`crate::browser`]) lái đúng cái Chrome đang đăng
//! nhập của chủ máy, nhưng nó cần quyền Tự động hoá `hubd → Google Chrome` —
//! **cấp quyền ấy phải ngồi trước máy**. Một cây cầu mà nhịp đầu chỉ bắc được
//! khi đang đứng ở bờ bên kia thì chưa phải cầu.
//!
//! CDP không đi qua Apple Events, cũng không cần Screen Recording: **không
//! quyền macOS nào**, chạy được cả khi màn hình khoá. Số đo và ba chỗ đã trả
//! giá nằm ở đầu `web.mjs`; ở đây chỉ giữ phần Rust phải biết.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::exec::{run, RunOpts};
use crate::logging;

/// Trần cho một lệnh trình duyệt. Rộng hơn `osascript` nhiều vì lượt ĐẦU phải
/// dựng cả một trình duyệt, và một trang chậm còn được `web.mjs` chờ tới 30s.
const TIMEOUT: Duration = Duration::from_secs(75);

/// 🔴 ĐƯỜNG TUYỆT ĐỐI, KHÔNG PHẢI `node`. Job của launchd chạy với PATH tối
/// thiểu (`/usr/bin:/bin:/usr/sbin:/sbin`), nên gọi trống tên là `rc=127` —
/// bài học đã ghi trong sổ workspace, và nó im lặng theo kiểu tệ nhất: lệnh
/// "chạy xong" mà chẳng có gì xảy ra.
fn node_bin() -> Option<PathBuf> {
    let mut ung = Vec::new();
    if let Ok(v) = std::env::var("HUB_NODE") {
        ung.push(PathBuf::from(v));
    }
    ung.push(PathBuf::from("/usr/local/bin/node"));
    ung.push(PathBuf::from("/opt/homebrew/bin/node"));
    ung.push(PathBuf::from("/usr/bin/node"));
    ung.into_iter().find(|p| p.is_file())
}

/// Gọi `web.mjs` một lượt và đọc dòng JSON nó in ra.
///
/// Mọi lỗi ra ngoài dưới dạng `Err(String)` đã thành CÂU cho người đọc: chỗ gọi
/// là một tin nhắn Telegram, không phải một stack trace.
pub fn call(hub_home: &Path, lenh: &Value) -> Result<Value, String> {
    let script = hub_home.join("web.mjs");
    if !script.is_file() {
        return Err(format!("không thấy {}", script.display()));
    }
    let node = node_bin().ok_or_else(|| {
        "không thấy `node` trên máy (đã tìm HUB_NODE · /usr/local/bin · /opt/homebrew/bin). \
         Trình duyệt của huba cần Node để chạy Playwright."
            .to_string()
    })?;
    let arg = lenh.to_string();
    let out = run(
        &node.display().to_string(),
        &[&script.display().to_string(), &arg],
        RunOpts {
            cwd: Some(hub_home),
            timeout: Some(TIMEOUT),
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    if out.timed_out {
        return Err(format!(
            "lệnh trình duyệt quá {}s — xem logs/browser.log",
            TIMEOUT.as_secs()
        ));
    }
    // 🔴 ĐỌC DÒNG JSON, KHÔNG ĐỌC MÃ THOÁT. `web.mjs` trả mã 1 kèm một câu lỗi
    // ĐÃ VIẾT SẴN cho người đọc; vứt câu ấy đi để in "exit 1" là đúng thứ đã
    // chặn mọi bản vá của huba hôm 22/08 (`exec::drain_capped`).
    let dong = out
        .stdout
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with('{'))
        .unwrap_or("");
    let v: Value = serde_json::from_str(dong).unwrap_or_else(|_| json!({}));
    if let Some(loi) = v.get("error").and_then(Value::as_str) {
        logging::warn(
            "web_cmd_failed",
            json!({ "do": lenh.get("do"), "err": loi }),
        );
        return Err(loi.to_string());
    }
    if !out.ok() {
        // Mã thoát khác 0 mà KHÔNG có dòng JSON nào: node chết trước khi kịp
        // nói. Lúc ấy `stderr` là thứ duy nhất còn lại — đừng nuốt nó.
        return Err(format!(
            "trình duyệt không trả lời (mã {:?}): {}",
            out.code,
            crate::exec::truncate(out.stderr.trim(), 200)
        ));
    }
    Ok(v)
}

/// Một câu ngắn nói trang đang mở là trang nào.
fn noi_trang(v: &Value) -> String {
    let title = v.get("title").and_then(Value::as_str).unwrap_or("").trim();
    let url = v.get("url").and_then(Value::as_str).unwrap_or("");
    let host = crate::pipeline::web_host(url);
    if title.is_empty() {
        host
    } else {
        format!("{title} · {host}")
    }
}

/// Trần chữ cho một trang gửi về Telegram. Cắt thì phải NÓI: một trang cắt im
/// lặng đọc lên y hệt một trang ngắn.
const TEXT_MAX: usize = 3500;

/// Trình duyệt ẨN của huba có đang chạy không — và pid của nó.
///
/// 🔴 Có mặt vì một phép đo 2026-08-23: khi bản ẩn còn sống, **Apple Events
/// trỏ vào NÓ chứ không vào Chrome của chủ máy**. `browser::tabs()` đọc ra
/// đúng một tab `iana.org` — trang của bản ẩn — rồi giết bản ẩn đi thì Chrome
/// thật trả về `0` cửa sổ. Hai thế giới cùng một bundle id, và Apple Events
/// không phân biệt được. Nên đường "Chrome thật" phải HỎI trước rồi mới nói,
/// không thì nó khai một điều sai về thế giới — thứ tệ hơn hẳn im lặng.
pub fn an_dang_chay(hub_home: &Path) -> Option<u32> {
    let pid: u32 = std::fs::read_to_string(hub_home.join("data/browser.pid"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    // `kill -0` chỉ HỎI, không giết.
    con_song(pid).then_some(pid)
}

/// Tiến trình ấy còn sống không — hỏi bằng `kill -0`, thứ chỉ HỎI chứ không
/// giết.
///
/// Đi bằng một tiến trình con chứ không kéo `libc` vào: `unsafe` của repo này
/// sống ở ĐÚNG MỘT TỆP (`cgkeys.rs`, xem `CLAUDE.md`), và một lời gọi `kill(2)`
/// trực tiếp sẽ mở cái cửa ấy ra ở tệp thứ hai để đổi lấy vài micro giây.
fn con_song(pid: u32) -> bool {
    crate::exec::run(
        "kill",
        &["-0", &pid.to_string()],
        RunOpts {
            timeout: Some(Duration::from_secs(5)),
            ..Default::default()
        },
    )
    .map(|o| o.ok())
    .unwrap_or(false)
}

/// `/web` — một chỗ quyết định cho mọi dạng tham số.
///
/// Trả `(câu trả lời, đường dẫn ảnh nếu có)`.
pub fn route(hub_home: &Path, want: &str) -> (String, Option<PathBuf>) {
    let want = want.trim();
    let lenh = match want {
        "" => json!({"do":"where"}),
        "doc" | "đọc" | "text" | "chu" | "chữ" => json!({"do":"text"}),
        "tat" | "tắt" | "close" => json!({"do":"close"}),
        "enter" => json!({"do":"press","key":"Enter"}),
        _ => {
            if let Some(chu) = want
                .strip_prefix("bấm ")
                .or_else(|| want.strip_prefix("bam "))
                .or_else(|| want.strip_prefix("click "))
            {
                json!({"do":"click","what": chu.trim()})
            } else if let Some(chu) = want
                .strip_prefix("gõ ")
                .or_else(|| want.strip_prefix("go "))
                .or_else(|| want.strip_prefix("type "))
            {
                // KHÔNG `trim()` phần chữ: khoảng trắng đầu/cuối có thể là một
                // phần của thứ người ta muốn gõ, và ở đây không có cách nào
                // đoán hộ cho đúng.
                json!({"do":"type","text": chu})
            } else if let Some(o) = want
                .strip_prefix("ô ")
                .or_else(|| want.strip_prefix("o "))
                .or_else(|| want.strip_prefix("field "))
            {
                json!({"do":"field","what": o.trim()})
            } else {
                match crate::browser::dia_chi_hop_le(want) {
                    Some(url) => json!({"do":"goto","url": url}),
                    None => {
                        return (
                            "Không hiểu. `/web an` = đang ở đâu · `… <địa chỉ>` = mở · \
                             `… doc` = đọc chữ · `… bấm <chữ>` · `… ô <nhãn>` = chọn ô nhập · \
                             `… gõ <chữ>` · `… enter` · `… tắt`"
                                .to_string(),
                            None,
                        )
                    }
                }
            }
        }
    };
    let v = match call(hub_home, &lenh) {
        Ok(v) => v,
        Err(e) => return (format!("⚠ {e}"), None),
    };
    if lenh["do"] == json!("close") {
        return ("🌐 Đã tắt trình duyệt của huba.".to_string(), None);
    }
    if lenh["do"] == json!("text") {
        let chu = v.get("text").and_then(Value::as_str).unwrap_or("").trim();
        if chu.is_empty() {
            return (
                format!("{} — trang này không có chữ nào.", noi_trang(&v)),
                None,
            );
        }
        let n = chu.chars().count();
        let than = if n > TEXT_MAX {
            format!(
                "{}\n\n… cắt ở {TEXT_MAX} ký tự (trang dài {n}).",
                chu.chars().take(TEXT_MAX).collect::<String>()
            )
        } else {
            chu.to_string()
        };
        return (format!("🌐 {}\n\n{than}", noi_trang(&v)), None);
    }
    // Còn lại đều là "đang đứng ở một trang": kèm ẢNH, vì thứ người cầm điện
    // thoại cần trước hết là NHÌN THẤY trang, rồi mới tới chữ.
    let anh = hub_home.join("data").join("web-shot.png");
    let co_anh = call(
        hub_home,
        &json!({"do":"shot","path": anh.display().to_string()}),
    )
    .is_ok()
        && anh.is_file();
    (format!("🌐 {}", noi_trang(&v)), co_anh.then_some(anh))
}
