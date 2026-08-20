mod common;

use common::fresh_db;
use huba::db::RunFinish;

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
    assert_eq!(huba::pipeline::owner_budget_state(&db).spent_usd, 0.0);

    // Well past any ceiling huba used to enforce: it must still only REPORT.
    db.record_spend("handover", "s-1", 1.70, "→ s-2").unwrap();
    db.record_spend("aside", "s-1", 8.00, "→ s-3").unwrap();
    let state = huba::pipeline::owner_budget_state(&db);
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

/// `cursor_or_log` phân biệt "chưa đặt" với "đọc hỏng" — và chỉ im ở vế đầu.
///
/// Vì sao có hàm này: `get_cursor` trả `Result<Option<_>>`, và **12 chỗ** trong
/// mã đã gộp `Err` với `Ok(None)` bằng `.ok().flatten()` hoặc `match … _ =>`.
/// Nhìn từ điện thoại, hậu quả là bấm ⏹ Dừng trên một phiên đang mở thì huba trả
/// lời *"chưa theo phiên nào"* — đúng câu nó nói khi chưa ai chọn gì — mà không
/// dòng log nào cho biết cơ sở dữ liệu vừa không đọc được.
///
/// Vế `Err` không dựng được từ đây (không có API công khai nào phá được bảng
/// đang mở), nên test này ghim vế "chưa đặt" và vế "đã đặt"; phần còn lại là ba
/// dòng: log rồi trả `None`.
#[test]
fn a_missing_cursor_is_silent_but_a_broken_read_is_not() {
    let (db, _dir) = fresh_db();

    // Chưa đặt: im lặng, không log — đây là chuyện thường.
    assert_eq!(db.cursor_or_log("focus:session"), None);

    db.set_cursor("focus:session", "abc-123").unwrap();
    assert_eq!(
        db.cursor_or_log("focus:session").as_deref(),
        Some("abc-123")
    );

    // Đặt rỗng = "thôi theo phiên nào cả": vẫn là một giá trị ĐÃ ĐẶT, chỗ gọi
    // tự lọc bằng `.filter(|s| !s.is_empty())` chứ hàm này không đoán hộ.
    db.set_cursor("focus:session", "").unwrap();
    assert_eq!(db.cursor_or_log("focus:session").as_deref(), Some(""));
}

/// Bước nâng cấp 4 dọn bốn bảng của sản phẩm hộp thư đã xoá — và CHỈ chúng.
///
/// Làm bằng bước nâng cấp chứ không phải một lệnh gõ tay: nằm trong mã, có
/// test, có log, chạy đúng một lần trên mọi máy, và ai đọc lịch sử cũng thấy vì
/// sao. Hà chốt 2026-08-10 sau khi đếm được 379 dòng chết không truy vấn nào
/// chạm tới, dừng đúng ngày nhánh hộp thư bị xoá.
///
/// Fixture dựng bằng chính `sqlite3` — đúng cách một người vận hành làm, và
/// cũng là đường duy nhất ở đây vì bộ test không có rusqlite.
#[test]
fn schema_step_4_drops_the_dead_inbox_tables_and_nothing_else() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.sqlite");

    // Một cơ sở dữ liệu ĐỜI CŨ: có bảng hộp thư, có dữ liệu, phiên bản 3.
    let seed = "
      CREATE TABLE schema_meta (k TEXT PRIMARY KEY, v TEXT NOT NULL);
      INSERT INTO schema_meta VALUES ('version','3');
      CREATE TABLE messages (id INTEGER PRIMARY KEY, body TEXT);
      INSERT INTO messages (body) VALUES ('tin cũ'),('tin thường');
      CREATE TABLE outbox (id INTEGER PRIMARY KEY);
      INSERT INTO outbox DEFAULT VALUES;
      CREATE TABLE decisions (id INTEGER PRIMARY KEY);
      CREATE TABLE dead_letter (id INTEGER PRIMARY KEY);
      CREATE TABLE cursors (k TEXT PRIMARY KEY, v TEXT, updated_at TEXT NOT NULL);
      INSERT INTO cursors VALUES ('focus:session','abc','2026-08-10T00:00:00Z');
    ";
    let out = Command::new("sqlite3")
        .arg(&path)
        .arg(seed)
        .output()
        .unwrap();
    assert!(out.status.success(), "dựng fixture hỏng: {out:?}");

    // Mở bằng chính huba — bước nâng cấp chạy ở đây.
    let db = huba::db::Db::open(&path).unwrap();
    // Dữ liệu SỐNG phải còn nguyên: dọn nhầm thứ đang dùng thì hỏng nặng hơn
    // hẳn việc để lại thứ đã chết.
    assert_eq!(db.cursor_or_log("focus:session").as_deref(), Some("abc"));
    drop(db);

    let names = Command::new("sqlite3")
        .arg(&path)
        .arg("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .output()
        .unwrap();
    let names = String::from_utf8_lossy(&names.stdout);
    for gone in ["messages", "outbox", "decisions", "dead_letter"] {
        assert!(
            !names.split('\n').any(|l| l.trim() == gone),
            "còn {gone}: {names}"
        );
    }
    for kept in ["cursors", "runs", "spend", "schema_meta"] {
        assert!(
            names.split('\n').any(|l| l.trim() == kept),
            "mất {kept}: {names}"
        );
    }

    // Chạy lại lần nữa: không nổ, không làm gì thêm (phiên bản đã là 4).
    let db = huba::db::Db::open(&path).unwrap();
    assert_eq!(db.cursor_or_log("focus:session").as_deref(), Some("abc"));
}
