//! Tệp NHỊ PHÂN chủ máy tự làm ra — `.docx`, `.pdf`, `.xlsx` — cũng phải tải
//! được về điện thoại.
//!
//! 🔴 Hà 2026-08-18, ảnh chụp một tin `/shot` của phiên onghut: *"Có file docx
//! nhưng không có nút tải"*. Tin ấy nhắc hai tệp cạnh nhau, cùng một câu:
//!
//! ```text
//! **`docs/phuong-an-trinh.md`** — bản đọc, và **`docs/phuong-an-trinh.docx`**
//! — bản in, Times New Roman 14, A4, lề gáy 3cm theo chuẩn văn bản hành chính.
//! ```
//!
//! `.md` mọc nút, `.docx` thì không — mà **bản in mới là thứ anh cần cầm đi
//! họp**. Hai cửa chặn, và cả hai dựa trên một lý do ĐÃ BÃI BỎ:
//!
//! 1. `keys::TEXT_FILE_EXT` — đường tương đối phải mang đuôi "văn bản đã biết",
//!    `docx` không có trong đó;
//! 2. `telegram::send_document` đọc tệp bằng `read_to_string`, nên mọi thứ
//!    không phải UTF-8 đều rụng.
//!
//! Lý do của cả hai là luật 5 bản CŨ: *"thứ gì rời khỏi máy này phải soi
//! được"*. Luật ấy đã đổi ngày 2026-08-16 (Hà: *"hub là cổng để làm việc từ xa
//! qua tele không cần giấu gì hết"*) — cổng quét rò nay **ghi log rồi đi tiếp**,
//! không chặn. Nên đòi hỏi "phải đọc được thành chữ" chỉ còn là cái vỏ của một
//! hàng rào không còn ai gác: nó không bảo vệ gì, chỉ chặn đúng tệp chủ máy vừa
//! chỉ tay vào.

use std::path::Path;

fn fixture() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/shot-docx-report-2026-08-18.txt");
    std::fs::read_to_string(p).expect("fixture nằm cạnh bài kiểm")
}

const DOCX: &str = "docs/phuong-an-trinh.docx";

/// Tầng 1 — bộ dò phải NHÌN THẤY đường dẫn `.docx`.
#[test]
fn the_docx_is_seen_on_the_screen() {
    let seen = hub::keys::paths_on_screen(hub::keys::body_before_box(&fixture()), 4);
    assert!(
        seen.iter().any(|p| p == DOCX),
        "bộ dò bỏ qua bản in: {seen:?}"
    );
}

/// Tầng 2 — và phép lọc dòng-lệnh không được nuốt nó.
///
/// Đường TUYỆT ĐỐI của cùng tệp ấy nằm trong `open ~/…docx`, nên nếu phép lọc
/// lại hỏi theo đường dẫn thì bản tương đối cũng chết theo (lỗi cùng ngày).
#[test]
fn the_docx_survives_the_command_filter() {
    let text = fixture();
    let seen = hub::keys::paths_on_screen(hub::keys::body_before_box(&text), 4);
    let cmds: Vec<hub::sessions::Cmd> = hub::keys::commands_in_report(&text, 8)
        .into_iter()
        .map(|line| hub::sessions::Cmd {
            line,
            cwd: String::new(),
        })
        .collect();
    let kept = hub::pipeline::paths_not_in_commands(&text, &seen, &cmds);
    assert!(kept.iter().any(|p| p == DOCX), "bản in mất nút: {kept:?}");
}

/// Tầng 3 — và lúc GỬI, tệp nhị phân phải đi được.
///
/// Đây là cửa đắt nhất: hai tầng trên có thể xanh mà cú bấm vẫn trả *"không
/// phải file chữ"*, tức cái nút mọc ra để rồi thất hứa — đúng thứ chú thích ở
/// `remember_files` đã gọi tên một lần.
#[test]
fn a_binary_document_is_read_and_typed_for_sending() {
    let dir = std::env::temp_dir().join("hub-docx-send");
    std::fs::create_dir_all(&dir).unwrap();

    // Đầu tệp thật của một `.docx`: nó là một kho ZIP (`PK\x03\x04`), và byte
    // thứ tư đã đủ làm `read_to_string` hỏng.
    let docx = dir.join("phuong-an-trinh.docx");
    std::fs::write(&docx, b"PK\x03\x04\x14\x00\x06\x00\xff\xfe binary").unwrap();
    let (bytes, mime) = hub::telegram::document_body(&docx).expect("tệp nhị phân phải gửi được");
    assert_eq!(
        bytes.len() as u64,
        std::fs::metadata(&docx).unwrap().len(),
        "phải gửi ĐỦ byte, không cắt xén"
    );
    assert!(
        mime.contains("wordprocessingml"),
        "docx phải mang đúng kiểu MIME, nhận: {mime}"
    );

    // Tệp chữ vẫn như cũ — không đánh đổi cái này lấy cái kia.
    let md = dir.join("phuong-an-trinh.md");
    std::fs::write(&md, "# bản đọc\n").unwrap();
    let (_, mime) = hub::telegram::document_body(&md).expect("tệp chữ vẫn gửi được");
    assert!(mime.starts_with("text/plain"), "nhận: {mime}");
}
