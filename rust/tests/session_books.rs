//! Nguồn của danh sách phiên: **sổ sách của chính CLI**, không phải `claude agents`.
//!
//! Đổi nguồn 2026-08-15 (Hà: *"tôi muốn mọi thông tin khi đi qua huba phải là
//! realtime chứ không phải đọc lịch sử"*). Mọi ca ở đây lấy hình dạng từ tệp
//! THẬT trên máy này — `~/.claude/sessions/<pid>.json` và
//! `~/.claude/jobs/<id>/state.json` — chứ không phải tôi nghĩ ra.

use huba::sessions::{book_updated_at, is_claude_process, list_account_books};
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

/// Nguyên văn `~/.claude/sessions/91890.json` lúc 2026-09-05 (bỏ id cầu nối).
///
/// Đây là hàng phá vỡ câu "sessions/ = tương tác": `kind:"bg"` + `jobId`.
fn real_background_session() -> serde_json::Value {
    json!({
        "pid": 91890,
        "sessionId": "167252e2-b2fb-4776-86a1-ba9faf8d14c0",
        "cwd": "/Users/hanguyen/projects",
        "startedAt": 1788581041472i64,
        "procStart": "Sat Sep  5 04:04:01 2026",
        "version": "2.1.228",
        "peerProtocol": 1,
        "kind": "bg",
        "entrypoint": "cli",
        "messagingSocketPath": "/tmp/cc-socks/91890.sock",
        "name": "Test multiple Facebook bots with real accounts",
        "jobId": "167252e2",
        "status": "busy",
        "updatedAt": 1788595666827i64,
        "statusUpdatedAt": 1788595666827i64
    })
}

/// Nguyên văn phần `jobs/167252e2/state.json` mà hàm này đọc, cùng lúc ấy.
fn real_background_job() -> serde_json::Value {
    json!({
        "state": "working",
        "detail": "deployed; testing mail.dipgle.com live",
        "sessionId": "167252e2-b2fb-4776-86a1-ba9faf8d14c0",
        "name": "Test multiple Facebook bots with real accounts",
        "cwd": "/Users/hanguyen/projects",
        "createdAt": "2026-09-04T10:57:02.411Z",
        "updatedAt": "2026-09-05T08:39:49.458Z"
    })
}

#[test]
fn one_background_session_in_both_drawers_is_one_row_not_two() {
    // 🔴 Hà 2026-09-05: *"tại sao danh sách lại có 2 mã phiên giống nhau"*.
    // Đo lúc ấy: `huba sessions --json` ra 11 hàng cho 10 phiên — `167252e2`
    // hai lần, vì CLI 2.1.228 ghi phiên nền vào CẢ HAI ngăn sổ.
    let d = book_dir();
    write_session(&d, 91890, real_background_session());
    write_job(&d, "167252e2", real_background_job());

    let rows = list_account_books(d.path()).unwrap();
    assert_eq!(rows.len(), 1, "hai cuốn sổ, MỘT phiên — phải ra một hàng");
    let r = &rows[0];

    // Hàng thắng phải là hàng mang PID THẬT: mất nó thì `host_of` đọc `pid:0`
    // thành "dead" và huba khai một phiên đang chạy là đã tắt.
    assert_eq!(r["pid"], 91890);
    assert_eq!(r["status"], "busy");
    assert_eq!(r["startedAt"], 1788581041472i64);
    // …và phải nói được nó là phiên NỀN bằng chữ huba đọc, không phải chữ
    // `"bg"` của CLI: `/stop` (`stop_background`) gác đúng trên chữ này.
    assert_eq!(r["kind"], "background");
    // Thứ chỉ sổ VIỆC có, không được rơi mất khi gộp.
    assert_eq!(r["state"], "working");
    // Mốc mới hơn thắng: sổ phiên dừng ở 08:07:46Z, sổ việc 08:39:49Z.
    assert_eq!(
        book_updated_at(r).as_deref(),
        Some("2026-09-05T08:39:49Z"),
        "hai cuốn nhích theo hai nhịp — lấy cuốn động sau"
    );
}

#[test]
fn the_fold_joins_only_rows_that_are_the_same_session() {
    // ĐỐI CHỨNG NGƯỢC cho bài trên: một phép gộp gộp bừa cũng ra "hết trùng".
    // Hai phiên KHÁC nhau ở hai ngăn phải ở lại là hai hàng.
    let d = book_dir();
    write_session(&d, 91890, real_background_session());
    write_job(
        &d,
        "c19b6a82",
        json!({ "state": "blocked", "sessionId": "c19b6a82-4038-41bb-b9b0-586699a54458",
                "name": "merge xem init-project", "cwd": "/Users/hanguyen/projects",
                "createdAt": "2026-08-13T08:16:34.001Z", "updatedAt": "2026-08-13T08:30:33.340Z" }),
    );

    let rows = list_account_books(d.path()).unwrap();
    assert_eq!(rows.len(), 2, "hai phiên khác nhau thì KHÔNG được gộp");
    let bg = rows
        .iter()
        .find(|r| r["sessionId"] == "167252e2-b2fb-4776-86a1-ba9faf8d14c0")
        .expect("hàng phiên nền còn nguyên");
    assert_eq!(bg["pid"], 91890);
    // Việc nền không có anh em ở sổ phiên thì vẫn là hàng `pid:0` như cũ.
    let job = rows
        .iter()
        .find(|r| r["sessionId"] == "c19b6a82-4038-41bb-b9b0-586699a54458")
        .expect("hàng việc nền còn nguyên");
    assert_eq!(job["pid"], 0);
    assert_eq!(job["state"], "blocked");
}

#[test]
fn the_fold_carries_state_because_the_blocked_trap_is_read_from_it() {
    // 🔴 Gộp bằng cách VỨT hàng sổ việc đi thì hết trùng — và giết luôn một cửa
    // gác: `sessions::start_background` chờ tới 14 giây rồi hỏi
    // `row.state == "blocked"` để bắt ca phiên nền chết đứng ở hộp duyệt MCP mà
    // không ai bấm hộ được. Chữ `blocked` CHỈ có ở sổ việc. Mất nó thì huba báo
    // "🚀 đã mở phiên" cho một phiên không bao giờ chạy — đúng cái tin xanh cho
    // việc chưa chạy mà repo này cấm.
    let d = book_dir();
    let mut sess = real_background_session();
    sess["status"] = json!("idle");
    write_session(&d, 91890, sess);
    let mut job = real_background_job();
    job["state"] = json!("blocked");
    write_job(&d, "167252e2", job);

    let rows = list_account_books(d.path()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["state"], "blocked", "cửa gác đọc trường này");
    assert_eq!(rows[0]["pid"], 91890);
}

#[test]
fn a_bg_row_with_no_job_file_is_still_a_background_row() {
    // Sổ việc có thể đã bị dọn (`claude stop` xong, hoặc thư mục `jobs/` chưa
    // sinh) trong khi tiến trình nền vẫn chạy. Chữ `"bg"` đi lọt tới đường dưới
    // là hỏng CÂM: nút `/stop` vẫn hiện, bấm vào thì `stop_background` trả lời
    // *"phiên này chạy trong một cửa sổ Terminal"* cho một phiên không cửa sổ.
    let d = book_dir();
    write_session(&d, 91890, real_background_session());

    let rows = list_account_books(d.path()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["kind"], "background");
    assert_eq!(rows[0]["pid"], 91890);
    // Không có sổ việc thì không có `state` để bịa ra.
    assert!(rows[0].get("state").is_none());
}
