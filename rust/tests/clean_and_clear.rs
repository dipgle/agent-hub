//! `/clean` dọn hàng chờ **rồi ô nhập**; `/clear` chỉ ô nhập.
//!
//! 🔴 Hà 2026-08-26: *"Sửa lại lệnh clean và thêm lệnh clear để cùng có tác dụng
//! xóa text ở ô chat"*.
//!
//! VÌ SAO `/clean` ĐỂ LẠI CHỮ, và nó không phải chuyện quên: `keys::clear_queue`
//! bấm **↑** để lôi từng tin trong hàng chờ **ngược vào ô nhập** rồi xoá — nên
//! tin cuối cùng được lôi ra **nằm lại đúng trong ô**. Chính lệnh dọn là thứ đổ
//! chữ vào cái ô mà chủ máy thấy vẫn còn.
//!
//! Vì thế thứ tự **hàng chờ trước, ô nhập sau** là bắt buộc: làm ngược lại là
//! xoá một cái ô sắp được đổ đầy trở lại — và đó chính là bản cũ.

use huba::adapters::CommandKind;
use huba::verbs::parse_command;

/// `parse_command` trả `(kind, id, phần còn lại)` — gói lại cho bài kiểm đọc được.
fn kind_of(l: &str) -> CommandKind {
    parse_command(l)
        .unwrap_or_else(|| panic!("không phân giải được: {l}"))
        .0
}
fn arg_of(l: &str) -> String {
    parse_command(l)
        .unwrap_or_else(|| panic!("không phân giải được: {l}"))
        .2
}

/// Hai lệnh phải phân giải ra HAI việc khác nhau.
#[test]
fn clean_and_clear_are_two_different_commands() {
    let a = kind_of("/clean");
    let b = kind_of("/clear");
    assert_eq!(a, CommandKind::Clean);
    assert_eq!(b, CommandKind::Clear);
    assert_ne!(
        a, b,
        "gộp làm một là đánh mất khác biệt về HẬU QUẢ: /clear chỉ bỏ chữ chưa \
         gửi, /clean bỏ cả tin đã xếp hàng chờ chạy"
    );
}

/// Cả hai nhận `[id]` để chỉ đúng phiên — không thì chúng chỉ dùng được cho
/// phiên đang theo, và chủ máy phải đổi con trỏ trước mỗi lần dọn.
#[test]
fn both_take_an_optional_session_id() {
    for l in ["/clean 7bdb4f41", "/clear 7bdb4f41"] {
        assert_eq!(arg_of(l), "7bdb4f41", "{l}");
    }
    // …và vẫn chạy được khi KHÔNG có id: lúc ấy là phiên đang theo.
    assert_eq!(arg_of("/clean"), "");
    assert_eq!(arg_of("/clear"), "");
}

/// Tên tiếng Việt phải gõ được — Hà gõ từ điện thoại, và bàn phím tiếng Việt
/// không tiện chuyển qua lại.
#[test]
fn the_vietnamese_aliases_resolve() {
    assert_eq!(
        kind_of("/don"),
        CommandKind::Clean,
        "alias cũ của clean phải còn"
    );
    assert_eq!(kind_of("/xoacho"), CommandKind::Clean);
    assert_eq!(kind_of("/xoao"), CommandKind::Clear);
}

/// 🔴 Cả hai phải nằm trong menu ☰ — một lệnh không được khai là một lệnh không
/// ai biết. Đúng bài học `/win`: đo ra "0 lượt dùng từ 26/07" rồi suýt bị gỡ,
/// trong khi con số ấy đo SỰ VÔ HÌNH chứ không đo sự vô dụng.
#[test]
fn both_are_listed_in_the_menu() {
    let khai = huba::commands::for_telegram();
    for ten in ["clean", "clear"] {
        assert!(
            khai.iter().any(|(n, _)| *n == ten),
            "`/{ten}` không được khai vào menu ☰ — Hà sẽ không biết nó tồn tại: {khai:?}"
        );
    }
}
