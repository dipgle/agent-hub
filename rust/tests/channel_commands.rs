//! Button presses arriving from a channel, and the config writes the UI does.

use hub::adapters::{telegram, CommandKind};
use hub::config::{self, Config};
use serde_json::json;

fn callback(chat_id: i64, data: &str) -> serde_json::Value {
    json!({
        "id": "cb-1",
        "data": data,
        "message": { "message_id": 77, "chat": { "id": chat_id } }
    })
}

#[test]
fn approve_and_reject_presses_become_commands() {
    let allowed = vec!["12345".to_string()];

    let a = telegram::parse_callback(&callback(12345, "a:9"), &allowed)
        .unwrap()
        .expect("approve parsed");
    assert_eq!(a.kind, CommandKind::Approve);
    assert_eq!(a.decision_id, 9);
    assert_eq!(a.chat_id, "12345");
    assert_eq!(a.message_id, Some(77));
    assert_eq!(a.callback_id, "cb-1");

    let r = telegram::parse_callback(&callback(12345, "r:9"), &allowed)
        .unwrap()
        .expect("reject parsed");
    assert_eq!(r.kind, CommandKind::Reject);
}

#[test]
fn a_press_from_an_unlisted_chat_is_dropped_not_executed() {
    let allowed = vec!["12345".to_string()];
    let out = telegram::parse_callback(&callback(999, "a:9"), &allowed).unwrap();
    assert!(out.is_none(), "a stranger must never be able to approve");
}

#[test]
fn malformed_callback_data_is_an_error_not_a_silent_skip() {
    let allowed: Vec<String> = vec![];
    for bad in ["", "approve", "x:9", "a:abc"] {
        assert!(
            telegram::parse_callback(&callback(1, bad), &allowed).is_err(),
            "{bad} should error"
        );
    }
}

#[test]
fn an_empty_allow_list_accepts_the_press_but_the_adapter_is_opt_in() {
    // allowed_chat_ids empty = adapter has not been linked yet; parsing still
    // works so `hub doctor` can show what arrived, and trust is decided later
    // by policy (untrusted ⇒ tier L0).
    let out = telegram::parse_callback(&callback(42, "a:3"), &[]).unwrap();
    assert!(out.is_some());
}

#[test]
fn config_save_round_trips_and_keeps_the_file_free_of_nulls() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hub.config.json");
    std::fs::write(&file, r#"{ "autonomy": { "default": "L1" } }"#).unwrap();

    let mut cfg = config::load(Some(&file)).unwrap();
    cfg.coalesce_hours = 7;
    cfg.adapters.telegram.enabled = true;
    cfg.adapters.telegram.allowed_chat_ids = vec!["12345".into()];
    cfg.routing.push(config::RoutingRule {
        when: config::RoutingWhen {
            repo: Some("dipgle/tfl5".into()),
            ..Default::default()
        },
        project: "tfl5".into(),
    });
    config::save(&cfg).unwrap();

    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        !text.contains(": null"),
        "saved config must not be padded with nulls:\n{text}"
    );
    assert!(text.contains("\"repo\": \"dipgle/tfl5\""));
    assert!(
        std::fs::metadata(file.with_extension("json.bak")).is_ok(),
        "a backup must be kept"
    );

    let reloaded = config::load(Some(&file)).unwrap();
    assert_eq!(reloaded.coalesce_hours, 7);
    assert_eq!(&*reloaded.autonomy.default, "L1");
    assert!(reloaded.adapters.telegram.enabled);
    assert_eq!(
        reloaded.adapters.telegram.allowed_chat_ids,
        vec!["12345".to_string()]
    );
    assert_eq!(reloaded.routing.len(), 1);
    assert_eq!(
        reloaded.routing[0].when.repo.as_deref(),
        Some("dipgle/tfl5")
    );
}

#[test]
fn save_refuses_an_invalid_config_so_the_ui_cannot_brick_the_daemon() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hub.config.json");
    std::fs::write(&file, "{}").unwrap();

    let mut cfg = config::load(Some(&file)).unwrap();
    cfg.poll_interval_sec = 1; // below the floor
    assert!(config::save(&cfg).is_err());

    // The file on disk is untouched by the rejected save.
    let still: Config = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(still.poll_interval_sec, 120);
}
