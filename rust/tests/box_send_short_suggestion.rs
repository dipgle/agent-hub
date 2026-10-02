//! Gợi ý NGẮN trong ô nhập (`❯ Ok`) cũng phải có nút ⏎ — và hai hàm trả lời
//! câu "ô nhập đang có gì" phải trả lời GIỐNG NHAU.
//!
//! 🔴 Hà 2026-10-02, ảnh một tin `/shot` của phiên `[huba]`: *"Xử lý gì xong mất
//! enter để gửi nội dung gợi ý trong ô chat"* rồi *"Lại sao mỗi lần cập nhật thì
//! lỗi cũ lại lại vậy … bị rất nhiều lần rồi"*. Ô nhập mang gợi ý mờ `❯ Ok`, tin
//! không có ⏎.
//!
//! Gốc đo được: câu ấy có HAI chỗ trả lời, hai ngưỡng — `keys::input_box_text`
//! nhận từ 2 ký tự, `pipeline::prompt_line_text` (thứ dựng nút) đòi từ 4 (ngưỡng
//! có từ 14–16/08, `20f6d85` · `2485080`). Mọi gợi ý dưới 4 ký tự — `Ok`, `ừ`,
//! `có` — chưa bao giờ có nút. Hai bản sao của một luật thì lần vá sau chỉ chạm
//! một bản; nên bài kiểm ở đây không chỉ đo ca `Ok`, nó đo cả việc HAI HÀM KHỚP
//! NHAU trên cùng một bộ màn.

use huba::keys::input_box_text;
use huba::pipeline::{prompt_line_text, render_session_data, SessionData};

/// Màn thật rút gọn, đúng hình dạng ảnh 2026-10-02 14:30.
const SCREEN_OK: &str = "⏺ [huba] Phiên này không còn việc nào đang chạy,\n\
                         \x20 ngoài lượt cổng ấy.\n\
                         \n\
                         ✻ Sautéed for 55s · done 1:53 PM\n\
                         \n\
                         ────────────────────────────────────────\n\
                         ❯ Ok\n\
                         ────────────────────────────────────────\n\
                         \x20 ⏵⏵ auto mode on (shift+tab to cycle) · ← for agents";

/// ⚠ Không bỏ: thiếu tên bot thì `deep_link` trả `None`, bài kiểm đỏ vì phép đo.
fn bot() {
    huba::telegram::set_bot_username("hub_test_bot");
}

fn data() -> SessionData {
    SessionData {
        sid: "26d92e09-fa3c-4c7c-9c98-b93ef5c3d4ae".into(),
        ..Default::default()
    }
}

#[test]
fn a_two_letter_suggestion_is_read_as_box_text() {
    assert_eq!(prompt_line_text(SCREEN_OK).as_deref(), Some("Ok"));
}

#[test]
fn a_two_letter_suggestion_gets_its_send_button() {
    bot();
    let html = render_session_data(SCREEN_OK, &data());
    assert!(
        html.contains("send_26d92e09"),
        "ô nhập `❯ Ok` mà không có nút ⏎:\n{html}"
    );
}

/// HAI hàm, MỘT câu hỏi ⟹ cùng câu trả lời trên mọi màn có ô nhập và không có
/// hộp chọn. Lệch ở bất kỳ màn nào là lần vá sau sẽ lại chỉ chạm một bên.
#[test]
fn both_readers_of_the_box_agree() {
    let line = "────────────────────────────────────────";
    let foot = "  ⏵⏵ auto mode on (shift+tab to cycle)";
    for inside in ["❯ Ok", "❯ ừ", "❯ có", "❯ làm tiếp đi", "❯ x", "❯ ", "❯"] {
        let screen = format!("⏺ xong.\n\n{line}\n{inside}\n{line}\n{foot}");
        assert_eq!(
            prompt_line_text(&screen).as_deref().map(str::trim),
            input_box_text(&screen).as_deref().map(str::trim),
            "hai hàm đọc ô nhập trả lời KHÁC NHAU cho `{inside}`"
        );
    }
}

/// Đối chứng: một ký tự trơ trọi (dấu nhắc mang một ký tự trang trí) vẫn KHÔNG
/// phải câu để gửi — lý do ngưỡng cũ tồn tại, không được mất theo.
#[test]
fn a_single_character_is_still_not_a_message() {
    bot();
    let one = SCREEN_OK.replace("❯ Ok", "❯ x");
    assert_eq!(prompt_line_text(&one), None);
    let html = render_session_data(&one, &data());
    assert!(
        !html.contains("send_26d92e09"),
        "dựng ⏎ cho một ký tự:\n{html}"
    );
}
