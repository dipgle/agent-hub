//! Sổ chờ đóng: "hỏi không được" và "không còn cửa sổ" là HAI chuyện.
//!
//! 🔴 Đo 2026-08-17 trên `~/Library/Logs/hubd.err`: **190 dòng
//! `close_check_failed` trong 5 tiếng** (08:44:50Z → 13:47:06Z), tất cả về đúng
//! một cửa sổ — `window 2131`, phiên `win-ttys002` — và tất cả cùng một câu:
//!
//! ```text
//! Can't make «class busy» of «class tcnt» of window id 2131
//! of application "Terminal" into type text. (-1700)
//! ```
//!
//! Cửa sổ ấy đã đóng từ lâu. `selected tab` của một cửa sổ không còn tab trả về
//! `missing value`, ép sang chữ thì -1700 — nên phép đo cũ (`tab_busy ->
//! Result<bool>`) chỉ có một chỗ để đặt sự thật "không còn": nhánh `Err`. Mà
//! `Err` ở đây có nghĩa là *hub mù*, và luật của hub với cái mù là GIỮ NGUYÊN
//! trong sổ. Luật không sai; phép đo mới sai — nó không biết nói "không còn".
//!
//! Cùng một họ với `keys::look` gộp ba kết cục vào `None`, và với
//! `keys::window_gone` phải đo bằng số tab + `visible` thay vì `id of every
//! window`: **một câu hỏi, một phép đo, và phép đo phải trỏ đúng chỗ.**

use hub::keys::TabState;
use hub::pipeline::{close_step, hidden_next, CloseStep, HiddenNext};

/// Trần bỏ cuộc trong `pipeline` là 600 giây; bài kiểm không đọc được hằng số
/// riêng tư nên nó dùng hai mốc nằm hẳn hai bên (5 phút · 20 phút).
const UNDER: i64 = 300;
const OVER: i64 = 1200;

/// Cái đã hỏng: cửa sổ không còn thì việc XONG — đóng sổ, không hỏi lại nữa.
#[test]
fn a_window_that_is_gone_closes_the_book() {
    assert_eq!(close_step(Some(TabState::Gone), 30), CloseStep::Gone);
    // Và không đổi ý theo thời gian chờ: "không còn" là một sự thật, không phải
    // một sự kiên nhẫn.
    assert_eq!(close_step(Some(TabState::Gone), OVER), CloseStep::Gone);
}

/// Không hỏi được thì GIỮ trong sổ — luật `Look::Blind`, và nó không được đổi
/// chiều nhân lượt sửa này.
#[test]
fn a_blind_check_keeps_the_entry() {
    assert_eq!(close_step(None, 30), CloseStep::Blind);
    assert_eq!(close_step(None, UNDER), CloseStep::Blind);
}

/// Giữ mãi mà im chính là 190 dòng warn kia. Mù quá trần thì nói một câu rồi
/// buông — cùng trần với "còn bận quá lâu", vì cùng một lý lẽ.
#[test]
fn blind_forever_is_not_an_answer_either() {
    assert_eq!(close_step(None, OVER), CloseStep::GiveUpBlind);
}

/// Rảnh thì đóng. Đây là đường thường ngày, và nó không được lẫn với ba đường kia.
#[test]
fn an_idle_tab_gets_closed() {
    assert_eq!(close_step(Some(TabState::Idle), 0), CloseStep::Close);
    assert_eq!(close_step(Some(TabState::Idle), OVER), CloseStep::Close);
}

/// Còn bận thì chờ — và chỉ tới trần, vì `/exit` gõ vào một phiên đang chạy có
/// thể nằm trong hàng chờ của TUI mãi mãi.
#[test]
fn a_busy_tab_waits_then_gives_up_out_loud() {
    assert_eq!(close_step(Some(TabState::Busy), UNDER), CloseStep::Wait);
    assert_eq!(
        close_step(Some(TabState::Busy), OVER),
        CloseStep::GiveUpBusy
    );
}

// ── Cửa sổ ẩn: đo được rằng lời từ chối là NHẤT THỜI ────────────────────────
//
// 17/08 lúc 10:20Z, năm cửa sổ từ chối `close` (chạy êm, trả 0, cửa sổ đứng
// nguyên) nên hub ẩn chúng đi. Gần bốn tiếng sau, gọi tay lên ĐÚNG những cửa sổ
// ấy, ĐÚNG lệnh ấy, khi chúng vẫn đang ẩn: `2151` · `2153` · `2156` đều đóng
// ngay lượt đầu (`1/false` → `0/false`). Phép thử A/B ấy bác luôn giả thuyết
// "cửa sổ ẩn không nhận close" mà tôi vừa nêu ra trước đó — nên cái đúng để làm
// là THỬ LẠI, và mục phải ở lại trong sổ thì mới có ai quay lại.

/// Mốc thời gian trong `pipeline`: thử lại mỗi 300 giây, bỏ cuộc sau 6 tiếng.
const RETRY_SEC: i64 = 300;
const GIVE_UP_SEC: i64 = 6 * 3600;
const HID: i64 = 1_000_000;

/// Vừa ẩn xong thì chưa thử ngay — nhịp thưa là có chủ ý, cửa sổ rác không có
/// ai đang chờ.
#[test]
fn a_freshly_hidden_window_waits_out_the_first_gap() {
    assert_eq!(hidden_next(HID, 0, HID + RETRY_SEC - 1), HiddenNext::Wait);
    assert_eq!(hidden_next(HID, 0, HID + RETRY_SEC), HiddenNext::Retry);
}

/// Nhịp đếm từ lần thử GẦN NHẤT, không phải từ lúc ẩn — nếu không thì sau lần
/// thử đầu, mọi lượt sau đều "tới hạn" và hub thử lại mỗi vòng chạy.
#[test]
fn the_gap_is_measured_from_the_last_attempt() {
    let r = HID + 500;
    assert_eq!(hidden_next(HID, r, r + RETRY_SEC - 1), HiddenNext::Wait);
    assert_eq!(hidden_next(HID, r, r + RETRY_SEC), HiddenNext::Retry);
}

/// Thử mãi cũng phải có hạn, và hạn ấy THẮNG nhịp thử lại — hết giờ thì nói một
/// câu rồi buông, không im lặng thử tới vô tận (đúng cái vừa phải sửa ở nhánh mù).
#[test]
fn after_the_ceiling_it_gives_up_out_loud() {
    let now = HID + GIVE_UP_SEC;
    assert_eq!(hidden_next(HID, now - RETRY_SEC, now), HiddenNext::GiveUp);
    // Ngay cả khi vừa thử xong một giây trước.
    assert_eq!(hidden_next(HID, now - 1, now), HiddenNext::GiveUp);
}

/// Mục chưa từng bị ẩn không phải việc của hàm này — nó phải nói ra thế, chứ
/// không lặng lẽ trả `Wait` (một mục thường mà rơi vào đường ẩn thì đứng mãi).
#[test]
fn an_entry_that_was_never_hidden_says_so() {
    assert_eq!(hidden_next(0, 0, HID), HiddenNext::NotHidden);
}

/// Sổ CŨ (chưa có `h`/`r`) phải đọc được bằng mã mới. Không phải chuyện lý
/// thuyết: đúng lúc nâng cấp, trong DB đang có một mục thật —
/// `{"win-ttys002":{"w":2131,…}}` — và một mục không đọc được là một cửa sổ
/// KHÔNG AI quay lại đóng, im lặng. Cùng luật "hợp đồng và người dùng hợp đồng
/// đi chung một commit".
#[test]
fn an_old_book_row_still_parses() {
    let old = r#"{"win-ttys002":{"w":2131,"n":"⬜ cửa sổ ttys002","t":1786955814,"c":1786975251}}"#;
    let book: std::collections::BTreeMap<String, hub::pipeline::Closing> =
        serde_json::from_str(old).expect("sổ cũ phải đọc được bằng mã mới");
    let c = &book["win-ttys002"];
    assert_eq!(c.w, 2131);
    assert_eq!(c.t, 1786955814);
    // Mục cũ = "chưa từng bị ẩn", nên nó đi đường thường, không rơi vào vòng
    // thử-lại với một mốc thời gian bịa ra.
    assert_eq!(c.h, 0);
    assert_eq!(c.r, 0);
    assert_eq!(hidden_next(c.h, c.r, 1_800_000_000), HiddenNext::NotHidden);
}

/// Bốn kết cục PHẢI phân biệt được nhau. Bài kiểm này tồn tại vì lỗi vừa sửa
/// đúng là hai kết cục bị gộp làm một: nếu ai đó gộp lại lần nữa cho gọn, chỗ
/// này đỏ trước khi cửa sổ nào kẹt trong sổ 5 tiếng.
#[test]
fn the_four_outcomes_are_four() {
    let all = [
        close_step(Some(TabState::Gone), 30),
        close_step(Some(TabState::Idle), 30),
        close_step(Some(TabState::Busy), 30),
        close_step(None, 30),
    ];
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            assert_ne!(a, b, "hai kết cục khác nhau lại ra cùng một nước đi");
        }
    }
}
