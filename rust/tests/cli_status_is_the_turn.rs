//! Lượt khép lúc nào là chuyện CLI KHAI, không phải chuyện huba ĐOÁN.
//!
//! 🔴 Hà 2026-10-03, kèm ảnh `/shot` của `[dwork/dhub]` ghi *"done 3:57 PM · 1
//! shell still running"*: *"việc nhận biết phiên đang dừng chờ để gửi thông báo
//! lên tele không ổn định lúc thì dồn dập lúc thì xong lâu rồi ko thấy báo"*.
//!
//! Đo được cả hai vế, cùng một gốc — huba đoán "đang chạy" từ ba nguồn dễ hỏng
//! (đọc MÀN, shell con, nhật ký im ≥ 180 giây) trong khi sổ `sessions/<pid>.json`
//! của CLI ghi thẳng `status` + `statusUpdatedAt` đúng lúc lượt đổi:
//!
//! * **dồn dập** — 02/10 15:39:21Z lượt `osascript` hỏng ⟹ mọi phiên có lệnh
//!   nền bị lật sang "đang chạy" ⟹ vòng 15:43:12Z lật cả loạt về "dừng": 5 tin 💤
//!   trong một phút, 4 tin *"sau 3 phút chạy"* về những phiên đứng im từ trước
//!   (`23eb8a98` khép lượt 15:32:49, không ghi gì tới 15:44).
//! * **không báo** — `e880f072` khép lượt 08:48:56 (03/10), đứng chờ tới
//!   08:53:44, không một tin: vòng 08:52:33 không đọc được màn ⟹ giữ "đang chạy".
//! * **mất hẳn** — sổ `16592.json` thừa một byte `}` ⟹ phiên sống 16 giờ rơi
//!   khỏi danh sách, không bao giờ được báo.

use std::collections::BTreeMap;

use huba::sessions::{
    book_status_fingerprint, cli_turn_known, cli_turn_started, is_working, list_account_books,
    parse_session_book, turn_verdict, LiveSession, TurnVerdict,
};
use huba::watch::{changes, Change, Mark, IDLE, MIN_RUN_SEC};

/// Tệp thật, chép nguyên văn ngày 2026-10-03 từ `~/.claude-acc6/sessions/16592.json`
/// — 530 byte, 529 byte đầu là JSON trọn vẹn, byte cuối là `}` thừa.
const BOOK_WITH_TRAILING: &str = r#"{"pid":16592,"sessionId":"7347daca-a0f6-441c-9566-e117afdc5d35","cwd":"/Users/hanguyen/projects","startedAt":1790958146595,"procStart":"Fri Oct  2 16:22:25 2026","version":"2.1.280","peerProtocol":1,"peerFeatures":["notify_idle","reply_across_default_dirs","artifact_yield"],"kind":"interactive","entrypoint":"cli","pidDomain":"darwin","messagingSocketPath":"/tmp/cc-socks/16592.sock","name":"projects-f8","nameSource":"derived","nameSince":1790958146596,"status":"idle","updatedAt":1790965788954,"statusUpdatedAt":1790965788954}}"#;

const NOW: i64 = 1_800_000_000;

#[test]
fn the_fixture_is_the_real_broken_file() {
    // Đối chứng: tệp này ĐÚNG là thứ làm đường đọc cũ gãy — nếu không thì mọi
    // bài bên dưới xanh vì một lẽ khác.
    assert_eq!(BOOK_WITH_TRAILING.len(), 530);
    let old = serde_json::from_str::<serde_json::Value>(BOOK_WITH_TRAILING);
    assert!(
        old.unwrap_err().to_string().contains("trailing characters"),
        "đường đọc cũ phải gãy đúng câu đã đo trong log"
    );
}

#[test]
fn a_book_with_a_trailing_byte_still_reads() {
    let (v, du) = parse_session_book(BOOK_WITH_TRAILING).expect("phải đọc được");
    assert_eq!(du, 1, "phải nói ra đúng một byte thừa, không giấu");
    assert_eq!(
        v["sessionId"].as_str(),
        Some("7347daca-a0f6-441c-9566-e117afdc5d35")
    );
    assert_eq!(v["status"].as_str(), Some("idle"));

    let clean = &BOOK_WITH_TRAILING[..529];
    assert_eq!(parse_session_book(clean).unwrap().1, 0);
    assert_eq!(parse_session_book(&format!("{clean}\n")).unwrap().1, 0);
    // Hỏng thật thì vẫn là hỏng — chỉ nới đúng ca đuôi thừa.
    assert!(parse_session_book("").is_err());
    assert!(parse_session_book(r#"{"pid":16592,"sessionId":"#).is_err());
    assert!(parse_session_book("không phải json").is_err());
}

#[test]
fn a_live_session_with_a_trailing_byte_is_still_listed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("sessions")).unwrap();
    std::fs::write(dir.path().join("sessions/16592.json"), BOOK_WITH_TRAILING).unwrap();
    let rows = list_account_books(dir.path()).unwrap();
    assert_eq!(rows.len(), 1, "phiên sống bị rơi khỏi danh sách: {rows:?}");
    assert_eq!(
        rows[0]["sessionId"].as_str(),
        Some("7347daca-a0f6-441c-9566-e117afdc5d35")
    );
}

/// Ca dồn dập + ca không báo: phiên CLI khai `shell` (xong lượt, còn lệnh nền),
/// có shell con, và vòng này KHÔNG đọc được màn.
#[test]
fn a_blind_screen_cannot_overrule_the_cli() {
    let v = turn_verdict(Some("shell"), 0, Some(200), true, None, true);
    assert_eq!(
        v,
        TurnVerdict {
            working: false,
            bg_shell: true
        },
        "mù màn mà vẫn lật sang 'đang chạy' — đúng cái sinh 5 tin 💤 một phút"
    );
    // Màn đọc được và nói "đang chạy" cũng không cãi được `shell`/`idle`.
    assert!(!turn_verdict(Some("shell"), 0, Some(200), true, Some(true), true).working);
    assert!(!turn_verdict(Some("idle"), 0, Some(200), true, Some(true), true).working);
    // `idle` + shell con ⟹ lệnh nền.
    assert!(turn_verdict(Some("idle"), 0, Some(200), true, None, true).bg_shell);
    // `busy` thì đang chạy, dù màn nói gì và dù không có shell nào.
    assert_eq!(
        turn_verdict(Some("busy"), 0, Some(9_999), true, Some(false), true),
        TurnVerdict {
            working: true,
            bg_shell: false
        }
    );
    assert!(turn_verdict(Some("busy"), 0, None, false, None, true).working);
    // Phiên chết thì không gì cả.
    assert_eq!(
        turn_verdict(Some("busy"), 0, None, true, None, false),
        TurnVerdict {
            working: false,
            bg_shell: false
        }
    );
}

/// CLI không khai (hoặc khai một chữ chưa đo) ⟹ đường cũ còn nguyên, đủ ba ngả.
#[test]
fn without_a_cli_status_the_old_evidence_still_decides() {
    for st in [None, Some("chua-do-bao-gio")] {
        assert!(!cli_turn_known(st));
        // shell con + mù màn ⟹ giữ bằng chứng cũ: đang chạy (luật 11b).
        assert!(turn_verdict(st, 0, Some(9_999), true, None, true).working);
        // shell con + màn nói đang chờ ⟹ lệnh nền.
        assert_eq!(
            turn_verdict(st, 0, Some(9_999), true, Some(false), true),
            TurnVerdict {
                working: false,
                bg_shell: true
            }
        );
        // không shell: nhật ký quyết.
        assert!(turn_verdict(st, 0, Some(60), false, None, true).working);
        assert!(!turn_verdict(st, 0, Some(9_999), false, None, true).working);
    }
    for st in ["busy", "idle", "shell", "done"] {
        assert!(cli_turn_known(Some(st)), "{st}");
    }
}

/// Nhật ký vừa ghi KHÔNG lật được `idle`/`shell` nữa: CLI ghi `turn_duration`
/// đúng lúc khép lượt, nên vòng chạy ngay sau mốc ấy luôn thấy nhật ký "vừa
/// ghi" — chính cái cửa cũ sẽ nuốt cú đánh thức.
#[test]
fn a_journal_written_at_turn_end_does_not_keep_it_running() {
    assert!(!is_working(Some("idle"), 0, Some(0)));
    assert!(!is_working(Some("shell"), 0, Some(1)));
    // Subagent nền vẫn là bằng chứng mạnh nhất.
    assert!(is_working(Some("shell"), 2, Some(9_999)));
}

fn live(id: &str, status: &str, status_at_ms: i64, working: bool) -> LiveSession {
    LiveSession {
        session_id: id.to_string(),
        name: "projects-fb".to_string(),
        host: "terminal".to_string(),
        account: "acc7".to_string(),
        status: Some(status.to_string()),
        status_at_ms,
        working,
        ..Default::default()
    }
}

#[test]
fn the_turn_start_comes_from_the_cli_not_from_the_first_cycle_that_saw_it() {
    let s = live("e880f072", "busy", (NOW - 584) * 1000, true);
    assert_eq!(cli_turn_started(&s, NOW), Some(NOW - 584));
    // Đồng hồ lệch (mốc ở tương lai) ⟹ không quá "bây giờ".
    let fut = live("x", "busy", (NOW + 50) * 1000, true);
    assert_eq!(cli_turn_started(&fut, NOW), Some(NOW));
    // Không `busy`, hoặc không có mốc ⟹ không có gì để dùng.
    assert_eq!(
        cli_turn_started(&live("x", "idle", NOW * 1000, false), NOW),
        None
    );
    assert_eq!(cli_turn_started(&live("x", "busy", 0, true), NOW), None);
}

/// Lượt thật dài 3 phút 20 giây, mà vòng đầu tiên thấy nó chạy lại tới SAU khi
/// nó đã chạy 3 phút (vòng 120 giây). Mốc cũ = "lúc vòng thấy" ⟹ đo ra 5 giây
/// ⟹ dưới `MIN_RUN_SEC` ⟹ im. Mốc CLI ⟹ đo đúng ⟹ có tin.
#[test]
fn a_long_turn_seen_late_is_still_announced() {
    let mut m = Mark {
        s: IDLE.to_string(),
        f: NOW - 3600,
        a: "acc7".to_string(),
        o: "terminal".to_string(),
        ..Default::default()
    };
    m.i = 18111;
    let prev: BTreeMap<String, Mark> = [("e880f072".to_string(), m)].into_iter().collect();

    let started = NOW - 195;
    let (ev1, mid) = changes(
        &prev,
        &[live("e880f072", "busy", started * 1000, true)],
        NOW,
        &[],
    );
    assert!(ev1.is_empty(), "bắt đầu chạy không phải tin: {ev1:?}");
    assert_eq!(mid["e880f072"].s, format!("working@{started}"));

    let (ev2, _) = changes(
        &mid,
        &[live("e880f072", "idle", (NOW + 5) * 1000, false)],
        NOW + 5,
        &[],
    );
    match ev2.as_slice() {
        [Change::Finished { ran_sec, .. }] => {
            assert_eq!(*ran_sec, 200);
            assert!(*ran_sec >= MIN_RUN_SEC);
        }
        other => panic!("lượt 200 giây phải được báo, nhận: {other:?}"),
    }
}

#[test]
fn the_wake_up_fingerprint_moves_when_a_turn_ends() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("sessions")).unwrap();
    let cfg = huba::config::Config {
        claude_accounts: vec![huba::config::ClaudeAccountCfg {
            name: "acc7".into(),
            config_dir: Some(dir.path().display().to_string()),
            launch: None,
            locked: false,
        }],
        ..Default::default()
    };
    let book = dir.path().join("sessions/18111.json");
    let write = |status: &str, at: i64| {
        std::fs::write(
            &book,
            format!(
                r#"{{"pid":18111,"sessionId":"e880f072","status":"{status}","statusUpdatedAt":{at}}}"#
            ),
        )
        .unwrap()
    };

    write("busy", 1_791_018_203_161);
    let a = book_status_fingerprint(&cfg);
    assert!(a.contains("e880f072:busy:"), "{a}");
    assert_eq!(book_status_fingerprint(&cfg), a, "không đổi gì ⟹ không kêu");

    write("shell", 1_791_018_787_959);
    let b = book_status_fingerprint(&cfg);
    assert_ne!(a, b, "lượt khép phải đánh thức vòng");

    // Cùng chữ `busy`, MỐC MỚI: lượt cũ khép rồi lượt mới bắt đầu gọn giữa hai
    // lần dò (3 giây) — chữ không đổi nên chỉ mốc nói được. Đối chứng ngược
    // 03/10: bản đầu của bài này so `shell`→`busy` (chữ đổi), nên bỏ hẳn mốc
    // khỏi vân tay mà nó vẫn XANH.
    write("busy", 1_791_018_203_161);
    let c1 = book_status_fingerprint(&cfg);
    write("busy", 1_791_018_810_528);
    let c = book_status_fingerprint(&cfg);
    assert_ne!(c1, c, "lượt mới cùng chữ `busy` mà vân tay đứng yên");
    assert_ne!(b, c);

    // Tệp thừa đuôi vẫn được đọc, không rơi về dấu `?`.
    std::fs::write(&book, BOOK_WITH_TRAILING).unwrap();
    let d = book_status_fingerprint(&cfg);
    assert!(
        d.starts_with("7347daca-a0f6-441c-9566-e117afdc5d35:idle:"),
        "{d}"
    );
}
