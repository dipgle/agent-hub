//! Đường dẫn tệp trong CÂU VĂN của phiên — nhận cái nào, bỏ cái nào.
//!
//! 🔴 Hà 2026-08-16, đọc một bản *"Xem đầy đủ"* có nhắc `docs/flow-boc-tach-lenh.md`:
//! *"nhận được tin có file nhưng chưa có nút tải hay xem"* · *"Có file .md đấy"*.
//! Luật cũ chỉ nhận đường TUYỆT ĐỐI, nên mọi đường tương đối trong báo cáo của
//! phiên đều là chữ chết trên điện thoại.

use hub::keys::paths_on_screen;

#[test]
fn a_relative_doc_path_in_prose_is_a_file() {
    let text = "Đây là lần thứ ba cùng một hình dạng (12/08 /type, 16/08 /runin, nay đường \
                gợi ý mờ) — tôi đã ghi nó thành luật trong mã.\n\n\
                Còn mỗi docs/flow-boc-tach-lenh.md chờ anh (giữ hay xoá).";
    let got = paths_on_screen(text, 4);
    assert_eq!(got, vec!["docs/flow-boc-tach-lenh.md".to_string()]);
}

/// …và câu văn thường KHÔNG được đẻ ra nút. Đây là cái giá phải trả cho việc
/// nới luật, nên nó phải có phép đo riêng.
///
/// `Node.js` là ca đã bắt được ngay lượt đầu: đuôi `js` hợp lệ, nhưng nó là một
/// cái TÊN giữa câu chứ không phải đường dẫn. Nên đường tương đối phải có `/`.
#[test]
fn ordinary_prose_makes_no_buttons() {
    let text = "Ngày 12/08 và 16/08, v.v. — Node.js chạy được. Tỷ lệ 3.5 vs 4.0, xong rồi.";
    assert!(
        paths_on_screen(text, 4).is_empty(),
        "{:?}",
        paths_on_screen(text, 4)
    );
}

/// Cái giá đã biết và CHẤP NHẬN: tên tệp viết trần, không thư mục, không thành
/// nút. Ghi lại thành phép đo để lần sau ai nới tiếp thì biết mình đang đổi gì.
#[test]
fn a_bare_file_name_without_a_folder_is_not_enough() {
    assert!(paths_on_screen("xem README.md nhé", 4).is_empty());
}

/// Đường tuyệt đối giữ nguyên đường cũ, kể cả đuôi lạ.
#[test]
fn absolute_paths_still_work_with_any_text_extension() {
    let text = "xem ~/projects/hub/hub.config.json và /etc/hosts.allow nhé";
    let got = paths_on_screen(text, 4);
    assert!(
        got.contains(&"~/projects/hub/hub.config.json".to_string()),
        "{got:?}"
    );
}

/// Tệp nhị phân thì không, dù đường tuyệt đối — bấm vào cũng không gửi được.
#[test]
fn binary_files_never_become_buttons() {
    let text = "ảnh ở ~/projects/hub/ui-shots/after.png";
    assert!(paths_on_screen(text, 4).is_empty());
}
