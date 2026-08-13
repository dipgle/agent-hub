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
        sess("7c2ae1a7-1111-2222-3333-444455556666", "dwork", "acc1", false),
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
    assert!(!text.contains("-3050-"), "id đầy đủ chiếm chỗ vô ích: {text}");
}

/// BA tình trạng, không phải hai — và dự án phải đọc được.
///
/// Hà 2026-08-12: *"phải thêm tình trạng đang xử lý, đã dừng"* + *"phải thêm
/// thông tin để biết phiên đang làm dự án hay thư mục nào"*.
#[test]
fn the_list_tells_running_waiting_and_stopped_apart() {
    let mut a = sess("aaaaaaaa-0000-0000-0000-000000000000", "projects-ff", "acc3", true);
    a.folder = "AI/hub".into();
    let mut b = sess("bbbbbbbb-0000-0000-0000-000000000000", "projects-11", "acc1", false);
    b.folder = "dwork".into();
    let mut c = sess("cccccccc-0000-0000-0000-000000000000", "Tự chạy lại", "acc3", false);
    c.host = "dead".into();
    let text = session_list_text(&[a, b, c], "", NOW);
    // Nhãn là ĐƯỜNG DỰ ÁN, không kèm tên tự sinh (Hà 2026-08-13: *"cần gì đoạn
    // text project-xx làm gì"*).
    assert!(text.contains("[AI/hub]"), "{text}");
    assert!(text.contains("[dwork]"), "{text}");
    assert!(!text.contains("projects-ff"), "tên tự sinh vẫn còn chiếm chỗ: {text}");
    // Chấm TRẠNG THÁI, không phải ký hiệu điều khiển của máy phát nhạc — Hà
    // 2026-08-13: *"icon biểu diễn chạy và dừng bị ngược"*. `▶` nay chỉ còn
    // nghĩa "bấm để chạy lệnh này" (`remember_quick`), một ký hiệu một nghĩa.
    assert!(text.contains("🟢 đang chạy"), "{text}");
    assert!(text.contains("🟡 đứng chờ"), "{text}");
    assert!(text.contains("⚫ đã tắt"), "phiên đã tắt bị gộp vào 'đứng chờ': {text}");
    assert!(!text.contains("▶"), "`▶` phải để dành cho NÚT chạy lệnh: {text}");
}

/// Phiên đang HỎI phải đọc ra được cả câu hỏi lẫn từng lựa chọn.
///
/// Hà 2026-08-12: *"có 1 phiên đang đưa lựa chọn nhưng không nhận được trên
/// tele"*. Trên danh sách, trạng thái này nhìn y hệt "đứng chờ" nếu không nói ra
/// — mà nó là trạng thái DUY NHẤT cần người đọc làm gì đó thì việc mới đi tiếp.
#[test]
fn a_session_waiting_for_an_answer_shows_the_question_and_the_options() {
    let mut s = sess("aaaaaaaa-0000-0000-0000-000000000000", "projects-11", "acc1", false);
    s.folder = "dwork".into();
    s.last_text = Some("[dùng AskUserQuestion]".into());
    s.asking = Some(hub::sessions::Asking {
        header: "Nửa ngày".into(),
        question: "Đơn vắng có khai được NỬA NGÀY không?".into(),
        options: vec!["Thêm ô nửa ngày".into(), "Luôn trọn ngày".into()],
    });
    let text = session_list_text(&[s], "", NOW);
    assert!(text.contains("⚠ dừng lại HỎI"), "tình trạng: {text}");
    assert!(text.contains("Nửa ngày"), "nhãn câu hỏi: {text}");
    assert!(text.contains("NỬA NGÀY không?"), "nguyên văn câu hỏi: {text}");
    assert!(text.contains("1. Thêm ô nửa ngày"), "lựa chọn có SỐ để bấm: {text}");
    assert!(text.contains("2. Luôn trọn ngày"), "{text}");
    // Câu cuối ("[dùng AskUserQuestion]") không nói thêm được gì khi đã có câu
    // hỏi thật — nó chỉ đẩy phiên sau ra khỏi màn.
    assert!(!text.contains("💬"), "câu cuối chen vào chỗ của câu hỏi: {text}");
}

/// Phiên đang theo phải nhận ra được: mọi lệnh KHÔNG mang id sẽ rơi vào nó.
#[test]
fn the_followed_session_is_marked() {
    let live = vec![
        sess("aaaaaaaa-0000-0000-0000-000000000000", "hub", "acc3", true),
        sess("bbbbbbbb-0000-0000-0000-000000000000", "dwork", "acc1", false),
    ];
    let text = session_list_text(&live, "bbbbbbbb-0000-0000-0000-000000000000", NOW);
    let followed = text
        .lines()
        .find(|l| l.contains("dwork"))
        .unwrap_or_default();
    assert!(followed.starts_with("👁"), "dòng đang theo không có dấu: {text}");
    let other = text.lines().find(|l| l.contains("hub")).unwrap_or_default();
    assert!(!other.starts_with("👁"), "đánh dấu nhầm phiên: {text}");
}

/// Con trỏ trỏ vào một phiên đã chết vẫn phải nói ra.
///
/// Im ở đây là kiểu im nguy hiểm: `/tell`, `/ask`, `/type` không mang id vẫn
/// đang nhắm vào cái phiên không còn nữa, và người đọc tưởng mình chưa chọn gì.
#[test]
fn a_focus_that_no_longer_exists_is_said_out_loud() {
    let live = vec![sess("aaaaaaaa-0000-0000-0000-000000000000", "hub", "acc3", true)];
    let text = session_list_text(&live, "deadbeef-0000-0000-0000-000000000000", NOW);
    assert!(text.contains("deadbeef"), "{text}");
    assert!(text.contains("không còn sống"), "{text}");
}

/// Cắt bớt thì phải NÓI, không thì danh sách nói dối về số phiên đang chạy.
#[test]
fn a_truncated_list_says_how_many_it_hid() {
    let live: Vec<LiveSession> = (0..MAX_SESSION_BUTTONS + 3)
        .map(|i| sess(&format!("{i:08}-0000-0000-0000-000000000000"), &format!("p{i}"), "acc1", false))
        .collect();
    let text = session_list_text(&live, "", NOW);
    assert!(
        text.contains(&format!("{} phiên đang sống", MAX_SESSION_BUTTONS + 3)),
        "tổng số phải đúng: {text}"
    );
    assert!(text.contains("còn 3 phiên nữa"), "cắt mà im: {text}");
    assert!(text.contains("/session <id>"), "cắt rồi phải chỉ đường khác: {text}");
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
    assert!(text.contains("💬 Đã sửa xong phần redaction"), "câu cuối: {text}");
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
    assert!(!text.contains("im 12 phút"), "phiên đang chạy mà bảo im: {text}");
}

/// Chọn phiên xong thì CHỮ THƯỜNG là chữ gõ vào phiên ấy.
///
/// Hà 2026-08-11: *"bấm vào mỗi phiên focus vào phiên đó luôn"* — chọn xong coi
/// như đang ngồi trong phiên, không phải nhớ thêm một động từ trước mỗi câu.
#[test]
fn plain_text_is_something_to_type_into_the_session() {
    assert_eq!(text_for_session("chạy test đi"), Some("chạy test đi"));
    assert_eq!(text_for_session("  có lỗi gì không?  "), Some("có lỗi gì không?"));
    assert_eq!(text_for_session("2"), Some("2"), "một con số cũng là câu trả lời");
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
    assert!(hub::keys::still_in_box(screen, typed), "so nguyên văn thì trượt");
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
    let b = hub::telegram::choice_buttons(SID, &["Tôi tiếp".to_string()], true);
    let (label, data) = b.last().expect("thiếu nút vào phiên");
    assert!(label.contains("Vào phiên"), "nhãn khó hiểu: {label}");
    // Round-trip qua CHÍNH bộ giải mã đang chạy — nút gửi đi mà không giải ra
    // được lệnh nào thì nó chỉ là một hình vẽ.
    assert_eq!(
        hub::telegram::callback_to_command(data).as_deref(),
        Some(format!("/session {SID}").as_str())
    );
    // …và vẫn dưới trần 64 byte của Telegram.
    assert!(data.len() <= 64, "callback_data {} byte: {data}", data.len());
}

/// Phiên ĐANG theo thì không có nút ấy — bấm vào chỉ để tới chỗ đang đứng.
#[test]
fn the_followed_session_gets_no_redundant_button() {
    let b = hub::telegram::choice_buttons(SID, &["Tôi tiếp".to_string()], false);
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
    assert!(!label.contains("hub-67"), "nhãn gọi tên phiên đã chết: {label}");
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
    assert_eq!(hub::pipeline::session_name_from_book("khong-phai-json", SID), None);
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
    let b = hub::telegram::choice_buttons(SID, &labels, true);
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
    assert!(due.is_empty(), "gọi xoá một tin Telegram không cho xoá: {due:?}");
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
    assert!(hub::keys::commands_on_screen(screen, 4).is_empty(), "{:?}", hub::keys::commands_on_screen(screen, 4));
}

/// Dấu nhắc shell đứng trước lệnh phải bị bóc, và lệnh trần (không tham số) bỏ qua.
#[test]
fn prompts_are_stripped_and_bare_verbs_ignored() {
    // `cargo` trần: đủ dài để qua cửa độ dài, nên nó ghim ĐÚNG luật "phải có
    // tham số" chứ không ăn theo một luật khác.
    let screen = "$ cargo test --offline\n❯ cargo\n  ./deploy/install.sh --no-build";
    let got = hub::keys::commands_on_screen(screen, 4);
    assert!(got.contains(&"cargo test --offline".to_string()), "{got:?}");
    assert!(got.contains(&"./deploy/install.sh --no-build".to_string()), "{got:?}");
    assert!(!got.iter().any(|c| c == "cargo"), "lệnh trần vẫn lọt: {got:?}");
}

/// Giữ các dòng CUỐI (mới nhất) và bỏ trùng.
#[test]
fn only_the_latest_few_survive_and_duplicates_collapse() {
    let screen = "git status\ngit status\nnpm run build\ncargo test --offline\nnode fe-smoke.mjs a b c";
    let got = hub::keys::commands_on_screen(screen, 2);
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[1], "node fe-smoke.mjs a b c", "phải giữ dòng mới nhất: {got:?}");
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
    assert!(got.contains(&"bash ./deploy.sh perapp-storage".to_string()), "{got:?}");
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
