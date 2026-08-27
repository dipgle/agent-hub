//! Bảng `AskUserQuestion` thật: bộ đọc màn phải thấy ĐỦ lựa chọn, và mọi nhãn
//! phải neo được vào chữ.
//!
//! 🔴 Tệp này ra đời vì một **tiền đề sai của chính tôi**, 2026-08-27. Hà báo
//! *"Có option nhưng ko chọn được"*; tôi đọc `CLAUDE.md` — nơi ghi rằng
//! `keys::parse_choices` **mù** với bảng `AskUserQuestion` (đo 14/08: *0 mục
//! trên chính màn ấy*) — rồi dựng cả một đường bù lấy nhãn từ nhật ký.
//!
//! Bài kiểm đầu tiên tôi viết cho đường bù ấy **ĐỎ ngay**, và nó bác tiền đề:
//! trên màn thật `fixtures/man-bang-hoi-dwork.txt` (lượt `/shot` 10:23:46Z của
//! `[dwork] 574e5be2`), `parse_choices` đọc ra **5/5 mục**. Sự mù ấy đã được vá
//! ngày 25/08 cùng lượt `chooser_footer.rs`; câu trong `CLAUDE.md` là **di sản
//! chưa ai xoá**. Đường bù bị gỡ, tệp này ở lại làm khoá.
//!
//! Bài học đắt hơn bản vá: một mệnh đề trong tài liệu, kể cả tài liệu của chính
//! repo này, vẫn phải verify bằng phép đo ở phiên NÀY.

use huba::keys::parse_choices;

const MAN: &str = include_str!("fixtures/man-bang-hoi-dwork.txt");

/// Năm nhãn, lấy nguyên văn từ màn — chúng vừa là kỳ vọng của bộ đọc, vừa là
/// chỗ neo mà `☑` bám vào.
const NHAN: &[&str] = &[
    "Tách ship trước, gấp (Recommended)",
    "Gộp một đợt sáu bề mặt",
    "Chỉ đo mức thiệt hại trước",
    "Type something.",
    "Chat about this",
];

#[test]
fn the_screen_reader_sees_every_option_of_a_real_question_table() {
    let thay = parse_choices(MAN);
    assert_eq!(
        thay.len(),
        NHAN.len(),
        "màn này có {} lựa chọn (dòng 5·8·10·12·14, mỗi cái có một dòng MÔ TẢ chen \
         giữa) mà bộ đọc ra {} — nếu con số này tụt xuống 0 thì luật 'liền dòng' đã \
         quay lại, và triệu chứng 'có option mà không chọn được' quay lại theo",
        NHAN.len(),
        thay.len()
    );
    assert!(
        thay.first().is_some_and(|(n, _)| *n == 1),
        "phải bắt đầu từ mục 1 — số đầu khác 1 nghĩa là hộp bị mép màn cắt"
    );
}

/// ☑ neo BẰNG NHÃN, nên nhãn phải tìm thấy được trong chính chữ gửi đi. Không
/// khớp thì cái nút biến mất trong im lặng — không lỗi, không log, không nút.
#[test]
fn every_label_can_be_anchored_in_the_text_that_gets_sent() {
    let thay = parse_choices(MAN);
    for (_, nhan) in &thay {
        assert!(
            MAN.contains(nhan.as_str()),
            "nhãn {nhan:?} bộ đọc trả về KHÔNG có mặt nguyên văn trên màn — ☑ sẽ không \
             có chỗ đặt. Nhãn dài bị màn cắt bằng `…` là ca đã biết: lúc ấy phải neo \
             bằng phần ĐẦU, đừng bỏ im"
        );
    }
}

/// ĐỐI CHỨNG NGƯỢC: hai bài trên chỉ có nghĩa nếu bộ đọc BIẾT từ chối. Một hàm
/// nhận mọi thứ có đánh số sẽ gắn ☑ vào giữa một đoạn văn — đúng cái mà luật
/// "liền dòng" sinh ra để chặn.
#[test]
fn numbered_prose_is_still_not_a_chooser() {
    let van_xuoi = "Ba việc phải làm:\n\n1. Đọc lại hợp đồng gửi A-CHUNG\n\n\
                    2. Dựng cổng cho luật nối ca\n\n3. Nghiệm thu trên UI thật\n\n\
                    Xong ba việc ấy thì đóng sổ.";
    assert!(
        parse_choices(van_xuoi).is_empty(),
        "một đoạn văn có đánh số KHÔNG phải hộp chọn — gắn ☑ vào đó là mời chủ máy \
         bấm một thứ không tồn tại"
    );
}
