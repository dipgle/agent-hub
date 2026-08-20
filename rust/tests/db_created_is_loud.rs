//! Dựng một DB rỗng là một SỰ KIỆN, không phải chuyện lặng lẽ.
//!
//! 🔴 Đêm 2026-08-20, `data/hub.sqlite` bị đổi tên trong lúc daemon đang giữ
//! nó. Kết nối mở-sẵn bám theo inode nên vẫn ghi đúng tệp; nhưng `telegram.rs`
//! mở-mới mỗi lượt theo đường dẫn của cấu hình lúc boot, không thấy gì ở đó,
//! và `Connection::open` **dựng một DB rỗng** — im lặng, exit 0, không một
//! dòng log. Từ đó `focus:session` ghi một bên đọc một bên: `/new` trỏ vào cửa
//! sổ `ttys004`, còn chữ chủ máy gõ đi thẳng vào phiên `dwork`. Trong nhật ký,
//! hai lần đọc CÙNG một khoá cách nhau 5,6 giây trả về hai giá trị khác nhau
//! mà không có lần ghi nào ở giữa — và không có gì để mà grep.
//!
//! Bài kiểm này giữ lấy dòng log ấy. Nó không ngăn được cú đổi tên; nó rút một
//! đêm truy lỗi xuống còn một lần tìm.

use std::fs;

use huba::db::Db;

#[test]
fn a_conjured_database_says_so_once_and_not_again() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log = dir.path().join("huba.log");
    huba::logging::set_log_file(&log);

    let db_path = dir.path().join("data").join("huba.sqlite");
    assert!(!db_path.exists(), "phải bắt đầu từ chỗ chưa có tệp");

    let db = Db::open(&db_path).expect("open");
    let after_first = fs::read_to_string(&log).unwrap_or_default();

    assert!(
        after_first.contains("db_created"),
        "dựng DB rỗng mà không nói gì — đúng lỗi im lặng của 08-20:\n{after_first}"
    );
    // Phải in ĐƯỜNG DẪN. Một cảnh báo không nói tệp nào thì người đọc vẫn phải
    // đi dò, tức vẫn là cái đêm ấy.
    assert!(
        after_first.contains(&db_path.display().to_string()),
        "cảnh báo không nói tệp nào: {after_first}"
    );

    // 🔴 Nửa quan trọng không kém: mở LẠI đúng tệp ấy phải IM. `Db::open` được
    // gọi mỗi lượt poll ở `telegram.rs`; kêu mọi lần thì dòng cảnh báo chìm
    // trong chính nó, và một cảnh báo luôn xuất hiện là một cảnh báo không ai
    // đọc. Nó chỉ được kêu đúng lúc có tệp vừa bị dựng ra.
    drop(db);
    let _again = Db::open(&db_path).expect("reopen");
    let after_second = fs::read_to_string(&log).unwrap_or_default();

    assert_eq!(
        after_second.matches("db_created").count(),
        1,
        "mở lại tệp ĐÃ CÓ vẫn kêu ⟹ mỗi lượt poll một dòng, cảnh báo tự chôn mình:\n{after_second}"
    );
}
