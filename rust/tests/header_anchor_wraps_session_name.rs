//! Nút 👁 phải bọc CHÍNH tên phiên trong caption, không nằm ở một nút rời.
//!
//! 🔴 Hà 2026-09-07, hai lượt vá hụt liên tiếp trước khi ra được ý này:
//! (1) đổi caption từ "Màn của [tfl5]" thành "acc2 [tfl5]" — đúng, nhưng
//! không có link nào cả; (2) thêm một nút RỜI "👁 Vào phiên" ở đáy tin —
//! *"Tôi bảo thay vào text 'màn của' cơ mà"*, rồi *"Vẫn thiếu link bao tên
//! phiên để bấm chọn vào phiên"*. Ý thật: chính chữ `[tfl5]` trong câu phải
//! là đích chạm, không phải một nút khác đứng cạnh nó.
//!
//! Cùng họ với `file_anchor_wraps_path.rs`: một chuỗi nằm GIỮA dòng, không
//! chiếm trọn dòng và không phải lệnh, cần đúng phép bọc-substring mà `📎`
//! đã có sẵn (`anchor_wraps_substring` trong `html_with_links_last`).

use huba::pipeline::{render_session_data, SessionData};

const SCREEN: &str = "📷 acc2 [tfl5]:\n\n❯ deploy đi";

fn data() -> SessionData {
    SessionData {
        sid: "7bdb4f41-dc79-4b6f-9d04-45bf37d9fcaa".into(),
        header: Some("[tfl5]".into()),
        ..Default::default()
    }
}

/// Đích chạm to bằng cả tên phiên, cùng luật đã áp cho tên tệp (25/08).
#[test]
fn the_eye_wraps_the_session_name_itself() {
    huba::telegram::set_bot_username("hub_test_bot");
    let html = render_session_data(SCREEN, &data());
    assert!(
        html.contains("👁 [tfl5]</a>"),
        "tên phiên không nằm trong thẻ <a> ⟹ không bấm được:\n{html}"
    );
    assert!(html.contains("s_7bdb4f41"), "mất route s_<id>:\n{html}");
}

/// ĐỐI CHỨNG NGƯỢC — không có `header` thì không neo gì, dù chữ `[tfl5]` có
/// đứng sẵn trên màn. Bịa neo cho một trường trống là bịa một đích chạm.
#[test]
fn no_header_means_no_anchor() {
    huba::telegram::set_bot_username("hub_test_bot");
    let mut d = data();
    d.header = None;
    let html = render_session_data(SCREEN, &d);
    assert!(
        !html.contains("s_7bdb4f41"),
        "mọc neo dù không khai header:\n{html}"
    );
}

/// Tên phiên không rỗng nhưng KHÔNG có sid (chưa xác định được phiên nào)
/// thì cũng không neo — bịa đích `s_` (chuỗi rỗng) là bịa một lệnh cụt.
#[test]
fn empty_sid_means_no_anchor() {
    huba::telegram::set_bot_username("hub_test_bot");
    let mut d = data();
    d.sid = String::new();
    let html = render_session_data(SCREEN, &d);
    assert!(!html.contains("s_\""), "mọc neo với sid rỗng:\n{html}");
}
