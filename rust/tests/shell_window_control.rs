//! Cửa sổ Terminal trần: gõ MỘT lệnh là MỘT Enter, và mở `nano` không làm mất
//! quyền kiểm soát nó.
//!
//! 🔴 Hà 2026-08-25, một ảnh và một câu, hai lỗi khác nhau cùng một cửa sổ:
//!
//! ① *"sau một vài tin nhắn với terminal thì mất quyền kiểm soát nó luôn"* —
//!    gõ `cd projects` được, `/shot` được, rồi gõ `nano .env`; lượt `/shot` sau
//!    đó trả `⚠ không thấy phiên 'win-ttys001' trong danh sách`.
//! ② *"gõ 1 lệnh mà có 4 lần enter"* — ảnh cho thấy **năm dấu nhắc trống** sau
//!    một lệnh `cd projects`.

use huba::keys::still_in_box;

/// 🔴 Lỗi ②: màn shell trần KHÔNG có ô nhập, nên không có gì "còn nằm trong ô".
///
/// `box_region` rơi về **bốn dòng cuối** khi không thấy khung — đường lùi ấy
/// đúng cho màn `claude` mà phép dò khung trượt, và sai hẳn ở đây: `do script`
/// đã gửi lệnh rồi, còn dòng lệnh thì **nằm lại trên màn vĩnh viễn** dưới dấu
/// nhắc. Phép kiểm trả "còn trong ô" ⟹ `type_and_send` bắn thêm hai cú Enter.
#[test]
fn a_bare_shell_screen_never_looks_like_a_full_input_box() {
    let man = "Last login: Tue Aug 25 11:56:48 on ttys002\n\
               hanguyen@MacBookPro ~ % cd ~/\n\
               hanguyen@MacBookPro ~ % cd projects\n\
               hanguyen@MacBookPro projects % \n";
    assert!(
        !still_in_box(man, "cd projects"),
        "dòng lệnh ĐÃ CHẠY vẫn bị đọc thành 'còn trong ô nhập' ⟹ Enter thừa"
    );
}

/// …kể cả khi lệnh là dòng cuối cùng nhìn thấy được.
#[test]
fn the_echoed_command_on_the_last_line_is_not_pending_text() {
    let man = "hanguyen@MacBookPro ~ % nano .env.example\n";
    assert!(!still_in_box(man, "nano .env.example"), "{man}");
}

/// 🔴 Hàng rào ngược: màn `claude` THẬT vẫn phải nhận ra chữ còn trong ô.
///
/// Nếu không thì bản vá này lặng lẽ gỡ mất cú Enter phụ — thứ đã ra đời vì
/// `do script` đẩy chữ + xuống dòng trong CÙNG một lượt ghi, và TUI đọc lượt ấy
/// như một cú DÁN nên dấu xuống dòng rơi vào nội dung thay vì kết thúc nó
/// (`CLAUDE.md` §13).
///
/// Ô nhập của `claude` nay nằm GIỮA HAI vạch `─` suốt bề ngang — không còn
/// `╭ ╰ │` từ 2026-08-20.
#[test]
fn a_real_claude_input_box_still_reports_text_sitting_in_it() {
    let rule = "─".repeat(60);
    let man = format!(
        "⏺ Tôi đã đọc xong tệp ấy.\n\
         {rule}\n\
         ❯ chạy hộ tôi cargo test --offline\n\
         {rule}\n\
         ⏵⏵ auto mode on\n"
    );
    assert!(
        still_in_box(&man, "chạy hộ tôi cargo test --offline"),
        "mất phép nhận ra chữ còn trong ô nhập của claude:\n{man}"
    );
}

/// Chữ quá ngắn thì vẫn không tính — luật cũ, giữ nguyên.
#[test]
fn a_very_short_string_is_never_treated_as_pending() {
    let rule = "─".repeat(60);
    let man = format!("{rule}\n❯ ok\n{rule}\n");
    assert!(!still_in_box(&man, "ok"));
}
