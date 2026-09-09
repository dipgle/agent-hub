//! "Không có ô nhập" phải là một CÂU TRẢ LỜI, không phải chỗ để đoán.
//!
//! 🔴 Gốc của 197 cú Enter bắn vào hư không, đo trên `logs/huba.log` ngày
//! 2026-09-08, 13:22→15:37Z:
//!
//! - `auto_unstick_box_firing` **197 lượt** vào 5 phiên;
//! - `auto_unstick_box_failed` **0 lần** — không một tín hiệu hụt nào;
//! - `projects-ef`: **39 lượt**, `text_len` đứng nguyên **55** suốt 2h15.
//!
//! Chữ ấy không đổi vì nó KHÔNG PHẢI chữ trong ô nhập: màn ấy không có ô nhập
//! nào, và `box_region` có đường lùi "bốn dòng không rỗng cuối màn" nên nó trả
//! về cái đuôi màn tĩnh, rồi `input_box_text` gói lại thành `Some(...)` — đọc ra
//! y hệt "một câu đang nằm chờ gửi". Không cú Enter nào làm một băng chữ tĩnh
//! đổi được, nên vòng lặp không có điểm dừng.
//!
//! Bằng chứng loại trừ: CÙNG phiên ấy `keys_typed` chạy tốt 11 lượt trong cùng
//! khoảng — cửa sổ, cách gửi phím và tài khoản đều lành.
//!
//! Luật thì `CLAUDE.md` đã ghi từ trước (*"rơi âm thầm về đường lùi bốn dòng
//! cuối — đủ để mọi thứ trông vẫn chạy, và đã trả giá"* · *"Một cái neo duy
//! nhất: `keys::box_start`"*), và `keys::still_in_box` đã tự gác đúng như vậy.
//! Chỗ thiếu chỉ là `input_box_text` chưa nhận cái phanh ấy.

use huba::keys::{box_start, input_box_text, still_in_box};

/// Màn THẬT có ô nhập — hai vạch kẹp một dòng `❯`. Giữ nguyên hình dạng đang
/// dùng ở `tests/telegram.rs`, để bản vá không lặng lẽ giết đường chạy đúng.
const CO_O_NHAP: &str = "\
  ⎿  Tip: Run tasks in the cloud while you keep coding locally
────────────────────────────────────────
❯ làm quota phép đi
────────────────────────────────────────
  ⏵⏵ auto mode on (shift+tab to cycle) · esc to interrupt";

/// Màn KHÔNG có ô nhập: phiên đang hiện băng chữ tĩnh. Đây đúng hình dạng đã
/// làm huba bắn Enter 39 lượt vào `projects-ef`.
const KHONG_O_NHAP: &str = "\
⏺ Đã vá xong cổng và đẩy lên.

  Phần còn lại chờ hạn mức mở lại rồi làm tiếp.

You've hit your session limit · resets 10pm (Asia/Saigon)";

/// 🔴 Ca đã xảy ra thật: không có ô nhập ⟹ `None`, không được bịa ra chữ.
#[test]
fn man_khong_co_o_nhap_thi_khong_co_chu_dang_cho() {
    assert!(
        box_start(KHONG_O_NHAP).is_none(),
        "màn mẫu này cố ý KHÔNG có ô nhập — nếu nó có thì bài kiểm đang đo nhầm thứ"
    );
    assert_eq!(
        input_box_text(KHONG_O_NHAP),
        None,
        "đuôi màn tĩnh bị đọc thành 'câu đang chờ gửi' — đúng lỗi 08/09, \
         197 cú Enter vào hư không"
    );
}

/// Chiều còn lại, và nó quan trọng ngang chiều trên: bản vá KHÔNG được giết
/// đường chạy đúng. Một cái phanh làm hàm luôn trả `None` cũng làm bài trên
/// xanh.
#[test]
fn man_co_o_nhap_van_doc_duoc_chu() {
    assert!(box_start(CO_O_NHAP).is_some(), "màn mẫu này phải có ô nhập");
    assert_eq!(
        input_box_text(CO_O_NHAP).as_deref(),
        Some("làm quota phép đi"),
        "vá xong mà không còn đọc được chữ trong ô nhập thì hỏng cả tính năng"
    );
}

/// MỘT cái neo, không phải hai. `still_in_box` đã gác `box_start` từ trước; nay
/// `input_box_text` gác cùng chỗ. Hai hàm cùng đọc một cái màn mà trả lời trái
/// nhau là mở đường cho đúng lớp lỗi này quay lại ở cửa bên cạnh.
#[test]
fn hai_ham_doc_man_phai_dung_chung_mot_neo() {
    for man in [KHONG_O_NHAP, CO_O_NHAP] {
        let co_neo = box_start(man).is_some();
        assert_eq!(
            input_box_text(man).is_some(),
            co_neo,
            "`input_box_text` phải im đúng lúc không có neo:\n{man}"
        );
        // `still_in_box` hỏi một câu khác ("chữ tôi vừa gõ còn đó không") nhưng
        // phải im ở CÙNG điều kiện — không có ô nhập thì không có gì để còn.
        assert!(
            co_neo || !still_in_box(man, "làm quota phép đi"),
            "không có ô nhập mà `still_in_box` vẫn khai có chữ:\n{man}"
        );
    }
}
