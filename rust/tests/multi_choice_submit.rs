//! Gửi một hộp CHỌN NHIỀU: Enter trần không bao giờ qua được.
//!
//! 🔴 Hà 2026-08-17, sau khi bấm đủ bốn lựa chọn rồi `/send_…`: *"Ko qua nổi màn
//! này"*. Màn thật (ảnh anh gửi) có dạng dưới đây — `Submit` là một dòng RIÊNG,
//! không mang số, nên bấm số không tới được; còn Enter thì tác động lên đúng
//! dòng con trỏ đang đứng, tức bật/tắt lại chính cái ô vừa chọn.
//!
//! 🔴 Và cùng ngày, một tầng nữa: *"Bấm cái nọ mất cái kia ảo lắm"*. Mỗi lượt
//! `do script` tự kèm một CR không tắt được, nên MỖI mũi tên cũng là một cú
//! bật/tắt — rơi vào đúng cái ô con trỏ vừa rời khỏi. Vì vậy các bài kiểm ở đây
//! đo **LƯỢT GHI**, không đo phím: `nav_plan` phải xếp đúng ba lượt để những cú
//! bật/tắt thừa tự triệt tiêu nhau. Xem `keys::press_writes` để có cả năm phép
//! đo trên hộp thật.

use huba::keys::{checkbox_plan, submit_plan};
use huba::pipeline::{render_session_data, SessionData};

/// ✅ phải bám ngay tại dòng `Submit` — nếu không, hộp chọn nhiều không có
/// đường gửi nào trong chữ.
///
/// 🔴 Hà 2026-08-17, sau khi ☑ đã bám đúng từng dòng: *"Bấm chọn được rồi, chưa
/// bấm được submit"*. `Submit` là dòng THẬT trên màn nhưng không mang số, nên
/// không `k_`/`pick_` nào trỏ tới nó.
#[test]
fn the_submit_line_gets_its_own_tap_target() {
    huba::telegram::set_bot_username("hub_test_bot");
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
/// vào. Bản `submit_plan` đầu tiên đếm theo dòng, nên nó bắn 5 `down` cho một
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

/// Mọi mũi tên của một quãng phải nằm TRONG CÙNG một lượt ghi, và trước nó phải
/// có một lượt `enter` trần.
///
/// Đây là bất biến giữ cho *"bấm cái nọ mất cái kia"* không mọc lại: lượt ghi
/// nào có mũi tên thì cũng kèm một cú bật/tắt vào ô đang đứng (cái CR mà
/// Terminal luôn thêm), nên nó phải được một lượt `enter` đứng trước trả lại
/// dấu cho đúng ô ấy. Tách mũi tên ra nhiều lượt = mỗi bước đi lật một ô.
fn assert_plan_shape(plan: &[Vec<String>]) {
    let is_arrow = |k: &String| k == "up" || k == "down";
    for (i, w) in plan.iter().enumerate() {
        if w.iter().any(is_arrow) {
            assert!(
                w.iter().all(is_arrow),
                "lượt ghi {i} trộn mũi tên với phím khác: {w:?}"
            );
            assert_eq!(
                plan.get(i.wrapping_sub(1)).map(Vec::as_slice),
                Some(&["enter".to_string()][..]),
                "lượt mũi tên phải có một lượt `enter` trần đứng ngay trước: {plan:?}"
            );
        }
    }
}

#[test]
fn walking_counts_items_not_lines() {
    let plan = checkbox_plan(SCREEN_WITH_DESCRIPTIONS, 5).expect("phải dựng được kế hoạch");
    assert_plan_shape(&plan);
    assert_eq!(
        plan,
        vec![
            vec!["enter"],
            vec!["down", "down", "down", "down"],
            vec!["enter"]
        ],
        "từ mục 1 tới mục 5 là 4 MỤC, dù cách nhau 9 DÒNG — và cả 4 bước đi trong MỘT lượt ghi"
    );
}

/// Trong hộp CHỌN NHIỀU, "chọn mục 3" = đi tới mục 3 rồi Enter — không phải gõ
/// phím `3`.
///
/// 🔴 Hà 2026-08-17: *"Bấm xong xem lại vẫn đứng im"*. Log ghi "đã bấm '1'" mà
/// màn không đổi một ô nào — hộp này không nhận phím số. Đo lại trên hộp thật
/// cùng ngày: gửi `"4"` trong khi con trỏ đứng ở mục 1 ⟹ mục 4 không nhúc
/// nhích, chỉ mục 1 đổi dấu (đó là cái CR, không phải con số).
#[test]
fn choosing_an_item_in_a_checkbox_list_walks_instead_of_typing_a_number() {
    let plan = checkbox_plan(SCREEN_WITH_DESCRIPTIONS, 3).expect("phải dựng được");
    assert_plan_shape(&plan);
    assert_eq!(
        plan,
        vec![vec!["enter"], vec!["down", "down"], vec!["enter"]]
    );
}

/// Con trỏ ĐÃ đứng sẵn ở mục ấy ⟹ đúng MỘT lượt ghi.
///
/// Không có quãng đường nào để đi thì cũng không có ô nào phải trả lại — và đây
/// là ca duy nhất bản cũ làm đúng, nên nó phải giữ nguyên hình dạng cũ.
#[test]
fn standing_on_the_target_costs_exactly_one_write() {
    let plan = checkbox_plan(SCREEN_WITH_DESCRIPTIONS, 1).expect("phải dựng được");
    assert_eq!(plan, vec![vec!["enter"]]);
}

/// Ack phải nói KẾT QUẢ (mấy ô đang tick), không chỉ nói đã bấm.
///
/// 🔴 Hà 2026-08-17: *"Bấm chọn hết nhưng shot lại thiếu… Phản hồi về là bấm rồi
/// mà"*. `✓ đã bấm '3'` chỉ khai phím rời khỏi huba — nó vẫn xanh y hệt khi phím
/// rơi vào mục khác.
#[test]
fn the_tick_count_is_read_from_the_screen() {
    let none = huba::keys::ticked(SCREEN_WITH_DESCRIPTIONS);
    assert_eq!(none, (0, 5), "5 ô, chưa tick ô nào");

    let two = SCREEN_WITH_DESCRIPTIONS
        .replacen("1. [ ]", "1. [\u{2713}]", 1)
        .replacen("3. [ ]", "3. [\u{2713}]", 1);
    assert_eq!(huba::keys::ticked(&two), (2, 5));
}

/// …nhưng TỔNG số ô bật thì mù đúng cái ca Hà bắt được, nên phải đếm ô ĐỔI DẤU.
///
/// 🔴 Nhật ký 12:39:31 hôm ấy: bấm mục 1, kết quả `2/5` — trước đó `4/5`. Còn cú
/// 12:40:06 thì ack `3/5` cả trước lẫn sau, xanh rờn, trong khi mục 1 vừa tắt và
/// mục 2 vừa bật. Một cú bấm lành làm đổi ĐÚNG MỘT ô; đếm tổng thì hai ô cùng
/// lật ngược chiều nhau ra đúng con số cũ.
#[test]
fn two_boxes_flipping_at_once_is_visible_even_when_the_total_is_unchanged() {
    let before = SCREEN_WITH_DESCRIPTIONS.replacen("1. [ ]", "1. [\u{2713}]", 1);
    let after = SCREEN_WITH_DESCRIPTIONS.replacen("2. [ ]", "2. [\u{2713}]", 1);
    assert_eq!(
        huba::keys::ticked(&before),
        huba::keys::ticked(&after),
        "tổng số ô bật KHÔNG đổi — đây là chỗ phép đo cũ mù"
    );
    assert_eq!(
        huba::keys::ticks_changed(&before, &after),
        vec![1, 2],
        "mà hai ô đã đổi dấu, và huba phải gọi tên được cả hai"
    );

    let one = SCREEN_WITH_DESCRIPTIONS.replacen("3. [ ]", "3. [\u{2713}]", 1);
    assert_eq!(
        huba::keys::ticks_changed(SCREEN_WITH_DESCRIPTIONS, &one),
        vec![3],
        "một cú bấm lành: đúng một ô"
    );
}

/// …còn hộp chọn MỘT thì giữ nguyên đường gõ số (chạy từ 13/08).
#[test]
fn a_plain_choice_box_is_not_a_checkbox_list() {
    let screen = "\u{276f} 1. Vá ACL\n  2. Bỏ qua\nEnter to select · ↑/↓ to navigate";
    assert!(!huba::keys::is_checkbox_list(screen));
    assert!(checkbox_plan(screen, 2).is_none());
}

/// 🔴 Submit đi NGANG, không đi xuống.
///
/// Đo 2026-08-17 trên hộp thật: con trỏ ở mục 4, gửi `↓↓` để tới dòng `Submit`
/// nằm ngay dưới mục 5 — con trỏ **quấn về mục 1** (4→5→1) và cú Enter cuối lật
/// mất dấu mục 1; bảng vẫn mở. Rồi `[enter] · [→] · [enter]` trên cùng cái hộp
/// ấy: phiên nhận đúng `Chon muc nao? → Beta, Delta`.
#[test]
fn the_submit_tab_is_reached_sideways_not_downwards() {
    let plan = submit_plan(SCREEN, 0).expect("phải dựng được kế hoạch");
    assert_plan_shape(&plan);
    assert_eq!(plan, vec![vec!["enter"], vec!["right"], vec!["enter"]]);
    assert!(
        !plan.concat().iter().any(|k| k == "down" || k == "up"),
        "không một bước dọc nào: dòng `Submit` không phải chỗ con trỏ dừng được"
    );
}

/// Bảng NHIỀU câu: `✔ Submit` là tab sau câu cuối, nên số bước `→` đếm từ câu
/// đang mở. Thanh tab lấy nguyên văn ký tự đã đo (`tests/ask_table_live.rs`).
#[test]
fn a_multi_question_table_counts_tabs_from_where_it_stands() {
    let screen = format!("←  ☒ Vá ACL  ☐ Đăng nhập  ✔ Submit  →\n{SCREEN}");
    assert_eq!(
        submit_plan(&screen, 0).expect("từ câu 1"),
        vec![vec!["enter"], vec!["right", "right"], vec!["enter"]],
        "đứng ở câu 1 của bảng 2 câu: hai bước mới tới nút gửi"
    );
    assert_eq!(
        submit_plan(&screen, 1).expect("từ câu 2"),
        vec![vec!["enter"], vec!["right"], vec!["enter"]]
    );
}

/// Con trỏ ĐANG Ở DƯỚI mục cần bấm thì phải đi LÊN — không phải lúc nào cũng
/// `down`.
#[test]
fn it_walks_up_when_the_cursor_sits_below_the_target() {
    let screen = SCREEN
        .replace("\u{276f} 1.", "  1.")
        .replace("  4.", "\u{276f} 4.");
    let plan = checkbox_plan(&screen, 3).expect("phải dựng được kế hoạch");
    assert_plan_shape(&plan);
    assert_eq!(plan, vec![vec!["enter"], vec!["up"], vec!["enter"]]);
}

/// Dòng không có ô (`6. Chat about this`) KHÔNG phải chỗ con trỏ dừng được —
/// nên đừng dựng ra một quãng đường tới nó. `None` ở đây là câu trả lời đúng:
/// chỗ gọi gõ thẳng con số, và hộp chọn nhiều bỏ qua con số ấy (đo 17/08), tức
/// không có cú bấm mù nào rơi vào một mục khác.
#[test]
fn a_row_without_a_checkbox_is_not_a_stop() {
    assert!(checkbox_plan(SCREEN, 6).is_none());
}

/// Không có dòng `Submit` (hộp chọn MỘT lựa chọn) ⟹ `None`, chỗ gọi rơi về
/// Enter trần như cũ. Đây là hộp thường gặp nhất, nên nhánh này phải im lặng
/// đúng chứ không được "cải tiến" nó.
#[test]
fn a_single_choice_box_keeps_the_plain_enter() {
    let screen = "\u{276f} 1. Vá ACL\n  2. Bỏ qua\nEnter to select · ↑/↓ to navigate";
    assert!(submit_plan(screen, 0).is_none());
}

/// Dòng chân KHÔNG nói `to navigate` ⟹ không phải hộp điều hướng được: đừng gửi
/// mũi tên. Luật cũ (mũi tên vừa move vừa confirm) vẫn đứng ở mọi màn khác.
#[test]
fn without_the_navigate_footer_it_refuses_to_send_arrows() {
    let screen =
        "\u{276f} 1. [ ] Vá ACL\n  2. [ ] Đăng nhập\n     Submit\n(không có dòng chân nào)";
    assert!(submit_plan(screen, 0).is_none());
    assert!(checkbox_plan(screen, 2).is_none());
}
