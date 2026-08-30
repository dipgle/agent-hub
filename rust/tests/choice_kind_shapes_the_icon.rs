//! Chọn MỘT thì radio, chọn NHIỀU thì checkbox — và hàng phiên nói cùng một thứ.
//!
//! 🔴 Hà 2026-08-30: *"Nếu option chỉ chọn 1 thì để nút radio và chọn nhiều mới
//! để checkbox, icon trạng thái phiên thêm icon checkbox nếu đang có option
//! chờ"*.
//!
//! Hai câu, một bộ ký hiệu. Trước đó `pipeline` gõ cứng `☑` cho mọi lựa chọn,
//! nên một hộp CHỌN MỘT đọc lên y hệt một hộp CHỌN NHIỀU — mà hai thứ ấy đòi hai
//! thao tác khác hẳn: một bên bấm là gửi, bên kia bấm là bật/tắt rồi còn phải
//! Submit. Đó đúng là con bug 2026-08-13 (*"option này chọn nhiều chứ không phải
//! chọn 1"*), lần này soi từ phía ngược lại.

use huba::sessions::{state_of, Asking, ChoiceKind, LiveSession, ST_ASK};

/// Ba giá trị, không phải hai — và `Unknown` vẽ CHECKBOX.
///
/// 🔴 Đây là bài quan trọng nhất tệp này, vì nó khoá một lựa chọn có HƯỚNG. Hai
/// cái sai không ngang giá: radio trên một hộp chọn-nhiều nói *"bấm một cái là
/// xong"*, chủ máy bấm rồi ngồi chờ một việc không xảy ra; checkbox trên một hộp
/// chọn-một thì bấm một cái nó vẫn gửi. Nên chỗ không đo được phải nghiêng về
/// checkbox.
#[test]
fn khong_do_duoc_thi_ve_checkbox_chu_khong_ve_radio() {
    assert_eq!(
        ChoiceKind::default(),
        ChoiceKind::Unknown,
        "mặc định là ẩn số"
    );
    assert_eq!(
        ChoiceKind::Unknown.glyph(),
        "☑",
        "không đo được mà vẽ radio là hứa một điều chưa đo"
    );
    assert_eq!(ChoiceKind::Multi.glyph(), "☑");
    assert_eq!(ChoiceKind::Single.glyph(), "◉");
    assert_ne!(
        ChoiceKind::Single.glyph(),
        ChoiceKind::Multi.glyph(),
        "hai loại hộp mà cùng một ký hiệu thì cả bản vá này không đo gì"
    );
}

/// Chỉ NHẬT KÝ mới được phép nói "chọn một".
///
/// Màn hình nói được `Multi` (có dòng `Submit`), nhưng VẮNG dòng ấy không chứng
/// minh được gì — nó có thể đã trôi khỏi khung nhìn. "Vắng bằng chứng" đọc thành
/// "bằng chứng vắng" là đúng cái hướng sai đắt hơn ở bài trên.
#[test]
fn man_hinh_khong_bao_gio_duoc_ket_luan_la_chon_mot() {
    assert_eq!(ChoiceKind::from_journal(false), ChoiceKind::Single);
    assert_eq!(ChoiceKind::from_journal(true), ChoiceKind::Multi);

    assert_eq!(ChoiceKind::from_screen(true), ChoiceKind::Multi);
    assert_eq!(
        ChoiceKind::from_screen(false),
        ChoiceKind::Unknown,
        "không thấy `Submit` ⟹ chưa biết, KHÔNG phải chọn một"
    );
}

fn phien_hoi(options: &[&str], multi: bool) -> LiveSession {
    LiveSession {
        host: "terminal".to_string(),
        asking: Some(Asking {
            header: "Chọn cách vá".to_string(),
            question: "Vá ACL thế nào?".to_string(),
            options: options.iter().map(|s| s.to_string()).collect(),
            multi,
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Hàng phiên mang ký hiệu khi CÓ option chờ, và KHÔNG mang khi không có.
///
/// Chấm cả hai chiều: một hàm luôn gắn ký hiệu cũng "qua" nếu chỉ chấm chiều có.
#[test]
fn hang_phien_mang_ky_hieu_dung_khi_co_option_cho() {
    let co = phien_hoi(&["Vá ngay", "Để sau"], false);
    let hang = huba::pipeline::session_list_text(std::slice::from_ref(&co), "", 0);
    assert!(
        hang.contains('◉'),
        "hộp CHỌN MỘT phải ra radio trên hàng phiên:\n{hang}"
    );

    let nhieu = phien_hoi(&["ACL", "Đăng nhập"], true);
    let hang = huba::pipeline::session_list_text(std::slice::from_ref(&nhieu), "", 0);
    assert!(
        hang.contains('☑'),
        "hộp CHỌN NHIỀU phải ra checkbox trên hàng phiên:\n{hang}"
    );

    // ĐỐI CHỨNG NGƯỢC ①: hỏi mà KHÔNG có lựa chọn nào (câu hỏi chữ tự do) thì
    // không được gắn — ký hiệu ấy hứa "bấm được ngay từ đây".
    let chu = phien_hoi(&[], false);
    let hang = huba::pipeline::session_list_text(std::slice::from_ref(&chu), "", 0);
    assert!(
        !hang.contains('◉') && !hang.contains('☑'),
        "không có lựa chọn nào mà vẫn mời bấm:\n{hang}"
    );

    // ĐỐI CHỨNG NGƯỢC ②: phiên không hỏi gì thì hàng phải sạch.
    let ranh = LiveSession {
        host: "terminal".to_string(),
        ..Default::default()
    };
    let hang = huba::pipeline::session_list_text(std::slice::from_ref(&ranh), "", 0);
    assert!(
        !hang.contains('◉') && !hang.contains('☑'),
        "phiên không hỏi gì mà hàng vẫn có ký hiệu chọn:\n{hang}"
    );
}

/// Ký hiệu này KHÔNG thay `❓` — nó đứng cạnh.
///
/// `❓` nói *"phiên dừng lại hỏi"*, đúng cả với một câu hỏi chữ tự do; ký hiệu
/// chọn nói *"có lựa chọn bấm được ngay từ đây"*. Hai câu dẫn tới hai thao tác,
/// nên nuốt cái này vào cái kia là mất một dữ kiện.
#[test]
fn ky_hieu_chon_dung_canh_dau_hoi_chu_khong_thay_no() {
    let s = phien_hoi(&["A", "B"], true);
    assert_eq!(state_of(&s).0, ST_ASK, "vẫn phải là ❓");
    let hang = huba::pipeline::session_list_text(std::slice::from_ref(&s), "", 0);
    assert!(hang.contains(ST_ASK), "mất ❓:\n{hang}");
    assert!(hang.contains('☑'), "mất ký hiệu chọn:\n{hang}");
}
