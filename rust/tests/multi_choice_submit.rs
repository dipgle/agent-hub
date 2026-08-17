//! Gửi một hộp CHỌN NHIỀU: Enter trần không bao giờ qua được.
//!
//! 🔴 Hà 2026-08-17, sau khi bấm đủ bốn lựa chọn rồi `/send_…`: *"Ko qua nổi màn
//! này"*. Màn thật (ảnh anh gửi) có dạng dưới đây — `Submit` là một dòng RIÊNG,
//! không mang số, nên bấm số không tới được; còn Enter thì tác động lên đúng
//! dòng con trỏ đang đứng, tức bật/tắt lại chính cái ô vừa chọn.

use hub::keys::submit_keys;
use hub::pipeline::{render_session_data, SessionData};

/// ✅ phải bám ngay tại dòng `Submit` — nếu không, hộp chọn nhiều không có
/// đường gửi nào trong chữ.
///
/// 🔴 Hà 2026-08-17, sau khi ☑ đã bám đúng từng dòng: *"Bấm chọn được rồi, chưa
/// bấm được submit"*. `Submit` là dòng THẬT trên màn nhưng không mang số, nên
/// không `k_`/`pick_` nào trỏ tới nó.
#[test]
fn the_submit_line_gets_its_own_tap_target() {
    hub::telegram::set_bot_username("hub_test_bot");
    let shown = render_session_data(
        SCREEN,
        &SessionData {
            sid: "8bf82c37-f88f-4e71-95a0-7810c07623cd".to_string(),
            submit: true,
            ..Default::default()
        },
    );
    let line = shown
        .lines()
        .find(|l| l.contains("Submit"))
        .expect("phải còn dòng Submit");
    assert!(
        line.contains("send_8bf82c37"),
        "phải trỏ đúng route: {line}"
    );
    assert!(
        line.find('\u{2705}').unwrap() < line.find("Submit").unwrap(),
        "✅ đứng TRƯỚC chữ Submit như ☑ đứng trước số: {line}"
    );
}

/// Màn chụp từ ảnh Hà gửi (rút gọn nhãn, giữ nguyên hình dạng).
const SCREEN: &str = "\
\u{276f} 1. [\u{2713}] Không xoá gì (Recommended)
  2. [\u{2713}] Bí danh deploy-*
  3. [\u{2713}] legacy-memory/update.md
  4. [\u{2713}] Rác build
  5. [ ] Type something
     Submit
  6. Chat about this
Enter to select · ↑/↓ to navigate · Esc to cancel";

#[test]
fn the_cursor_walks_down_to_submit_then_presses_enter() {
    let keys = submit_keys(SCREEN).expect("phải dựng được chuỗi phím");
    assert_eq!(
        keys,
        vec!["down", "down", "down", "down", "down", "enter"],
        "con trỏ ở dòng 1, Submit ở dòng thứ 6 tính từ đó"
    );
}

/// Con trỏ ĐANG Ở DƯỚI Submit thì phải đi LÊN — không phải lúc nào cũng `down`.
#[test]
fn it_walks_up_when_the_cursor_sits_below_submit() {
    let screen = SCREEN
        .replace("\u{276f} 1.", "  1.")
        .replace("  6.", "\u{276f} 6.");
    let keys = submit_keys(&screen).expect("phải dựng được chuỗi phím");
    assert_eq!(keys, vec!["up", "enter"]);
}

/// Không có dòng `Submit` (hộp chọn MỘT lựa chọn) ⟹ `None`, chỗ gọi rơi về
/// Enter trần như cũ. Đây là hộp thường gặp nhất, nên nhánh này phải im lặng
/// đúng chứ không được "cải tiến" nó.
#[test]
fn a_single_choice_box_keeps_the_plain_enter() {
    let screen = "\u{276f} 1. Vá ACL\n  2. Bỏ qua\nEnter to select · ↑/↓ to navigate";
    assert!(submit_keys(screen).is_none());
}

/// Dòng chân KHÔNG nói `to navigate` ⟹ không phải hộp điều hướng được: đừng gửi
/// mũi tên. Luật cũ (mũi tên vừa move vừa confirm) vẫn đứng ở mọi màn khác.
#[test]
fn without_the_navigate_footer_it_refuses_to_send_arrows() {
    let screen = "\u{276f} 1. Vá ACL\n     Submit\n(không có dòng chân nào)";
    assert!(submit_keys(screen).is_none());
}
