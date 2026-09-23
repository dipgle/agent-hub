//! Hộp HỎI QUYỀN của `claude` (sửa tệp ngoài thư mục làm việc): phải đọc ra đủ
//! lựa chọn để dựng nút.
//!
//! 🔴 Hà 2026-09-23 20:42, ảnh tin huba gửi: *"Tin có option nhưng không bấm
//! được"*. Tin mang nguyên văn `❯ 1. Yes · 2. Yes, and always allow access to
//! /Users/…/memory, /Users/…/memory for this session (shift+tab) · 3. No · Esc to
//! cancel · Tab to amend` — mà không có nút nào.
//!
//! Màn thật (`fixtures/man-hop-quyen-lua-chon-gap-dong-2026-09-23.txt`, cắt từ
//! cửa sổ 7949, chỉ giữ phần hộp): `parse_choices` đọc ra **0** lựa chọn (đo bằng
//! `tests/screen_probe.rs` trên bản dựng TRƯỚC lượt vá). Ba chỗ cùng làm nó rơi:
//!
//! 1. lựa chọn 2 dài hơn 120 ký tự ⟹ bị BỎ (trần chống đọc nhầm đoạn văn) ⟹ khối
//!    còn `1, 3`, đứt số ⟹ cả khối bị loại;
//! 2. dòng chân `Esc to cancel · Tab to amend` không được nhận là dòng chân hộp
//!    chọn (bộ nhận đòi `to select`/`to confirm`);
//! 3. không có dòng chân thì luật "liền dòng" loại khối, vì dòng gấp
//!    `(shift+tab)` nằm giữa lựa chọn 2 và 3.

use huba::keys::{has_chooser_footer, parse_choices};

const MAN: &str = include_str!("fixtures/man-hop-quyen-lua-chon-gap-dong-2026-09-23.txt");

#[test]
fn hop_hoi_quyen_doc_du_ba_lua_chon() {
    assert!(
        MAN.contains("Tab to amend") && MAN.contains("2. Yes, and always allow access to"),
        "tệp mẫu phải còn nguyên dòng chân và lựa chọn dài — không thì bài này đo nhầm thứ"
    );
    assert!(
        has_chooser_footer(MAN),
        "`Esc to cancel · Tab to amend` là dòng chân của hộp hỏi quyền"
    );
    let c = parse_choices(MAN);
    let so: Vec<usize> = c.iter().map(|(n, _)| *n).collect();
    assert_eq!(so, vec![1, 2, 3], "phải đủ 3 lựa chọn: {c:?}");
    assert_eq!(c[0].1, "Yes");
    assert!(
        c[1].1.starts_with("Yes, and always allow access to"),
        "lựa chọn dài phải còn, và đầu nhãn giữ nguyên: {:?}",
        c[1].1
    );
    assert!(
        c[1].1.chars().count() <= 121,
        "nhãn dài phải được CẮT để hiện, không đổ nguyên đường dẫn vào nút: {} ký tự",
        c[1].1.chars().count()
    );
    assert_eq!(c[2].1, "No");
}

/// Chiều ngược: KHÔNG có dòng chân thì trần 120 ký tự vẫn đứng — một đoạn văn
/// đánh số với câu dài không được thành một bảng nút.
#[test]
fn khong_co_dong_chan_thi_cau_dai_danh_so_van_khong_phai_hop_chon() {
    let dai = "x".repeat(130);
    let van = format!("Kế hoạch:\n❯ 1. {dai}\n2. {dai}\n3. ngắn\n");
    assert!(!has_chooser_footer(&van));
    assert!(
        parse_choices(&van).is_empty(),
        "đoạn văn đánh số (câu dài, không dòng chân) bị đọc thành hộp chọn"
    );
}

/// Dòng chân mới không được làm lỏng bộ nhận: chỉ `to amend` hay chỉ `to cancel`
/// đứng một mình trong câu văn thì không phải dòng chân.
#[test]
fn dong_chan_van_doi_hai_ve() {
    assert!(!has_chooser_footer("Đừng quên Tab to amend lại bản nháp."));
    assert!(!has_chooser_footer("Press Esc to cancel the build."));
    assert!(has_chooser_footer(" Esc to cancel · Tab to amend"));
    // Các dòng chân cũ vẫn nhận.
    assert!(has_chooser_footer(
        "Enter to select · ↑/↓ to navigate · Esc to cancel"
    ));
    assert!(has_chooser_footer("Enter to confirm · Esc to cancel"));
}
