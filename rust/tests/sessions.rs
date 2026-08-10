//! The read-only view of the Claude CLI sessions running on this machine.
//! Every case here came from real output on 2026-08-08, not from imagination.

use hub::sessions::{
    parse_stream, parse_tail, pending_for_display, preview_risk, transcript_path, transcript_slug,
};
use std::collections::HashSet;
use std::path::Path;

/// Phiên chưa từng tung subagent chạy nền — trạng thái của phần lớn phiên.
fn no_background() -> HashSet<String> {
    HashSet::new()
}

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
    let t = parse_tail(tail, &no_background());
    assert_eq!(t.last_role.as_deref(), Some("assistant"));
    assert_eq!(t.last_text.as_deref(), Some("Đã chạy, 162 test xanh."));
    assert_eq!(t.last_ts.as_deref(), Some("2026-08-08T01:02:00Z"));
}

#[test]
fn a_turn_that_is_only_tool_calls_still_says_what_it_was_doing() {
    // Two of the live sessions were mid-tool-call, which has no text at all.
    // An empty row would read as "idle" when the session is actually working.
    let tail = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{}}]},"timestamp":"2026-08-08T02:00:00Z"}"#;
    assert_eq!(parse_tail(tail, &no_background()).last_text.as_deref(), Some("[dùng Bash]"));
}

#[test]
fn a_half_line_at_the_start_of_the_tail_is_skipped_not_fatal() {
    // The tail starts mid-file by design (256 KB from the end), so the first
    // line is usually a fragment.
    let tail = "ontent\":\"cụt\"}}\n{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"ổn\"}]}}";
    assert_eq!(parse_tail(tail, &no_background()).last_text.as_deref(), Some("ổn"));
}

#[test]
fn nothing_conversational_yields_nothing_rather_than_a_wrong_guess() {
    let tail = r#"{"type":"system","subtype":"init"}
{"type":"queue-operation","op":"add"}"#;
    let t = parse_tail(tail, &no_background());
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

/// Whose session is this — the decision the phone acts on.
///
/// The screen HIDES editor rows (Hà, 2026-08-09: *"bỏ các phiên của editor đi
/// vì có quản lý được tin nhắn của nó đâu?"*), so getting this wrong either
/// hides a session he can drive or shows one he cannot. It is also the branch
/// that CANNOT be exercised through the UI on a machine with no editor session
/// listed — on 2026-08-09 three VS Code `claude` processes were running and
/// `claude agents` listed none of them.
///
/// Both command lines below are copied from `ps -o command=` on this machine,
/// not invented: the extension ships its own binary under `~/.vscode/…`, so the
/// PATH is the only thing separating it from a terminal session — the process
/// name is `claude` on both sides.
#[test]
fn a_session_belongs_to_the_editor_or_the_terminal_by_its_path() {
    use hub::sessions::classify_host;

    let vscode = "/Users/hanguyen/.vscode/extensions/anthropic.claude-code-2.1.220-darwin-arm64/resources/native-binary/claude --output-format stream-json --verbose --input-format stream-json";
    // Editor thắng cả tty: extension có thể chạy kèm tty hay không, không đổi.
    assert_eq!(classify_host(vscode, "interactive", "??"), "editor");
    assert_eq!(classify_host(vscode, "interactive", "ttys009"), "editor");
    assert_eq!(
        classify_host("/Users/x/.cursor/extensions/anthropic.claude-code/claude", "interactive", "??"),
        "editor"
    );
    assert_eq!(
        classify_host("/Applications/Cursor.app/Contents/Resources/claude", "interactive", "??"),
        "editor"
    );

    // Dòng terminal thật, chép nguyên từ `ps` trên máy này (tty ttys005).
    assert_eq!(
        classify_host("claude tiếp /Users/hanguyen/Documents/projects/AI/hub", "interactive", "ttys005"),
        "terminal"
    );
    assert_eq!(classify_host("claude tiếp tfl5", "interactive", "ttys006"), "terminal");

    // KHÔNG có tty thì KHÔNG được gọi là terminal. Trước 2026-08-09 nhãn này
    // suy bằng loại trừ, nên một `claude` do script hay cron chạy vẫn đọc là
    // "terminal" — màn hình khai một thứ chưa ai kiểm. `ps` in `??` hoặc `-`.
    assert_eq!(classify_host("claude tiếp dwork", "interactive", "??"), "detached");
    assert_eq!(classify_host("claude tiếp dwork", "interactive", "-"), "detached");
    assert_eq!(classify_host("claude tiếp dwork", "interactive", ""), "detached");

    // `kind` thắng đường dẫn: phiên hub mở bằng `--bg` là của hub, dù binary
    // nào tình cờ đứng trước trong PATH. Thiếu vế này, một phiên nền mở từ
    // binary của editor sẽ bị xếp loại "editor" và biến mất khỏi đúng màn có
    // thể dừng nó.
    assert_eq!(classify_host(vscode, "background", "??"), "background");

    // Tên dự án có chữ "vscode" KHÔNG phải phiên editor; dấu hiệu là thư mục ẩn.
    assert_eq!(
        classify_host("claude tiếp /Users/hanguyen/Documents/projects/vscode-notes", "interactive", "ttys002"),
        "terminal"
    );
}

/// Subagent đang chạy: đếm theo `tool_use_id`, không đếm theo tên.
///
/// Một phiên có thể tung ra nhiều subagent và nhận về vài cái; đếm tên sẽ báo
/// "5 đang chạy" đúng vào lúc con số ấy cần đúng nhất. Kết quả rơi ra ngoài
/// cửa sổ 256KB thì coi như đã xong — thà thiếu còn hơn để một subagent ma
/// chạy mãi trên màn.
#[test]
fn pending_subagents_are_counted_by_id_not_by_name() {
    use hub::sessions::parse_tail;

    let started_two = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"a1","name":"Agent","input":{}},{"type":"tool_use","id":"a2","name":"Agent","input":{}}]}}"#;
    let one_came_back = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"a1","content":"xong"}]}}"#;
    let other_tool = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"b1","name":"Bash","input":{}}]}}"#;

    assert_eq!(parse_tail(started_two, &no_background()).pending_subagents, 2);
    assert_eq!(
        parse_tail(&format!("{started_two}\n{one_came_back}"), &no_background()).pending_subagents,
        1
    );
    // Công cụ khác không phải subagent.
    assert_eq!(parse_tail(other_tool, &no_background()).pending_subagents, 0);
    // Không có gì thì không có gì — và một dòng hỏng không được làm hỏng cả bộ.
    assert_eq!(parse_tail("", &no_background()).pending_subagents, 0);
    assert_eq!(parse_tail("{ đây không phải json", &no_background()).pending_subagents, 0);
}

/// Subagent CHẠY NỀN: `tool_result` về ngay, nên nó KHÔNG phải dấu kết thúc.
///
/// Bug thật, đo 2026-08-10 trên máy này: hai agent đang chạy mà `hub sessions`
/// khai `pending 0`. Lý do là lệnh gọi nền nhận `tool_result` ngay lập tức —
/// nội dung chỉ là "đã tung agent" — nên phép khớp tool_use↔tool_result tưởng
/// nó xong. Đau nhất là chính chế độ nền mới là chế độ con số này sinh ra để
/// bắt: agent chặn thì phiên cha đang bận nhìn là biết, agent nền thì phiên cha
/// rảnh tay, từ điện thoại nhìn y hệt một phiên treo.
///
/// Dấu kết thúc đúng là khối `<task-notification>` mang cùng `tool-use-id`.
/// Hai chuỗi dưới đây là hình dạng THẬT lấy từ nhật ký phiên hôm nay, không
/// phải bịa.
#[test]
fn a_background_subagent_is_not_finished_just_because_the_call_returned() {
    let bg: HashSet<String> = ["toolu_bg1".to_string()].into_iter().collect();

    let launched = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_bg1","name":"Agent","input":{}}]}}"#;
    // Câu trả lời tức thì của lệnh gọi nền: nó nói "đã tung", không nói "đã xong".
    let ack = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_bg1","content":"Async agent launched successfully."}]}}"#;
    let notified = r#"{"type":"user","message":{"content":[{"type":"text","text":"<task-notification>\n<task-id>a55b</task-id>\n<tool-use-id>toolu_bg1</tool-use-id>\n<status>completed</status>\n</task-notification>"}]}}"#;

    let running = format!("{launched}\n{ack}");
    // Trước bản vá, dòng này ra 0 — và 0 là câu trả lời sai cho "đang chạy không".
    assert_eq!(parse_tail(&running, &bg).pending_subagents, 1);

    let done = format!("{running}\n{notified}");
    assert_eq!(parse_tail(&done, &bg).pending_subagents, 0);

    // Cùng một nhật ký, nếu KHÔNG biết đó là lệnh gọi nền thì `tool_result` vẫn
    // là dấu kết thúc đúng — hai chế độ không được lẫn vào nhau.
    assert_eq!(parse_tail(&running, &no_background()).pending_subagents, 0);
}

/// Thông báo kết thúc chỉ đóng ĐÚNG lệnh gọi mà nó nói tới.
///
/// Cùng kỷ luật với phần đếm theo id ở trên: tung ba, một cái báo về, thì còn
/// hai — chứ không phải "có thông báo nào đó nên coi như xong hết".
#[test]
fn a_stop_notice_closes_only_the_call_it_names() {
    let bg: HashSet<String> = ["b1", "b2", "b3"].iter().map(|s| s.to_string()).collect();
    let launched = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"b1","name":"Agent","input":{}},{"type":"tool_use","id":"b2","name":"Task","input":{}},{"type":"tool_use","id":"b3","name":"Agent","input":{}}]}}"#;
    let one_stopped = r#"{"type":"user","message":{"content":[{"type":"text","text":"<task-notification>\n<tool-use-id>b2</tool-use-id>\n<status>completed</status>\n</task-notification>"}]}}"#;

    assert_eq!(parse_tail(launched, &bg).pending_subagents, 3);
    assert_eq!(
        parse_tail(&format!("{launched}\n{one_stopped}"), &bg).pending_subagents,
        2
    );

    // Một lượt chạy nền của Bash cũng sinh `<task-notification>` y hệt; nó
    // không được đóng nhầm một agent nào cả.
    let other_task = r#"{"type":"user","message":{"content":[{"type":"text","text":"<task-notification>\n<tool-use-id>bash-xyz</tool-use-id>\n<status>completed</status>\n</task-notification>"}]}}"#;
    assert_eq!(
        parse_tail(&format!("{launched}\n{other_task}"), &bg).pending_subagents,
        3
    );
}

/// Tự đóng sổ: từng điều kiện một, vì đây là cơ chế TỰ CHẠY.
///
/// Hà chốt bật 2026-08-10 kèm ràng buộc *"phải đảm bảo đã chạy hết chỗ dở"* —
/// và đó mới là phần khó. Một cơ chế tự động cắt ngang việc đang làm thì tệ hơn
/// hẳn việc không có cơ chế nào.
#[test]
fn auto_handover_only_fires_when_the_session_is_truly_done() {
    use hub::pipeline::{auto_handover_why, AutoWhy};

    // đủ đầy · có cửa sổ · không bận · không hỏi · không subagent · đã im đủ lâu
    let go = |pct, busy, asking, subs, idle| {
        auto_handover_why(pct, 80, false, true, busy, asking, subs, idle, 120)
    };
    assert_eq!(go(85, false, false, 0, 300), AutoWhy::Do);

    // Chưa đầy thì thôi.
    assert_eq!(go(79, false, false, 0, 300), AutoWhy::NotFull(79));
    // ĐANG CHẠY DỞ — đây là điều kiện Hà đặt ra.
    assert_eq!(go(95, true, false, 0, 300), AutoWhy::Busy);
    // Đang hỏi thì đóng sổ là trả lời thay người dùng.
    assert_eq!(go(95, false, true, 0, 300), AutoWhy::Asking);
    // Subagent còn chạy thì việc chưa xong, dù màn trông rảnh.
    assert_eq!(go(95, false, false, 2, 300), AutoWhy::Subagents(2));
    // Vừa im được 30 giây chưa đủ chắc: giữa hai lệnh cũng im.
    assert_eq!(go(95, false, false, 0, 30), AutoWhy::TooFresh(30));

    // Không đọc được màn thì KHÔNG đoán là rảnh.
    assert_eq!(
        auto_handover_why(95, 80, false, false, false, false, 0, 300, 120),
        AutoWhy::NoWindow
    );
    // Đã đóng rồi thì thôi, kể cả mọi thứ khác đều thoả.
    assert_eq!(
        auto_handover_why(95, 80, true, true, false, false, 0, 300, 120),
        AutoWhy::AlreadyDone
    );
}

/// Phiên đã chết thì không có gì "đang chạy" — kể cả nhật ký còn dở.
///
/// Hồi quy do chính bản vá agent-nền đẻ ra, bắt được bằng cách chạy trên máy
/// thật chứ không phải bằng test: phiên "Tự chạy lại khi gặp lỗi" tắt từ 11
/// tiếng trước mà khai 3 subagent đang chạy, vì tiến trình chết mang theo cả
/// những thông báo kết thúc chưa kịp ghi.
#[test]
fn a_dead_session_has_no_running_subagents() {
    assert_eq!(pending_for_display("dead", 3), 0);
    // Không dò được tiến trình = KHÔNG BIẾT, và không biết thì không khai
    // "đang chạy" — cùng lựa chọn "thà thiếu còn hơn nói dối".
    assert_eq!(pending_for_display("unknown", 3), 0);
    // Còn sống thì con số đếm được đi thẳng ra màn, không ai chỉnh sửa.
    for host in ["terminal", "background", "detached", "editor"] {
        assert_eq!(pending_for_display(host, 3), 3, "host={host}");
        assert_eq!(pending_for_display(host, 0), 0, "host={host}");
    }
}

/// Lời văn NHẮC TỚI thẻ thông báo không được đóng lệnh gọi nào.
///
/// Bẫy dogfood, và là bẫy hàng ngày trên chính dự án này: phiên đang sửa đúng
/// tính năng đếm subagent sẽ có cả `<task-notification>` lẫn một `tool-use-id`
/// thật nằm trong lời văn — chỗ dán nhật ký ra xem, chỗ bàn cách sửa. Bản đầu
/// chỉ hỏi "đoạn này có chứa chữ ấy không" rồi hốt mọi id trong cả đoạn, nên
/// một subagent ĐANG CHẠY bị đóng dấu "xong": con số vẫn nói dối, chỉ đổi
/// chiều. Cắt theo cặp thẻ là hết.
#[test]
fn prose_that_merely_mentions_the_notice_closes_nothing() {
    let bg: HashSet<String> = ["b1"].iter().map(|s| s.to_string()).collect();
    let launched = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"b1","name":"Agent","input":{}}]}}"#;
    // Đúng hình dạng một lượt bàn việc: nhắc tên thẻ, rồi trích một id thật.
    // Mẫu này phải là hình dạng THẬT của một câu bàn việc: nhắc tên khối, rồi
    // trích một cặp thẻ NGUYÊN VẸN. Mẫu đầu tiên tôi viết có một thẻ mở lửng
    // đứng trước cặp thật, nên mã cũ hớt phải một chuỗi rác thay vì `b1` và
    // test xanh cả với mã hỏng — một phép đo mù, bắt được nhờ chạy RED trước.
    let talking = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Khối <task-notification> đóng lệnh gọi bằng <tool-use-id>b1</tool-use-id>, tức khớp theo id chứ không theo tên."}]}}"#;
    assert_eq!(
        parse_tail(&format!("{launched}\n{talking}"), &bg).pending_subagents,
        1,
        "lời văn nhắc tới thẻ không được tính là đã xong"
    );

    // Còn thông báo THẬT thì vẫn phải đóng được.
    let real = r#"{"type":"user","message":{"content":[{"type":"text","text":"<task-notification>\n<tool-use-id>b1</tool-use-id>\n<status>completed</status>\n</task-notification>"}]}}"#;
    assert_eq!(
        parse_tail(&format!("{launched}\n{talking}\n{real}"), &bg).pending_subagents,
        0
    );

    // Khối THIẾU thẻ đóng không được tính là thông báo — vì nó không thể là
    // một thông báo thật: mỗi bản ghi là một dòng JSON trọn vẹn, dòng bị cửa sổ
    // 256KB cắt thì trượt `from_str` và bị bỏ cả dòng. Còn thứ mở mà không đóng
    // thì đúng là lời văn, và lời văn không được đóng lệnh gọi nào.
    let dangling = r#"{"type":"user","message":{"content":[{"type":"text","text":"nói về <task-notification> rồi trích <tool-use-id>b1</tool-use-id> mà không đóng khối"}]}}"#;
    assert_eq!(parse_tail(&format!("{launched}\n{dangling}"), &bg).pending_subagents, 1);
}

/// Thông báo kết thúc tới bằng BA đường, không phải một.
///
/// Đo trên máy 2026-08-10, SAU khi bản vá đầu đã xanh cả bộ kịch bản: một agent
/// đã về từ lâu vẫn nằm trên màn. Lý do là nó về đúng lúc phiên cha đang chạy dở
/// một lệnh, nên CLI không giao thông báo thành một lượt `user` mà xếp vào sổ
/// (`queue-operation` với chữ ở `content`, rồi `attachment` với chữ ở
/// `attachment.prompt`). Bộ kịch bản không bắt được vì nó chỉ đo lúc agent đang
/// chạy thật — con ma chỉ hiện ra khi đi kiểm trạng thái sống.
#[test]
fn a_stop_notice_arrives_by_three_different_roads() {
    let bg: HashSet<String> = ["q1", "a1"].iter().map(|s| s.to_string()).collect();
    let launched = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"q1","name":"Agent","input":{}},{"type":"tool_use","id":"a1","name":"Agent","input":{}}]}}"#;
    assert_eq!(parse_tail(launched, &bg).pending_subagents, 2);

    // Đường 2: xếp hàng, chữ nằm thẳng ở `content`.
    let queued = r#"{"type":"queue-operation","operation":"enqueue","content":"<task-notification>\n<tool-use-id>q1</tool-use-id>\n<status>completed</status>\n</task-notification>"}"#;
    assert_eq!(parse_tail(&format!("{launched}\n{queued}"), &bg).pending_subagents, 1);

    // Đường 3: đính kèm, chữ nằm ở `attachment.prompt`.
    let attached = r#"{"type":"attachment","attachment":{"type":"queued_command","prompt":"<task-notification>\n<tool-use-id>a1</tool-use-id>\n<status>completed</status>\n</task-notification>"}}"#;
    assert_eq!(
        parse_tail(&format!("{launched}\n{queued}\n{attached}"), &bg).pending_subagents,
        0
    );
}
