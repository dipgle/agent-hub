//! Bắt "vừa xong" / "vừa tắt" — và **chỉ nói một lần**.
//!
//! Cái loa này chạy trong một vòng lặp ~10 giây. Sai một nhịp ở đây không ra
//! một dòng log xấu mà ra một cái điện thoại rung mãi không thôi — rồi chủ máy
//! tắt thông báo, và mất luôn cả những lần đáng nghe. Nên mọi ca đều ghim.

use std::collections::BTreeMap;

use hub::sessions::LiveSession;
use hub::watch::{changes, Change, Idle, Mark, DEAD, IDLE, MIN_RUN_SEC, WORKING};

/// Mốc thời gian giả, để test không phụ thuộc đồng hồ thật.
const NOW: i64 = 1_800_000_000;

/// Sổ ghi "đang chạy từ lúc nào" — chạy đủ lâu để lượt xong ĐƯỢC tính là tin.
fn mark(state: &str, tty: &str, kind: &str) -> Mark {
    Mark { s: state.to_string(), y: tty.to_string(), k: kind.to_string(), p: String::new() }
}
fn working_long(id: &str) -> (String, Mark) {
    (id.to_string(), mark(&format!("working@{}", NOW - MIN_RUN_SEC - 5), "ttys009", "interactive"))
}

fn sess(id: &str, name: &str, host: &str, working: bool) -> LiveSession {
    LiveSession {
        session_id: id.to_string(),
        name: name.to_string(),
        host: host.to_string(),
        working,
        ..Default::default()
    }
}

fn book(pairs: &[(&str, &str)]) -> BTreeMap<String, Mark> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), mark(v, "ttys009", "interactive")))
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
    let (events, next) = changes(&BTreeMap::new(), &now, NOW);
    assert!(events.is_empty(), "lượt đầu phải im: {events:?}");
    assert!(next.get("a").is_some_and(|m| m.s.starts_with(WORKING)), "{next:?}");
    assert_eq!(next.get("b").map(|m| m.s.as_str()), Some(IDLE));
}

/// Đang chạy → đứng ở dấu nhắc = xong việc, nói ĐÚNG một lần.
#[test]
fn finishing_is_announced_once_not_every_cycle() {
    let prev: BTreeMap<String, Mark> = [working_long("a")].into_iter().collect();
    let now = vec![sess("a", "dwork", "terminal", false)];

    let (events, next) = changes(&prev, &now, NOW);
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], Change::Finished { id, .. } if id == "a"));
    // Kiểm Ý NGHĨA, không kiểm mặt chữ: tin phải nói nó đang CHỜ NGƯỜI.
    // (Câu chữ đổi theo Hà 2026-08-11: "chia làm 2 trường hợp thôi — dừng chờ
    // giao tiếp và tắt hẳn"; ghim mặt chữ thì mỗi lần đổi lời là một test đỏ
    // vô nghĩa.)
    assert!(events[0].say(&Idle::Prompt, None).contains("chờ bạn"));

    // Vòng sau, cùng trạng thái: KHÔNG nói nữa. Đây là điều kiện sống còn —
    // vòng lặp chạy mỗi ~10 giây.
    let (again, _) = changes(&next, &now, NOW);
    assert!(again.is_empty(), "không được nói lại: {again:?}");
}

/// Biến mất khỏi danh sách = đã tắt. Đây là ĐƯỜNG CHÍNH, không phải `dead`.
///
/// `claude agents` bỏ một phiên đã dừng khỏi danh sách sau vài giây, nên chỉ
/// rình `host == "dead"` là bỏ lọt gần hết các lần tắt thật.
#[test]
fn a_session_that_leaves_the_list_counts_as_ended() {
    let prev: BTreeMap<String, Mark> =
        [working_long("a"), ("b".to_string(), mark(IDLE, "ttys002", "interactive"))]
            .into_iter()
            .collect();
    let now = vec![sess("b", "tfl5", "terminal", false)];

    let (events, next) = changes(&prev, &now, NOW);
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
    // Câu kết cục nay do `pipeline` dựng sau khi DÒ cửa sổ terminal, nên ở
    // đây chỉ kiểm sự kiện mang đủ dữ kiện để dò.
    assert!(matches!(&events[0], Change::Ended { tty, kind, .. } if !tty.is_empty() && !kind.is_empty()));
    assert!(!next.contains_key("a"), "đã tắt thì rời sổ, không nói lại lần nữa");

    let (again, _) = changes(&next, &now, NOW);
    assert!(again.is_empty());
}

/// Còn trong danh sách nhưng `host == "dead"` cũng là tắt — và cũng chỉ một lần.
#[test]
fn a_row_that_turns_dead_is_announced_once_and_then_dropped() {
    let prev = book(&[("a", IDLE)]);
    let now = vec![sess("a", "dwork", "dead", false)];

    let (events, next) = changes(&prev, &now, NOW);
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], Change::Ended { was_working: false, .. }));
    // KHÔNG ghi lại vào sổ: nếu ghi, lần sau nó biến khỏi danh sách và bị báo
    // tắt lần thứ hai.
    assert!(!next.contains_key("a"));
    assert_eq!(next.len(), 0);

    let (again, _) = changes(&next, &now, NOW);
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
    let (events, next) = changes(&prev, &now, NOW);
    assert!(events.is_empty(), "{events:?}");
    assert!(next.get("a").is_some_and(|m| m.s.starts_with(WORKING)), "{next:?}");
}

/// Nhiều phiên đổi cùng lúc thì nói đủ, không gộp mất cái nào.
#[test]
fn several_sessions_changing_at_once_are_all_reported() {
    let prev: BTreeMap<String, Mark> = [
        working_long("a"),
        working_long("b"),
        ("c".to_string(), mark(IDLE, "ttys003", "interactive")),
    ]
    .into_iter()
    .collect();
    let now = vec![
        sess("a", "dwork", "terminal", false), // xong
        sess("c", "hub", "terminal", true),    // bắt đầu — im
                                               // b biến mất — tắt
    ];
    let (events, _) = changes(&prev, &now, NOW);
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(events.iter().any(|e| matches!(e, Change::Finished { id, .. } if id == "a")));
    assert!(events.iter().any(|e| matches!(e, Change::Ended { id, .. } if id == "b")));
}

/// Sổ cũ có, danh sách rỗng (mọi phiên đã tắt) — vẫn phải nói, và sổ về rỗng.
#[test]
fn everything_disappearing_still_reports_each_one() {
    let prev: BTreeMap<String, Mark> =
        [working_long("a"), ("b".to_string(), mark(IDLE, "ttys002", "interactive"))]
            .into_iter()
            .collect();
    let (events, next) = changes(&prev, &[], NOW);
    assert_eq!(events.len(), 2);
    assert!(next.is_empty());
    assert_eq!(DEAD, "dead"); // hằng số dùng chung với `state_of`
}

/// Chạy chớp nhoáng thì KHÔNG phải tin.
///
/// Đo thật lượt đầu bật loa (2026-08-10): một phiên có người ngồi gõ bắn "vừa
/// chạy xong" hai lần trong 75 giây — cả hai đều ĐÚNG, nó chạy hai lượt ngắn
/// thật. Đúng mà vẫn sai chỗ: người ấy đang nhìn thẳng vào phiên đó. Cái loa
/// này có giá trị ở phiên KHÔNG ai nhìn.
#[test]
fn a_burst_of_short_turns_stays_quiet() {
    let now = vec![sess("a", "hub-bd", "terminal", false)];

    // Vừa chạy 10 giây rồi dừng: im.
    let brief: BTreeMap<String, Mark> =
        [("a".to_string(), mark(&format!("working@{}", NOW - 10), "ttys009", "interactive"))]
            .into_iter()
            .collect();
    let (events, _) = changes(&brief, &now, NOW);
    assert!(events.is_empty(), "lượt ngắn không được kêu: {events:?}");

    // Chạy quá ngưỡng rồi dừng: nói, và nói luôn nó chạy bao lâu.
    let long: BTreeMap<String, Mark> = [working_long("a")].into_iter().collect();
    let (events, _) = changes(&long, &now, NOW);
    assert_eq!(events.len(), 1);
    assert!(events[0].say(&Idle::Prompt, None).contains("phút"), "{}", events[0].say(&Idle::Prompt, None));
}

/// Tin phải NÓI RA THỨ NHÌN THẤY, và mỗi tin phải khác nhau.
///
/// Hà 2026-08-10, đọc Telegram: *"rõ ràng là lỗi mà sao tele tôi nhận được lại
/// là phiên đang đứng ở dấu nhắc, chờ lượt sau"* và *"toàn thông báo giống
/// nhau"*. Vế đầu nặng hơn: câu ấy là một khẳng định hub không hề biết — thứ nó
/// biết chỉ là "nhật ký thôi lớn lên", mà nhật ký cũng thôi lớn lên khi phiên
/// KẸT ở hộp thoại. Vế sau: tin nào cũng một câu thì người ta thôi đọc.
#[test]
fn the_message_reports_what_was_seen_and_never_repeats_itself() {
    let e = Change::Finished { id: "a".into(), name: "dwork".into(), ran_sec: 300 };

    // Màn có hộp chọn ⟹ KHÔNG được nói "xong", phải nói là đang kẹt hỏi và
    // CẦN người trả lời — đó là khác biệt duy nhất người đọc quan tâm.
    //
    // Và phải mang NGUYÊN VĂN từng lựa chọn. Hà 2026-08-11: *"cần thêm thông
    // tin mô tả liên quan tới lựa chọn đó mới hợp lý"* — một cái chuông chỉ nói
    // "có 3 lựa chọn" vẫn bắt người ta mở máy ra mới biết chọn gì.
    let asking = e.say(
        &Idle::Asking {
            n: 3,
            options: vec![
                "Yes, and don't ask again".into(),
                "Yes".into(),
                "No, tell Claude what to do differently".into(),
            ],
        },
        None,
    );
    assert!(asking.contains("HỎI") && asking.contains("cần bạn"), "{asking}");
    assert!(asking.contains("don't ask again"), "phải có chữ của lựa chọn: {asking}");
    assert!(asking.contains("No, tell Claude"), "phải có ĐỦ các lựa chọn: {asking}");
    assert!(!asking.contains("dấu nhắc"), "không được khẳng định đang rảnh: {asking}");

    // Màn bị giữ lại vì có dấu hiệu bí mật ⟹ chỉ CON SỐ, và phải nói vì sao —
    // im lặng đưa mỗi con số thì người đọc tưởng hub keo kiệt, rồi lần sau bỏ
    // qua cả những tin có chữ.
    let hidden = e.say(&Idle::Asking { n: 2, options: vec![] }, None);
    assert!(hidden.contains('2'), "{hidden}");
    assert!(hidden.contains("bí mật"), "phải khai vì sao không có chữ: {hidden}");

    // Không đọc được màn ⟹ nói thẳng là không đọc được, đừng đoán.
    let blind = e.say(&Idle::Unknown, None);
    assert!(blind.contains("không đọc được màn"), "{blind}");

    // Câu cuối của phiên đi kèm ⟹ hai tin khác nhau đọc ra hai chuyện khác nhau.
    let a = e.say(&Idle::Prompt, Some("[dwork] đã dựng phiếu chuyển thiết kế"));
    let b = e.say(&Idle::Prompt, Some("[mailler] soak xong, 0 đỏ"));
    assert!(a.contains("phiếu chuyển"), "{a}");
    assert_ne!(a, b, "hai phiên khác nhau mà tin giống hệt nhau");
}
