// Test setup builds a Config by starting from the default and setting the one or
// two fields under test. Clippy prefers struct-update syntax; here the mutation
// form is the clearer statement of "everything default EXCEPT this".
#![allow(clippy::field_reassign_with_default)]

use hub::config::{
    self, ActCfg, Config, RoutingRule, RoutingWhen, TierName, TriageCfg, ALWAYS_HUMAN_ACTIONS,
};

fn write_config(json: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hub.config.json");
    std::fs::write(&file, json).unwrap();
    (dir, file)
}

#[test]
fn defaults_are_safe_draft_only_act_off() {
    let d = Config::default();
    assert_eq!(&*d.autonomy.default, "L0");
    assert!(!d.act.enabled);
    assert_eq!(d.coalesce_hours, 12);
}

#[test]
fn deploy_and_merge_can_never_be_auto_executed() {
    for a in [
        "deploy",
        "merge",
        "force_push",
        "delete_data",
        "rotate_secret",
    ] {
        assert!(ALWAYS_HUMAN_ACTIONS.contains(&a), "{a} must be human-only");
    }
}

#[test]
fn config_file_overrides_merge_deeply_and_paths_become_absolute() {
    let (_dir, file) = write_config(
        r#"{ "adapters": { "tfl5": { "room": "hub" } }, "autonomy": { "default": "L1" } }"#,
    );
    let cfg = config::load(Some(&file)).unwrap();

    assert_eq!(&*cfg.autonomy.default, "L1");
    assert_eq!(cfg.adapters.tfl5.room, "hub");
    // untouched sibling keys survive the merge
    assert_eq!(cfg.adapters.tfl5.limit, 50);
    assert!(cfg.db.is_absolute() && cfg.log_file.is_absolute());
    assert!(cfg.workspace_root.is_absolute());
}

#[test]
fn invalid_tier_interval_and_confidence_are_rejected_loudly() {
    let mut bad_tier = Config::default();
    bad_tier.autonomy.default = TierName("L9".into());
    assert!(config::validate(&bad_tier)
        .unwrap_err()
        .to_string()
        .contains("autonomy.default"));

    let mut bad_project = Config::default();
    bad_project
        .autonomy
        .projects
        .insert("x".into(), "L5".into());
    assert!(config::validate(&bad_project)
        .unwrap_err()
        .to_string()
        .contains("autonomy.projects.x"));

    let mut bad_interval = Config::default();
    bad_interval.poll_interval_sec = 1;
    assert!(config::validate(&bad_interval)
        .unwrap_err()
        .to_string()
        .contains("poll_interval_sec"));

    let mut bad_conf = Config::default();
    bad_conf.triage = TriageCfg {
        min_confidence_auto: 2.0,
        ..TriageCfg::default()
    };
    assert!(config::validate(&bad_conf)
        .unwrap_err()
        .to_string()
        .contains("min_confidence_auto"));

    let mut bad_budget = Config::default();
    bad_budget.triage = TriageCfg {
        max_budget_usd: 0.0,
        ..TriageCfg::default()
    };
    assert!(config::validate(&bad_budget)
        .unwrap_err()
        .to_string()
        .contains("max_budget_usd"));

    let mut bad_routing = Config::default();
    bad_routing.routing = vec![RoutingRule {
        when: RoutingWhen::default(),
        project: "  ".into(),
    }];
    assert!(config::validate(&bad_routing)
        .unwrap_err()
        .to_string()
        .contains("routing rule"));
}

#[test]
fn a_malformed_config_file_fails_fast_instead_of_running_with_defaults() {
    let (_dir, file) = write_config("{ not json");
    let err = config::load(Some(&file)).unwrap_err().to_string();
    assert!(err.contains("cannot parse config"), "{err}");
}

#[test]
fn an_invalid_config_file_is_rejected_at_load_time() {
    let (_dir, file) = write_config(r#"{ "autonomy": { "default": "L7" } }"#);
    let err = config::load(Some(&file)).unwrap_err().to_string();
    assert!(err.contains("invalid hub config"), "{err}");
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

#[test]
fn act_stage_defaults_are_bounded() {
    let a = ActCfg::default();
    assert!(!a.enabled);
    assert!(a.max_budget_usd > 0.0 && a.max_budget_usd <= 5.0);
    assert!(a.timeout_sec <= 3600);
}
