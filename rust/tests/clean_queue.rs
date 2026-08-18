//! `/clean` — đếm hàng chờ bằng chính màn hình, không bằng niềm tin.
//!
//! 🔴 Hà 2026-08-18: *"Thêm lệnh clean xóa hết ở chờ"* (chốt sau đó: hàng chờ
//! của PHIÊN). Chuỗi dưới đây là màn THẬT, chụp lúc gõ hai dòng vào một phiên
//! đang bận — kể cả mấy khoảng trắng đuôi dòng mà TUI đệm ra.

use hub::keys::queued_count;

/// Màn thật, hai tin đang xếp hàng.
const TWO_QUEUED: &str = "  ❯ (bo qua - dong do hang cho cua hub A)                    \n\
                          \x20 ❯ (bo qua - dong do hang cho cua hub B)                    \n\
                          \n\
                          ────────────────────────────────────────\n\
                          ❯ Press up to edit queued messages\n\
                          ────────────────────────────────────────\n\
                          \x20 ⏵⏵ auto mode on · 2 shells · esc to interrupt · ← 1 agent · ↓ to manage";

/// Màn thật, phiên đang chạy nhưng KHÔNG có hàng chờ.
const BUSY_NO_QUEUE: &str = "· Noodling… (12s · ↓ 454 tokens)\n\
                             ────────────────────────────────────────\n\
                             ❯ \n\
                             ────────────────────────────────────────\n\
                             \x20 ⏵⏵ auto mode on · 1 shell · esc to interrupt";

#[test]
fn the_queue_is_counted_from_the_screen_the_cli_prints() {
    assert_eq!(queued_count(TWO_QUEUED), 2);
}

/// 🔴 Phép đo phải ĐỎ ĐƯỢC theo đúng chiều nó hay sai: dấu nhắc của ô nhập cũng
/// mở đầu bằng `❯`. Đếm cả nó thì một hàng chờ RỖNG đọc ra "còn 1" và `/clean`
/// quay đủ 25 vòng rồi báo "dọn không được" cho một việc chẳng có gì để dọn.
#[test]
fn the_prompt_line_is_not_a_queued_message() {
    assert_eq!(queued_count(BUSY_NO_QUEUE), 0);
    // Không có dòng quảng cáo `queued message` ⟹ không có hàng chờ, dù màn có
    // bao nhiêu dòng thụt lề bắt đầu bằng ❯ đi nữa (chữ cũ cuộn lên chẳng hạn).
    let old_lines = "  ❯ câu này của lượt trước\n  ❯ và câu này nữa\n❯ \n  ⏵⏵ auto mode on";
    assert_eq!(queued_count(old_lines), 0);
}

/// Màn rỗng / không đọc được ⟹ 0, không hoảng.
#[test]
fn an_empty_screen_has_no_queue() {
    assert_eq!(queued_count(""), 0);
}
