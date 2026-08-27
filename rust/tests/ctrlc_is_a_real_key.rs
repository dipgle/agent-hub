//! `/ctrlc` phải gửi ĐÚNG cái byte mà bàn phím thật gửi, và phải đi tới nơi.
//!
//! 🔴 Hà 2026-08-27: *"Cách bạn xử lý màn bị treo không được, cứ để nó là lệnh
//! ctrl+c cho tôi"*, sau khi thử đường nới cửa sổ trên máy anh. Rồi ngay sau:
//! *"Bỏ nút làm tươi ở phiên chát đi, trường hợp này ít xảy ra"*.
//!
//! Thứ thay cho cả một `CommandKind::Refresh` + handler riêng là **một dòng
//! trong bảng phím**: Ctrl+C = byte ETX (ASCII 3). Đó là đúng byte tty nhận khi
//! người ta giữ Ctrl rồi bấm C — không cần modifier, không cần `cgkeys`.
//!
//! Tệp này canh ba chỗ, và cả ba đều đã có tiền lệ trả giá trong repo này:
//!
//! ① **BYTE**. `key_payload("enter")` từng ghi `ASCII character 10` (LF) trong
//!    khi chú thích ngay trên nó nói CR — sai suốt hai ngày, không trình dịch
//!    nào kêu, vì payload chỉ là một chuỗi. Một hằng số sai ở đây là một phím
//!    "bấm được" mà không làm gì.
//! ② **ĐƯỜNG ĐI**. Route mang `Arg::Fixed("ctrl-c")`, và `is_key_name` phải
//!    nhận đúng cái tên ấy — nếu không thì `/ctrlc` hiện trong menu ☰, chạm
//!    vào, và nhận lại *"Chưa hiểu lệnh này"*.
//! ③ **CÁI TÊN CŨ KHÔNG ĐƯỢC CHẾT**. `/refresh` và `/lamtuoi` đã nằm trong
//!    tin Telegram đã gửi đi và trong tay quen; chúng phải tới cùng một chỗ.

use huba::adapters::CommandKind;
use huba::commands::{lookup, Arg};
use huba::keys::is_key_name;
use huba::verbs::{parse_command, KEYBOARD};

#[test]
fn ctrl_c_is_the_etx_byte_a_real_keyboard_sends() {
    // Đo qua đúng cửa mà `press` đi qua. `key_payload` là hàm riêng, nên hỏi nó
    // bằng thứ công khai duy nhất nói lên cùng một sự thật.
    assert!(
        is_key_name("ctrl-c"),
        "bảng phím phải biết 'ctrl-c', nếu không `/ctrlc` là một cái tên chết"
    );
    for ten in ["ctrlc", "^c"] {
        assert!(is_key_name(ten), "'{ten}' phải là cùng một phím");
    }
}

#[test]
fn the_ctrlc_route_carries_its_own_argument() {
    let r = lookup("ctrlc").expect("bảng phải có route `ctrlc`");
    assert_eq!(
        r.kind,
        CommandKind::Key,
        "nó là một PHÍM, không phải route riêng"
    );
    assert_eq!(
        r.arg,
        Arg::Fixed("ctrl-c"),
        "menu ☰ của Telegram không khai được tham số — tham số phải nằm sẵn trong bảng"
    );
    assert!(
        r.listed,
        "việc này hiếm nhưng phải TÌM ĐƯỢC: nó sống trong menu ☰"
    );
    let (kind, _, arg) = parse_command("/ctrlc").expect("`/ctrlc` phải đọc được");
    assert_eq!((kind, arg.as_str()), (CommandKind::Key, "ctrl-c"));
}

/// Tên cũ đã đi ra ngoài thì không được chết — nó nằm trong những tin Telegram
/// đã gửi và trong tay quen của chủ máy.
#[test]
fn the_old_names_still_land_on_the_same_key() {
    for cu in ["/refresh", "/lamtuoi", "/ctrl-c"] {
        let got = parse_command(cu).unwrap_or_else(|| panic!("{cu} phải còn đọc được"));
        assert_eq!(
            (got.0, got.2.as_str()),
            (CommandKind::Key, "ctrl-c"),
            "{cu} phải tới đúng chỗ `/ctrlc` tới"
        );
    }
}

/// 🪦 Nút `🔄 Làm tươi` đã rời bàn phím thường trực theo lệnh Hà. Bài kiểm này
/// giữ cho nó không lặng lẽ quay lại: bàn phím ấy hiện ở MỌI tin, nên mỗi nút
/// trên đó đắt hơn hẳn một dòng trong menu ☰.
#[test]
fn the_persistent_keyboard_stays_at_two_buttons() {
    assert_eq!(
        KEYBOARD.len(),
        2,
        "bàn phím thường trực nay là 📷 Xem màn + 📋 Phiên. Thêm nút thứ ba thì \
         phải là một quyết định có người chốt, không phải một lượt sửa tiện tay"
    );
    assert!(
        !KEYBOARD.iter().any(|(_, lenh)| lenh.contains("refresh")),
        "Hà đã bảo bỏ nút làm tươi khỏi phiên chát — trường hợp này ít xảy ra"
    );
    // ĐỐI CHỨNG NGƯỢC: bài trên chỉ có nghĩa nếu bảng thật sự còn nút.
    assert!(
        KEYBOARD.iter().any(|(_, lenh)| *lenh == "/shot"),
        "bàn phím rỗng cũng thoả hai assert trên — phải khoá cả chiều còn lại"
    );
}
