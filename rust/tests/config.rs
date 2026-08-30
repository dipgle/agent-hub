// Test setup builds a Config by starting from the default and setting the one or
// two fields under test. Clippy prefers struct-update syntax; here the mutation
// form is the clearer statement of "everything default EXCEPT this".
#![allow(clippy::field_reassign_with_default)]

use huba::config::{self, CallCfg, Config};

fn write_config(json: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("huba.config.json");
    std::fs::write(&file, json).unwrap();
    (dir, file)
}

#[test]
fn defaults_are_bounded_and_dial_nothing_on_their_own() {
    let d = Config::default();
    // 🔴 Hai dòng "chưa nối kênh nào" (`adapters.tfl5.enabled`,
    // `trust.tfl5_user_tids`) đi cùng tfl5 ngày 2026-08-14. Câu hỏi ấy nay
    // không trả lời được từ tệp cấu hình: kênh duy nhất là Telegram, và nó bật
    // hay tắt là do có KHOÁ trong `huba.env` hay không — một bí mật, cố ý không
    // nằm trong tệp này. Thứ còn kiểm được ở đây là tên biến, không phải giá trị.
    assert_eq!(d.confirm.bot_token_env, "HUB_TELEGRAM_BOT_TOKEN");
    assert_eq!(d.confirm.chat_id_env, "HUB_TELEGRAM_CHAT_ID");
    // And one call has a stop on it, in money and in time.
    assert!(d.call.max_budget_usd > 0.0 && d.call.max_budget_usd <= 5.0);
    assert!(d.call.timeout_sec >= 10 && d.call.timeout_sec <= 3600);
}

#[test]
fn config_file_overrides_merge_deeply_and_paths_become_absolute() {
    let (_dir, file) =
        write_config(r#"{ "confirm": { "timeout_sec": 45 }, "call": { "timeout_sec": 300 } }"#);
    let cfg = config::load(Some(&file)).unwrap();

    assert_eq!(cfg.call.timeout_sec, 300);
    assert_eq!(cfg.confirm.timeout_sec, 45);
    // untouched sibling keys survive the merge
    assert_eq!(cfg.confirm.bot_token_env, "HUB_TELEGRAM_BOT_TOKEN");
    assert!(cfg.confirm.enabled, "anh em cùng bảng phải giữ mặc định");
    assert_eq!(cfg.call.max_budget_usd, CallCfg::default().max_budget_usd);
    assert!(cfg.db.is_absolute() && cfg.log_file.is_absolute());
    assert!(cfg.workspace_root.is_absolute());
}

/// A config file left over from the inbox era must still LOAD. Every key that
/// went away on 2026-08-08 (`triage`, `act`, `autonomy`, `routing`,
/// `daily_budget_usd`, `max_triage_per_cycle`, `web`, `leak_patterns`) is
/// simply unknown to serde now — a huba that refused to start because of a stale
/// key would be a huba that cannot be upgraded without hand-editing json first.
///
/// 🔴 `adapters` và `trust` vào cùng danh sách ấy ngày 2026-08-14, và lượt này
/// KHÔNG phải giả thiết: `huba.config.json` thật trên máy đang mang cả hai, với
/// một `app_tid` thật và hai `user_tid` thật. Nếu chúng làm huba từ chối khởi
/// động thì chủ máy mất kênh duy nhất của mình vì một tệp cũ.
#[test]
fn a_config_from_the_inbox_era_still_loads_and_its_dead_keys_are_ignored() {
    let (_dir, file) = write_config(
        r#"{
            "adapters": { "tfl5": { "enabled": true, "room": "huba", "app_tid": "a-1234" } },
            "trust": { "tfl5_user_tids": ["u-owner"], "trusted_sources": ["cli"] },
            "triage": { "model": "sonnet", "max_budget_usd": 0.5 },
            "act": { "enabled": true },
            "autonomy": { "default": "L2" },
            "routing": [{ "when": { "repo": "x/y" }, "project": "y" }],
            "daily_budget_usd": 3.0,
            "max_triage_per_cycle": 6,
            "web": { "enabled": true, "port": 9200 },
            "leak_patterns": ["secret"]
        }"#,
    );
    let cfg = config::load(Some(&file)).unwrap();
    // The dead `triage.max_budget_usd` must NOT quietly become the live one.
    assert_eq!(cfg.call.max_budget_usd, CallCfg::default().max_budget_usd);
    let text = serde_json::to_string(&cfg).unwrap();
    for gone in [
        "triage",
        "\"act\"",
        "autonomy",
        "routing",
        "daily_budget_usd",
        "web",
        "adapters",
        "trust",
        "tfl5",
    ] {
        assert!(
            !text.contains(gone),
            "saving the config again must drop `{gone}`, not carry it forward"
        );
    }
}

#[test]
fn a_malformed_config_file_fails_fast_instead_of_running_with_defaults() {
    let (_dir, file) = write_config("{ not json");
    let err = config::load(Some(&file)).unwrap_err().to_string();
    assert!(err.contains("cannot parse config"), "{err}");
}

#[test]
fn an_invalid_config_file_is_rejected_at_load_time() {
    let (_dir, file) = write_config(r#"{ "poll_interval_sec": 1 }"#);
    let err = config::load(Some(&file)).unwrap_err().to_string();
    assert!(err.contains("invalid huba config"), "{err}");
}

// 🔴 `a_room_with_no_owner_and_no_app_is_refused_at_startup` đã bỏ 2026-08-14.
// Nó canh hai luật của PHÒNG CHAT ("bật kênh thì phải có `app_tid`", "bật kênh
// thì danh sách chủ không được rỗng"), và cả hai đi cùng cái phòng.
//
// Câu hỏi tương đương của Telegram — "có khoá chưa" — cố ý KHÔNG trả lời được
// ở đây: khoá là bí mật trong `huba.env`, không phải một trường trong tệp cấu
// hình, nên `validate()` không nhìn thấy nó và không được giả vờ nhìn thấy.
// Chỗ canh đúng là `telegram::Inbox::start`: thiếu khoá thì không dựng luồng và
// NÓI RA (luật #4 — thiếu bí mật là SKIP-WITH-LOG, không phải chết máy).

#[test]
fn a_call_with_no_ceiling_is_refused() {
    let mut zero = Config::default();
    zero.call.max_budget_usd = 0.0;
    let err = config::validate(&zero).unwrap_err().to_string();
    assert!(err.contains("call.max_budget_usd"), "{err}");
}

#[test]
fn secrets_come_from_the_environment_never_the_config_file() {
    std::env::remove_var("HUB_TEST_SECRET");
    assert_eq!(config::secret_from_env("HUB_TEST_SECRET"), None);
    std::env::set_var("HUB_TEST_SECRET", "  abc  ");
    assert_eq!(
        config::secret_from_env("HUB_TEST_SECRET").as_deref(),
        Some("abc")
    );
    std::env::set_var("HUB_TEST_SECRET", "   ");
    assert_eq!(
        config::secret_from_env("HUB_TEST_SECRET"),
        None,
        "blank must count as absent so the adapter skips"
    );
    std::env::remove_var("HUB_TEST_SECRET");

    // The serialized default config must never carry a credential-looking value.
    let text = serde_json::to_string(&Config::default()).unwrap();
    assert!(
        !text.contains("sk-"),
        "no credential literal may appear in config"
    );
    assert!(!text.contains("gho_"));
    // 🔴 Tên khoá tfl5 đi cùng kênh 2026-08-14; hai tên còn lại là của Telegram,
    // và vẫn chỉ được có TÊN trong tệp này.
    assert!(
        text.contains("HUB_TELEGRAM_BOT_TOKEN") && text.contains("HUB_TELEGRAM_CHAT_ID"),
        "only the env var NAME belongs in config"
    );
    assert!(
        !text.contains("HUB_TFL5"),
        "tên khoá của một kênh đã gỡ không được mọc lại trong cấu hình"
    );
    assert!(
        !text.contains("password\":\"") || text.contains("password_env"),
        "a password VALUE must never be serialized"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// `/accounts` — câu trả lời cho "chưa có lệnh xem danh sách acc" (Hà 2026-08-12)
//
// Và cho câu ngay sau đó: *"vậy lệnh new chọn acc kiểu gì? hay đang để
// random?"*. KHÔNG random: `/new` không mang `@acc` thì `account = None` ⟹
// `terminal_command` không đặt `CLAUDE_CONFIG_DIR` ⟹ luôn chạy bằng tài khoản
// mặc định. Màn phải nói ra điều đó, vì hậu quả (tuần cạn hạn mức) chỉ lộ về sau.
// ─────────────────────────────────────────────────────────────────────────────

fn acc_cfg() -> huba::config::Config {
    let mut c = huba::config::Config::default();
    c.claude_accounts = vec![
        huba::config::ClaudeAccountCfg {
            name: "acc1".into(),
            config_dir: None,
            launch: Some("claude".into()),
        },
        huba::config::ClaudeAccountCfg {
            name: "acc2".into(),
            config_dir: Some("~/.claude-acc2".into()),
            launch: Some("claude2".into()),
        },
        huba::config::ClaudeAccountCfg {
            name: "acc3".into(),
            config_dir: Some("~/.claude-acc3".into()),
            launch: Some("claude3".into()),
        },
    ];
    c
}

fn snap_with(sessions: Vec<huba::sessions::LiveSession>) -> huba::sessions::SessionsSnapshot {
    huba::sessions::SessionsSnapshot {
        sessions,
        ..Default::default()
    }
}

fn row(account: &str, name: &str) -> huba::sessions::LiveSession {
    huba::sessions::LiveSession {
        account: account.to_string(),
        name: name.to_string(),
        session_id: format!("{account}-{name}"),
        host: "terminal".to_string(),
        ..Default::default()
    }
}

#[test]
fn the_accounts_list_names_the_one_that_new_lands_on() {
    let live = snap_with(vec![row("acc1", "projects-7c")]);
    let said = huba::runtime::accounts_text(&acc_cfg(), &live, &serde_json::json!({}), &[]);
    let line = said
        .lines()
        .find(|l| l.starts_with("acc1"))
        .unwrap_or_else(|| panic!("thiếu dòng acc1:\n{said}"));
    assert!(
        line.contains("mặc định"),
        "không nói acc nào là mặc định: {line}"
    );
    assert!(
        !said
            .lines()
            .find(|l| l.starts_with("acc2"))
            .unwrap()
            .contains("mặc định"),
        "acc có config_dir riêng không phải mặc định:\n{said}"
    );
    assert!(
        said.contains("projects-7c"),
        "không nói phiên nào của acc nào:\n{said}"
    );
}

/// Tài khoản KHÔNG liệt kê được phiên thì "0 phiên" là con số của một phép đo
/// hỏng — phải nói thẳng, đúng bài học 14:44 hôm nay.
#[test]
fn a_blind_account_says_its_zero_is_not_trustworthy() {
    let mut live = snap_with(vec![]);
    live.blind.push("acc2".into());
    live.notes
        .push("acc2: spawn claude failed: No such file or directory".into());
    let said = huba::runtime::accounts_text(&acc_cfg(), &live, &serde_json::json!({}), &[]);
    assert!(
        said.contains("KHÔNG liệt kê được"),
        "không cảnh báo tài khoản mù:\n{said}"
    );
    assert!(
        said.contains("spawn claude failed"),
        "không nói lý do:\n{said}"
    );
}

/// "Chưa đo xong" KHÁC "đã đo và bằng 0" — một con số bịa trông y hệt số thật.
#[test]
fn usage_still_being_measured_says_so_instead_of_showing_zero() {
    let live = snap_with(vec![]);
    let said = huba::runtime::accounts_text(
        &acc_cfg(),
        &live,
        &serde_json::json!({ "pending": true }),
        &[],
    );
    assert!(said.contains("đang đo"), "phải nói đang đo:\n{said}");
    assert!(!said.contains("0%"), "không được bịa 0%:\n{said}");
}

#[test]
fn measured_quota_reaches_the_line() {
    let live = snap_with(vec![]);
    let said = huba::runtime::accounts_text(
        &acc_cfg(),
        &live,
        &serde_json::json!({ "accounts": { "acc1": { "week_pct": 98, "session_pct": 6 } } }),
        &[],
    );
    assert!(said.contains("tuần 98%"), "thiếu hạn mức tuần:\n{said}");
    assert!(said.contains("phiên 6%"), "thiếu hạn mức phiên:\n{said}");
}

/// Số hạn mức đọc từ SỔ của chính CLI phải tới được dòng, kèm TUỔI của nó.
///
/// 🔴 Hà 2026-08-30: *"mở phiên mới ở acc khác chưa kiểm soát được acc đó có
/// đang còn nhiều tokens nhất không"*. `/accounts` là chỗ DUY NHẤT anh soi lại
/// được luật chọn tài khoản, nên nó phải in cả cái để soi — và tuổi bản đọc là
/// một nửa của cái ấy: acc1 trên máy này đang mang một con số già hai ngày.
#[test]
fn quota_from_the_cli_book_reaches_the_line() {
    let live = snap_with(vec![]);
    // Mốc giả 2026-08-30T15:00:00Z; bản đọc trước đó 2 tiếng.
    let now = 1_788_102_000_000i64;
    let q = huba::quota::Quota {
        account: "acc1".into(),
        week_pct: Some(22),
        week_resets_at: Some("2026-09-02T10:59:59+00:00".into()),
        hour5_pct: Some(0),
        hour5_resets_at: None,
        fetched_at_ms: Some(now - 2 * 60 * 60 * 1000),
        why_unknown: None,
    };
    let said = huba::runtime::accounts_text(&acc_cfg(), &live, &serde_json::json!({}), &[q]);
    assert!(said.contains("tuần 22%"), "thiếu số tuần:\n{said}");
    assert!(
        said.contains("đã dùng 22%"),
        "thiếu HẠNG — số trần không nói được ai rộng cửa hơn:\n{said}"
    );
    assert!(said.contains("tiếng trước"), "thiếu TUỔI bản đọc:\n{said}");
}

/// 🔴 ĐỐI CHỨNG NGƯỢC của ranh giới "dựng chữ ≠ đi đo" — bài kiểm này ĐỎ nếu
/// `accounts_text` lại tự đi đọc `$HOME`.
///
/// Đây là chỗ tôi vừa phá trong chính ngày 30/08: cho hàm dựng chữ tự gọi
/// `quota::read` làm nó phụ thuộc máy đang chạy, và
/// `usage_still_being_measured_says_so_instead_of_showing_zero` đỏ vì câu in ra
/// mang `5 tiếng 0%` thật của acc2. Không truyền bản đọc nào vào thì dòng hạn
/// mức KHÔNG được xuất hiện — im khác hẳn với bịa một câu "chưa đo được" mà
/// chính chỗ gọi chưa hề đi đo.
#[test]
fn accounts_text_khong_tu_di_doc_o_dia() {
    let live = snap_with(vec![]);
    let said = huba::runtime::accounts_text(&acc_cfg(), &live, &serde_json::json!({}), &[]);
    assert!(
        !said.contains("hạn mức:"),
        "không đưa bản đọc nào vào mà vẫn có dòng hạn mức ⟹ hàm đang tự đọc đĩa:\n{said}"
    );
    // …và có bản đọc thì dòng ấy PHẢI hiện — không thì bài trên xanh nhờ hàm
    // không bao giờ in gì, tức nó không đo cái gì cả.
    let q = huba::quota::Quota {
        account: "acc1".into(),
        week_pct: Some(5),
        week_resets_at: Some("2026-09-02T10:59:59+00:00".into()),
        hour5_pct: Some(0),
        hour5_resets_at: None,
        fetched_at_ms: None,
        why_unknown: None,
    };
    let said = huba::runtime::accounts_text(&acc_cfg(), &live, &serde_json::json!({}), &[q]);
    assert!(said.contains("hạn mức:"), "có bản đọc mà không in:\n{said}");
}

// ─────────────────────────────────────────────────────────────────────────────
// LỐI GÕ CÓ CỜ — Hà 2026-08-12: *"kiến trúc lại lệnh cho hợp lý, ví dụ:
// `/new -a acc2 -s dwork`"*.
//
// Lối cũ là VỊ TRÍ (`/new <dự án> @acc <việc>`): thứ tự phải thuộc lòng, và
// `@acc` sai khe thì nó lặng lẽ thành một phần đề bài. Cờ gõ đâu cũng được và
// tự nói nó là gì — nhưng chỉ cờ ĐÃ BIẾT mới được bóc ra khỏi đề bài.
// ─────────────────────────────────────────────────────────────────────────────

const NEW_FLAGS: &[&str] = &["a", "acc", "account", "s", "p", "project", "duan", "du-an"];

#[test]
fn flags_are_read_wherever_they_sit() {
    let (f, rest) = huba::pipeline::split_flags("-a acc2 -s dwork sửa lịch làm việc", NEW_FLAGS);
    assert_eq!(f.get("a").map(String::as_str), Some("acc2"));
    assert_eq!(f.get("s").map(String::as_str), Some("dwork"));
    assert_eq!(rest, "sửa lịch làm việc");

    // …kể cả khi đứng SAU đề bài.
    let (f2, rest2) = huba::pipeline::split_flags("sửa lịch làm việc -s dwork -a acc2", NEW_FLAGS);
    assert_eq!(f2.get("s").map(String::as_str), Some("dwork"));
    assert_eq!(rest2, "sửa lịch làm việc");
}

/// Đúng ví dụ Hà gõ: không có đề bài, chỉ mở phiên.
#[test]
fn the_example_from_the_owner_parses_to_account_project_and_no_task() {
    let (f, rest) = huba::pipeline::split_flags("-a acc2 -s dwork", NEW_FLAGS);
    assert_eq!(f.get("a").map(String::as_str), Some("acc2"));
    assert_eq!(f.get("s").map(String::as_str), Some("dwork"));
    assert!(
        rest.is_empty(),
        "không có đề bài mà lại sinh ra chữ: {rest}"
    );
}

/// 🔴 Cờ LẠ phải ở nguyên trong đề bài.
///
/// Nuốt im lặng một mẩu đề bài là loại lỗi không truy ra được: phiên vẫn mở,
/// vẫn chạy — chỉ là chạy một đề bài khác với đề bài đã gõ.
#[test]
fn unknown_flags_stay_in_the_text_body() {
    let (f, rest) = huba::pipeline::split_flags("-s huba sửa cờ -x của script", NEW_FLAGS);
    assert_eq!(f.get("s").map(String::as_str), Some("huba"));
    assert_eq!(rest, "sửa cờ -x của script");
}

/// Cờ bỏ trống (`-a` rồi tới cờ khác) không được nuốt cờ kế tiếp.
#[test]
fn an_empty_flag_does_not_eat_the_next_one() {
    let (f, _) = huba::pipeline::split_flags("-a -s dwork", NEW_FLAGS);
    assert_eq!(
        f.get("a").map(String::as_str),
        Some(""),
        "cờ trống phải rỗng, không nuốt -s"
    );
    assert_eq!(f.get("s").map(String::as_str), Some("dwork"));
}

/// Không cờ nào thì phải y hệt lối gõ cũ — lối ấy vẫn nằm trong tay quen của
/// chủ máy và trong các nút Telegram đã gửi đi.
#[test]
fn the_old_positional_form_is_untouched() {
    let (f, rest) = huba::pipeline::split_flags("dwork @acc2 sửa lịch", NEW_FLAGS);
    assert!(f.is_empty(), "không có cờ nào mà lại bóc ra: {f:?}");
    assert_eq!(rest, "dwork @acc2 sửa lịch");
}

/// `/new` mở cửa sổ bằng ĐÚNG TỪ chủ máy gõ, và **không nhét đề bài vào argv**.
///
/// 🔴 Hà 2026-08-15 chốt năm bước: *"mở terminal mới → chèn vào lệnh `claude3`
/// → kiểm tra xem có vướng gì không … để vào được chỗ chờ gõ text → chuyển chế
/// độ auto mode on → gõ vào chuỗi `tiếp dwork`"*, và *"tôi có 3 tài khoản và
/// trên terminal tôi gõ `claude` `claude2` `claude3`"*.
///
/// Cái giá của lối cũ đo được cùng ngày: `/new acc3 dwork` ra
/// `claude --permission-mode auto '[] acc3 dwork' …` — mọi thứ đứng sai chỗ
/// đều hoá thành **một phần đề bài**, im lặng, tới tận argv.
#[test]
fn a_new_window_opens_with_the_word_the_owner_would_type_and_no_task_in_argv() {
    let cfg = acc_cfg();
    assert_eq!(
        huba::sessions::account_launch(&cfg, Some("acc3")),
        "claude3"
    );
    assert_eq!(huba::sessions::account_launch(&cfg, Some("acc1")), "claude");

    let cmd = huba::sessions::terminal_command(
        &huba::sessions::account_launch(&cfg, Some("acc3")),
        std::path::Path::new("/Users/x/projects"),
        None,
    );
    assert!(cmd.contains("&& claude3 --permission-mode auto"), "{cmd}");
    // Đề bài KHÔNG có mặt — cả dạng rỗng lẫn dạng chữ. `claude ''` là "một đề
    // bài rỗng", khác hẳn `claude` là "chưa có đề bài".
    assert!(
        !cmd.contains("''"),
        "đề bài rỗng vẫn được truyền vào: {cmd}"
    );
    assert!(
        cmd.contains("--permission-mode auto --disallowedTools"),
        "giữa hai cờ không được còn gì: {cmd}"
    );
    // Rào KHÔNG nới: `auto` bỏ bước HỎI, `--disallowedTools` bỏ bước LÀM, và vế
    // sau mới là hàng rào (điều 1). `claude3` TRẦN thì không có rào nào.
    for guard in ["Bash(git push:*)", "Bash(sudo:*)", "Bash(rm:*)"] {
        assert!(cmd.contains(guard), "rào '{guard}' biến mất: {cmd}");
    }
}

/// Không khai `launch` thì rơi về cách cũ — cùng kết quả, KHÔNG đoán tên alias.
#[test]
fn an_account_without_a_declared_launch_word_falls_back_not_guesses() {
    let mut cfg = huba::config::Config::default();
    cfg.claude_accounts = vec![huba::config::ClaudeAccountCfg {
        name: "accX".into(),
        config_dir: Some("~/.claude-accX".into()),
        launch: None,
    }];
    let got = huba::sessions::account_launch(&cfg, Some("accX"));
    assert!(got.starts_with("CLAUDE_CONFIG_DIR="), "{got}");
    assert!(got.contains(".claude-accX"), "{got}");
    // KHÔNG được bịa ra `claudeX` từ tên tài khoản.
    assert!(!got.contains("claudeX"), "đoán tên alias: {got}");
}

/// Tên tài khoản LẠ không được lặng lẽ hoá thành tài khoản mặc định.
///
/// Nó vẫn phải trả về một chuỗi (hàm này không từ chối được), nhưng lượt rơi ấy
/// là một sự kiện phải ghi log — luật 3. `/new` đã chặn tên lạ ở tầng trên, nên
/// tới được đây nghĩa là có một chỗ gọi mới quên.
#[test]
fn an_unknown_account_name_does_not_silently_become_the_default() {
    let cfg = acc_cfg();
    let got = huba::sessions::account_launch(&cfg, Some("acc9"));
    // Rơi về mặc định là hành vi đúng; ĐIỀU KIỆN là nó có kêu. Ở đây chốt phần
    // đo được của mã: nó KHÔNG được trả về từ của một tài khoản có thật khác.
    assert!(
        !got.contains("claude2") && !got.contains("claude3"),
        "tên lạ vớ phải tài khoản khác: {got}"
    );
}

/// Ngoại lệ DUY NHẤT còn đi bằng argv: bản bàn giao ~2 KB do huba tự soạn.
#[test]
fn only_the_handover_payload_still_travels_as_argv() {
    let with = huba::sessions::terminal_command(
        "claude",
        std::path::Path::new("/Users/x/projects"),
        Some("BÀN GIAO: …"),
    );
    let p = with.find("BÀN GIAO").expect("thiếu đề bài");
    let d = with.find("--disallowedTools").expect("thiếu rào");
    assert!(p < d, "đề bài phải đứng trước --disallowedTools: {with}");
}

// ─────────────────────────────────────────────────────────────────────────────
// HỎI ĐƯỢC MỘT PHIÊN VỪA TẮT
//
// 🔴 Hà 2026-08-12 16:37 gõ `/ask` và nhận `⚠ không thấy phiên … đang chạy nữa`
// — con trỏ đang theo trỏ vào một phiên vừa tắt lúc 16:08. Ngồi trước máy thì
// câu ấy vẫn hỏi được (`claude --resume <id>` chạy trên NHẬT KÝ, không cần tiến
// trình), nên phía điện thoại không làm được là một KHOẢNG TRỐNG — đúng phép
// thử "huba là cầu nối" trong CLAUDE.md.
// ─────────────────────────────────────────────────────────────────────────────

const T_NOW: i64 = 1_800_000_000;

fn ended(id: &str, at: i64) -> (huba::sessions::LiveSession, i64) {
    (
        huba::sessions::LiveSession {
            session_id: id.to_string(),
            name: "projects-d8".into(),
            account: "acc1".into(),
            cwd: "/Users/hanguyen/projects".into(),
            ..Default::default()
        },
        at,
    )
}

#[test]
fn a_session_that_just_ended_is_still_askable() {
    let book = [ended("69a38c64", T_NOW - 600)];
    let got = huba::pipeline::pick_ended(&book, "69a38c64", T_NOW).expect("mất phiên vừa tắt");
    // Ba thứ `--resume` cần, và cả ba chỉ còn ở cuốn sổ: id, tài khoản, thư mục.
    assert_eq!(got.account, "acc1");
    assert!(!got.cwd.is_empty(), "thiếu cwd thì không tìm ra nhật ký");
}

/// …nhưng không phải phiên của tuần trước: con trỏ bị bỏ quên thì `/ask` sẽ âm
/// thầm chạy trên một cuộc hội thoại chẳng liên quan, và vẫn tính hạn mức.
#[test]
fn an_old_ended_session_is_not_resurrected() {
    let book = [ended(
        "cu-lam-roi",
        T_NOW - huba::pipeline::ENDED_KEEP_SEC - 1,
    )];
    assert!(huba::pipeline::pick_ended(&book, "cu-lam-roi", T_NOW).is_none());
}

#[test]
fn an_unknown_id_stays_unknown() {
    let book = [ended("69a38c64", T_NOW - 60)];
    assert!(huba::pipeline::pick_ended(&book, "khong-co", T_NOW).is_none());
}

/// Sổ giữ ĐÚNG 24 giờ — con số này là lời hứa in ra màn ("giữ 24 giờ"), nên nó
/// không được trôi mà câu chữ đứng yên.
#[test]
fn the_promise_on_screen_matches_the_constant() {
    assert_eq!(huba::pipeline::ENDED_KEEP_SEC, 24 * 3600);
}

// ─────────────────────────────────────────────────────────────────────────────
// `/cmd` — câu trả lời phải nói được ba điều, và cả ba từng là chỗ đoán mò.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn a_failing_command_never_reads_like_a_successful_one() {
    let ok = huba::pipeline::cmd_report(Some(0), false, "hai dòng\nđây", "", 1200);
    let bad = huba::pipeline::cmd_report(Some(1), false, "", "không thấy tệp", 300);
    assert!(ok.starts_with("✅"), "{ok}");
    assert!(bad.starts_with("❌") && bad.contains("exit 1"), "{bad}");
    assert!(bad.contains("không thấy tệp"), "stderr phải tới nơi: {bad}");
}

/// "Không in ra gì" KHÁC "chưa chạy được" — nói thẳng, đừng trả một câu rỗng.
#[test]
fn a_silent_command_says_it_was_silent() {
    let out = huba::pipeline::cmd_report(Some(0), false, "   ", "", 40);
    assert!(out.contains("không in ra gì"), "{out}");
}

#[test]
fn a_timeout_is_its_own_answer() {
    let out = huba::pipeline::cmd_report(None, true, "", "", 120_000);
    assert!(out.contains("quá giờ"), "{out}");
}

/// Bị cắt thì phải NÓI là bị cắt: một tin Telegram cụt đuôi trông y hệt một
/// lệnh chạy xong sớm.
#[test]
fn a_truncated_output_says_how_much_is_missing() {
    let big = "x".repeat(huba::pipeline::CMD_OUT_MAX + 250);
    let out = huba::pipeline::cmd_report(Some(0), false, &big, "", 10);
    assert!(
        out.contains("còn 250 ký tự"),
        "{}",
        &out[out.len().saturating_sub(120)..]
    );
}

/// Cửa sổ TRẦN mở ở `~`; phiên CLI mở ở GỐC WORKSPACE. Hai chỗ, hai lý do.
///
/// 🔴 Hà 2026-08-15: *"nếu lệnh `/new` trống thì mặc định mở terminal ở `~`"* ·
/// *"còn có tham số, tức là vào cli thì vào đúng workspace"*.
///
/// Chỗ khác nhau ấy không phải sở thích: `~/projects` là thư mục duy nhất cả ba
/// tài khoản đã duyệt (`hasTrustDialogAccepted`), nên nó là điều kiện của một
/// **phiên CLI** — không phải của một cái cửa sổ. Mở cửa sổ trần ở đó là mang
/// một ràng buộc sang chỗ không có ràng buộc ấy.
#[test]
fn a_bare_window_opens_at_home_and_a_cli_session_at_the_workspace_root() {
    // Phiên CLI: `cd <gốc workspace>` rồi mới tới từ mở.
    let cli = huba::sessions::terminal_command(
        "claude3",
        std::path::Path::new("/Users/x/projects"),
        None,
    );
    assert!(
        cli.starts_with("cd '/Users/x/projects' && claude3 "),
        "{cli}"
    );

    // Cửa sổ trần: đọc CHÍNH hằng số mã dùng, không phải một chuỗi chép lại vào
    // test — chép lại thì bài kiểm tự khẳng định điều nó định kiểm.
    let bare = huba::sessions::BARE_TERMINAL_CMD;
    assert_eq!(bare, "cd ~/");
    // Dấu ngã KHÔNG được bọc nháy: trong shell `'~'` là một thư mục TÊN dấu
    // ngã, không phải nhà. Lỗi này IM LẶNG — cửa sổ vẫn mở, chỉ là mở nhầm chỗ.
    assert!(
        !bare.contains('\''),
        "dấu ngã bị bọc nháy ⟹ không nở ra nhà: {bare}"
    );
    assert!(
        !bare.contains("projects"),
        "cửa sổ trần không được mở ở gốc workspace: {bare}"
    );
}

/// Cửa sổ trần là một MỤC TIÊU bấm được — cùng route, cùng sổ với phiên CLI.
///
/// 🔴 Hà 2026-08-15: *"`/terminal` luôn liệt kê terminal trống → bấm chọn thì
/// làm việc được với nó (giống như session)"* · *"lúc này lệnh shot sẽ làm được
/// cho cả 2"*.
///
/// "Giống như session" đọc theo nghĩa đen, và đó là chỗ tiết kiệm được cả một
/// nhánh: nút gửi `sess:<id>`, tức ĐÚNG callback của `/session`. Thêm một hạng
/// mục tiêu mà KHÔNG thêm một đường đi — đúng câu Hà chê sáng nay (*"chưa kế
/// thừa được các lệnh"*).
#[test]
fn a_bare_window_is_a_target_on_the_same_route_as_a_session() {
    // Nút của `/terminal` phải đi đúng route `/session`, không đẻ callback mới.
    let id = format!("{}ttys006", huba::sessions::SHELL_ID_PREFIX);
    assert_eq!(
        huba::telegram::callback_to_command(&format!("sess:{id}")).as_deref(),
        Some("/session win-ttys006")
    );
    // Và `same_session` phải nhận nguyên id ấy — nó không phải uuid 8 ký tự.
    assert!(huba::pipeline::same_session(&id, &id));
    assert!(!huba::pipeline::same_session("win-ttys007", &id));

    // Phân hạng đọc từ MỘT chỗ, không mỗi nơi tự so chuỗi.
    assert!(huba::sessions::is_shell_id(&id));
    assert!(!huba::sessions::is_shell_id(
        "dda2aa85-0000-0000-0000-000000000000"
    ));
}

/// Cửa sổ TRẦN cũng là một mục tiêu mà câu lệnh tự nói ra được.
///
/// 🔴 Hà 2026-08-15: bấm cửa sổ `ttys002` rồi gõ `ls`, và cái shell nhận nguyên
/// `win-ttys002 Ls` → `zsh: command not found: win-ttys002`. Đo trong log:
/// `telegram_text_as_typing len=2` mà `keys_typed` **len=14** — id bị dán vào
/// ĐẦU chữ. Đường gõ dựng `/type <id> <chữ>` rồi `split_target` tách lại; nó chỉ
/// biết hình dạng uuid, nên `win-ttys002` không phải id ⟹ cả chuỗi thành chữ.
///
/// Đúng con bệnh của cả ngày: `is_shell_id` vừa dựng xong ở `sessions`, mà chỗ
/// này chưa ai bảo.
#[test]
fn a_bare_window_id_is_recognised_as_a_target_not_typed_as_text() {
    assert_eq!(
        huba::pipeline::split_target("win-ttys002 ls -la"),
        Some(("win-ttys002".to_string(), "ls -la".to_string()))
    );
    // Trơn thì KHÔNG phải mệnh lệnh nhắm vào cửa sổ — cùng luật với id ngắn:
    // `/type win-ttys002` là chữ gõ vào phiên đang theo.
    assert_eq!(huba::pipeline::split_target("win-ttys002"), None);
    // Và một câu văn mở đầu bằng chữ `win-` thì không được nuốt mất từ đầu.
    assert_eq!(huba::pipeline::split_target("win-win thế nào rồi"), None);
    // uuid + id ngắn vẫn nguyên.
    assert_eq!(
        huba::pipeline::split_target("dda2aa85 chạy test đi"),
        Some(("dda2aa85".to_string(), "chạy test đi".to_string()))
    );
}

/// Danh sách cửa sổ Terminal — bản chụp THẬT, và cái bẫy "rỗng nghĩa là gì".
///
/// 🔴 Hà 2026-08-15: *"lệnh terminal chưa đúng"* · *"đang có 2 cửa sổ không chạy
/// gì"*. Đo tay đúng lúc ấy: 6 tab, 2 trần (`ttys000`, `ttys002`) — còn
/// `keys::terminal_tabs()` trả `Ok(vec![])`, không một tiếng nào.
///
/// Hai lỗi nằm gọn trong bốn dòng AppleScript, và **không bài kiểm nào chạm tới
/// chúng** vì cả hàm đòi một Terminal thật:
/// 1. `tab` bên trong `tell application "Terminal"` là TÊN LỚP của Terminal,
///    không phải ký tự tab ⟹ nối vào chuỗi là ném lỗi;
/// 2. `(p as string)` trên một phần tử `processes` cũng ném lỗi.
///
/// Và thứ biến hai lỗi ấy thành hai ngày im lặng là một `try` không có
/// `on error`: nó dựng lên đúng lý do (có một "cửa sổ" không phải cửa sổ thật,
/// `-1728`), nhưng vì lỗi xảy ra với MỌI cửa sổ, nó nuốt sạch — và *"không có
/// cửa sổ nào"* là một câu trả lời nghe hoàn toàn hợp lý.
#[test]
fn an_empty_tab_list_must_be_distinguishable_from_a_blind_one() {
    // Bản chép NGUYÊN VĂN kết quả AppleScript trên máy này, 2026-08-15 — kèm
    // cột thứ tư (số dòng màn) mà khuôn bản tin thêm vào 2026-08-16; lượt dò
    // không xin chữ thì cột ấy là 0 ở mọi hàng.
    let real = "/dev/ttys004\ttrue\tlogin|-zsh|claude|project-agent|node|caffeinate\t0\n\
                /dev/ttys002\tfalse\tlogin|-zsh\t0\n\
                /dev/ttys003\ttrue\tlogin|-zsh|claude|project-agent|node|caffeinate\t0\n\
                /dev/ttys000\tfalse\tlogin|-zsh\t0\n\
                /dev/ttys001\ttrue\tlogin|-zsh|claude|project-agent|node\t0\n\
                /dev/ttys005\ttrue\tlogin|-zsh|claude|project-agent|node|caffeinate\t0\n\
                #skipped\t1\n";
    let (tabs, skipped) = huba::keys::parse_tabs(real, false);
    assert_eq!(tabs.len(), 6, "{tabs:?}");
    assert_eq!(
        skipped, 1,
        "một cửa sổ không phải cửa sổ thật — chuyện thường"
    );
    assert_eq!(tabs[0].tty, "ttys004", "phải bỏ tiền tố /dev/");
    assert!(
        tabs.iter().all(|t| t.screen.is_none()),
        "lượt dò không xin chữ ⟹ `None`, KHÔNG phải `Some(\"\")` — hai chuyện khác nhau"
    );

    // Và ĐÚNG hai cái trần — con số Hà đọc bằng mắt.
    let bare: Vec<&str> = tabs
        .iter()
        .filter(|t| t.cli().is_none())
        .map(|t| t.tty.as_str())
        .collect();
    assert_eq!(bare, vec!["ttys002", "ttys000"], "{tabs:?}");

    // 🔴 Chốt của cả bài: MÙ khác RỖNG. Mọi cửa sổ đều ném lỗi thì `tabs` rỗng
    // *và* `skipped` > 0 — chỗ gọi phải phân biệt được, không thì nó lại báo
    // "máy không có cửa sổ nào" trên một cái máy đang mở sáu cửa sổ.
    let (blind, blind_skipped) = huba::keys::parse_tabs("#skipped\t6\n", false);
    assert!(blind.is_empty());
    assert_eq!(blind_skipped, 6);

    // …còn máy KHÔNG có cửa sổ nào thật thì cả hai bằng 0.
    let (none, none_skipped) = huba::keys::parse_tabs("#skipped\t0\n", false);
    assert!(none.is_empty());
    assert_eq!(none_skipped, 0);
}

/// Chữ trên màn về cùng danh sách tab — và KHUNG phải chịu được chữ của người khác.
///
/// Vì sao đếm dòng chứ không cắt theo dấu phân cách: chữ trên màn là chữ huba
/// không viết. Bất cứ dấu nào chọn làm ranh giới cũng có ngày nằm sẵn trên một
/// màn nào đó — và hôm ấy phép đọc lệch mà không ai biết, vì một danh sách tab
/// lệch vẫn trông hoàn toàn hợp lý. Bài này cố tình đặt một dòng **giống hệt
/// dòng đầu bản tin** vào giữa màn: khung đếm dòng thì không thấy nó, khung cắt
/// theo dấu thì gãy ngay tại đó.
///
/// Số dòng và tên tiến trình lấy từ bản chụp thật trên máy này (2026-08-16,
/// 11 tab / 304 dòng); chữ MÀN thì viết tay — màn thật của phiên người khác
/// không được nằm trong repo (điều 5).
#[test]
fn screens_are_framed_by_line_count_so_screen_text_cannot_break_the_frame() {
    let real = "/dev/ttys002\tfalse\tlogin|-zsh\t3\n\
                ❯ ls\n\
                /dev/ttys999\ttrue\tlogin|-zsh|claude\t99\n\
                ❯ \n\
                /dev/ttys001\ttrue\tlogin|-zsh|claude|project-agent|node\t2\n\
                ✻ Đang nghĩ… (2m 14s · 1.2k tokens)\n\
                ╭─────────────╮\n\
                #skipped\t1\n";
    let (tabs, skipped) = huba::keys::parse_tabs(real, true);

    assert_eq!(skipped, 1);
    assert_eq!(
        tabs.len(),
        2,
        "dòng giữa màn ttys002 TRÔNG như một dòng đầu bản tin — nó không được thành một tab: {tabs:?}"
    );
    assert_eq!(tabs[0].tty, "ttys002");
    assert_eq!(
        tabs[0].screen.as_deref(),
        Some("❯ ls\n/dev/ttys999\ttrue\tlogin|-zsh|claude\t99\n❯ "),
        "ba dòng, nguyên văn, kể cả dòng trông giống bản tin"
    );
    assert_eq!(tabs[1].tty, "ttys001");
    assert!(tabs[1].screen.as_deref().unwrap().contains("2m 14s"));

    // Và dòng "đang làm gì" đọc được từ đúng chữ ấy, không hỏi Terminal lần nào.
    let seen = huba::keys::alive_tab(&tabs, "/dev/ttys001").expect("tab còn tiến trình");
    let huba::keys::Look::Saw { body, .. } =
        huba::keys::look_from_screen(seen.screen.as_deref().unwrap(), 6)
    else {
        panic!("màn không có bí mật ⟹ phải nhìn rõ");
    };
    assert_eq!(
        huba::keys::activity(&body).map(|a| a.verb),
        Some("Đang nghĩ".to_string())
    );

    // Bản tin CỤT (osascript bị cắt, Terminal chết giữa chừng) không được đọc
    // thành "màn trống": đọc thiếu bao nhiêu dòng thì trả bấy nhiêu, và chỗ dò
    // đã ghi log — nhưng tuyệt đối không dựng thêm tab từ phần thiếu.
    let cut = "/dev/ttys001\ttrue\tlogin|-zsh|claude\t5\nmột dòng\n";
    let (short, _) = huba::keys::parse_tabs(cut, true);
    assert_eq!(short.len(), 1);
    assert_eq!(short[0].screen.as_deref(), Some("một dòng"));
}

/// Một tty có thể ứng với NHIỀU tab, và chỉ một trong số đó là phiên thật.
///
/// Cùng luật với `window_script` (đo 2026-08-11: ba cửa sổ cùng khai
/// `/dev/ttys005`, hai là xác). Đây là bản tra-trong-tập của nó, nên nó phải
/// gãy ở đúng chỗ AppleScript đã gãy — không thì phép tra rẻ hơn mà sai hơn.
#[test]
fn a_recycled_tty_must_resolve_to_the_live_tab_not_a_corpse() {
    let real = "/dev/ttys005\tfalse\t\t1\n\
                [Process completed]\n\
                /dev/ttys005\tfalse\tlogin|-zsh\t1\n\
                ❯ \n\
                /dev/ttys005\ttrue\tlogin|-zsh|claude\t1\n\
                ✻ Đang chạy… (0m 3s)\n\
                #skipped\t0\n";
    let (tabs, _) = huba::keys::parse_tabs(real, true);
    assert_eq!(tabs.len(), 3);

    let got = huba::keys::alive_tab(&tabs, "ttys005").expect("có tab sống");
    assert!(
        got.busy && got.screen.as_deref().unwrap().contains("Đang chạy"),
        "phải chọn tab ĐANG CHẠY, không phải cái xác đứng đầu danh sách: {got:?}"
    );
    // Chấp cả hai cách gọi tên — `ps` trả `/dev/ttysNNN`, Terminal trả `ttysNNN`.
    assert_eq!(huba::keys::alive_tab(&tabs, "/dev/ttys005"), Some(got));
    // Không có tab nào mang tty ấy ⟹ huba KHÔNG có tay nào chạm tới.
    assert!(huba::keys::alive_tab(&tabs, "ttys042").is_none());
    // Chỉ còn cái xác (không tiến trình nào) thì cũng vậy: một tab đã chết
    // không phải một cửa sổ gõ vào được.
    let (dead, _) = huba::keys::parse_tabs("/dev/ttys005\tfalse\t\t0\n#skipped\t0\n", true);
    assert!(huba::keys::alive_tab(&dead, "ttys005").is_none());
}
