//! `/refresh` phải phân biệt **cửa sổ quá nhỏ** với **phiên treo** — và phải
//! làm được thế mà không mượn một phép so luôn đổi.
//!
//! 🔴 Hàm `refresh_verdict` viết lại HAI lần trong một ngày, vì cùng một kiểu
//! sai: hỏi câu dễ đo thay vì câu đáng hỏi.
//!
//! **Lần một** — Hà: *"Phiên này đang bị kẹt không làm được gì, treo view"*. Đo
//! thẳng trên cửa sổ ấy (`[social]`, `ttys013`, window 16384):
//!
//! ```text
//! cửa sổ 24×80 (cỡ đang bị)  →    25 byte   ← trống trơn
//! nới 999×999                 →  1405 byte   ← TUI VẼ ĐƯỢC
//! trả lại 24×80               →    25 byte   ← và sau vài giây vẫn 25
//! 40×120 → 620 byte  ·  50×180 → 1084 byte
//! ```
//!
//! Bản đầu so màn TRƯỚC với màn SAU **ở cùng một cỡ**, thấy y hệt (25 ↔ 25), rồi
//! kết luận *"phiên đang treo thật"*. Phiên không treo; cửa sổ quá nhỏ để vẽ.
//!
//! **Lần hai** — Hà bấm 🔄 rồi báo *"cửa sổ vẫn không to lên"*. Nhật ký:
//! `session 171d0566 [huba] · size 24×80 · verdict Redrew`. Cùng phép so ấy hỏng
//! theo chiều NGƯỢC LẠI: phiên đang chạy thì hai lượt đọc cách nhau vài giây
//! **luôn** khác nhau — `Redrew` là công của phiên vừa in thêm chữ, không phải
//! của cú vẽ lại. Dương giả ở đây, âm giả ở kia ⟹ nó không đo cái nó khai.
//!
//! Nay chỉ còn hai lượt đọc ở HAI cỡ, trả lời đúng câu đáng hỏi: **nới ra thì
//! đọc được không.**

use huba::pipeline::{refresh_verdict, RefreshVerdict};

/// Màn `24×80` của phiên `[social]`: 24 dòng trắng — đúng 25 byte đã đo.
fn man_trang() -> String {
    "\n".repeat(24) + " "
}

fn man_co_chu() -> &'static str {
    "⏺ Cổng BE còn chạy bước test. Trong lúc chờ, hỏi Hà hai quyết định \
     thuộc về Hà chứ không thuộc về tôi:"
}

#[test]
fn a_window_too_small_to_draw_is_not_reported_as_a_hung_session() {
    assert_eq!(
        refresh_verdict(Some(&man_trang()), Some(man_co_chu())),
        RefreshVerdict::WasBlankNowDraws,
        "cỡ cũ trắng, nới ra có chữ ⟹ CỬA SỔ quá nhỏ. Gọi nó là 'phiên treo' là gửi \
         chủ máy đi tìm một sự cố không có, trong khi việc cần làm chỉ là nới cửa sổ"
    );
}

/// 🔴 CA DƯƠNG GIẢ — chính là ca đã cắn lần hai.
///
/// Phiên đang chạy thì chữ trên màn khác nhau ở mọi lượt đọc. Phép đo cũ đọc sự
/// khác nhau ấy thành "vẽ lại có tác dụng"; phép đo mới phải TRƠ với nó, vì cả
/// hai lượt đều có chữ nên câu trả lời đúng chỉ là *"màn vẽ được"*.
#[test]
fn a_busy_session_printing_new_text_does_not_earn_a_redraw_verdict() {
    assert_eq!(
        refresh_verdict(Some("⏺ đang chạy… bước 1"), Some("⏺ đang chạy… bước 2")),
        RefreshVerdict::Draws,
        "hai lượt đọc khác nhau CHỈ vì phiên vừa in thêm chữ — không được quy công \
         cho cú vẽ lại, và cũng không được đọc thành 'cửa sổ quá nhỏ'"
    );
}

/// ĐỐI CHỨNG NGƯỢC: nới hết cỡ mà VẪN không có chữ thì cấm đổ cho cỡ cửa sổ —
/// lời chẩn đoán ấy dẫn tới một hành động không sửa được gì.
#[test]
fn without_proof_that_the_tui_can_draw_we_do_not_blame_the_window_size() {
    let pq = refresh_verdict(Some(&man_trang()), Some("   \n  "));
    assert_eq!(
        pq,
        RefreshVerdict::StillBlank,
        "nới rồi vẫn trống là trạng thái RIÊNG — đây là ca duy nhất được phép nhắc \
         chữ 'treo', và vẫn chỉ được nhắc, không được khẳng định"
    );
    assert_ne!(pq, RefreshVerdict::WasBlankNowDraws);
}

/// Màn vốn đã đúng thì `/refresh` không được mượn sự im lặng ấy để dựng chẩn đoán.
#[test]
fn a_screen_that_was_already_fine_is_not_a_problem() {
    assert_eq!(
        refresh_verdict(Some(man_co_chu()), Some(man_co_chu())),
        RefreshVerdict::Draws
    );
}

/// "Không đo được" là trạng thái RIÊNG (§13②) — cấm lẫn vào bất kỳ màu nào khác.
#[test]
fn a_reading_that_failed_is_its_own_state() {
    assert_eq!(
        refresh_verdict(Some(man_co_chu()), None),
        RefreshVerdict::Unknown
    );
    assert_eq!(
        refresh_verdict(None, Some(man_co_chu())),
        RefreshVerdict::Unknown
    );
}
