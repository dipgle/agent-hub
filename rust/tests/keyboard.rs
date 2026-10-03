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

/// 🔴 Hà 03/10: *"Tại sao thi thoảng lại mất mấy nút menu ở dưới cùng tele"*.
/// App Telegram có lúc bỏ bàn phím; huba chỉ gắn lại ở lời chào sau mỗi lần cài
/// và tin báo phiên tắt không nút — có khi hàng giờ. Nay MỌI tin trơn
/// (`send_text`) mang nó, nên mất ở đâu thì câu trả lời kế tiếp gắn lại.
#[test]
fn moi_tin_tron_deu_gan_lai_ban_phim() {
    let b = huba::telegram::Inbox::plain_message_body("1", "Chưa hiểu lệnh này");
    // Nền: thân tin thật sự có chữ và đúng buồng — một thân rỗng không được đọc
    // ra là "đã gắn bàn phím".
    assert_eq!(b["chat_id"], "1");
    assert_eq!(b["text"], "Chưa hiểu lệnh này");
    let hang = b["reply_markup"]["keyboard"][0]
        .as_array()
        .expect("tin trơn phải mang reply_markup.keyboard");
    let nhan: Vec<&str> = hang.iter().filter_map(|n| n["text"].as_str()).collect();
    let mong: Vec<&str> = KEYBOARD.iter().map(|(n, _)| *n).collect();
    assert_eq!(nhan, mong, "bàn phím gắn lại phải ĐÚNG bảng KEYBOARD");
    assert_eq!(b["reply_markup"]["is_persistent"], true);
    assert!(b["reply_markup"].get("inline_keyboard").is_none());
}

/// Chỗ nối: `send_text` phải dựng thân bằng ĐÚNG hàm trên — tự dựng một thân
/// khác là bàn phím lại rơi khỏi tin trơn mà bài trên vẫn xanh. `None` = thiếu
/// mỏ neo ⟹ ĐỎ.
fn send_text_dung_than_chung(src: &str) -> Option<bool> {
    let a = src.find("pub fn send_text(&self, text: &str)")?;
    let rest = &src[a..];
    let het = rest[1..]
        .find("\n    pub fn ")
        .map(|i| i + 1)
        .unwrap_or(rest.len());
    Some(rest[..het].contains("Self::plain_message_body(&self.chat_id, text)"))
}

#[test]
fn send_text_di_qua_than_co_ban_phim() {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/src/telegram.rs");
    let src = std::fs::read_to_string(p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: {p}: {e}"));
    assert_eq!(send_text_dung_than_chung(&src), Some(true));
    // Đối chứng ngược: hình dạng trước 03/10 (thân tự dựng, không bàn phím).
    let cu = "    pub fn send_text(&self, text: &str) -> Result<(), String> {\n        let v = self.post_retry(\"sendMessage\", &json!({ \"chat_id\": self.chat_id, \"text\": t }))?;\n    }\n    pub fn khac() {}\n";
    assert_eq!(send_text_dung_than_chung(cu), Some(false));
}
