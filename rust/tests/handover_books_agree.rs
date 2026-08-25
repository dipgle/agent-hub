//! Hai cuốn sổ của `auto_handover` phải nói cùng một sự thật.
//!
//! 🔴 Hà 2026-08-24: *"Trong danh sách phiên tôi thấy có 1 phiên 64% rồi tại
//! sao chưa tự chuyển, tôi thấy vấn đề này chạy không được ổn định"*.
//!
//! Anh mô tả đúng cả triệu chứng lẫn tính chất. Nó **không** ổn định, và thứ
//! quyết định phiên nào hỏng là **thứ tự chữ cái của uuid**:
//!
//! * `auto_handover:done` là một `Vec`, cắt từ đầu ⟹ đúng là cũ-trước;
//! * `auto_handover:pct` là một `BTreeMap`, cắt bằng `keys().next()` ⟹ bỏ khoá
//!   **nhỏ nhất theo bảng chữ cái**, chẳng liên quan gì tới tuổi.
//!
//! Đo trên DB thật lúc phát hiện: `pct` mở đầu bằng `5a7f2f4a` — mọi khoá bắt
//! đầu bằng `0`–`4` đã biến mất, còn `done` vẫn giữ chúng. Phiên `1ad3e613` rơi
//! đúng khe: **có** trong `done`, **mất** trong `pct` ⟹ mốc hỏi-lại rơi về
//! `unwrap_or(at_percent) + 10` = 70%, nên nó nằm im ở 63% suốt nhiều giờ trong
//! khi log chỉ nói `AlreadyDone`.
//!
//! Hai bản vá, và bài kiểm này khoá cả hai.

use huba::pipeline::{auto_handover_why, AutoWhy};

const AT: u8 = 60;

/// Gọi ĐÚNG hàm `auto_handover` gọi — không dựng lại phép quyết định ở đây.
///
/// 🔴 Bản đầu của bài kiểm này chép logic vào chính nó, nên nó xanh kể cả khi
/// mã sản xuất hỏng. Luật §13 của workspace gọi đúng tên: *một cổng không bao
/// giờ đỏ được là một cổng không có*. Bắt được lúc tự soi lại, 2026-08-25.
fn already_done(sid: &str, pct: u8, done: &[&str], done_at: &[(&str, u8)]) -> bool {
    let done: Vec<String> = done.iter().map(|s| s.to_string()).collect();
    let at: std::collections::BTreeMap<String, u8> =
        done_at.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    huba::pipeline::already_handed_over(sid, pct, &done, &at)
}

/// 🔴 Ca của Hà: có trong `done`, MẤT trong `pct` ⟹ phải hỏi lại, không khoá.
#[test]
fn a_session_whose_recorded_pct_was_evicted_is_reconsidered() {
    let done = ["1ad3e613"];
    let done_at: [(&str, u8); 0] = []; // đã bị cắt khỏi sổ pct
    assert!(
        !already_done("1ad3e613", 63, &done, &done_at),
        "quên mốc cũ mà vẫn khoá ⟹ phiên đứng im tới 70%"
    );
    // …và khi ấy nó phải đi tiếp tới các cửa THẬT, không dừng ở AlreadyDone.
    let why = auto_handover_why(63, AT, false, true, false, false, 0, 300, 120);
    assert_eq!(why, AutoWhy::Do, "{why:?}");
}

/// Còn nhớ mốc thì luật cũ giữ nguyên — bàn giao ở 61% thì im tới 71%.
#[test]
fn a_remembered_pct_still_holds_until_it_climbs_a_step() {
    let done = ["1ad3e613"];
    let at = [("1ad3e613", 61u8)];
    assert!(already_done("1ad3e613", 63, &done, &at), "63 < 71 thì phải giữ");
    assert!(already_done("1ad3e613", 70, &done, &at), "70 < 71 thì vẫn giữ");
    assert!(
        !already_done("1ad3e613", 71, &done, &at),
        "leo đủ một mốc thì phải hỏi lại"
    );
}

/// Chưa từng bàn giao thì `pct` không liên quan.
#[test]
fn a_session_never_handed_over_is_never_already_done() {
    let at = [("1ad3e613", 61u8)];
    assert!(!already_done("aaaaaaaa", 99, &[], &at));
    assert!(!already_done("aaaaaaaa", 99, &["1ad3e613"], &at));
}

/// 🔴 Sổ `pct` phải cắt THEO `done`, không theo thứ tự chữ cái.
///
/// Dựng lại đúng phép cắt mới và chứng minh nó không còn phụ thuộc uuid: một
/// khoá bắt đầu bằng `0` vẫn sống chừng nào `done` còn giữ nó, và biến mất đúng
/// lúc `done` bỏ nó — chứ không phải vì nó xếp trước theo bảng chữ cái.
#[test]
fn the_pct_book_follows_the_done_list_not_the_alphabet() {
    let mut at: std::collections::BTreeMap<String, u8> = Default::default();
    for k in ["0aaa", "1ad3e613", "5a7f2f4a", "zzzz"] {
        at.insert(k.to_string(), 61);
    }
    // `done` giữ ba cái, bỏ `5a7f2f4a` — thứ mà phép cắt cũ sẽ GIỮ LẠI.
    let done: Vec<String> = ["0aaa", "1ad3e613", "zzzz"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    at.retain(|k, _| done.contains(k));

    assert!(at.contains_key("0aaa"), "khoá 'nhỏ' bị cắt oan lần nữa");
    assert!(at.contains_key("1ad3e613"), "đúng ca của Hà, lại mất");
    assert!(!at.contains_key("5a7f2f4a"), "đã rời done mà vẫn nằm lại");
    assert_eq!(at.len(), done.len(), "hai sổ phải cùng một tập");
}
