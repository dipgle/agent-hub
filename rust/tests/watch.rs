//! Bắt "vừa xong" / "vừa tắt" — và **chỉ nói một lần**.
//!
//! Cái loa này chạy trong một vòng lặp ~10 giây. Sai một nhịp ở đây không ra
//! một dòng log xấu mà ra một cái điện thoại rung mãi không thôi — rồi chủ máy
//! tắt thông báo, và mất luôn cả những lần đáng nghe. Nên mọi ca đều ghim.

use std::collections::BTreeMap;

use hub::sessions::LiveSession;
use hub::watch::{changes, Change, DEAD, IDLE, WORKING};

fn sess(id: &str, name: &str, host: &str, working: bool) -> LiveSession {
    LiveSession {
        session_id: id.to_string(),
        name: name.to_string(),
        host: host.to_string(),
        working,
        ..Default::default()
    }
}

fn book(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Lượt ĐẦU im hoàn toàn.
///
/// Sổ trống nghĩa là hub vừa khởi động lại, không phải mọi phiên vừa đổi trạng
/// thái. Báo hết là một tràng tin cho những việc xảy ra lúc hub còn chưa chạy.
#[test]
fn the_first_round_says_nothing_it_only_writes_the_book() {
    let now = vec![
        sess("a", "dwork", "terminal", true),
        sess("b", "tfl5", "terminal", false),
    ];
    let (events, next) = changes(&BTreeMap::new(), &now);
    assert!(events.is_empty(), "lượt đầu phải im: {events:?}");
    assert_eq!(next.get("a").map(String::as_str), Some(WORKING));
    assert_eq!(next.get("b").map(String::as_str), Some(IDLE));
}

/// Đang chạy → đứng ở dấu nhắc = xong việc, nói ĐÚNG một lần.
#[test]
fn finishing_is_announced_once_not_every_cycle() {
    let prev = book(&[("a", WORKING)]);
    let now = vec![sess("a", "dwork", "terminal", false)];

    let (events, next) = changes(&prev, &now);
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], Change::Finished { id, .. } if id == "a"));
    assert!(events[0].say().contains("chạy xong"));

    // Vòng sau, cùng trạng thái: KHÔNG nói nữa. Đây là điều kiện sống còn —
    // vòng lặp chạy mỗi ~10 giây.
    let (again, _) = changes(&next, &now);
    assert!(again.is_empty(), "không được nói lại: {again:?}");
}

/// Biến mất khỏi danh sách = đã tắt. Đây là ĐƯỜNG CHÍNH, không phải `dead`.
///
/// `claude agents` bỏ một phiên đã dừng khỏi danh sách sau vài giây, nên chỉ
/// rình `host == "dead"` là bỏ lọt gần hết các lần tắt thật.
#[test]
fn a_session_that_leaves_the_list_counts_as_ended() {
    let prev = book(&[("a", WORKING), ("b", IDLE)]);
    let now = vec![sess("b", "tfl5", "terminal", false)];

    let (events, next) = changes(&prev, &now);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Change::Ended { id, was_working, .. } => {
            assert_eq!(id, "a");
            // Tắt lúc đang chạy dở là chuyện khác hẳn tắt lúc đang rảnh — câu
            // nói phải phân biệt, vì một cái là bình thường còn cái kia đáng ngờ.
            assert!(*was_working);
        }
        other => panic!("phải là Ended: {other:?}"),
    }
    assert!(events[0].say().contains("TẮT HẲN"));
    assert!(!next.contains_key("a"), "đã tắt thì rời sổ, không nói lại lần nữa");

    let (again, _) = changes(&next, &now);
    assert!(again.is_empty());
}

/// Còn trong danh sách nhưng `host == "dead"` cũng là tắt — và cũng chỉ một lần.
#[test]
fn a_row_that_turns_dead_is_announced_once_and_then_dropped() {
    let prev = book(&[("a", IDLE)]);
    let now = vec![sess("a", "dwork", "dead", false)];

    let (events, next) = changes(&prev, &now);
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], Change::Ended { was_working: false, .. }));
    // KHÔNG ghi lại vào sổ: nếu ghi, lần sau nó biến khỏi danh sách và bị báo
    // tắt lần thứ hai.
    assert!(!next.contains_key("a"));
    assert_eq!(next.len(), 0);

    let (again, _) = changes(&next, &now);
    assert!(again.is_empty(), "báo tắt hai lần: {again:?}");
}

/// Rảnh → chạy thì KHÔNG nói gì.
///
/// Bắt đầu một lượt là chuyện người ta vừa tự tay làm; báo lại là kể cho người
/// ta nghe chính việc họ vừa bấm.
#[test]
fn starting_work_is_not_worth_a_notification() {
    let prev = book(&[("a", IDLE)]);
    let now = vec![sess("a", "dwork", "terminal", true)];
    let (events, next) = changes(&prev, &now);
    assert!(events.is_empty(), "{events:?}");
    assert_eq!(next.get("a").map(String::as_str), Some(WORKING));
}

/// Nhiều phiên đổi cùng lúc thì nói đủ, không gộp mất cái nào.
#[test]
fn several_sessions_changing_at_once_are_all_reported() {
    let prev = book(&[("a", WORKING), ("b", WORKING), ("c", IDLE)]);
    let now = vec![
        sess("a", "dwork", "terminal", false), // xong
        sess("c", "hub", "terminal", true),    // bắt đầu — im
                                               // b biến mất — tắt
    ];
    let (events, _) = changes(&prev, &now);
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(events.iter().any(|e| matches!(e, Change::Finished { id, .. } if id == "a")));
    assert!(events.iter().any(|e| matches!(e, Change::Ended { id, .. } if id == "b")));
}

/// Sổ cũ có, danh sách rỗng (mọi phiên đã tắt) — vẫn phải nói, và sổ về rỗng.
#[test]
fn everything_disappearing_still_reports_each_one() {
    let prev = book(&[("a", WORKING), ("b", IDLE)]);
    let (events, next) = changes(&prev, &[]);
    assert_eq!(events.len(), 2);
    assert!(next.is_empty());
    assert_eq!(DEAD, "dead"); // hằng số dùng chung với `state_of`
}
