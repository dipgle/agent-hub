//! Ảnh ra đen: NÓI ĐÚNG vì sao, đừng đoán.
//!
//! 🔴 Hà 2026-08-18: *"Chụp ảnh ra den xì"*. Bản đầu của tôi kết luận ngay là
//! thiếu quyền Screen Recording — rồi Hà hỏi lại: *"Máy đang ở màn hình chờ đăng
//! nhập không chụp được ảnh?"*, và đo ra đúng thế
//! (`"CGSSessionScreenIsLocked"=Yes`). Hai nguyên nhân cho cùng một tấm ảnh đen,
//! nên câu trả lời phải PHÂN BIỆT được, không thì huba bắt chủ máy đi kiểm hộ.

use huba::keys::{blank_frame_reason, lock_verdict};

/// Nguyên văn `ioreg -n Root -d1` trên máy này lúc màn ĐANG khoá (18/08).
const LOCKED: &str = r#"      "IOConsoleUsers" = ({"kCGSSessionOnConsoleKey"=Yes,"kSCSecuritySessionID"=100023,"kCGSSessionSystemSafeBoot"=No,"kCGSessionLoginDoneKey"=Yes,"kCGSSessionIDKey"=257,"kCGSSessionUserNameKey"="hanguyen","CGSSessionScreenLockedTime"=1786984979,"CGSSessionScreenIsLocked"=Yes,"kCGSSessionUserIDKey"=501})"#;

/// Cùng dòng ấy khi màn KHÔNG khoá: khoá `CGSSessionScreenIsLocked` vắng mặt.
const UNLOCKED: &str = r#"      "IOConsoleUsers" = ({"kCGSSessionOnConsoleKey"=Yes,"kCGSSessionIDKey"=257,"kCGSSessionUserNameKey"="hanguyen","kCGSSessionUserIDKey"=501})"#;

#[test]
fn a_locked_screen_is_read_from_ioreg_not_guessed() {
    assert_eq!(lock_verdict(LOCKED), Some(true));
    assert_eq!(lock_verdict(UNLOCKED), Some(false));
    // Không có `IOConsoleUsers` ⟹ KHÔNG đo được. Phải khác hẳn "đo rồi, không
    // khoá" — nếu không thì một ngày `ioreg` đổi định dạng là huba lại khẳng
    // định chắc nịch một điều nó không biết.
    assert_eq!(lock_verdict("ioreg: command not found"), None);
}

/// Ba trạng thái ⟹ ba việc khác nhau cho chủ máy ⟹ ba câu khác nhau.
#[test]
fn each_reason_tells_the_owner_a_different_thing_to_do() {
    let locked = blank_frame_reason(Some(true));
    assert!(locked.contains("màn hình đăng nhập"), "{locked}");
    assert!(
        locked.contains("/shot"),
        "phải chỉ đường còn đi được: {locked}"
    );
    // 🔴 Câu của ca KHOÁ MÀN không được đổ cho quyền — đó đúng là cái sai Hà
    // vừa bắt được, và bài kiểm này là chỗ giữ cho nó không quay lại.
    assert!(
        !locked.contains("Screen Recording"),
        "khoá màn KHÔNG phải chuyện quyền: {locked}"
    );

    let no_grant = blank_frame_reason(Some(false));
    assert!(no_grant.contains("Screen Recording"), "{no_grant}");
    assert!(
        no_grant.contains("bin/hubd"),
        "phải nói cấp cho binary NÀO: {no_grant}"
    );

    let unknown = blank_frame_reason(None);
    // Không đo được thì nói cả hai, theo thứ tự kiểm — không chọn bừa một cái.
    assert!(unknown.contains("không đo được"), "{unknown}");
    assert!(unknown.contains("đăng nhập") && unknown.contains("Screen Recording"));
}
