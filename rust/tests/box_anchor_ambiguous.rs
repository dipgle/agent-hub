//! Chữ trong ô nhập TRÙNG với chữ đang bàn về nó ⟹ đừng neo nút vào giữa văn.
//!
//! 🔴 Hà 2026-08-18, ảnh chụp một tin `/shot` của phiên `[hub]`: *"Sao lại chèn
//! lệnh /clear vào ô chat"*. Trên ảnh, hai nút ⏎ và `⌫ xoá ô nhập` dán vào GIỮA
//! một câu tôi viết — ngay sau chữ `/clean` — chứ không nằm ở dòng ô nhập dưới
//! đáy màn.
//!
//! Gốc: ô nhập lúc ấy chứa đúng `/clean`, neo là CHUỖI ấy, và `html_with_links`
//! duyệt từ dòng đầu nên bám vào chỗ khớp ĐẦU TIÊN. Phiên `[hub]` thì nói về
//! lệnh của hub cả ngày, nên chữ ngắn trong ô nhập gần như luôn trùng.

use hub::pipeline::{render_session_data, SessionData};

/// Hình dạng thật: một câu văn có `/clean`, rồi ô nhập cũng đúng `/clean`.
const SCREEN: &str = "⏺ Ba việc còn chờ một cú bấm của anh, giờ chỉ còn hai:\n\
                      \x20 - /clean lúc phiên đang bận và có chữ trong hàng chờ;\n\
                      \x20 - câu xác nhận trơn lặp lại phải gộp thành ×2.\n\
                      ────────────────────────────────────────\n\
                      ❯ /clean\n\
                      ────────────────────────────────────────\n\
                      \x20 ⏵⏵ auto mode on · 2 shells · ← 1 agent · ↓ to manage";

fn data() -> SessionData {
    SessionData {
        sid: "7bdb4f41-dc79-4b6f-9d04-45bf37d9fcaa".into(),
        box_text: Some("/clean".into()),
        ..Default::default()
    }
}

/// Neo mập mờ ⟹ KHÔNG chèn liên kết vào chữ. Hai cái nút vẫn còn ở đáy tin
/// (đường lùi), nên chức năng không mất — chỉ đứng xa hơn một chút.
#[test]
fn an_ambiguous_box_text_does_not_get_linked_mid_sentence() {
    hub::telegram::set_bot_username("hub_test_bot");
    let html = render_session_data(SCREEN, &data());
    assert!(
        !html.contains("send_7bdb4f41"),
        "neo mập mờ mà vẫn chèn ⟹ nút ⌫ mời xoá một dòng KHÔNG phải ô nhập:\n{html}"
    );
}

/// …và khi chữ trong ô nhập chỉ khớp ĐÚNG MỘT dòng thì vẫn neo như thường —
/// nếu không thì bản vá này lặng lẽ gỡ mất tính năng.
#[test]
fn a_unique_box_text_still_gets_its_links() {
    hub::telegram::set_bot_username("hub_test_bot");
    let screen = SCREEN.replace("❯ /clean", "❯ dọn hàng chờ giúp tôi với");
    let mut d = data();
    d.box_text = Some("dọn hàng chờ giúp tôi với".into());
    let html = render_session_data(&screen, &d);
    assert!(html.contains("send_7bdb4f41"), "{html}");
    assert!(html.contains("clr_7bdb4f41"), "{html}");
}
