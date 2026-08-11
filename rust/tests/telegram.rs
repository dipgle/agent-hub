//! Telegram làm KÊNH RA LỆNH: cái nút và cái danh sách.
//!
//! Hai thứ ở đây đều thuần (không cần mạng, không cần Telegram) vì đó chính là
//! chỗ dễ sai mà lại không ai nhìn thấy: một `callback_data` sai hình dạng thì
//! cú bấm rơi vào hư không, còn một dòng danh sách thiếu id thì người đọc không
//! gõ tiếp được lệnh nào.

use hub::pipeline::{session_button_label, session_list_text, MAX_SESSION_BUTTONS};
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
    assert!(text.contains("▶"), "không nói phiên nào đang chạy: {text}");
    assert!(text.contains("⏸"), "không nói phiên nào đứng chờ: {text}");
    assert!(
        text.contains("Chưa theo phiên nào"),
        "chưa theo phiên nào thì phải nói ra: {text}"
    );
    // Id ĐẦY ĐỦ không lên màn: nó dài gấp bốn lần chỗ nó chiếm mà không thêm
    // một thông tin nào — `claude stop` và `/session` đều nhận id ngắn.
    assert!(!text.contains("-3050-"), "id đầy đủ chiếm chỗ vô ích: {text}");
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

/// Nhãn nút gọn hơn dòng chữ nhưng vẫn phải nói phiên nào + có chạy không.
#[test]
fn a_button_label_still_answers_which_and_whether() {
    let s = sess("aaaaaaaa-0000-0000-0000-000000000000", "hub", "acc3", true);
    let label = session_button_label(&s);
    assert!(label.starts_with('▶'), "{label}");
    assert!(label.contains("hub") && label.contains("acc3"), "{label}");
    let idle = session_button_label(&sess("b", "dwork", "acc1", false));
    assert!(idle.starts_with('⏸'), "{idle}");
}
