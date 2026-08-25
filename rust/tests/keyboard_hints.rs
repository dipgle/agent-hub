//! Gợi ý bàn phím của TUI không được đi theo tin sang điện thoại.
//!
//! 🔴 Hà 2026-08-23: *"Sao cuối tin gửi tele lại có / rc"*.
//!
//! `/rc` là chỉ báo **Remote Control** của chính TUI `claude`, căn phải ở thanh
//! trạng thái. Bằng chứng lấy từ bản đang cài chứ không từ trí nhớ:
//!
//! ```text
//! :"/rc reconnecting",color:"warning"};if(r||t)return{label:"/rc active",…
//! let e5l = v7r.label==="/rc active" && !ggD ? "/rc" : v7r.label
//! ```
//!
//! Và từ log của chính huba (20.000 dòng gần nhất): `/rc active` ×29, `/rc`
//! ×21, `/rc failed` ×12 — luôn ở cuối dòng chế độ quyền, sau một dải cách dài.

use huba::pipeline::strip_keyboard_hints;

/// Nguyên văn dòng trạng thái lấy từ một `ack` thật trong `logs/huba.log`
/// (2026-08-23T11:21:33Z, phiên `[dwork/A-DSIGN]`).
const STATUS_LINE: &str = "  ⏵⏵ auto mode on (shift+tab to cycle) · ← for agents                                                                                                                                                     /rc";

#[test]
fn the_remote_control_hint_leaves_the_message() {
    let got = strip_keyboard_hints(STATUS_LINE);
    assert!(!got.contains("/rc"), "{got:?}");
    // …và cắt luôn dải cách căn phải của nó: trên màn 38 cột, 150 dấu cách nở
    // thành mấy hàng trống.
    assert_eq!(got, "  ⏵⏵ auto mode on (shift+tab to cycle) · ← for agents");
}

/// Cả bốn nhãn, vì Remote Control có bốn trạng thái.
#[test]
fn every_remote_control_label_is_recognised() {
    for label in ["/rc", "/rc active", "/rc failed", "/rc reconnecting"] {
        let line = format!("⏵⏵ auto mode on · ← 2 agents          {label}");
        let got = strip_keyboard_hints(&line);
        assert_eq!(
            got, "⏵⏵ auto mode on · ← 2 agents",
            "nhãn {label:?} không được nhận ra"
        );
    }
}

/// 🔴 Và đây là hàng rào: neo là CHUỖI, không phải "đoạn căn phải".
///
/// Cắt mọi thứ sau một dải cách dài thì gọn hơn thật — và ăn mất cả bảng kẻ
/// cột lẫn dòng `… +35 lines` của công cụ, tức chữ người đọc cần. Cùng bài học
/// đã trả giá ở `strip_box_rules`: nới phạm vi là tự xoá nội dung của mình.
#[test]
fn a_wide_gap_alone_never_cuts_anything() {
    for line in [
        "│ uc-dca-kiemnhiem-giao │ 32 ĐẠT / 1 HỎNG │ 23/08g: 32/33 — khớp │",
        "      … +35 lines ",
        "  ⎿  Wrote 45 lines to ../../../../.tmp/commit-msg.txt",
        "                                        new task? /clear to save 432.2k tokens",
        "Chạy: bash ~/.zshrc",
    ] {
        assert_eq!(
            strip_keyboard_hints(line),
            line,
            "dòng không mang nhãn Remote Control mà vẫn bị cắt"
        );
    }
}

/// `/rc` dính liền chữ khác thì KHÔNG phải cái nhãn ấy.
///
/// Không có dải cách ngăn ⟹ nó là một mẩu đường dẫn hoặc một câu, và cắt nó là
/// cắt vào chữ.
#[test]
fn a_path_that_merely_ends_in_rc_is_left_alone() {
    for line in ["đọc tệp ~/.config/rc", "cd /etc/rc", "xem src/rc"] {
        assert_eq!(strip_keyboard_hints(line), line);
    }
}

/// Nhiều dòng: chỉ dòng mang nhãn bị đụng, thứ tự và số dòng giữ nguyên.
#[test]
fn only_the_line_that_carries_the_hint_changes() {
    let man = format!("⏺ Xong rồi.\n{STATUS_LINE}\n❯ vá nốt đi");
    let got = strip_keyboard_hints(&man);
    assert_eq!(got.lines().count(), 3, "{got:?}");
    assert!(got.starts_with("⏺ Xong rồi.\n"));
    assert!(got.ends_with("\n❯ vá nốt đi"));
    assert!(!got.contains("/rc"));
}
