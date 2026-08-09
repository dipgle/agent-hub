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

/// Đoạn AppleScript hỏi cửa sổ nào đang chạy `dev`.
///
/// Tách ra thành hàm THUẦN để kiểm được bằng test: lỗi đầu tiên của tính năng
/// này là một dòng `return ""` thừa ở cuối, bị cú pháp chuỗi thô của Rust nuốt
/// mất dấu nháy, và nó chỉ lộ khi chạy thật vì không có gì soi chuỗi sinh ra.
fn window_script(dev: &str) -> String {
    // `try` quanh vòng trong, KHÔNG phải cho đẹp: Terminal có những cửa sổ
    // không có tab nào (cửa sổ cài đặt, inspector), và hỏi `tabs of w` ở đó thì
    // cả script chết giữa chừng — *"Can't get every tab of item 1 of every
    // window. (-1728)"*, đo 2026-08-10. Một cửa sổ lạ không được phép làm hỏng
    // việc tìm cửa sổ đúng.
    format!(
        r#"tell application "Terminal"
  repeat with w in every window
    try
      repeat with t in tabs of w
        if tty of t is {} then return id of w
      end repeat
    end try
  end repeat
end tell"#,
        as_string(dev)
    )
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
    let script = window_script(&dev);
    // Không tìm thấy thì script chạy hết mà không `return` gì — `osascript` in
    // ra chuỗi rỗng, `parse` hỏng, và ta được `None`. Đó là câu trả lời đúng.
    //
    // ⚠ Bản đầu có thêm dòng `return ""` ở cuối để nói rõ "không thấy". Trong
    // chuỗi thô `r#"…"#` của Rust, dấu `"` đầu là nội dung còn `"#` đóng chuỗi,
    // nên AppleScript nhận được `return "` treo lửng và hỏng ngay ở dòng đầu:
    // *"Expected string but found end of script. (-2741)"*. Một dòng thừa viết
    // cho dễ hiểu lại làm hỏng cả tính năng — và nó chỉ lộ khi CHẠY THẬT, vì
    // test của tệp này chỉ kiểm phần thoát chuỗi.
    let out = osascript(&script)?;
    Ok(out.trim().parse::<i64>().ok())
}

/// Gõ `text` vào cửa sổ của phiên, rồi Enter.
///
/// `enter = false` cho các lựa chọn cần phím riêng (mũi tên, Esc) — xem
/// [`press`].
pub fn type_into(window: i64, text: &str, enter: bool) -> Result<()> {
    let _ = enter;
    osascript(&do_script(window, &as_string(text)))?;
    Ok(())
}

/// Gửi chữ vào cửa sổ bằng `do script` — API CỦA CHÍNH Terminal.
///
/// Đường cũ đi qua `System Events keystroke`, và macOS chặn thẳng:
/// *"osascript is not allowed to send keystrokes (1002)"*. Cấp Accessibility
/// cho `hubd` không gỡ được, vì thứ gọi AXAPI là `/usr/bin/osascript` —
/// một binary hệ thống, không gán quyền cho nó qua đường daemon được (đo
/// 2026-08-10: cấp quyền rồi khởi động lại daemon, vẫn 1002).
///
/// `do script` thì khác hẳn: nó là scripting API của Terminal, chỉ cần quyền
/// **Automation** — thứ hub đã có, bằng chứng là nó đang đọc được `contents of
/// selected tab`. Nó đẩy chữ vào đúng tab như người gõ, kể cả khi có chương
/// trình đang chạy phía trước.
///
/// ⚠ `do script` LUÔN kèm một dấu xuống dòng — không tắt được. Với ô nhập của
/// `claude` thì đó đúng là điều ta muốn (gõ xong là gửi), nhưng nó cũng có
/// nghĩa: không có cách "gõ mà chưa gửi" qua đường này.
fn do_script(window: i64, applescript_string: &str) -> String {
    format!(
        r#"tell application "Terminal"
  do script {applescript_string} in selected tab of window id {window}
end tell"#
    )
}

/// Một phím điều khiển: `up` `down` `enter` `esc` `tab` `space`, hoặc `1`–`9`.
///
/// Hộp chọn của `claude` đi bằng mũi tên + Enter, và gửi chữ "xuống" vào đó thì
/// nó gõ ra chữ chứ không di chuyển.
pub fn press(window: i64, keyname: &str) -> Result<()> {
    // Ký tự điều khiển gửi qua `do script` như mọi chuỗi khác. Mũi tên là dãy
    // thoát ANSI: ESC [ A/B/C/D — đúng thứ terminal nhận khi người ta bấm.
    let payload = match keyname {
        "enter" => "\"\"".to_string(),          // chuỗi rỗng: `do script` tự kèm xuống dòng
        "esc" => "(ASCII character 27)".to_string(),
        "up" => "((ASCII character 27) & \"[A\")".to_string(),
        "down" => "((ASCII character 27) & \"[B\")".to_string(),
        "right" => "((ASCII character 27) & \"[C\")".to_string(),
        "left" => "((ASCII character 27) & \"[D\")".to_string(),
        "tab" => "(ASCII character 9)".to_string(),
        "space" => as_string(" "),
        d if d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) => as_string(d),
        other => return Err(anyhow!("không biết phím '{other}'")),
    };
    osascript(&do_script(window, &payload))?;
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
    // Thu nhỏ TRƯỚC khi gửi đi. Ảnh Retina của một cửa sổ terminal là 1–3 MB;
    // base64 hoá lên gấp rưỡi và nhét vào một doc thì mỗi lần chụp là vài MB
    // đi qua mạng cho một thứ chỉ để LIẾC. 1200px vẫn đọc rõ chữ terminal.
    // `sips` có sẵn trong macOS, không thêm phụ thuộc nào.
    let _ = run(
        "sips",
        &["-Z", "1200", &path.display().to_string()],
        RunOpts {
            timeout: Some(Duration::from_secs(20)),
            ..Default::default()
        },
    );
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

/// CHỮ đang hiện trên màn của cửa sổ ấy.
///
/// Hà 2026-08-10: *"sao lại đẩy ảnh, dựng lại đúng option chứ? thì mới có nhiều
/// lựa chọn thao tác hơn, ví dụ dùng chuột để chọn"*. Đúng — ảnh chỉ NHÌN được,
/// còn cái người ta cần là BẤM được.
///
/// Và hoá ra không cần ảnh: Terminal cho đọc thẳng `contents of selected tab`,
/// tức đúng chữ đang hiện. Không OCR, không vài trăm KB base64, và chữ thì đi
/// qua được `redaction::leak_scan` — ảnh thì không.
pub fn screen_text(window: i64) -> Result<String> {
    let script = format!(
        r#"tell application "Terminal"
  return contents of selected tab of window id {window}
end tell"#
    );
    osascript(&script)
}

/// Hộp chọn đang chờ trên màn, nếu có.
///
/// `claude` vẽ hộp chọn dạng:
///
/// ```text
///   Câu hỏi ở đây?
///   ❯ 1. Phương án một
///     2. Phương án hai
/// ```
///
/// Nhận diện bằng HÌNH DẠNG (`❯` hoặc `N.` đứng đầu dòng), không bằng cách
/// đoán nội dung: câu hỏi là chữ của người khác viết, hình dạng mới là thứ
/// `claude` bảo đảm.
pub fn parse_choices(screen: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for line in screen.lines() {
        let t = line.trim();
        // Bỏ dấu con trỏ ❯ nếu có, rồi tìm "<số>." ở đầu.
        let t = t.strip_prefix('❯').map(str::trim_start).unwrap_or(t);
        let Some((num, rest)) = t.split_once('.') else { continue };
        let Ok(n) = num.trim().parse::<usize>() else { continue };
        if n == 0 || n > 9 {
            continue;
        }
        let label = rest.trim();
        // Một dòng "1." trống không phải lựa chọn; một dòng dài lê thê cũng
        // không — hộp chọn của claude là nhãn ngắn.
        if label.is_empty() || label.len() > 120 {
            continue;
        }
        out.push((n, label.to_string()));
    }
    // Số phải liên tiếp từ 1: "3. xong" trong một đoạn văn không phải hộp chọn.
    if out.is_empty() || out[0].0 != 1 {
        return Vec::new();
    }
    for (i, (n, _)) in out.iter().enumerate() {
        if *n != i + 1 {
            return Vec::new();
        }
    }
    out
}

/// Phiên có đang chạy dở không, đọc từ màn hình.
///
/// `claude` chạy dở thì in một dòng đếm giờ — `✶ Unravelling… (2m 36s · ↓ 2.0k
/// tokens · …)`. Chữ đầu đổi liên tục (Unravelling, Pondering, Herding…), nên
/// bắt theo chữ là bắt trượt; thứ KHÔNG đổi là cái đồng hồ `(<số>m <số>s ·`.
///
/// Đây là tín hiệu cho câu "đã chạy hết chỗ dở chưa" (Hà 2026-08-10). Nhật ký
/// không trả lời được: nó chỉ ghi SAU khi lượt xong, nên một phiên đang nghĩ ba
/// phút trông y hệt một phiên đã nghỉ.
pub fn is_busy(screen: &str) -> bool {
    screen.lines().any(|l| {
        let b = l.as_bytes();
        // tìm "(<số>m <số>s ·" — quét tay cho rẻ, không kéo regex vào đây.
        let mut i = 0;
        while let Some(p) = l[i..].find('(') {
            let rest = &l[i + p + 1..];
            let mut it = rest.chars();
            let mut digits = 0;
            for c in it.by_ref() {
                if c.is_ascii_digit() {
                    digits += 1;
                } else {
                    if digits > 0 && c == 'm' && rest[digits..].starts_with("m ") {
                        // "<số>m " — kiểm tiếp "<số>s"
                        let after = &rest[digits + 2..];
                        let secs = after.chars().take_while(|c| c.is_ascii_digit()).count();
                        if secs > 0 && after[secs..].starts_with('s') {
                            return true;
                        }
                    }
                    break;
                }
            }
            i += p + 1;
            if i >= l.len() {
                break;
            }
        }
        let _ = b;
        false
    })
}

/// Chữ trên màn của phiên, đã gác bí mật và cắt gọn — dạng dùng được ngay.
///
/// `None` khi phiên không có cửa sổ, hoặc khi màn có dấu hiệu chứa bí mật:
/// chữ này rời khỏi máy y như phần xem trước của phiên, nên nó phải đi qua
/// đúng cái cổng ấy (điều 5 trong CLAUDE.md).
pub fn screen_of(tty: &str, lines: usize) -> Option<(String, Vec<(usize, String)>)> {
    let w = window_of(tty).ok().flatten()?;
    let screen = screen_text(w).ok()?;
    if !crate::sessions::preview_risk(&screen).is_empty() {
        return None;
    }
    let choices = parse_choices(&screen);
    let tail: Vec<&str> = screen
        .lines()
        .filter(|l| !l.trim().is_empty())
        .rev()
        .take(lines)
        .collect();
    let body = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
    Some((body, choices))
}

/// Ảnh cửa sổ, đã thu nhỏ, dưới dạng base64 để đi trong một doc.
///
/// Doc chứ KHÔNG phải file: `portal.rs` đã bỏ `/app/file/save` vì tệp nằm dưới
/// cây asset công khai với ACL rỗng — ai cũng đọc được. Ảnh chụp một cửa sổ
/// terminal có thể chứa mật khẩu, token, đường dẫn riêng; nó phải đi đúng con
/// đường mà ảnh chụp trạng thái đang đi, tức doc gác bằng ACL của app.
pub fn capture_base64(window: i64, tmp_dir: &std::path::Path) -> Result<(String, u64)> {
    let path = capture(window, tmp_dir)?;
    let bytes = std::fs::read(&path)?;
    let n = bytes.len() as u64;
    Ok((b64(&bytes), n))
}

/// base64 chuẩn, tự viết để khỏi thêm một crate cho 20 dòng.
fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{as_string, window_script};

    /// Chuỗi AppleScript sinh ra phải ĐÓNG ĐỦ dấu nháy và kết thúc đúng chỗ.
    ///
    /// Lỗi thật 2026-08-10: một dòng `return ""` ở cuối, viết trong chuỗi thô
    /// `r#"…"#` của Rust, bị cắt thành `return "` — AppleScript hỏng ngay dòng
    /// đầu (*"Expected string but found end of script. (-2741)"*) và cả tính
    /// năng chết. Không phép đo nào bắt được vì test cũ chỉ soi phần thoát chuỗi.
    /// base64 tự viết thì phải kiểm bằng những ca người ta hay sai: độ dài
    /// không chia hết cho 3, và byte 0xFF (dễ lộ lỗi dấu).
    /// Hộp chọn nhận ra bằng HÌNH DẠNG, và phải từ chối những thứ chỉ trông
    /// giống. Đây là chỗ dễ nhận nhầm nhất: mọi đoạn văn đều có thể chứa "1.".
    /// "Đang chạy dở" phải bắt theo ĐỒNG HỒ, không theo chữ — chữ đổi mỗi lần.
    #[test]
    fn busy_is_read_from_the_clock_not_the_word() {
        use super::is_busy;
        assert!(is_busy("✶ Unravelling… (2m 36s · ↓ 2.0k tokens · thinking)"));
        assert!(is_busy("✻ Pondering… (12m 4s · ↑ 900 tokens)"));
        assert!(is_busy("· Herding cats… (0m 8s ·)"));
        // Rảnh: dấu nhắc trống, dòng gợi ý, không có đồng hồ.
        assert!(!is_busy("❯ \n⏵⏵ auto mode on (shift+tab to cycle) · esc to interrupt"));
        assert!(!is_busy(""));
        // Ngoặc có số nhưng KHÔNG phải đồng hồ thì không tính.
        assert!(!is_busy("Đã sửa 3 tệp (xem lại 2 chỗ)"));
        assert!(!is_busy("chạy trong (500ms)"));
    }

    #[test]
    fn choices_are_recognised_by_shape_only() {
        use super::parse_choices;
        let box_ = "Chọn cách đi tiếp?\n❯ 1. Sửa tại chỗ\n  2. Mở phiên mới\n  3. Bỏ qua";
        assert_eq!(
            parse_choices(box_),
            vec![
                (1, "Sửa tại chỗ".to_string()),
                (2, "Mở phiên mới".to_string()),
                (3, "Bỏ qua".to_string())
            ]
        );
        // Không bắt đầu từ 1 thì không phải hộp chọn.
        assert!(parse_choices("3. xong rồi\n4. tiếp").is_empty());
        // Số nhảy cóc cũng không.
        assert!(parse_choices("1. một\n3. ba").is_empty());
        // Đoạn văn có dấu chấm không phải lựa chọn.
        assert!(parse_choices("Tôi đã sửa 2 tệp. Xong.").is_empty());
        // Màn trống thì không có gì.
        assert!(parse_choices("").is_empty());
    }

    #[test]
    fn base64_matches_the_standard() {
        use super::b64;
        assert_eq!(b64(b""), "");
        assert_eq!(b64(b"f"), "Zg==");
        assert_eq!(b64(b"fo"), "Zm8=");
        assert_eq!(b64(b"foo"), "Zm9v");
        assert_eq!(b64(b"foob"), "Zm9vYg==");
        assert_eq!(b64(b"fooba"), "Zm9vYmE=");
        assert_eq!(b64(b"foobar"), "Zm9vYmFy");
        assert_eq!(b64(&[0xff, 0xff, 0xff]), "////");
        assert_eq!(b64(&[0x00, 0x00, 0x00]), "AAAA");
    }

    #[test]
    fn window_script_is_well_formed() {
        let s = window_script("/dev/ttys005");
        assert!(s.contains(r#"is "/dev/ttys005" then"#), "{s}");
        assert!(s.trim_end().ends_with("end tell"), "kết thúc sai:\n{s}");
        // Số dấu nháy phải CHẴN — lẻ nghĩa là có một chuỗi treo lửng.
        assert_eq!(s.matches('"').count() % 2, 0, "dấu nháy lẻ:\n{s}");
        // Và không được còn chỗ giữ chỗ nào chưa thay.
        assert!(!s.contains("{}"), "{s}");
        // Cửa sổ không có tab (cài đặt, inspector) phải bị BỎ QUA chứ không
        // làm chết cả script — lỗi -1728 đo được 2026-08-10.
        // Đếm `"try"` trần là sai: nó nằm trong cả `end try`. Mỗi `try` phải có
        // đúng một `end try` — đó mới là điều cần kiểm.
        assert_eq!(s.matches("end try").count(), 1, "{s}");
        assert_eq!(s.matches("try").count() - s.matches("end try").count(), 1, "{s}");
    }

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
