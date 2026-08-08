// Test setup builds a Config by starting from the default and setting the one or
// two fields under test. Clippy prefers struct-update syntax; here the mutation
// form is the clearer statement of "everything default EXCEPT this".
#![allow(clippy::field_reassign_with_default)]

//! Guards for running unattended: the daily spend ceiling, the secrets file,
//! and the refusal to expose the console without a password.

mod common;

use common::fresh_db;
use hub::config::{self, is_loopback_bind, Config};
use hub::db::NewDecision;
use hub::pipeline::budget_state;

fn seed_decision(db: &hub::db::Db, cost: f64) {
    let (id, _) = db
        .insert_message(&common::sample_new_message(&format!("m{cost}")))
        .unwrap();
    db.insert_decision(&NewDecision {
        message_id: id.unwrap(),
        tier: "L0".into(),
        kind: Some("ci_failure".into()),
        severity: Some("p2".into()),
        summary: Some("s".into()),
        needs_human: true,
        confidence: Some(0.8),
        cost_usd: Some(cost),
        status: "pending".into(),
        ..Default::default()
    })
    .unwrap();
}

#[test]
fn daily_budget_counts_only_todays_spend() {
    let (db, _dir) = fresh_db();
    seed_decision(&db, 0.30);
    seed_decision(&db, 0.20);

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    assert!((db.cost_on_day(&today).unwrap() - 0.5).abs() < 1e-9);
    assert_eq!(
        db.cost_on_day("1999-01-01").unwrap(),
        0.0,
        "another day must not count"
    );
}

#[test]
fn budget_state_reports_spend_against_the_cap_and_zero_disables_it() {
    let (db, _dir) = fresh_db();
    seed_decision(&db, 1.25);

    let mut cfg = Config::default();
    cfg.daily_budget_usd = 5.0;
    let (spent, cap) = budget_state(&db, &cfg)
        .unwrap()
        .expect("ceiling configured");
    assert!((spent - 1.25).abs() < 1e-9);
    assert_eq!(cap, 5.0);

    cfg.daily_budget_usd = 0.0;
    assert!(
        budget_state(&db, &cfg).unwrap().is_none(),
        "0 must mean no ceiling, not an instant stop"
    );
}

#[test]
fn reaching_the_ceiling_stops_triage_and_warns_exactly_once() {
    let (db, dir) = fresh_db();

    // Something waiting to be triaged…
    db.insert_message(&common::sample_new_message("pending-item"))
        .unwrap();
    // …and today's spend already past the ceiling.
    seed_decision(&db, 2.0);

    let mut cfg = Config::default();
    cfg.daily_budget_usd = 1.0;
    cfg.notify.macos_notification = false;
    cfg.notify.file = dir.path().join("notify.log");
    cfg.workspace_root = dir.path().to_path_buf(); // no projects to scan

    let first = hub::pipeline::triage_new(&db, &cfg).unwrap();
    assert!(
        first.budget_stop.is_some(),
        "the ceiling must stop the cycle"
    );
    assert_eq!(
        first.triaged, 0,
        "no triage call may happen past the ceiling"
    );
    assert_eq!(first.cost_usd, 0.0);

    // Queued items are untouched — stopped, not dropped or closed.
    let msgs = db.list_messages(None, None, 10).unwrap();
    assert!(!msgs.is_empty());
    assert!(
        msgs.iter().all(|m| m.status == "new"),
        "nothing may change state while the ceiling holds"
    );

    // Stopping is announced, but only once per day, not once per cycle.
    let warned = || -> i64 {
        db.conn
            .query_row(
                "SELECT COUNT(*) FROM outbox WHERE subject = 'hub: chạm trần chi phí ngày'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(warned(), 1);
    let second = hub::pipeline::triage_new(&db, &cfg).unwrap();
    assert!(second.budget_stop.is_some());
    assert_eq!(warned(), 1, "a second cycle must not re-notify");
}

#[test]
fn spend_below_the_ceiling_does_not_stop_the_cycle() {
    let (db, dir) = fresh_db();
    seed_decision(&db, 0.25);

    let mut cfg = Config::default();
    cfg.daily_budget_usd = 1.0;
    cfg.max_triage_per_cycle = 0; // nothing to claim → no claude call in the test
    cfg.workspace_root = dir.path().to_path_buf();

    let out = hub::pipeline::triage_new(&db, &cfg).unwrap();
    assert!(
        out.budget_stop.is_none(),
        "under the ceiling the loop keeps working"
    );
}

#[test]
fn default_config_ships_with_a_ceiling_so_unattended_runs_are_bounded() {
    let d = Config::default();
    assert!(
        d.daily_budget_usd > 0.0,
        "an always-on daemon must have a default spend ceiling"
    );
    assert!(
        d.web.enabled,
        "hubd serving the console keeps auto-start to one agent"
    );
    assert_eq!(d.web.bind, "127.0.0.1", "never expose by default");
}

#[test]
fn a_non_loopback_bind_without_a_password_is_rejected_at_validate() {
    let mut cfg = Config::default();
    cfg.web.bind = "0.0.0.0".into();
    cfg.web.password_env = "HUB_TEST_WEB_PW_UNSET".into();
    std::env::remove_var("HUB_TEST_WEB_PW_UNSET");

    let err = config::validate(&cfg).unwrap_err().to_string();
    assert!(err.contains("not loopback"), "{err}");
    assert!(err.contains("HUB_TEST_WEB_PW_UNSET"), "{err}");

    // With the password present it is allowed.
    std::env::set_var("HUB_TEST_WEB_PW_UNSET", "hunter2");
    assert!(config::validate(&cfg).is_ok());
    std::env::remove_var("HUB_TEST_WEB_PW_UNSET");
}

#[test]
fn loopback_names_are_recognised() {
    for ok in ["127.0.0.1", "::1", "localhost"] {
        assert!(is_loopback_bind(ok), "{ok}");
    }
    for exposed in ["0.0.0.0", "192.168.1.10", "10.0.0.5", ""] {
        assert!(!is_loopback_bind(exposed), "{exposed}");
    }
}

#[test]
fn env_file_loads_names_only_and_never_overrides_the_real_environment() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("hub.env"),
        "# comment\n\nHUB_TEST_A=alpha\nHUB_TEST_B = \"beta\"\nbroken-line\nHUB_TEST_C=gamma\n",
    )
    .unwrap();

    for k in ["HUB_TEST_A", "HUB_TEST_B", "HUB_TEST_C"] {
        std::env::remove_var(k);
    }
    std::env::set_var("HUB_TEST_C", "from-shell");

    let loaded = config::load_env_file(dir.path());

    assert!(loaded.contains(&"HUB_TEST_A".to_string()));
    assert!(loaded.contains(&"HUB_TEST_B".to_string()));
    assert!(
        !loaded.contains(&"HUB_TEST_C".to_string()),
        "an existing env var must win"
    );
    assert_eq!(std::env::var("HUB_TEST_A").unwrap(), "alpha");
    assert_eq!(
        std::env::var("HUB_TEST_B").unwrap(),
        "beta",
        "quotes are stripped"
    );
    assert_eq!(std::env::var("HUB_TEST_C").unwrap(), "from-shell");

    for k in ["HUB_TEST_A", "HUB_TEST_B", "HUB_TEST_C"] {
        std::env::remove_var(k);
    }
}

#[test]
fn a_missing_env_file_is_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(config::load_env_file(dir.path()).is_empty());
}

// ----------------------------------------------------------------------
// Per-source ceilings — one noisy channel must not drain the whole day.
// ----------------------------------------------------------------------

fn seed_decision_from(db: &hub::db::Db, source: &str, external_id: &str, cost: f64) {
    let m = hub::db::NewMessage {
        source: source.into(),
        ..common::sample_new_message(external_id)
    };
    let (id, _) = db.insert_message(&m).unwrap();
    db.insert_decision(&NewDecision {
        message_id: id.unwrap(),
        tier: "L0".into(),
        kind: Some("question".into()),
        severity: Some("p2".into()),
        summary: Some("s".into()),
        needs_human: true,
        confidence: Some(0.8),
        cost_usd: Some(cost),
        status: "pending".into(),
        ..Default::default()
    })
    .unwrap();
}

#[test]
fn spend_is_attributable_to_the_source_that_caused_it() {
    let (db, _dir) = fresh_db();
    seed_decision_from(&db, "tfl5", "chat-1", 0.40);
    seed_decision_from(&db, "github", "gh-1", 0.90);

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    assert!((db.cost_on_day_for_source(&today, "tfl5").unwrap() - 0.40).abs() < 1e-9);
    assert!((db.cost_on_day_for_source(&today, "github").unwrap() - 0.90).abs() < 1e-9);
    // …and the global total still sees both.
    assert!((db.cost_on_day(&today).unwrap() - 1.30).abs() < 1e-9);
}

#[test]
fn a_source_ceiling_binds_only_that_source() {
    let (db, _dir) = fresh_db();
    seed_decision_from(&db, "tfl5", "chat-1", 0.60);

    let mut cfg = Config::default();
    cfg.source_daily_budget_usd.insert("tfl5".into(), 0.50);

    let (spent, cap) = hub::pipeline::source_budget_state(&db, &cfg, "tfl5")
        .unwrap()
        .expect("ceiling configured");
    assert!(
        spent >= cap,
        "chat is over its own ceiling: {spent} vs {cap}"
    );
    // github never had a ceiling configured, so it is unaffected.
    assert!(hub::pipeline::source_budget_state(&db, &cfg, "github")
        .unwrap()
        .is_none());
}

#[test]
fn a_zero_source_ceiling_means_no_ceiling_not_an_instant_stop() {
    // Same trap the global budget had: 0 must read as "unset", otherwise
    // writing the key at all would silently freeze the channel.
    let (db, _dir) = fresh_db();
    let mut cfg = Config::default();
    cfg.source_daily_budget_usd.insert("tfl5".into(), 0.0);
    assert!(hub::pipeline::source_budget_state(&db, &cfg, "tfl5")
        .unwrap()
        .is_none());
}

#[test]
fn doctor_and_ingest_read_the_same_on_off_table() {
    // These used to be two hand-written matches. Adding a channel to one and
    // not the other made `doctor` report "off" for an adapter the loop was
    // actively polling. Assert every declared adapter is answerable — a name
    // that falls through to the `_ => false` arm is the drift itself.
    let mut cfg = Config::default();
    cfg.adapters.github.enabled = true;
    cfg.adapters.devlog.enabled = true;
    cfg.adapters.email.enabled = true;
    cfg.adapters.telegram.enabled = true;
    cfg.adapters.tfl5.enabled = true;

    for name in hub::pipeline::ADAPTER_NAMES {
        assert!(
            hub::pipeline::adapter_enabled(&cfg, name),
            "{name} is in ADAPTER_NAMES but adapter_enabled cannot see its flag"
        );
    }

    // …and the off case is honoured too, not just "always true".
    cfg.adapters.tfl5.enabled = false;
    assert!(!hub::pipeline::adapter_enabled(&cfg, "tfl5"));
}

// ----------------------------------------------------------------------
// The waker. A chat message that lands mid-sleep must start the next cycle,
// not sit out the poll interval — and a wake must never be lost.
// ----------------------------------------------------------------------

#[test]
fn a_wake_cuts_the_sleep_short() {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let w = hub::live::Waker::new();
    let w2 = Arc::clone(&w);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        w2.wake();
    });
    let t = Instant::now();
    w.sleep(Duration::from_secs(30));
    assert!(
        t.elapsed() < Duration::from_secs(5),
        "woke after {:?}, should be ~80ms",
        t.elapsed()
    );
}

#[test]
fn a_wake_that_arrives_before_the_sleep_is_not_lost() {
    use std::time::{Duration, Instant};
    // The listener can insert while the loop is still working. If the flag were
    // only checked during the wait, that message would wait a whole interval.
    let w = hub::live::Waker::new();
    w.wake();
    let t = Instant::now();
    w.sleep(Duration::from_secs(30));
    assert!(
        t.elapsed() < Duration::from_secs(5),
        "a pre-set wake must return immediately"
    );
}

#[test]
fn without_a_wake_the_sleep_actually_sleeps() {
    use std::time::{Duration, Instant};
    // Otherwise "interruptible" would just mean "busy loop", and the daemon
    // would poll tfl5 and GitHub as fast as the CPU allows.
    let w = hub::live::Waker::new();
    let t = Instant::now();
    w.sleep(Duration::from_millis(200));
    assert!(
        t.elapsed() >= Duration::from_millis(150),
        "returned after {:?}",
        t.elapsed()
    );
}
