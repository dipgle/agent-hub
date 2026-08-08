//! The read-only view of the Claude CLI sessions running on this machine.
//! Every case here came from real output on 2026-08-08, not from imagination.

use hub::sessions::{parse_stream, parse_tail, preview_risk, transcript_path, transcript_slug};
use std::path::Path;

#[test]
fn cwd_maps_to_the_folder_the_cli_writes_transcripts_into() {
    assert_eq!(
        transcript_slug("/Users/hanguyen/Documents/projects"),
        "-Users-hanguyen-Documents-projects"
    );
    // A trailing slash must not produce a different folder for the same cwd.
    assert_eq!(
        transcript_slug("/Users/hanguyen/Documents/projects/"),
        "-Users-hanguyen-Documents-projects"
    );
    assert_eq!(
        transcript_slug("/Users/hanguyen/Documents/projects/AI/hub"),
        "-Users-hanguyen-Documents-projects-AI-hub"
    );
    assert_eq!(transcript_slug("/"), "-");

    assert_eq!(
        transcript_path(Path::new("/home/x/.claude"), "/a/b", "sess-1"),
        Path::new("/home/x/.claude/projects/-a-b/sess-1.jsonl")
    );
}

#[test]
fn the_last_record_is_usually_not_the_last_turn() {
    // Real transcripts end on bookkeeping as often as on conversation: of 14
    // live sessions, 5 ended on pr-link / attachment / system / last-prompt.
    // Reading the final line and calling it the state is wrong for those.
    let tail = r#"
{"type":"user","message":{"role":"user","content":"chạy test đi"},"timestamp":"2026-08-08T01:00:00Z"}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Đã chạy, 162 test xanh."}]},"timestamp":"2026-08-08T01:02:00Z"}
{"type":"pr-link","url":"https://github.com/x/y/pull/1"}
{"type":"file-history-snapshot","files":3}
"#;
    let t = parse_tail(tail);
    assert_eq!(t.last_role.as_deref(), Some("assistant"));
    assert_eq!(t.last_text.as_deref(), Some("Đã chạy, 162 test xanh."));
    assert_eq!(t.last_ts.as_deref(), Some("2026-08-08T01:02:00Z"));
}

#[test]
fn a_turn_that_is_only_tool_calls_still_says_what_it_was_doing() {
    // Two of the live sessions were mid-tool-call, which has no text at all.
    // An empty row would read as "idle" when the session is actually working.
    let tail = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{}}]},"timestamp":"2026-08-08T02:00:00Z"}"#;
    assert_eq!(parse_tail(tail).last_text.as_deref(), Some("[dùng Bash]"));
}

#[test]
fn a_half_line_at_the_start_of_the_tail_is_skipped_not_fatal() {
    // The tail starts mid-file by design (256 KB from the end), so the first
    // line is usually a fragment.
    let tail = "ontent\":\"cụt\"}}\n{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"ổn\"}]}}";
    assert_eq!(parse_tail(tail).last_text.as_deref(), Some("ổn"));
}

#[test]
fn nothing_conversational_yields_nothing_rather_than_a_wrong_guess() {
    let tail = r#"{"type":"system","subtype":"init"}
{"type":"queue-operation","op":"add"}"#;
    let t = parse_tail(tail);
    assert_eq!(t.last_text, None);
    assert_eq!(t.last_role, None);
}

// ----------------------------------------------------------------------
// The preview leaves this machine (it lands in a tfl5 doc), so it passes the
// leak gate first. Calibration matters as much as coverage: a gate that hides
// everything teaches the owner to ignore it. Measured on the real 14 sessions:
// 1 hidden, 12 previewable, 0 leaks left.
// ----------------------------------------------------------------------

#[test]
fn a_password_stated_in_vietnamese_is_caught() {
    // The case that proved the gate was decorative: every English pattern
    // passed this through, because this workspace does not work in English.
    let hits = preview_risk("Mật khẩu là Abcd!2026 — tôi đặt nó trong lệnh /reg lúc nãy");
    assert!(
        hits.iter().any(|h| h == "credential_word_vi"),
        "phải bắt được 'mật khẩu', bắt được: {hits:?}"
    );

    for text in [
        "mã bí mật của app là ...",
        "khoá riêng nằm ở đâu?",
        "thông tin đăng nhập gửi qua chat",
        "mat khau khong dau van phai bat",
    ] {
        assert!(!preview_risk(text).is_empty(), "phải bắt: {text}");
    }
    // English still works — this augments, it does not replace.
    assert!(!preview_risk("the password is hunter2").is_empty());
}

// ----------------------------------------------------------------------
// UC-S02 — the stream that has to read like the terminal: what was said, every
// command WITH its arguments, and every command's output.
// ----------------------------------------------------------------------

#[test]
fn the_stream_carries_commands_and_their_output_not_just_talk() {
    let tail = r#"
{"type":"user","message":{"role":"user","content":"chạy test đi"},"timestamp":"2026-08-08T01:00:00Z"}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"cần chạy cargo test"},{"type":"tool_use","name":"Bash","input":{"command":"cargo test"}}]},"timestamp":"2026-08-08T01:00:05Z"}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"test result: ok. 169 passed"}]},"timestamp":"2026-08-08T01:02:00Z"}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"169 test xanh."}]},"timestamp":"2026-08-08T01:02:05Z"}
{"type":"pr-link","url":"https://example.invalid/1"}
"#;
    let s = parse_stream(tail, 100);
    let kinds: Vec<&str> = s.events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, vec!["say", "think", "tool", "result", "say"]);

    let tool = s.events.iter().find(|e| e.kind == "tool").unwrap();
    assert_eq!(tool.name, "Bash");
    assert!(
        tool.text.contains("cargo test"),
        "lệnh phải kèm tham số: {}",
        tool.text
    );

    let result = s.events.iter().find(|e| e.kind == "result").unwrap();
    assert!(result.text.contains("169 passed"), "kết quả lệnh phải hiện");

    // Bookkeeping records never become events.
    assert!(!s.events.iter().any(|e| e.text.contains("example.invalid")));
    assert_eq!(s.older_hidden, 0);
}

#[test]
fn a_secret_in_command_output_is_withheld_not_printed() {
    // The exact risk UC-S02 creates: showing `tool_result` means showing what
    // commands print, which is where keys and env vars surface. Gating only the
    // last turn (what the list screen does) would publish this.
    let tail = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"đăng nhập ok — mật khẩu là Abcd!2026"}]},"timestamp":"2026-08-08T01:00:00Z"}"#;
    let s = parse_stream(tail, 100);
    let e = &s.events[0];
    assert!(e.withheld, "phải bị giữ lại");
    assert!(
        !e.text.contains("Abcd!2026"),
        "giá trị không được lọt ra: {}",
        e.text
    );
    assert!(
        e.text.contains("hub ẩn"),
        "phải nói rõ vì sao trống: {}",
        e.text
    );
}

#[test]
fn the_window_is_bounded_and_says_how_much_it_dropped() {
    let mut lines = String::new();
    for i in 0..30 {
        lines.push_str(&format!(
            "{{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"dòng {i}\"}}]}}}}\n"
        ));
    }
    let s = parse_stream(&lines, 10);
    assert_eq!(s.events.len(), 10, "cửa sổ phải bị chặn");
    assert_eq!(s.older_hidden, 20, "phải nói rõ đã bỏ bao nhiêu");
    // Keeps the NEWEST, like a terminal.
    assert!(s.events.last().unwrap().text.contains("dòng 29"));
    assert!(s.events.first().unwrap().text.contains("dòng 20"));
}

#[test]
fn ordinary_development_chatter_stays_visible() {
    // These all trip the FULL outbound scan (paths, IPs, "blocker"), and that
    // is right for a reply to an outsider — but blanking them here would empty
    // the view the owner opens on his phone.
    for text in [
        "Đã sửa /Users/hanguyen/Documents/projects/AI/hub/rust/src/db.rs:120",
        "node RPC 46.250.231.130:41100 trả 403",
        "còn 1 blocker: CI đỏ vì billing",
        "[dùng Bash]",
    ] {
        assert!(
            preview_risk(text).is_empty(),
            "không được ẩn dòng bình thường: {text}"
        );
    }
}

#[test]
fn the_background_id_is_the_first_token_not_the_rest_of_the_line() {
    use hub::sessions::parse_backgrounded_id;

    // The happy line, as `claude --bg '<task>'` prints it.
    assert_eq!(
        parse_backgrounded_id(
            "Starting background service…\nbackgrounded · a3a24ccd\n  claude agents  list sessions\n"
        ),
        Some("a3a24ccd")
    );

    // THE ONE THAT COST A REAL RUN. A session that came up with no prompt adds
    // a sentence on the same line; taking the tail stored
    // "6514f454 (idle — send a prompt to start)" as the session id, and every
    // later verb quietly failed to match it while hub reported success.
    assert_eq!(
        parse_backgrounded_id("backgrounded · 6514f454 (idle — send a prompt to start)"),
        Some("6514f454")
    );

    // No marker means hub does NOT know what it started — that has to be an
    // error, not a cheerful empty id.
    assert_eq!(parse_backgrounded_id("Starting background service…"), None);
    assert_eq!(parse_backgrounded_id(""), None);
    assert_eq!(parse_backgrounded_id("backgrounded · "), None);
}
