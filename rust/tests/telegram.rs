//! Telegram làm KÊNH RA LỆNH: cái nút và cái danh sách.
//!
//! Hai thứ ở đây đều thuần (không cần mạng, không cần Telegram) vì đó chính là
//! chỗ dễ sai mà lại không ai nhìn thấy: một `callback_data` sai hình dạng thì
//! cú bấm rơi vào hư không, còn một dòng danh sách thiếu id thì người đọc không
//! gõ tiếp được lệnh nào.

use hub::pipeline::{
    session_button_label, session_list_text, text_for_session, MAX_SESSION_BUTTONS,
};
use hub::sessions::LiveSession;
use hub::telegram::callback_to_command;

/// Đồng hồ giả — 2026-08-11T15:30:00Z. Danh sách nói "im bao lâu rồi", nên nó
/// phải đọc giờ từ tham số chứ không từ đồng hồ máy, không thì test hôm nay
/// xanh và test ngày mai đỏ.
const NOW: i64 = 1_786_462_200_000;

fn sess(id: &str, name: &str, account: &str, working: bool) -> LiveSession {
    LiveSession {
        session_id: id.to_string(),
        name: name.to_string(),
        account: account.to_string(),
        working,
        ..Default::default()
    }
}

/// Nút = phím tắt của một ROUTE đã có, không phải một nhánh xử lý riêng.
#[test]
fn a_button_press_becomes_the_very_line_a_finger_would_type() {
    assert_eq!(
        callback_to_command("key:3e9a7fd6-3050-4a54-ba52-0dfb24de033c:2").as_deref(),
        Some("/key 3e9a7fd6-3050-4a54-ba52-0dfb24de033c 2")
    );
    assert_eq!(
        callback_to_command("sess:3e9a7fd6-3050-4a54-ba52-0dfb24de033c").as_deref(),
        Some("/session 3e9a7fd6-3050-4a54-ba52-0dfb24de033c")
    );
}

/// Nút xác nhận KHÔNG phải mệnh lệnh: nó chỉ có nghĩa trong lúc `confirm::ask`
/// đang chờ. Nhận nhầm nó thành lệnh là biến một cú bấm ✅ đã hết hạn thành một
/// hành động chạy thật.
#[test]
fn confirm_buttons_are_not_commands() {
    assert_eq!(callback_to_command("ok:a1b2"), None);
    assert_eq!(callback_to_command("no:a1b2"), None);
    assert_eq!(callback_to_command("gì đó lạ"), None);
}

/// Dữ liệu nút hỏng thì trả `None` — đừng đẻ ra một dòng lệnh cụt.
///
/// `/key <id>` thiếu số sẽ được `parse_command` hiểu thành một phím rỗng, và
/// một phím rỗng gõ vào phiên đang mở hộp chọn là thứ không lùi lại được.
#[test]
fn a_malformed_button_never_becomes_a_half_command() {
    assert_eq!(callback_to_command("key:chỉ-có-id"), None);
    assert_eq!(callback_to_command("key::2"), None);
    assert_eq!(callback_to_command("key:id:"), None);
    assert_eq!(callback_to_command("sess:"), None);
}

/// Telegram từ chối `callback_data` quá 64 byte — mà id phiên là uuid 36 ký tự.
#[test]
fn callback_data_fits_telegrams_64_byte_ceiling() {
    let id = "3e9a7fd6-3050-4a54-ba52-0dfb24de033c";
    assert!(format!("sess:{id}").len() <= 64);
    assert!(format!("key:{id}:9").len() <= 64);
}

/// Mỗi dòng phải trả lời ba câu: phiên nào · đang chạy hay không · id để gõ tiếp.
#[test]
fn every_row_carries_the_id_the_next_command_needs() {
    let live = vec![
        sess("3e9a7fd6-3050-4a54-ba52-0dfb24de033c", "hub", "acc3", true),
        sess(
            "7c2ae1a7-1111-2222-3333-444455556666",
            "dwork",
            "acc1",
            false,
        ),
    ];
    let text = session_list_text(&live, "", NOW);
    assert!(text.contains("hub"), "{text}");
    assert!(text.contains("acc3"), "{text}");
    assert!(text.contains("3e9a7fd6"), "thiếu id ngắn: {text}");
    assert!(text.contains("7c2ae1a7"), "thiếu id ngắn: {text}");
    assert!(text.contains("🟢"), "không nói phiên nào đang chạy: {text}");
    assert!(text.contains("🟡"), "không nói phiên nào đứng chờ: {text}");
    assert!(
        text.contains("Chưa theo phiên nào"),
        "chưa theo phiên nào thì phải nói ra: {text}"
    );
    // Id ĐẦY ĐỦ không lên màn: nó dài gấp bốn lần chỗ nó chiếm mà không thêm
    // một thông tin nào — `claude stop` và `/session` đều nhận id ngắn.
    assert!(
        !text.contains("-3050-"),
        "id đầy đủ chiếm chỗ vô ích: {text}"
    );
}

/// BA tình trạng, không phải hai — và dự án phải đọc được.
///
/// Hà 2026-08-12: *"phải thêm tình trạng đang xử lý, đã dừng"* + *"phải thêm
/// thông tin để biết phiên đang làm dự án hay thư mục nào"*.
#[test]
fn the_list_tells_running_waiting_and_stopped_apart() {
    let mut a = sess(
        "aaaaaaaa-0000-0000-0000-000000000000",
        "projects-ff",
        "acc3",
        true,
    );
    a.folder = "AI/hub".into();
    let mut b = sess(
        "bbbbbbbb-0000-0000-0000-000000000000",
        "projects-11",
        "acc1",
        false,
    );
    b.folder = "dwork".into();
    let mut c = sess(
        "cccccccc-0000-0000-0000-000000000000",
        "Tự chạy lại",
        "acc3",
        false,
    );
    c.host = "dead".into();
    let text = session_list_text(&[a, b, c], "", NOW);
    // Nhãn là ĐƯỜNG DỰ ÁN, không kèm tên tự sinh (Hà 2026-08-13: *"cần gì đoạn
    // text project-xx làm gì"*).
    assert!(text.contains("[AI/hub]"), "{text}");
    assert!(text.contains("[dwork]"), "{text}");
    assert!(
        !text.contains("projects-ff"),
        "tên tự sinh vẫn còn chiếm chỗ: {text}"
    );
    // Chấm TRẠNG THÁI, không phải ký hiệu điều khiển của máy phát nhạc — Hà
    // 2026-08-13: *"icon biểu diễn chạy và dừng bị ngược"*. `▶` nay chỉ còn
    // nghĩa "bấm để chạy lệnh này" (`remember_quick`), một ký hiệu một nghĩa.
    assert!(text.contains("🟢 đang chạy"), "{text}");
    assert!(text.contains("🟡 đứng chờ"), "{text}");
    assert!(
        text.contains("⚫ đã tắt"),
        "phiên đã tắt bị gộp vào 'đứng chờ': {text}"
    );
    assert!(
        !text.contains("▶"),
        "`▶` phải để dành cho NÚT chạy lệnh: {text}"
    );
}

/// Phiên đang HỎI phải đọc ra được cả câu hỏi lẫn từng lựa chọn.
///
/// Hà 2026-08-12: *"có 1 phiên đang đưa lựa chọn nhưng không nhận được trên
/// tele"*. Trên danh sách, trạng thái này nhìn y hệt "đứng chờ" nếu không nói ra
/// — mà nó là trạng thái DUY NHẤT cần người đọc làm gì đó thì việc mới đi tiếp.
#[test]
fn a_session_waiting_for_an_answer_shows_the_question_and_the_options() {
    let mut s = sess(
        "aaaaaaaa-0000-0000-0000-000000000000",
        "projects-11",
        "acc1",
        false,
    );
    s.folder = "dwork".into();
    s.last_text = Some("[dùng AskUserQuestion]".into());
    s.asking = Some(hub::sessions::Asking {
        header: "Nửa ngày".into(),
        question: "Đơn vắng có khai được NỬA NGÀY không?".into(),
        options: vec!["Thêm ô nửa ngày".into(), "Luôn trọn ngày".into()],
        multi: false,
        rest: Vec::new(),
    });
    let text = session_list_text(&[s], "", NOW);
    assert!(text.contains("⚠ dừng lại HỎI"), "tình trạng: {text}");
    assert!(text.contains("Nửa ngày"), "nhãn câu hỏi: {text}");
    assert!(
        text.contains("NỬA NGÀY không?"),
        "nguyên văn câu hỏi: {text}"
    );
    assert!(
        text.contains("1. Thêm ô nửa ngày"),
        "lựa chọn có SỐ để bấm: {text}"
    );
    assert!(text.contains("2. Luôn trọn ngày"), "{text}");
    // Câu cuối ("[dùng AskUserQuestion]") không nói thêm được gì khi đã có câu
    // hỏi thật — nó chỉ đẩy phiên sau ra khỏi màn.
    assert!(
        !text.contains("💬"),
        "câu cuối chen vào chỗ của câu hỏi: {text}"
    );
}

/// Phiên đang theo phải nhận ra được: mọi lệnh KHÔNG mang id sẽ rơi vào nó.
#[test]
fn the_followed_session_is_marked() {
    let live = vec![
        sess("aaaaaaaa-0000-0000-0000-000000000000", "hub", "acc3", true),
        sess(
            "bbbbbbbb-0000-0000-0000-000000000000",
            "dwork",
            "acc1",
            false,
        ),
    ];
    let text = session_list_text(&live, "bbbbbbbb-0000-0000-0000-000000000000", NOW);
    let followed = text
        .lines()
        .find(|l| l.contains("dwork"))
        .unwrap_or_default();
    assert!(
        followed.starts_with("👁"),
        "dòng đang theo không có dấu: {text}"
    );
    let other = text.lines().find(|l| l.contains("hub")).unwrap_or_default();
    assert!(!other.starts_with("👁"), "đánh dấu nhầm phiên: {text}");
}

/// Con trỏ trỏ vào một phiên đã chết vẫn phải nói ra.
///
/// Im ở đây là kiểu im nguy hiểm: `/tell`, `/ask`, `/type` không mang id vẫn
/// đang nhắm vào cái phiên không còn nữa, và người đọc tưởng mình chưa chọn gì.
#[test]
fn a_focus_that_no_longer_exists_is_said_out_loud() {
    let live = vec![sess(
        "aaaaaaaa-0000-0000-0000-000000000000",
        "hub",
        "acc3",
        true,
    )];
    let text = session_list_text(&live, "deadbeef-0000-0000-0000-000000000000", NOW);
    assert!(text.contains("deadbeef"), "{text}");
    assert!(text.contains("không còn sống"), "{text}");
}

/// Cắt bớt thì phải NÓI, không thì danh sách nói dối về số phiên đang chạy.
#[test]
fn a_truncated_list_says_how_many_it_hid() {
    let live: Vec<LiveSession> = (0..MAX_SESSION_BUTTONS + 3)
        .map(|i| {
            sess(
                &format!("{i:08}-0000-0000-0000-000000000000"),
                &format!("p{i}"),
                "acc1",
                false,
            )
        })
        .collect();
    let text = session_list_text(&live, "", NOW);
    assert!(
        text.contains(&format!("{} phiên đang sống", MAX_SESSION_BUTTONS + 3)),
        "tổng số phải đúng: {text}"
    );
    assert!(text.contains("còn 3 phiên nữa"), "cắt mà im: {text}");
    assert!(
        text.contains("/session <id>"),
        "cắt rồi phải chỉ đường khác: {text}"
    );
}

#[test]
fn no_sessions_is_a_sentence_not_an_empty_message() {
    assert_eq!(
        session_list_text(&[], "", NOW),
        "Không có phiên nào đang sống."
    );
}

/// Hàng phụ nói **tình trạng**, và nó phải khớp với thẻ trên trang điện thoại.
///
/// Hà 2026-08-11: *"cần thêm thông tin cuối mỗi phiên"* — danh sách chỉ có tên
/// và id thì nói phiên nào TỒN TẠI, không nói phiên nào ĐÁNG mở ra.
#[test]
fn each_row_carries_the_same_facts_as_the_card_on_the_page() {
    let mut s = sess("aaaaaaaa-0000-0000-0000-000000000000", "hub", "acc3", false);
    s.permission_mode = Some("auto".into());
    s.context_tokens = 460_000;
    s.pending_subagents = 2;
    // 12 phút trước NOW.
    s.last_activity = Some("2026-08-11T15:18:00+00:00".into());
    s.last_text = Some("Đã sửa xong\nphần redaction, chờ xác nhận".into());
    let text = session_list_text(&[s], "", NOW);
    assert!(text.contains("ngữ cảnh 46%"), "{text}");
    assert!(text.contains("tự duyệt"), "chế độ quyền: {text}");
    assert!(text.contains("2 subagent"), "{text}");
    assert!(text.contains("im 12 phút"), "{text}");
    assert!(
        text.contains("💬 Đã sửa xong phần redaction"),
        "câu cuối: {text}"
    );
    // Xuống dòng trong câu cuối phải bị dẹp: một dòng 💬 vỡ làm ba dòng thì
    // danh sách năm phiên trông như mười lăm.
    assert!(!text.contains("Đã sửa xong\n"), "còn xuống dòng: {text}");
}

/// Phiên ĐANG CHẠY thì không nói "im N phút".
///
/// Nhật ký của một phiên đang chạy đứng yên suốt cả một lượt `cargo test` hai
/// phút — "im 2 phút" ở đó là một câu SAI, không phải một câu muộn. Cùng lý do
/// `IDLE_AFTER_SEC` là 180 giây chứ không phải 60.
#[test]
fn a_running_session_is_never_described_as_quiet() {
    let mut s = sess("aaaaaaaa-0000-0000-0000-000000000000", "hub", "acc3", true);
    s.last_activity = Some("2026-08-11T15:18:00+00:00".into());
    s.activity = Some("Brewing… 2m 14s".into());
    let text = session_list_text(&[s], "", NOW);
    assert!(text.contains("Brewing…"), "đang làm gì: {text}");
    assert!(
        !text.contains("im 12 phút"),
        "phiên đang chạy mà bảo im: {text}"
    );
}

/// Chọn phiên xong thì CHỮ THƯỜNG là chữ gõ vào phiên ấy.
///
/// Hà 2026-08-11: *"bấm vào mỗi phiên focus vào phiên đó luôn"* — chọn xong coi
/// như đang ngồi trong phiên, không phải nhớ thêm một động từ trước mỗi câu.
#[test]
fn plain_text_is_something_to_type_into_the_session() {
    assert_eq!(text_for_session("chạy test đi"), Some("chạy test đi"));
    assert_eq!(
        text_for_session("  có lỗi gì không?  "),
        Some("có lỗi gì không?")
    );
    assert_eq!(
        text_for_session("2"),
        Some("2"),
        "một con số cũng là câu trả lời"
    );
}

/// Một LỆNH gõ nhầm KHÔNG được biến thành lượt gõ thật.
///
/// Đây là vế nguy hiểm của cùng một ranh giới: `/sesion` mà bị bơm vào cửa sổ
/// đang chạy thì kèm luôn Enter, tức hub tự tay biến lỗi chính tả của chủ máy
/// thành một hành động. Dòng rỗng cũng vậy — Enter trần vào một hộp chọn là một
/// lựa chọn.
#[test]
fn a_mistyped_command_is_never_typed_into_a_live_window() {
    assert_eq!(text_for_session("/sesion"), None);
    assert_eq!(text_for_session("/help"), None);
    assert_eq!(text_for_session("  /stop  "), None);
    assert_eq!(text_for_session(""), None);
    assert_eq!(text_for_session("   "), None);
}

/// Chữ còn nằm trong ô nhập thì phải nhận ra được — kể cả khi ô ngắt dòng.
///
/// 🔴 Hà đo 2026-08-12: *"nhận được text nhưng không tự gửi"*. `do script` đẩy
/// chữ và dấu xuống dòng trong CÙNG một lượt ghi, và ô nhập của `claude` đọc
/// lượt ấy như một cú DÁN — dấu xuống dòng bị nuốt vào nội dung. Muốn gửi Enter
/// rời cho đúng lúc thì phải NHÌN thấy chữ còn nằm đó, chứ không đoán.
#[test]
fn text_still_sitting_in_the_input_box_is_recognised_even_when_wrapped() {
    let typed = "Phải thêm thông tin để biết phiên đang làm dự án nào";
    // Ô nhập vẽ khung và ngắt dòng theo bề ngang cửa sổ.
    let screen = "╭──────────────╮\n\
                  │ > Phải thêm thông tin để biết phiên │\n\
                  │   đang làm dự án nào                │\n\
                  ╰──────────────╯";
    assert!(
        hub::keys::still_in_box(screen, typed),
        "so nguyên văn thì trượt"
    );
    // Đã gửi đi rồi thì ô trống — không được nhận nhầm là còn.
    let after = "╭──────────────╮\n│ >                    │\n╰──────────────╯";
    assert!(!hub::keys::still_in_box(after, typed));
}

/// ⚠ Gửi ĐI RỒI thì câu ấy vẫn còn trên màn — ở phần hội thoại, không phải ô nhập.
///
/// Đây là chỗ phép đo suýt trỏ sai chỗ: soi cả màn thì hub đọc "đã gửi" thành
/// "còn nằm trong ô", rồi bắn một Enter thừa VÀ báo sai cho chủ máy. Ô nhập là
/// khối đóng khung cuối cùng, và chỉ nó mới trả lời được câu hỏi này.
#[test]
fn a_line_echoed_in_the_transcript_is_not_a_line_still_in_the_box() {
    let typed = "Phải thêm thông tin để biết phiên đang làm dự án nào";
    let screen = "❯ Phải thêm thông tin để biết phiên đang làm dự án nào\n\
                  ⏺ Đang xem lại danh sách phiên…\n\
                  ╭──────────────╮\n\
                  │ >                    │\n\
                  ╰──────────────╯";
    assert!(
        !hub::keys::still_in_box(screen, typed),
        "đọc phần hội thoại thành nội dung ô nhập"
    );
}

/// Chữ QUÁ NGẮN không đủ đặc trưng — thà bỏ sót một Enter còn hơn bắn nhầm.
///
/// "2" hay "ok" nằm sẵn trong gần như mọi màn hình; nhận nhầm thành "còn trong
/// ô" là gửi một Enter thừa, mà Enter thừa trên một hộp chọn là CHỐT hộ chủ máy.
#[test]
fn a_very_short_line_never_triggers_a_stray_enter() {
    assert!(!hub::keys::still_in_box("… 2 …", "2"));
    assert!(!hub::keys::still_in_box("nói ok đi", "ok"));
}

/// Nhãn nút gọn hơn dòng chữ nhưng vẫn phải nói phiên nào + có chạy không.
#[test]
fn a_button_label_still_answers_which_and_whether() {
    let s = sess("aaaaaaaa-0000-0000-0000-000000000000", "hub", "acc3", true);
    let label = session_button_label(&s);
    assert!(label.starts_with('🟢'), "{label}");
    assert!(label.contains("hub") && label.contains("acc3"), "{label}");
    let idle = session_button_label(&sess("b", "dwork", "acc1", false));
    assert!(idle.starts_with('🟡'), "{idle}");
}

// ─────────────────────────────────────────────────────────────────────────────
// NÚT "VÀO PHIÊN" — Hà 2026-08-12: *"nếu báo phiên khác phiên đang theo thì
// thêm nút vào phiên"*.
//
// Không có nút thì tin báo bắt người đọc gõ tay `/session <uuid>` trên điện
// thoại — đúng loại việc làm người ta bỏ tính năng.
// ─────────────────────────────────────────────────────────────────────────────

const SID: &str = "bc1a73db-1111-2222-3333-444444444444";

#[test]
fn the_enter_session_button_decodes_back_to_the_session_route() {
    let b = hub::telegram::choice_buttons(SID, &["Tôi tiếp".to_string()], true, false, &[]);
    let (label, data) = b.last().expect("thiếu nút vào phiên");
    assert!(label.contains("Vào phiên"), "nhãn khó hiểu: {label}");
    // Round-trip qua CHÍNH bộ giải mã đang chạy — nút gửi đi mà không giải ra
    // được lệnh nào thì nó chỉ là một hình vẽ.
    assert_eq!(
        hub::telegram::callback_to_command(data).as_deref(),
        Some(format!("/session {SID}").as_str())
    );
    // …và vẫn dưới trần 64 byte của Telegram.
    assert!(
        data.len() <= 64,
        "callback_data {} byte: {data}",
        data.len()
    );
}

/// Phiên ĐANG theo thì không có nút ấy — bấm vào chỉ để tới chỗ đang đứng.
#[test]
fn the_followed_session_gets_no_redundant_button() {
    let b = hub::telegram::choice_buttons(SID, &["Tôi tiếp".to_string()], false, false, &[]);
    assert_eq!(b.len(), 1, "thừa nút: {b:?}");
    assert!(b[0].1.starts_with("key:"), "{:?}", b[0]);
}

// ─────────────────────────────────────────────────────────────────────────────
// Nút "vào phiên" phải có một phiên SỐNG để vào.
//
// Hà 2026-08-12, đọc đúng tin `⏹ hub-67 (033059d8) đã tắt — cửa sổ ấy nay đang
// chạy phiên hub-ec.` kèm nút: *"tại sao 1 phiên đã tắt mà vẫn gắn nút vào
// phiên để làm gì?"* · *"hình như phiên nào bạn cũng mặc định gắn nút vào
// phiên, quá vô lý"*. Luật cũ chỉ hỏi "có phải phiên đang theo không", không
// bao giờ hỏi "phiên còn sống không".
// ─────────────────────────────────────────────────────────────────────────────

fn ended(id: &str, name: &str) -> hub::watch::Change {
    hub::watch::Change::Ended {
        id: id.to_string(),
        name: name.to_string(),
        was_working: false,
        tty: "ttys002".to_string(),
        kind: "interactive".to_string(),
        parent: String::new(),
    }
}

const DEAD: &str = "033059d8-1111-2222-3333-444444444444";
const HEIR: &str = "cfd25b5f-5555-6666-7777-888888888888";

/// Phiên đã tắt thì KHÔNG có nút — bấm vào là đi tới một phiên không tồn tại.
#[test]
fn a_dead_session_offers_nothing_to_walk_into() {
    assert_eq!(
        hub::pipeline::enter_button(&ended(DEAD, "hub-67"), DEAD, None, "khac"),
        None
    );
}

/// Trừ khi cửa sổ của nó đã bị phiên khác chiếm: lúc ấy nút trỏ vào **phiên
/// đang ngồi ở đó**, và nhãn mang tên phiên MỚI. Một cái nút gọi tên người chết
/// là một cái nút nói dối.
#[test]
fn a_taken_over_window_sends_you_to_the_session_that_holds_it() {
    let (label, data) =
        hub::pipeline::enter_button(&ended(DEAD, "hub-67"), DEAD, Some((HEIR, "hub-ec")), "khac")
            .expect("mất đường vào phiên đang giữ cửa sổ");
    assert!(label.contains("hub-ec"), "nhãn gọi nhầm tên: {label}");
    assert!(
        !label.contains("hub-67"),
        "nhãn gọi tên phiên đã chết: {label}"
    );
    assert_eq!(
        callback_to_command(&data).as_deref(),
        Some(format!("/session {HEIR}").as_str())
    );
}

/// …và nếu phiên chiếm cửa sổ CHÍNH LÀ phiên đang theo thì cũng không cần nút.
#[test]
fn no_button_when_the_heir_is_already_the_followed_session() {
    assert_eq!(
        hub::pipeline::enter_button(&ended(DEAD, "hub-67"), DEAD, Some((HEIR, "hub-ec")), HEIR),
        None
    );
}

/// Phiên còn SỐNG mà không phải phiên đang theo thì vẫn có nút — luật này không
/// được siết nhầm sang phía kia.
#[test]
fn a_live_session_still_gets_its_button() {
    let c = hub::watch::Change::Finished {
        id: SID.to_string(),
        name: "projects-fb".to_string(),
        ran_sec: 90,
    };
    let (label, data) = hub::pipeline::enter_button(&c, SID, None, "khac").expect("mất nút");
    assert!(label.contains("projects-fb"), "{label}");
    assert_eq!(
        callback_to_command(&data).as_deref(),
        Some(format!("/session {SID}").as_str())
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Bấm "vào phiên" phải trả lời NGAY: tên và tài khoản lấy từ SỔ.
//
// Hà 2026-08-12: *"bấm vào phiên vẫn phản hồi rất chậm, sao không chỉnh để nhận
// được luôn"*. Đo: `command_done kind=Session ms=48407` — 48 giây, nằm gọn
// trong lệnh, và đi vào đúng một dòng `snapshot_cached(20s)` gọi CHỈ để lấy
// hai chuỗi ký tự cho câu chào.
// ─────────────────────────────────────────────────────────────────────────────

fn book_json(id: &str, name: &str, account: &str) -> String {
    format!(
        r#"{{"{id}":{{"s":"idle","y":"ttys002","k":"interactive","p":"","f":1786500000,"h":false,"n":"{name}","d":"AI/hub","a":"{account}","c":"/Users/hanguyen/projects"}}}}"#
    )
}

#[test]
fn following_a_session_is_answered_from_the_book_not_from_a_new_snapshot() {
    let b = book_json(SID, "projects-fb", "acc1");
    // Tên mang theo dự án (`sessions::display_name`): `projects-fb` một mình
    // không nói được gì — mọi phiên trên máy này đều tên như thế.
    assert_eq!(
        hub::pipeline::session_name_from_book(&b, SID),
        Some(("[AI/hub]".to_string(), "acc1".to_string()))
    );
}

/// Sổ không biết id thì trả `None` — chỗ gọi rơi về đường ảnh chụp, nơi câu từ
/// chối còn nói được "đang có N phiên". Đoán bừa một cái tên thì tệ hơn chậm.
#[test]
fn an_unknown_id_falls_through_instead_of_inventing_a_name() {
    let b = book_json(SID, "projects-fb", "acc1");
    assert_eq!(hub::pipeline::session_name_from_book(&b, "khong-co"), None);
    assert_eq!(hub::pipeline::session_name_from_book("{}", SID), None);
    assert_eq!(
        hub::pipeline::session_name_from_book("khong-phai-json", SID),
        None
    );
}

/// Sổ cũ (ghi từ trước khi nhớ tên) có id mà tên rỗng — cũng phải rơi xuống
/// đường kia, không chào bằng một cái tên trống.
#[test]
fn a_book_entry_without_a_name_is_not_good_enough_to_greet_with() {
    let b = book_json(SID, "", "acc1");
    assert_eq!(hub::pipeline::session_name_from_book(&b, SID), None);
}

/// Nút trả lời hộp chọn vẫn giữ nguyên đường cũ, kể cả khi có thêm nút vào phiên.
#[test]
fn choice_buttons_still_answer_the_right_session() {
    let labels = vec!["Một".to_string(), "Hai".to_string(), "Ba".to_string()];
    let b = hub::telegram::choice_buttons(SID, &labels, true, false, &[]);
    assert_eq!(b.len(), 4, "3 lựa chọn + 1 nút vào phiên: {b:?}");
    assert_eq!(
        hub::telegram::callback_to_command(&b[2].1).as_deref(),
        Some(format!("/key {SID} 3").as_str())
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// TỰ XOÁ TIN CŨ — Hà 2026-08-12: *"đã có cơ chế tự xóa tin nhắn cũ hơn 1.5 ngày
// chưa"*. Chưa có; đây là nó.
//
// Ràng buộc THẬT, không phải lựa chọn của hub: Telegram chỉ cho bot xoá tin của
// chính nó trong **48 giờ**. Quá đó là vĩnh viễn không xoá được — nên tin quá
// hạn phải bị bỏ khỏi sổ kèm log, chứ không nằm lại bắt hub thử mãi một việc
// không bao giờ xong.
// ─────────────────────────────────────────────────────────────────────────────

const H: i64 = 3600;
/// Đồng hồ giả riêng cho nhóm test này.
const T0: i64 = 1_800_000_000;

#[test]
fn a_message_younger_than_the_limit_is_left_alone() {
    let (due, gone) = hub::telegram::due_for_delete(&[(1, T0 - 35 * H)], T0, 36);
    assert!(due.is_empty(), "35 giờ mà đã đòi xoá: {due:?}");
    assert!(gone.is_empty());
}

#[test]
fn a_message_past_the_limit_is_deleted() {
    let (due, gone) = hub::telegram::due_for_delete(&[(7, T0 - 37 * H)], T0, 36);
    assert_eq!(due, vec![7]);
    assert!(gone.is_empty());
}

/// Quá 48 giờ: KHÔNG gọi Telegram nữa, chỉ bỏ khỏi sổ.
#[test]
fn a_message_past_telegrams_own_window_is_dropped_not_retried() {
    let (due, gone) = hub::telegram::due_for_delete(&[(9, T0 - 49 * H)], T0, 36);
    assert!(
        due.is_empty(),
        "gọi xoá một tin Telegram không cho xoá: {due:?}"
    );
    assert_eq!(gone, vec![9], "phải bỏ khỏi sổ, đừng giữ lại thử mãi");
}

/// `0` = tắt hẳn, và tắt phải là tắt: không đụng tin nào.
#[test]
fn zero_hours_turns_the_whole_thing_off() {
    let list = [(1, T0 - 100 * H), (2, T0 - 40 * H)];
    let (due, gone) = hub::telegram::due_for_delete(&list, T0, 0);
    assert!(due.is_empty() && gone.is_empty(), "{due:?} {gone:?}");
}

/// Ngưỡng mặc định phải nằm DƯỚI trần 48h một khoảng an toàn — đặt sát trần là
/// tự dựng bẫy: hub ngủ một giấc là cả loạt tin rơi ra ngoài cửa.
#[test]
fn the_default_window_leaves_room_before_telegrams_hard_limit() {
    let cfg = hub::config::Config::default();
    let h = cfg.confirm.delete_after_hours as i64;
    assert_eq!(h, 36, "mặc định phải đúng 1,5 ngày Hà đặt");
    assert!(
        h * H + 6 * H <= hub::telegram::TELEGRAM_DELETE_WINDOW_SEC,
        "còn dưới 6 giờ dự phòng trước trần 48h"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// TOKEN SAI PHẢI KÊU, KHÔNG ĐƯỢC IM
//
// 🔴 Bắt được 2026-08-12 lúc Hà đổi bot: `getUpdates` với token sai trả về JSON
// HỢP LỆ (`{"ok":false,"description":"Unauthorized"}`), nên `r.json()` thành
// công, `result` rỗng, và vòng lặp đọc nó y như "không ai nhắn gì" — không log,
// không lỗi, kênh chết câm. Từ bên ngoài, một token sai trông hệt một buổi
// chiều yên tĩnh.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn an_unauthorized_poll_is_reported_not_read_as_silence() {
    let resp = serde_json::json!({ "ok": false, "description": "Unauthorized" });
    assert_eq!(
        hub::telegram::poll_rejected(&resp).as_deref(),
        Some("Unauthorized")
    );
}

#[test]
fn a_refusal_without_a_reason_still_says_something() {
    let resp = serde_json::json!({ "ok": false });
    let why = hub::telegram::poll_rejected(&resp).expect("từ chối mà không kêu");
    assert!(!why.trim().is_empty(), "câu báo rỗng thì cũng là im lặng");
}

#[test]
fn a_normal_poll_is_not_mistaken_for_a_refusal() {
    let resp = serde_json::json!({ "ok": true, "result": [] });
    assert!(hub::telegram::poll_rejected(&resp).is_none());
}

// ─────────────────────────────────────────────────────────────────────────────
// LỆNH THẤY TRÊN MÀN → NÚT GỬI NHANH
//
// Hà 2026-08-12: *"phiên hiện ra rõ ràng có lệnh để chạy trên terminal … nếu có
// lệnh như vậy thì hiển thị luôn lệnh gửi nhanh"* → *"có thể chạy trực tiếp từ ô
// chat trong cli bằng cách thêm ký tự `!` ở đầu"*.
//
// Nhận diện theo HÌNH DẠNG và cố ý HẸP: đoán rộng ở đây là đưa lên màn một cái
// nút chạy nhầm thứ, mà nút thì bấm một cái là xong.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn a_command_on_screen_is_picked_up_with_its_arguments() {
    let screen = "  Trạng thái để lại: cây sạch.\n\
                  Chạy nốt lệnh này:\n\
                    git -C ~/projects/AI/tcc/amm push origin main\n\
                  Xong thì báo lại.";
    let got = hub::keys::commands_on_screen(screen, 4);
    assert_eq!(got, vec!["git -C ~/projects/AI/tcc/amm push origin main"]);
}

/// Câu văn không được thành nút.
#[test]
fn prose_is_not_mistaken_for_a_command() {
    let screen = "Tôi sẽ chạy git để kiểm tra.\nls các thư mục xong rồi.\nfind ra nguyên nhân:";
    assert!(
        hub::keys::commands_on_screen(screen, 4).is_empty(),
        "{:?}",
        hub::keys::commands_on_screen(screen, 4)
    );
}

/// Dấu nhắc shell đứng trước lệnh phải bị bóc, và lệnh trần (không tham số) bỏ qua.
#[test]
fn prompts_are_stripped_and_bare_verbs_ignored() {
    // `cargo` trần: đủ dài để qua cửa độ dài, nên nó ghim ĐÚNG luật "phải có
    // tham số" chứ không ăn theo một luật khác.
    let screen = "$ cargo test --offline\n❯ cargo\n  ./deploy/install.sh --no-build";
    let got = hub::keys::commands_on_screen(screen, 4);
    assert!(got.contains(&"cargo test --offline".to_string()), "{got:?}");
    assert!(
        got.contains(&"./deploy/install.sh --no-build".to_string()),
        "{got:?}"
    );
    assert!(
        !got.iter().any(|c| c == "cargo"),
        "lệnh trần vẫn lọt: {got:?}"
    );
}

/// Giữ các dòng CUỐI (mới nhất) và bỏ trùng.
#[test]
fn only_the_latest_few_survive_and_duplicates_collapse() {
    let screen =
        "git status\ngit status\nnpm run build\ncargo test --offline\nnode fe-smoke.mjs a b c";
    let got = hub::keys::commands_on_screen(screen, 2);
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(
        got[1], "node fe-smoke.mjs a b c",
        "phải giữ dòng mới nhất: {got:?}"
    );
}

/// 🔴 Đo trên bản THẬT, lượt `/shot` đầu tiên (2026-08-12 21:15): màn có dòng
/// "`git push origin main` (a plain push to main) executed from a nested-repo".
/// Bản đầu bóc dấu nháy mở rồi nuốt luôn cả câu phía sau ⟹ một cái nút chạy
/// nhầm thứ. Chỉ ĐỌC MÃ thì thấy hợp lý; chỉ chạy thật mới thấy.
#[test]
fn a_command_quoted_inside_prose_keeps_only_the_command() {
    let screen = "  `git push origin main` (a plain push to main) executed from a nested-repo";
    assert_eq!(
        hub::keys::commands_on_screen(screen, 4),
        vec!["git push origin main"]
    );
}

/// Và một câu văn có dấu phẩy thì không phải lệnh, dù mở đầu bằng tên lệnh.
#[test]
fn a_sentence_with_a_comma_is_not_a_command() {
    let screen = "git status trước, rồi push sau";
    assert!(hub::keys::commands_on_screen(screen, 4).is_empty());
}

/// Lệnh nằm trong DẤU NHÁY giữa câu văn cũng phải bấm chạy được.
///
/// 🔴 Hà 2026-08-12: *"nội dung của phiên có lệnh script cần chạy đã có tính
/// năng bấm chạy luôn chưa"*. Có — nhưng luật cũ đòi lệnh **đứng đầu dòng**,
/// đúng cho một màn terminal và sai cho một BÁO CÁO. Đo trên tin báo thật: 0
/// nút, trong khi báo cáo viết rõ hai việc cần chạy.
#[test]
fn a_command_quoted_in_a_report_becomes_a_button_cut_at_the_backticks() {
    let report = "Còn đúng hai việc của anh:\n\
        1. `git -C ~/projects/AI/tcc/amm push origin main` (3 commit, hook chặn tôi push main)\n\
        2. Chạy `bash ./deploy.sh perapp-storage` rồi kiểm lại.\n\
        cargo test --all-targets chưa chạy lại sau bản vá SDK đó.";
    let got = hub::keys::commands_on_screen(report, 4);
    assert!(
        got.contains(&"git -C ~/projects/AI/tcc/amm push origin main".to_string()),
        "{got:?}"
    );
    assert!(
        got.contains(&"bash ./deploy.sh perapp-storage".to_string()),
        "{got:?}"
    );
    // Câu VĂN mở đầu bằng một lệnh thì KHÔNG thành nút — nút chạy nhầm thứ còn
    // tệ hơn không có nút.
    assert!(
        !got.iter().any(|c| c.starts_with("cargo test")),
        "biến một câu văn thành nút: {got:?}"
    );
    // Và cắt ĐÚNG trong cặp nháy: bẫy 08-12 tối là bóc nháy mở rồi nuốt cả câu
    // phía sau, ra một cái nút chạy nhầm thứ.
    let prose = "Nhắc tới `git push origin main` (a plain push to main) executed from…";
    assert_eq!(
        hub::keys::commands_on_screen(prose, 4),
        vec!["git push origin main".to_string()]
    );
}

/// Lệnh DÀI hơn bề ngang cửa sổ vẫn phải ra nút — TUI bẻ dòng, hub nối lại.
///
/// 🔴 Hà 2026-08-13, ảnh chụp Telegram: *"Không có lệnh merge mà bấm"*. Chữ dưới
/// đây là bản CHÉP NGUYÊN từ nhật ký (`kind=Shot`, 04:15:24Z) — kể cả dấu cách
/// cuối dòng và hai dấu cách thụt đầu dòng, vì chính chúng là bằng chứng chỗ bẻ
/// rơi vào ranh giới từ. Cổng `contains('\n')` viết hôm 08-12 vứt thẳng span
/// này, tức nó loại đúng những lệnh dài — thứ đáng có nút nhất.
#[test]
fn a_command_the_terminal_wrapped_is_joined_back_not_dropped() {
    let screen =
        "※ recap: Goal was fixing what the tfl5 walk found; that's done, pushed, and PR \n  \
                  #54 is open with all tests green. Next action is yours: merge PR #54, then \n  \
                  deploy with `bash scripts/deploy.sh walk-fixes-0813 --expect-symbol \n  \
                  renderChatPending`. (disable recaps in /config)";
    assert_eq!(
        hub::keys::commands_on_screen(screen, 4),
        vec![
            "bash scripts/deploy.sh walk-fixes-0813 --expect-symbol renderChatPending".to_string()
        ]
    );

    // …nhưng bẻ GIỮA MỘT TỪ thì KHÔNG nối: nối lại là bịa ra một lệnh khác, và
    // một cái nút chạy nhầm thứ tệ hơn hẳn một cái nút thiếu.
    let cut_mid_word = "chạy `bash scripts/deploy.sh walk-fix\nes-0813 --expect-symbol x`";
    assert!(
        hub::keys::commands_on_screen(cut_mid_word, 4).is_empty(),
        "{:?}",
        hub::keys::commands_on_screen(cut_mid_word, 4)
    );

    // Và một KHỐI chữ trong cặp nháy vẫn không phải một cái nút.
    let block = "xem `line one here\n\nline two here\nline three\nline four\nline five`";
    assert!(hub::keys::commands_on_screen(block, 4).is_empty());
}

/// Lệnh nằm trong KHỐI CODE vẫn phải ra nút.
///
/// 🔴 Hà 2026-08-13: *"ở [dwork] đang có lệnh và cũng không hiển thị nút chạy"*.
/// Ba dòng `bash ./dci-deploy-be.sh …` nằm trong một khối ```, mà
/// `watch::key_points` **cố ý bỏ khối code** khi rút gọn tin — rồi nút lại được
/// dựng từ bản đã rút gọn ấy. Hai luật đều đúng một mình; ghép lại thì đúng
/// những dòng ĐÁNG BẤM NHẤT là những dòng duy nhất không tới được chỗ nhận
/// diện. Nay `announce_changes` quét BẢN DÀI; test này giữ vế còn lại: bộ nhận
/// diện phải đọc được khối code khi được đưa cho nó.
#[test]
fn commands_inside_a_fenced_block_still_become_buttons() {
    let report = "Deploy vẫn bị chặn quyền. Ba lệnh cần bạn chạy:\n\
        ```\n\
        bash ./dci-deploy-be.sh module/\n\
        bash ./dci-deploy-be.sh dci/leave-quota/\n\
        bash ./dci-deploy-be.sh dci/config/holiday/\n\
        ```";
    let got = hub::keys::commands_on_screen(report, 4);
    assert_eq!(got.len(), 3, "{got:?}");
    assert!(
        got.contains(&"bash ./dci-deploy-be.sh module/".to_string()),
        "{got:?}"
    );
    assert!(
        got.contains(&"bash ./dci-deploy-be.sh dci/config/holiday/".to_string()),
        "{got:?}"
    );
    // Dấu rào ``` không phải một lệnh.
    assert!(!got.iter().any(|c| c.contains("``")), "{got:?}");
}

/// `!<lệnh>` là quy ước của CHÍNH hub — nó phải nhận ra được chữ mình dạy.
///
/// 🔴 Hà 2026-08-13, ảnh chụp màn `[AI/codetrail]`: *"rõ ràng có lệnh chạy
/// trong nội dung nhưng lại không có nút để chạy nó"*. Dòng trên màn là
/// `! git -C ~/projects/AI/codetrail push origin main` — đúng hình dạng nút
/// `▶` gõ vào phiên, mà `!` lại không nằm trong danh sách dấu nhắc cần bóc, nên
/// từ đầu tiên là `!` chứ không phải `git` ⟹ 0 nút.
#[test]
fn hub_recognises_the_bang_prefix_it_invented() {
    let screen = "Anh gõ giúp:\n  ! git -C ~/projects/AI/codetrail push origin main";
    assert_eq!(
        hub::keys::commands_on_screen(screen, 4),
        vec!["git -C ~/projects/AI/codetrail push origin main".to_string()]
    );
    // Dạng không có dấu cách cũng thế.
    assert_eq!(
        hub::keys::commands_on_screen("!cargo test --offline", 4),
        vec!["cargo test --offline".to_string()]
    );
}

/// File được NHẮC TỚI phải mở được — nhưng chỉ file chữ, chỉ đường tuyệt đối.
///
/// 🔴 Hà 2026-08-13: *"các nội dung có path file thì nên cho click vào nhận
/// được file để mở trực tiếp trên tele"*. Trước đó cây cầu tệp đi MỘT CHIỀU:
/// hub nhận được tệp từ Telegram nhưng không gửi ra được cái nào.
#[test]
fn a_file_path_on_screen_becomes_something_you_can_open() {
    let text = "Đã viết xong /Users/hanguyen/projects/AI/hub/ARCHITECTURE.md và \
                `~/projects/AI/hub/README.md` — đọc thử đi.";
    let got = hub::keys::paths_on_screen(text, 4);
    assert!(
        got.contains(&"/Users/hanguyen/projects/AI/hub/ARCHITECTURE.md".to_string()),
        "{got:?}"
    );
    assert!(
        got.contains(&"~/projects/AI/hub/README.md".to_string()),
        "{got:?}"
    );

    // Dấu chấm cuối câu không được dính vào tên file.
    let dot = hub::keys::paths_on_screen("xem /tmp/bao-cao.md.", 4);
    assert_eq!(dot, vec!["/tmp/bao-cao.md".to_string()]);

    // Đường TƯƠNG ĐỐI bỏ qua: `src/main.rs` không nói được nó thuộc dự án nào,
    // mà đoán sai ở đây là gửi nhầm file của dự án khác.
    assert!(hub::keys::paths_on_screen("sửa ở src/main.rs rồi", 4).is_empty());

    // File NHỊ PHÂN không gửi: cổng quét rò chỉ đọc được chữ, mà một ảnh chụp
    // màn hình có thể mang nguyên một mật khẩu.
    assert!(hub::keys::paths_on_screen("ảnh ở /tmp/man-hinh.png nhé", 4).is_empty());
    assert!(hub::keys::paths_on_screen("/tmp/data.sqlite", 4).is_empty());

    // 🔴 Đuôi LẠ vẫn phải ra nút. Bản đầu dùng danh sách TRẮNG và nó sai ngay
    // lần dùng đầu tiên: tôi mời Hà bấm thử `hub.env.example`, đuôi `.example`
    // không có trong danh sách ⟹ không nút nào hiện. Câu hỏi thật là *đọc được
    // chữ không*, mà câu ấy chỉ trả lời được lúc mở file — nên nó được hỏi ở
    // `send_document`, không phải ở đây.
    assert_eq!(
        hub::keys::paths_on_screen("chép /tmp/hub.env.example ra", 4),
        vec!["/tmp/hub.env.example".to_string()]
    );

    // Thư mục thì không: không có tên file thì không có gì để gửi.
    assert!(hub::keys::paths_on_screen("mở /Users/hanguyen/projects/AI/hub xem", 4).is_empty());

    // Và câu CẤM thì vẫn không thành nút, y như với lệnh.
    assert!(hub::keys::paths_on_screen("⚠ đừng mở /tmp/bi-mat.md", 4).is_empty());
}

/// Câu CẤM một lệnh không được biến thành cái nút chạy chính lệnh ấy.
///
/// 🔴 Trả giá đúng ngày đặt tính năng, 2026-08-13: bộ gác lệnh từ chối
/// `git filter-branch` và in ra câu giải thích có chứa chính lệnh ấy trong dấu
/// nháy. hub đọc màn, thấy hình dạng một lệnh, gửi cho Hà ba cái nút — trong đó
/// có `▶ git filter-branch --force`. Một lời cảnh báo biến thành một cú bấm là
/// làm đúng điều bị cấm.
#[test]
fn a_warning_about_a_command_never_becomes_a_button_for_it() {
    // Chép nguyên hình dạng câu của bộ gác.
    let block = "BLOCK: `git filter-branch --force` on `rewrite/main` rewrites commit history";
    assert!(
        hub::keys::commands_on_screen(block, 4).is_empty(),
        "{:?}",
        hub::keys::commands_on_screen(block, 4)
    );

    // Và câu cảnh báo của chính hub.
    let mine = "⚠ Nút lệnh của hub vừa mời anh bấm `git filter-branch --force` — đừng bấm";
    assert!(hub::keys::commands_on_screen(mine, 4).is_empty(), "{mine}");

    // Lệnh đứng đầu dòng trong một dòng cảnh báo cũng không.
    let bare = "❌ KHÔNG ĐƯỢC chạy dòng này:\n⚠ git push --force origin main";
    assert!(
        hub::keys::commands_on_screen(bare, 4).is_empty(),
        "{:?}",
        hub::keys::commands_on_screen(bare, 4)
    );

    // …nhưng câu MỜI chạy thì vẫn phải ra nút, không thì cửa này ăn hết.
    let invite = "Chạy giúp tôi `cargo test --offline` rồi báo lại";
    assert_eq!(
        hub::keys::commands_on_screen(invite, 4),
        vec!["cargo test --offline".to_string()]
    );
}

/// Bấm "Xem đầy đủ" là đã chọn phiên ấy — nhưng chỉ được NÓI khi sổ đã ghi.
///
/// 🔴 Hà 2026-08-13: *"khi bấm xem đầy đủ thì rõ ràng nó đang ở phiên đúng rồi
/// cần gì có nút vào phiên nữa"*. Sáng nay chính anh xin cái nút ấy; chiều dùng
/// thật thì nó là một cú bấm thừa. Nay hub đi luôn — mà đổi con trỏ là đổi NƠI
/// CHỮ ANH GÕ SẼ ĐI TỚI, nên nhánh ghi-hỏng phải nói thật, không được im cũng
/// không được khoe.
#[test]
fn the_full_report_says_where_the_cursor_went_only_when_it_really_went() {
    use hub::pipeline::full_report_follow_note;

    // Đang theo sẵn phiên ấy ⟹ không thêm chữ nào.
    assert_eq!(full_report_follow_note("[dwork]", None), "");

    let ok = full_report_follow_note("[dwork]", Some(true));
    assert!(ok.contains("Đang theo phiên [dwork]"), "{ok}");
    assert!(ok.contains("gõ thẳng vào đây"), "{ok}");
    assert!(!ok.contains("⚠"), "{ok}");

    // Ghi sổ hỏng: TUYỆT ĐỐI không được in "Đang theo" — câu ấy làm chủ máy gõ
    // việc vào nhầm phiên.
    let bad = full_report_follow_note("[dwork]", Some(false));
    assert!(bad.contains("chưa chuyển được"), "{bad}");
    assert!(bad.contains("vẫn đang theo phiên cũ"), "{bad}");
    assert!(
        !bad.contains("Đang theo phiên [dwork]"),
        "khoe đã chuyển trong khi sổ chưa ghi: {bad}"
    );
}

/// `gh` là công cụ merge trên máy này — nó phải nằm trong danh sách lệnh quen.
///
/// Câu Hà hỏi 2026-08-13 là *"Không có lệnh merge mà bấm"*: màn viết *"Next
/// action is yours: merge PR #54"*. Chữ ấy là CÂU VĂN nên không có nút nào dựng
/// được từ nó — nhưng lúc phiên viết ra lệnh thật thì phải có.
#[test]
fn a_gh_command_is_known_so_a_merge_can_be_pressed() {
    let screen = "Việc của anh:\n  gh pr merge 54 --squash --delete-branch";
    assert_eq!(
        hub::keys::commands_on_screen(screen, 4),
        vec!["gh pr merge 54 --squash --delete-branch".to_string()]
    );
    // Câu VĂN nhắc chuyện merge thì vẫn không thành nút.
    let prose = "Next action is yours: merge PR #54, then deploy";
    assert!(hub::keys::commands_on_screen(prose, 4).is_empty());
}

/// Số trên nút "xem đầy đủ" không được LỆCH khi bản cũ rơi ra khỏi kho.
///
/// 🔴 Hà 2026-08-12: *"cuối tin nhắn sao lại báo còn số dòng vậy, muốn xem nốt
/// thì làm thế nào"*. Kho giữ 8 bản gần nhất; nếu đánh số theo VỊ TRÍ trong
/// mảng thì mỗi lần đẩy một bản mới, nút cũ trong lịch sử chat sẽ trả về báo
/// cáo của một phiên khác — sai còn tệ hơn không có nút.
#[test]
fn a_stale_full_report_button_says_so_instead_of_showing_someone_elses() {
    let dir = std::env::temp_dir().join(format!("hub-full-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = hub::db::Db::open(&dir.join("t.sqlite")).unwrap();

    let mut first = None;
    for i in 0..12 {
        let (_, data) = hub::pipeline::remember_full(
            &db,
            &format!("sess-{i}"),
            &format!("[hub] phiên {i}"),
            &format!("báo cáo số {i}"),
        )
        .unwrap();
        if i == 0 {
            first = Some(data);
        }
    }
    // Bản mới nhất lấy được nguyên văn.
    // …và mang theo CHỦ của nó, để nút "vào phiên" gắn được (Hà 2026-08-13).
    let got = hub::pipeline::full_report(&db, 11).expect("mất bản mới nhất");
    assert_eq!(got.0, "sess-11");
    assert_eq!(got.1, "[hub] phiên 11");
    assert_eq!(got.2, "báo cáo số 11");
    // Bản đầu đã rơi ra ⟹ trả None, KHÔNG trả nhầm bản khác.
    let n: usize = first.unwrap().trim_start_matches("full:").parse().unwrap();
    assert_eq!(n, 0);
    assert_eq!(hub::pipeline::full_report(&db, 0), None);

    let _ = std::fs::remove_dir_all(&dir);
}

/// Tên tệp gửi từ Telegram là chuỗi do NGƯỜI GỬI đặt — chỉ giữ phần tên cuối.
///
/// 🔴 Hà 2026-08-13: *"thêm cơ chế nhận đính kèm file vào tin nhắn"*. Đường
/// nhận tệp là đường DUY NHẤT trong hub mà một chuỗi từ ngoài quyết định một
/// đường dẫn ghi xuống đĩa, nên nó phải bị bóc thư mục trước khi chạm tới
/// `.inbox/`.
#[test]
fn an_attachment_name_can_never_choose_where_it_is_written() {
    let clean = |name: &str| -> String {
        std::path::Path::new(name)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty() && *n != "." && *n != "..")
            .unwrap_or("tep-nhan-duoc")
            .to_string()
    };
    assert_eq!(clean("log.txt"), "log.txt");
    assert_eq!(clean("thu/muc/log.txt"), "log.txt");
    assert_eq!(clean("../../../etc/hosts"), "hosts");
    assert_eq!(clean(".."), "tep-nhan-duoc");
    assert_eq!(clean(""), "tep-nhan-duoc");
    // Tên có dấu và khoảng trắng thì giữ nguyên — chỉ chặn đường dẫn, không
    // chặn tiếng Việt.
    assert_eq!(clean("báo cáo cuối.md"), "báo cáo cuối.md");
}

/// Màn có sẵn chữ trong ô nhập ⟹ đọc ra được, để dựng nút "⏎ Gửi".
///
/// 🔴 Hà 2026-08-13, gửi ảnh một màn `/shot`: *"như ảnh vừa gửi có gợi ý nội
/// dung chat cần có cách bấm nhanh để gửi nó"*. Màn ấy có `❯ làm quota phép đi`
/// nằm trong ô — chữ đã tới nơi, chỉ thiếu cú Enter — mà từ điện thoại thì
/// không bấm được cú ấy nếu không gõ lại cả câu.
#[test]
fn text_waiting_in_the_input_box_is_read_back_for_a_send_button() {
    let screen = "\
  ⎿  Tip: Run tasks in the cloud while you keep coding locally
────────────────────────────────────────
❯ làm quota phép đi
────────────────────────────────────────
  ⏵⏵ auto mode on (shift+tab to cycle) · esc to interrupt";
    assert_eq!(
        hub::keys::input_box_text(screen).as_deref(),
        Some("làm quota phép đi")
    );

    // Ô TRỐNG ⟹ không dựng nút: một cái nút gửi chữ rỗng là một cái nút gửi
    // Enter vào hư không.
    let empty = "\
────────────────────────────────────────
❯
────────────────────────────────────────
  ⏵⏵ auto mode on (shift+tab to cycle) · esc to interrupt";
    assert_eq!(hub::keys::input_box_text(empty), None);
}

/// Lệnh trên màn của phiên ĐANG THEO thì gõ thẳng vào phiên ấy — một đường.
///
/// 📌 Ở đây từng có một test cho `denied_for_session`, dựng trên tiền đề rằng
/// `!<lệnh>` bị `DENIED_TOOLS` chặn. Hà chặn đúng lúc: *"vô lý việc gõ vào
/// phiên là hub làm mà"*. `DENIED_TOOLS` gác lời gọi công cụ của AI; `!` là chế
/// độ bash của TUI, tức ngón tay chủ máy. Một cái nút, một con đường.
#[test]
fn a_quick_command_button_always_types_into_the_session() {
    assert_eq!(
        callback_to_command("run:0"),
        None,
        "`run:` do Inbox tự xử, không biến thành dòng lệnh ở đây"
    );
}

/// Danh sách phải nói phiên mở ra từ ĐÂU — vì hai nguồn làm được hai việc khác nhau.
///
/// 🔴 Hà 2026-08-13, ngay sau khi phiên VS Code hiện lên: *"ở danh sách phiên
/// nên thêm icon biểu diễn nguồn là terminal hay vs code"*. Từ hôm nay danh
/// sách trộn hai loại phiên trông y hệt nhau mà `/type` `/key` `/shot` chỉ chạy
/// trên phiên Terminal — không có dấu phân biệt thì người đọc biết bằng cách
/// nhận một câu từ chối.
#[test]
fn the_list_says_whether_a_session_lives_in_a_terminal_or_an_editor() {
    use hub::pipeline::source_icon;

    assert_eq!(source_icon("terminal"), "⌨", "gõ thẳng vào được");
    assert_eq!(source_icon("editor"), "💻", "xem và hỏi được, gõ thì không");
    assert_eq!(source_icon("background"), "🌙");
    assert_eq!(source_icon("detached"), "🔌");

    // ⚠ Dòng `editor` KHÔNG còn tới được danh sách: chiều 2026-08-13 hub thôi
    // liệt kê phiên VS Code (`sessions::snapshot`, Hà: *"nếu đã không thao tác
    // được vào vs code thì bỏ đi, chỉ làm với terminal thôi"*). Bảng tra ở trên
    // vẫn giữ nhánh ấy — nó là một phép tra thuần, và giữ để nếu có ngày phiên
    // editor quay lại thì ký hiệu đã sẵn. Còn phần dựng chữ dưới đây phải đo
    // trên loại phiên CÒN xảy ra thật, không thì test canh một màn hình không
    // ai nhìn thấy nữa.
    let mut term = sess(
        "aaaaaaaa-0000-0000-0000-000000000000",
        "projects-ff",
        "acc1",
        true,
    );
    term.folder = "AI/hub".into();
    let mut bg = sess(
        "bbbbbbbb-0000-0000-0000-000000000000",
        "merge init",
        "acc1",
        false,
    );
    bg.folder = "games".into();
    bg.host = "background".into();

    let text = session_list_text(&[term.clone(), bg.clone()], "", NOW);
    let l_term = text
        .lines()
        .find(|l| l.contains("[AI/hub]"))
        .unwrap_or_default();
    let l_bg = text
        .lines()
        .find(|l| l.contains("[games]"))
        .unwrap_or_default();
    assert!(l_term.contains("⌨"), "{text}");
    assert!(l_bg.contains("🌙"), "{text}");
    assert!(!l_term.contains("🌙"), "gắn nhầm nguồn: {text}");

    // Và trên NÚT nữa — cái nút mới là thứ ngón tay chạm vào.
    assert!(
        session_button_label(&bg).contains("🌙"),
        "{}",
        session_button_label(&bg)
    );
    assert!(
        session_button_label(&term).contains("⌨"),
        "{}",
        session_button_label(&term)
    );
}

/// Chữ ra Telegram phải sạch dấu trang trí — vì kênh ấy KHÔNG parse markdown.
///
/// 🔴 Hà 2026-08-13, gửi lại ảnh chụp chính tin của tôi: *"lệnh ở nội dung bị
/// cắt mất mã"*. Trên ảnh: hai cặp sao hiện nguyên, ba dấu nháy của khối code
/// hiện nguyên, và một dòng lệnh bị mấy ký tự ấy cắt vụn giữa chừng.
#[test]
fn telegram_text_loses_the_decoration_but_never_the_command() {
    use hub::telegram::strip_markdown;

    let msg = "**Thử được rồi** — ba nút `bash ./x.sh` phải hiện.\n\
        ```\n\
        bash ./dci-deploy-be.sh module/\n\
        ```\n\
        Xong.";
    let got = strip_markdown(msg);
    assert!(!got.contains("**"), "{got}");
    assert!(!got.contains('`'), "{got}");
    // NỘI DUNG trong khối code ở lại — đó thường là chỗ chứa lệnh.
    assert!(got.contains("bash ./dci-deploy-be.sh module/"), "{got}");
    assert!(got.contains("Thử được rồi"), "{got}");
    assert!(got.contains("bash ./x.sh"), "{got}");

    // Không đụng tới `_` và `*` lẻ: chúng nằm trong tên tệp và đường dẫn thật.
    let path = "xem /tmp/a_b-c*.md rồi báo";
    assert_eq!(strip_markdown(path), path);
}

/// hub KHÔNG được đọc lại chữ của chính nó rồi biến thành lệnh.
///
/// 🔴 Hà 2026-08-13, ảnh chụp màn phiên codetrail: *"bấm vào nút chạy lệnh thì
/// bị dính text ngoài như này"*. Thứ gõ vào phiên là nguyên dòng trang trí của
/// hub kèm cả câu trong ngoặc; zsh vấp dấu ngoặc và **cú push không hề chạy**,
/// nhưng nhìn thì như đã bấm.
#[test]
fn hub_never_reads_its_own_decoration_back_as_a_command() {
    // Chép đúng hình dạng dòng đã lọt ra shell.
    let echoed = "▶ Lệnh thấy trên màn (bấm nút dưới để gõ `!` vào chính phiên):\n\
                  • git -C ~/projects/AI/codetrail push origin main";
    let got = hub::keys::commands_on_screen(echoed, 4);
    // Dòng lệnh THẬT (sau dấu •) vẫn phải ra nút…
    assert_eq!(
        got,
        vec!["git -C ~/projects/AI/codetrail push origin main".to_string()],
        "{got:?}"
    );
    // …còn dòng trang trí thì tuyệt đối không.
    assert!(
        !got.iter()
            .any(|c| c.contains("bấm nút") || c.contains("Lệnh thấy")),
        "chữ của chính hub thành lệnh: {got:?}"
    );

    // Lượt quét trong DẤU NHÁY phải dùng CÙNG bộ luật — đây là chỗ đã thủng.
    let in_ticks = "chạy `git push (bấm nút dưới để gõ vào phiên): thêm chữ`";
    assert!(
        hub::keys::commands_on_screen(in_ticks, 4).is_empty(),
        "{:?}",
        hub::keys::commands_on_screen(in_ticks, 4)
    );
}

/// Nút file phải GỌN và phải PHÂN BIỆT được nhau.
///
/// 🔴 Hà 2026-08-13, ảnh chụp ba nút 📎 chồng nhau dưới một tin: *"sao không
/// chèn thẳng nút xem file vào nội dung cho gọn thay vì nút độc lập"*. Trên ảnh
/// có `📎 Cargo.toml`, `📎 cq.log`, `📎 Cargo.toml…` — ba hàng, và hai trong ba
/// đọc y hệt nhau.
#[test]
fn file_buttons_share_a_row_and_say_which_file_is_which() {
    use hub::telegram::Inbox;

    // Ba nút file đứng liền nhau ⟹ MỘT hàng; nút khác vẫn hàng riêng.
    let buttons = vec![
        ("👁 Vào phiên".to_string(), "sess:abc".to_string()),
        ("📎 Cargo.toml".to_string(), "file:0".to_string()),
        ("📎 cq.log".to_string(), "file:1".to_string()),
        ("📎 mod.rs".to_string(), "file:2".to_string()),
        ("📄 Xem đầy đủ".to_string(), "full:abc".to_string()),
    ];
    let rows = Inbox::keyboard_rows(&buttons);
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert_eq!(rows[0].len(), 1, "nút phiên phải đứng riêng: {rows:?}");
    assert_eq!(
        rows[1].len(),
        3,
        "ba nút file phải chung một hàng: {rows:?}"
    );
    assert_eq!(rows[2].len(), 1, "{rows:?}");

    // Quá 3 thì xuống hàng — 4 nút trên 390px là bắt đầu cắt nhãn.
    let many: Vec<(String, String)> = (0..4)
        .map(|i| (format!("📎 f{i}"), format!("file:{i}")))
        .collect();
    let rows = Inbox::keyboard_rows(&many);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0].len(), 3);
    assert_eq!(rows[1].len(), 1);
}

/// Đường dẫn bị TUI cắt cụt không được thành một cái nút.
///
/// Trên ảnh, `Cargo.toml…` sinh ra từ chính màn hình: `claude` cắt dòng lệnh
/// dài rồi dán `…` vào cuối. Một cái nút trỏ vào đó không bao giờ mở được.
#[test]
fn a_truncated_path_is_not_a_file_button() {
    use hub::keys::paths_on_screen;

    let screen =
        "cargo test --manifest-path /Users/hanguyen/projects/AI/hub/rust/Cargo.toml… (46s)\n\
                  clippy > /tmp/cq.log 2>&1\n\
                  đọc /Users/hanguyen/projects/AI/hub/rust/Cargo.toml xem";
    let got = paths_on_screen(screen, 4);
    assert!(
        got.iter().all(|p| !p.contains('…')),
        "nút ma từ đường dẫn cắt cụt: {got:?}"
    );
    assert!(got.contains(&"/tmp/cq.log".to_string()), "{got:?}");
    assert!(
        got.contains(&"/Users/hanguyen/projects/AI/hub/rust/Cargo.toml".to_string()),
        "{got:?}"
    );
    assert_eq!(got.len(), 2, "một file một nút: {got:?}");
}

/// `cd <thư mục> && <lệnh>` phải mọc ra nút chạy.
///
/// 🔴 Hà 2026-08-13, ảnh chụp một tin báo mang nguyên dòng
/// `cd ~/projects/AI/codetrail && git push` mà không có cái nút nào. Danh sách
/// động từ cố tình hẹp (nó là hàng rào, không phải bảng tra) nên `cd` không nằm
/// trong đó, và từ đầu tiên của dòng là `cd` ⟹ 0 nút.
#[test]
fn a_cd_then_command_line_is_still_a_command() {
    use hub::keys::commands_on_screen;

    let got = commands_on_screen("cd ~/projects/AI/codetrail && git push", 3);
    assert_eq!(
        got,
        vec!["cd ~/projects/AI/codetrail && git push".to_string()],
        "{got:?}"
    );

    // Dấu `;` cũng vậy.
    let got = commands_on_screen("cd /tmp; bash ./run.sh", 3);
    assert_eq!(got.len(), 1, "{got:?}");

    // Hàng rào KHÔNG được nới: phần sau `&&` vẫn phải là một động từ đã biết.
    assert!(
        commands_on_screen("cd ~/projects && rm -rf everything", 3).is_empty(),
        "nới hàng rào: {:?}",
        commands_on_screen("cd ~/projects && rm -rf everything", 3)
    );
    // `cd` một mình không chạy gì cả.
    assert!(commands_on_screen("cd ~/projects/AI/hub", 3).is_empty());
    // Và câu văn có chữ "cd" ở đầu vẫn là câu văn.
    assert!(commands_on_screen("cd vào thư mục ấy rồi chạy thử; xong báo tôi", 3).is_empty());
}

/// Nút phải nhớ PHIÊN đã sinh ra nó, không phải phiên đang được chọn lúc bấm.
///
/// 🔴 Hà 2026-08-13: *"Sao bấm nút được tạo phiên này lại gửi vào phiên đang
/// chọn thế"* · *"Nội dung có nút bấm nhưng bấm xong lại gửi vào phiên khác
/// đang đc chọn"*. Bằng chứng rơi thẳng vào cuộc trò chuyện: tin của `[tfl5]`
/// mang nút `▶ bash scripts/verify-acl-2026-08-13.sh`, bấm xong dòng
/// `!bash scripts/verify-acl-…` chạy trong phiên `[hub]` — mà tệp ấy nằm ở
/// `AI/tfl5/scripts/`, hub không có. Con trỏ focus ĐỔI ĐƯỢC giữa lúc nút sinh
/// ra và lúc nút được bấm.
#[test]
fn a_button_remembers_which_session_made_it() {
    use hub::db::Db;
    use hub::pipeline::{quick_cmd, remember_quick};

    let dir = std::env::temp_dir().join(format!(
        "hub-quick-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let db = Db::open(&dir.join("t.sqlite")).unwrap();

    let tfl5 = "4963b95c-0000-0000-0000-000000000000";
    let cmds = vec!["bash scripts/verify-acl-2026-08-13.sh".to_string()];
    // Hai nút một lệnh: ▶ (chạy trong phiên) và 🖥 (cửa sổ thật có tty).
    // MỘT lệnh, MỘT nút, nhãn đúng là lệnh ấy — Hà 2026-08-13: *"sao vẫn ra
    // một đống nút ở đây?"* · *"tôi chỉ cần biết nút đó chạy cái gì"*.
    let btns = remember_quick(&db, tfl5, &cmds);
    assert_eq!(btns.len(), 1, "{btns:?}");
    assert_eq!(btns[0].1, "run:0");
    assert!(
        btns[0].0.contains("verify-acl-2026-08-13.sh"),
        "nhãn phải là chính lệnh: {}",
        btns[0].0
    );

    // Con trỏ đã sang phiên khác — nút vẫn phải trỏ về phiên đã sinh ra nó.
    let (sid, line) = quick_cmd(&db, 0).expect("sổ phải nhớ");
    assert_eq!(sid, tfl5, "nút quên mất phiên của mình");
    assert_eq!(line, "bash scripts/verify-acl-2026-08-13.sh");

    // Sổ CŨ (mảng trần, chưa có tên phiên) ⟹ None: thà bắt bấm lại /shot còn
    // hơn gõ một dòng lệnh vào một phiên đoán bừa.
    db.set_cursor("quick:cmds", r#"["git push"]"#).unwrap();
    assert!(
        quick_cmd(&db, 0).is_none(),
        "sổ cũ mà vẫn đoán ra một phiên"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// Câu trong BÁO CÁO không được đọc thành lệnh.
///
/// 🔴 Hà 2026-08-13, ảnh chụp nút `▶ cargo test 258 · clippy 0 warning`: *"Thực
/// sự mấy cái nút đọc không dám bấm vì không thể hiểu nó làm gì"*. Cái nút ấy
/// dựng từ một dòng tổng kết trong chính báo cáo của hub — bấm vào là chạy một
/// thứ vô nghĩa. `looks_like_prose` bắt dấu phẩy và ngoặc, nhưng câu ấy ngăn vế
/// bằng dấu chấm giữa `·`, và lọt sạch mọi cửa.
#[test]
fn a_sentence_from_a_report_is_not_a_command() {
    use hub::keys::commands_on_screen;

    for prose in [
        "cargo test 258 · clippy 0 warning",
        "cargo test 256 · clippy 0 · self-install đã chạy",
        "git push — xong thì báo tôi",
        "npm test … (còn 4 dòng)",
        "cargo test 258",
    ] {
        assert!(
            commands_on_screen(prose, 3).is_empty(),
            "đọc câu văn thành lệnh: {prose:?} → {:?}",
            commands_on_screen(prose, 3)
        );
    }

    // …mà lệnh thật thì vẫn ra nút.
    for cmd in [
        "cargo test --offline",
        "git -C ~/projects/hub push origin main",
        "bash scripts/verify-acl-2026-08-13.sh",
        "cd ~/projects/hub && git push",
    ] {
        assert_eq!(commands_on_screen(cmd, 3).len(), 1, "mất nút thật: {cmd:?}");
    }
}

/// 🔴 Hà 2026-08-13, ảnh chụp bảng hỏi của `[AI/tfl5]`: *"chọn option xong thì
/// vẫn còn bước nữa nên không pass qua được"* · *"có nhiều option thì phải có
/// cơ chế chọn được nhiều"*.
///
/// Bảng nhiều câu chỉ gửi đi được khi KHÔNG còn ô trống. Bộ nút cũ chỉ mang số
/// lựa chọn (`key:<id>:<n>`), tức nó mặc định "câu đang mở là câu người ta muốn
/// trả lời" — đúng với bảng một câu, và dẫn vào ngõ cụt với bảng nhiều câu: bấm
/// xong câu đầu là hết đường, các câu sau nằm sau một phím mũi tên mà hub (đúng
/// luật) từ chối gửi khi màn đang có hộp chọn.
#[test]
fn a_table_with_several_questions_gets_a_button_for_every_question() {
    let rest = vec![hub::sessions::Question {
        header: "Đăng nhập".into(),
        question: "Đăng nhập có phân biệt hoa thường không?".into(),
        options: vec!["Không phân biệt".into(), "Chặn từ form".into()],
        multi: false,
    }];
    let b = hub::telegram::choice_buttons(
        SID,
        &["Từ chối".to_string(), "Vẫn lưu".to_string()],
        false,
        false,
        &rest,
    );
    let data: Vec<&str> = b.iter().map(|(_, d)| d.as_str()).collect();
    // Câu 1 mang số câu như mọi câu khác: một khuôn duy nhất thì chỗ nhận lệnh
    // không phải nhớ hai luật.
    assert!(
        data.contains(&format!("pick:{SID}:1.1").as_str()),
        "{data:?}"
    );
    assert!(
        data.contains(&format!("pick:{SID}:1.2").as_str()),
        "{data:?}"
    );
    // Và câu 2 — thứ trước bản vá KHÔNG có nút nào dẫn tới.
    assert!(
        data.contains(&format!("pick:{SID}:2.1").as_str()),
        "{data:?}"
    );
    assert!(
        data.contains(&format!("pick:{SID}:2.2").as_str()),
        "{data:?}"
    );
    // Trả lời đủ mọi ô rồi bảng VẪN đứng chờ một dấu Enter (`✔ Submit` là một
    // tab riêng trên ảnh) — không có nút này thì mọi cú bấm trên kia vô nghĩa.
    assert!(
        data.contains(&format!("key:{SID}:enter").as_str()),
        "{data:?}"
    );
    // Nhãn phải đọc được trong một liếc trên màn 390px: số câu ▸ số lựa chọn.
    assert!(
        b.iter().any(|(l, _)| l.starts_with("2▸1 Không phân biệt")),
        "{:?}",
        b.iter().map(|(l, _)| l).collect::<Vec<_>>()
    );
    // Cú bấm phải dịch được thành một lệnh THẬT, không thì nó rơi vào hư không.
    assert_eq!(
        callback_to_command(&format!("pick:{SID}:2.1")),
        Some(format!("/pick {SID} 2.1"))
    );
}

/// Bảng MỘT câu không đổi hình dạng: đường cũ đã chạy đúng, và đổi nó chỉ để
/// cho "nhất quán" là bắt cả một đường đang tốt gánh rủi ro của đường mới.
#[test]
fn a_single_question_keeps_the_old_shape() {
    let b = hub::telegram::choice_buttons(
        SID,
        &["Có".to_string(), "Không".to_string()],
        false,
        false,
        &[],
    );
    let data: Vec<&str> = b.iter().map(|(_, d)| d.as_str()).collect();
    assert!(data.contains(&format!("key:{SID}:1").as_str()), "{data:?}");
    assert!(!data.iter().any(|d| d.starts_with("pick:")), "{data:?}");
    // Không có bảng thì cũng không có nút Gửi: bấm số là xong, thêm một nút nữa
    // chỉ mời người ta bấm một cái Enter thừa vào phiên.
    assert!(!data.iter().any(|d| d.ends_with(":enter")), "{data:?}");
}

/// 🔴 Hà 2026-08-13: *"Nút chưa chèn vào đúng chỗ của nó"* · *"Bấm vẫn chưa
/// chạy được"*.
///
/// Đo trong log hubd: ba cú bấm (16:29:39 · 16:30:55 · 16:31:26Z) đều xếp
/// `/runin … ./hub self-install`, không cú nào có dòng `runin_ran` — mà bản cài
/// đổi lúc 16:31:37Z, tức lệnh CHẠY XONG. Nó chạy được; thứ không về là lời
/// báo, vì lệnh ấy khởi động lại chính hubd và giết mất cái mồm đang định trả
/// lời. Nút phải đi route `/upgrade`, nơi câu trả lời được gửi TRƯỚC khi
/// restart.
#[test]
fn the_rebuild_command_gets_a_button_that_goes_through_upgrade() {
    for cmd in [
        "cd ~/projects/hub && ./hub self-install",
        "./hub self-install",
        "bash deploy/install.sh",
    ] {
        assert!(hub::pipeline::is_self_rebuild(cmd), "phải nhận ra: {cmd}");
    }
    // Hàng rào HẸP: nới ra thì `npm install` cũng thành "dựng lại hub", và
    // người bấm nhận một câu trả lời nói về chuyện khác hẳn.
    for cmd in ["npm install", "cargo install cargo-nextest", "git pull"] {
        assert!(
            !hub::pipeline::is_self_rebuild(cmd),
            "không được nhận: {cmd}"
        );
    }
    assert_eq!(callback_to_command("upgrade"), Some("/upgrade".to_string()));
}

/// 🔴 Hà 2026-08-14: *"nút chạy lệnh chỉ cần 1 icon là đủ chèn ngay sau câu
/// lệnh"* · *"Chèn ngay sau câu lệnh chứ không phải 1 nút ở cuối"*.
///
/// Telegram không đặt nút giữa chữ (bàn phím luôn treo dưới đáy MỘT tin), nên
/// thứ điều khiển được là chỗ tin KẾT THÚC. Cắt ngay sau dòng lệnh thì cái nút
/// rơi đúng chỗ ấy.
#[test]
fn a_message_is_cut_right_after_each_command_line() {
    let text = "Cài bản mới:\n\
                cd ~/projects/hub && ./hub self-install\n\
                Xong thì thử lại giúp tôi.\n\
                bash ./deploy.sh\n\
                Hết.";
    let cmds = vec![
        "cd ~/projects/hub && ./hub self-install".to_string(),
        "bash ./deploy.sh".to_string(),
    ];
    let s = hub::pipeline::command_slices(text, &cmds);
    assert_eq!(s.len(), 3, "{s:#?}");
    // Mẩu 1 KẾT THÚC bằng dòng lệnh — nút của nó nằm ngay dưới, không phải
    // dưới đáy cả tin dài.
    assert!(s[0].0.ends_with("./hub self-install"), "{:?}", s[0].0);
    assert_eq!(s[0].1, Some(0));
    assert!(s[1].0.ends_with("bash ./deploy.sh"), "{:?}", s[1].0);
    assert_eq!(s[1].1, Some(1));
    // Mẩu đuôi không mang lệnh nào; nó là chỗ đứng của các nút còn lại.
    assert_eq!(s[2].1, None);
    assert!(s[2].0.contains("Hết."));
}

#[test]
fn a_command_named_twice_gets_exactly_one_button() {
    // Báo cáo hay nhắc lại lệnh ở phần tóm tắt. Hai nút giống hệt nhau cho
    // cùng một việc là mời người ta bấm hai lần.
    let text = "chạy ./hub doctor đi\nnhắc lại: ./hub doctor";
    let s = hub::pipeline::command_slices(text, &["./hub doctor".to_string()]);
    assert_eq!(s.iter().filter(|(_, i)| i.is_some()).count(), 1, "{s:#?}");
}

#[test]
fn a_message_with_no_command_line_is_not_cut_at_all() {
    // Tin gửi đi là bản RÚT GỌN, lệnh có khi chỉ nằm trong bản dài. Lúc ấy
    // không có chữ nào quanh cái nút nói nó sắp chạy gì ⟹ không tách, và nhãn
    // nút vẫn phải mang nguyên dòng lệnh.
    let text = "Phiên đã dừng, còn 12 dòng nữa.";
    let s = hub::pipeline::command_slices(text, &["bash ./deploy.sh".to_string()]);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].1, None, "không tách khi lệnh không nằm trong tin");
}

/// 🔴 Hà 2026-08-14: *"Sao không dùng Deep Links để định dạng bên trong nội
/// dung văn bản như khối lệnh thay vì tạo 1 cái nút rất khó hiểu"* · *"Hạn chế
/// dùng khối nút ở cuối tin"*.
///
/// Tài liệu Bot API (mục Commands): *"Highlight commands in messages. When the
/// user taps a highlighted command, that command is immediately sent again."*
/// Nên lựa chọn không cần nút — nó cần được VIẾT RA đúng chỗ nó thuộc về.
#[test]
fn every_option_becomes_a_tappable_command_next_to_its_question() {
    let a = hub::sessions::Asking {
        header: "Vá ACL".into(),
        question: "Server nên xử sao?".into(),
        options: vec!["Từ chối".into(), "Vẫn lưu".into()],
        multi: false,
        rest: vec![hub::sessions::Question {
            header: "Đăng nhập".into(),
            question: "Phân biệt hoa thường?".into(),
            options: vec!["Không phân biệt".into()],
            multi: true,
        }],
    };
    let txt = hub::pipeline::ask_command_lines("4963b95c-93b0-46e3-baf9-40bbfacbef2f", &a);
    // Tham số nằm trong TÊN lệnh: chạm chỉ gửi lại token, chữ sau dấu cách rơi.
    assert!(txt.contains("/pick_4963b95c_1_1 Từ chối"), "{txt}");
    assert!(txt.contains("/pick_4963b95c_1_2 Vẫn lưu"), "{txt}");
    assert!(txt.contains("/pick_4963b95c_2_1 Không phân biệt"), "{txt}");
    // Tên lệnh phải lọt trần 32 ký tự của Telegram.
    for tok in txt.split_whitespace().filter(|w| w.starts_with("/pick_")) {
        assert!(tok.len() <= 32, "tên lệnh quá dài: {tok} ({})", tok.len());
    }
    // Câu chọn-nhiều phải khai đúng bản chất, và bảng phải nói cách gửi.
    assert!(txt.contains("(CHỌN NHIỀU)"), "{txt}");
    // Lệnh gửi bảng cũng phải CHẠM ĐƯỢC: tham số nằm trong tên, không đứng sau
    // dấu cách (chạm chỉ gửi lại token lệnh).
    assert!(txt.contains("/send_4963b95c"), "{txt}");
}

#[test]
fn a_short_id_names_a_session_only_when_something_follows_it() {
    use hub::pipeline::{same_session, split_target};
    let full = "4963b95c-93b0-46e3-baf9-40bbfacbef2f";
    assert!(
        same_session(full, "4963b95c"),
        "8 ký tự đầu là một cái tên thật"
    );
    assert!(same_session(full, full));
    assert!(
        !same_session(full, "4963b95"),
        "7 ký tự thì không — nửa vời là mơ hồ"
    );
    assert!(!same_session(full, ""), "rỗng không trỏ vào đâu cả");
    // `/pick 4963b95c 2.1` → nhắm đúng phiên ấy.
    assert_eq!(
        split_target("4963b95c 2.1"),
        Some(("4963b95c".to_string(), "2.1".to_string()))
    );
    // …còn `/type deadbeef` trống thì vẫn là CHỮ gõ vào phiên đang theo.
    assert_eq!(split_target("deadbeef"), None);
}

#[test]
fn a_link_is_only_built_for_payloads_telegram_accepts() {
    // Chưa biết tên bot (chưa kịp getMe) ⟹ None ⟹ chỗ gọi rơi về nút. Một liên
    // kết không bấm được thì tệ hơn một cái nút.
    assert!(
        hub::telegram::deep_link("run_0").is_none(),
        "chưa có getMe trong test"
    );
    // Escape đúng ba ký tự Telegram đòi, không hơn: MarkdownV2 mới là thứ bắt
    // escape mọi ký tự 1–126, và đó là lý do hub gột Markdown suốt từ đầu.
    assert_eq!(
        hub::telegram::html_escape("a < b & c > d"),
        "a &lt; b &amp; c &gt; d"
    );
    assert_eq!(
        hub::telegram::html_escape("cd ~/x && ./hub"),
        "cd ~/x &amp;&amp; ./hub"
    );
}

/// Phép đo trễ chỉ được đo cái nó thật sự đo được.
///
/// 🔴 Khoá lại một phép đo đã NÓI DỐI (2026-08-14). `telegram_update_lag` rơi
/// về `callback_query.message.date` khi update là một cú bấm nút — nhưng đó là
/// dấu thời gian của **tin nhắn chứa cái nút**, tức lúc BOT gửi nó đi. Telegram
/// không có trường nào mang thời điểm bấm.
///
/// Bằng chứng nó sai, từ `logs/hub.log` ngày 08-14: ba dòng "trễ" 190s · 239s ·
/// 304s quy đúng về MỘT mốc 08:00:20 — lúc hub gửi danh sách 4 phiên. Ba cú bấm
/// vào cùng một danh sách, và con số leo thang chỉ vì cái danh sách mỗi lúc một
/// cũ. Trong quãng ấy hub vẫn nhận và chạy 6 update khác.
///
/// Một phép đo luôn có số để in là thứ khiến người đọc đi vá nhầm chỗ; ở đây nó
/// suýt đổi cả kiến trúc vòng đọc để chữa một độ trễ chưa từng tồn tại.
#[test]
fn a_button_press_carries_no_timestamp_to_measure_lag_from() {
    use hub::telegram::text_sent_at;
    use serde_json::json;

    // Tin CHỮ: có mốc thật, đo được.
    assert_eq!(
        text_sent_at(&json!({ "message": { "date": 1_786_462_200, "text": "/session" } })),
        Some(1_786_462_200)
    );
    assert_eq!(
        text_sent_at(&json!({ "edited_message": { "date": 1_786_462_201, "text": "sửa" } })),
        Some(1_786_462_201)
    );
    // Cú BẤM NÚT: `message.date` bên trong là tuổi của tin chứa nút — KHÔNG
    // được nhận là mốc gửi. Không có mốc thì không in số nào.
    assert_eq!(
        text_sent_at(&json!({
            "callback_query": {
                "id": "42",
                "data": "sess:4963b95c-93b0-46e3-baf9-40bbfacbef2f",
                "message": { "date": 1_786_462_000 }
            }
        })),
        None,
        "bấm nút thì không có mốc — chứ không phải lấy tạm tuổi của tin có nút"
    );
    assert_eq!(text_sent_at(&json!({ "my_chat_member": {} })), None);
}

/// Trần độ dài phải theo NGUỒN — báo cáo không bị bẻ dòng như màn.
///
/// 🔴 Hà 2026-08-14, ảnh chụp bản đầy đủ của `[tfl5]`: *"Có lệnh bash sao lại ko
/// có nút bấm chạy cho nó, tôi vẫn để chờ ở đấy để bạn làm"*. Trong tin có hai
/// dòng lệnh; chỉ dòng `git` có nút. Đo đúng hai dòng ấy: `git …` = 56 ký tự,
/// `bash …` = 81, và trần cũ là 60 — nó đứng đúng giữa hai dòng.
///
/// Trần 60 sinh ra cho nguồn MÀN, nơi TUI đã bẻ dòng theo bề ngang cửa sổ nên
/// một dòng dài có thể chỉ là một mẩu cụt. Chữ của báo cáo đến từ nhật ký
/// `.jsonl`, không qua cửa sổ nào — ở đó dài là dài THẬT.
#[test]
fn a_report_is_not_a_screen_so_its_lines_are_never_half_a_command() {
    // Đúng dòng trong ảnh Hà gửi (80 ký tự).
    let deploy =
        "bash /Users/hanguyen/projects/AI/tfl5/scripts/deploy.sh static-cache-refresh-0814";
    assert_eq!(deploy.len(), 81, "dòng chuẩn của ca này");
    let report = format!(
        "2. Bản vá chưa lên prod. Ba commit đang chờ ở local.\n\n\
         git -C /Users/hanguyen/projects/AI/tfl5 push origin main\n\n\
         {deploy}\n"
    );
    // BÁO CÁO: cả hai dòng đều bấm chạy được.
    assert_eq!(
        hub::keys::commands_in_report(&report, 3),
        vec![
            "git -C /Users/hanguyen/projects/AI/tfl5 push origin main".to_string(),
            deploy.to_string(),
        ]
    );
    // MÀN: dòng dài vẫn bị từ chối — lý do của trần 60 không đổi ở đó.
    assert_eq!(
        hub::keys::commands_on_screen(&report, 3),
        vec!["git -C /Users/hanguyen/projects/AI/tfl5 push origin main".to_string()],
        "trên màn, một dòng 80 ký tự có thể là mẩu cụt của một lệnh dài hơn"
    );
    // Nới trần KHÔNG được biến thành mở cửa: cả một KHỐI lệnh vẫn không ra nút
    // (Hà cùng ngày: *"Cả 1 khối lệnh dài này thì không được tạo nút"*).
    let block = format!(
        "git for-each-ref --format='%(refname)' refs/original {}",
        "| xargs -n1 git update-ref -d ".repeat(8)
    );
    assert!(
        block.len() > hub::keys::BTN_CMD_REPORT_MAX,
        "khối chuẩn phải dài hơn trần"
    );
    assert!(hub::keys::commands_in_report(&block, 3).is_empty());
}

/// Một lệnh CỤT là một lệnh KHÁC — và cụt thì thường NGẮN, nên trần độ dài
/// không bắt được nó.
///
/// 🔴 Màn THẬT của `[tfl5]` trong câu trả lời `/shot` lúc 2026-08-14 08:59,
/// chép nguyên văn từ log (`channel_command_handled`, kind Shot). Cửa sổ rộng
/// 80 — ba dòng đạt đúng 80 — và dòng lệnh bị bẻ theo TỪ nên nó chỉ dài 57:
///   [11] 57  "  bash /Users/hanguyen/projects/AI/tfl5/scripts/deploy.sh"
///   [12] 27  "  static-cache-refresh-0814"
/// 55 ký tự sau trim ⟹ LỌT trần 60 ⟹ ra một cái nút, và sổ `quick:cmds` ghi
/// đúng bản cụt ấy. Bấm vào là chạy một lệnh triển khai THIẾU tên bản, trên dự
/// án thật.
///
/// Phép đo phải là DẤU HIỆU WORD-WRAP, không phải độ dài: từ
/// `static-cache-refresh-0814` (25 ký tự) không lọt vào 23 chỗ trống còn lại,
/// nên nó bị đẩy xuống. Bản đầu của phép đo này hỏi "dòng có chạm mép không"
/// và trượt, vì một dòng bị bẻ theo từ thì KHÔNG chạm mép.
#[test]
fn a_command_cut_at_the_window_edge_is_rejoined_or_dropped() {
    let screen = "uc_site_e2e 1/1 · clippy -D warnings 0. \n  Hai điều tôi nói rõ để không ai đọc quá lời:\n  1. Chưa xác nhận cpanel/codetrail thật sự chạy nhánh STATIC trên prod — ssh\n  vps-a bị chặn trong phiên này nên tôi không đọc được cấu hình service. Nếu hoá\n  ra nó chạy chế độ full thì bản vá này vẫn đúng và vẫn cần, nhưng nguyên nhân\n  ca của codetrail sẽ là chuyện khác và tôi phải đo tiếp.\n  2. Bản vá chưa lên prod. Ba commit đang chờ ở local: sổ sách, vá hạn mức fd,\n  và cái này.\n  git -C /Users/hanguyen/projects/AI/tfl5 push origin main\n  bash /Users/hanguyen/projects/AI/tfl5/scripts/deploy.sh\n  static-cache-refresh-0814\n✻ Cooked for 13m 15s\n";
    let got = hub::keys::commands_on_screen(screen, 4);
    assert!(
        got.iter().any(|c| c
            == "bash /Users/hanguyen/projects/AI/tfl5/scripts/deploy.sh static-cache-refresh-0814"),
        "đuôi bị bẻ phải được nối lại: {got:?}"
    );
    assert!(
        !got.iter().any(|c| c.ends_with("deploy.sh")),
        "không được còn bản cụt nào: {got:?}"
    );
    // Dòng `git …` (58, đứng trước một dòng bắt đầu bằng "bash") KHÔNG bị nối:
    // từ "bash" lọt thoải mái vào 22 chỗ trống, tức nó xuống dòng vì người viết
    // muốn thế, không phải vì cửa sổ.
    assert!(
        got.iter()
            .any(|c| c == "git -C /Users/hanguyen/projects/AI/tfl5 push origin main"),
        "{got:?}"
    );
}

/// Icon phải bám được vào dòng mà CỬA SỔ đã bẻ làm đôi.
///
/// 🔴 Hà 2026-08-14: *"Rõ ràng là 1 dòng sao lại biến thành 2"*. `cmds` mang
/// bản đã nối lại (đầy đủ), còn chữ của màn thì vẫn là hai dòng — nên so nguyên
/// chuỗi sẽ trượt, mẩu không cắt được, và icon rơi về khối nút ở đáy: đúng thứ
/// vừa bị chê. Khớp theo phần đầu thì cắt đúng chỗ.
#[test]
fn an_icon_still_finds_the_line_the_window_broke_in_two() {
    let screen = "  và cái này.\n  \
bash /Users/hanguyen/projects/AI/tfl5/scripts/deploy.sh\n  \
static-cache-refresh-0814\n✻ Cooked for 13m 15s\n";
    let full = "bash /Users/hanguyen/projects/AI/tfl5/scripts/deploy.sh static-cache-refresh-0814";
    let slices = hub::pipeline::command_slices(screen, &[full.to_string()]);
    assert!(
        slices.iter().any(|(_, i)| *i == Some(0)),
        "phải cắt được một mẩu kết bằng dòng lệnh: {slices:?}"
    );
    // …và không nhầm sang một lệnh khác chỉ vì vài ký tự đầu giống nhau.
    let other = "bash /Users/hanguyen/projects/AI/OTHER/scripts/deploy.sh xyz";
    let slices2 = hub::pipeline::command_slices(screen, &[other.to_string()]);
    assert!(
        slices2.iter().all(|(_, i)| i.is_none()),
        "lệnh của dự án khác không được khớp: {slices2:?}"
    );
}

/// Chỉ câu XÁC NHẬN TRƠN mới rút thành emoji được.
///
/// 🔴 Hà 2026-08-14: *"Có thể đổi cách phản hồi tin đã gửi bằng 1 emoji trực
/// tiếp vào tin nhắn cho gọn"*. Đúng cho `✓ đã gửi` — dòng ấy không mang gì
/// ngoài "đã nhận". Sai cho mọi câu khác: một lời từ chối hay một báo lỗi mà
/// biến thành mặt cười là giấu đúng thứ người ta cần đọc.
#[test]
fn only_a_bare_acknowledgement_shrinks_to_an_emoji() {
    use hub::pipeline::ack_as_emoji;
    // "đã gửi" nay mang dấu RIÊNG của dự án (Hà 2026-08-14) — nhìn dấu là biết
    // chữ vừa rơi vào phiên nào; không đọc ra tên thì mới rơi về 👍.
    assert_eq!(
        ack_as_emoji("✓ đã gửi · [hub]"),
        Some(hub::pipeline::project_emoji("hub"))
    );
    assert_eq!(ack_as_emoji("✓ đã gửi · không rõ phiên"), Some("👍"));
    assert_eq!(
        ack_as_emoji("✓ đã bấm 'esc' · [hub]"),
        Some(hub::pipeline::project_emoji("hub"))
    );
    // "vào hàng chờ" là một trạng thái KHÁC (phiên đang bận), đáng dấu khác.
    assert_eq!(ack_as_emoji("✓ vào hàng chờ · [tfl5]"), Some("👌"));
    // Xác nhận thuần khác, cùng luật (Hà: *"nó đơn giản là xác nhận thôi không
    // cần thông tin"*).
    assert_eq!(ack_as_emoji("👁 Đang theo phiên [tfl5] (acc1)"), Some("👀"));
    assert_eq!(ack_as_emoji("▶ đang chạy — bash deploy.sh abc"), Some("⚡"));
    assert_eq!(ack_as_emoji("⏹ đã bảo dừng: bash deploy.sh"), Some("👌"));
    // Còn lại phải giữ nguyên chữ — chúng MANG THÔNG TIN.
    assert_eq!(ack_as_emoji("⚠ không gõ được: cửa sổ đã đóng"), None);
    assert_eq!(
        ack_as_emoji("✅ Đã chạy trên máy rồi dán kết quả vào [hub]: …"),
        None
    );
    assert_eq!(ack_as_emoji("📋 4 phiên đang sống: …"), None);
    assert_eq!(ack_as_emoji("📷 Màn của [hub]:\n…"), None);
    assert_eq!(ack_as_emoji("▶ chạy trong 4963b95c: bash deploy.sh"), None);
    assert_eq!(ack_as_emoji(""), None);
}

/// Hạng đi theo VIỆC, không theo tiến trình — và phải tự trả lại khi xong.
///
/// 🔴 Hà 2026-08-14: *"phải phân biệt việc gì cần xử lý nhanh chậm để chạy đúng
/// phân loại nhân chứ"*. Mặc định là nền (vòng quét định kỳ, đẩy ảnh chụp); chỉ
/// đường có người đang chờ mới nâng lên. Nếu guard quên trả lại thì luồng nền
/// dùng chung sẽ ở lại hạng gấp mãi mãi, và cả phép phân loại thành vô nghĩa.
#[test]
fn a_lane_belongs_to_the_work_not_to_the_process() {
    use hub::exec::{lane, urgent, Lane};
    assert_eq!(lane(), Lane::Background, "mặc định phải là nền");
    {
        let _g = urgent();
        assert_eq!(lane(), Lane::Urgent);
        {
            // Lồng nhau (một lệnh gấp gọi tiếp một khâu gấp) vẫn phải đúng.
            let _g2 = urgent();
            assert_eq!(lane(), Lane::Urgent);
        }
        assert_eq!(
            lane(),
            Lane::Urgent,
            "rời guard trong không được hạ hạng ngoài"
        );
    }
    assert_eq!(lane(), Lane::Background, "xong việc thì trả lại đường");

    // Luồng khác KHÔNG thừa hưởng: một lượt quét nền chạy song song với một cú
    // bấm thì vẫn phải là nền.
    let _g = urgent();
    let other = std::thread::spawn(lane).join().unwrap();
    assert_eq!(other, Lane::Background);
}

/// Tin gửi trong lúc hub khởi động lại KHÔNG được mất.
///
/// 🔴 Hà 2026-08-14, ngay sau một lượt cài lại: *"Vừa rồi đã dừng ko nhận được
/// tin nhắn"*. Bản cũ mở đầu bằng `getUpdates?offset=-1` — bảo Telegram "coi
/// như đã nhận hết" rồi vứt sạch phần tồn đọng. Lý do viết ra thì đúng (đừng
/// chạy lệnh gõ từ hôm qua), nhưng nó không phân biệt được một câu gõ hôm qua
/// với một câu gõ bốn giây trước, trong đúng cửa sổ hub đang khởi động lại.
///
/// Cái gác nay hỏi đúng câu nó vốn muốn hỏi — TIN NÀY GÕ LÚC NÀO — và câu đó
/// chỉ trả lời được cho tin chữ (`text_sent_at`); nút bấm không mang mốc nào,
/// nên không bị lọc theo tuổi.
#[test]
fn a_message_typed_while_hub_restarts_is_not_thrown_away() {
    use hub::telegram::text_sent_at;
    use serde_json::json;
    let now = 1_786_462_200i64;
    // Tin vừa gõ (4 giây trước) — phải có mốc để so, và mốc ấy nói "còn mới".
    let fresh = json!({ "message": { "date": now - 4, "text": "chạy test đi" } });
    assert_eq!(text_sent_at(&fresh), Some(now - 4));
    assert!(now - text_sent_at(&fresh).unwrap() < 900);
    // Tin của hôm qua — cùng phép đo ấy nói "quá cũ", nên nó bị bỏ CÓ LOG.
    let stale = json!({ "message": { "date": now - 86_400, "text": "/new dwork" } });
    assert!(now - text_sent_at(&stale).unwrap() > 900);
    // Nút bấm: không có mốc ⟹ không lọc theo tuổi (Telegram không nói lúc bấm).
    let press = json!({ "callback_query": { "id": "1", "data": "run:0",
                                            "message": { "date": now - 86_400 } } });
    assert_eq!(text_sent_at(&press), None);
}

/// Hai cách viết cùng một script chỉ đáng MỘT cái nút.
///
/// 🔴 Hà 2026-08-14, ảnh chụp một tin báo mang ba nút lệnh: *"sao lắm nút lệnh
/// thế"*. Hai trong ba là `bash ./deploy.sh` và `bash scripts/deploy.sh` — cùng
/// một việc, viết hai kiểu ở hai chỗ khác nhau trong cùng một báo cáo.
#[test]
fn two_spellings_of_one_script_are_one_button() {
    let report = "Chạy `bash ./deploy.sh` để lên bản mới.\n\
                  Chi tiết: `bash scripts/deploy.sh` đọc biến môi trường.\n\
                  Sau đó `cargo test --offline` cho chắc.\n";
    let got = hub::keys::commands_in_report(report, 4);
    let deploys: Vec<_> = got.iter().filter(|c| c.contains("deploy.sh")).collect();
    assert_eq!(deploys.len(), 1, "chỉ một nút cho một script: {got:?}");
    assert!(
        deploys[0].contains("scripts/deploy.sh"),
        "giữ bản nói rõ tệp nằm đâu: {deploys:?}"
    );
    // Lệnh KHÁC script thì vẫn giữ nguyên — dedupe không được ăn lan.
    assert!(got.iter().any(|c| c.starts_with("cargo test")), "{got:?}");
}

/// Mỗi dự án một dấu, sinh từ chính cái tên — và không bao giờ đổi.
///
/// 🔴 Hà 2026-08-14: *"tự tạo emoji theo tên dự án được không? → add vào để
/// dùng làm phản hồi"*. Cái dấu phải ỔN ĐỊNH: người ta học nó rồi, một cái dấu
/// đổi giữa chừng còn tệ hơn không có dấu (cùng bài học với nhãn màu đổi sau
/// mỗi lần khởi động).
#[test]
fn a_project_always_gets_the_same_mark() {
    use hub::pipeline::{ack_as_emoji, project_emoji};
    // Ổn định: gọi bao nhiêu lần cũng một kết quả.
    assert_eq!(project_emoji("tfl5"), project_emoji("tfl5"));
    assert_eq!(project_emoji("hub"), project_emoji("hub"));
    // Không phụ thuộc hoa/thường hay dấu ngoặc của nhãn.
    assert_eq!(project_emoji("dwork"), project_emoji("[DWork]"));
    // Tên khác nhau thì (gần như luôn) dấu khác nhau — kiểm trên bộ tên thật.
    let names = [
        "hub",
        "tfl5",
        "dwork",
        "sdvi",
        "codetrail",
        "social",
        "anpha1",
    ];
    let marks: std::collections::HashSet<_> = names.iter().map(|n| project_emoji(n)).collect();
    assert!(marks.len() >= names.len() - 1, "trùng quá nhiều: {marks:?}");
    // Tên rỗng thì rơi về dấu chung, không hoảng.
    assert_eq!(project_emoji(""), "👍");

    // …và câu "đã gửi" dùng đúng dấu của phiên nó vừa gửi vào.
    assert_eq!(
        ack_as_emoji("✓ đã gửi · 🟩 [tfl5]"),
        Some(project_emoji("tfl5"))
    );
    // Còn "vào hàng chờ" vẫn là trạng thái riêng của nó.
    assert_eq!(ack_as_emoji("✓ vào hàng chờ · 🟩 [tfl5]"), Some("👌"));
}

/// Một lời CẤM không phải một lời mời, và một lệnh phá huỷ không phải một cái nút.
///
/// 🔴 Hà 2026-08-14, ảnh chụp Telegram: *"Nút lệnh chạy ko đúng"*. Cái nút ấy
/// mời chạy một lệnh xoá trần, và hub bắt được chữ đó từ một THÔNG BÁO CHẶN của
/// hook — "the command runs … which permanently deletes tracked source files …
/// Safer form: …". Cả đoạn là lời cấm; hub đọc thành lời mời.
///
/// Hai cửa, độc lập nhau: câu văn quanh nó có phải lời cấm không (`forbids`),
/// và chính lệnh ấy có phá gì không (`destructive`). Một cửa trượt thì cửa kia
/// vẫn giữ — vì bấm nút là chạy, không có bước xác nhận nào ở giữa.
#[test]
fn a_warning_about_a_command_is_not_an_invitation_to_run_it() {
    // Nguyên văn hình dạng thông báo của hook.
    let warning = "PreToolUse:Bash hook stopped continuation: BLOCK — the command runs\n\
        git rm -q rust/src/live.rs rust/src/portal.rs which permanently deletes\n\
        tracked source files from the real repo with no backup. Safer form: first\n\
        verify the files are backed up, then run it only after confirming.\n";
    let got = hub::keys::commands_in_report(warning, 4);
    assert!(
        got.is_empty(),
        "không được dựng nút nào từ một lời cấm: {got:?}"
    );

    // …và ngay cả khi câu văn quanh nó hoàn toàn trung tính, lệnh phá huỷ vẫn
    // không thành nút.
    let neutral = "Bước tiếp theo trong quy trình dọn dẹp kho mã của dự án:\n\
                   git rm rust/src/live.rs\n\
                   Sau đó chạy lại toàn bộ bài kiểm để chắc chắn mọi thứ ổn.\n";
    assert!(
        !hub::keys::commands_in_report(neutral, 4)
            .iter()
            .any(|c| c.contains("git rm")),
        "lệnh xoá không bao giờ là một cái nút"
    );
    // Lệnh lành thì vẫn ra nút như thường — cửa không được ăn lan.
    let ok = "Chạy lại bộ kiểm tra cho chắc trước khi đóng sổ nhé bạn:\n\
              cargo test --offline\n\
              Rồi báo lại kết quả giúp tôi.\n";
    assert!(
        hub::keys::commands_in_report(ok, 4)
            .iter()
            .any(|c| c.starts_with("cargo test")),
        "{:?}",
        hub::keys::commands_in_report(ok, 4)
    );
}

/// 🔴 CỔNG NGƯỜI — nay là cổng DUY NHẤT của hub (2026-08-14).
///
/// Tới sáng nay còn hai lớp: `chat_id` ở kênh, và `trust.tfl5_user_tids` kiểm
/// trong `verbs::parse_command`. Lớp thứ hai đi cùng phòng chat tfl5, và nó
/// đáng đi — chỗ gọi phải tự bịa ra "người gõ" (lấy `first()` của danh sách chủ
/// rồi đem so với chính danh sách ấy) nên nó không bao giờ từ chối được, TRỪ khi
/// danh sách rỗng, và khi ấy nó nuốt sạch mọi mệnh lệnh trong im lặng.
///
/// Nên bài kiểm này thay chỗ cho năm bài "người lạ gõ vẫn chỉ là chữ" từng nằm ở
/// `tests/verbs.rs`. Nó canh thứ mà một phép so `a == b` không canh nổi: **đọc
/// đúng ô nào trong JSON**. Trong buồng riêng, `message.chat.id` và
/// `message.from.id` BẰNG NHAU — nên lẫn hai ô ấy thì mọi thử nghiệm tay đều
/// xanh, và chỉ lộ ra khi bot bị kéo vào một nhóm.
#[test]
fn a_message_from_another_chat_is_not_an_order() {
    use hub::telegram::update_sender;
    use serde_json::json;

    const OWNER: &str = "8110";

    // CHỮ: hỏi BUỒNG. `from.id` ở đây là người gõ trong nhóm — đọc nhầm sang nó
    // là mở cổng cho cả nhóm.
    let in_group = json!({
        "message": { "chat": { "id": 4242 }, "from": { "id": 8110 }, "text": "/stop" }
    });
    assert_eq!(
        update_sender(&in_group).as_deref(),
        Some("4242"),
        "chữ phải gác theo buồng (chat.id), không theo người gõ (from.id)"
    );
    assert_ne!(update_sender(&in_group).as_deref(), Some(OWNER));

    // Buồng riêng của chủ máy: hai ô bằng nhau, và lệnh đi qua.
    let mine = json!({
        "message": { "chat": { "id": 8110 }, "from": { "id": 8110 }, "text": "/stop" }
    });
    assert_eq!(update_sender(&mine).as_deref(), Some(OWNER));

    // Tin đã sửa cũng là tin — nếu không, sửa một dòng cũ thành `/stop` là một
    // đường vòng qua cổng.
    let edited = json!({
        "edited_message": { "chat": { "id": 4242 }, "text": "/stop" }
    });
    assert_eq!(update_sender(&edited).as_deref(), Some("4242"));

    // NÚT: hỏi NGƯỜI BẤM. Bất đối xứng có chủ ý — xem `update_sender`.
    let stranger_pressed = json!({
        "callback_query": { "id": "c1", "from": { "id": 4242 }, "data": "key:enter" }
    });
    assert_eq!(update_sender(&stranger_pressed).as_deref(), Some("4242"));
    assert_ne!(update_sender(&stranger_pressed).as_deref(), Some(OWNER));

    // Hình dạng lạ ⟹ None ⟹ chỗ gọi từ chối. Fail CLOSED: một update không đọc
    // ra được người gửi không được coi như của chủ máy.
    for weird in [
        json!({}),
        json!({ "message": { "text": "/stop" } }),
        json!({ "channel_post": { "chat": { "id": 8110 }, "text": "/stop" } }),
        json!({ "callback_query": { "id": "c1", "data": "key:enter" } }),
    ] {
        assert_eq!(update_sender(&weird), None, "phải từ chối: {weird}");
    }
}
