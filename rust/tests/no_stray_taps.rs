//! Ba lỗi HIỂN THỊ Hà đọc ra trên điện thoại ngày 2026-08-17, một tệp.
//!
//! Cả ba cùng một họ: tin đi ra đúng nội dung, mà **hình dạng của nó nói sai
//! chuyện** — một mẩu tin đọc như tin lạc, một lệnh hiện ra hai lần, và một chữ
//! không phải lệnh thì lại bấm được. Không cái nào làm `cargo test` đỏ trước
//! đây, vì không cái nào là lỗi tính toán; chúng là lỗi của thứ CHỮ cuối cùng
//! rời khỏi hub.

use hub::pipeline::{render_session_data, split_for_telegram, tame_auto_links, SessionData};

/// 🔴 *"Tin dài bị Telegram cắt làm hai mẩu, mẩu sau không có dấu nối nên đọc
/// như tin lạc"*.
#[test]
fn every_slice_of_a_long_message_says_where_it_belongs() {
    let long = (1..=400)
        .map(|i| format!("dòng số {i} của một báo cáo dài"))
        .collect::<Vec<_>>()
        .join("\n");
    let parts = split_for_telegram(&long);
    assert!(parts.len() >= 2, "chữ này phải bị cắt: {}", parts.len());

    let last = parts.len() - 1;
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            assert!(
                p.starts_with(&format!("⋯ mẩu {}/{}", i + 1, parts.len())),
                "mẩu {i} phải mở đầu bằng dấu nối: {:?}",
                &p[..40.min(p.len())]
            );
        }
        if i < last {
            assert!(
                p.trim_end()
                    .ends_with(&format!("⋯ còn mẩu {}/{} ở tin dưới", i + 2, parts.len())),
                "mẩu {i} phải khai còn tin dưới"
            );
        }
    }
    assert!(
        !parts[0].starts_with('⋯'),
        "mẩu ĐẦU không có gì ở trên nó để mà nối"
    );
}

/// Một tin vừa một mẩu thì KHÔNG đeo dấu nối — dấu ấy chỉ có nghĩa khi có mẩu
/// thứ hai, và thêm bừa là bắt người đọc đi tìm một tin không tồn tại.
#[test]
fn a_message_that_fits_stays_bare() {
    let parts = split_for_telegram("một dòng ngắn\nhai dòng ngắn");
    assert_eq!(parts.len(), 1);
    assert!(!parts[0].contains('⋯'));
}

/// Mẩu RỖNG không được gửi: Telegram trả `message text is empty`, và một mẩu
/// rỗng đeo dấu nối thì càng khó hiểu hơn.
#[test]
fn empty_slices_never_ship() {
    let parts = split_for_telegram("\n\n\n");
    assert_eq!(parts.len(), 1, "vẫn giữ đúng hợp đồng: luôn có một mẩu");
}

/// 🔴 *"Dòng lệnh in hai biến thể trùng nhau trong cùng một tin"*.
///
/// Màn bẻ một lệnh dài làm hai dòng. Chỗ chèn liên kết vẫn bám được vào nửa đầu
/// (`line_carries`), nên nếu chỗ dựng chữ hỏi bằng một phép đo KHÁC
/// (`text.contains`) thì nó tưởng lệnh vắng mặt và chép nguyên văn xuống cuối:
/// một lệnh, hai biến thể, hai chỗ bấm.
#[test]
fn a_command_the_window_broke_in_two_is_not_printed_a_second_time() {
    hub::telegram::set_bot_username("hub_test_bot");
    let cmd = "git -C /Users/hanguyen/projects/hub add rust/src/pipeline.rs rust/src/keys.rs";
    // Đúng hình dạng màn thật: cửa sổ 80 cột bẻ dòng lệnh làm hai, nên KHÔNG
    // dòng nào chứa trọn nó — `text.contains(cmd)` trượt, còn `line_carries`
    // vẫn bám được vào nửa đầu (nó đứng ĐẦU một dòng).
    let screen = "⏺ Đang chạy lệnh:\n\
                  git -C /Users/hanguyen/projects/hub add rust/src/pipeline.rs\n\
                  rust/src/keys.rs\n\
                  ⎿ xong\n"
        .to_string();
    assert!(!screen.contains(cmd), "màn phải KHÔNG chứa trọn dòng lệnh");
    let shown = render_session_data(
        &screen,
        &SessionData {
            sid: "8bf82c37-f88f-4e71-95a0-7810c07623cd".to_string(),
            cmds: vec![cmd.to_string()],
            ..Default::default()
        },
    );
    assert!(
        !shown.contains("không thấy trên màn"),
        "lệnh ĐÃ có trên màn (dù bị bẻ dòng) thì không được chép lại lần nữa:\n{shown}"
    );
    assert_eq!(
        shown
            .matches("git -C /Users/hanguyen/projects/hub add")
            .count(),
        1,
        "đúng MỘT biến thể trong tin:\n{shown}"
    );
}

/// …còn lệnh thật sự KHÔNG có trên màn thì vẫn phải được viết ra — đó là dòng
/// đáng đọc nhất của `/shot` khi cổng quyền chặn phiên in nó ra. Nhãn nay nói
/// đúng thứ hub biết chắc (không thấy trên màn), không đoán nguyên nhân.
#[test]
fn a_command_that_is_really_absent_still_gets_written_out() {
    hub::telegram::set_bot_username("hub_test_bot");
    let shown = render_session_data(
        "⏺ Phiên đang nghĩ…\n",
        &SessionData {
            sid: "8bf82c37-f88f-4e71-95a0-7810c07623cd".to_string(),
            cmds: vec!["cargo test --offline".to_string()],
            ..Default::default()
        },
    );
    assert!(shown.contains("không thấy trên màn"), "{shown}");
    assert!(shown.contains("cargo test --offline"), "{shown}");
}

/// 🔴 *"`/healthz` bị Telegram tô xanh thành lệnh bot — bấm nhầm là gửi lệnh rác
/// cho hub"*.
#[test]
fn a_slash_word_from_the_session_is_not_left_tappable() {
    assert_eq!(
        tame_auto_links("thử /healthz xem sao"),
        "thử <code>/healthz</code> xem sao"
    );
    assert_eq!(
        tame_auto_links("/Users/hanguyen/projects/hub"),
        "<code>/Users/hanguyen/projects/hub</code>",
        "đường dẫn tuyệt đối cũng bị Telegram tô — nó cũng phải được bọc"
    );
    assert_eq!(
        tame_auto_links("mở /healthz."),
        "mở <code>/healthz</code>.",
        "dấu câu dính đuôi không thuộc về cái lệnh"
    );
}

/// URL thì KHÔNG được đụng tới: dấu `/` ở giữa một từ không thành lệnh bot, và
/// bọc nó là cắt đôi một đường dẫn người ta cần chép.
#[test]
fn a_url_keeps_its_shape() {
    let url = "curl -s http://127.0.0.1:8787/healthz";
    assert_eq!(tame_auto_links(url), url);
}

/// Lệnh THẬT của hub thì giữ nguyên đích chạm — đó là thứ có ích, và bảng route
/// là chỗ duy nhất biết cái nào thật.
#[test]
fn the_hubs_own_routes_stay_tappable() {
    assert_eq!(tame_auto_links("gõ /shot để nhìn"), "gõ /shot để nhìn");
    assert_eq!(
        tame_auto_links("Bấm gửi: /key 8bf82c37 enter"),
        "Bấm gửi: /key 8bf82c37 enter"
    );
    assert_eq!(
        tame_auto_links("/shotgun"),
        "<code>/shotgun</code>",
        "chỉ khớp TRỌN tên route, không khớp phần đầu"
    );
}

/// 🔴 `@update-be` bị Telegram tô thành MENTION — cùng cái bẫy, khác ký tự.
///
/// Ảnh `/shot` `[dwork]` 2026-08-17: hai dòng `printf '@update-be …'` hiện ra
/// với `@update` xanh, dẫn tới một tài khoản không tồn tại. Đó là tên trong thư
/// viện lệnh của dwork, không phải người.
#[test]
fn an_at_word_from_the_session_is_not_left_tappable() {
    // Nguyên văn dòng trong ảnh: `@update` xanh dù đứng sau dấu nháy, còn
    // `dci/config/holiday/` giữa từ thì không.
    assert_eq!(
        tame_auto_links("printf '@update-be dci/config/holiday/\\n'"),
        "printf '<code>@update-be</code> dci/config/holiday/\\n'",
        "@ sau dấu nháy vẫn bị tô; đường dẫn giữa từ thì không"
    );
    assert_eq!(
        tame_auto_links("gửi @an cho tôi"),
        "gửi @an cho tôi",
        "tên tài khoản Telegram tối thiểu 5 ký tự — ngắn hơn thì nó không nối gì để mà gỡ"
    );
    assert_eq!(
        tame_auto_links("mail hanguyen@example.com nhé"),
        "mail hanguyen@example.com nhé",
        "@ giữa từ là địa chỉ thư, không phải mention"
    );
}

/// Chữ đã escape thì thực thể HTML phải nguyên vẹn sau khi bọc — `;` cuối một
/// thực thể không phải dấu câu để cắt.
#[test]
fn escaped_entities_survive() {
    assert_eq!(
        tame_auto_links("chạy /a&amp;b rồi thôi"),
        "chạy <code>/a&amp;b</code> rồi thôi"
    );
}
