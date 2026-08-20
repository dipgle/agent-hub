//! `document_body` chạy THẬT trên một tệp có thật của máy này.
//!
//! 🔴 Bài kiểm thuần (`tests/file_button_docx.rs`) dựng một `.docx` giả 17 byte
//! — đủ để chứng minh `read_to_string` không còn chặn, KHÔNG đủ để nói một bản
//! `.docx` thật 28 KB do Word/`python-docx` sinh ra thì đọc ra bao nhiêu byte và
//! mang kiểu gì. Mà đó mới là thứ rơi vào điện thoại của Hà.
//!
//! Đòi đường dẫn qua biến môi trường, KHÔNG có mặc định và KHÔNG tự bỏ qua: một
//! bài kiểm im lặng trôi qua khi thiếu đầu vào là một phép đo mù — nó luôn xanh,
//! nên nó không nói gì.
//!
//! ```
//! cd ~/projects/huba/rust
//! HUB_DOC_LIVE=~/projects/onghut/docs/phuong-an-trinh.docx \
//!   cargo test --offline --test document_body_live -- --ignored --nocapture
//! ```

#[test]
#[ignore = "cần một tệp thật trên máy — chạy tay bằng --ignored"]
fn a_real_file_on_this_mac_is_read_whole_and_typed() {
    let p = std::env::var("HUB_DOC_LIVE")
        .expect("đặt HUB_DOC_LIVE=<đường dẫn tệp thật> — không có mặc định, xem đầu tệp");
    let path = std::path::PathBuf::from(shellexpand(&p));
    let on_disk = std::fs::metadata(&path)
        .unwrap_or_else(|e| panic!("không đọc được {}: {e}", path.display()))
        .len();

    let (bytes, mime) = huba::telegram::document_body(&path).expect("huba phải gửi được tệp này");
    println!(
        "{} · {} byte trên đĩa · {} byte đọc được · {mime}",
        path.display(),
        on_disk,
        bytes.len()
    );
    assert_eq!(
        bytes.len() as u64,
        on_disk,
        "gửi thiếu byte thì tệp tới nơi cũng hỏng"
    );
    assert!(!mime.is_empty());
}

/// `~` là chữ của shell, không phải của filesystem — một đường dẫn dán từ tin
/// nhắn thường mang nó.
fn shellexpand(p: &str) -> String {
    match p.strip_prefix("~/") {
        Some(rest) => format!("{}/{rest}", std::env::var("HOME").unwrap_or_default()),
        None => p.to_string(),
    }
}
