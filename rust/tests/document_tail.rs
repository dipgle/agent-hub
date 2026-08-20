//! Tệp quá trần THẬT của Telegram thì gửi PHẦN CUỐI, không bỏ cuộc.
//!
//! 🔴 Hà 2026-08-19: *"Sao lại có giới hạn dung lượng"* — ảnh chụp
//! `⚠ chưa gửi được huba.log — 21.4 MB — quá trần 5 MB`. Trần 5 MB ấy là của
//! **huba**, viết ra từ một câu đoán hộ (*"một file 5 MB đọc trên điện thoại là
//! chuyện không xảy ra"*), trong khi Telegram cho tới 50 MB. Cùng họ với ba lần
//! trước: một hàng rào dựng trên khẩu vị, áp lên đúng cái nút chủ máy tự bấm.
//!
//! Trần nay chỉ còn là trần của Telegram. Trên nó là tường thật, và đường ra là
//! thứ chủ máy làm khi ngồi ở máy: `tail`.

use std::io::Write;

fn tmp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("huba-doc-tail");
    std::fs::create_dir_all(&dir).expect("dựng thư mục tạm");
    dir.join(name)
}

/// Cắt phải bắt đầu ở ĐẦU MỘT DÒNG, và phải là đúng phần cuối.
#[test]
fn the_tail_starts_on_a_line_and_ends_at_the_end() {
    let p = tmp("nhat-ky.log");
    let mut f = std::fs::File::create(&p).expect("ghi tệp thử");
    for i in 0..5000 {
        writeln!(
            f,
            "dòng {i} — có dấu tiếng Việt để chắc chắn cắt đúng ranh giới UTF-8"
        )
        .expect("ghi dòng");
    }
    drop(f);
    let whole = std::fs::read_to_string(&p).expect("đọc lại");

    let (bytes, mime) = huba::telegram::document_tail(&p, 4096).expect("phải cắt được");
    let tail = String::from_utf8(bytes).expect("cắt xong vẫn phải là UTF-8 hợp lệ");

    assert!(tail.len() <= 4096, "cắt hụt trần: {} byte", tail.len());
    assert!(
        whole.ends_with(&tail),
        "phần cắt không phải phần CUỐI của tệp"
    );
    assert!(
        tail.starts_with("dòng "),
        "mở đầu bằng một dòng cụt: {:?}",
        &tail[..40.min(tail.len())]
    );
    assert!(tail.contains("dòng 4999"), "thiếu dòng cuối cùng");
    assert!(mime.starts_with("text/plain"), "{mime}");
}

/// Tệp NHỎ hơn phần muốn cắt thì trả nguyên vẹn — không được rụng dòng đầu.
///
/// Đây là chỗ dễ hỏng nhất của phép cắt "bỏ tới dấu xuống dòng đầu tiên": làm vô
/// điều kiện là ăn mất dòng đầu của một tệp lẽ ra đọc trọn.
#[test]
fn a_small_file_comes_back_whole() {
    let p = tmp("nho.txt");
    std::fs::write(&p, "dòng đầu\ndòng hai\n").expect("ghi tệp thử");
    let (bytes, _) = huba::telegram::document_tail(&p, 1024).expect("phải đọc được");
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "dòng đầu\ndòng hai\n",
        "cắt mất dòng đầu của một tệp đọc trọn"
    );
}

/// Tệp nhị phân thì KHÔNG cắt — nửa cuối một `.zip` không mở được bằng gì, và
/// gửi nó đi là gửi một thứ trông như thành công.
#[test]
fn a_binary_file_is_not_tailed() {
    let p = tmp("anh.bin");
    std::fs::write(&p, [0x89u8, 0x50, 0x4e, 0x47, 0xff, 0xfe, 0x00, 0x01]).expect("ghi tệp thử");
    let err = huba::telegram::document_tail(&p, 4).expect_err("tệp nhị phân phải bị từ chối cắt");
    assert!(err.contains("nhị phân"), "lý do phải đọc được: {err}");
}

/// Và phép cắt phải chạy được trên tệp THẬT đã làm Hà hỏi — `logs/huba.log`.
///
/// Không mặc định, không tự bỏ qua: đòi đường dẫn qua biến môi trường, cùng
/// khuôn với `document_body_live` (một bài kiểm im lặng trôi qua là bài kiểm
/// luôn xanh, tức không nói gì).
///
/// ```
/// HUB_TAIL_LIVE=~/projects/huba/logs/huba.log \
///   cargo test --offline --test document_tail -- --ignored --nocapture
/// ```
#[test]
#[ignore = "cần tệp thật trên máy — chạy tay bằng --ignored"]
fn the_real_hub_log_can_be_tailed() {
    let raw = std::env::var("HUB_TAIL_LIVE").expect("đặt HUB_TAIL_LIVE=<đường dẫn nhật ký thật>");
    let path = std::path::PathBuf::from(match raw.strip_prefix("~/") {
        Some(rest) => format!("{}/{rest}", std::env::var("HOME").unwrap_or_default()),
        None => raw,
    });
    let on_disk = std::fs::metadata(&path)
        .unwrap_or_else(|e| panic!("không đọc được {}: {e}", path.display()))
        .len();
    let want = 5 * 1024 * 1024;
    let (bytes, mime) = huba::telegram::document_tail(&path, want).expect("phải cắt được");
    println!(
        "{} · {on_disk} byte trên đĩa · {} byte phần cuối · {mime}",
        path.display(),
        bytes.len()
    );
    assert!(bytes.len() as u64 <= want);
    let text = String::from_utf8(bytes).expect("phần cuối phải là UTF-8 hợp lệ");
    assert!(
        text.starts_with('{'),
        "nhật ký là JSON mỗi dòng — mở đầu bằng dòng cụt: {:?}",
        &text[..40.min(text.len())]
    );
}
