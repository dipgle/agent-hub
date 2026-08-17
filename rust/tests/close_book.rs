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
use hub::pipeline::{close_step, CloseStep};

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
