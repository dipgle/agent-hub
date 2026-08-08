mod common;

use common::fresh_db;
use hub::db::RunFinish;

#[test]
fn cursors_round_trip_and_runs_record_health() {
    let (db, _dir) = fresh_db();

    assert_eq!(db.get_cursor("tfl5:since").unwrap(), None);
    db.set_cursor("tfl5:since", "2026-07-26T00:00:00Z").unwrap();
    db.set_cursor("tfl5:since", "2026-07-27T00:00:00Z").unwrap();
    assert_eq!(
        db.get_cursor("tfl5:since").unwrap().as_deref(),
        Some("2026-07-27T00:00:00Z")
    );
    assert_eq!(db.all_cursors().unwrap().len(), 1);

    let ok_run = db.start_run("tfl5", "poll").unwrap();
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
    let bad_run = db.start_run("tfl5-2", "poll").unwrap();
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
    let skip_run = db.start_run("tfl5-3", "poll").unwrap();
    db.finish_run(
        skip_run,
        RunFinish {
            ok: true,
            n_new: 0,
            err: None,
            skipped: Some("HUB_TFL5_PASSWORD not set".into()),
        },
    )
    .unwrap();

    let runs = db.last_runs(5).unwrap();
    assert_eq!(runs.len(), 3);
    let by = |name: &str| runs.iter().find(|r| r.adapter == name).unwrap().clone();
    assert_eq!(by("tfl5").n_new, Some(3));
    assert_eq!(by("tfl5-2").ok, Some(0));
    assert!(by("tfl5-2").err.unwrap().contains("401"));
    assert!(
        by("tfl5-3").skipped.unwrap().contains("not set"),
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
