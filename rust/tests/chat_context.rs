//! Conversation context in a chat room.
//!
//! The complaint that started this (2026-08-07): "phải nhắc quá nhiều thông tin
//! trong nội dung chat để biết đang nói về dự án nào". It was accurate. A chat
//! line carries no repo, `routing` is empty and the project registry matches on
//! repos, so `resolve_project` could only recognise a project when the person
//! literally typed "tfl5:" or "[tfl5]" at the start of EVERY message — and
//! every chat row in the store had `project = NULL`.
//!
//! What must hold now: the room remembers what it is about, within a bounded
//! window, and an explicit pin outranks the history.

mod common;

use common::fresh_db;
use hub::db::NewMessage;

const ROOM: &str = "tfl5:a-65dd60d3:hub";

fn chat_msg(external_id: &str, body: &str, project: Option<&str>, at: &str) -> NewMessage {
    NewMessage {
        source: "tfl5".into(),
        external_id: external_id.into(),
        thread_key: Some(ROOM.into()),
        project: project.map(str::to_string),
        sender: Some("tfl5:alice".into()),
        sender_trust: Some("trusted".into()),
        subject: Some(body.chars().take(120).collect()),
        body: Some(body.into()),
        url: None,
        received_at: Some(at.into()),
        raw: Some(serde_json::json!({ "room": "hub" })),
    }
}

fn hours_ago(h: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::hours(h))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[test]
fn the_room_remembers_the_project_it_was_last_told_about() {
    let (db, _dir) = fresh_db();
    db.insert_message(&chat_msg(
        "c1",
        "tfl5: CI đỏ ở main",
        Some("tfl5"),
        &hours_ago(1),
    ))
    .unwrap();

    let since = hours_ago(12);
    assert_eq!(
        db.last_project_for_thread(ROOM, &since).unwrap().as_deref(),
        Some("tfl5"),
        "câu sau trong cùng phòng phải kế thừa dự án đã nhắc"
    );
}

#[test]
fn yesterdays_topic_does_not_colour_todays_question() {
    let (db, _dir) = fresh_db();
    let mut old = chat_msg("c-old", "sdvi: cutover ACL", Some("sdvi"), &hours_ago(40));
    // `ingested_at` is what the lookup filters on, and it is set by the
    // insert — so age the row the way the store would see it.
    old.received_at = Some(hours_ago(40));
    let (id, _) = db.insert_message(&old).unwrap();
    db.conn
        .execute(
            "UPDATE messages SET ingested_at = ?1 WHERE id = ?2",
            rusqlite::params![hours_ago(40), id.unwrap()],
        )
        .unwrap();

    let since = hours_ago(12);
    assert_eq!(
        db.last_project_for_thread(ROOM, &since).unwrap(),
        None,
        "quá cửa sổ 12h thì KHÔNG được kế thừa — chủ đề hôm qua không phải chủ đề hôm nay"
    );
}

#[test]
fn the_newest_mention_wins() {
    let (db, _dir) = fresh_db();
    db.insert_message(&chat_msg("c1", "sdvi: ...", Some("sdvi"), &hours_ago(3)))
        .unwrap();
    db.insert_message(&chat_msg("c2", "tfl5: ...", Some("tfl5"), &hours_ago(1)))
        .unwrap();

    assert_eq!(
        db.last_project_for_thread(ROOM, &hours_ago(12))
            .unwrap()
            .as_deref(),
        Some("tfl5"),
        "đổi chủ đề thì lấy cái mới nhất"
    );
}

#[test]
fn messages_without_a_project_do_not_erase_the_context() {
    let (db, _dir) = fresh_db();
    db.insert_message(&chat_msg("c1", "tfl5: CI đỏ", Some("tfl5"), &hours_ago(2)))
        .unwrap();
    db.insert_message(&chat_msg("c2", "còn cái kia thì sao", None, &hours_ago(1)))
        .unwrap();

    assert_eq!(
        db.last_project_for_thread(ROOM, &hours_ago(12))
            .unwrap()
            .as_deref(),
        Some("tfl5"),
        "một câu không nêu dự án không được xoá ngữ cảnh của phòng"
    );
}

#[test]
fn a_different_room_is_a_different_conversation() {
    let (db, _dir) = fresh_db();
    db.insert_message(&chat_msg("c1", "tfl5: ...", Some("tfl5"), &hours_ago(1)))
        .unwrap();

    assert_eq!(
        db.last_project_for_thread("tfl5:a-other:lobby", &hours_ago(12))
            .unwrap(),
        None,
        "ngữ cảnh không được rò từ phòng này sang phòng khác"
    );
}

#[test]
fn the_pin_is_stored_per_thread() {
    let (db, _dir) = fresh_db();
    let key = hub::pipeline::project_pin_key(ROOM);
    assert_eq!(db.get_cursor(&key).unwrap(), None);
    db.set_cursor(&key, "tfl5").unwrap();
    assert_eq!(db.get_cursor(&key).unwrap().as_deref(), Some("tfl5"));
    // Clearing writes an empty value rather than deleting, so "was pinned and
    // then unpinned" stays distinguishable from "never pinned".
    db.set_cursor(&key, "").unwrap();
    assert_eq!(db.get_cursor(&key).unwrap().as_deref(), Some(""));
    assert_eq!(
        db.get_cursor(&hub::pipeline::project_pin_key("tfl5:a-other:lobby"))
            .unwrap(),
        None
    );
}
