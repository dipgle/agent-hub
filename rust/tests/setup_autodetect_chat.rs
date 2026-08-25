//! Trang cài chỉ cần TOKEN — chat id thì huba tự dò.
//!
//! 🔴 Hà 2026-08-25: *"Khi cài mới hub ở ui chỉ cần nhập token thì có cơ chế tự
//! quét id chứ, bắt người dùng đi lấy id thành phức tạp"*.
//!
//! Trước lượt này ô `HUB_TELEGRAM_CHAT_ID` là **bắt buộc**, và cái gợi ý của nó
//! bảo chủ máy tự mở `api.telegram.org/bot<TOKEN>/getUpdates` rồi đọc
//! `message.chat.id` bằng mắt — đúng việc huba làm hộ được, vì nó đã có token
//! và đã nói chuyện với chính cái API ấy suốt ngày.
//!
//! Phần thuần (JSON → buồng chat) tách riêng nên kiểm được không cần mạng. Và
//! nó PHẢI được kiểm kỹ: một `chat_id` sai là **cái cổng của cả huba mở nhầm
//! buồng** (luật 7 — huba chỉ nhận lệnh từ đúng một buồng).

use huba::telegram::chat_in_updates;
use serde_json::json;

#[test]
fn a_normal_message_yields_its_chat_and_who_typed_it() {
    let v = json!({"ok": true, "result": [
        {"update_id": 1, "message": {
            "chat": {"id": 8110123, "type": "private"},
            "from": {"id": 999, "username": "ha", "first_name": "Hà"},
            "text": "chào"}}
    ]});
    assert_eq!(chat_in_updates(&v), Some((8110123, "@ha".to_string())));
}

/// Không có `username` thì lấy tên — để chủ máy vẫn nhận ra buồng nào.
#[test]
fn a_user_without_a_username_still_gets_named() {
    let v = json!({"ok": true, "result": [
        {"update_id": 1, "message": {
            "chat": {"id": 42, "type": "private"},
            "from": {"id": 999, "first_name": "Hà"}}}
    ]});
    assert_eq!(chat_in_updates(&v), Some((42, "Hà".to_string())));
}

/// Lấy tin MỚI NHẤT: chủ máy vừa nhắn xong là bấm Lưu, nên tin cuối mới là ý
/// định của họ — tin đầu có thể là của tuần trước, hoặc của một buồng khác.
#[test]
fn the_newest_update_wins() {
    let v = json!({"ok": true, "result": [
        {"update_id": 1, "message": {"chat": {"id": 111}, "from": {"username": "cu"}}},
        {"update_id": 2, "message": {"chat": {"id": 222}, "from": {"username": "moi"}}}
    ]});
    assert_eq!(chat_in_updates(&v), Some((222, "@moi".to_string())));
}

/// 🔴 CHƯA AI NHẮN ⟹ `None`, tuyệt đối không đoán một con số.
///
/// Đây là nhánh đắt nhất nếu sai: đoán bừa một `chat_id` là mở cổng ra lệnh cho
/// một buồng chat không phải của chủ máy.
#[test]
fn nothing_to_go_on_never_invents_a_chat_id() {
    for v in [
        json!({"ok": true, "result": []}),
        json!({"ok": false, "description": "Unauthorized"}),
        json!({"ok": true}),
        json!({}),
        // Cập nhật CÓ, nhưng không phải tin nhắn — không có `chat` nào để lấy.
        json!({"ok": true, "result": [{"update_id": 1, "callback_query": {"from": {"id": 7}}}]}),
        // Tin có, nhưng thiếu `chat.id`.
        json!({"ok": true, "result": [{"update_id": 1, "message": {"from": {"id": 7}}}]}),
    ] {
        assert_eq!(chat_in_updates(&v), None, "đoán bừa từ: {v}");
    }
}

/// Tin đã sửa và bài trong kênh cũng mang `chat` — ba chỗ, cùng một luật.
#[test]
fn edited_messages_and_channel_posts_count_too() {
    let v = json!({"ok": true, "result": [
        {"update_id": 1, "edited_message": {"chat": {"id": 55}, "from": {"username": "ha"}}}
    ]});
    assert_eq!(chat_in_updates(&v), Some((55, "@ha".to_string())));

    let v = json!({"ok": true, "result": [
        {"update_id": 1, "channel_post": {"chat": {"id": -100777}}}
    ]});
    assert_eq!(chat_in_updates(&v), Some((-100777, "?".to_string())));
}

/// Id ÂM là hợp lệ — nhóm và kênh của Telegram mang id âm. Chặn nó là chặn một
/// cấu hình có thật.
#[test]
fn a_negative_group_id_is_not_rejected() {
    let v = json!({"ok": true, "result": [
        {"update_id": 1, "message": {"chat": {"id": -1001234567890i64}, "from": {"username": "ha"}}}
    ]});
    assert_eq!(
        chat_in_updates(&v).map(|(id, _)| id),
        Some(-1001234567890i64)
    );
}
