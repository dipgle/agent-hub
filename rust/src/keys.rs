//! Gõ vào cửa sổ terminal của một phiên, và chụp lại cửa sổ ấy.
//!
//! # Vì sao có tệp này
//!
//! Cho tới 2026-08-09 hub **không gõ được** vào phiên interactive: `claude` từ
//! chối `--resume` một phiên đang chạy, và không có primitive nào nhét chữ vào
//! đó (`CLAUDE.md` điều 10). Hệ quả thực tế: một phiên dừng lại hỏi *"chọn
//! phương án nào?"* thì từ điện thoại **không thấy và không trả lời được** —
//! bản ghi câu hỏi chỉ vào nhật ký SAU khi lượt kết thúc, nên nó vô hình cả với
//! `sessions::stream`.
//!
//! Hà chốt 2026-08-09, sau khi tôi nêu rõ đánh đổi: cho hub **gõ tự do** vào
//! phiên. Đây là quyết định của chủ máy, và nó **bỏ qua `DENIED_TOOLS`** —
//! chữ gõ thẳng vào terminal không đi qua bộ khoá nào. Ghi rõ ở đây để không ai
//! đọc mã sau này tưởng đó là sơ suất.
//!
//! # Hàng rào còn giữ (không phải về quyền, mà về ĐÚNG ĐÍCH)
//!
//! * Chỉ gõ vào cửa sổ **ghép được với một phiên có thật** qua `tty`. Không ghép
//!   được thì từ chối — gõ vào cửa sổ lạ là gõ vào việc của người khác.
//! * Mọi lần gõ đều **log** (`keys_typed`) kèm phiên và độ dài chuỗi. Nội dung
//!   không log: nó là chữ của chủ máy, và log là tệp nằm lâu.
//! * Lệnh đi qua phòng chat như mọi động từ khác, nên có dấu vết ở nơi đọc được.
//!
//! # Cái giá phải nói trước
//!
//! `System Events` gõ vào **cửa sổ đang ở trước**, nên hub phải kéo cửa sổ ấy
//! lên trước khi gõ. Tức là gõ từ điện thoại sẽ **giật tiêu điểm** trên máy.
//! Không có đường vòng: đó là cách macOS cho gõ vào một tiến trình interactive.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::json;

use crate::exec::{run, RunOpts};
use crate::logging;

/// `osascript` mất vài trăm ms; 20s là quá rộng rãi, và một cái treo ở đây sẽ
/// giữ cả vòng chạy của daemon.
const OSA_TIMEOUT: Duration = Duration::from_secs(20);

fn osascript(script: &str) -> Result<String> {
    let out = run(
        "osascript",
        &["-e", script],
        RunOpts {
            timeout: Some(OSA_TIMEOUT),
            ..Default::default()
        },
    )?;
    if out.timed_out {
        return Err(anyhow!("osascript quá {}s", OSA_TIMEOUT.as_secs()));
    }
    if out.code != Some(0) {
        return Err(anyhow!(
            "osascript hỏng: {}",
            crate::exec::truncate(out.stderr.trim(), 200)
        ));
    }
    Ok(out.stdout.trim().to_string())
}

/// Chuỗi cho AppleScript: chỉ có hai ký tự phải thoát, và bỏ sót một cái là
/// script hỏng cú pháp — hoặc tệ hơn, đổi nghĩa.
fn as_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Cửa sổ Terminal đang chạy `tty` này, nếu có.
///
/// `Terminal` công bố `tty` của từng tab qua AppleScript (đo 2026-08-09:
/// `/dev/ttys005, /dev/ttys000, …`), và hub đã biết `tty` của từng phiên từ
/// `ps -o tty=`. Ghép hai đầu ấy lại là ra đúng cửa sổ của phiên.
pub fn window_of(tty: &str) -> Result<Option<i64>> {
    if tty.is_empty() || tty == "??" || tty == "-" {
        return Ok(None);
    }
    // `ps` in `ttys005`, AppleScript trả `/dev/ttys005`.
    let dev = if tty.starts_with("/dev/") {
        tty.to_string()
    } else {
        format!("/dev/{tty}")
    };
    let script = format!(
        r#"tell application "Terminal"
  repeat with w in windows
    repeat with t in tabs of w
      if tty of t is {} then return id of w
    end repeat
  end repeat
end tell
return ""#,
        as_string(&dev)
    );
    let out = osascript(&script)?;
    Ok(out.trim().parse::<i64>().ok())
}

/// Gõ `text` vào cửa sổ của phiên, rồi Enter.
///
/// `enter = false` cho các lựa chọn cần phím riêng (mũi tên, Esc) — xem
/// [`press`].
pub fn type_into(window: i64, text: &str, enter: bool) -> Result<()> {
    let script = format!(
        r#"tell application "Terminal"
  activate
  set index of window id {window} to 1
end tell
delay 0.25
tell application "System Events"
  keystroke {}
  {}
end tell"#,
        as_string(text),
        if enter { "key code 36" } else { "" }
    );
    osascript(&script)?;
    Ok(())
}

/// Một phím điều khiển: `up` `down` `enter` `esc` `tab` `space`, hoặc `1`–`9`.
///
/// Hộp chọn của `claude` đi bằng mũi tên + Enter, và gửi chữ "xuống" vào đó thì
/// nó gõ ra chữ chứ không di chuyển.
pub fn press(window: i64, keyname: &str) -> Result<()> {
    let code = match keyname {
        "up" => "key code 126",
        "down" => "key code 125",
        "left" => "key code 123",
        "right" => "key code 124",
        "enter" => "key code 36",
        "esc" => "key code 53",
        "tab" => "key code 48",
        "space" => "key code 49",
        d if d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) => {
            return type_into(window, d, false)
        }
        other => return Err(anyhow!("không biết phím '{other}'")),
    };
    let script = format!(
        r#"tell application "Terminal"
  activate
  set index of window id {window} to 1
end tell
delay 0.25
tell application "System Events"
  {code}
end tell"#
    );
    osascript(&script)?;
    Ok(())
}

/// Chụp cửa sổ ấy ra PNG.
///
/// Đây là đường DUY NHẤT hub nhìn thấy câu hỏi đang chờ: hộp chọn nằm trên màn
/// hình, chưa vào nhật ký, nên không có cách nào đọc nó từ tệp.
///
/// ⚠ Cần quyền **Screen Recording** cho tiến trình chạy hub. Không có quyền thì
/// `screencapture` trả về ảnh trống chứ KHÔNG báo lỗi — nên hàm này kiểm cỡ tệp
/// và coi ảnh quá nhỏ là hỏng, thay vì đưa lên màn một khung đen.
pub fn capture(window: i64, out_dir: &std::path::Path) -> Result<PathBuf> {
    std::fs::create_dir_all(out_dir)?;
    let path = out_dir.join(format!("window-{window}.png"));
    let out = run(
        "screencapture",
        &[
            "-x",
            "-o",
            "-l",
            &window.to_string(),
            &path.display().to_string(),
        ],
        RunOpts {
            timeout: Some(Duration::from_secs(20)),
            ..Default::default()
        },
    )?;
    if out.code != Some(0) {
        return Err(anyhow!(
            "screencapture hỏng: {}",
            crate::exec::truncate(out.stderr.trim(), 200)
        ));
    }
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if size < 2048 {
        logging::warn(
            "capture_too_small",
            json!({ "window": window, "bytes": size,
                    "why": "thường là thiếu quyền Screen Recording" }),
        );
        return Err(anyhow!(
            "ảnh chụp rỗng ({size} byte) — nhiều khả năng hub chưa được cấp quyền Screen Recording"
        ));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::as_string;

    /// Thoát chuỗi sai là script hỏng cú pháp — hoặc đổi nghĩa, thứ tệ hơn.
    #[test]
    fn applescript_strings_escape_quotes_and_backslashes() {
        assert_eq!(as_string("hello"), "\"hello\"");
        assert_eq!(as_string(r#"nói "xin chào""#), "\"nói \\\"xin chào\\\"\"");
        assert_eq!(as_string(r"C:\path"), "\"C:\\\\path\"");
        // Chuỗi rỗng vẫn phải là một chuỗi hợp lệ, không phải hai dấu nháy trần.
        assert_eq!(as_string(""), "\"\"");
    }
}
