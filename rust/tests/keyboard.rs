//! Bàn phím thường trực: mỗi nhãn phải dịch được về ĐÚNG route nó hứa.
//!
//! 🔴 Hà 2026-08-26: *"sao pin msg không bấm được nút trực tiếp ở trên à, nó đang
//! cuộn tới tin đó không hợp lý lắm"*. Băng gim ở đỉnh buồng chat không nhận nút
//! được (giới hạn của Telegram), nên đường một-chạm-không-cuộn là
//! `ReplyKeyboardMarkup` dưới ô nhập.
//!
//! Bài kiểm này khoá đúng cái vòng tròn dễ gãy nhất của thiết kế ấy: nút hiện ra
//! là do `telegram::persistent_keyboard` dựng, còn cú bấm về thì do
//! `verbs::parse_command` đọc — hai chỗ, một bảng. Lệch một ký tự là nút vẫn hiện
//! nhưng bấm vào huba đáp *"Chưa hiểu lệnh này"*, đúng con bug `/key enter` đã
//! trả giá sáng cùng ngày: ở đó huba mời chạm một thứ chính nó không hiểu.

use huba::verbs::{parse_command, KEYBOARD};

#[test]
fn every_keyboard_label_parses_to_the_command_it_promises() {
    assert!(
        !KEYBOARD.is_empty(),
        "bảng rỗng thì bài kiểm này xanh vô nghĩa"
    );
    for (nhan, lenh) in KEYBOARD {
        let qua_nhan = parse_command(nhan);
        assert!(
            qua_nhan.is_some(),
            "nhãn {nhan:?} hiện ra trên bàn phím mà `parse_command` không hiểu — \
             bấm vào sẽ nhận 'Chưa hiểu lệnh này'"
        );
        assert_eq!(
            qua_nhan,
            parse_command(lenh),
            "nhãn {nhan:?} phải ra ĐÚNG route mà {lenh:?} ra — nếu không thì cái nút \
             làm một việc khác với điều nó hứa"
        );
    }
}

/// ĐỐI CHỨNG NGƯỢC: phép dịch chỉ được ăn ĐÚNG nhãn trong bảng.
///
/// Thiếu nửa này thì một phép dịch "cứ có chữ Xem màn là thành /shot" cũng đạt —
/// và lúc ấy một câu chủ máy gõ tay lỡ trùng chữ sẽ đi chạy một lệnh.
#[test]
fn only_the_exact_labels_are_translated() {
    for khong_phai in [
        "📷 Xem màn hình",
        "Xem màn",
        "📷",
        "xem màn",
        "📷  Xem màn",
        "cho tôi 📷 Xem màn đi",
    ] {
        assert!(
            parse_command(khong_phai).is_none(),
            "{khong_phai:?} KHÔNG phải một nhãn trên bàn phím — nó phải rơi ra ngoài, \
             không được lặng lẽ thành một lệnh"
        );
    }
}

/// Nhãn vẫn phải đi qua được sau khi Telegram/người dùng thêm khoảng trắng thừa —
/// `parse_command` trim trước khi tra bảng.
#[test]
fn surrounding_whitespace_does_not_break_a_label() {
    let (nhan, lenh) = KEYBOARD[0];
    assert_eq!(parse_command(&format!("  {nhan}  ")), parse_command(lenh));
}
