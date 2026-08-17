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

/// Mỗi lựa chọn kéo theo MẤY DÒNG mô tả — đếm dòng thì con trỏ dừng giữa đường.
///
/// 🔴 Đây là màn THẬT của `[dwork]`: mỗi lựa chọn có hai ba dòng giải thích thụt
/// vào. Bản `submit_keys` đầu tiên đếm theo dòng, nên nó bắn 5 `down` cho một
/// quãng chỉ dài 5 MỤC nhưng 14 DÒNG — và cú Enter rơi vào một lựa chọn.
const SCREEN_WITH_DESCRIPTIONS: &str = "\
\u{276f} 1. [ ] Không xoá gì (Recommended)
   Đĩa đã trống 218 Gi, vitest chạy lại 1517/1517 exit 0.
   Không có gì cần dọn.
  2. [ ] Bí danh deploy-*
   Xoá 7 dòng bí danh tương thích.
  3. [ ] legacy-memory/update.md
   Ảnh chụp quy trình deploy PROD cũ.
  4. [ ] Rác build (target/, node_modules, cache)
   Dọn artifact tái tạo được.
  5. [ ] Type something
     Submit
  6. Chat about this
Enter to select · ↑/↓ to navigate · Esc to cancel";

#[test]
fn walking_counts_items_not_lines() {
    let keys = submit_keys(SCREEN_WITH_DESCRIPTIONS).expect("phải dựng được chuỗi phím");
    assert_eq!(
        keys,
        vec!["down", "down", "down", "down", "down", "enter"],
        "từ mục 1 tới Submit là 5 MỤC, dù cách nhau 10 dòng"
    );
}

/// Trong hộp CHỌN NHIỀU, "chọn mục 3" = đi tới mục 3 rồi Enter — không phải gõ
/// phím `3`.
///
/// 🔴 Hà 2026-08-17: *"Bấm xong xem lại vẫn đứng im"*. Log ghi "đã bấm '1'" mà
/// màn không đổi một ô nào — hộp này không nhận phím số.
#[test]
fn choosing_an_item_in_a_checkbox_list_walks_instead_of_typing_a_number() {
    let keys = hub::keys::checkbox_keys(SCREEN_WITH_DESCRIPTIONS, 3).expect("phải dựng được");
    assert_eq!(keys, vec!["down", "down", "enter"]);
}

/// Ack phải nói KẾT QUẢ (mấy ô đang tick), không chỉ nói đã bấm.
///
/// 🔴 Hà 2026-08-17: *"Bấm chọn hết nhưng shot lại thiếu… Phản hồi về là bấm rồi
/// mà"*. `✓ đã bấm '3'` chỉ khai phím rời khỏi hub — nó vẫn xanh y hệt khi phím
/// rơi vào mục khác.
#[test]
fn the_tick_count_is_read_from_the_screen() {
    let none = hub::keys::ticked(SCREEN_WITH_DESCRIPTIONS);
    assert_eq!(none, (0, 5), "5 ô, chưa tick ô nào");

    let two = SCREEN_WITH_DESCRIPTIONS
        .replacen("1. [ ]", "1. [\u{2713}]", 1)
        .replacen("3. [ ]", "3. [\u{2713}]", 1);
    assert_eq!(hub::keys::ticked(&two), (2, 5));
}

/// …còn hộp chọn MỘT thì giữ nguyên đường gõ số (chạy từ 13/08).
#[test]
fn a_plain_choice_box_is_not_a_checkbox_list() {
    let screen = "\u{276f} 1. Vá ACL\n  2. Bỏ qua\nEnter to select · ↑/↓ to navigate";
    assert!(!hub::keys::is_checkbox_list(screen));
    assert!(hub::keys::checkbox_keys(screen, 2).is_none());
}

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
