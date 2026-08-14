//! Bộ phân tích MỆNH LỆNH — kiểm không cần máy chủ nào.
//!
//! 🔴 Tệp này từng tên là `tests/tfl5.rs` và kiểm cả một cái kênh: dựng URL,
//! thoát chuỗi truy vấn, ánh xạ lỗi bắt tay websocket, cửa sổ im lặng. Kênh ấy
//! gỡ ngày 2026-08-14 (Hà: *"tạm thời không dùng tfl5 để xem cứ xóa hết đi"*),
//! và những bài kiểm ấy đi theo nó.
//!
//! Thứ ở lại là phần đã luôn đúng ở mọi kênh: chữ người gõ → một route. Cửa tin
//! cậy trong đó là ranh giới an toàn thật sự — kênh quyết ai được VÀO phòng,
//! hub quyết ai được RA LỆNH cho cái máy này.

use hub::verbs::parse_command;

// ----------------------------------------------------------------------
// Slash commands in the room. The trust check is the security boundary:
// tfl5 decides who may ENTER the room, hub decides who may give it ORDERS.
// ----------------------------------------------------------------------

const OWNER: &str = "u-owner";

fn owners() -> Vec<String> {
    vec![OWNER.to_string()]
}

#[test]
fn a_stranger_typing_a_command_is_just_typing() {
    // THE BOUNDARY. Being in the room must never be enough to drive this Mac.
    // It is not silently dropped either — returning None means the line stays
    // an ordinary message in an ordinary conversation.
    assert!(parse_command("/stop", "u-stranger", &owners()).is_none());
    assert!(parse_command("/new tfl5 sửa CI", "u-stranger", &owners()).is_none());
}

#[test]
fn an_empty_owner_list_grants_nobody_command_rights() {
    // Fail closed: an unconfigured trust list must not mean "everyone".
    assert!(parse_command("/stop", OWNER, &[]).is_none());
    assert!(parse_command("/approve 12", "", &[]).is_none());
}

/// `/sessions` (số nhiều) hỏi DANH SÁCH; `/session <id>` chọn một phiên.
///
/// Cùng một route, và cái tên số nhiều tồn tại vì người ta hỏi "có những phiên
/// nào" bằng số nhiều — bắt nhớ "gõ /session không tham số" là bắt nhớ một luật
/// của mã. Số nhiều **không nhận id**: `/sessions <id>` là gõ nhầm, mà im lặng
/// theo một phiên vì gõ nhầm thì mọi lệnh sau đó (`/tell`, `/type`, `/key`) đi
/// vào sai cửa sổ — đúng con bug 2026-08-11 sáng.
#[test]
fn the_plural_asks_for_the_list_and_never_picks_a_session() {
    for line in ["/sessions", "/phiens", "/danhsach"] {
        let (kind, id, arg) = parse_command(line, OWNER, &owners()).expect("parsed");
        assert_eq!(kind, hub::adapters::CommandKind::Session, "{line}");
        assert_eq!(id, 0, "{line}");
        assert_eq!(arg, "", "{line} phải là danh sách, không mang id");
    }
    let (_, _, arg) =
        parse_command("/sessions 3e9a7fd6-3050", OWNER, &owners()).expect("parsed");
    assert_eq!(arg, "", "số nhiều mà nuốt id thì nó lặng lẽ đổi phiên đang theo");

    // Số ít vẫn giữ nguyên hai nghĩa cũ.
    let (_, _, arg) = parse_command("/session abc-123", OWNER, &owners()).expect("parsed");
    assert_eq!(arg, "abc-123");
    let (_, _, arg) = parse_command("/session", OWNER, &owners()).expect("parsed");
    assert_eq!(arg, "");
}

#[test]
fn a_side_question_keeps_every_word_including_the_slashes() {
    // The whole remainder is the question, not just the first token — a
    // question is a sentence, and one that arrives truncated is worse than one
    // refused. It also must survive text that looks like more commands.
    let (kind, id, arg) = parse_command(
        "/ask lệnh /run vừa rồi đã chạy xong chưa, còn kẹt ở đâu?",
        OWNER,
        &owners(),
    )
    .expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Ask);
    assert_eq!(id, 0, "the target is the focused session, never an id");
    assert_eq!(arg, "lệnh /run vừa rồi đã chạy xong chưa, còn kẹt ở đâu?");

    let (kind, _, arg) = parse_command("/hoi đang làm gì đấy", OWNER, &owners()).unwrap();
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
            parse_command(t, OWNER, &owners()).is_none(),
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
        parse_command("/ask bí mật gì trong phiên đó?", "u-stranger", &owners()).is_none()
    );
    assert!(parse_command("/ask bí mật gì trong phiên đó?", OWNER, &[]).is_none());
}

#[test]
fn starting_a_session_needs_both_a_project_and_a_task() {
    let (kind, id, arg) =
        parse_command("/new tfl5 sửa nút Releases bị vỡ regex", OWNER, &owners())
            .expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::New);
    assert_eq!(id, 0);
    assert_eq!(arg, "tfl5 sửa nút Releases bị vỡ regex");

    // A project with no task would start an agent with nothing to do — and it
    // would still cost money while it worked that out.
    for t in ["/new", "/new tfl5", "/new   "] {
        assert!(
            parse_command(t, OWNER, &owners()).is_none(),
            "sai với: {t}"
        );
    }
}

#[test]
fn stop_defaults_to_the_session_being_read() {
    let (kind, _, arg) = parse_command("/stop", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Stop);
    assert_eq!(arg, "", "empty means: whatever /session is following");

    let (_, _, arg) = parse_command("/stop a3a24ccd-6ad8", OWNER, &owners()).unwrap();
    assert_eq!(arg, "a3a24ccd-6ad8");
}

#[test]
fn telling_a_session_keeps_the_whole_sentence() {
    let (kind, id, arg) = parse_command(
        "/tell chạy lại test rồi báo kết quả, đừng commit",
        OWNER,
        &owners(),
    )
    .expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Tell);
    assert_eq!(id, 0);
    assert_eq!(arg, "chạy lại test rồi báo kết quả, đừng commit");
    assert!(parse_command("/tell", OWNER, &owners()).is_none());
}

#[test]
fn a_stranger_cannot_start_or_steer_a_session() {
    // These three spend money and run tools on the owner's machine, so they sit
    // behind exactly the same gate as `/approve`.
    for t in ["/new tfl5 xoá hết đi", "/stop", "/tell chạy rm -rf"] {
        assert!(
            parse_command(t, "u-stranger", &owners()).is_none(),
            "người lạ không được gọi: {t}"
        );
        assert!(
            parse_command(t, OWNER, &[]).is_none(),
            "danh sách chủ rỗng thì không ai được gọi: {t}"
        );
    }
}

#[test]
fn ordinary_text_is_never_mistaken_for_a_command() {
    for t in [
        "cho tôi hỏi /session nghĩa là gì?",
        "đường dẫn là /var/log/app.log",
        "/khong-ton-tai 12", // unknown verb
        // Gone with the inbox on 2026-08-08. They must read as TEXT now, not as
        // a command the room accepts and then does nothing about.
        "/approve 12",
        "/reject 12 sai",
        // ⚠ `/close` KHÔNG còn ở danh sách này: 2026-08-13 nó được lấy lại làm
        // một route SỐNG (đóng hẳn một phiên — Hà: *"ah stop là dừng rồi vậy
        // dùng close"*). Động từ của thời hộp thư chết đi thì tên của nó là chỗ
        // trống, và lấy lại một cái tên trống là chuyện bình thường — chỉ phải
        // nói ra ở đúng chỗ đang canh nó, không thì lượt sau có người đọc test
        // này và tưởng route mới là một lỗi.
        "/reply 9 xong",
        "/act 12",
    ] {
        assert!(
            parse_command(t, OWNER, &owners()).is_none(),
            "sai với: {t}"
        );
    }
}

#[test]
fn help_needs_no_decision_id() {
    let (kind, id, _) = parse_command("/help", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Help);
    assert_eq!(id, 0);
}

#[test]
fn the_new_verbs_are_owner_only_too() {
    // Same boundary as approve: a stranger in the room must not be able to
    // close someone's item or send mail in hub's name.
    assert!(parse_command("/close 41", "u-stranger", &owners()).is_none());
    assert!(parse_command("/reply 41 xin chào", "u-stranger", &owners()).is_none());
}

/// REGRESSION (2026-08-07, cost real money): the live socket ingested every
/// frame as a message without ever asking `parse_command`, so an owner's
/// `/close 155` was executed by the poller AND stored AND sent to the model —
/// $0.18 to classify the word "close". The two paths must agree on what a
/// command is; this pins the shared predicate they now both use.
#[test]
fn the_live_path_and_the_poller_agree_on_what_counts_as_a_command() {
    for text in [
        "/session 1a2b3c4d",
        "/stop",
        "/handover",
        "/tell chạy nốt test đi",
    ] {
        assert!(
            parse_command(text, OWNER, &owners()).is_some(),
            "cả hai đường phải coi đây là LỆNH, không phải tin nhắn: {text}"
        );
    }
    // ...and ordinary chat must still be a message on both paths, or the live
    // socket would start silently swallowing questions.
    for text in ["hôm nay CI sao rồi?", "/approve 12", "closing the issue"] {
        assert!(
            parse_command(text, OWNER, &owners()).is_none(),
            "đây là tin nhắn thường, không được nuốt: {text}"
        );
    }
}

#[test]
fn set_needs_both_a_key_and_a_value() {
    let (kind, _, arg) =
        parse_command("/set autonomy.default L1", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::SetConfig);
    assert_eq!(arg, "autonomy.default L1");
    // A key with no value would blank the field — refuse it here.
    assert!(parse_command("/set autonomy.default", OWNER, &owners()).is_none());
    assert!(parse_command("/set", OWNER, &owners()).is_none());
    // And it stays owner-only, like every other verb.
    assert!(parse_command("/set autonomy.default L2", "u-stranger", &owners()).is_none());
}

#[test]
fn the_cycle_verbs_take_no_id() {
    for (text, want) in [
        ("/ingest", hub::adapters::CommandKind::Ingest),
        ("/poll", hub::adapters::CommandKind::Ingest),
        ("/run", hub::adapters::CommandKind::Run),
        ("/doctor", hub::adapters::CommandKind::Doctor),
    ] {
        let (kind, id, _) = parse_command(text, OWNER, &owners()).expect(text);
        assert_eq!(kind, want, "sai verb cho {text}");
        assert_eq!(id, 0, "{text} không nhận id");
    }
}

#[test]
fn project_pin_reads_shows_and_clears() {
    let (kind, _, arg) = parse_command("/project tfl5", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Project);
    assert_eq!(arg, "tfl5");
    // No name = "what is pinned right now?"
    let (kind, _, arg) = parse_command("/project", OWNER, &owners()).expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Project);
    assert_eq!(arg, "");
    // "-" clears it.
    let (_, _, arg) = parse_command("/project -", OWNER, &owners()).expect("parsed");
    assert_eq!(arg, "-");
    // Still owner-only: the pin decides where every later question is routed.
    assert!(parse_command("/project sdvi", "u-stranger", &owners()).is_none());
}


/// 🔴 Mệnh lệnh đụng vào một phiên sống thì phải TỰ NÓI nó đụng vào phiên nào.
///
/// Đo 2026-08-11, lỗi nặng nhất trong ngày: `/ask`, `/tell`, `/type`, `/key`
/// định vị bằng con trỏ `focus:session` — một biến toàn cục mà một lệnh KHÁC
/// đặt. Trang vì thế gửi hai bản ghi (`/session <id>` rồi `/ask <câu>`), và
/// phòng chat KHÔNG bảo đảm thứ tự:
///
/// ```text
/// 10:32:38  /session 3e9a7fd6…      ← hoãn
/// 10:32:51  /ask Tóm tắt…           ← hoãn
///           ack: "Hỏi bên lề phiên projects-1f"   ← SAI PHIÊN, hub gõ thật vào đó
/// 10:33:42  ack: "Đang theo phiên projects-ff"    ← lệnh trước, chạy sau
/// ```
#[test]
fn an_order_that_touches_a_session_carries_that_sessions_id() {
    use hub::pipeline::split_target;

    let (id, rest) = split_target("3e9a7fd6-3050-4a54-ba52-0dfb24de033c Tóm tắt trong 1 câu?")
        .expect("id đứng đầu phải nhận ra được");
    assert_eq!(id, "3e9a7fd6-3050-4a54-ba52-0dfb24de033c");
    assert_eq!(rest, "Tóm tắt trong 1 câu?");

    // Phím cũng đi cùng id — `/key <id> down`.
    let (id, rest) = split_target("3e9a7fd6-3050-4a54-ba52-0dfb24de033c down").unwrap();
    assert_eq!(id, "3e9a7fd6-3050-4a54-ba52-0dfb24de033c");
    assert_eq!(rest, "down");

    // Không có id ⟹ None, để chỗ gọi rơi về con trỏ focus CÓ LOG. Quan trọng
    // hơn: một câu tiếng Việt không được nuốt mất chữ đầu.
    assert_eq!(split_target("Tóm tắt trong 1 câu: phiên này đang làm gì?"), None);
    assert_eq!(split_target("down"), None);
    assert_eq!(split_target(""), None);
    // Chuỗi dài mà không phải uuid cũng không được nhận nhầm.
    assert_eq!(split_target("khong-phai-uuid-nhung-rat-dai-va-co-gach-noi xin chao"), None);
}

/// `/accounts` — Hà 2026-08-12: *"chưa có lệnh xem danh sách acc"*.
///
/// Ba lối gõ vì đây là lệnh gõ trên điện thoại: tên đầy đủ, tên tắt, và lối
/// không dấu quen thuộc của phòng này.
#[test]
fn the_accounts_verb_answers_to_three_spellings() {
    for text in ["/accounts", "/acc", "/taikhoan"] {
        let (kind, id, arg) = parse_command(text, OWNER, &owners()).expect(text);
        assert_eq!(kind, hub::adapters::CommandKind::Accounts, "sai verb cho {text}");
        assert_eq!(id, 0, "{text} không nhận id");
        assert!(arg.is_empty(), "{text} không nhận tham số: {arg}");
    }
}

/// `/cmd <dòng lệnh>` — Hà 2026-08-12: *"thêm một cổng chạy lệnh nữa… chạy 1
/// command xong trả về kết quả rồi nó đóng luôn"*, gõ từ Telegram.
///
/// Phần sau động từ phải giữ NGUYÊN VĂN: một dòng shell có `|`, `&&`, dấu nháy.
#[test]
fn the_cmd_verb_keeps_the_whole_line_verbatim() {
    let (kind, id, arg) =
        parse_command("/cmd git -C ~/x status | head -3", OWNER, &owners()).expect("parse");
    assert_eq!(kind, hub::adapters::CommandKind::Cmd);
    assert_eq!(id, 0);
    assert_eq!(arg, "git -C ~/x status | head -3", "dòng lệnh bị cắt xén");
}

#[test]
fn the_cmd_verb_without_a_line_still_parses_so_the_reply_can_teach() {
    let (kind, _, arg) = parse_command("/cmd", OWNER, &owners()).expect("parse");
    assert_eq!(kind, hub::adapters::CommandKind::Cmd);
    assert!(arg.is_empty(), "không có lệnh thì arg phải rỗng: {arg}");
}

/// `/close` là một động từ RIÊNG, không phải `/stop` đội tên khác.
///
/// 🔴 Hà 2026-08-13: *"ah stop là dừng rồi vậy dùng close"* — `/stop` dừng một
/// phiên nền và giữ hội thoại, còn đóng hẳn (thoát CLI + đóng cửa sổ) là một
/// kết cục khác hẳn về mức mất mát. Một động từ hai kết cục thì người bấm không
/// biết mình sắp nhận cái nào.
#[test]
fn close_is_its_own_verb_and_takes_an_optional_id() {
    use hub::adapters::CommandKind;
    use hub::verbs::parse_command;

    let me = "u-owner";
    let trusted = vec![me.to_string()];

    // Trống = phiên đang theo (chỗ gọi tra con trỏ), nên arg rỗng là HỢP LỆ.
    assert_eq!(
        parse_command("/close", me, &trusted),
        Some((CommandKind::Close, 0, String::new()))
    );
    // Có id thì đóng đúng phiên ấy.
    assert_eq!(
        parse_command("/close 0a109818", me, &trusted),
        Some((CommandKind::Close, 0, "0a109818".to_string()))
    );
    // …và KHÔNG lẫn với /stop.
    assert_eq!(
        parse_command("/stop", me, &trusted),
        Some((CommandKind::Stop, 0, String::new()))
    );

    // `/win` cần một dòng lệnh — trống thì không phải lệnh, đừng mở cửa sổ rỗng.
    assert_eq!(
        parse_command("/win sudo -v", me, &trusted),
        Some((CommandKind::Win, 0, "sudo -v".to_string()))
    );
    assert_eq!(parse_command("/win", me, &trusted), None);

    // Người khác gõ thì vẫn chỉ là chữ — cổng người không đổi.
    assert_eq!(parse_command("/close", "u-nguoi-la", &trusted), None);
}

/// `/runin <id> <lệnh>` — máy chạy, phiên đọc.
///
/// 🔴 Hà 2026-08-13, sau khi biết dấu `!` chưa bao giờ bật chế độ bash: *"có lẽ
/// nên gọi lệnh ở command khác rồi lấy kết quả dán gửi lại vào phiên"* → *"nhưng
/// ngữ cảnh lại bị mất dấu"*.
#[test]
fn runin_needs_both_a_session_and_a_command() {
    use hub::adapters::CommandKind;
    use hub::verbs::parse_command;

    let me = "u-owner";
    let trusted = vec![me.to_string()];

    assert_eq!(
        parse_command("/runin 4963b95c cargo test --offline", me, &trusted),
        Some((CommandKind::RunIn, 0, "4963b95c cargo test --offline".to_string()))
    );
    // Thiếu lệnh ⟹ không nhận.
    assert_eq!(parse_command("/runin 4963b95c", me, &trusted), None);
    // Thiếu id ⟹ không nhận: một `/runin` không id sẽ rơi vào phiên đang theo,
    // đúng con đường đã gõ nhầm phiên tối 08-13.
    assert_eq!(parse_command("/runin", me, &trusted), None);
    // Người khác gõ thì vẫn chỉ là chữ.
    assert_eq!(parse_command("/runin 4963b95c ls", "u-la", &trusted), None);
}

/// 🔴 Hà 2026-08-14: *"thêm 1 cái icon để bấm chạy bên trong text chỗ cuối dòng
/// lệnh"* · *"chứ ko phải đi thay icon"*.
///
/// Một icon nằm GIỮA CHỮ không thể là nút — bàn phím Telegram luôn treo dưới
/// đáy tin. Thứ đặt được vào giữa chữ là một LIÊN KẾT, và liên kết chạy được
/// lệnh chỉ có một dạng: deep link về chính bot. Đó đúng là thứ Hà hỏi ngay từ
/// đầu (*"sao không dùng Deep Links"*) và tôi đã đi vòng mất mấy lượt.
#[test]
fn a_deep_link_payload_round_trips_back_into_the_same_command() {
    let owners = owners();
    // Bấm icon ⟹ Telegram gửi `/start <payload>` ⟹ phải ra ĐÚNG lệnh gõ tay.
    for (payload, typed) in [
        ("run_0", "/run_0"),
        ("pick_4963b95c_2_1", "/pick_4963b95c_2_1"),
        ("send_4963b95c", "/send_4963b95c"),
        ("upgrade", "/upgrade"),
    ] {
        assert_eq!(
            parse_command(&format!("/start {payload}"), OWNER, &owners),
            parse_command(typed, OWNER, &owners),
            "payload {payload} phải cởi ra đúng như gõ tay"
        );
    }
    // `/start` trống thì không phải lệnh gì cả — đừng đoán hộ.
    assert!(parse_command("/start", OWNER, &owners).is_none());
}
