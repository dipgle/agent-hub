//! Màn CHUẨN cho ca "bảng nhiều câu": bước **Review your answers**.
//!
//! 🔴 Hà 2026-08-18: *"Màn phiên onghut đang chờ chọn với nhiều tab, lấy đó làm
//! chuẩn để test trường hợp nhiều option"*. Fixture dưới đây là NGUYÊN VĂN màn
//! của phiên `[onghut]` lúc ấy, đọc thẳng từ Terminal — không phải bản chép tay.
//!
//! Vì sao đúng màn này mới là phép thử: nó là bước CUỐI của một bảng nhiều câu,
//! và nó thiếu đúng cái mà hai cổng an toàn của huba hay dựa vào —
//! **không có dòng chân** `Enter to select · ↑/↓ to navigate`. Chỉ còn thanh tab
//! (`←  ☒ App onghut  ☒ Dung lượng  ✔ Submit  →`) và một hộp chọn hai mục.

use huba::keys::{ask_table, has_chooser_footer, parse_choices};
use huba::pipeline::{multi_question_screen, prompt_line_text};

const SCREEN: &str = include_str!("fixtures/chooser-review-2026-08-18.txt");

/// Hộp chọn của bước Submit phải đọc ra ĐỦ hai mục.
#[test]
fn the_submit_box_is_read() {
    let choices = parse_choices(SCREEN);
    assert_eq!(
        choices,
        vec![(1, "Submit answers".to_string()), (2, "Cancel".to_string())],
        "đọc hụt hộp chọn ⟹ từ điện thoại không bấm được bước cuối"
    );
}

/// 🔴 CỬA AN TOÀN. `❯ 1. Submit answers` là CON TRỎ của hộp chọn, không phải
/// chữ trong ô nhập. Đọc nhầm thì huba dựng nút ⏎ "gửi" — và một cú Enter ở màn
/// này **CHỐT luôn Submit**, tức trả lời hộ chủ máy một việc không lùi lại được.
///
/// Ca này đặc biệt vì cổng thứ nhất (`has_chooser_footer`) MÙ: màn không có dòng
/// chân nào. Nên nó phải được cổng thứ hai (so với `parse_choices`) chặn lại —
/// đúng lý do 16/08 dựng hai cổng hỏi hai câu khác nhau.
#[test]
fn the_cursor_on_a_choice_is_not_input_box_text() {
    assert!(
        !has_chooser_footer(SCREEN),
        "màn này KHÔNG có dòng chân — nếu ngày nào nó có thì bài kiểm mất ý nghĩa, sửa fixture"
    );
    assert_eq!(
        prompt_line_text(SCREEN),
        None,
        "con trỏ hộp chọn bị đọc thành ô nhập ⟹ nút ⏎ sẽ CHỐT Submit hộ chủ máy"
    );
}

/// 🔴 Ở bước Review, hai mục trên màn KHÔNG phải lựa chọn của câu số 1.
///
/// Đo trên chính màn này (18/08): huba dựng nút bằng mã `pick_<sid>_1_<n>` — tức
/// đường của bảng nhiều câu, thứ gửi *mũi tên rồi số* để đi tới câu 1. Nhưng ở
/// đây không còn câu nào để đi tới; `Submit answers` / `Cancel` là hộp chọn đơn
/// của bước xác nhận, và phải đi bằng `k_`.
///
/// Gốc: `table` hỏi NHẬT KÝ ("bảng có nhiều câu không") rồi dùng câu trả lời ấy
/// cho một chuyện khác ("màn đang đứng ở đâu").
#[test]
fn the_review_step_is_not_a_question_of_the_table() {
    // Nhật ký vẫn nói đúng: bảng này có nhiều câu.
    assert!(
        !multi_question_screen(true, SCREEN),
        "màn Review phải đi bằng `k_`, không phải `pick_`"
    );
    // …và khi màn ĐANG đứng ở một câu còn trống thì `pick_` vẫn là đường đúng.
    let mid_table = SCREEN.replace("☒ Dung lượng", "☐ Dung lượng");
    assert!(
        multi_question_screen(true, &mid_table),
        "còn ô trống ⟹ vẫn là bảng nhiều câu"
    );
    // Hộp một câu thì không bao giờ thành bảng, dù màn nói gì.
    assert!(!multi_question_screen(false, SCREEN));
}

/// Thanh tab của bảng nhiều câu: hai câu đã tick, không còn ô trống.
#[test]
fn the_question_tabs_are_read() {
    let t = ask_table(SCREEN).expect("màn có thanh tab");
    assert_eq!(
        t.answered,
        vec![true, true],
        "hai câu đã trả lời (☒ App onghut · ☒ Dung lượng)"
    );
    assert_eq!(t.left(), 0, "không còn ô trống ⟹ bảng gửi đi được");
    assert_eq!(
        t.headers,
        vec!["App onghut".to_string(), "Dung lượng".to_string()]
    );
}
