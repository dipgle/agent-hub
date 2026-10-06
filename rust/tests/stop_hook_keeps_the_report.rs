//! A Stop hook that makes the session go on must not erase what it said
//! when it stopped.
//!
//! 🔴 Measured 2026-10-06 on session `5ed3d329`: the turn's report ended at
//! 14:07:47.491Z with a line `📎 /…/huba-windows-a2498af.zip` — the one shape
//! huba promises to turn into a tap-to-send link. 87 ms later the workspace Stop
//! hook (`scripts/nhac-chuyen-phien.py`, context ≥ 50 %) answered
//! `{"decision":"block"}`, the CLI wrote a `user` record `Stop hook feedback: …`
//! (`isMeta: true`) and the SAME turn ran on until 14:09:20Z. The CLI never went
//! idle in between, so there was exactly one 💤 — and it carried only the prose
//! written after the hook. The report, and its `📎`, never reached the phone.
//!
//! Not a one-off: over 14 days of transcripts (3,704 files, 0 unreadable)
//! 195 end-of-turn reports were cut off this way, 2 of them carrying a `📎`
//! line (`.tmp/do-stop-hook-noi-luot/dem.py`).
//!
//! At the terminal both are on screen, one after the other. The bridge must
//! carry both (CLAUDE.md, "the one test").
//!
//! Records below keep the real shape of `5ed3d329` lines 1862–1872 and the
//! final turn; only the prose is shortened.

const REPORT: &str = "[huba] Đã cài và đang chạy: hubd pid 60253. Hai việc của bạn đều xong.\n\n**Dùng thử luôn:** dòng dưới đây là gói Windows.\n📎 /Users/hanguyen/projects/huba/.tmp/huba-windows-a2498af.zip";
const FINAL: &str = "[huba] Đã mở phiên huba kế nhiệm ở cửa sổ Terminal 13924.\n\nViệc còn chờ bạn: bấm vào dòng 📎 gói Windows.";

fn assistant(text: &str, stop: &str, ts: &str) -> String {
    serde_json::json!({
        "type": "assistant", "timestamp": ts,
        "message": {"role": "assistant", "stop_reason": stop,
                    "content": [{"type": "text", "text": text}]}
    })
    .to_string()
}

fn thinking(stop: &str, ts: &str) -> String {
    serde_json::json!({
        "type": "assistant", "timestamp": ts,
        "message": {"role": "assistant", "stop_reason": stop,
                    "content": [{"type": "thinking", "thinking": ""}]}
    })
    .to_string()
}

fn tool_call(ts: &str) -> String {
    serde_json::json!({
        "type": "assistant", "timestamp": ts,
        "message": {"role": "assistant", "stop_reason": "tool_use",
                    "content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {}}]}
    })
    .to_string()
}

fn tool_result(ts: &str) -> String {
    serde_json::json!({
        "type": "user", "timestamp": ts,
        "message": {"role": "user",
                    "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "0\t0"}]}
    })
    .to_string()
}

fn hook_feedback(ts: &str) -> String {
    serde_json::json!({
        "type": "user", "isMeta": true, "timestamp": ts,
        "message": {"role": "user",
                    "content": "Stop hook feedback:\n[Nhắc tự động của Hà — ngữ cảnh phiên 56% ≥ 50%] còn việc gì đang làm giở thì làm gọn rồi chuyển phiên đi"}
    })
    .to_string()
}

fn prompt(text: &str, ts: &str) -> String {
    serde_json::json!({
        "type": "user", "timestamp": ts,
        "message": {"role": "user", "content": text}
    })
    .to_string()
}

/// Bookkeeping the CLI writes around a hook — must not break the walk.
fn hook_bookkeeping(ts: &str) -> Vec<String> {
    vec![
        serde_json::json!({"type": "attachment", "timestamp": ts,
            "attachment": {"type": "hook_blocking_error", "hookEvent": "Stop"}})
        .to_string(),
        serde_json::json!({"type": "system", "subtype": "stop_hook_summary", "timestamp": ts})
            .to_string(),
    ]
}

/// The turn exactly as it happened: report ⟹ hook ⟹ work ⟹ final words.
fn hooked_turn() -> String {
    let mut lines = vec![
        prompt("Làm tiếp các mục còn lại", "2026-10-06T13:41:00.000Z"),
        tool_call("2026-10-06T13:41:05.000Z"),
        tool_result("2026-10-06T13:41:06.000Z"),
        thinking("end_turn", "2026-10-06T14:07:45.744Z"),
        assistant(REPORT, "end_turn", "2026-10-06T14:07:47.491Z"),
        hook_feedback("2026-10-06T14:07:47.578Z"),
    ];
    lines.extend(hook_bookkeeping("2026-10-06T14:07:47.580Z"));
    lines.extend([
        thinking("tool_use", "2026-10-06T14:07:56.563Z"),
        assistant(
            "[huba] Chuyển phiên. Mọi việc đã đẩy.",
            "tool_use",
            "2026-10-06T14:07:57.461Z",
        ),
        tool_call("2026-10-06T14:07:58.963Z"),
        tool_result("2026-10-06T14:08:01.285Z"),
        thinking("end_turn", "2026-10-06T14:09:20.479Z"),
        assistant(FINAL, "end_turn", "2026-10-06T14:09:20.481Z"),
    ]);
    lines.join("\n")
}

#[test]
fn the_report_a_stop_hook_interrupted_stays_in_the_last_words() {
    let (said, at) =
        huba::sessions::last_prose_at(&hooked_turn(), huba::sessions::SAY_MAX).expect("có lời");
    assert!(
        said.contains("📎 /Users/hanguyen/projects/huba/.tmp/huba-windows-a2498af.zip"),
        "bản báo cáo trước hook bị đánh rơi:\n{said}"
    );
    assert!(
        said.contains("Đã mở phiên huba kế nhiệm"),
        "lời cuối phải còn:\n{said}"
    );
    // Order is the order on screen: the report first, the final words last —
    // `key_points` keeps the TAIL, so a reversed join would bury the close.
    let r = said.find("Hai việc của bạn đều xong").unwrap();
    let f = said.find("Đã mở phiên huba kế nhiệm").unwrap();
    assert!(r < f, "thứ tự phải theo màn:\n{said}");
    // Mid-turn prose (stop_reason tool_use) is narration, not a stop point.
    assert!(!said.contains("Chuyển phiên. Mọi việc đã đẩy"), "{said}");
    // The age is the age of the FINAL words: that is when the turn ended.
    assert_eq!(at.as_deref(), Some("2026-10-06T14:09:20.481Z"));
}

/// The whole chain to the 💤 message: the `📎` line must survive `key_points`,
/// or the in-text link the 2026-10-06 rule promises has nothing to sit on.
#[test]
fn the_chain_to_the_message_keeps_the_file_mark() {
    let said = huba::sessions::last_prose(&hooked_turn(), huba::sessions::SAY_MAX).unwrap();
    let points = huba::watch::key_points(&said, 700);
    assert!(
        points.contains("📎 /Users/hanguyen/projects/huba/.tmp/huba-windows-a2498af.zip"),
        "dòng 📎 rơi khỏi bản rút gọn:\n{points}"
    );
    assert!(
        points.contains("Việc còn chờ bạn"),
        "câu chốt phải còn:\n{points}"
    );
    // …and the file is still found by the scanner that builds the link.
    assert_eq!(
        huba::keys::file_markers(&said),
        vec!["/Users/hanguyen/projects/huba/.tmp/huba-windows-a2498af.zip".to_string()]
    );
}

/// A report from the PREVIOUS turn is not part of this one, hook or not: the
/// owner's prompt in between is the boundary.
#[test]
fn a_hooked_report_from_an_earlier_turn_does_not_leak_in() {
    let mut lines = vec![
        assistant(
            "Báo cáo của lượt TRƯỚC.",
            "end_turn",
            "2026-10-06T10:00:00.000Z",
        ),
        hook_feedback("2026-10-06T10:00:00.100Z"),
        assistant(
            "Lời cuối của lượt trước.",
            "end_turn",
            "2026-10-06T10:01:00.000Z",
        ),
        prompt("Câu mới của chủ máy", "2026-10-06T11:00:00.000Z"),
        tool_call("2026-10-06T11:00:05.000Z"),
        tool_result("2026-10-06T11:00:06.000Z"),
    ];
    lines.push(assistant(
        "Lời của lượt này.",
        "end_turn",
        "2026-10-06T11:01:00.000Z",
    ));
    let said = huba::sessions::last_prose(&lines.join("\n"), 2000).unwrap();
    assert_eq!(said, "Lời của lượt này.");
}

/// No hook between two proses ⟹ the old rule holds: the last word is the LAST
/// word (`tests/sessions.rs::the_last_word_goes_out_whole_and_is_never_an_older_one`).
#[test]
fn without_a_hook_an_earlier_end_of_turn_is_not_pulled_in() {
    let lines = [
        assistant("Lượt cũ, đã khép.", "end_turn", "2026-10-06T10:00:00.000Z"),
        tool_result("2026-10-06T10:00:01.000Z"),
        assistant("Lời mới nhất.", "end_turn", "2026-10-06T10:01:00.000Z"),
    ];
    let said = huba::sessions::last_prose(&lines.join("\n"), 2000).unwrap();
    assert_eq!(said, "Lời mới nhất.");
}

/// The hook just fired and nothing new is written yet: the report IS the last
/// word, alone — not duplicated, not lost.
#[test]
fn right_after_the_hook_the_report_is_the_last_word() {
    let lines = [
        prompt("Làm đi", "2026-10-06T14:00:00.000Z"),
        assistant(REPORT, "end_turn", "2026-10-06T14:07:47.491Z"),
        hook_feedback("2026-10-06T14:07:47.578Z"),
    ];
    let said = huba::sessions::last_prose(&lines.join("\n"), 2000).unwrap();
    assert_eq!(said, REPORT);
}

/// Two hooks in one turn (a project can install its own Stop hook) ⟹ every
/// stop point is kept, in screen order.
#[test]
fn every_stop_point_of_the_turn_is_kept_in_order() {
    let lines = [
        prompt("Làm đi", "2026-10-06T14:00:00.000Z"),
        assistant("Một: báo cáo đầu.", "end_turn", "2026-10-06T14:01:00.000Z"),
        hook_feedback("2026-10-06T14:01:00.100Z"),
        tool_call("2026-10-06T14:01:05.000Z"),
        tool_result("2026-10-06T14:01:06.000Z"),
        assistant(
            "Hai: sau hook thứ nhất.",
            "end_turn",
            "2026-10-06T14:02:00.000Z",
        ),
        hook_feedback("2026-10-06T14:02:00.100Z"),
        assistant("Ba: lời cuối.", "end_turn", "2026-10-06T14:03:00.000Z"),
    ];
    let said = huba::sessions::last_prose(&lines.join("\n"), 2000).unwrap();
    let a = said.find("Một").expect(&said);
    let b = said.find("Hai").expect(&said);
    let c = said.find("Ba").expect(&said);
    assert!(a < b && b < c, "{said}");
}
