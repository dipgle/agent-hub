//! Nguồn của danh sách phiên: **sổ sách của chính CLI**, không phải `claude agents`.
//!
//! Đổi nguồn 2026-08-15 (Hà: *"tôi muốn mọi thông tin khi đi qua hub phải là
//! realtime chứ không phải đọc lịch sử"*). Mọi ca ở đây lấy hình dạng từ tệp
//! THẬT trên máy này — `~/.claude/sessions/<pid>.json` và
//! `~/.claude/jobs/<id>/state.json` — chứ không phải tôi nghĩ ra.

use hub::sessions::{book_updated_at, is_claude_process, list_account_books};
use serde_json::json;
use std::fs;
use tempfile::TempDir;

/// Dựng một thư mục cấu hình giả với đúng hai ngăn CLI dùng.
fn book_dir() -> TempDir {
    let d = TempDir::new().expect("tempdir");
    fs::create_dir_all(d.path().join("sessions")).unwrap();
    fs::create_dir_all(d.path().join("jobs")).unwrap();
    d
}

fn write_session(dir: &TempDir, pid: i64, body: serde_json::Value) {
    fs::write(
        dir.path().join("sessions").join(format!("{pid}.json")),
        body.to_string(),
    )
    .unwrap();
}

fn write_job(dir: &TempDir, short: &str, body: serde_json::Value) {
    let d = dir.path().join("jobs").join(short);
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("state.json"), body.to_string()).unwrap();
}

/// Nguyên văn một tệp thật trên máy này, 2026-08-15 (đã bỏ token).
fn real_interactive() -> serde_json::Value {
    json!({
        "pid": 10716,
        "sessionId": "bab47095-40c0-416e-aa87-dd4a463ac460",
        "cwd": "/Users/hanguyen/projects",
        "startedAt": 1786758022947i64,
        "procStart": "Sat Aug 15 01:40:22 2026",
        "version": "2.1.228",
        "peerProtocol": 1,
        "kind": "interactive",
        "entrypoint": "cli",
        "messagingSocketPath": "/tmp/cc-socks/10716.sock",
        "name": "projects-35",
        "nameSource": "derived",
        "status": "idle",
        "updatedAt": 1786804403744i64,
        "statusUpdatedAt": 1786804403744i64
    })
}

#[test]
fn an_interactive_book_carries_every_field_the_snapshot_reads() {
    let d = book_dir();
    write_session(&d, 10716, real_interactive());

    let rows = list_account_books(d.path()).expect("đọc được sổ");
    assert_eq!(rows.len(), 1, "một tệp = một hàng");
    let r = &rows[0];
    // Đây đúng là bộ trường `snapshot()` đọc ra khỏi mỗi hàng — thiếu một cái
    // là một cột trống trên điện thoại, không phải một lỗi biên dịch.
    assert_eq!(r["sessionId"], "bab47095-40c0-416e-aa87-dd4a463ac460");
    assert_eq!(r["cwd"], "/Users/hanguyen/projects");
    assert_eq!(r["name"], "projects-35");
    assert_eq!(r["kind"], "interactive");
    assert_eq!(r["pid"], 10716);
    assert_eq!(r["status"], "idle");
    assert_eq!(r["startedAt"], 1786758022947i64);
}

#[test]
fn a_book_without_a_session_id_is_dropped_not_shown_as_a_nameless_row() {
    let d = book_dir();
    write_session(&d, 999, json!({ "pid": 999, "kind": "interactive" }));
    write_session(&d, 10716, real_interactive());

    let rows = list_account_books(d.path()).unwrap();
    assert_eq!(rows.len(), 1, "hàng không có id thì không địa chỉ hoá được");
    assert_eq!(rows[0]["pid"], 10716);
}

#[test]
fn one_unreadable_book_must_not_take_the_whole_account_down() {
    let d = book_dir();
    fs::write(
        d.path().join("sessions").join("777.json"),
        "{ đây không phải JSON",
    )
    .unwrap();
    write_session(&d, 10716, real_interactive());

    // Luật 11b: một phép đo hỏng không phải một sự thật về thế giới — nhưng nó
    // cũng không được kéo theo những phép đo CÒN chạy. Hàng hỏng đi vào log,
    // hàng lành vẫn lên danh sách.
    let rows = list_account_books(d.path()).expect("một tệp hỏng không làm hỏng cả tài khoản");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["pid"], 10716);
}

#[test]
fn a_missing_sessions_drawer_is_an_error_not_an_empty_machine() {
    let d = TempDir::new().unwrap();
    // KHÔNG tạo `sessions/`. Trả rỗng ở đây là khai "máy không chạy phiên nào",
    // và cái loa đọc câu ấy thành "mọi phiên vừa tắt" (luật 11b, ba tin sai
    // ngày 12/08). Nên phải là lỗi, để tài khoản vào `blind`.
    assert!(
        list_account_books(d.path()).is_err(),
        "không có ngăn sổ ≠ không có phiên nào"
    );
}

#[test]
fn a_finished_job_is_not_a_live_background_row() {
    let d = book_dir();
    // Ba trạng thái ĐÃ ĐO trên máy: `claude agents` khai `blocked`, im với
    // `done`/`stopped` (acc1, 60 thư mục) và im với `failed` (acc2 → `[]`).
    for (short, state) in [
        ("aaaa1111", "done"),
        ("bbbb2222", "stopped"),
        ("cccc3333", "failed"),
    ] {
        write_job(
            &d,
            short,
            json!({ "state": state, "sessionId": format!("{short}-0000-0000-0000-000000000000"),
                    "name": "xong rồi", "cwd": "/Users/hanguyen/projects",
                    "createdAt": "2026-08-11T06:29:46.209Z", "updatedAt": "2026-08-11T06:29:46.306Z" }),
        );
    }
    write_job(
        &d,
        "dddd4444",
        json!({ "state": "blocked", "sessionId": "c19b6a82-4038-41bb-b9b0-586699a54458",
                "name": "merge xem init-project", "cwd": "/Users/hanguyen/projects",
                "createdAt": "2026-08-13T08:16:34.001Z", "updatedAt": "2026-08-13T08:30:33.340Z" }),
    );

    let rows = list_account_books(d.path()).unwrap();
    assert_eq!(
        rows.len(),
        1,
        "chỉ việc chưa kết thúc mới là hàng đang sống"
    );
    let r = &rows[0];
    assert_eq!(r["sessionId"], "c19b6a82-4038-41bb-b9b0-586699a54458");
    assert_eq!(r["kind"], "background");
    assert_eq!(r["pid"], 0, "việc nền không có tiến trình để gõ vào");
    assert_eq!(r["name"], "merge xem init-project");
    // Mốc để `drop_stale_dead` chấm tuổi. Thiếu nó thì hàng chết nằm lại MÃI —
    // luật của hàm ấy là "không biết thì đừng bỏ".
    assert!(
        book_updated_at(r).is_some(),
        "hàng nền phải mang mốc thời gian"
    );
}

#[test]
fn an_unknown_job_state_is_shown_not_hidden() {
    let d = book_dir();
    write_job(
        &d,
        "eeee5555",
        json!({ "state": "một-trạng-thái-chưa-ai-thấy", "sessionId": "eeee5555-0000-0000-0000-000000000000",
                "name": "lạ", "cwd": "/Users/hanguyen/projects",
                "createdAt": "2026-08-15T08:16:34.001Z", "updatedAt": "2026-08-15T08:30:33.340Z" }),
    );
    // Danh sách tên trạng thái đã thiếu một lần rồi (`failed`). Nên cửa này mở
    // theo hướng NÓI RA: giấu thứ mình chưa hiểu là cách một danh sách bắt đầu
    // nói dối, còn hiện nhầm một hàng nguội thì `drop_stale_dead` dọn theo tuổi.
    let rows = list_account_books(d.path()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["kind"], "background");
}

#[test]
fn updated_at_reads_both_shapes_the_cli_writes() {
    // `sessions/<pid>.json` ghi mili-giây; `jobs/<id>/state.json` ghi RFC 3339.
    assert_eq!(
        book_updated_at(&json!({ "updatedAt": 1786804403744i64 })).as_deref(),
        Some("2026-08-15T14:33:23Z")
    );
    assert_eq!(
        book_updated_at(&json!({ "updatedAt": "2026-08-13T08:30:33.340Z" })).as_deref(),
        Some("2026-08-13T08:30:33Z")
    );
    assert_eq!(book_updated_at(&json!({})), None);
    assert_eq!(
        book_updated_at(&json!({ "updatedAt": "không phải giờ" })),
        None
    );
}

#[test]
fn a_recycled_pid_must_not_pass_as_a_live_session() {
    // Sổ `sessions/<pid>.json` chỉ biến mất khi CLI thoát tử tế. Bị `kill -9`
    // thì tệp ở lại, macOS cấp lại con số ấy cho tiến trình khác, và hàng ấy
    // đọc ra "còn sống" kèm **tty của người khác** — mà `/type` gõ theo tty.
    assert!(is_claude_process("claude --permission-mode auto"));
    assert!(is_claude_process("claude"));
    assert!(is_claude_process(
        "/Users/hanguyen/.vscode/extensions/anthropic.claude-code/resources/native/claude"
    ));
    assert!(is_claude_process(
        "node /Users/hanguyen/.npm-global/lib/node_modules/@anthropic-ai/claude-code/cli.js"
    ));

    assert!(!is_claude_process("/usr/sbin/cupsd -l"));
    assert!(!is_claude_process("zsh"));
    assert!(!is_claude_process(""));
    assert!(
        !is_claude_process("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        "một tiến trình bất kỳ giành được pid cũ thì KHÔNG được mượn tty của phiên đã chết"
    );
}
