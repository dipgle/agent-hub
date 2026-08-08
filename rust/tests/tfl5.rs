//! tfl5 chat adapter — the pure parts, tested without a server.
//!
//! The network halves are exercised for real against a live tfl5 (see
//! `PLAN-portal.md` F0). What lives here is the logic that silently goes wrong
//! without anyone noticing: URL derivation, query escaping, and the handshake
//! error mapping — the last of which exists because tfl5 answers an
//! authenticated-but-unauthorized upgrade with HTTP **200**, not 403.

use hub::adapters::tfl5;
use hub::config::Tfl5Cfg;

fn cfg() -> Tfl5Cfg {
    Tfl5Cfg {
        base_url: "http://localhost:8090".into(),
        app_tid: "a-65dd60d3".into(),
        room: "hub".into(),
        ..Default::default()
    }
}

fn url_of(c: &Tfl5Cfg) -> String {
    tfl5::ws_url(c, &c.app_tid, &c.room)
}

#[test]
fn ws_url_downgrades_http_to_ws() {
    assert_eq!(
        url_of(&cfg()),
        "ws://localhost:8090/ws/chat?app_tid=a-65dd60d3&room=hub"
    );
}

#[test]
fn ws_url_keeps_tls_for_https() {
    // Prod is behind TLS. Silently dialing `ws://` there would either fail or,
    // worse, succeed against a plaintext port.
    let c = Tfl5Cfg {
        base_url: "https://tfl5.example.com".into(),
        ..cfg()
    };
    assert!(url_of(&c).starts_with("wss://tfl5.example.com/ws/chat?"));
}

#[test]
fn ws_url_tolerates_a_trailing_slash() {
    let c = Tfl5Cfg {
        base_url: "http://localhost:8090/".into(),
        ..cfg()
    };
    assert_eq!(url_of(&c), url_of(&cfg()));
}

#[test]
fn room_names_are_escaped_into_the_query() {
    // A room called "a&b" must not turn into a second query parameter — that
    // would silently join the DEFAULT room instead, and the operator would see
    // messages vanish with no error anywhere.
    let c = Tfl5Cfg {
        room: "a&b c".into(),
        ..cfg()
    };
    let url = url_of(&c);
    assert!(url.ends_with("&room=a%26b%20c"), "got {url}");
    assert_eq!(url.matches("room=").count(), 1);
}

#[test]
fn a_queued_reply_goes_to_the_room_it_was_addressed_to() {
    // The config may have moved on between triage and flush. The outbox row
    // wins — otherwise an approved answer surfaces in the wrong conversation.
    let c = Tfl5Cfg {
        app_tid: "a-new".into(),
        room: "elsewhere".into(),
        ..cfg()
    };
    assert_eq!(
        tfl5::parse_target(&c, &tfl5::target_of("a-old", "support")),
        ("a-old".to_string(), "support".to_string())
    );
}

#[test]
fn an_unusable_target_falls_back_to_config_rather_than_vanishing() {
    let c = cfg();
    assert_eq!(
        tfl5::parse_target(&c, ""),
        (c.app_tid.clone(), c.room.clone())
    );
    assert_eq!(
        tfl5::parse_target(&c, "no-colon"),
        (c.app_tid.clone(), c.room.clone())
    );
    assert_eq!(
        tfl5::parse_target(&c, "a-x:"),
        (c.app_tid.clone(), c.room.clone())
    );
}

#[test]
fn urlencode_leaves_the_unreserved_alphabet_alone() {
    assert_eq!(tfl5::urlencode("a-Z_0.9~"), "a-Z_0.9~");
}

/// Build the exact shape tungstenite hands back for a rejected handshake.
fn http_err(status: u16, body: &str) -> tungstenite::Error {
    let resp = tungstenite::http::Response::builder()
        .status(status)
        .body(Some(body.as_bytes().to_vec()))
        .expect("response builds");
    tungstenite::Error::Http(resp)
}

#[test]
fn access_denied_names_the_missing_role_not_the_status() {
    // tfl5 returns 200 here (error.rs:289, deliberate w3c wire compat). Before
    // this mapping the operator saw "HTTP error: 200 OK" and had no idea an
    // ACL grant was missing.
    let e = http_err(
        200,
        r#"{"result":false,"msg":"Access denied","code":"access_denied"}"#,
    );
    let msg = tfl5::ws_connect_error(e, "ws://x/ws/chat", &cfg()).to_string();
    assert!(msg.contains("không có vai"), "got {msg}");
    assert!(msg.contains("a-65dd60d3"), "must name the app: {msg}");
    assert!(msg.contains("/app/acl-set"), "must name the fix: {msg}");
}

#[test]
fn unauthorized_points_at_the_credentials() {
    let e = http_err(
        401,
        r#"{"result":true,"code":"unauthorized","isSignout":true}"#,
    );
    let msg = tfl5::ws_connect_error(e, "ws://x/ws/chat", &cfg()).to_string();
    assert!(msg.contains("HUB_TFL5_PASSWORD"), "got {msg}");
}

#[test]
fn draining_cell_is_reported_as_retryable() {
    let e = http_err(
        503,
        r#"{"result":false,"msg":"node draining","code":"service_draining"}"#,
    );
    let msg = tfl5::ws_connect_error(e, "ws://x/ws/chat", &cfg()).to_string();
    assert!(msg.contains("drain"), "got {msg}");
}

#[test]
fn an_unknown_envelope_still_carries_status_and_body() {
    // Never swallow a rejection just because the shape is unfamiliar.
    let e = http_err(418, r#"{"code":"teapot","msg":"nope"}"#);
    let msg = tfl5::ws_connect_error(e, "ws://x/ws/chat", &cfg()).to_string();
    assert!(msg.contains("418"), "got {msg}");
    assert!(msg.contains("teapot") && msg.contains("nope"), "got {msg}");
}

#[test]
fn a_bodyless_rejection_still_reports_where_it_happened() {
    let resp = tungstenite::http::Response::builder()
        .status(502)
        .body(None)
        .expect("builds");
    let msg = tfl5::ws_connect_error(tungstenite::Error::Http(resp), "ws://x/ws/chat", &cfg())
        .to_string();
    assert!(msg.contains("502"), "got {msg}");
    assert!(msg.contains("ws://x/ws/chat"), "got {msg}");
}

// ----------------------------------------------------------------------
// select_new — the money guard. Tested as a pure function precisely because
// racing a live server to prove a 10-second window is how guards go untested.
// ----------------------------------------------------------------------

use serde_json::json;

const NOW: i64 = 1_800_000_000_000;

fn row(tid: &str, uid: &str, text: &str, ts: i64) -> serde_json::Value {
    json!({ "tid": tid, "from_user_tid": uid, "from": "someone", "text": text, "ts": ts })
}

fn quiet() -> Tfl5Cfg {
    Tfl5Cfg {
        silence_window_sec: 10,
        min_chars: 3,
        ..cfg()
    }
}

#[test]
fn a_message_still_inside_the_window_is_held_not_dropped() {
    let rows = vec![row("cm-1", "u-alice", "câu hỏi vừa gõ xong", NOW - 2_000)];
    let s = tfl5::select_new(&rows, 0, NOW, "u-hubbot", &quiet());
    assert!(
        s.messages.is_empty(),
        "must not triage a message still being typed around"
    );
    assert_eq!(s.held, 1);
    // The cursor must NOT move past it, or it is lost forever.
    assert_eq!(s.newest_ts, 0);
}

#[test]
fn the_same_message_is_taken_once_the_window_passes() {
    let rows = vec![row("cm-1", "u-alice", "câu hỏi vừa gõ xong", NOW - 20_000)];
    let s = tfl5::select_new(&rows, 0, NOW, "u-hubbot", &quiet());
    assert_eq!(s.messages.len(), 1);
    assert_eq!(s.held, 0);
    assert_eq!(s.newest_ts, NOW - 20_000);
}

#[test]
fn a_burst_is_never_split_across_the_window_boundary() {
    // Three lines of one thought: two settled, one still fresh. Taking the
    // first two now would triage half a question and pay for the other half
    // again next cycle.
    let rows = vec![
        row("cm-1", "u-alice", "chào hub", NOW - 30_000),
        row("cm-2", "u-alice", "cho hỏi về vụ deploy", NOW - 20_000),
        row("cm-3", "u-alice", "CI đang đỏ ở main", NOW - 1_000),
    ];
    let s = tfl5::select_new(&rows, 0, NOW, "u-hubbot", &quiet());
    assert_eq!(s.messages.len(), 2, "the settled pair");
    assert_eq!(s.held, 1, "the fresh one waits");
    assert_eq!(
        s.newest_ts,
        NOW - 20_000,
        "cursor stops before the held row"
    );
}

#[test]
fn hubs_own_replies_never_re_enter_the_queue() {
    // Our answer is echoed back on the same feed. Ingesting it would triage
    // hub's own words and bill for the privilege.
    let rows = vec![row(
        "cm-1",
        "u-hubbot",
        "đây là câu trả lời của hub",
        NOW - 60_000,
    )];
    let s = tfl5::select_new(&rows, 0, NOW, "u-hubbot", &quiet());
    assert!(s.messages.is_empty());
    assert_eq!(
        s.newest_ts,
        NOW - 60_000,
        "still advance, or we re-read it forever"
    );
}

#[test]
fn one_word_acknowledgements_are_dropped_and_said_so() {
    let rows = vec![
        row("cm-1", "u-alice", "ok", NOW - 60_000),
        row("cm-2", "u-alice", "👍", NOW - 59_000),
    ];
    let s = tfl5::select_new(&rows, 0, NOW, "u-hubbot", &quiet());
    assert!(
        s.messages.is_empty(),
        "not questions — not worth $0.11 each"
    );
    assert_eq!(s.filtered.len(), 2, "dropped, but never silently");
    assert_eq!(s.newest_ts, NOW - 59_000, "decided, so the cursor moves on");
}

#[test]
fn the_cursor_makes_already_seen_rows_free() {
    let rows = vec![
        row("cm-1", "u-alice", "câu cũ đã xử lý rồi", NOW - 90_000),
        row("cm-2", "u-alice", "câu mới cần trả lời", NOW - 60_000),
    ];
    let s = tfl5::select_new(&rows, NOW - 90_000, NOW, "u-hubbot", &quiet());
    assert_eq!(s.messages.len(), 1);
    assert_eq!(s.messages[0].body.as_deref(), Some("câu mới cần trả lời"));
}

#[test]
fn a_zero_window_takes_everything_immediately() {
    // The escape hatch must actually disable the wait, not clamp to a default.
    let c = Tfl5Cfg {
        silence_window_sec: 0,
        ..quiet()
    };
    let rows = vec![row("cm-1", "u-alice", "gõ xong đúng lúc này", NOW)];
    let s = tfl5::select_new(&rows, 0, NOW, "u-hubbot", &c);
    assert_eq!(s.messages.len(), 1);
    assert_eq!(s.held, 0);
}

#[test]
fn history_order_does_not_change_the_outcome() {
    // tfl5 returns newest-first. If the walk ever stopped honouring that, the
    // cursor could jump a held row and lose it.
    let a = row("cm-1", "u-alice", "câu thứ nhất dài đủ", NOW - 40_000);
    let b = row("cm-2", "u-alice", "câu thứ hai dài đủ", NOW - 30_000);
    let fresh = row("cm-3", "u-alice", "câu vừa gõ xong đây", NOW - 1_000);
    let newest_first = tfl5::select_new(
        &[fresh.clone(), b.clone(), a.clone()],
        0,
        NOW,
        "u-hubbot",
        &quiet(),
    );
    let oldest_first = tfl5::select_new(&[a, b, fresh], 0, NOW, "u-hubbot", &quiet());
    assert_eq!(newest_first.messages.len(), oldest_first.messages.len());
    assert_eq!(newest_first.newest_ts, oldest_first.newest_ts);
    assert_eq!(newest_first.held, oldest_first.held);
}

#[test]
fn an_ingested_row_carries_the_room_and_the_sender_forward() {
    // policy.rs reads `app_tid`/`room` back out of `raw` to address the reply,
    // and `from_user_tid` is what the trust list matches on.
    let rows = vec![row(
        "cm-9",
        "u-alice",
        "câu hỏi đủ dài để qua bộ lọc",
        NOW - 60_000,
    )];
    let m = &tfl5::select_new(&rows, 0, NOW, "u-hubbot", &quiet()).messages[0];
    assert_eq!(m.source, "tfl5");
    assert_eq!(m.external_id, "tfl5:cm-9");
    assert_eq!(
        m.thread_key.as_deref(),
        Some("tfl5:a-65dd60d3:hub"),
        "one room = one coalescing thread"
    );
    assert_eq!(m.sender.as_deref(), Some("tfl5:u-alice"));
    assert_eq!(m.sender_trust, None, "room membership is not hub trust");
    let raw = m.raw.as_ref().unwrap();
    assert_eq!(raw["app_tid"], "a-65dd60d3");
    assert_eq!(raw["room"], "hub");
    assert_eq!(raw["from_user_tid"], "u-alice");
}

// ----------------------------------------------------------------------
// Slash commands in the room. The trust check is the security boundary:
// tfl5 decides who may ENTER the room, hub decides who may give it ORDERS.
// ----------------------------------------------------------------------

const OWNER: &str = "u-owner";

fn owners() -> Vec<String> {
    vec![OWNER.to_string()]
}

#[test]
fn the_owner_can_approve_from_the_room() {
    let (kind, id, arg) = tfl5::parse_command("/approve 12", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Approve);
    assert_eq!(id, 12);
    assert_eq!(arg, "");
}

#[test]
fn a_reject_reason_survives_into_the_books() {
    let (kind, id, arg) =
        tfl5::parse_command("/reject 12 câu trả lời sai chỗ", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Reject);
    assert_eq!(id, 12);
    assert_eq!(arg, "câu trả lời sai chỗ");
}

#[test]
fn a_stranger_typing_slash_approve_is_just_typing() {
    // THE BOUNDARY. Being in the room must never be enough to approve an
    // outbound reply. It is not silently dropped either — returning None means
    // it stays an ordinary message and goes through triage like anything else.
    assert!(tfl5::parse_command("/approve 12", "u-stranger", &owners()).is_none());
}

#[test]
fn an_empty_owner_list_grants_nobody_command_rights() {
    // Fail closed: an unconfigured trust list must not mean "everyone".
    assert!(tfl5::parse_command("/approve 12", OWNER, &[]).is_none());
    assert!(tfl5::parse_command("/approve 12", "", &[]).is_none());
}

#[test]
fn a_side_question_keeps_every_word_including_the_slashes() {
    // The whole remainder is the question, not just the first token — a
    // question is a sentence, and one that arrives truncated is worse than one
    // refused. It also must survive text that looks like more commands.
    let (kind, id, arg) = tfl5::parse_command(
        "/ask lệnh /run vừa rồi đã chạy xong chưa, còn kẹt ở đâu?",
        OWNER,
        &owners(),
    )
    .expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Ask);
    assert_eq!(id, 0, "the target is the focused session, never an id");
    assert_eq!(arg, "lệnh /run vừa rồi đã chạy xong chưa, còn kẹt ở đâu?");

    let (kind, _, arg) = tfl5::parse_command("/hoi đang làm gì đấy", OWNER, &owners()).unwrap();
    assert_eq!(kind, hub::adapters::CommandKind::Ask);
    assert_eq!(arg, "đang làm gì đấy");
}

#[test]
fn an_empty_side_question_never_reaches_the_wallet() {
    // `/ask` alone would otherwise pay for a `claude` call that answers
    // nothing. Returning None keeps it an ordinary message, so the person sees
    // it was not understood instead of being billed for silence.
    for t in ["/ask", "/ask   ", "/hoi"] {
        assert!(
            tfl5::parse_command(t, OWNER, &owners()).is_none(),
            "sai với: {t}"
        );
    }
}

#[test]
fn a_stranger_cannot_spend_money_asking() {
    // `/ask` forks a session and bills the owner, so it sits behind the same
    // gate as `/approve` — being in the room is tfl5's decision, spending the
    // owner's money is not.
    assert!(
        tfl5::parse_command("/ask bí mật gì trong phiên đó?", "u-stranger", &owners()).is_none()
    );
    assert!(tfl5::parse_command("/ask bí mật gì trong phiên đó?", OWNER, &[]).is_none());
}

#[test]
fn starting_a_session_needs_both_a_project_and_a_task() {
    let (kind, id, arg) =
        tfl5::parse_command("/new tfl5 sửa nút Releases bị vỡ regex", OWNER, &owners())
            .expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::New);
    assert_eq!(id, 0);
    assert_eq!(arg, "tfl5 sửa nút Releases bị vỡ regex");

    // A project with no task would start an agent with nothing to do — and it
    // would still cost money while it worked that out.
    for t in ["/new", "/new tfl5", "/new   "] {
        assert!(
            tfl5::parse_command(t, OWNER, &owners()).is_none(),
            "sai với: {t}"
        );
    }
}

#[test]
fn stop_defaults_to_the_session_being_read() {
    let (kind, _, arg) = tfl5::parse_command("/stop", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Stop);
    assert_eq!(arg, "", "empty means: whatever /session is following");

    let (_, _, arg) = tfl5::parse_command("/stop a3a24ccd-6ad8", OWNER, &owners()).unwrap();
    assert_eq!(arg, "a3a24ccd-6ad8");
}

#[test]
fn telling_a_session_keeps_the_whole_sentence() {
    let (kind, id, arg) = tfl5::parse_command(
        "/tell chạy lại test rồi báo kết quả, đừng commit",
        OWNER,
        &owners(),
    )
    .expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Tell);
    assert_eq!(id, 0);
    assert_eq!(arg, "chạy lại test rồi báo kết quả, đừng commit");
    assert!(tfl5::parse_command("/tell", OWNER, &owners()).is_none());
}

#[test]
fn a_stranger_cannot_start_or_steer_a_session() {
    // These three spend money and run tools on the owner's machine, so they sit
    // behind exactly the same gate as `/approve`.
    for t in ["/new tfl5 xoá hết đi", "/stop", "/tell chạy rm -rf"] {
        assert!(
            tfl5::parse_command(t, "u-stranger", &owners()).is_none(),
            "người lạ không được gọi: {t}"
        );
        assert!(
            tfl5::parse_command(t, OWNER, &[]).is_none(),
            "danh sách chủ rỗng thì không ai được gọi: {t}"
        );
    }
}

#[test]
fn act_is_refused_from_chat_on_purpose() {
    // It writes code and can run for half an hour. A chat keyboard is the wrong
    // trigger, and blocking the poll loop on it would be worse.
    let (kind, id, _) = tfl5::parse_command("/act 12", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::ActRefused);
    assert_eq!(id, 12);
}

#[test]
fn ordinary_text_is_never_mistaken_for_a_command() {
    for t in [
        "cho tôi hỏi /approve nghĩa là gì?",
        "đường dẫn là /var/log/app.log",
        "/approve",          // no id
        "/approve abc",      // not a number
        "/khong-ton-tai 12", // unknown verb
    ] {
        assert!(
            tfl5::parse_command(t, OWNER, &owners()).is_none(),
            "sai với: {t}"
        );
    }
}

#[test]
fn help_needs_no_decision_id() {
    let (kind, id, _) = tfl5::parse_command("/help", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Help);
    assert_eq!(id, 0);
}

#[test]
fn the_owner_can_close_a_message_from_the_room() {
    let (kind, id, arg) =
        tfl5::parse_command("/close 41 hết giá trị", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Close);
    assert_eq!(id, 41, "close takes a MESSAGE id, not a decision id");
    assert_eq!(arg, "hết giá trị");
    // Reason is optional.
    let (kind, id, arg) = tfl5::parse_command("/close 41", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Close);
    assert_eq!(id, 41);
    assert_eq!(arg, "");
}

#[test]
fn reply_carries_the_text_and_refuses_to_send_nothing() {
    let (kind, id, arg) =
        tfl5::parse_command("/reply 41 mình đã xử lý xong", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Reply);
    assert_eq!(id, 41);
    assert_eq!(arg, "mình đã xử lý xong");
    // An empty reply would queue a blank message on a real channel.
    assert!(tfl5::parse_command("/reply 41", OWNER, &owners()).is_none());
    assert!(tfl5::parse_command("/reply 41    ", OWNER, &owners()).is_none());
}

#[test]
fn the_new_verbs_are_owner_only_too() {
    // Same boundary as approve: a stranger in the room must not be able to
    // close someone's item or send mail in hub's name.
    assert!(tfl5::parse_command("/close 41", "u-stranger", &owners()).is_none());
    assert!(tfl5::parse_command("/reply 41 xin chào", "u-stranger", &owners()).is_none());
}

/// REGRESSION (2026-08-07, cost real money): the live socket ingested every
/// frame as a message without ever asking `parse_command`, so an owner's
/// `/close 155` was executed by the poller AND stored AND sent to the model —
/// $0.18 to classify the word "close". The two paths must agree on what a
/// command is; this pins the shared predicate they now both use.
#[test]
fn the_live_path_and_the_poller_agree_on_what_counts_as_a_command() {
    for text in [
        "/close 155 hết giá trị",
        "/approve 12",
        "/reject 12 sai",
        "/reply 9 xong",
    ] {
        assert!(
            tfl5::parse_command(text, OWNER, &owners()).is_some(),
            "cả hai đường phải coi đây là LỆNH, không phải tin nhắn: {text}"
        );
    }
    // ...and ordinary chat must still be a message on both paths, or the live
    // socket would start silently swallowing questions.
    for text in ["hôm nay CI sao rồi?", "/close", "closing the issue"] {
        assert!(
            tfl5::parse_command(text, OWNER, &owners()).is_none(),
            "đây là tin nhắn thường, không được nuốt: {text}"
        );
    }
}

#[test]
fn set_needs_both_a_key_and_a_value() {
    let (kind, _, arg) =
        tfl5::parse_command("/set autonomy.default L1", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::SetConfig);
    assert_eq!(arg, "autonomy.default L1");
    // A key with no value would blank the field — refuse it here.
    assert!(tfl5::parse_command("/set autonomy.default", OWNER, &owners()).is_none());
    assert!(tfl5::parse_command("/set", OWNER, &owners()).is_none());
    // And it stays owner-only, like every other verb.
    assert!(tfl5::parse_command("/set autonomy.default L2", "u-stranger", &owners()).is_none());
}

#[test]
fn the_cycle_verbs_take_no_id() {
    for (text, want) in [
        ("/ingest", hub::adapters::CommandKind::Ingest),
        ("/poll", hub::adapters::CommandKind::Ingest),
        ("/run", hub::adapters::CommandKind::Run),
        ("/doctor", hub::adapters::CommandKind::Doctor),
    ] {
        let (kind, id, _) = tfl5::parse_command(text, OWNER, &owners()).expect(text);
        assert_eq!(kind, want, "sai verb cho {text}");
        assert_eq!(id, 0, "{text} không nhận id");
    }
}

#[test]
fn project_pin_reads_shows_and_clears() {
    let (kind, _, arg) = tfl5::parse_command("/project tfl5", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Project);
    assert_eq!(arg, "tfl5");
    // No name = "what is pinned right now?"
    let (kind, _, arg) = tfl5::parse_command("/project", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Project);
    assert_eq!(arg, "");
    // "-" clears it.
    let (_, _, arg) = tfl5::parse_command("/project -", OWNER, &owners()).expect("parsed");
    assert_eq!(arg, "-");
    // Still owner-only: the pin decides where every later question is routed.
    assert!(tfl5::parse_command("/project sdvi", "u-stranger", &owners()).is_none());
}

#[test]
fn a_reply_marker_is_stripped_and_carried_separately() {
    // tfl5's chat has no reply field, so the page encodes it in the text.
    // The model must never see the plumbing, and the tid must survive.
    let (tid, body) = tfl5::split_reply_marker("↩[cm-abc123] còn cái này thì sao");
    assert_eq!(tid.as_deref(), Some("cm-abc123"));
    assert_eq!(body, "còn cái này thì sao");

    // Ordinary text is untouched — including text that merely mentions the
    // arrow, or a malformed marker.
    for plain in [
        "bình thường",
        "↩ không có ngoặc",
        "↩[] rỗng",
        "dùng ↩[x] ở giữa",
    ] {
        let (tid, body) = tfl5::split_reply_marker(plain);
        assert!(tid.is_none(), "không được coi là reply: {plain}");
        assert_eq!(body, plain);
    }
}
