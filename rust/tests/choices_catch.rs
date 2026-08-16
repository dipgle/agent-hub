//! Hộp chọn trên màn `[AI/mailler]` 2026-08-16 — hub có đọc ra lựa chọn không?
//!
//! 🔴 Hà: *"Màn có option nhưng không có bảng chọn"*. Chữ dưới đây chép từ chính
//! ảnh chụp ấy, giữ nguyên thụt lề và dấu `❯` ở dòng đang trỏ.

#[test]
fn the_five_options_on_that_screen_are_read() {
    let man = "□ Phạm vi\n\
               \"Sửa deploy.sh thành update.sh\" — anh muốn cái nào?\n\
               ❯ 1. Đổi tên file ở gốc repo\n\
               \x20     Rename deploy.sh → update.sh ở gốc + sửa mọi tham chiếu (README, docs).\n\
               \x20  2. Chỉ sửa tham chiếu trong tài liệu\n\
               \x20     Các chỗ trong docs/report đang ghi deploy.sh đổi thành update.sh.\n\
               \x20  3. Chuyển phần ghi env sang update.sh\n\
               \x20     Mang khối ghi /opt/mailler/.env sang update.sh.\n\
               \x20  4. Type something.\n\
               \x20  5. Chat about this\n\
               Enter to select · ↑/↓ to navigate · Esc to cancel";

    let got = hub::keys::parse_choices(man);
    println!("đọc ra {} lựa chọn:", got.len());
    for (n, s) in &got {
        println!("  {n}. {s}");
    }
    assert_eq!(got.len(), 5, "màn có 5 lựa chọn: {got:?}");
    assert_eq!(got[0].0, 1);
    assert!(got[0].1.contains("Đổi tên file"));
    assert_eq!(got[4].0, 5);
}
