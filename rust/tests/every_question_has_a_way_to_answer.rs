//! Câu hỏi nào hiện ra thì phải có ĐƯỜNG trả lời — kể cả câu đang mở.
//!
//! 🔴 Hà 2026-08-27, ảnh `/shot` của `[dwork]` 574e5be2: *"Có option nhưng ko
//! chọn được"*. Đo trên chính tin ấy (log `10:23:46Z`):
//!
//! ```text
//! Câu 2 (KHÔNG hiện trên màn) → 3 đích chạm /pick_574e5be2_2_*
//! Câu 1 (ĐANG mở, 5 lựa chọn) → 0 đích chạm, 0 ký tự ☑
//! ```
//!
//! Câu đang mở rơi trọn vào khe giữa hai đường:
//! * đường **trong chữ** chèn ☑ tại dòng lựa chọn, dựng từ `keys::parse_choices`
//!   — thứ **mù** với bảng `AskUserQuestion` (mỗi lựa chọn có một dòng mô tả bên
//!   dưới, đúng hình dạng luật "liền dòng" loại bỏ). CLAUDE.md ghi sự mù ấy từ
//!   14/08, kèm số đo: **0 mục** trên chính màn ấy.
//! * đường **ở đáy** (`ask_command_lines`) thì **cố ý** bỏ câu đầu, vì nó tin
//!   rằng đường kia đã lo.
//!
//! Hai đường, mỗi đường đúng một mình, và chẳng đường nào hỏi *"câu này đã có
//! đường trả lời chưa"*. Đúng hình dạng luật 14 cấm — nhưng soi từ đầu ngược
//! lại: không phải một cái nút không dẫn vào đâu, mà một câu hỏi **không có nút
//! nào cả**, trong khi huba vẫn vẽ đủ 5 lựa chọn ra màn cho người ta nhìn.

use huba::pipeline::ask_command_lines;
use huba::sessions::{Asking, Question};

fn bang_hai_cau() -> Asking {
    Asking {
        header: "Thứ tự ship".into(),
        question: "Con lỗi ca đêm đang ăn tiền thật ngay lúc này".into(),
        options: vec![
            "Tách ship trước, gấp (Recommended)".into(),
            "Gộp một đợt sáu bề mặt".into(),
            "Chỉ đo mức thiệt hại trước".into(),
        ],
        multi: false,
        rest: vec![Question {
            header: "Cách vá".into(),
            question: "Vá ở đâu".into(),
            options: vec![
                "Vá gốc — giữ mốc tuyệt đối (Recommended)".into(),
                "Vá vị từ :271 cho đúng".into(),
                "Để tôi đo rồi đề xuất".into(),
            ],
            multi: false,
        }],
    }
}

/// Ca đã cắn: KHÔNG dựng được đích chạm nào trong chữ ⟹ khu chữ ở đáy phải phủ
/// CẢ câu đang mở. Fail-closed: thừa một khu chữ còn hơn một câu không trả lời được.
#[test]
fn when_nothing_is_tappable_inline_the_open_question_still_gets_targets() {
    let chu = ask_command_lines("574e5be2", &bang_hai_cau(), false);
    assert!(
        chu.contains("/pick_574e5be2_1_1"),
        "câu ĐANG MỞ phải có đích chạm — đây đúng là chỗ Hà nhìn thấy 5 lựa chọn \
         mà không bấm được cái nào.\n{chu}"
    );
    for n in 1..=3 {
        assert!(
            chu.contains(&format!("/pick_574e5be2_1_{n}")),
            "thiếu lựa chọn {n} của câu đang mở"
        );
    }
    assert!(
        chu.contains("/pick_574e5be2_2_1"),
        "và câu sau vẫn phải còn nguyên đường của nó"
    );
}

/// ĐỐI CHỨNG NGƯỢC: bài trên chỉ có nghĩa nếu `skip_current = true` THẬT SỰ bỏ
/// câu đầu. Thiếu vế này thì một hàm luôn in mọi câu cũng làm bài trên xanh, và
/// lúc ấy nó không đo cái nó khai.
#[test]
fn skipping_the_current_question_really_skips_it() {
    let chu = ask_command_lines("574e5be2", &bang_hai_cau(), true);
    assert!(
        !chu.contains("/pick_574e5be2_1_"),
        "bỏ câu đang hiện thì đích chạm của nó KHÔNG được nằm ở đáy — nếu không \
         thì một màn có ☑ trong chữ sẽ mọc thêm một khu chữ trùng lặp.\n{chu}"
    );
    assert!(
        chu.contains("/pick_574e5be2_2_1"),
        "…mà các câu sau thì vẫn phải có"
    );
}

/// Dù đi đường nào, phải luôn còn cách GỬI. Một bảng trả lời xong mà không gửi
/// được thì vẫn là phiên đứng kẹt — đúng ca 13/08 sinh ra `/pick`.
#[test]
fn there_is_always_a_way_to_submit() {
    for bo_cau_dau in [true, false] {
        let chu = ask_command_lines("574e5be2", &bang_hai_cau(), bo_cau_dau);
        assert!(
            chu.contains("/send_574e5be2"),
            "thiếu dòng gửi khi skip_current={bo_cau_dau}"
        );
    }
}
