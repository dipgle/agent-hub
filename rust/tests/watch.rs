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
    Mark {
        s: state.to_string(),
        y: tty.to_string(),
        k: kind.to_string(),
        p: String::new(),
        // Đã thấy từ lâu: mặc định của các test cũ là phiên sống thật, không
        // phải phiên chớp nhoáng — cửa tuổi thọ mới không được đổi ý nghĩa của
        // chúng.
        f: NOW - 3600,
        h: false,
        n: String::new(),
        d: String::new(),
        // Mặc định của test cũ: sổ ĐÃ biết phiên này thuộc tài khoản nào. Sổ
        // chưa biết (`a` rỗng) là một ca riêng, có test riêng bên dưới.
        a: "acc1".to_string(),
        c: "/Users/hanguyen/projects".to_string(),
        i: 4242,
        o: "terminal".to_string(),
    }
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

/// Thư mục làm việc của hubd — nơi mọi phép dò `/usage` của chính hub chạy.
fn hub_runtime_cwd() -> String {
    format!(
        "{}/Library/Application Support/hub",
        std::env::var("HOME").unwrap_or_default()
    )
}

/// Phép dò hạn mức của CHÍNH hub sống LÂU vẫn không phải tin.
///
/// 🔴 Hà 2026-08-12, đọc `⏹ hub-67 (033059d8) đã tắt — cửa sổ ấy nay đang chạy
/// phiên hub-ec.`: *"quá vô lý"*. Cửa tuổi thọ (120 giây) bắt được phần lớn phép
/// dò, nhưng nó bắt sai chỗ: thứ khiến cái chết ấy không phải tin không phải là
/// **nó ngắn** mà là **nó của hub**. Ca thật lọt lưới: phiên nằm trong danh sách
/// 11 phút vì lượt dò treo tới trần 60 giây.
#[test]
fn hub_own_usage_probe_never_rings_even_when_it_lived_long() {
    let mut m = mark(IDLE, "", "interactive");
    m.f = NOW - 11 * 60; // sống 11 phút — qua thừa cửa tuổi thọ
    m.c = hub_runtime_cwd();
    let prev: BTreeMap<String, Mark> = [("probe".to_string(), m)].into_iter().collect();
    let (events, next) = changes(&prev, &[], NOW, &[]);
    assert!(events.is_empty(), "chuông kêu vì phiên của chính hub: {events:?}");
    assert!(!next.contains_key("probe"), "giữ lại trong sổ thì lượt sau lại kêu");
}

/// …kể cả khi nó CÒN SỐNG: phép dò của hub cũng không được báo "vừa xong", và
/// không được vào sổ. Hai cửa, hai đường khác nhau — cửa trên đọc sổ (phiên đã
/// rời danh sách), cửa này đọc danh sách đang sống.
#[test]
fn a_running_hub_probe_never_reports_finishing_either() {
    let mut m = mark(
        &format!("working@{}", NOW - MIN_RUN_SEC - 5),
        "",
        "interactive",
    );
    m.c = hub_runtime_cwd();
    let prev: BTreeMap<String, Mark> = [("probe".to_string(), m)].into_iter().collect();
    let mut s = sess("probe", "hub-67", "detached", false);
    s.cwd = hub_runtime_cwd();
    let (events, next) = changes(&prev, &[s], NOW, &[]);
    assert!(events.is_empty(), "chuông kêu cho phép dò của hub: {events:?}");
    // Vẫn VÀO SỔ: cửa đặt ở chỗ phát ngôn, không ở đầu vào — nhờ vậy lúc nó
    // chết còn có một dòng `session_end_muted` để kiểm luật có đang chạy không.
    assert!(next.contains_key("probe"), "sổ phải nhớ nó như mọi phiên khác");
}

/// …và một phiên THẬT sống lâu thì vẫn phải báo — luật trên không được siết lan.
#[test]
fn a_real_long_lived_session_still_rings_when_it_ends() {
    let mut m = mark(IDLE, "", "interactive");
    m.f = NOW - 11 * 60;
    m.c = "/Users/hanguyen/projects".to_string();
    let prev: BTreeMap<String, Mark> = [("real".to_string(), m)].into_iter().collect();
    let (events, _) = changes(&prev, &[], NOW, &[]);
    assert_eq!(events.len(), 1, "nuốt mất tin của phiên thật: {events:?}");
}

/// Phiên sống chớp nhoáng chết đi thì KHÔNG phải tin.
///
/// 🔴 Hà 2026-08-12: *"tại sao cứ báo phiên đã tắt liên tục"*. Log: 20 tin trong
/// 4 tiếng, mỗi tin một id khác — không phải một phiên báo lặp, mà là **phép dò
/// hạn mức của chính hub** (`claude -p "/usage"`, 5 phút một lượt) đẻ ra phiên
/// thật rồi kết thúc trong vài giây.
#[test]
fn a_session_that_lived_only_seconds_dies_quietly() {
    let mut m = mark(IDLE, "", "background");
    m.f = NOW - 20; // vừa sinh ra 20 giây trước
    let prev: BTreeMap<String, Mark> = [("probe".to_string(), m)].into_iter().collect();
    let (events, _) = changes(&prev, &[], NOW, &[]);
    assert!(events.is_empty(), "phiên sống 20 giây mà vẫn kêu: {events:?}");
}

/// …trừ phiên do CHÍNH hub mở: ở đó chết ≠ xong.
///
/// Chủ máy bấm mở một phiên từ điện thoại rồi nó chết trong 30 giây là đúng thứ
/// phải báo — người mở đang chờ nó chạy, không ngồi nhìn màn hình máy.
#[test]
fn a_hub_opened_session_dying_young_is_always_reported() {
    let mut m = mark(IDLE, "ttys009", "interactive");
    m.f = NOW - 20;
    m.h = true;
    let prev: BTreeMap<String, Mark> = [("just-opened".to_string(), m)].into_iter().collect();
    let (events, _) = changes(&prev, &[], NOW, &[]);
    assert_eq!(events.len(), 1, "phiên hub vừa mở mà chết thì phải báo: {events:?}");
}

/// Tin báo phải gọi được TÊN phiên, không chỉ id.
///
/// Hà 2026-08-12: *"không biết nó là phiên nào rất mơ hồ"*. Lúc phiên rời khỏi
/// danh sách thì hàng của nó đi theo, nên tên phải nằm sẵn trong sổ.
#[test]
fn the_farewell_says_which_session_it_was() {
    let mut m = mark(IDLE, "ttys009", "interactive");
    m.n = "projects-71".into();
    m.d = "AI/hub".into();
    let prev: BTreeMap<String, Mark> =
        [("8db91183-1111-2222-3333-444444444444".to_string(), m)].into_iter().collect();
    let (events, _) = changes(&prev, &[], NOW, &[]);
    let said = match events.first() {
        Some(Change::Ended { name, .. }) => name.clone(),
        other => panic!("phải là Ended: {other:?}"),
    };
    // Nhãn là DỰ ÁN, không phải tên `claude` tự đặt — Hà 2026-08-13 phải nhắc
    // hai lần vì lượt trước tôi đổi ở danh sách mà quên đường của cái loa.
    assert!(said.contains("[AI/hub]"), "thiếu dự án: {said}");
    assert!(!said.contains("projects-71"), "tên tự sinh vẫn chiếm chỗ: {said}");
    assert!(said.contains("8db91183"), "thiếu id để gõ lệnh tiếp: {said}");
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
    let (events, next) = changes(&BTreeMap::new(), &now, NOW, &[]);
    assert!(events.is_empty(), "lượt đầu phải im: {events:?}");
    assert!(next.get("a").is_some_and(|m| m.s.starts_with(WORKING)), "{next:?}");
    assert_eq!(next.get("b").map(|m| m.s.as_str()), Some(IDLE));
}

/// Đang chạy → đứng ở dấu nhắc = xong việc, nói ĐÚNG một lần.
#[test]
fn finishing_is_announced_once_not_every_cycle() {
    let prev: BTreeMap<String, Mark> = [working_long("a")].into_iter().collect();
    let now = vec![sess("a", "dwork", "terminal", false)];

    let (events, next) = changes(&prev, &now, NOW, &[]);
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], Change::Finished { id, .. } if id == "a"));
    // Kiểm Ý NGHĨA, không kiểm mặt chữ: tin phải nói nó đang CHỜ NGƯỜI.
    // (Câu chữ đổi theo Hà 2026-08-11: "chia làm 2 trường hợp thôi — dừng chờ
    // giao tiếp và tắt hẳn"; ghim mặt chữ thì mỗi lần đổi lời là một test đỏ
    // vô nghĩa.)
    assert!(events[0].say(&Idle::Prompt, None).contains("chờ bạn"));

    // Vòng sau, cùng trạng thái: KHÔNG nói nữa. Đây là điều kiện sống còn —
    // vòng lặp chạy mỗi ~10 giây.
    let (again, _) = changes(&next, &now, NOW, &[]);
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

    let (events, next) = changes(&prev, &now, NOW, &[]);
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

    let (again, _) = changes(&next, &now, NOW, &[]);
    assert!(again.is_empty());
}

/// Còn trong danh sách nhưng `host == "dead"` cũng là tắt — và cũng chỉ một lần.
#[test]
fn a_row_that_turns_dead_is_announced_once_and_then_dropped() {
    let prev = book(&[("a", IDLE)]);
    let now = vec![sess("a", "dwork", "dead", false)];

    let (events, next) = changes(&prev, &now, NOW, &[]);
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], Change::Ended { was_working: false, .. }));
    // KHÔNG ghi lại vào sổ: nếu ghi, lần sau nó biến khỏi danh sách và bị báo
    // tắt lần thứ hai.
    assert!(!next.contains_key("a"));
    assert_eq!(next.len(), 0);

    let (again, _) = changes(&next, &now, NOW, &[]);
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
    let (events, next) = changes(&prev, &now, NOW, &[]);
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
    let (events, _) = changes(&prev, &now, NOW, &[]);
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
    let (events, next) = changes(&prev, &[], NOW, &[]);
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
    let (events, _) = changes(&brief, &now, NOW, &[]);
    assert!(events.is_empty(), "lượt ngắn không được kêu: {events:?}");

    // Chạy quá ngưỡng rồi dừng: nói, và nói luôn nó chạy bao lâu.
    let long: BTreeMap<String, Mark> = [working_long("a")].into_iter().collect();
    let (events, _) = changes(&long, &now, NOW, &[]);
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

/// Phiên BẮT ĐẦU HỎI là một sự kiện, và nó không bao giờ bị im.
///
/// Hà 2026-08-12: *"có 1 phiên đang đưa lựa chọn nhưng không nhận được trên
/// tele"*. Trước đây hub chỉ nhận ra "đang hỏi" nếu nó tình cờ đọc màn đúng lúc
/// phiên vừa im, và luật "đừng kêu vào mặt người đang nhìn" nuốt nốt phần còn
/// lại. Nay "đang hỏi" là một TRẠNG THÁI đọc từ nhật ký.
#[test]
fn a_session_that_starts_asking_is_announced_once_with_its_options() {
    let mut s = sess("a", "dwork", "terminal", false);
    s.asking = Some(hub::sessions::Asking {
        header: "Nửa ngày".into(),
        question: "Đơn vắng có khai được NỬA NGÀY không?".into(),
        options: vec!["Thêm ô".into(), "Trọn ngày".into()],
    });
    let prev: BTreeMap<String, Mark> = [("a".to_string(), mark(IDLE, "ttys009", "interactive"))]
        .into_iter()
        .collect();
    let (events, next) = changes(&prev, &[s.clone()], NOW, &[]);
    let said = match events.first() {
        Some(c @ Change::Asking { .. }) => c.say(&Idle::Unknown, None),
        other => panic!("phải là Asking: {other:?}"),
    };
    assert!(said.contains("dừng lại HỎI"), "{said}");
    assert!(said.contains("Nửa ngày"), "{said}");
    assert!(said.contains("1. Thêm ô") && said.contains("2. Trọn ngày"), "{said}");

    // NÓI MỘT LẦN: vòng sau vẫn đang hỏi thì im.
    let (again, _) = changes(&next, &[s], NOW + 60, &[]);
    assert!(again.is_empty(), "kêu lại lần hai: {again:?}");
}
// ─────────────────────────────────────────────────────────────────────────────
// key_points — rút thông tin chốt cho một cái chuông trên điện thoại
//
// Mẫu thử là một bản báo cáo THẬT, cắt ngắn: phiên `296972d4` ngày 2026-08-12
// trả lời "sao máy treo". Nó có đủ bốn hình dạng làm hỏng bản đầu của hàm này —
// đoạn văn dài có chữ đậm, một cái bảng, một danh sách đánh số, và câu chốt
// cuối cùng là văn trơn không dấu nhấn nào.
// ─────────────────────────────────────────────────────────────────────────────

/// Một báo cáo thật, rút gọn nhưng giữ nguyên hình dạng.
const REPORT: &str = r#"Tìm ra rồi — **không phải một tiến trình nào treo. Cả máy đang thrash swap.**

## Bằng chứng

| Chỉ số | Giá trị |
|---|---|
| RAM vật lý | **16 GB** |
| Swap đang dùng | **10.7 GB / 12 GB** (còn 1.5 GB) |

10.7 GB swap trên máy 16 GB nghĩa là mỗi lần bạn gõ phím, terminal phải **đọc trang bộ nhớ từ SSD về** mới xử lý được → nhìn y hệt "treo cứng", nhưng thực ra nó đang bò. Bấm `Ctrl+C` cũng vô ích vì không phải lệnh nào treo, và cái làm nó chậm không nằm trong tiến trình nào cả mà nằm ở chỗ bộ nhớ vật lý đã hết từ lâu.

```
- **pageins** 386 triệu
uptime 22 ngày
```

**Đề xuất dọn (chưa làm gì cả — chờ bạn duyệt):**

1. **Đóng Activity Monitor** → lấy lại ~61% CPU của `sysmond`.
2. **Kill 2 loop chết** (pid 20626, 38879) → chúng đang chờ file không bao giờ tới.

Nói "dọn đi" là mình chạy phần an toàn. Riêng VM thì mình để bạn quyết — mình không biết cái nào đang dở việc."#;

/// Câu CHỐT của một báo cáo nằm ở dòng cuối, và nó thường là văn trơn.
///
/// Bản đầu của `key_points` lấy tuần tự từ trên xuống nên dòng này luôn là dòng
/// rơi — tức bản rút gọn bỏ đi đúng cái nó sinh ra để mang đi. Đo trên ba báo
/// cáo thật ngày 2026-08-12, cả ba đều đóng bằng một câu văn trơn: *"Nói 'dọn
/// đi' là mình chạy phần an toàn…"* · *"Tôi nghiêng về (1) rồi tôi chạy tiếp"* ·
/// *"Hà mở lại phiên là tôi chạy nốt"*.
#[test]
fn the_closing_sentence_survives_the_cut() {
    let out = hub::watch::key_points(REPORT, 700);
    assert!(
        out.contains("mình để bạn quyết"),
        "câu chốt cuối bị cắt mất:\n{out}"
    );
}

/// Một đoạn văn dài KHÔNG được ăn hết trần.
///
/// Đoạn "10.7 GB swap trên máy 16 GB…" dài hơn 400 ký tự và lọt lưới vì có chữ
/// đậm. Bản đầu để nguyên nó ⟹ hết trần 700 ký tự ⟹ danh sách việc phải làm ở
/// dưới không bao giờ tới điện thoại.
#[test]
fn one_fat_paragraph_cannot_eat_the_whole_budget() {
    let out = hub::watch::key_points(REPORT, 700);
    assert!(
        out.contains("Đóng Activity Monitor"),
        "mục việc phải làm bị đoạn văn dài đẩy ra ngoài:\n{out}"
    );
    let longest = out.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    assert!(longest <= 181, "còn một dòng {longest} ký tự:\n{out}");
}

/// Hàng bảng phải thành chữ đọc được, không phải một hàng rào `|`.
#[test]
fn a_table_row_reads_as_a_sentence() {
    let out = hub::watch::key_points(REPORT, 700);
    assert!(out.contains("RAM vật lý · 16 GB"), "bảng chưa dọn:\n{out}");
    assert!(!out.contains('|'), "còn dấu bảng trong tin:\n{out}");
}

/// Cắt thì NÓI RA còn bao nhiêu dòng — và đếm từ bản GỐC.
///
/// Một bản rút gọn im lặng đọc như một bản đầy đủ, và người đọc sẽ quyết định
/// trên thứ họ tưởng là toàn bộ.
#[test]
fn what_is_hidden_is_counted_from_the_original() {
    let out = hub::watch::key_points(REPORT, 300);
    let note = out
        .lines()
        .last()
        .expect("tin rỗng")
        .to_string();
    assert!(note.starts_with("… (còn "), "không có câu nói phần bị giấu: {note}");
    let n: usize = note
        .trim_start_matches("… (còn ")
        .trim_end_matches(" dòng)")
        .parse()
        .unwrap_or_else(|_| panic!("không đọc được con số: {note}"));
    let total = REPORT.lines().filter(|l| !l.trim().is_empty()).count();
    let shown = out.lines().count() - 1 - usize::from(out.contains("⋯\n"));
    assert_eq!(n, total - shown, "con số nói dối:\n{out}");
}

/// Không giấu gì thì KHÔNG được dọa là có giấu.
#[test]
fn a_short_report_carries_no_warning_about_hidden_lines() {
    let short = "✅ Xong cả ba việc.\n\nHỏi tiếp gì không?";
    let out = hub::watch::key_points(short, 700);
    assert!(!out.contains("còn"), "dọa người đọc là còn phần chưa xem:\n{out}");
    assert!(out.contains("Xong cả ba việc"));
}

/// `10.7 GB…` mở đầu một câu KHÔNG phải mục đánh số.
///
/// Nhận nhầm "số rồi chấm" thành mục đánh số thì bản rút gọn đầy những mảnh câu
/// giữa bài, đúng thứ không quyết được gì.
#[test]
fn a_number_starting_a_sentence_is_not_a_list_item() {
    let text = "Mở đầu.\n\n10.7 GB swap là con số đáng ngại nhưng câu này chỉ là văn.\n\n1. Việc phải làm.\n\nCâu chốt.";
    let out = hub::watch::key_points(text, 700);
    assert!(out.contains("1. Việc phải làm"), "mất mục đánh số thật:\n{out}");
    assert!(
        !out.contains("10.7 GB swap"),
        "câu văn mở đầu bằng số bị đọc thành mục đánh số:\n{out}"
    );
}

/// Chữ trong rào mã không lên điện thoại — nhưng vẫn được ĐẾM.
///
/// Dòng trong rào cố tình mang **cả hai** dấu nhấn (`- ` và chữ đậm), nên nếu
/// nó vắng mặt thì chỉ có một lời giải thích: luật rào mã đang chạy. Bản đầu
/// của phép đo này dùng một dòng trần — thứ bị loại vì nhạt, không phải vì nằm
/// trong rào — nên nó xanh cả với mã hỏng.
#[test]
fn fenced_code_is_dropped_but_still_counted() {
    let out = hub::watch::key_points(REPORT, 700);
    assert!(!out.contains("pageins"), "chữ trong rào mã lọt ra:\n{out}");
}

/// Bản rút gọn không được dán liền hai mẩu cách xa nhau mà không nói.
#[test]
fn a_gap_in_the_middle_is_marked() {
    let out = hub::watch::key_points(REPORT, 300);
    assert!(out.contains('⋯'), "cắt giữa mà không có dấu đứt:\n{out}");
}

// ─────────────────────────────────────────────────────────────────────────────
// KHÔNG NHÌN THẤY ≠ ĐÃ TẮT
//
// 🔴 Đo từ chính log của hub, 2026-08-12 14:44:07 — ba dòng liền nhau:
//
// ```text
// claude_agents_list_failed acc1  "spawn claude failed: No such file or directory"
// claude_agents_list_failed acc2  …
// claude_agents_list_failed acc3  …
// session_change  "⏹ projects-71 · games (296972d4) đã tắt hẳn."
// session_change  "⏹ projects-b3 · AI/hub (37e59209) đã tắt (…)"
// session_change  "⏹ projects-d8 · AI/hub (69a38c64) đã tắt (…)"
// ```
//
// Ba tin trong 8 giây, cả ba phiên VẪN SỐNG: lúc 16:08 lệnh `/sessions` còn
// liệt kê `projects-d8 · đang chạy`, và nó làm việc tới 16:41. Thứ hỏng là
// `npm` đang ghi đè binary `claude` ngay lúc ấy — tức PHÉP ĐO, không phải máy.
// ─────────────────────────────────────────────────────────────────────────────

/// Tài khoản không liệt kê được phiên thì phiên của nó KHÔNG được coi là đã tắt.
#[test]
fn a_failed_listing_is_not_a_dead_session() {
    let prev = book(&[("69a38c64", IDLE)]);
    let (events, next) = changes(&prev, &[], NOW, &["acc1".to_string()]);
    assert!(events.is_empty(), "danh sách hỏng mà vẫn báo tắt: {events:?}");
    // …và SỔ PHẢI CÒN. Xoá sổ là lượt sau phiên quay lại thành "phiên mới", rồi
    // cái chết THẬT của nó bị báo thêm một lần nữa — đúng thứ đo được hôm nay
    // (`37e59209` báo 14:44 + 16:08; `69a38c64` báo 14:44 + 16:42).
    assert!(next.contains_key("69a38c64"), "sổ bị xoá trong lúc mù: {next:?}");
}

/// Sổ CŨ chưa ghi tài khoản thì cũng phải im khi có tài khoản mù.
///
/// Thà lỡ một tin còn hơn một tin sai: `a` được ghi lại ở mọi lượt nhìn thấy
/// phiên, nên ca này tự hết sau đúng một vòng lành lặn.
#[test]
fn an_old_book_entry_without_an_account_is_kept_while_blind() {
    let mut m = mark(IDLE, "ttys009", "interactive");
    m.a = String::new(); // sổ ghi trước 2026-08-12
    let prev: BTreeMap<String, Mark> = [("cu".to_string(), m)].into_iter().collect();
    let (events, next) = changes(&prev, &[], NOW, &["acc3".to_string()]);
    assert!(events.is_empty(), "sổ cũ mà vẫn báo tắt lúc mù: {events:?}");
    assert!(next.contains_key("cu"), "sổ cũ bị xoá trong lúc mù");
}

/// Một tài khoản hỏng KHÔNG được làm câm những tài khoản còn nhìn được.
///
/// Cửa mù hẹp đúng bằng chỗ hub không nhìn thấy — rộng hơn thì nó thành cái cớ
/// để im lặng, và một cái loa im lặng thì tệ hơn không có loa.
#[test]
fn a_blind_account_does_not_gag_the_others() {
    let mut m = mark(IDLE, "ttys009", "interactive");
    m.a = "acc1".into();
    let prev: BTreeMap<String, Mark> = [("cua-acc1".to_string(), m)].into_iter().collect();
    let (events, _) = changes(&prev, &[], NOW, &["acc2".to_string()]);
    assert_eq!(events.len(), 1, "acc2 hỏng mà phiên acc1 tắt lại không báo: {events:?}");
}

/// Và khi mọi tài khoản đều trả lời được, luật cũ giữ nguyên: vắng mặt = đã tắt.
#[test]
fn with_every_account_answering_a_missing_session_still_ends() {
    let prev = book(&[("da-tat", IDLE)]);
    let (events, next) = changes(&prev, &[], NOW, &[]);
    assert_eq!(events.len(), 1, "phiên biến mất mà không báo: {events:?}");
    assert!(!next.contains_key("da-tat"), "phiên đã tắt vẫn nằm lại trong sổ");
}

/// Lỗi API KHÔNG được đọc thành "đang chờ bạn".
///
/// 🔴 Hà 2026-08-12: *"vừa rồi báo lỗi api mà chưa thấy bắt được"*. Hai trạng
/// thái nhìn giống hệt nhau từ xa — nhật ký thôi lớn lên, màn đứng im — mà việc
/// phải làm ngược nhau: một bên chờ anh trả lời, một bên chờ anh biết là nó
/// hỏng.
#[test]
fn an_api_error_is_not_reported_as_waiting_for_you() {
    let c = Change::Finished {
        id: "s1".to_string(),
        name: "[amm] projects-fb".to_string(),
        ran_sec: 600,
    };
    let failed = c.say(
        &Idle::Failed {
            line: "API Error: Server is temporarily limiting requests (not your usage limit) · Rate limited".to_string(),
        },
        None,
    );
    assert!(failed.contains("LỖI API"), "{failed}");
    assert!(failed.contains("Rate limited"), "phải mang nguyên câu lỗi: {failed}");
    assert!(!failed.contains("đang chờ bạn"), "vẫn đọc thành chờ người: {failed}");

    // Và dấu nhắc thật thì vẫn nói như cũ — luật mới không siết lan.
    let ok = c.say(&Idle::Prompt, None);
    assert!(ok.contains("đang chờ bạn"), "{ok}");
}

/// Phiên dừng vì LỖI phải kêu — và kêu khác hẳn "vừa xong".
///
/// 🔴 Hà 2026-08-13: *"cần lệnh kiểm các phiên đã xử lý xong và đang dừng, hoặc
/// tìm phiên đang dừng do lỗi"* · *"vì lỗi chưa thấy cảnh báo gì"*. Trước đó lỗi
/// chỉ được nhận ra nếu hub tình cờ ĐỌC MÀN đúng lúc phiên vừa im; lỡ nhịp thì
/// phiên nằm im và nhìn y hệt một phiên đã xong việc.
#[test]
fn a_session_stopped_by_an_error_rings_and_says_the_error() {
    let mut s = sess("s1", "projects-06", "terminal", false);
    s.folder = "AI/hub".into();
    s.error = Some("API Error: 500 internal".into());
    let prev = book(&[("s1", WORKING)]);
    let (events, next) = changes(&prev, &[s], NOW, &[]);
    let said = match events.first() {
        Some(c @ Change::Failed { .. }) => c.say(&Idle::Unknown, None),
        other => panic!("phải là Failed: {other:?}"),
    };
    assert!(said.contains("LỖI"), "{said}");
    assert!(said.contains("API Error: 500"), "phải mang nguyên dòng lỗi: {said}");
    assert!(!said.contains("vừa chạy xong"), "{said}");
    // …và NÓI MỘT LẦN: lượt sau cùng trạng thái thì im.
    let (again, _) = changes(&next, &[{
        let mut s = sess("s1", "projects-06", "terminal", false);
        s.folder = "AI/hub".into();
        s.error = Some("API Error: 500 internal".into());
        s
    }], NOW + 30, &[]);
    assert!(again.is_empty(), "kêu lại lần hai: {again:?}");
}
