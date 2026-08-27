//! `/refresh` phải phân biệt **cửa sổ quá nhỏ** với **phiên treo**.
//!
//! 🔴 Hà 2026-08-27: *"Phiên này đang bị kẹt không làm được gì, treo view"*, kèm
//! ảnh `/shot` của `[social]` với các dòng mất phần đầu. Đo thẳng trên cửa sổ ấy
//! (`ttys013`, window 16384) trước khi sửa một dòng nào:
//!
//! ```text
//! cửa sổ 24×80 (cỡ đang bị)  →    25 byte   ← trống trơn
//! trong lúc nới 999×999       →  1405 byte   ← TUI VẼ ĐƯỢC
//! trả lại 24×80               →    25 byte   ← trống lại
//! đọc lại sau vài giây        →    25 byte   ← nên KHÔNG phải nhịp trễ
//! 40×120 → 620 byte  ·  50×180 → 1084 byte
//! ```
//!
//! Phiên **không hề treo**: TUI sống, nó chỉ không vẽ nổi ở `24×80`.
//!
//! Bản `/refresh` đầu (viết và CÀI cùng ngày) chỉ so màn TRƯỚC với màn SAU ở
//! cùng cỡ, thấy y hệt nhau (25 ↔ 25), rồi kết luận *"phiên đang treo thật,
//! không phải màn vẽ dở"* — sai, và sai một cách tự tin, thứ tệ hơn không trả
//! lời. Bằng chứng phân biệt hai ca nằm ở phép đọc THỨ BA (bản nới) mà nó đã
//! cầm trong tay rồi không dùng.
//!
//! Đây là bài kiểm khoá cái phân biệt ấy lại.

use huba::pipeline::{refresh_verdict, RefreshVerdict};

/// Màn `24×80` của phiên `[social]`: 24 dòng trắng — đúng 25 byte đã đo.
fn man_trang() -> String {
    "\n".repeat(24) + " "
}

/// Bản nới đọc được chữ — rút gọn từ chính lượt đo hôm ấy.
fn man_co_chu() -> &'static str {
    "⏺ Cổng BE còn chạy bước test. Trong lúc chờ, hỏi Hà hai quyết định \
     thuộc về Hà chứ không thuộc về tôi:"
}

#[test]
fn a_window_too_small_to_draw_is_not_reported_as_a_hung_session() {
    assert_eq!(
        refresh_verdict(Some(&man_trang()), man_co_chu(), Some(&man_trang())),
        RefreshVerdict::TooSmall,
        "nới ra thì có chữ, trả lại chiều cũ thì trống ⟹ CỬA SỔ quá nhỏ. Gọi nó là \
         'phiên treo' là gửi chủ máy đi tìm một sự cố không có, trong khi việc cần \
         làm chỉ là nới cửa sổ"
    );
}

/// ĐỐI CHỨNG NGƯỢC: thiếu bằng chứng "TUI vẽ được" thì KHÔNG được kết luận
/// cửa sổ nhỏ. Nới hết cỡ mà vẫn không có chữ là một ca khác hẳn.
#[test]
fn without_proof_that_the_tui_can_draw_we_do_not_blame_the_window_size() {
    assert_ne!(
        refresh_verdict(Some(&man_trang()), "   \n  ", Some(&man_trang())),
        RefreshVerdict::TooSmall,
        "bản nới cũng rỗng thì không có gì chứng minh TUI còn vẽ được — cấm đổ cho cỡ \
         cửa sổ, vì lời chẩn đoán ấy sẽ dẫn tới một hành động không sửa được gì"
    );
}

#[test]
fn a_screen_that_changed_after_the_redraw_says_so() {
    assert_eq!(
        refresh_verdict(Some("dòng cũ vẽ dở"), man_co_chu(), Some(man_co_chu())),
        RefreshVerdict::Redrew
    );
}

/// Màn vốn đã đúng thì `/refresh` phải nói "không có gì để sửa", chứ không
/// mượn sự im lặng ấy để dựng một chẩn đoán.
#[test]
fn a_screen_that_was_already_fine_is_not_a_problem() {
    assert_eq!(
        refresh_verdict(Some(man_co_chu()), man_co_chu(), Some(man_co_chu())),
        RefreshVerdict::Unchanged
    );
}

/// "Không đo được" là trạng thái RIÊNG (§13②) — cấm lẫn vào "không đổi".
#[test]
fn a_reading_that_failed_is_its_own_state() {
    assert_eq!(
        refresh_verdict(Some(man_co_chu()), man_co_chu(), None),
        RefreshVerdict::Unknown
    );
    assert_eq!(
        refresh_verdict(None, man_co_chu(), Some(man_co_chu())),
        RefreshVerdict::Unknown
    );
}
