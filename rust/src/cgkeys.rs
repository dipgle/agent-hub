//! Gửi MỘT phím vào Terminal — không kèm dấu xuống dòng nào.
//!
//! 🔴 ĐÂY LÀ TỆP DUY NHẤT CÓ `unsafe` TRONG REPO NÀY, và luật cũ (*"No `unsafe`
//! anywhere"*) được Hà gỡ đúng cho chỗ này ngày 2026-08-19, sau khi tôi trình
//! đủ hai bức tường đã đo được.
//!
//! **Bức tường:** mọi lượt ghi qua `do script` của Terminal kèm một CR không tắt
//! được (`keys::press_writes`). Trên một hộp chọn, CR là một cú CHỐT. Nên huba
//! không có phím nào chỉ *di chuyển*: một cái nút "sang tab bên phải" sẽ trả lời
//! hộ câu đang mở trước khi kịp sang. Không phải suy luận — đo được, và cái giá
//! trả bằng việc thật: 2026-08-19, một cú Enter lạc vào bảng hỏi của phiên
//! `[AI/tcc/amm]` chốt luôn `☐ RPC pool` → `☒` với lựa chọn 1.
//!
//! **Đường vòng:** `CGEventPostToPid` đưa thẳng một sự kiện bàn phím vào tiến
//! trình Terminal. Phím rời là phím rời — không có CR nào đi kèm, vì không có
//! `do script` nào cả. Đổi lại: cần quyền **Accessibility** cấp cho đúng bản
//! `hubad` đang chạy, và cần `unsafe` để gọi vào CoreGraphics.
//!
//! Quyền ấy bám được là nhờ việc đã làm 2026-08-10: `hubad` ký bằng chứng chỉ cố
//! định nên macOS nhận ra nó là CÙNG một chương trình qua mọi lần dựng lại (xem
//! `install_update.sh`). Ad-hoc thì mỗi lần build là một chương trình khác, và
//! quyền sẽ rụng sau đúng một `cargo build`.
//!
//! ## Ba luật của tệp này
//!
//! 1. **`unsafe` không được rời khỏi đây.** Ngoài kia chỉ thấy `Result`.
//! 2. **Cứ gửi, rồi HỎI, rồi NÓI.** `CGEventPostToPid` im lặng khi thiếu quyền:
//!    không trả lỗi, sự kiện chỉ đơn giản không tới. Đó là hình dạng tệ nhất với
//!    luật 3 (*"không có lỗi im lặng"*), nên [`trusted`] phải được hỏi ở MỖI
//!    lượt — chủ máy có thể gỡ quyền giữa chừng — và câu trả lời phải đi ra
//!    ngoài thành một câu đọc được.
//!
//!    🔴 Nhưng hỏi TRƯỚC rồi từ chối gửi là một ngõ cụt, và bản đầu của tệp này
//!    mắc đúng vào đó: `hubad` là một tiến trình nền không giao diện, nên nó
//!    **không bao giờ xuất hiện trong danh sách Trợ năng** cho tới khi nó thật
//!    sự THỬ làm một việc cần quyền ấy. Từ chối thử = không bao giờ được hỏi =
//!    không bao giờ được cấp. Nên thứ tự là: gửi (vô hại nếu chưa có quyền —
//!    sự kiện rơi vào hư không), rồi hỏi, rồi báo đúng chuyện gì đã xảy ra.
//! 3. **Kiểm hết tên phím TRƯỚC khi gửi phím đầu tiên.** Nửa dãy phím đi được
//!    rồi mới phát hiện phím thứ ba viết sai là để lại con trỏ ở một chỗ không
//!    ai lường trước — trên một bảng hỏi thì đó là một câu trả lời sai.

use anyhow::{anyhow, bail, Result};

/// Con trỏ mờ tới một `CGEvent` / `CGEventSource`. Chỉ đi qua lại giữa các hàm
/// FFI ngay dưới đây, không bao giờ được đọc nội dung.
type CfRef = *const std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    /// Tiến trình này có được cấp quyền Accessibility chưa.
    fn AXIsProcessTrusted() -> bool;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    /// `kCGEventSourceStateHIDSystemState` = 1 — nguồn sự kiện dùng chung với
    /// bàn phím thật, tức phím đi ra mang đúng trạng thái modifier hệ thống.
    fn CGEventSourceCreate(state_id: i32) -> CfRef;
    fn CGEventCreateKeyboardEvent(source: CfRef, key: u16, key_down: bool) -> CfRef;
    /// Đưa sự kiện vào ĐÚNG tiến trình ấy, không qua vòi HID chung.
    ///
    /// Khác `CGEventPost` ở chỗ quyết định: `CGEventPost` bắn vào bất kỳ cửa sổ
    /// nào đang nhận phím trên cả máy — tức nếu chủ máy vừa chuyển sang trình
    /// duyệt thì phím rơi vào đó. Bản này chỉ tiến trình Terminal nhận được.
    fn CGEventPostToPid(pid: i32, event: CfRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CfRef);
}

/// `hubad` (hay tiến trình đang gọi) đã được cấp quyền Accessibility chưa.
///
/// Không hỏi kèm hộp thoại xin quyền: `AXIsProcessTrustedWithOptions` bật được
/// một hộp thoại hệ thống, mà hộp ấy hiện ra trên MÀN HÌNH của cái máy — đúng
/// thứ huba sinh ra để khỏi phải ngồi trước. Nên huba chỉ ĐỌC trạng thái rồi nói
/// ra; việc cấp quyền là một câu chỉ dẫn gửi về điện thoại.
pub fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Tên phím → mã phím ảo của bàn phím ANSI.
///
/// Danh sách hẹp có chủ ý, cùng lý do với `keys::KNOWN`: đây là hàng rào, không
/// phải bảng tra. Mỗi phím ở đây là một phím huba thật sự cần để lái một hộp
/// chọn — mũi tên để đi, số để chọn, `enter` để chốt, `escape` để thoát,
/// `tab` vì chính TUI khai nó ở dòng chân (*"Tab/Arrow keys to navigate"*).
pub fn keycode(name: &str) -> Option<u16> {
    Some(match name {
        "left" => 123,
        "right" => 124,
        "down" => 125,
        "up" => 126,
        "enter" | "return" => 36,
        "escape" | "esc" => 53,
        "tab" => 48,
        "1" => 18,
        "2" => 19,
        "3" => 20,
        "4" => 21,
        "5" => 23,
        "6" => 22,
        "7" => 26,
        "8" => 28,
        "9" => 25,
        "0" => 29,
        _ => return None,
    })
}

/// Nghỉ giữa hai phím. Đủ để TUI vẽ lại, đủ ngắn để một dãy 5 phím vẫn dưới
/// nửa giây.
const GAP_MS: u64 = 40;

/// Gửi cả dãy phím vào tiến trình `pid`, KHÔNG kèm dấu xuống dòng nào.
///
/// Trả `Err` khi thiếu quyền hoặc tên phím lạ — và cả hai đều được kiểm TRƯỚC
/// khi phím đầu tiên rời đi (luật 2 và 3 ở đầu tệp).
pub fn post(pid: i32, keys: &[String]) -> Result<()> {
    if keys.is_empty() {
        bail!("không có phím nào để gửi");
    }
    // Dịch hết tên phím trước. Một tên sai ở giữa dãy mà đã gửi nửa đầu là để
    // con trỏ đứng ở chỗ không ai lường được.
    let codes: Vec<u16> = keys
        .iter()
        .map(|k| keycode(k).ok_or_else(|| anyhow!("phím lạ: {k}")))
        .collect::<Result<_>>()?;
    // SAFETY: mọi con trỏ dựng ra ở đây đều được giải phóng ngay trong hàm, và
    // không con trỏ nào rời khỏi nó. `CGEventCreateKeyboardEvent` trả NULL khi
    // hết bộ nhớ — kiểm trước khi dùng, vì `CGEventPostToPid(NULL)` là hành vi
    // không xác định.
    unsafe {
        let source = CGEventSourceCreate(1);
        for (sent, code) in codes.iter().enumerate() {
            for down in [true, false] {
                let ev = CGEventCreateKeyboardEvent(source, *code, down);
                if ev.is_null() {
                    if !source.is_null() {
                        CFRelease(source);
                    }
                    bail!(
                        "không dựng được sự kiện bàn phím (gửi được {sent}/{} phím)",
                        codes.len()
                    );
                }
                CGEventPostToPid(pid, ev);
                CFRelease(ev);
            }
            std::thread::sleep(std::time::Duration::from_millis(GAP_MS));
        }
        if !source.is_null() {
            CFRelease(source);
        }
    }
    // Hỏi SAU khi đã thử: lượt thử là thứ đưa `hubad` vào danh sách Trợ năng, và
    // câu trả lời này là thứ duy nhất phân biệt "phím đã tới" với "phím rơi vào
    // hư không" — hệ thống không nói gì cả.
    if !trusted() {
        bail!(
            "hubad chưa có quyền Trợ năng nên phím KHÔNG tới nơi (macOS không báo lỗi cho việc \
             này — nên đây là chỗ duy nhất nói ra). Mở Cài đặt Hệ thống ▸ Quyền riêng tư & Bảo \
             mật ▸ Trợ năng rồi bật cho `hubad`; vừa rồi huba đã thử một lần nên nó phải có tên \
             trong danh sách. Không thấy thì bấm ➕ và trỏ vào \
             ~/Library/Application Support/hub/bin/hubd"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bảng mã phím là thứ gõ tay nên phải có bài kiểm gõ lại — sai một mã là
    /// gửi nhầm một phím vào việc thật, và nó không lùi lại được.
    #[test]
    fn the_keys_hub_actually_needs_all_have_codes() {
        for k in [
            "left", "right", "up", "down", "enter", "escape", "tab", "1", "5", "9", "0",
        ] {
            assert!(keycode(k).is_some(), "thiếu mã cho phím {k}");
        }
        // Mũi tên đi liền nhau trên bàn phím ANSI: 123..126. Một chữ số lệch ở
        // đây là "sang phải" thành "xuống dưới".
        assert_eq!(
            (
                keycode("left"),
                keycode("right"),
                keycode("down"),
                keycode("up")
            ),
            (Some(123), Some(124), Some(125), Some(126))
        );
        // …và hàng số KHÔNG liền nhau (5 và 6 đảo chỗ so với trực giác), nên
        // ghim đúng cặp dễ sai nhất.
        assert_eq!((keycode("5"), keycode("6")), (Some(23), Some(22)));
    }

    #[test]
    fn an_unknown_key_name_is_refused_before_anything_is_sent() {
        assert!(keycode("f13").is_none());
        let err = post(0, &["right".to_string(), "f13".to_string()]).unwrap_err();
        assert!(err.to_string().contains("phím lạ"), "{err}");
    }

    #[test]
    fn an_empty_sequence_is_refused() {
        assert!(post(0, &[]).is_err());
    }
}
