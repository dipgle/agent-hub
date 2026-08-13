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
    // Gốc workspace đổi sang `~/projects` ngày 2026-08-12 ⟹ KHOÁ nhật ký đổi
    // theo (`~/.claude/projects/-Users-hanguyen-projects`). Thư mục khoá mới là
    // symlink trỏ về khoá cũ, nên hai đường cùng ra một kho — nhưng thứ hub tính
    // ra từ `cwd` phải là khoá MỚI, vì đó là cwd thật của mọi phiên từ hôm ấy.
    assert_eq!(
        transcript_slug("/Users/hanguyen/projects"),
        "-Users-hanguyen-projects"
    );
    // A trailing slash must not produce a different folder for the same cwd.
    assert_eq!(
        transcript_slug("/Users/hanguyen/projects/"),
        "-Users-hanguyen-projects"
    );
    assert_eq!(
        transcript_slug("/Users/hanguyen/projects/AI/hub"),
        "-Users-hanguyen-projects-AI-hub"
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
        "Đã sửa /Users/hanguyen/projects/AI/hub/rust/src/db.rs:120",
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
        classify_host("claude tiếp /Users/hanguyen/projects/AI/hub", "interactive", "ttys005"),
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
        classify_host("claude tiếp /Users/hanguyen/projects/vscode-notes", "interactive", "ttys002"),
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

    // Đường 1 ở dạng THẬT của nó: `message.content` là chuỗi thuần, không phải
    // mảng khối. Đếm trên 384 tệp nhật ký của máy này: 355 chuỗi / 4 mảng.
    let plain = r#"{"type":"user","message":{"role":"user","content":"<task-notification>\n<tool-use-id>q1</tool-use-id>\n<status>completed</status>\n</task-notification>"}}"#;
    assert_eq!(parse_tail(&format!("{launched}\n{plain}"), &bg).pending_subagents, 1);

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

/// Thông báo KHÔNG mang `tool-use-id` thì không đóng được lệnh gọi nào.
///
/// Đếm thật trên 384 tệp nhật ký: 250/2535 khối là `Monitor event` — chúng có
/// `<task-id>` nhưng không có `<tool-use-id>`, vì chúng thuộc một cơ chế khác.
/// Đóng bừa theo `task-id` sẽ là khớp nhầm hai hệ id với nhau.
#[test]
fn a_notice_without_a_tool_use_id_closes_nothing() {
    let bg: HashSet<String> = ["b1".to_string()].into_iter().collect();
    let launched = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"b1","name":"Agent","input":{}}]}}"#;
    let monitor = r#"{"type":"queue-operation","operation":"enqueue","content":"<task-notification>\n<task-id>b1</task-id>\n<summary>Monitor event</summary>\n</task-notification>"}"#;
    assert_eq!(parse_tail(&format!("{launched}\n{monitor}"), &bg).pending_subagents, 1);
}

/// Chuỗi thẻ HỎNG không được sinh id rác, và không được bỏ sót id thật.
///
/// Bản đếm bằng JS trong `fe-subagent-uc.mjs` là ĐỐI CHỨNG ĐỘC LẬP của kịch bản
/// E2E; hai bản cài đặt mà xử khác nhau thì cái đối chứng ấy sẽ báo lệch cho một
/// chuyện không phải lỗi sản phẩm. Regex `[^<]+` bên JS đồng bộ lại ở thẻ mở kế
/// tiếp, nên bản Rust cũng phải vậy: id chạy tới dấu `<` kế tiếp và dấu ấy phải
/// mở đúng thẻ đóng.
#[test]
fn a_malformed_id_sequence_matches_the_javascript_twin() {
    let bg: HashSet<String> = ["real".to_string()].into_iter().collect();
    let launched = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"real","name":"Agent","input":{}}]}}"#;

    // Thẻ mở lồng nhau: `real` vẫn phải được nhận ra là ĐÃ XONG.
    let broken = r#"{"type":"queue-operation","content":"<task-notification><tool-use-id>UNCLOSED<tool-use-id>real</tool-use-id></task-notification>"}"#;
    assert_eq!(parse_tail(&format!("{launched}\n{broken}"), &bg).pending_subagents, 0);

    // Id RỖNG không đóng gì (và không được làm hỏng vòng quét).
    let bg2: HashSet<String> = ["".to_string(), "real".to_string()].into_iter().collect();
    let empty = r#"{"type":"queue-operation","content":"<task-notification><tool-use-id></tool-use-id></task-notification>"}"#;
    assert_eq!(parse_tail(&format!("{launched}\n{empty}"), &bg2).pending_subagents, 1);
}

/// Cửa sổ Terminal mà `/new` mở ra — dòng lệnh phải đúng ĐẾN TỪNG THỨ TỰ.
///
/// Hai thứ ở đây từng làm hỏng một phiên thật, nên chúng được khoá lại bằng
/// test chứ không bằng lời dặn trong bình luận.
mod cua_so_moi {
    use hub::sessions::terminal_command;
    use std::path::Path;

    /// Đề bài phải đứng TRƯỚC `--disallowedTools`.
    ///
    /// Cờ ấy variadic: đặt đề bài sau nó thì đề bài bị nuốt thành một mẫu công
    /// cụ nữa, phiên dựng lên mà không có việc gì để làm — đúng lỗi lần `/new`
    /// đầu tiên (`CLAUDE.md` §10). Test đọc VỊ TRÍ, không đọc câu chữ.
    #[test]
    fn de_bai_dung_truoc_co_variadic() {
        let cmd = terminal_command("claude", Path::new("/Users/x/projects"), "[hub] dọn nợ", None);
        let de_bai = cmd.find("dọn nợ").expect("đề bài phải có trong lệnh");
        let co = cmd.find("--disallowedTools").expect("phải có rào công cụ");
        assert!(
            de_bai < co,
            "đề bài bị đẩy ra sau cờ variadic ⟹ phiên sẽ dựng lên rỗng: {cmd}"
        );
        // Và rào công cụ phải thật sự có hàng, không phải một cờ trống.
        assert!(cmd.trim_end().len() > co + "--disallowedTools".len() + 3, "{cmd}");
    }

    /// Nháy đơn trong đề bài không được phép thoát ra ngoài chuỗi.
    ///
    /// Đề bài là chữ chủ máy gõ trên điện thoại; một dấu `'` mà lọt ra thì phần
    /// còn lại của câu trở thành LỆNH SHELL. Đây là rào an toàn, không phải
    /// chuyện thẩm mỹ.
    #[test]
    fn nhay_don_trong_de_bai_khong_thoat_ra_shell() {
        let cmd = terminal_command(
            "claude",
            Path::new("/Users/x/projects"),
            "đừng 'rm -rf /' nhé",
            None,
        );
        // Sau khi bọc, mọi nháy đơn của người dùng phải nằm trong dạng '\''.
        let tho = cmd.replace(r"'\''", "");
        assert_eq!(
            tho.matches('\'').count() % 2,
            0,
            "nháy đơn lẻ ⟹ chuỗi shell hở: {cmd}"
        );
        assert!(cmd.contains(r"'\''rm -rf /'\''"), "phải bọc lại nháy của người dùng: {cmd}");
    }

    /// Chạy đúng tài khoản: `CLAUDE_CONFIG_DIR` phải đứng trước lệnh `claude`.
    #[test]
    fn tai_khoan_duoc_cam_theo_vao_cua_so() {
        let cmd = terminal_command(
            "claude",
            Path::new("/Users/x/projects"),
            "việc",
            Some("/Users/x/.claude-acc2"),
        );
        let env = cmd.find("CLAUDE_CONFIG_DIR").expect("phải cắm biến tài khoản");
        let cli = cmd.find("'claude'").expect("phải gọi claude");
        assert!(env < cli, "biến môi trường phải đứng trước lệnh: {cmd}");
        assert!(cmd.contains("/Users/x/.claude-acc2"), "{cmd}");
        // Không có tài khoản thì KHÔNG cắm biến rỗng.
        let khong = terminal_command("claude", Path::new("/Users/x/projects"), "việc", None);
        assert!(!khong.contains("CLAUDE_CONFIG_DIR"), "{khong}");
    }

    /// Rào công cụ phải SỐNG SÓT qua shell.
    ///
    /// `Bash(git push:*)` để trần là lỗi cú pháp của bash/zsh — cửa sổ sẽ mở ra
    /// với một dòng đỏ, không có phiên nào, và rào an toàn coi như không tồn
    /// tại. Nhánh `--bg` không dính bẫy này vì nó truyền argv thẳng.
    #[test]
    fn rao_cong_cu_song_sot_qua_shell() {
        let cmd = terminal_command("claude", Path::new("/Users/x/projects"), "việc", None);
        let (_, rao) = cmd.split_once("--disallowedTools").expect("phải có rào");
        assert!(rao.contains("'Bash(git push:*)'"), "mẫu để trần: {rao}");
        // Không một dấu ngoặc nào được đứng ngoài chuỗi.
        for doan in rao.split('\'').step_by(2) {
            assert!(
                !doan.contains('(') && !doan.contains(')') && !doan.contains('*'),
                "ký tự shell nuốt được nằm ngoài nháy: {doan:?} trong {rao}"
            );
        }
    }

    /// Cửa sổ mở ở GỐC WORKSPACE — thư mục duy nhất cả ba tài khoản đã duyệt.
    #[test]
    fn mo_o_goc_workspace() {
        let cmd = terminal_command("claude", Path::new("/Users/x/projects"), "việc", None);
        assert!(cmd.starts_with("cd '/Users/x/projects' &&"), "{cmd}");
    }
}

/// Câu trả lời `/btw` phải là CÂU TRẢ LỜI, không phải ảnh chụp màn hình.
///
/// Màn dưới đây là bản chụp THẬT (2026-08-11, phiên `projects-ff` trên
/// `ttys001`) của lần `/btw` đầu tiên chạy được đầu-tới-cuối. Nó mang
/// `~/Documents/projects` vì hôm ấy gốc workspace nằm ở đó — **giữ nguyên
/// từng byte**, đừng "sửa cho hợp thời": một bản chụp đã sửa thôi là bằng
/// chứng, và cái đang đo ở đây là cách CẮT màn, không phải đường dẫn. Bản đầu gửi
/// nguyên cả cái màn này về điện thoại — logo khởi động, dòng vừa gõ, chân bảng
/// hướng dẫn phím — và người đọc phải tự lọc ra câu trả lời giữa đống ấy.
#[test]
fn a_btw_answer_is_cut_out_of_the_screen_not_shipped_whole() {
    let screen = "\
▗ ▗   ▖ ▖  Claude Code v2.1.227
           Opus 5 (1M context) with xhigh effort · Claude Max
  ▘▘ ▝▝    ~/Documents/projects


▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔

    /btw Tóm tắt trong 1 câu: phiên này đang làm việc gì?

      Phiên này thực tế chưa có trao đổi nào ngoài context khởi động
      (CLAUDE.md workspace + memory index + hook SessionStart), nên chưa có
      việc nào đang làm.

    ↑/↓ to scroll · c to copy · f to fork · Esc to close";
    let out = hub::sessions::btw_answer(screen, "Tóm tắt trong 1 câu: phiên này đang làm việc gì?");
    assert!(out.contains("chưa có trao đổi nào"), "mất câu trả lời:\n{out}");
    assert!(!out.contains("Claude Code v"), "còn logo khởi động:\n{out}");
    assert!(!out.contains("Esc to close"), "còn chân bảng phím:\n{out}");
    assert!(!out.contains("/btw"), "còn chính câu vừa gõ:\n{out}");
}

/// Không tìm thấy mốc cắt thì thà trả cả màn còn hơn trả rỗng — im lặng đưa một
/// chuỗi rỗng là biến "đọc không được" thành "phiên không nói gì".
#[test]
fn an_uncuttable_screen_still_returns_something() {
    let out = hub::sessions::btw_answer("một màn hình lạ hoắc", "câu hỏi không có trên màn");
    assert_eq!(out, "một màn hình lạ hoắc");
}

/// Bảng đang VIẾT DỞ thì chưa phải câu trả lời — và chân bảng KHÔNG nói được
/// điều đó.
///
/// Ảnh chụp thật 2026-08-11: `Esc to close` có mặt ngay từ lúc bảng mới mở,
/// nên bản vá đầu (neo vào chân bảng) vẫn tóm về một bảng còn đang chạy chữ
/// `✳ Answering…`. Đây là lần thứ hai của cùng một lỗi trong ngày: lấy một dấu
/// hiệu "đang mở" làm dấu hiệu "đã xong".
#[test]
fn a_panel_still_writing_is_not_an_answer() {
    let writing = "    /btw Tóm tắt trong 1 câu: phiên này đang làm việc gì?\n      ✳ Answering…\n    Esc to close";
    assert!(!hub::sessions::btw_panel_finished(writing), "bảng đang viết mà đã coi là xong");

    let done = "    /btw Tóm tắt?\n\n      Phiên này chưa làm gì cả.\n\n    ↑/↓ to scroll · c to copy · f to fork · Esc to close";
    assert!(hub::sessions::btw_panel_finished(done), "bảng đã xong mà không nhận ra");
    assert!(!hub::sessions::btw_panel_finished("❯ \n  ⏵⏵ auto mode on"), "màn thường không phải bảng");
}

/// Câu hỏi dài bị TUI ngắt dòng thì phép cắt vẫn phải đúng.
///
/// Bản trước tìm "dòng nào chứa cả câu hỏi" — với cửa sổ hẹp thì không dòng nào
/// chứa cả câu, phép tìm trượt, và câu trả lời trả về còn nguyên dòng lệnh
/// `/btw …` ở đầu. Nay neo vào chính chữ `/btw` mà `claude` vẽ lại.
#[test]
fn a_wrapped_question_still_gets_cut_off_the_answer() {
    let screen = "    /btw Tóm tắt trong 1 câu: phiên này\n    đang làm việc gì?\n\n      Chưa có việc nào đang chạy.\n\n    ↑/↓ to scroll · Esc to close";
    let out = hub::sessions::btw_answer(screen, "Tóm tắt trong 1 câu: phiên này đang làm việc gì?");
    assert!(out.contains("Chưa có việc nào"), "mất câu trả lời:\n{out}");
    assert!(!out.contains("/btw"), "còn dòng lệnh vừa gõ:\n{out}");
}

/// Gốc workspace TRẦN không được làm câm cả phép đo.
///
/// 🔴 Đo 2026-08-12: hai trong bốn phiên khai "(chưa rõ)" trong khi nhật ký của
/// chúng nhắc tên dự án 4 lần. Gốc: mỗi bản ghi mang `"cwd":"…/projects"` trần,
/// và bản đầu dùng `strip_prefix('/')?` — dấu `?` trong hàm trả `Option` thoát
/// khỏi CẢ vòng lặp ngay lần gặp đầu tiên. Một lỗi im lặng đúng nghĩa: câu trả
/// lời "chưa đủ bằng chứng" nghe hoàn toàn hợp lý.
#[test]
fn a_bare_workspace_root_does_not_silence_the_whole_scan() {
    let root = "/Users/x/projects";
    let tail = r#"{"cwd":"/Users/x/projects","type":"user"}
{"path":"/Users/x/projects/AI/hub/PLAN.md"}
{"path":"/Users/x/projects/AI/hub/UC.md"}"#;
    assert_eq!(
        hub::sessions::folder_from_tail(tail, root).as_deref(),
        Some("AI/hub")
    );
}

/// `AI/` là ngăn kéo, không phải dự án — lấy tên bên trong nó.
#[test]
fn the_drawer_named_ai_is_never_the_project() {
    let root = "/Users/x/projects";
    let tail = "a /Users/x/projects/AI/sdvi/x.rs b /Users/x/projects/AI/sdvi/y.rs";
    assert_eq!(
        hub::sessions::folder_from_tail(tail, root).as_deref(),
        Some("AI/sdvi")
    );
}

/// Nhắc đúng một lần thì chưa đủ — một câu văn lỡ nêu tên không được lật kết quả.
#[test]
fn one_mention_is_not_evidence() {
    let root = "/Users/x/projects";
    let tail = "chỉ nhắc một lần /Users/x/projects/dwork/a.ts";
    assert_eq!(hub::sessions::folder_from_tail(tail, root), None);
}

/// Phiên dừng lại HỎI thì đọc ra được câu hỏi + từng lựa chọn — từ NHẬT KÝ.
///
/// 🔴 Hà 2026-08-12: *"có 1 phiên đang đưa lựa chọn nhưng không nhận được trên
/// tele"*. Trước đó hub hỏi sai nguồn: nó đọc MÀN rồi để `keys::parse_choices`
/// nhận dạng, mà hàm ấy đòi các mục liền dòng nhau (luật 08-11, sinh ra để khỏi
/// đọc nhầm một đoạn văn có đánh số) — còn bảng `AskUserQuestion` thì mỗi lựa
/// chọn có một dòng MÔ TẢ bên dưới. Hai luật đúng gặp nhau thành một phiên kẹt
/// mà điện thoại không hay biết. Nhật ký thì có cấu trúc, và có cả với phiên
/// hub không đọc được màn.
#[test]
fn a_waiting_question_is_read_with_all_its_options() {
    let tail = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"AskUserQuestion","input":{"questions":[{"header":"Nửa ngày","question":"Đơn vắng có khai được NỬA NGÀY không?","options":[{"label":"Thêm ô nửa ngày","description":"…"},{"label":"Luôn trọn ngày","description":"…"}]}]}}]}}"#;
    let a = hub::sessions::pending_question(tail).expect("phải thấy câu hỏi");
    assert_eq!(a.header, "Nửa ngày");
    assert!(a.question.contains("NỬA NGÀY"));
    assert_eq!(a.options, vec!["Thêm ô nửa ngày", "Luôn trọn ngày"]);
}

/// Trả lời rồi thì thôi — không để cái chuông kêu về một câu đã xong.
#[test]
fn a_question_that_was_answered_is_no_longer_pending() {
    let ask = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"AskUserQuestion","input":{"questions":[{"header":"h","question":"q","options":[{"label":"a"},{"label":"b"}]}]}}]}}"#;
    let answer = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"The user answered"}]}}"#;
    assert!(hub::sessions::pending_question(&format!("{ask}\n{answer}")).is_none());
    // Hỏi tiếp sau khi đã trả lời câu trước ⟹ câu SAU mới là câu đang chờ.
    let ask2 = ask.replace("toolu_1", "toolu_2").replace("\"question\":\"q\"", "\"question\":\"q2\"");
    let a = hub::sessions::pending_question(&format!("{ask}\n{answer}\n{ask2}"))
        .expect("câu thứ hai còn treo");
    assert_eq!(a.question, "q2");
}

/// Điều 5: câu hỏi cũng là chữ rời khỏi máy — có dấu hiệu bí mật thì giữ chữ
/// lại, nhưng KHÔNG giữ lại sự thật là phiên đang kẹt.
#[test]
fn a_question_that_smells_of_secrets_keeps_the_fact_but_not_the_words() {
    let tail = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_9","name":"AskUserQuestion","input":{"questions":[{"header":"h","question":"mật khẩu tfl5 là Abcd1234! đúng không?","options":[{"label":"đúng"},{"label":"sai"}]}]}}]}}"#;
    let a = hub::sessions::pending_question(tail).expect("vẫn phải báo là đang kẹt");
    assert!(a.options.is_empty(), "không đưa lựa chọn ra: {a:?}");
    assert!(!a.question.contains("Abcd1234"), "lộ bí mật: {a:?}");
    assert!(a.question.contains("bí mật"), "phải nói vì sao trống: {a:?}");
}

// ─────────────────────────────────────────────────────────────────────────────
// last_prose — LỜI cuối cùng phiên nói ra, thứ đi kèm tin báo lên điện thoại
//
// Ba mẫu dưới đây là hình dạng ĐO ĐƯỢC trên nhật ký thật ngày 2026-08-12, không
// phải hình dạng tưởng tượng: lượt hỏi chỉ có một khối `tool_use`
// (`a5f06b76…` bản ghi 328), dòng máy tự chèn `[Request interrupted…]`, và lượt
// vừa nói vừa gọi công cụ.
// ─────────────────────────────────────────────────────────────────────────────

/// Lượt cuối chỉ GỌI CÔNG CỤ thì chưa phải lời cuối.
///
/// Đây là ca đắt nhất và cũng là ca tin báo sinh ra để phục vụ: phiên **dừng
/// lại HỎI** có lượt cuối là `AskUserQuestion` thuần, `text_of` dựng thành
/// `[dùng AskUserQuestion]`. Lấy nguyên nó thì tin lên điện thoại rỗng nghĩa
/// đúng lúc cần nhất, còn thứ quyết được câu trả lời nằm ở bản ghi liền trước.
#[test]
fn a_turn_that_only_calls_a_tool_is_not_the_last_word() {
    let tail = concat!(
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Đo xong: node v24.4.0, cổng 8090 đang bận."}]}}"#,
        "\n",
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"AskUserQuestion","input":{}}]}}"#,
    );
    assert_eq!(
        hub::sessions::last_prose(tail, 2000).as_deref(),
        Some("Đo xong: node v24.4.0, cổng 8090 đang bận."),
    );
}

/// Câu của CHỦ MÁY không được đọc ngược về điện thoại của chính anh ấy.
///
/// `[Request interrupted by user for tool use]` là một bản ghi `user` do máy tự
/// chèn — đo thật trên phiên `37e59209` lúc 16:10, và nó chính là thứ kiểu cũ
/// mang đi làm "lời cuối".
#[test]
fn what_the_owner_typed_is_not_read_back_to_him() {
    let tail = concat!(
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Đã cài xong, daemon chạy lại rồi."}]}}"#,
        "\n",
        r#"{"type":"user","message":{"role":"user","content":"[Request interrupted by user for tool use]"}}"#,
    );
    assert_eq!(
        hub::sessions::last_prose(tail, 2000).as_deref(),
        Some("Đã cài xong, daemon chạy lại rồi."),
    );
}

/// Vừa nói vừa gọi công cụ thì phần LỜI vẫn được giữ.
#[test]
fn a_turn_that_speaks_while_calling_a_tool_keeps_the_speech() {
    let tail = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Chạy nốt bộ test đây."},{"type":"tool_use","id":"t2","name":"Bash","input":{}}]}}"#;
    assert_eq!(
        hub::sessions::last_prose(tail, 2000).as_deref(),
        Some("Chạy nốt bộ test đây."),
    );
}

/// Điều 5: chữ có dấu hiệu bí mật thì GIỮ LẠI, và **không** đi lùi tìm lượt sạch.
///
/// Một lượt cũ hơn đọc lên như thể là lời mới nhất — đó là một câu SAI, còn im
/// lặng chỉ là một câu thiếu.
#[test]
fn a_secret_in_the_last_word_is_withheld_not_swapped_for_an_older_one() {
    let tail = concat!(
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Câu này sạch, và nó CŨ."}]}}"#,
        "\n",
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Mật khẩu là Abcd!2026 nhé"}]}}"#,
    );
    assert_eq!(hub::sessions::last_prose(tail, 2000), None);
}

/// Không có lượt nào của phiên thì trả None, để chỗ gọi rơi về bản xem trước.
///
/// Đúng hình dạng của 40 phiên dò `/usage` do chính hub đẻ ra: cả nhật ký chỉ
/// có một bản ghi `user` mang `<command-name>/usage</command-name>`.
#[test]
fn a_transcript_with_no_assistant_turn_says_nothing() {
    let tail = r#"{"type":"user","message":{"role":"user","content":"<command-name>/usage</command-name>"}}"#;
    assert_eq!(hub::sessions::last_prose(tail, 2000), None);
}

/// Cả CHUỖI — nhật ký → lời cuối → thông tin chốt — phải giữ được câu chốt.
///
/// 🔴 Đây là con bug chạy thật mới thấy (16:26 ngày 2026-08-12, phiên
/// `projects-71`): hai đầu đều đúng mà nối lại thì sai. `key_points` giữ dòng
/// cuối rất tử tế, nhưng `last_prose` đã cắt bản dài ở 2000 ký tự TRƯỚC đó, nên
/// "dòng cuối" nó giữ chỉ là chỗ bị chặt giữa câu. Một trần đặt sai chỗ đọc lên
/// y hệt một tính năng chạy đúng.
///
/// Phép đo này đi đúng đường của `pipeline::announce_changes`, nên hạ `SAY_MAX`
/// về 2000 là nó đỏ ngay.
#[test]
fn the_chain_from_transcript_to_message_keeps_the_closing_sentence() {
    let filler = "Đoạn văn giữa bài, dài và không quyết được gì. ".repeat(60);
    let report = format!(
        "**Bắt được thủ phạm rồi** — đối chứng cùng một thời điểm.\n\n{filler}\n\nNói \"dọn đi\" là mình chạy phần an toàn."
    );
    assert!(report.chars().count() > 2600, "mẫu thử phải dài hơn trần cũ");
    let tail = serde_json::json!({
        "type": "assistant",
        "message": {"role": "assistant", "content": [{"type": "text", "text": report}]}
    })
    .to_string();

    let said = hub::sessions::last_prose(&tail, hub::sessions::SAY_MAX).expect("phải đọc được");
    let points = hub::watch::key_points(&said, 700);
    assert!(
        points.contains("Nói \"dọn đi\" là mình chạy phần an toàn."),
        "câu chốt chết ở giữa đường:\n{points}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// "CỬA SỔ CÒN MỞ" PHẢI LÀ CỬA SỔ CỦA PHIÊN ẤY
//
// 🔴 Hà 2026-08-12: *"tele nhận được 'projects-d8 đã tắt cửa sổ còn mở' nhưng
// thực tế không còn mở nữa"*. Đo lại được cả chuỗi trên máy:
//
//   12:28:08  cửa sổ `ttys002` mở (login-zsh pid 39536)
//   …         `projects-d8` (69a38c64) sống trong cửa sổ ấy
//   16:41:16  Hà thoát CLI rồi gõ `claude` LẠI ngay trong cửa sổ đó (pid 43422)
//   16:42:33  hub nhận ra phiên cũ đi → hỏi "tab nào mang ttys002 không?" → CÒN
//             → nói "cửa sổ terminal còn mở"
//   16:42:33  …đúng vòng ấy, sổ ghi phiên MỚI (e27806c2) cũng ở `ttys002`
//
// tty là một con số ĐƯỢC DÙNG LẠI. Hỏi Terminal "còn tab nào mang số này
// không" trả lời được câu "có cửa sổ", không trả lời được câu "cửa sổ CỦA AI".
// ─────────────────────────────────────────────────────────────────────────────

fn row_at(id: &str, name: &str, tty: &str) -> hub::sessions::LiveSession {
    hub::sessions::LiveSession {
        session_id: id.to_string(),
        name: name.to_string(),
        tty: tty.to_string(),
        host: "terminal".to_string(),
        ..Default::default()
    }
}

#[test]
fn a_window_reused_by_the_next_session_is_not_a_window_left_open() {
    let live = vec![row_at("e27806c2", "projects-7c", "ttys002")];
    let taken = hub::sessions::window_taken_over("69a38c64", "ttys002", &live);
    assert_eq!(
        taken.map(|s| s.name.as_str()),
        Some("projects-7c"),
        "cửa sổ đã bị phiên khác chiếm mà không nhận ra"
    );
}

/// Không ai chiếm thì đừng bịa ra người chiếm — ca này vẫn đi hỏi Terminal.
#[test]
fn an_empty_window_is_not_reported_as_taken_over() {
    let live = vec![row_at("khac", "projects-bb", "ttys001")];
    assert!(hub::sessions::window_taken_over("69a38c64", "ttys002", &live).is_none());
}

/// Chính nó không tính là "phiên khác" — hàng của phiên vừa tắt có thể còn nằm
/// trong ảnh chụp vài giây (`claude agents` bỏ chậm).
#[test]
fn a_session_does_not_take_over_its_own_window() {
    let live = vec![row_at("69a38c64", "projects-d8", "ttys002")];
    assert!(hub::sessions::window_taken_over("69a38c64", "ttys002", &live).is_none());
}

/// Phiên nền không gắn cửa sổ nào: tty rỗng không được khớp với tty rỗng khác.
#[test]
fn sessions_without_a_tty_never_match_each_other() {
    let live = vec![row_at("nen-khac", "projects-zz", "")];
    assert!(hub::sessions::window_taken_over("nen-nay", "", &live).is_none());
}

/// `??` KHÔNG phải một cửa sổ.
///
/// 🔴 Đo 2026-08-12 22:59 trên đúng cái tin Hà đọc: `⏹ hub-67 đã tắt — cửa sổ
/// ấy nay đang chạy phiên hub-ec.` Cả hai là phiên `claude -p "/usage"` của
/// chính hub, **không phiên nào có cửa sổ** — `ps` in `??` khi không có tty điều
/// khiển. `??` không rỗng nên cửa `tty.is_empty()` cho qua, rồi phép so "cùng
/// tty" khớp `??` với `??`: hub tuyên bố một cửa sổ không tồn tại đã bị chiếm.
#[test]
fn a_process_without_a_terminal_has_no_window_to_be_taken_over() {
    let ghost = |id: &str, tty: &str| hub::sessions::LiveSession {
        session_id: id.to_string(),
        name: id.to_string(),
        tty: tty.to_string(),
        host: "detached".to_string(),
        ..Default::default()
    };
    let live = vec![ghost("heir", "??")];
    for no_window in ["??", "-", ""] {
        assert!(
            hub::sessions::window_taken_over("dead", no_window, &live).is_none(),
            "đọc {no_window:?} thành một cửa sổ có thật"
        );
    }
    // Còn tty THẬT thì vẫn phải nhận ra — luật này không được siết lan.
    let live = vec![ghost("heir", "ttys002")];
    assert_eq!(
        hub::sessions::window_taken_over("dead", "ttys002", &live)
            .map(|s| s.session_id.as_str()),
        Some("heir")
    );
}

/// Ngăn kéo phải MỞ RA được, và luật ấy phải là phép ĐO chứ không phải một cái
/// tên gõ sẵn.
///
/// 🔴 Hà 2026-08-12: *"sao phiên fb rõ ràng là ai/tcc/amm nhưng danh sách phiên
/// chỉ hiện ai/tcc"*. Vì mã chỉ biết đúng một ngăn kéo — `"AI"`. Đo trên máy:
/// `AI/tcc` **không có marker nào**, còn `AI/tcc/amm` có `.git`.
///
/// Test dựng một workspace THẬT trong thư mục tạm, vì luật này đọc đĩa: một
/// bản giả bằng chuỗi sẽ không bao giờ phân biệt được ngăn kéo với dự án.
#[test]
fn a_drawer_is_opened_but_a_project_is_not_dug_into() {
    let root = std::env::temp_dir().join(format!("hub-drawer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    // `AI/tcc` = ngăn kéo (không marker) · `AI/tcc/amm` = dự án (.git)
    std::fs::create_dir_all(root.join("AI/tcc/amm/.git")).unwrap();
    // `AI/hub` = dự án (CLAUDE.md) và bên trong có `rust/` mang Cargo.toml —
    // đúng cái bẫy: nếu luật chỉ hỏi "có marker không" mà không dừng đúng lúc
    // thì phiên làm hub sẽ bị khai là `AI/hub/rust`.
    std::fs::create_dir_all(root.join("AI/hub/rust")).unwrap();
    std::fs::write(root.join("AI/hub/CLAUDE.md"), "x").unwrap();
    std::fs::write(root.join("AI/hub/rust/Cargo.toml"), "[package]").unwrap();
    let r = root.display().to_string();

    let tail = format!("sửa {r}/AI/tcc/amm/src/a.rs rồi {r}/AI/tcc/amm/src/b.rs");
    assert_eq!(
        hub::sessions::folder_from_tail(&tail, &r).as_deref(),
        Some("AI/tcc/amm"),
        "ngăn kéo không mở ra"
    );

    let tail = format!("đọc {r}/AI/hub/rust/src/x.rs và {r}/AI/hub/rust/src/y.rs");
    assert_eq!(
        hub::sessions::folder_from_tail(&tail, &r).as_deref(),
        Some("AI/hub"),
        "đào sâu vào bên trong một dự án"
    );

    // Nhắc đúng MỘT lần thì không đủ để mở ngăn kéo — cùng ngưỡng ≥2 với nhãn.
    let tail = format!("{r}/AI/tcc/amm/x.rs và {r}/AI/tcc/beta3/y.rs");
    assert_eq!(
        hub::sessions::folder_from_tail(&tail, &r).as_deref(),
        Some("AI/tcc"),
        "một lần nhắc mỗi bên mà vẫn chọn bừa một bên"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// Không kiểm được thì GIỮ NGUYÊN nhãn: thư mục không có trên máy này (sổ cũ,
/// máy khác) thì "ngăn kéo hay dự án" là câu hub không trả lời được — và đoán
/// sâu thêm một bậc là đoán.
#[test]
fn an_unknown_folder_keeps_the_shallow_label() {
    let tail = "/khong/co/that/AI/tcc/amm/a.rs và /khong/co/that/AI/tcc/amm/b.rs";
    assert_eq!(
        hub::sessions::folder_from_tail(tail, "/khong/co/that").as_deref(),
        Some("AI/tcc")
    );
}

/// Sổ nhớ cửa sổ nào — nhưng phải bắt `ps` chứng thực trước khi gõ vào đó.
///
/// 🔴 Hà 2026-08-12: *"chát từ tele toàn báo không thấy phiên"* + *"tất cả các
/// lệnh từ tele sao không xử lý luôn lại phải chờ"*. Cả hai là một: `/type`·
/// `/key`·`/shot` dựng lại ảnh chụp (3 lần spawn `claude` 279 MB) chỉ để tra
/// `tty`, mà máy đang swap ⟹ **117–134 giây**, rồi trả "không thấy phiên".
///
/// Sổ trả lời trong vài mili giây — nhưng `tty` một mình KHÔNG đủ, vì tty được
/// dùng lại: cửa sổ ấy có thể đã thuộc phiên khác. `pid` + `tty` mới đủ, và
/// `ps` trả lời cả hai câu cùng lúc.
#[test]
fn a_remembered_window_must_still_belong_to_that_process() {
    let book = |pid: i64, tty: &str, name: &str| {
        format!(
            r#"{{"sess-1":{{"s":"idle","y":"{tty}","k":"interactive","p":"","f":1786500000,"h":false,"n":"{name}","d":"AI/hub","a":"acc1","c":"/x","i":{pid},"o":"terminal"}}}}"#
        )
    };
    let f = hub::sessions::window_target_from_book;

    // Sổ cũ chưa có pid ⟹ không đủ để gõ, rơi về đường ảnh chụp.
    assert!(f(&book(0, "ttys002", "projects-fb"), "sess-1").is_none());
    // Không có cửa sổ thật thì không có gì để gõ vào.
    assert!(f(&book(4242, "??", "projects-fb"), "sess-1").is_none());
    // Tên rỗng: sổ biết id mà không chào được ⟹ để đường kia lo.
    assert!(f(&book(4242, "ttys002", ""), "sess-1").is_none());
    // pid ĐÃ CHẾT: đây là ca nguy hiểm nhất — cửa sổ `ttys002` có thể đang là
    // của phiên khác, và gõ vào đó là gõ vào việc của người khác.
    assert!(f(&book(999_999, "ttys002", "projects-fb"), "sess-1").is_none());
    // Sổ nói một tty KHÁC với chỗ pid đang ngồi ⟹ cũng từ chối.
    let me = std::process::id() as i64;
    assert!(f(&book(me, "ttys999", "projects-fb"), "sess-1").is_none());

    // Ca thuận: chính tiến trình test, nếu lượt chạy này có tty thật.
    let out = std::process::Command::new("ps")
        .args(["-o", "tty=", "-p", &me.to_string()])
        .output()
        .expect("ps");
    let tty = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if hub::sessions::is_real_tty(&tty) {
        let got = f(&book(me, &tty, "projects-fb"), "sess-1").expect("mất cửa sổ đang sống");
        assert_eq!(got.name, "projects-fb");
        assert_eq!(got.tty, tty);
        assert_eq!(got.host, "terminal");
        assert_eq!(got.pid, me);
    }
}

/// Tên phiên phải đọc ra được đang làm dự án nào — và CHỈ thế thôi.
///
/// 🔴 Hà 2026-08-13: *"điều chỉnh lại các chỗ `[hub] project-06` = `[ai/hub]`
/// là được, cho dễ nhận biết"* · *"cần gì đoạn text project-xx làm gì"*. Đúng:
/// `claude` đặt tên theo thư mục MỞ phiên, mà cả máy này mở ở gốc workspace nên
/// mọi phiên đều `projects-xx` — một cái tên không phân biệt được gì, chiếm chỗ
/// của thứ phân biệt được.
#[test]
fn a_session_name_says_which_project_it_is_working_on() {
    let d = hub::sessions::display_name;
    assert_eq!(d("projects-fb", "AI/tcc/amm"), "[AI/tcc/amm]");
    assert_eq!(d("projects-be", "AI/tfl5"), "[AI/tfl5]");
    assert_eq!(d("hanguyen-41", "dwork"), "[dwork]");
    // ĐƯỜNG ĐẦY ĐỦ, không phải mỗi lá cuối: `amm` một mình không nói được nó
    // nằm trong `tcc`.
    assert!(d("x", "AI/tcc/amm").contains("tcc"));
    // Chưa đo được dự án thì GIỮ NGUYÊN tên — thà một cái tên vô nghĩa còn hơn
    // một cặp ngoặc rỗng.
    assert_eq!(d("projects-fb", ""), "projects-fb");
}

/// Nhật ký đang được ghi thì phiên ĐANG CHẠY — kể cả khi CLI khai `idle`.
///
/// 🔴 Hà 2026-08-12: *"trạng thái dừng, đang chạy ở danh sách phiên hình như
/// không đúng"*. Đo đúng lúc ấy: `hanguyen-8e` có `status: "idle"` từ
/// `claude agents` trong khi **nhật ký vừa được ghi 1 giây trước**. Một tệp vừa
/// lớn lên là bằng chứng trực tiếp; `status` là một trường được báo cáo lại, và
/// ở phiên terminal nó trễ hẳn một lượt.
#[test]
fn a_transcript_being_written_beats_a_stale_idle_flag() {
    let w = hub::sessions::is_working;
    // Ca thật đo được: CLI nói idle, nhật ký vừa ghi 1 giây trước.
    assert!(w(Some("idle"), 0, Some(1)), "tin `idle` trong khi nhật ký đang lớn lên");
    assert!(w(Some("done"), 0, Some(3)));
    // …nhưng im lâu rồi thì `idle` vẫn là `idle` — cửa mới không được siết lan.
    assert!(!w(Some("idle"), 0, Some(60)));
    assert!(!w(Some("idle"), 0, None));
    // CLI nói busy thì vẫn tin thẳng.
    assert!(w(Some("busy"), 0, Some(600)));
    // Subagent nền vẫn là bằng chứng mạnh nhất.
    assert!(w(Some("idle"), 2, Some(9_999)));
    // Không có status (phiên terminal cũ): rơi về lưới đỡ mtime.
    assert!(w(None, 0, Some(60)));
    assert!(!w(None, 0, Some(9_999)));
}
