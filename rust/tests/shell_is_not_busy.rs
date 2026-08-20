//! "Có shell con" ≠ "phiên đang chạy" — và ngược lại cũng ≠ "phiên đã dừng".
//!
//! 🔴 Hai lỗi ngược chiều nhau, cách nhau hai ngày, cùng một chỗ trong mã:
//!
//! - 16/08, Hà: *"Rõ ràng dưới cùng có shell đang chạy, nhưng ở danh sách phiên
//!   lại báo đã dừng"* ⟹ bản vá: có shell con thì tính là đang chạy.
//! - 18/08, Hà: *"lệnh session hiện danh sách phiên với icon biểu thị đang chạy
//!   nhưng thực ra phiên đang dừng"* ⟹ một lệnh chạy NỀN cũng là đúng cái tiến
//!   trình shell ấy, và nó sống tiếp sau khi phiên đã trả lời xong.
//!
//! Bài kiểm này giữ CẢ HAI đứng cùng lúc, nên bản vá sau không xoá bản vá trước.

use huba::keys::{is_busy, screen_running};
use huba::sessions::shell_verdict;

/// Màn THẬT của một phiên đang chạy. Dòng đầu là kiểu đang-chạy **không có
/// ngoặc** — nguyên văn từ ảnh màn huba gửi đi 18/08, và chính là ca `is_busy`
/// đọc sai.
const RUNNING: &str = "✻ Cogitated for 37m 51s · 2 shells still running\n\
                       ────────────\n❯ \n────────────\n\
                       \x20 ⏵⏵ auto mode on · 1 shell · esc to interrupt · ← 1 agent";

/// Dòng trạng thái THẬT của một phiên ĐANG CHỜ người gõ, nhưng còn lệnh chạy
/// nền — nguyên văn từ ảnh Hà gửi 18/08.
const IDLE_WITH_BACKGROUND: &str = "❯ Đã bấm /clean rồi, không thấy phản hồi gì\n\
                                    ────────────\n\
                                    \x20 ⏵⏵ auto mode on · 2 shells · ← 1 agent · ↓ to manage";

#[test]
fn a_background_command_does_not_make_the_session_look_busy() {
    // Dòng chân không có `esc to interrupt` ⟹ phiên đang chờ, dù có 2 shell.
    assert_eq!(screen_running(IDLE_WITH_BACKGROUND), Some(false));
    assert!(
        !shell_verdict(true, Some(false)),
        "có shell nền mà màn nói đang chờ ⟹ không được tính là đang chạy"
    );
}

/// 🔴 Ca này là lý do phép đo KHÔNG được dùng `is_busy`. Màn thật của một phiên
/// đang chạy có thể mang dạng `✻ Cogitated for 37m 51s ·` — không ngoặc — nên
/// `is_busy` đọc ra "rảnh"; lấy đó lật ngược bằng chứng shell là xoá luôn bản vá
/// 16/08. Bài kiểm giữ cả hai câu: `is_busy` mù ở đây, `screen_running` thì không.
#[test]
fn a_real_turn_still_counts_as_busy() {
    assert!(
        !is_busy(RUNNING),
        "nếu ngày nào `is_busy` đọc được ca này thì sửa lại ghi chú, đừng sửa lén phép đo"
    );
    assert_eq!(screen_running(RUNNING), Some(true));
    assert!(
        shell_verdict(true, Some(true)),
        "bản vá 16/08 phải còn nguyên"
    );
}

/// Không có dòng chân ⟹ KHÔNG đo được, và phải nói đúng như thế.
#[test]
fn a_screen_without_a_footer_is_not_a_verdict() {
    assert_eq!(
        screen_running("chỉ có mấy dòng chữ\nkhông có dòng chân"),
        None
    );
}

/// Không đọc được màn ⟹ GIỮ bằng chứng cũ. Mù không phải lý do để lật ngược một
/// phép đo đã có (cùng luật với `keys::look` trả `Blind`).
#[test]
fn a_blind_screen_keeps_the_shell_evidence() {
    assert!(shell_verdict(true, None));
    assert!(!shell_verdict(false, None));
    assert!(!shell_verdict(false, Some(true)), "không có shell thì thôi");
}
