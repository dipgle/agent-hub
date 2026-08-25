//! Ô nhập có chữ ⟹ PHẢI có ⏎/⌫, kể cả khi chữ ấy chứa một `/lệnh`.
//!
//! 🔴 Hà 2026-08-18, ảnh chụp tin `/shot`: *"ô chat có gợi ý tại sao lại không
//! có nút bấm, sao cứ update lại mất vài thứ"*. Dòng ô nhập lúc ấy là
//! `❯ Đã bấm /clean rồi, không thấy phản hồi gì` — có chữ, không có nút.

use huba::pipeline::{html_with_links, prompt_line_text, render_session_data, SessionData};

/// Màn thật rút gọn: ô nhập mang một câu CÓ `/clean` trong đó.
const SCREEN: &str = "  ▶▶ auto mode on · 2 shells · ← 1 agent · ↓ to manage\n\
                      ────────────────────────────────────────\n\
                      ❯ Đã bấm /clean rồi, không thấy phản hồi gì\n\
                      ────────────────────────────────────────\n\
                      \x20 ⏵⏵ auto mode on · 2 shells · ← 1 agent";

/// ⚠ KHÔNG BỎ DÒNG NÀY. `deep_link` trả `None` khi chưa biết tên bot, nên mọi
/// liên kết rỗng và bài kiểm đỏ VÌ PHÉP ĐO chứ không vì sản phẩm — nó đã đỏ
/// đúng như thế ở lượt chạy đầu, kể cả ca đối chứng.
fn bot() {
    huba::telegram::set_bot_username("hub_test_bot");
}

fn data() -> SessionData {
    SessionData {
        sid: "7bdb4f41-dc79-4b6f-9d04-45bf37d9fcaa".into(),
        ..Default::default()
    }
}

/// Bước 1 vẫn đúng: huba ĐỌC ĐƯỢC chữ trong ô nhập.
#[test]
fn the_box_text_is_still_read() {
    assert_eq!(
        prompt_line_text(SCREEN).as_deref(),
        Some("Đã bấm /clean rồi, không thấy phản hồi gì")
    );
}

/// Bước 2 là chỗ hỏng: neo phải bám được vào HTML cuối cùng.
///
/// Nếu `<code>` bọc `/clean` chen vào GIỮA chuỗi neo thì `html_with_links`
/// không tìm thấy neo nữa, và hai đích chạm biến mất không một tiếng động.
#[test]
fn the_send_and_clear_links_survive_a_slash_command_in_the_box() {
    bot();
    let html = render_session_data(SCREEN, &data());
    assert!(
        html.contains("send_7bdb4f41"),
        "mất nút ⏎ ngay tại dòng ô nhập:\n{html}"
    );
    // 🔄 ĐẢO CHIỀU 2026-08-25 — Hà: *"nút xóa ô nhập không cần thiết vì có lệnh
    // xóa rồi"*. Chủ đề của bài kiểm này là *"neo còn bám được khi ô nhập chứa
    // một `/lệnh`"*, và `send_` ở trên đã đo trọn điều ấy; `clr_` chỉ là bản
    // chép thứ hai của cùng một phép đo. Giữ chiều ngược để `clr_` mọc lại là đỏ.
    assert!(!html.contains("clr_7bdb4f41"), "nút ⌫ mọc lại:\n{html}");
}

/// Và neo vẫn bám khi ô nhập KHÔNG có `/lệnh` nào — ca đối chứng, để bài kiểm
/// trên chỉ đỏ vì đúng nguyên nhân của nó.
#[test]
fn a_plain_box_keeps_its_links() {
    bot();
    let plain = SCREEN.replace("/clean", "clean");
    let html = render_session_data(&plain, &data());
    assert!(html.contains("send_7bdb4f41"), "{html}");
    assert!(!html.contains("clr_7bdb4f41"), "nút ⌫ mọc lại: {html}");
}

/// Phép đo trần: chèn liên kết vào một neo có chứa `/lệnh`.
#[test]
fn a_bare_anchor_with_a_slash_command_still_takes_its_link() {
    bot();
    let text = "❯ Đã bấm /clean rồi";
    let anchors = vec![(
        "Đã bấm /clean rồi".to_string(),
        vec![("https://t.me/bot?start=send_x".to_string(), "⏎".to_string())],
    )];
    let (html, linked, unlinked) = html_with_links(text, &anchors);
    assert_eq!(linked, 1, "neo không bám được: {html} · trượt {unlinked:?}");
}
