//! Cổng chặn mũi tên phán trên BAO NHIÊU chữ — và vì sao nó đáng trả 1,2 giây.
//!
//! 🔴 Điều kiện để gửi một mũi tên là **biết chắc KHÔNG có hộp chọn**, chứ không
//! phải *"không thấy hộp chọn nào"* (`keys::arrow_verdict`). `do script` luôn kèm
//! một CR, nên trên hộp chọn một mũi tên vừa DI vừa CHỐT — chốt nhầm hộ chủ máy
//! là thứ không lùi lại được.
//!
//! Tệp này đo đúng khoảng cách giữa hai câu ấy: cùng MỘT màn thật, đọc rộng thì
//! cổng ĐÓNG, đọc hẹp thì cổng MỞ. Đó là lý do hai cổng nguy hiểm nhất đổi sang
//! `keys::look_sure` (nới cửa sổ hết cỡ trước khi đọc) trong khi cả làn Urgent
//! vẫn đọc hẹp — xem chú thích ở `look_sure` để biết vì sao không gộp theo
//! `exec::Lane`.

use huba::keys::{arrow_verdict, look_from_screen, parse_choices, Arrow, Look};

/// Màn THẬT, chụp 2026-08-19 trên phiên `[tcc/amm]`.
///
/// Nó đã là hình dạng "hộp dài hơn màn": danh sách bắt đầu từ `2.` vì lựa chọn 1
/// cuộn khỏi mép trên, và thứ giữ cho `parse_choices` còn nhận ra đây là một cái
/// hộp là DÒNG CHÂN ở đáy.
const MAN: &str = include_str!("fixtures/shot-amm-chooser-2026-08-19.txt");

#[test]
fn the_wide_read_sees_the_box_and_the_gate_shuts() {
    let choices = parse_choices(MAN);
    assert!(
        !choices.is_empty(),
        "màn này CÓ hộp chọn — đọc ra rỗng là cổng mất căn cứ"
    );
    assert_eq!(
        arrow_verdict(&look_from_screen(MAN, 24)),
        Arrow::RefuseDialog,
        "thấy hộp thì phải TỪ CHỐI gửi mũi tên"
    );
}

/// 🔴 ĐÂY LÀ CÁI GIÁ CỦA BẢN ĐỌC HẸP, đo được chứ không phải suy đoán.
///
/// Mô hình khung nhìn hẹp: hộp cao hơn màn nên MỌI dòng đánh số đã cuộn khỏi mép
/// trên; chỉ dòng chân sống sót, vì nó nằm ở ĐÁY hộp. Cắt từ chính màn thật ở
/// trên, không bịa một chuỗi nào.
///
/// Kết quả: `parse_choices` trả RỖNG ⟹ `arrow_verdict` = **`Send`**. Tức đúng lúc
/// huba mù nhất là lúc nó dám tay nhất — và cái nó sắp gửi vào là một hộp chọn
/// đang mở.
#[test]
fn the_narrow_read_loses_the_box_and_the_gate_opens() {
    let duoi: String = MAN
        .lines()
        .skip_while(|l| !l.contains("Chat about this"))
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n");

    // Phép đo phải trỏ đúng chỗ: mô hình chỉ đúng khi nó THẬT SỰ mất hết dòng số
    // mà GIỮ được dòng chân. Kiểm cả hai vế trước khi phán.
    assert!(
        !duoi.contains("Chat about this"),
        "mô hình hỏng: vẫn còn một dòng đánh số"
    );
    assert!(
        duoi.to_lowercase().contains("to select"),
        "mô hình hỏng: mất luôn dòng chân, thành một màn khác hẳn"
    );

    assert!(
        parse_choices(&duoi).is_empty(),
        "không còn dòng số nào thì không đếm ra lựa chọn nào — đây là tiền đề của ca dưới"
    );
    assert_eq!(
        arrow_verdict(&look_from_screen(&duoi, 24)),
        Arrow::Send,
        "ĐÂY là chỗ hỏng: cùng một cái màn, đọc hẹp thì cổng MỞ và mũi tên bay vào \
         một hộp chọn đang mở. Bài kiểm này KHÔNG mô tả hành vi mong muốn — nó ghim \
         cái giá phải trả khi đọc hẹp, để lượt sau ai định bỏ `look_sure` thì thấy."
    );
}

/// ĐỐI CHỨNG: mù thì phải TỪ CHỐI, không được đọc thành "không có hộp".
///
/// Đây là con bug gốc mà `Look` ba-kết-cục sinh ra để chặn (`screen_of` cũ gộp cả
/// ba vào `None`). Giữ nó ở đây vì `look_sure` thêm một đường rơi mới — nới hụt
/// thì về bản hẹp — và không đường nào trong số đó được phép biến thành `Send`.
#[test]
fn blind_never_becomes_permission_to_send() {
    let mu = Look::Blind {
        why: "Terminal không trả lời".into(),
    };
    assert!(matches!(arrow_verdict(&mu), Arrow::RefuseBlind(_)));
}

/// Màn KHÔNG có hộp chọn thì mới được gửi — nếu không thì cổng này chỉ là một
/// cánh cửa đóng chặt, và `/key` mất hẳn phím mũi tên.
#[test]
fn a_screen_with_no_box_still_lets_the_arrow_through() {
    let trong = "$ ls\nCargo.toml  src  tests\n$ ";
    assert!(parse_choices(trong).is_empty());
    assert_eq!(arrow_verdict(&look_from_screen(trong, 24)), Arrow::Send);
}
