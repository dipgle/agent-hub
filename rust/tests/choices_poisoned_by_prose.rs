//! Một dòng VĂN XUÔI đánh số không được giết cả hộp chọn thật.
//!
//! 🔴 Hà 2026-08-25, ảnh `/shot` phiên `[dwork]`: *"Có option nhưng không có
//! chọn được"*. Màn ấy có hộp `AskUserQuestion` 5 lựa chọn rành rành, dòng chân
//! `Enter to select · ↑/↓ to navigate · Esc to cancel` nằm ngay dưới, mà
//! `parse_choices` trả về **0**.
//!
//! Màn thật lưu ở `fixtures/shot-choices-poisoned-2026-08-25.txt`, lấy nguyên
//! văn từ `logs/huba.log` (bản ghi `channel_command_handled` lúc 13:00:11Z).
//!
//! GỐC, đo được bằng byte trên chính màn ấy — `parse_choices` quét CẢ màn, và
//! nửa trên màn là văn xuôi của phiên có ba dòng đánh số:
//!
//! ```text
//! dòng 10 · "1." · 153 byte → bị trần 120 loại
//! dòng 11 · "2." · 115 byte → LỌT
//! dòng 12 · "3." · 185 byte → bị loại
//! dòng 28 · "1." ·  56 byte → hộp thật
//! dòng 31 · "2." ·  53 byte
//! dòng 33 · "3." ·  55 byte
//! dòng 35 · "4." ·  15 byte
//! dòng 37 · "5." ·  15 byte
//! ```
//!
//! Nên `out[0]` là cái VĂN XUÔI ở dòng 11, `first = 2`, và phép kiểm liên tiếp
//! (`*n != first + i`) gãy ngay phần tử sau (1 ≠ 3) ⟹ **vứt sạch cả hộp**.
//!
//! Cái trần 120 byte không phải một cái cổng — nó là XỔ SỐ: hai dòng văn xuôi
//! bị loại nhờ may, dòng thứ ba sống sót và đầu độc cả danh sách. Và nó đếm
//! BYTE nên chữ Việt có dấu chỉ được ~nửa số ký tự so với tiếng Anh.

use huba::keys::parse_choices;

const MAN: &str = include_str!("fixtures/shot-choices-poisoned-2026-08-25.txt");

/// Hộp thật phải ra đủ 5 lựa chọn, đánh số 1..5.
#[test]
fn the_real_chooser_survives_numbered_prose_higher_up_the_screen() {
    let got = parse_choices(MAN);
    let so: Vec<usize> = got.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        so,
        vec![1, 2, 3, 4, 5],
        "hộp chọn thật bị một dòng văn xuôi đánh số làm hỏng:\n{got:#?}"
    );
}

/// …và nhãn phải là nhãn của HỘP, không phải của văn xuôi.
#[test]
fn the_labels_come_from_the_box_not_from_the_prose() {
    let got = parse_choices(MAN);
    let nhan: Vec<&str> = got.iter().map(|(_, l)| l.as_str()).collect();
    assert!(
        nhan.iter().any(|l| l.contains("Huỷ cả 3")),
        "mất nhãn của lựa chọn 1: {nhan:?}"
    );
    assert_eq!(nhan.last().copied(), Some("Chat about this"), "{nhan:?}");
    assert!(
        !nhan.iter().any(|l| l.contains("dead-end phiếu")),
        "nhãn lấy nhầm từ văn xuôi nửa trên màn: {nhan:?}"
    );
}

/// 🔴 HÀNG RÀO NGƯỢC — bỏ dòng chân đi thì màn ấy KHÔNG còn là hộp chọn.
///
/// Đây là thứ giữ cho bản vá không nới thành "mọi đoạn văn đánh số đều mọc ☑",
/// đúng con bug 21/08 (`[tfl5]`): huba gắn ☑ vào ba dòng của một đoạn văn, Hà
/// bấm, rồi huba tự đoán *"hộp này có thể không nhận phím số"* — không phải hộp
/// nào không nhận, mà không có hộp nào cả.
#[test]
fn without_the_footer_the_same_screen_is_not_a_chooser() {
    let khong_chan: String = MAN
        .lines()
        .filter(|l| !l.contains("to navigate"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        parse_choices(&khong_chan).is_empty(),
        "không có dòng chân mà vẫn dựng hộp chọn từ văn xuôi"
    );
}

/// Trần độ dài nhãn phải đếm KÝ TỰ, không đếm byte — nếu không thì cùng một
/// câu, tiếng Việt bị cắt ở khoảng nửa độ dài của tiếng Anh.
#[test]
fn a_long_vietnamese_label_is_not_cut_shorter_than_an_english_one() {
    // 95 ký tự · 136 byte — nằm ĐÚNG giữa hai trần, nên nó phân biệt được hai
    // cách đếm. Đo bằng chính hai khẳng định dưới, không tin vào mắt.
    let viet = "Đề nghị huỷ hết rồi làm lại từ đầu để tránh lệch sổ giữa hai bên nhân sự và kế toán của quý này";
    assert!(viet.chars().count() < 120, "câu thử phải dưới trần ký tự");
    assert!(
        viet.len() > 120,
        "câu thử phải VƯỢT trần byte — nếu không thì bài kiểm này không đo gì"
    );
    let man = format!(
        "  1. {viet}\n  2. Chat about this\nEnter to select · ↑/↓ to navigate · Esc to cancel\n"
    );
    let got = parse_choices(&man);
    assert_eq!(
        got.len(),
        2,
        "nhãn tiếng Việt bị trần BYTE cắt mất: {got:#?}"
    );
}
