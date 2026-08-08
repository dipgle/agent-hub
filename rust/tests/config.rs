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
