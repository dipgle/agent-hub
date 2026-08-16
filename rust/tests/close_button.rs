//! Nút ⏹ trong `/terminal`, và câu hỏi xác nhận chỉ đứng ở chỗ có gì để mất.
//!
//! 🔴 Hà 2026-08-16: *"danh sách terminal thêm nút close để đóng nhanh, trạng
//! thái có đang chạy giở gì không"*. Trước đó đóng một cửa sổ là ba nhịp — bấm
//! cửa sổ, gõ `/close`, xác nhận — và anh vừa đi trọn chuỗi ấy **sáu lần liên
//! tiếp** lúc 12:25–12:29 để dọn sáu cửa sổ rỗng (`logs/hub.log`,
//! `confirm_asked` × 6, tất cả đều `Confirmed`).

/// Nút phải đi đúng route sẵn có, không đẻ nhánh riêng.
#[test]
fn the_close_button_maps_to_the_close_route() {
    assert_eq!(
        hub::telegram::callback_to_command("close:win-ttys007").as_deref(),
        Some("/close win-ttys007")
    );
    assert_eq!(
        hub::telegram::callback_to_command("close:0a109818").as_deref(),
        Some("/close 0a109818")
    );
}

/// Nút rỗng KHÔNG được thành `/close` trơn — `/close` trơn nhắm phiên ĐANG
/// THEO, nên một payload hỏng sẽ đóng nhầm một phiên hoàn toàn khác.
#[test]
fn an_empty_close_payload_is_not_a_command() {
    assert_eq!(hub::telegram::callback_to_command("close:"), None);
}

/// Hai nút của cùng một hàng phải phân biệt được nhau và nói ra chúng thuộc
/// cửa sổ nào — cùng bài học với cặp ⏎/⌫ của ô nhập.
#[test]
fn open_and_close_are_different_buttons() {
    let open = hub::telegram::callback_to_command("sess:win-ttys003");
    let close = hub::telegram::callback_to_command("close:win-ttys003");
    assert_eq!(open.as_deref(), Some("/session win-ttys003"));
    assert_eq!(close.as_deref(), Some("/close win-ttys003"));
    assert_ne!(open, close);
}

fn bare_window(working: bool) -> hub::sessions::LiveSession {
    hub::sessions::LiveSession {
        session_id: "win-ttys003".into(),
        kind: "shell".into(),
        account: String::new(),
        working,
        ..Default::default()
    }
}

/// Cửa sổ trần đứng ở dấu nhắc trống: đóng thẳng, không hỏi. Đây là toàn bộ
/// nghĩa của chữ "đóng nhanh".
#[test]
fn an_idle_bare_window_closes_without_a_question() {
    assert!(!hub::sessions::closing_needs_confirm(&bare_window(false)));
}

/// 🔴 ĐẢO CHIỀU 2026-08-16 (bản cũ: `a_busy_bare_window_still_asks`). Hà:
/// *"đóng cửa sổ trần bỏ hỏi luôn"*.
///
/// Bản cũ giữ câu hỏi cho cửa sổ trần ĐANG CHẠY DỞ, và lý lẽ nghe hợp lý — trừ
/// một điều: người bấm ⏹ là chủ máy, trên đúng cái hàng vừa in ra nó đang chạy
/// gì. Hỏi lại là bắt xác nhận một điều vừa đọc.
#[test]
fn a_busy_bare_window_closes_without_a_question_too() {
    assert!(!hub::sessions::closing_needs_confirm(&bare_window(true)));
}

/// Phiên CLI luôn hỏi, kể cả lúc rảnh — nó giữ cả một hội thoại, và `working`
/// của một phiên vừa trả lời xong là `false`.
#[test]
fn a_cli_session_always_asks() {
    let s = hub::sessions::LiveSession {
        session_id: "0a109818-1111-2222-3333-444455556666".into(),
        kind: "interactive".into(),
        account: "acc1".into(),
        working: false,
        ..Default::default()
    };
    assert!(hub::sessions::closing_needs_confirm(&s));
}

/// Một hàng `kind = "shell"` mà lại CÓ tài khoản là thứ hub không dựng ra —
/// nhưng nếu có, nó không được rơi vào đường đóng-không-hỏi. Cổng phải đóng khi
/// dữ kiện lạ, không mở.
#[test]
fn an_odd_shaped_row_falls_to_the_safe_side() {
    let mut s = bare_window(false);
    s.account = "acc3".into();
    assert!(hub::sessions::closing_needs_confirm(&s));
}
