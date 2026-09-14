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
//! `Err` ở đây có nghĩa là *huba mù*, và luật của huba với cái mù là GIỮ NGUYÊN
//! trong sổ. Luật không sai; phép đo mới sai — nó không biết nói "không còn".
//!
//! Cùng một họ với `keys::look` gộp ba kết cục vào `None`, và với
//! `keys::window_gone` phải đo bằng số tab + `visible` thay vì `id of every
//! window`: **một câu hỏi, một phép đo, và phép đo phải trỏ đúng chỗ.**

use huba::keys::TabState;
use huba::pipeline::{close_step, hidden_next, CloseStep, HiddenNext};

/// Trần bỏ cuộc trong `pipeline` là 600 giây; bài kiểm không đọc được hằng số
/// riêng tư nên nó dùng hai mốc nằm hẳn hai bên (5 phút · 20 phút).
const UNDER: i64 = 300;
const OVER: i64 = 1200;

/// `/exit` ĐÃ được gõ — trạng thái của mọi mục vào sổ qua route `/close`, và là
/// tiền đề ngầm của cả tệp này trước 13/09.
const DA_GO: bool = true;
/// Sổ CHƯA thấy `/exit` đi lần nào — ca của nhánh bàn giao khi `quit_and_close`
/// chết ngay ở `send_exit`.
const CHUA_GO: bool = false;

/// Cái đã hỏng: cửa sổ không còn thì việc XONG — đóng sổ, không hỏi lại nữa.
#[test]
fn a_window_that_is_gone_closes_the_book() {
    assert_eq!(close_step(Some(TabState::Gone), 30, DA_GO), CloseStep::Gone);
    // Và không đổi ý theo thời gian chờ: "không còn" là một sự thật, không phải
    // một sự kiên nhẫn.
    assert_eq!(
        close_step(Some(TabState::Gone), OVER, DA_GO),
        CloseStep::Gone
    );
    // Cũng không đổi ý theo việc đã gõ `/exit` hay chưa: cửa sổ không còn thì
    // gõ vào đâu.
    assert_eq!(
        close_step(Some(TabState::Gone), 30, CHUA_GO),
        CloseStep::Gone
    );
}

/// Không hỏi được thì GIỮ trong sổ — luật `Look::Blind`, và nó không được đổi
/// chiều nhân lượt sửa này.
#[test]
fn a_blind_check_keeps_the_entry() {
    assert_eq!(close_step(None, 30, DA_GO), CloseStep::Blind);
    assert_eq!(close_step(None, UNDER, DA_GO), CloseStep::Blind);
    // Mù thì KHÔNG gõ bừa: `SendExit` phải đứng sau một câu trả lời `Busy` thật,
    // không phải sau một lượt hỏi hỏng. Gõ vào chỗ mình không nhìn thấy là đúng
    // thứ luật `Look::Blind` cấm.
    assert_eq!(close_step(None, UNDER, CHUA_GO), CloseStep::Blind);
}

/// Giữ mãi mà im chính là 190 dòng warn kia. Mù quá trần thì nói một câu rồi
/// buông — cùng trần với "còn bận quá lâu", vì cùng một lý lẽ.
#[test]
fn blind_forever_is_not_an_answer_either() {
    assert_eq!(close_step(None, OVER, DA_GO), CloseStep::GiveUpBlind);
    assert_eq!(close_step(None, OVER, CHUA_GO), CloseStep::GiveUpBlind);
}

/// Rảnh thì đóng. Đây là đường thường ngày, và nó không được lẫn với ba đường kia.
#[test]
fn an_idle_tab_gets_closed() {
    assert_eq!(close_step(Some(TabState::Idle), 0, DA_GO), CloseStep::Close);
    assert_eq!(
        close_step(Some(TabState::Idle), OVER, DA_GO),
        CloseStep::Close
    );
    // 🔴 `Idle` THẮNG `chưa gõ`: tab đã rảnh thì việc cần làm là đóng, không
    // phải gõ thêm chữ vào một cửa sổ sắp biến mất. Nếu ai đó đảo thứ tự hai
    // nhánh trong `close_step`, chỗ này đỏ.
    assert_eq!(
        close_step(Some(TabState::Idle), 0, CHUA_GO),
        CloseStep::Close
    );
}

/// Còn bận thì chờ — và chỉ tới trần, vì `/exit` gõ vào một phiên đang chạy có
/// thể nằm trong hàng chờ của TUI mãi mãi.
#[test]
fn a_busy_tab_waits_then_gives_up_out_loud() {
    assert_eq!(
        close_step(Some(TabState::Busy), UNDER, DA_GO),
        CloseStep::Wait
    );
    assert_eq!(
        close_step(Some(TabState::Busy), OVER, DA_GO),
        CloseStep::GiveUpBusy
    );
}

// ── Sổ đóng phải biết GÕ, không chỉ biết CHỜ ────────────────────────────────
//
// 🔴 Hà 2026-09-13, ảnh danh sách: *"Vẫn còn hiện tượng chuyển phiên nhưng phiên
// cũ vẫn không đóng được"*. Chữ **vẫn** là điểm chính — cùng triệu chứng đã nghe
// 19/08, và lượt ấy vá "tab còn bận có bốn nghĩa", không phải gốc.
//
// Gốc đo được trên phiên `53a0683e`: `handover_old_window_not_closed` —
// *"vòng nền đã tiêu hết ngân sách hỏi Terminal (11.9s/10.0s)"* ⇒ `send_exit`
// chết trước khi chạm bàn phím. Trong **cả 12** dòng `keys_exit_sent` của 30 MB
// nhật ký có `451` và `455`, **không có `452`**. Sổ ngồi đợi một chữ `Idle`
// không thể tới, `close_gave_up waited_sec=713`, mục rời sổ, `pid 14131` sống
// tiếp hơn 12 tiếng.

/// Khoá hồi quy cho đúng ca ấy: bận + sổ chưa thấy `/exit` ⇒ GÕ, đừng chờ.
#[test]
fn a_busy_tab_that_never_got_exit_gets_one() {
    assert_eq!(
        close_step(Some(TabState::Busy), UNDER, CHUA_GO),
        CloseStep::SendExit
    );
    // Ngay lượt hỏi đầu tiên, không phải sau một hạn kiên nhẫn nào: chờ một việc
    // chưa ai bắt đầu thì chờ bao lâu cũng thế.
    assert_eq!(
        close_step(Some(TabState::Busy), 0, CHUA_GO),
        CloseStep::SendExit
    );
}

/// Và nó KHÔNG gõ mãi: quá trần thì vẫn buông, không thử lại vô tận.
///
/// Ca này là cái phanh của ca trên. Một nhánh "cứ gõ khi chưa gõ được" mà không
/// có trần thì mỗi 30 giây một lượt `osascript` vào một cửa sổ không bao giờ
/// nhận — đúng hình dạng 190 dòng warn mà tệp này sinh ra để chặn, chỉ đổi tên.
#[test]
fn it_does_not_type_forever() {
    assert_eq!(
        close_step(Some(TabState::Busy), OVER, CHUA_GO),
        CloseStep::GiveUpBusy
    );
}

/// Đã gõ rồi thì THÔI gõ — nếu không, mỗi 30 giây một `/exit` nữa vào cùng một
/// TUI, và cái thứ hai rơi xuống shell sau khi cái thứ nhất ăn.
#[test]
fn a_tab_that_already_got_exit_is_not_typed_into_again() {
    assert_eq!(
        close_step(Some(TabState::Busy), UNDER, DA_GO),
        CloseStep::Wait
    );
}

// ── Cửa sổ ẩn: đo được rằng lời từ chối là NHẤT THỜI ────────────────────────
//
// 17/08 lúc 10:20Z, năm cửa sổ từ chối `close` (chạy êm, trả 0, cửa sổ đứng
// nguyên) nên huba ẩn chúng đi. Gần bốn tiếng sau, gọi tay lên ĐÚNG những cửa sổ
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
/// thử đầu, mọi lượt sau đều "tới hạn" và huba thử lại mỗi vòng chạy.
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
    let book: std::collections::BTreeMap<String, huba::pipeline::Closing> =
        serde_json::from_str(old).expect("sổ cũ phải đọc được bằng mã mới");
    let c = &book["win-ttys002"];
    assert_eq!(c.w, 2131);
    assert_eq!(c.t, 1786955814);
    // Mục cũ = "chưa từng bị ẩn", nên nó đi đường thường, không rơi vào vòng
    // thử-lại với một mốc thời gian bịa ra.
    assert_eq!(c.h, 0);
    assert_eq!(c.r, 0);
    assert_eq!(hidden_next(c.h, c.r, 1_800_000_000), HiddenNext::NotHidden);
    // 🔴 Và mục cũ = "sổ CHƯA thấy `/exit` đi" — đây là mặc định fail-closed về
    // phía LÀM. Một mục viết trước lượt vá này không mang `x`, nên nếu `serde`
    // mặc định nó thành "đã gõ" thì đúng những cửa sổ đang kẹt lúc nâng cấp sẽ
    // kẹt tiếp, im lặng. `0` khiến sổ gõ hộ ngay lượt hỏi đầu.
    assert_eq!(c.x, 0);
}

/// Các kết cục PHẢI phân biệt được nhau. Bài kiểm này tồn tại vì lỗi 17/08 đúng
/// là hai kết cục bị gộp làm một: nếu ai đó gộp lại lần nữa cho gọn, chỗ này đỏ
/// trước khi cửa sổ nào kẹt trong sổ 5 tiếng.
///
/// 13/09 thêm một kết cục thứ năm (`SendExit`) và một chiều thứ hai (đã gõ
/// `/exit` hay chưa) — nên bảng phải phủ CẢ HAI chiều, không chỉ `TabState`.
/// Một ma trận chỉ quét một chiều đọc ra như đã phủ hết trong khi nửa kia chưa
/// ai chạm tới.
#[test]
fn the_outcomes_stay_distinct() {
    let all = [
        close_step(Some(TabState::Gone), 30, DA_GO),
        close_step(Some(TabState::Idle), 30, DA_GO),
        close_step(Some(TabState::Busy), 30, DA_GO),
        close_step(Some(TabState::Busy), 30, CHUA_GO),
        close_step(Some(TabState::Busy), OVER, DA_GO),
        close_step(None, 30, DA_GO),
        close_step(None, OVER, DA_GO),
    ];
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            assert_ne!(a, b, "hai kết cục khác nhau lại ra cùng một nước đi");
        }
    }
    // MẪU SỐ: bảy ô trên phải là bảy nước đi khác nhau, tức đúng bằng số biến
    // thể của `CloseStep`. Thiếu một ô thì vòng lặp trên vẫn xanh mà chiều kia
    // không ai đo — nên khai thẳng con số.
    assert_eq!(all.len(), 7, "bảng phủ thiếu một kết cục");
}
