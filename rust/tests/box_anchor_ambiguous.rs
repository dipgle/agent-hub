//! Chữ trong ô nhập TRÙNG với chữ đang bàn về nó ⟹ đừng neo nút vào giữa văn.
//!
//! 🔴 Hà 2026-08-18, ảnh chụp một tin `/shot` của phiên `[huba]`: *"Sao lại chèn
//! lệnh /clear vào ô chat"*. Trên ảnh, hai nút ⏎ và `⌫ xoá ô nhập` dán vào GIỮA
//! một câu tôi viết — ngay sau chữ `/clean` — chứ không nằm ở dòng ô nhập dưới
//! đáy màn.
//!
//! Gốc: ô nhập lúc ấy chứa đúng `/clean`, neo là CHUỖI ấy, và `html_with_links`
//! duyệt từ dòng đầu nên bám vào chỗ khớp ĐẦU TIÊN. Phiên `[huba]` thì nói về
//! lệnh của huba cả ngày, nên chữ ngắn trong ô nhập gần như luôn trùng.

use huba::pipeline::{render_session_data, SessionData};

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

/// 🔄 ĐẢO CHIỀU 2026-08-25 — neo trùng chỗ thì bám **dòng CUỐI**, không bỏ cuộc.
///
/// Bản gốc của bài này khoá hành vi *"mập mờ ⟹ không chèn, để nút ở đáy"*. Nó
/// đúng với dữ kiện hồi 18/08, và sai kể từ khi có `html_with_links_last`:
/// ô nhập là **dòng dấu nhắc cuối cùng** theo đúng định nghĩa `prompt_line_text`
/// dùng để đọc ra nó, nên mọi bản trùng đều nằm PHÍA TRÊN. Không có gì mập mờ
/// để mà né — chỉ có một phép dò hỏi sai câu.
///
/// Hà 2026-08-25, ảnh một tin không có nút ⏎: *"sao ô chờ gợi ý mờ lại không có
/// nút enter"*. Cái giá của việc né: nút rơi xuống đáy, nơi nó không nói được
/// nó thuộc dòng nào.
///
/// Vế THẬT của bài kiểm không mất, nó chỉ chặt hơn: nút phải nằm đúng dòng ô
/// nhập, **không** nằm ở lần nhắc `/clean` giữa câu văn phía trên.
#[test]
fn an_ambiguous_box_text_binds_the_last_line_not_the_first() {
    huba::telegram::set_bot_username("hub_test_bot");
    let html = render_session_data(SCREEN, &data());
    let neo = html
        .find("send_7bdb4f41")
        .unwrap_or_else(|| panic!("mất nút ⏎ vì chữ trùng chỗ khác:\n{html}"));
    let nhac_dau = html
        .find("/clean")
        .expect("lần nhắc trong câu văn phải còn");
    assert!(
        nhac_dau < neo,
        "neo bám lần nhắc ĐẦU giữa câu văn ⟹ cú Enter đi vào một dòng KHÔNG \
         phải ô nhập:\n{html}"
    );
}

/// …và khi chữ trong ô nhập chỉ khớp ĐÚNG MỘT dòng thì vẫn neo như thường —
/// nếu không thì bản vá này lặng lẽ gỡ mất tính năng.
#[test]
fn a_unique_box_text_still_gets_its_links() {
    huba::telegram::set_bot_username("hub_test_bot");
    let screen = SCREEN.replace("❯ /clean", "❯ dọn hàng chờ giúp tôi với");
    let mut d = data();
    d.box_text = Some("dọn hàng chờ giúp tôi với".into());
    let html = render_session_data(&screen, &d);
    assert!(html.contains("send_7bdb4f41"), "{html}");
    // 🔴 ĐẢO CHIỀU 2026-08-25 — Hà: *"nút xóa ô nhập không cần thiết vì có lệnh
    // xóa rồi"*. `⌫` đi hẳn: hai đích chạm cạnh nhau, một bên GỬI một bên XOÁ,
    // cả hai đều không lùi lại được. Bài kiểm ở lại để khoá chiều mới, chứ
    // không xoá — nếu `clr_` mọc lại thì phải đỏ.
    assert!(
        !html.contains("clr_7bdb4f41"),
        "nút xoá ô nhập mọc lại:\n{html}"
    );
}
