//! Bộ phân tích MỆNH LỆNH — kiểm không cần máy chủ nào.
//!
//! 🔴 Tệp này từng tên là `tests/tfl5.rs` và kiểm cả một cái kênh: dựng URL,
//! thoát chuỗi truy vấn, ánh xạ lỗi bắt tay websocket, cửa sổ im lặng. Kênh ấy
//! gỡ ngày 2026-08-14 (Hà: *"tạm thời không dùng tfl5 để xem cứ xóa hết đi"*),
//! và những bài kiểm ấy đi theo nó.
//!
//! 🔴 Cùng ngày, **cổng người cũng rời khỏi đây** — và cần nói rõ vì đây từng là
//! chỗ ghi "ranh giới an toàn thật sự". `parse_command` nhận thêm
//! `(from_user_tid, owner_tids)` khi hub sống trong một PHÒNG CHAT: ai cũng vào
//! phòng được, nên phải hỏi thêm ai được ra lệnh. Telegram không có hình dạng
//! ấy — cổng của nó là `chat_id`, gác ở `telegram.rs` trước khi một chữ nào tới
//! được bộ phân tích. Giữ cả hai tầng thì tầng dưới không bao giờ từ chối được
//! (chỗ gọi phải tự bịa ra người gõ để đi qua chính nó), trừ đúng một trường
//! hợp: danh sách chủ RỖNG, và khi ấy nó nuốt sạch mọi mệnh lệnh trong im lặng.
//!
//! Luật không đổi, chỗ đứng thì đổi. Những bài kiểm "người lạ gõ vẫn chỉ là
//! chữ" ở tệp này nay nằm tại `tests/telegram.rs`
//! (`a_message_from_another_chat_is_not_an_order`) — kiểm đúng cái cổng thật.
//!
//! Thứ ở lại đây là phần đã luôn đúng ở mọi kênh: chữ người gõ → một route.

use hub::verbs::parse_command;

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
        let (kind, id, arg) = parse_command(line).expect("parsed");
        assert_eq!(kind, hub::adapters::CommandKind::Session, "{line}");
        assert_eq!(id, 0, "{line}");
        assert_eq!(arg, "", "{line} phải là danh sách, không mang id");
    }
    let (_, _, arg) = parse_command("/sessions 3e9a7fd6-3050").expect("parsed");
    assert_eq!(
        arg, "",
        "số nhiều mà nuốt id thì nó lặng lẽ đổi phiên đang theo"
    );

    // Số ít vẫn giữ nguyên hai nghĩa cũ.
    let (_, _, arg) = parse_command("/session abc-123").expect("parsed");
    assert_eq!(arg, "abc-123");
    let (_, _, arg) = parse_command("/session").expect("parsed");
    assert_eq!(arg, "");
}

#[test]
fn a_side_question_keeps_every_word_including_the_slashes() {
    // The whole remainder is the question, not just the first token — a
    // question is a sentence, and one that arrives truncated is worse than one
    // refused. It also must survive text that looks like more commands.
    let (kind, id, arg) =
        parse_command("/ask lệnh /run vừa rồi đã chạy xong chưa, còn kẹt ở đâu?").expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Ask);
    assert_eq!(id, 0, "the target is the focused session, never an id");
    assert_eq!(arg, "lệnh /run vừa rồi đã chạy xong chưa, còn kẹt ở đâu?");

    let (kind, _, arg) = parse_command("/hoi đang làm gì đấy").unwrap();
    assert_eq!(kind, hub::adapters::CommandKind::Ask);
    assert_eq!(arg, "đang làm gì đấy");
}

#[test]
fn an_empty_side_question_never_reaches_the_wallet() {
    // `/ask` alone would otherwise pay for a `claude` call that answers
    // nothing. Returning None keeps it an ordinary message, so the person sees
    // it was not understood instead of being billed for silence.
    for t in ["/ask", "/ask   ", "/hoi"] {
        assert!(parse_command(t).is_none(), "sai với: {t}");
    }
}

/// 🔴 REGRESSION — `/new` gõ trơn phải MỞ ĐƯỢC CỬA SỔ (Hà 2026-08-14: *"Lệnh
/// new nữa chưa chạy đc"*).
///
/// Bài kiểm ở đây trước ngày ấy tên là `starting_a_session_needs_both_a_project_
/// and_a_task`, và nó khoá đúng con lỗi lại: bắt `/new`, `/new dwork`, `/new   `
/// đều phải là `None`. Luật ấy đúng khi từ đầu tiên còn là TÊN DỰ ÁN — nhưng
/// luật kia đã bị bỏ ngày 2026-08-13, còn bài kiểm thì ở lại, và từ đó nó không
/// canh một hành vi nữa mà **bảo vệ một hành vi hỏng**.
///
/// Cái giá đo được trong nhật ký: `/new` → `telegram_not_a_command` ba lần
/// (13-08 13:27 · 14-08 08:13 · 14-08 22:27), mỗi lần hub đáp *"Chưa hiểu lệnh
/// này"* về một động từ chính nó vừa khai với Telegram và đang hiện trong menu.
/// Chạm vào một dòng trong menu chỉ gửi đúng token lệnh, nên với lệnh
/// `listed: true` thì "gõ trơn" là cách dùng MẶC ĐỊNH, không phải cách dùng sai.
///
/// Bài học chung, đáng giữ hơn con lỗi: **một bài kiểm sống lâu hơn lý do của
/// nó thì nó đổi phe.**
#[test]
fn a_bare_new_opens_a_window_because_that_is_what_tapping_the_menu_sends() {
    use hub::adapters::CommandKind;

    // Gõ trơn — đúng thứ Telegram gửi khi chạm vào `/new` trong menu.
    assert_eq!(
        parse_command("/new"),
        Some((CommandKind::New, 0, String::new())),
        "chạm vào /new trong menu mà bị từ chối thì lệnh ấy không bấm được"
    );
    assert_eq!(
        parse_command("/moi"),
        Some((CommandKind::New, 0, String::new()))
    );
    // Khoảng trắng thừa cũng vậy — không có gì để phân biệt với gõ trơn.
    assert_eq!(
        parse_command("/new   "),
        Some((CommandKind::New, 0, String::new()))
    );

    // MỘT chữ cũng là một đề bài. Cổng cũ đòi có dấu cách, nên `/new sua` —
    // một câu ngắn hoàn toàn hợp lệ — cũng rơi mất cùng một chỗ.
    let (kind, _, arg) = parse_command("/new sua").expect("một chữ vẫn là đề bài");
    assert_eq!(kind, CommandKind::New);
    assert_eq!(arg, "sua");

    // Cả câu là ĐỀ BÀI, không phải "<dự án> <việc>" — chỗ chỉ thư mục là `-s`.
    // Đây chính là luật đã đổi 2026-08-13, sau khi `/new Tại sao lại có phiên
    // này…` bị đọc thành tên thư mục và trả về `⚠ không biết dự án 'Tại'`.
    let (_, _, arg) = parse_command("/new Tại sao lại có phiên này chạy?").expect("parsed");
    assert_eq!(arg, "Tại sao lại có phiên này chạy?");
    let (_, _, arg) = parse_command("/new -s dwork sửa lịch làm việc").expect("parsed");
    assert_eq!(arg, "-s dwork sửa lịch làm việc", "cờ phải tới nguyên vẹn");
}

#[test]
fn stop_defaults_to_the_session_being_read() {
    let (kind, _, arg) = parse_command("/stop").expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Stop);
    assert_eq!(arg, "", "empty means: whatever /session is following");

    let (_, _, arg) = parse_command("/stop a3a24ccd-6ad8").unwrap();
    assert_eq!(arg, "a3a24ccd-6ad8");
}

/// 🔴 `/tell` GỠ 2026-08-15 — và test cũ đổi vai, không xoá lặng.
///
/// Hà: *"lệnh tell là không cần thiết?"* · *"vì trên tele tôi chỉ gõ text bình
/// thường thôi"*. Đo cả cuốn log: **0 lượt dùng** — nhưng con số ấy một mình đã
/// lừa một lần rồi (`/win`, `listed:false`, đo SỰ VÔ HÌNH), nên bằng chứng thật
/// nằm trong mã: `sessions::tell` mở đầu bằng
/// `if session.kind != "background" { bail!(…) }`, mà hạng phiên nền nay chỉ
/// còn sinh ra khi MỞ CỬA SỔ THẤT BẠI.
///
/// Nay `/tell` phải đọc thành CHỮ, y như mọi câu khác — vì đó chính là cách chủ
/// máy nói với phiên: gõ thẳng. Một động từ đã gỡ mà vẫn còn phân tích được là
/// đúng con bug `CLAUDE.md` gọi tên (*"a verb whose handler has nothing left to
/// DO is the same bug wearing a uniform"*), chỉ khác chiều.
#[test]
fn tell_is_gone_and_reads_as_ordinary_text() {
    assert!(parse_command("/tell chạy lại test rồi báo kết quả").is_none());
    assert!(parse_command("/tell").is_none());
    assert!(parse_command("/noi gì đó").is_none());
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
        assert!(parse_command(t).is_none(), "sai với: {t}");
    }
}

#[test]
fn help_needs_no_decision_id() {
    let (kind, id, _) = parse_command("/help").expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::Help);
    assert_eq!(id, 0);
}

/// REGRESSION (2026-08-07, cost real money): the live socket ingested every
/// frame as a message without ever asking `parse_command`, so an owner's
/// `/close 155` was executed by the poller AND stored AND sent to the model —
/// $0.18 to classify the word "close". The two paths must agree on what a
/// command is; this pins the shared predicate they now both use.
#[test]
fn the_live_path_and_the_poller_agree_on_what_counts_as_a_command() {
    for text in ["/session 1a2b3c4d", "/stop", "/handover"] {
        assert!(
            parse_command(text).is_some(),
            "cả hai đường phải coi đây là LỆNH, không phải tin nhắn: {text}"
        );
    }
    // ...and ordinary chat must still be a message on both paths, or the live
    // socket would start silently swallowing questions.
    for text in ["hôm nay CI sao rồi?", "/approve 12", "closing the issue"] {
        assert!(
            parse_command(text).is_none(),
            "đây là tin nhắn thường, không được nuốt: {text}"
        );
    }
}

#[test]
fn set_needs_both_a_key_and_a_value() {
    let (kind, _, arg) = parse_command("/set auto_handover.at_percent 70").expect("parsed");
    assert_eq!(kind, hub::adapters::CommandKind::SetConfig);
    assert_eq!(arg, "auto_handover.at_percent 70");
    // A key with no value would blank the field — refuse it here.
    assert!(parse_command("/set auto_handover.at_percent").is_none());
    assert!(parse_command("/set").is_none());
}

/// 🔴 `/ingest` (`/poll`) đã CHẾT ngày 2026-08-14, và chỗ này canh cái xác.
///
/// Động từ ấy đọc phòng chat tfl5. Sau khi phòng đóng, nó chỉ còn một câu trả
/// lời khả dĩ — *"disabled in config"* — tức đúng thứ luật riêng của dự án gọi
/// là tệ hơn một động từ không tồn tại: kênh nhận nó, không có gì xảy ra, và
/// không có gì nói ra điều đó. Nay nó phải đọc như CHỮ THƯỜNG.
#[test]
fn the_cycle_verbs_take_no_id_and_ingest_is_no_longer_one_of_them() {
    for (text, want) in [
        ("/run", hub::adapters::CommandKind::Run),
        ("/cycle", hub::adapters::CommandKind::Run),
        ("/doctor", hub::adapters::CommandKind::Doctor),
        ("/health", hub::adapters::CommandKind::Doctor),
    ] {
        let (kind, id, _) = parse_command(text).expect(text);
        assert_eq!(kind, want, "sai verb cho {text}");
        assert_eq!(id, 0, "{text} không nhận id");
    }
    for dead in ["/ingest", "/poll"] {
        assert!(
            parse_command(dead).is_none(),
            "{dead} đọc một cái phòng không còn tồn tại — phải là chữ thường"
        );
    }
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
    assert_eq!(
        split_target("Tóm tắt trong 1 câu: phiên này đang làm gì?"),
        None
    );
    assert_eq!(split_target("down"), None);
    assert_eq!(split_target(""), None);
    // Chuỗi dài mà không phải uuid cũng không được nhận nhầm.
    assert_eq!(
        split_target("khong-phai-uuid-nhung-rat-dai-va-co-gach-noi xin chao"),
        None
    );
}

/// `/accounts` — Hà 2026-08-12: *"chưa có lệnh xem danh sách acc"*.
///
/// Ba lối gõ vì đây là lệnh gõ trên điện thoại: tên đầy đủ, tên tắt, và lối
/// không dấu quen thuộc của phòng này.
#[test]
fn the_accounts_verb_answers_to_three_spellings() {
    for text in ["/accounts", "/acc", "/taikhoan"] {
        let (kind, id, arg) = parse_command(text).expect(text);
        assert_eq!(
            kind,
            hub::adapters::CommandKind::Accounts,
            "sai verb cho {text}"
        );
        assert_eq!(id, 0, "{text} không nhận id");
        assert!(arg.is_empty(), "{text} không nhận tham số: {arg}");
    }
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

    // Trống = phiên đang theo (chỗ gọi tra con trỏ), nên arg rỗng là HỢP LỆ.
    assert_eq!(
        parse_command("/close"),
        Some((CommandKind::Close, 0, String::new()))
    );
    // Có id thì đóng đúng phiên ấy.
    assert_eq!(
        parse_command("/close 0a109818"),
        Some((CommandKind::Close, 0, "0a109818".to_string()))
    );
    // …và KHÔNG lẫn với /stop.
    assert_eq!(
        parse_command("/stop"),
        Some((CommandKind::Stop, 0, String::new()))
    );
}

/// `/runin <id> <lệnh>` — máy chạy, phiên đọc.
///
/// 🔴 Hà 2026-08-13, sau khi biết dấu `!` chưa bao giờ bật chế độ bash: *"có lẽ
/// nên gọi lệnh ở command khác rồi lấy kết quả dán gửi lại vào phiên"* → *"nhưng
/// ngữ cảnh lại bị mất dấu"*.
#[test]
fn runin_needs_both_a_session_and_a_command() {
    use hub::adapters::CommandKind;

    assert_eq!(
        parse_command("/runin 4963b95c cargo test --offline"),
        Some((
            CommandKind::RunIn,
            0,
            "4963b95c cargo test --offline".to_string()
        ))
    );
    // Thiếu lệnh ⟹ không nhận.
    assert_eq!(parse_command("/runin 4963b95c"), None);
    // Thiếu id ⟹ không nhận: một `/runin` không id sẽ rơi vào phiên đang theo,
    // đúng con đường đã gõ nhầm phiên tối 08-13.
    assert_eq!(parse_command("/runin"), None);
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
    // 🔴 MÃ LẤY TỪ CHÍNH BÊN SINH RA NÓ, không gõ tay một hình dạng cho dễ.
    //
    // Bản trước bài kiểm này tự chọn `run_0`, và nó xanh suốt trong khi đường
    // thật đã gãy: `quick_token` sinh **8 ký tự hex** (`d1704560`) còn bên đọc
    // vẫn đòi chữ số. Hà bấm icon, hub đáp *"Chưa hiểu lệnh này"* — và không
    // bài kiểm nào đỏ, vì cả hai đầu đều đúng với hình dạng **tôi tưởng tượng**.
    // Gọi thẳng `quick_token` thì hình dạng ấy không còn là chuyện tưởng tượng.
    let token = hub::pipeline::quick_token("4963b95c-1111-2222-3333-444455556666", "cargo test");
    assert_eq!(token.len(), 8, "mã nút là 8 ký tự hex: {token}");
    // …và bài kiểm phải CÓ RĂNG: mã toàn chữ số thì nó xanh cả với bản cũ
    // (`is_ascii_digit`), tức lại là một phép đo không bao giờ đỏ. Chốt luôn ở
    // đây thay vì tin vào may rủi của một hàm băm.
    assert!(
        token.chars().any(|c| c.is_ascii_alphabetic()),
        "mã {token} toàn chữ số — đổi đầu vào, không thì bài kiểm này không bắt được lỗi nó sinh ra để bắt"
    );
    for (payload, typed) in [
        (format!("run_{token}"), format!("/run_{token}")),
        // …và mã toàn chữ số vẫn phải chạy: nó là một mã hex hợp lệ, không phải
        // một hình dạng khác.
        ("run_12345678".to_string(), "/run_12345678".to_string()),
        (
            "pick_4963b95c_2_1".to_string(),
            "/pick_4963b95c_2_1".to_string(),
        ),
        ("send_4963b95c".to_string(), "/send_4963b95c".to_string()),
        ("upgrade".to_string(), "/upgrade".to_string()),
    ] {
        assert_eq!(
            parse_command(&format!("/start {payload}")),
            parse_command(&typed),
            "payload {payload} phải cởi ra đúng như gõ tay"
        );
        assert!(
            parse_command(&format!("/start {payload}")).is_some(),
            "payload {payload} rơi vào nhánh 'không phải lệnh' ⟹ hub sẽ đáp 'Chưa hiểu lệnh này'"
        );
    }
    // `/start` trống thì không phải lệnh gì cả — đừng đoán hộ.
    assert!(parse_command("/start").is_none());
}

/// `/type <nút> [id phiên]` — nút trước, id sau và tuỳ chọn.
///
/// 🔴 Hà 2026-08-16: *"cấu trúc lại lệnh type thành `/type <nút> [id phiên]`, ko
/// có id phiên thì vào phiên đang trỏ tới"* — sau khi gõ `/type esc` và nhận về
/// một dòng chữ "esc" gõ thẳng vào phiên.
#[test]
fn a_button_typed_as_type_goes_to_the_focused_session_by_default() {
    use hub::adapters::CommandKind;
    // Không id ⟹ phiên đang trỏ tới (arg chỉ mang tên phím).
    for k in ["enter", "esc", "up", "down", "tab", "2", "clear"] {
        assert_eq!(
            parse_command(&format!("/type {k}")),
            Some((CommandKind::Key, 0, k.to_string())),
            "/type {k} phải là một NÚT, không phải chữ gõ vào phiên"
        );
    }
    // Có id ⟹ đúng phiên ấy, và id đứng SAU.
    assert_eq!(
        parse_command("/type enter bab47095"),
        Some((CommandKind::Key, 0, "bab47095 enter".to_string()))
    );
    // …còn một CÂU thì vẫn là chữ, không được nuốt thành nút.
    assert!(matches!(
        parse_command("/type enter vào phiên đi"),
        Some((CommandKind::Type, _, _))
    ));
    assert!(matches!(
        parse_command("/type chạy test đi"),
        Some((CommandKind::Type, _, _))
    ));
}

/// Nút ⏎ và ⌫ trong tin mang MÃ PHIÊN trong chính liên kết.
///
/// 🔴 Hà 2026-08-16: *"khi chèn 1 nút hay 1 link đã phải có đủ mã phiên và nội
/// dung gửi đi chứ"*. Bấm lại một tin cũ phải chạm đúng phiên của tin ấy, không
/// phải phiên con trỏ đang trỏ tới lúc bấm.
#[test]
fn the_enter_and_clear_links_carry_their_own_session() {
    use hub::adapters::CommandKind;
    assert_eq!(
        parse_command("/start send_bab47095"),
        Some((CommandKind::Key, 0, "bab47095 enter".to_string()))
    );
    assert_eq!(
        parse_command("/start clr_bab47095"),
        Some((CommandKind::Key, 0, "bab47095 clear".to_string()))
    );
}

/// `/terminal <lệnh>` — cửa sổ Terminal THẬT, và cái tên phải tự nói ra điều đó.
///
/// 🔴 Hà 2026-08-15, sau khi route này bị gỡ vì "0 lượt dùng": *"cái tên win hơi
/// mơ hồ mà bạn cũng không đưa vào help nên tôi ko hề biết"*. Con số 0 ấy đo
/// **sự vô hình** — route để `listed: false` nên không vào menu ☰ và không hiện
/// khi gõ `/` — chứ không đo sự vô dụng. Nay tên là `/terminal`, nằm trong menu,
/// và hai tên cũ vẫn nhận để nút hay thói quen cũ không gãy.
#[test]
fn the_terminal_verb_says_what_it_opens() {
    use hub::adapters::CommandKind;
    for name in ["/terminal", "/win", "/cuaso", "/tty"] {
        assert_eq!(
            parse_command(&format!("{name} sudo -v")),
            Some((CommandKind::Win, 0, "sudo -v".to_string())),
            "{name} phải mở được cửa sổ"
        );
        // 🔴 Trơn = XEM DANH SÁCH cửa sổ (Hà 2026-08-15), không phải None.
        // Bản trước trả `None`: gõ đúng tên một route rồi nhận lại sự im lặng —
        // đúng thứ làm người ta tưởng nó hỏng.
        assert_eq!(
            parse_command(name),
            Some((CommandKind::Win, 0, String::new())),
            "{name} trơn phải là 'liệt kê', không phải im lặng"
        );
    }
    // Và nó phải NHÌN THẤY ĐƯỢC: có trong danh sách gửi lên menu Telegram.
    assert!(
        hub::commands::for_telegram()
            .iter()
            .any(|(c, _)| *c == "terminal"),
        "route vô hình là route không ai gọi — đó là cả bài học của lần gỡ nhầm"
    );
}

/// Hai đích chạm của MỘT hàng `/terminal`, dạng liên kết chèn giữa chữ.
///
/// 🔴 Hà 2026-08-17, ảnh 8 cửa sổ đẻ ra 16 cái nút xếp dọc, hai cái một cặp
/// giống hệt nhau: *"danh sách đó mỗi cái và nút nằm trên 1 dòng"*. Nút chỉ nằm
/// được dưới đáy tin; thứ đặt được ngay trên dòng của cửa sổ là một liên kết —
/// và payload deep link chỉ nhận `[A-Za-z0-9_-]`, nên `sess:`/`close:` (có dấu
/// hai chấm) không đi đường ấy được.
///
/// Vẫn về đúng route cũ: thêm một chỗ BẤM, không thêm một đường ĐI.
#[test]
fn a_terminal_row_carries_its_own_two_taps() {
    use hub::adapters::CommandKind;
    assert_eq!(
        parse_command("/start w_ttys014"),
        Some((CommandKind::Session, 0, "win-ttys014".to_string())),
        "🖥 vào = /session win-<tty>"
    );
    assert_eq!(
        parse_command("/start wx_ttys014"),
        Some((CommandKind::Close, 0, "win-ttys014".to_string())),
        "⏹ đóng = /close win-<tty>"
    );
    // Hẹp có chủ ý: payload lạ thì KHÔNG được hoá thành một id phiên.
    for bad in [
        "/start w_",
        "/start w_abc",
        "/start wx_../etc",
        "/start w_ttys",
    ] {
        assert!(
            !matches!(
                parse_command(bad),
                Some((CommandKind::Session, _, _)) | Some((CommandKind::Close, _, _))
            ),
            "{bad} không được đi tới cửa sổ nào"
        );
    }
}
