// Test setup builds a Config by starting from the default and setting the one or
// two fields under test. Clippy prefers struct-update syntax; here the mutation
// form is the clearer statement of "everything default EXCEPT this".
#![allow(clippy::field_reassign_with_default)]

mod common;

use common::fresh_db;
use hub::db::{Db, NewDecision, NewMessage};

const THREAD: &str = "dipgle/tfl5:CheckSuite:ci";
const LONG_AGO: &str = "1990-01-01T00:00:00Z";
const SOON: &str = "2999-01-01T00:00:00Z";

fn seed_pending(db: &Db, thread: &str, status: &str, external_id: &str) -> i64 {
    let (id, _) = db
        .insert_message(&NewMessage {
            source: "github".into(),
            external_id: external_id.into(),
            thread_key: Some(thread.into()),
            sender: Some("github:dipgle/tfl5".into()),
            subject: Some("CI failed".into()),
            body: Some("CI failed".into()),
            ..Default::default()
        })
        .unwrap();
    db.insert_decision(&NewDecision {
        message_id: id.unwrap(),
        tier: "L0".into(),
        kind: Some("ci_failure".into()),
        severity: Some("p1".into()),
        summary: Some("s".into()),
        needs_human: true,
        confidence: Some(0.8),
        status: status.into(),
        ..Default::default()
    })
    .unwrap()
}

#[test]
fn a_repeat_on_the_same_thread_finds_the_open_decision() {
    let (db, _dir) = fresh_db();
    let did = seed_pending(&db, THREAD, "pending", "a");
    let found = db
        .pending_decision_for_thread(Some(THREAD), LONG_AGO)
        .unwrap();
    assert_eq!(found.map(|d| d.id), Some(did));
}

#[test]
fn a_different_thread_never_coalesces() {
    let (db, _dir) = fresh_db();
    seed_pending(&db, THREAD, "pending", "a");
    assert!(db
        .pending_decision_for_thread(Some("dipgle/other:Issue:1"), LONG_AGO)
        .unwrap()
        .is_none());
}

#[test]
fn an_answered_decision_does_not_swallow_new_items() {
    let (db, _dir) = fresh_db();
    seed_pending(&db, THREAD, "approved", "a");
    assert!(
        db.pending_decision_for_thread(Some(THREAD), LONG_AGO)
            .unwrap()
            .is_none(),
        "resolved threads must triage again"
    );
}

#[test]
fn the_coalescing_window_is_respected() {
    let (db, _dir) = fresh_db();
    seed_pending(&db, THREAD, "pending", "a");
    assert!(
        db.pending_decision_for_thread(Some(THREAD), SOON)
            .unwrap()
            .is_none(),
        "outside the window means triage again"
    );
}

#[test]
fn no_thread_key_means_no_coalescing() {
    let (db, _dir) = fresh_db();
    seed_pending(&db, THREAD, "pending", "a");
    assert!(db
        .pending_decision_for_thread(None, LONG_AGO)
        .unwrap()
        .is_none());
    assert!(db
        .pending_decision_for_thread(Some(""), LONG_AGO)
        .unwrap()
        .is_none());
}

// ----------------------------------------------------------------------
// Per-source coalescing windows.
//
// Found the hard way on 2026-08-06: with `thread_key` = the chat ROOM and the
// global 12-hour window, a real question was attached to a decision opened by
// an unrelated message earlier in the same room. The draft answered the older
// message; the real question was never triaged at all. A room is a channel,
// not a topic.
// ----------------------------------------------------------------------

#[test]
fn chat_gets_a_short_window_while_issues_keep_the_long_one() {
    let mut cfg = hub::config::Config::default();
    cfg.coalesce_hours = 12;
    cfg.source_coalesce_hours.insert("tfl5".into(), 0.05); // 3 minutes

    let chat = hub::pipeline::coalesce_window_for(&cfg, "tfl5");
    assert_eq!(chat.num_minutes(), 3, "a chat burst, not a working day");

    let gh = hub::pipeline::coalesce_window_for(&cfg, "github");
    assert_eq!(
        gh.num_hours(),
        12,
        "an issue really is one conversation all day"
    );
}

#[test]
fn an_explicit_zero_turns_coalescing_off_for_that_source_only() {
    let mut cfg = hub::config::Config::default();
    cfg.coalesce_hours = 12;
    cfg.source_coalesce_hours.insert("tfl5".into(), 0.0);

    assert!(
        hub::pipeline::coalesce_window_for(&cfg, "tfl5").is_zero(),
        "every message paid for, deliberately"
    );
    assert_eq!(
        hub::pipeline::coalesce_window_for(&cfg, "github").num_hours(),
        12
    );
}

#[test]
fn without_an_override_every_source_keeps_the_global_window() {
    let mut cfg = hub::config::Config::default();
    cfg.coalesce_hours = 6;
    assert_eq!(
        hub::pipeline::coalesce_window_for(&cfg, "tfl5").num_hours(),
        6
    );
    assert_eq!(
        hub::pipeline::coalesce_window_for(&cfg, "email").num_hours(),
        6
    );
}

/// A message on `thread` whose author wrote it at `received_at`, with an open
/// decision attached — the shape a chat room produces.
fn seed_pending_written_at(db: &Db, thread: &str, external_id: &str, received_at: &str) -> i64 {
    let (id, _) = db
        .insert_message(&NewMessage {
            source: "tfl5".into(),
            external_id: external_id.into(),
            thread_key: Some(thread.into()),
            sender: Some("tfl5:u-alice".into()),
            subject: Some("câu hỏi".into()),
            body: Some("câu hỏi".into()),
            received_at: Some(received_at.into()),
            ..Default::default()
        })
        .unwrap();
    db.insert_decision(&NewDecision {
        message_id: id.unwrap(),
        tier: "L0".into(),
        kind: Some("question".into()),
        severity: Some("p2".into()),
        summary: Some("s".into()),
        needs_human: true,
        confidence: Some(0.9),
        status: "pending".into(),
        ..Default::default()
    })
    .unwrap()
}

const ROOM: &str = "tfl5:a-hub:hub";

#[test]
fn the_window_is_measured_from_when_the_message_was_written() {
    // THE REGRESSION LOCK. The window used to be compared against
    // `decisions.ts` — when hub caught up on a backlog, two questions typed
    // twenty minutes apart were triaged seconds apart, so the second was folded
    // into the first and never answered. Observed live on 2026-08-06.
    let (db, _dir) = fresh_db();
    seed_pending_written_at(&db, ROOM, "old", "2026-08-06T08:00:00.000Z");

    // A second question, written 20 minutes later. With a 3-minute chat window
    // its lookback starts at 08:17 — the earlier message is outside it.
    let since_for_second = "2026-08-06T08:17:00.000Z";
    assert!(
        db.pending_decision_for_thread(Some(ROOM), since_for_second)
            .unwrap()
            .is_none(),
        "two questions twenty minutes apart must get two answers"
    );
}

#[test]
fn a_genuine_burst_still_coalesces() {
    // The other half: three lines of one thought, seconds apart, must stay one
    // paid decision — that is what the window is FOR.
    let (db, _dir) = fresh_db();
    let did = seed_pending_written_at(&db, ROOM, "first", "2026-08-06T08:00:00.000Z");
    let since_for_second = "2026-08-06T07:58:30.000Z"; // 08:01:30 minus 3 minutes
    assert_eq!(
        db.pending_decision_for_thread(Some(ROOM), since_for_second)
            .unwrap()
            .map(|d| d.id),
        Some(did)
    );
}
