mod common;

use common::{fresh_db, sample_new_message};
use hub::db::{MessagePatch, NewDecision, NewOutbox, RunFinish};
use serde_json::json;

#[test]
fn insert_message_is_idempotent_on_source_and_external_id() {
    let (db, _dir) = fresh_db();

    let (_, first) = db
        .insert_message(&sample_new_message("notif:1:2026-07-26T00:00:00Z"))
        .unwrap();
    assert!(first);
    let (_, second) = db
        .insert_message(&sample_new_message("notif:1:2026-07-26T00:00:00Z"))
        .unwrap();
    assert!(!second, "replaying the same poll window must not duplicate");
    assert_eq!(db.list_messages(None, None, 50).unwrap().len(), 1);

    // A new updated_at is a genuinely new item.
    let (_, third) = db
        .insert_message(&sample_new_message("notif:1:2026-07-27T00:00:00Z"))
        .unwrap();
    assert!(third);
    assert_eq!(db.list_messages(None, None, 50).unwrap().len(), 2);
}

#[test]
fn status_transitions_attempts_and_error_text_persist() {
    let (db, _dir) = fresh_db();
    let (id, _) = db.insert_message(&sample_new_message("a")).unwrap();
    let id = id.unwrap();

    db.set_message_status(
        id,
        "triaging",
        MessagePatch {
            bump_attempts: true,
            project: Some("tfl5".into()),
            ..Default::default()
        },
    )
    .unwrap();
    db.set_message_status(
        id,
        "new",
        MessagePatch {
            last_error: Some(Some("boom".into())),
            bump_attempts: true,
            ..Default::default()
        },
    )
    .unwrap();

    let row = db.get_message(id).unwrap().unwrap();
    assert_eq!(row.status, "new");
    assert_eq!(row.attempts, 2);
    assert_eq!(row.last_error.as_deref(), Some("boom"));

    // Clearing must actually clear, not write the string "null".
    db.set_message_status(
        id,
        "closed",
        MessagePatch {
            last_error: Some(None),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(db.get_message(id).unwrap().unwrap().last_error, None);
}

#[test]
fn reset_triaging_rescues_rows_stranded_by_a_crash() {
    let (db, _dir) = fresh_db();
    let (id, _) = db.insert_message(&sample_new_message("a")).unwrap();
    let id = id.unwrap();
    db.set_message_status(id, "triaging", MessagePatch::default())
        .unwrap();

    // Never claimed (claimed_at IS NULL) ⇒ stranded by a crash ⇒ rescued.
    assert_eq!(db.reset_triaging(600).unwrap(), 1);
    assert_eq!(db.get_message(id).unwrap().unwrap().status, "new");
    assert_eq!(db.reset_triaging(600).unwrap(), 0);
}

#[test]
fn claiming_is_atomic_so_two_cycles_cannot_take_the_same_message() {
    let (db, _dir) = fresh_db();
    for i in 0..3 {
        db.insert_message(&sample_new_message(&format!("m{i}")))
            .unwrap();
    }

    let first = db.claim_new_messages(2).unwrap();
    let second = db.claim_new_messages(2).unwrap();

    assert_eq!(first.len(), 2, "first cycle takes two");
    assert_eq!(second.len(), 1, "second cycle can only get what is left");
    let ids: Vec<i64> = first.iter().chain(second.iter()).map(|m| m.id).collect();
    let unique: std::collections::HashSet<i64> = ids.iter().copied().collect();
    assert_eq!(
        ids.len(),
        unique.len(),
        "no message may be handed to two cycles"
    );

    for m in first.iter().chain(second.iter()) {
        assert_eq!(m.status, "triaging");
        assert!(m.claimed_at.is_some(), "a claim must be stamped");
        assert_eq!(m.attempts, 1);
    }
    assert!(
        db.claim_new_messages(5).unwrap().is_empty(),
        "queue is drained"
    );
}

#[test]
fn a_freshly_claimed_message_is_not_yanked_back_by_a_sibling_cycle() {
    let (db, _dir) = fresh_db();
    db.insert_message(&sample_new_message("m")).unwrap();
    let claimed = db.claim_new_messages(1).unwrap();
    assert_eq!(claimed.len(), 1);

    // Another cycle starting right now must leave the in-flight row alone…
    assert_eq!(
        db.reset_triaging(600).unwrap(),
        0,
        "an in-flight claim must survive"
    );
    // …but a genuinely stale one comes back. (Timestamps are millisecond
    // resolution, so step past the claim instant before asking.)
    std::thread::sleep(std::time::Duration::from_millis(5));
    assert_eq!(db.reset_triaging(0).unwrap(), 1);
    assert_eq!(
        db.get_message(claimed[0].id).unwrap().unwrap().status,
        "new"
    );
}

#[test]
fn decisions_attach_to_a_message_and_the_latest_one_wins() {
    let (db, _dir) = fresh_db();
    let (id, _) = db.insert_message(&sample_new_message("a")).unwrap();
    let id = id.unwrap();

    let first = db
        .insert_decision(&NewDecision {
            message_id: id,
            tier: "L0".into(),
            kind: Some("bug".into()),
            severity: Some("p1".into()),
            summary: Some("one".into()),
            needs_human: true,
            actions: Some(json!([{ "type": "reply", "detail": "x" }])),
            evidence: Some(json!(["a.ts:1"])),
            confidence: Some(0.5),
            cost_usd: Some(0.01),
            status: "pending".into(),
            ..Default::default()
        })
        .unwrap();
    let second = db
        .insert_decision(&NewDecision {
            message_id: id,
            tier: "L1".into(),
            kind: Some("question".into()),
            severity: Some("p3".into()),
            summary: Some("two".into()),
            needs_human: false,
            confidence: Some(0.9),
            cost_usd: Some(0.02),
            status: "auto".into(),
            ..Default::default()
        })
        .unwrap();

    let latest = db.latest_decision_for(id).unwrap().unwrap();
    assert_eq!(latest.id, second);
    assert_eq!(latest.summary.as_deref(), Some("two"));
    assert!(!latest.needs_human);

    let firstrow = db.get_decision(first).unwrap().unwrap();
    assert_eq!(
        firstrow.actions_json(),
        json!([{ "type": "reply", "detail": "x" }])
    );
    assert_eq!(firstrow.evidence.as_deref(), Some("[\"a.ts:1\"]"));

    let c = db.counts().unwrap();
    assert!(
        (c.cost_usd_total - 0.03).abs() < 1e-9,
        "cost total was {}",
        c.cost_usd_total
    );
}

#[test]
fn outbox_retries_then_dead_letters_after_max_attempts() {
    let (db, _dir) = fresh_db();
    let (id, _) = db.insert_message(&sample_new_message("a")).unwrap();
    let oid = db
        .enqueue_outbox(&NewOutbox {
            message_id: id,
            channel: "github".into(),
            target: "dipgle/tfl5#1".into(),
            body: "hi".into(),
            ..Default::default()
        })
        .unwrap();

    for i in 1..=4 {
        let (attempts, status) = db.mark_outbox_failed(oid, &format!("net {i}"), 5).unwrap();
        assert_eq!(attempts, i);
        assert_eq!(status, "queued", "must stay retryable below the cap");
        assert_eq!(db.queued_outbox(10).unwrap().len(), 1);
    }
    let (_, status) = db.mark_outbox_failed(oid, "net 5", 5).unwrap();
    assert_eq!(status, "failed");
    assert!(
        db.queued_outbox(10).unwrap().is_empty(),
        "a failed row must stop being picked up"
    );

    db.dead_letter(
        Some("github"),
        Some(&oid.to_string()),
        "outbound",
        None,
        "gave up",
    )
    .unwrap();
    assert_eq!(db.counts().unwrap().dead_letter, 1);
}

#[test]
fn mark_outbox_sent_clears_the_error() {
    let (db, _dir) = fresh_db();
    let (id, _) = db.insert_message(&sample_new_message("a")).unwrap();
    let oid = db
        .enqueue_outbox(&NewOutbox {
            message_id: id,
            channel: "notify".into(),
            target: "local".into(),
            body: "hi".into(),
            ..Default::default()
        })
        .unwrap();
    db.mark_outbox_failed(oid, "temporary", 5).unwrap();
    db.mark_outbox_sent(oid).unwrap();

    let row: (String, Option<String>, i64, Option<String>) = db
        .conn
        .query_row(
            "SELECT status, last_error, attempts, sent_at FROM outbox WHERE id = ?1",
            [oid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(row.0, "sent");
    assert_eq!(row.1, None);
    assert_eq!(row.2, 2);
    assert!(row.3.is_some());
}

#[test]
fn rejecting_a_decision_cancels_only_its_queued_rows() {
    let (db, _dir) = fresh_db();
    let (id, _) = db.insert_message(&sample_new_message("a")).unwrap();
    let mid = id.unwrap();
    let did = db
        .insert_decision(&NewDecision {
            message_id: mid,
            tier: "L1".into(),
            kind: Some("question".into()),
            severity: Some("p3".into()),
            summary: Some("s".into()),
            needs_human: false,
            confidence: Some(0.9),
            status: "pending".into(),
            ..Default::default()
        })
        .unwrap();
    let mine = db
        .enqueue_outbox(&NewOutbox {
            decision_id: Some(did),
            message_id: Some(mid),
            channel: "github".into(),
            target: "dipgle/tfl5#1".into(),
            body: "reply".into(),
            ..Default::default()
        })
        .unwrap();
    let other = db
        .enqueue_outbox(&NewOutbox {
            message_id: Some(mid),
            channel: "notify".into(),
            target: "local".into(),
            body: "brief".into(),
            ..Default::default()
        })
        .unwrap();

    db.cancel_outbox_for(did).unwrap();
    let status = |id: i64| -> String {
        db.conn
            .query_row("SELECT status FROM outbox WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap()
    };
    assert_eq!(status(mine), "cancelled");
    assert_eq!(status(other), "queued");
}

#[test]
fn cursors_round_trip_and_runs_record_health() {
    let (db, _dir) = fresh_db();

    assert_eq!(db.get_cursor("github:since").unwrap(), None);
    db.set_cursor("github:since", "2026-07-26T00:00:00Z")
        .unwrap();
    db.set_cursor("github:since", "2026-07-27T00:00:00Z")
        .unwrap();
    assert_eq!(
        db.get_cursor("github:since").unwrap().as_deref(),
        Some("2026-07-27T00:00:00Z")
    );
    assert_eq!(db.all_cursors().unwrap().len(), 1);

    let ok_run = db.start_run("github", "poll").unwrap();
    db.finish_run(
        ok_run,
        RunFinish {
            ok: true,
            n_new: 3,
            err: None,
            skipped: None,
        },
    )
    .unwrap();
    let bad_run = db.start_run("email", "poll").unwrap();
    db.finish_run(
        bad_run,
        RunFinish {
            ok: false,
            n_new: 0,
            err: Some("HTTP 401".into()),
            skipped: None,
        },
    )
    .unwrap();
    let skip_run = db.start_run("telegram", "poll").unwrap();
    db.finish_run(
        skip_run,
        RunFinish {
            ok: true,
            n_new: 0,
            err: None,
            skipped: Some("HUB_TELEGRAM_TOKEN not set".into()),
        },
    )
    .unwrap();

    let runs = db.last_runs(5).unwrap();
    assert_eq!(runs.len(), 3);
    let by = |name: &str| runs.iter().find(|r| r.adapter == name).unwrap().clone();
    assert_eq!(by("github").n_new, Some(3));
    assert_eq!(by("email").ok, Some(0));
    assert!(by("email").err.unwrap().contains("401"));
    assert!(
        by("telegram").skipped.unwrap().contains("not set"),
        "a credential skip must be recorded, not silent"
    );
}

#[test]
fn the_owners_own_spend_is_counted_but_never_used_to_refuse() {
    // THE BOUNDARY, and it points the opposite way from the robot's. Pressing
    // "hỏi bên lề" on a phone is the owner working — the same work at the same
    // price as typing it in the terminal, where no daily ceiling exists.
    //
    // A ceiling here was built and thrown out the same day (2026-08-08). The
    // books said why: of $2.98 triaged that day, $2.24 belonged to the github
    // and devlog branches that had already been deleted, so the ceiling was
    // mostly the ghost of a dead product — reaching out to block the owner's
    // own hand. What survives is the counting, so the price can be shown next
    // to the button that spends it.
    let (db, _dir) = fresh_db();
    assert_eq!(hub::pipeline::owner_budget_state(&db).spent_usd, 0.0);

    // Well past any ceiling hub used to enforce: it must still only REPORT.
    db.record_spend("handover", "s-1", 1.70, "→ s-2").unwrap();
    db.record_spend("aside", "s-1", 8.00, "→ s-3").unwrap();
    let state = hub::pipeline::owner_budget_state(&db);
    assert!(
        (state.spent_usd - 9.70).abs() < 1e-9,
        "both owner-initiated calls must be counted, got {}",
        state.spent_usd
    );
}

#[test]
fn a_side_question_and_a_handover_share_one_set_of_books() {
    // Two spending paths, one ceiling. If `aside` were counted anywhere else,
    // the owner's budget would be a budget for one feature rather than for the
    // owner — which is how the inbox branch quietly ate $2.24 of a $3 day.
    let (db, _dir) = fresh_db();
    db.record_spend("handover", "s-1", 0.40, "→ s-2").unwrap();
    db.record_spend("aside", "s-1", 0.30, "→ s-3").unwrap();

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let spent = db.owner_cost_on_day(&today).unwrap();
    assert!(
        (spent - 0.70).abs() < 1e-9,
        "both owner-initiated calls must land in the same day's total, got {spent}"
    );
}
