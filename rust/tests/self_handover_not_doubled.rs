//! A session that already opened its own successor must not get a second one.
//!
//! 🔴 Measured 2026-10-08 (reported by [dwork/account], verified in huba.log):
//! session `7096bc70` (81 %) ran `acc-mo-vai.sh dev .tmp/brief-phien-duy-nhat-account.md
//! --acc acc7` at 15:00:27Z; `.acc-mo-vai.log` 22:01:15 rc=0 ⟹ pid 47993 started with
//! argv `cd …/dwork/dev && đọc .tmp/brief-phien-duy-nhat-account.md …`. At 15:05:04Z
//! `auto_handover_firing` still opened `6082b444` (pid 460) — two successors on one
//! tree, both handed the same job. `auto_handover_why` only knows handovers huba itself
//! made; a self-handover was invisible to it.
//!
//! The signal is the OLD session's own transcript (its `acc-mo-vai.sh` call), confirmed
//! by a LIVE `claude` that started AFTER that call and carries the same brief in argv.
//! Anchoring on the session's own transcript is what keeps a second session standing on
//! the same tree (dwork/dev also had `qg-may-xa`, pid 8114) from counting.

use huba::sessions::{self_handover_calls, successor_by_brief};

/// The exact command the old session ran — a heredoc first, the launch on the last line.
const REAL_CMD: &str = "python3 - <<'EOF'\np='/Users/hanguyen/projects/dwork/dev/.tmp/brief-phien-duy-nhat-account.md'\ns=open(p,encoding='utf-8').read()\nprint(\"ok\")\nEOF\nbash /Users/hanguyen/projects/scripts/acc-mo-vai.sh dev .tmp/brief-phien-duy-nhat-account.md --acc acc7 2>&1 | tail -12 | cut -c1-200";

const SUCCESSOR_ARGV: &str = "claude --permission-mode auto --model opus cd /Users/hanguyen/projects/dwork/dev && đọc .tmp/brief-phien-duy-nhat-account.md rồi làm theo. Đó là bản giao việc đầy đủ của bạn";

fn bash_call(id: &str, cmd: &str, ts: &str) -> String {
    serde_json::json!({
        "type": "assistant", "timestamp": ts,
        "message": {"role": "assistant", "stop_reason": "tool_use",
                    "content": [{"type": "tool_use", "id": id, "name": "Bash",
                                 "input": {"command": cmd, "description": "x"}}]}
    })
    .to_string()
}

fn ms(iso: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(iso)
        .unwrap()
        .timestamp_millis()
}

#[test]
fn the_real_launch_line_is_read_as_a_self_handover() {
    let tail = bash_call("t1", REAL_CMD, "2026-10-08T15:00:27.868Z");
    let calls = self_handover_calls(&tail);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].tree, "dev");
    assert_eq!(calls[0].brief, ".tmp/brief-phien-duy-nhat-account.md");
    assert_eq!(calls[0].at_ms, ms("2026-10-08T15:00:27.868Z"));
}

/// Reading the script is not running it — the 14:59:01Z call in the same transcript
/// printed the script's source. And `--tu-kiem` / `--xem` open nothing.
#[test]
fn reading_or_self_testing_the_script_is_not_a_handover() {
    let tail = [
        bash_call(
            "a",
            "sed -n '1,40p' ~/projects/scripts/acc-mo-vai.sh | cut -c1-200",
            "2026-10-08T14:59:01.292Z",
        ),
        bash_call(
            "b",
            "command grep -n 'brief' /Users/hanguyen/projects/scripts/acc-mo-vai.sh",
            "2026-10-08T14:59:02.000Z",
        ),
        bash_call(
            "c",
            "bash ~/projects/scripts/acc-mo-vai.sh --tu-kiem 2>&1 | tail -3",
            "2026-10-08T14:59:03.000Z",
        ),
        bash_call(
            "d",
            "bash ~/projects/scripts/acc-mo-vai.sh --xem",
            "2026-10-08T14:59:04.000Z",
        ),
    ]
    .join("\n");
    assert!(
        self_handover_calls(&tail).is_empty(),
        "{:?}",
        self_handover_calls(&tail)
    );
}

#[test]
fn flags_before_the_positionals_and_quotes_are_tolerated() {
    let tail = bash_call(
        "t1",
        "bash ~/projects/scripts/acc-mo-vai.sh --acc acc5 --model opus dwork/dev-dci \"/Users/hanguyen/projects/dwork/dev-dci/.tmp/brief-dci.md\" --checklist",
        "2026-10-08T10:00:00.000Z",
    );
    let calls = self_handover_calls(&tail);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].tree, "dwork/dev-dci");
    assert_eq!(
        calls[0].brief,
        "/Users/hanguyen/projects/dwork/dev-dci/.tmp/brief-dci.md"
    );
}

/// The live successor: started after the call, carries the brief ⟹ match.
#[test]
fn a_live_session_started_after_the_call_with_that_brief_is_the_successor() {
    let calls = self_handover_calls(&bash_call("t1", REAL_CMD, "2026-10-08T15:00:27.868Z"));
    let cands = vec![(
        "8ed68b81-aaaa".to_string(),
        ms("2026-10-08T15:01:14.000Z"),
        SUCCESSOR_ARGV.to_string(),
    )];
    assert_eq!(
        successor_by_brief(&calls, &cands).map(|(sid, _)| sid),
        Some("8ed68b81-aaaa".to_string())
    );
}

/// Counter-check (b) of the report: another session on the SAME tree, different brief
/// (dwork/dev had `qg-may-xa`, pid 8114) is not this session's successor ⟹ still open.
#[test]
fn another_session_on_the_same_tree_is_not_the_successor() {
    let calls = self_handover_calls(&bash_call("t1", REAL_CMD, "2026-10-08T15:00:27.868Z"));
    let cands = vec![(
        "qgmayxa".to_string(),
        ms("2026-10-08T15:02:00.000Z"),
        "claude --permission-mode auto --model opus cd /Users/hanguyen/projects/dwork/dev && đọc .tmp/brief-qg-may-xa.md rồi làm theo".to_string(),
    )];
    assert_eq!(successor_by_brief(&calls, &cands), None);
}

/// Briefs get reused across generations: a session that STARTED BEFORE the call with
/// the same brief (the old session itself, or its predecessor) is not the successor.
#[test]
fn a_session_older_than_the_call_is_not_the_successor_even_with_the_same_brief() {
    let calls = self_handover_calls(&bash_call("t1", REAL_CMD, "2026-10-08T15:00:27.868Z"));
    let cands = vec![(
        "7096bc70".to_string(),
        ms("2026-10-08T13:30:00.000Z"),
        SUCCESSOR_ARGV.to_string(),
    )];
    assert_eq!(successor_by_brief(&calls, &cands), None);
}

/// Counter-check (a): no call in THIS session's transcript ⟹ nothing to match, even if
/// a fresh session with a brief exists (it belongs to someone else's call).
#[test]
fn without_its_own_call_a_session_has_no_self_successor() {
    let tail = bash_call("x", "cargo test --offline", "2026-10-08T15:00:00.000Z");
    let calls = self_handover_calls(&tail);
    assert!(calls.is_empty());
    let cands = vec![(
        "8ed68b81".to_string(),
        ms("2026-10-08T15:01:14.000Z"),
        SUCCESSOR_ARGV.to_string(),
    )];
    assert_eq!(successor_by_brief(&calls, &cands), None);
}

/// An absolute brief in the call, a relative one in the successor's prompt — same file.
#[test]
fn the_brief_matches_by_file_name_across_absolute_and_relative_forms() {
    let calls = self_handover_calls(&bash_call(
        "t1",
        "bash ~/projects/scripts/acc-mo-vai.sh dev /Users/hanguyen/projects/dwork/dev/.tmp/brief-phien-duy-nhat-account.md",
        "2026-10-08T15:00:27.868Z",
    ));
    let cands = vec![(
        "s".to_string(),
        ms("2026-10-08T15:01:14.000Z"),
        SUCCESSOR_ARGV.to_string(),
    )];
    assert!(successor_by_brief(&calls, &cands).is_some());
}
