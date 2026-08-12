// Test setup builds a Config by starting from the default and setting the one or
// two fields under test. Clippy prefers struct-update syntax; here the mutation
// form is the clearer statement of "everything default EXCEPT this".
#![allow(clippy::field_reassign_with_default)]

use hub::config::{self, CallCfg, Config};

fn write_config(json: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hub.config.json");
    std::fs::write(&file, json).unwrap();
    (dir, file)
}

#[test]
fn defaults_are_bounded_and_dial_nothing_on_their_own() {
    let d = Config::default();
    // A fresh install talks to no room until it is told which one.
    assert!(!d.adapters.tfl5.enabled);
    assert!(d.trust.tfl5_user_tids.is_empty());
    // And one call has a stop on it, in money and in time.
    assert!(d.call.max_budget_usd > 0.0 && d.call.max_budget_usd <= 5.0);
    assert!(d.call.timeout_sec >= 10 && d.call.timeout_sec <= 3600);
}

#[test]
fn config_file_overrides_merge_deeply_and_paths_become_absolute() {
    let (_dir, file) = write_config(
        r#"{ "adapters": { "tfl5": { "room": "hub" } }, "call": { "timeout_sec": 300 } }"#,
    );
    let cfg = config::load(Some(&file)).unwrap();

    assert_eq!(cfg.call.timeout_sec, 300);
    assert_eq!(cfg.adapters.tfl5.room, "hub");
    // untouched sibling keys survive the merge
    assert_eq!(cfg.adapters.tfl5.limit, 50);
    assert_eq!(cfg.call.max_budget_usd, CallCfg::default().max_budget_usd);
    assert!(cfg.db.is_absolute() && cfg.log_file.is_absolute());
    assert!(cfg.workspace_root.is_absolute());
}

/// A config file left over from the inbox era must still LOAD. Every key that
/// went away on 2026-08-08 (`triage`, `act`, `autonomy`, `routing`,
/// `daily_budget_usd`, `max_triage_per_cycle`, `web`, `leak_patterns`) is
/// simply unknown to serde now — a hub that refused to start because of a stale
/// key would be a hub that cannot be upgraded without hand-editing json first.
#[test]
fn a_config_from_the_inbox_era_still_loads_and_its_dead_keys_are_ignored() {
    let (_dir, file) = write_config(
        r#"{
            "adapters": { "tfl5": { "room": "hub" } },
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
    assert_eq!(cfg.adapters.tfl5.room, "hub");
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
    assert!(err.contains("invalid hub config"), "{err}");
}

/// The room is the only way in, so a hub configured to listen to a room nobody
/// may command is a hub that silently ignores its owner. Both halves are
/// checked at startup rather than discovered in a log line.
#[test]
fn a_room_with_no_owner_and_no_app_is_refused_at_startup() {
    let mut no_owner = Config::default();
    no_owner.adapters.tfl5.enabled = true;
    no_owner.adapters.tfl5.app_tid = "a-1234".into();
    let err = config::validate(&no_owner).unwrap_err().to_string();
    assert!(err.contains("tfl5_user_tids"), "{err}");

    let mut no_app = Config::default();
    no_app.adapters.tfl5.enabled = true;
    no_app.trust.tfl5_user_tids = vec!["u-owner".into()];
    let err = config::validate(&no_app).unwrap_err().to_string();
    assert!(err.contains("app_tid"), "{err}");
}

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
    // The mailler and Telegram key names went with their adapters; the tfl5
    // credentials are the ones left, and only their NAMES may appear.
    assert!(
        text.contains("HUB_TFL5_USER") && text.contains("HUB_TFL5_PASSWORD"),
        "only the env var NAME belongs in config"
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

fn acc_cfg() -> hub::config::Config {
    let mut c = hub::config::Config::default();
    c.claude_accounts = vec![
        hub::config::ClaudeAccountCfg { name: "acc1".into(), config_dir: None },
        hub::config::ClaudeAccountCfg {
            name: "acc2".into(),
            config_dir: Some("~/.claude-acc2".into()),
        },
    ];
    c
}

fn snap_with(sessions: Vec<hub::sessions::LiveSession>) -> hub::sessions::SessionsSnapshot {
    hub::sessions::SessionsSnapshot { sessions, ..Default::default() }
}

fn row(account: &str, name: &str) -> hub::sessions::LiveSession {
    hub::sessions::LiveSession {
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
    let said = hub::runtime::accounts_text(&acc_cfg(), &live, &serde_json::json!({}));
    let line = said
        .lines()
        .find(|l| l.starts_with("acc1"))
        .unwrap_or_else(|| panic!("thiếu dòng acc1:\n{said}"));
    assert!(line.contains("mặc định"), "không nói acc nào là mặc định: {line}");
    assert!(
        !said.lines().find(|l| l.starts_with("acc2")).unwrap().contains("mặc định"),
        "acc có config_dir riêng không phải mặc định:\n{said}"
    );
    assert!(said.contains("projects-7c"), "không nói phiên nào của acc nào:\n{said}");
}

/// Tài khoản KHÔNG liệt kê được phiên thì "0 phiên" là con số của một phép đo
/// hỏng — phải nói thẳng, đúng bài học 14:44 hôm nay.
#[test]
fn a_blind_account_says_its_zero_is_not_trustworthy() {
    let mut live = snap_with(vec![]);
    live.blind.push("acc2".into());
    live.notes.push("acc2: spawn claude failed: No such file or directory".into());
    let said = hub::runtime::accounts_text(&acc_cfg(), &live, &serde_json::json!({}));
    assert!(said.contains("KHÔNG liệt kê được"), "không cảnh báo tài khoản mù:\n{said}");
    assert!(said.contains("spawn claude failed"), "không nói lý do:\n{said}");
}

/// "Chưa đo xong" KHÁC "đã đo và bằng 0" — một con số bịa trông y hệt số thật.
#[test]
fn usage_still_being_measured_says_so_instead_of_showing_zero() {
    let live = snap_with(vec![]);
    let said = hub::runtime::accounts_text(
        &acc_cfg(),
        &live,
        &serde_json::json!({ "pending": true }),
    );
    assert!(said.contains("đang đo"), "phải nói đang đo:\n{said}");
    assert!(!said.contains("0%"), "không được bịa 0%:\n{said}");
}

#[test]
fn measured_quota_reaches_the_line() {
    let live = snap_with(vec![]);
    let said = hub::runtime::accounts_text(
        &acc_cfg(),
        &live,
        &serde_json::json!({ "accounts": { "acc1": { "week_pct": 98, "session_pct": 6 } } }),
    );
    assert!(said.contains("tuần 98%"), "thiếu hạn mức tuần:\n{said}");
    assert!(said.contains("phiên 6%"), "thiếu hạn mức phiên:\n{said}");
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
    let (f, rest) = hub::pipeline::split_flags("-a acc2 -s dwork sửa lịch làm việc", NEW_FLAGS);
    assert_eq!(f.get("a").map(String::as_str), Some("acc2"));
    assert_eq!(f.get("s").map(String::as_str), Some("dwork"));
    assert_eq!(rest, "sửa lịch làm việc");

    // …kể cả khi đứng SAU đề bài.
    let (f2, rest2) = hub::pipeline::split_flags("sửa lịch làm việc -s dwork -a acc2", NEW_FLAGS);
    assert_eq!(f2.get("s").map(String::as_str), Some("dwork"));
    assert_eq!(rest2, "sửa lịch làm việc");
}

/// Đúng ví dụ Hà gõ: không có đề bài, chỉ mở phiên.
#[test]
fn the_example_from_the_owner_parses_to_account_project_and_no_task() {
    let (f, rest) = hub::pipeline::split_flags("-a acc2 -s dwork", NEW_FLAGS);
    assert_eq!(f.get("a").map(String::as_str), Some("acc2"));
    assert_eq!(f.get("s").map(String::as_str), Some("dwork"));
    assert!(rest.is_empty(), "không có đề bài mà lại sinh ra chữ: {rest}");
}

/// 🔴 Cờ LẠ phải ở nguyên trong đề bài.
///
/// Nuốt im lặng một mẩu đề bài là loại lỗi không truy ra được: phiên vẫn mở,
/// vẫn chạy — chỉ là chạy một đề bài khác với đề bài đã gõ.
#[test]
fn unknown_flags_stay_in_the_text_body() {
    let (f, rest) = hub::pipeline::split_flags("-s hub sửa cờ -x của script", NEW_FLAGS);
    assert_eq!(f.get("s").map(String::as_str), Some("hub"));
    assert_eq!(rest, "sửa cờ -x của script");
}

/// Cờ bỏ trống (`-a` rồi tới cờ khác) không được nuốt cờ kế tiếp.
#[test]
fn an_empty_flag_does_not_eat_the_next_one() {
    let (f, _) = hub::pipeline::split_flags("-a -s dwork", NEW_FLAGS);
    assert_eq!(f.get("a").map(String::as_str), Some(""), "cờ trống phải rỗng, không nuốt -s");
    assert_eq!(f.get("s").map(String::as_str), Some("dwork"));
}

/// Không cờ nào thì phải y hệt lối gõ cũ — lối ấy vẫn nằm trong tay quen của
/// chủ máy và trong các nút Telegram đã gửi đi.
#[test]
fn the_old_positional_form_is_untouched() {
    let (f, rest) = hub::pipeline::split_flags("dwork @acc2 sửa lịch", NEW_FLAGS);
    assert!(f.is_empty(), "không có cờ nào mà lại bóc ra: {f:?}");
    assert_eq!(rest, "dwork @acc2 sửa lịch");
}

/// Cửa sổ mở KHÔNG đề bài thì lệnh không được mang một tham số rỗng.
///
/// `claude ''` là "một đề bài rỗng", khác hẳn `claude` là "chưa có đề bài".
#[test]
fn an_empty_task_opens_a_plain_session_with_no_positional_argument() {
    let cmd = hub::sessions::terminal_command(
        "claude",
        std::path::Path::new("/Users/x/projects"),
        "",
        None,
    );
    assert!(!cmd.contains("'' --disallowedTools"), "đề bài rỗng vẫn được truyền vào: {cmd}");
    assert!(cmd.contains("'claude' --disallowedTools"), "{cmd}");
    // …và có đề bài thì nó vẫn phải đứng TRƯỚC `--disallowedTools` (cờ variadic).
    let with = hub::sessions::terminal_command(
        "claude",
        std::path::Path::new("/Users/x/projects"),
        "[dwork] sửa lịch",
        None,
    );
    let p = with.find("[dwork]").expect("thiếu đề bài");
    let d = with.find("--disallowedTools").expect("thiếu rào");
    assert!(p < d, "đề bài phải đứng trước --disallowedTools: {with}");
}
