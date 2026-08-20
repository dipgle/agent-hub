//! `/tab <n>` — sang một tab của bảng hỏi, KHÔNG chốt gì.
//!
//! 🔴 Hà 2026-08-19: *"mặc định thao tác trên máy muốn chuyển tab thì bấm phím
//! phải trái, giờ qua tele thì có nút bấm ở chính tab để nhận như click chuột"*.
//!
//! Route này không dựng được suốt ba ngày vì một bức tường thật: `do script`
//! kèm một CR không tắt được, nên "sang phải" luôn kèm "chốt câu đang mở" — và
//! nó đã chốt thật, `☐ RPC pool` → `☒`, trên bảng của phiên `[AI/tcc/amm]`.
//! Nó đứng được nhờ `cgkeys` (phím rời, không CR).
//!
//! 📐 Phép đếm bước ở đây đứng trên bốn phép đo trên chính bảng ấy (19/08):
//! `→` không quấn vòng (6 lượt đều dừng ở `Review your answers`), `←` cũng
//! không (6 lượt thì 3 lượt cuối đều là câu số 1), thứ tự đúng như thanh tab
//! vẽ, và `answered` giữ nguyên qua **cả 12 lượt**.

use huba::keys::tab_keys;

fn count(keys: &[String], which: &str) -> usize {
    keys.iter().filter(|k| *k == which).count()
}

/// Về mép trái rồi đếm sang phải — vì huba KHÔNG biết con trỏ đang ở đâu.
///
/// Tab hiện hành vẽ bằng màu nền, mà `contents of tab` trả chữ trần, nên "đang
/// đứng ở tab mấy" là câu hỏi không có nguồn nào trả lời được. Đếm từ một chỗ
/// đoán là đi nhầm tab — thứ, ở đường `/pick`, chốt một lựa chọn cho câu người
/// ta chưa đọc.
#[test]
fn going_to_a_tab_homes_left_first_then_counts() {
    let keys = tab_keys(3, 1);
    assert_eq!(count(&keys, "left"), 4, "phải đẩy sát mép trái: {keys:?}");
    assert_eq!(
        count(&keys, "right"),
        0,
        "tab 1 chính là mép trái: {keys:?}"
    );

    let keys = tab_keys(3, 3);
    assert_eq!(count(&keys, "left"), 4);
    assert_eq!(count(&keys, "right"), 2, "từ tab 1 sang tab 3 là 2 bước");
    // Thứ tự phải là TRÁI hết rồi mới PHẢI. Trộn lẫn là đếm từ chỗ không biết.
    let first_right = keys.iter().position(|k| k == "right").unwrap();
    assert!(
        keys[..first_right].iter().all(|k| k == "left"),
        "mũi tên phải chen vào giữa quãng về mốc: {keys:?}"
    );
}

/// Thừa một lượt `←` là cố ý: mép trái NUỐT lượt thừa (đo được), nên thừa thì
/// vô hại còn thiếu thì về sai chỗ.
#[test]
fn homing_spends_one_extra_step_on_purpose() {
    for tabs in 1..=6 {
        let keys = tab_keys(tabs, 1);
        assert_eq!(
            count(&keys, "left"),
            tabs + 1,
            "bảng {tabs} câu phải đẩy {} lượt",
            tabs + 1
        );
    }
}

/// `/tab 0` = bước `Review your answers`, nằm sát mép PHẢI — nên nó rẻ hơn:
/// đẩy hết sang phải, mép phải nuốt lượt thừa.
#[test]
fn tab_zero_is_the_review_step_at_the_right_edge() {
    let keys = tab_keys(3, 0);
    assert_eq!(count(&keys, "right"), 4, "{keys:?}");
    assert_eq!(count(&keys, "left"), 0, "{keys:?}");
}

/// KHÔNG có `enter` trong dãy — đây là cả lời hứa của route.
///
/// Một `enter` lọt vào đây là chốt một câu chủ máy chưa đọc, và không lùi lại
/// được. Bài kiểm này là chỗ nó phải chết.
#[test]
fn a_tab_move_never_carries_a_commit() {
    for tabs in 1..=5 {
        for target in 0..=tabs {
            let keys = tab_keys(tabs, target);
            assert!(
                keys.iter().all(|k| k == "left" || k == "right"),
                "dãy đi tab mang phím chốt: {keys:?}"
            );
        }
    }
}

/// Nút trên Telegram và lệnh gõ tay phải cho ra CÙNG một route.
#[test]
fn the_button_and_the_typed_command_agree() {
    assert_eq!(
        huba::telegram::callback_to_command("tab:da29807e:2").as_deref(),
        Some("/tab da29807e 2")
    );
    // …và liên kết chạm được trong chữ cũng vậy (`/tab_<id>_<n>`).
    let got = huba::verbs::parse_command("/tab_da29807e_2").expect("phải phân tích được");
    assert_eq!(got.0, huba::adapters::CommandKind::Tab);
    assert_eq!(got.2, "da29807e 2");
}

/// Nút hỏng thì thôi, đừng dựng ra một route trỏ vào chỗ trống.
#[test]
fn a_malformed_tab_button_is_refused() {
    assert!(huba::telegram::callback_to_command("tab:da29807e:").is_none());
    assert!(huba::telegram::callback_to_command("tab::2").is_none());
    // Mã phiên phải là hex — `tab_xyz_2` là chữ ai đó gõ nhầm, không phải phiên.
    assert!(huba::verbs::parse_command("/tab_xyz!_2").is_none());
    // …và số tab phải là số.
    assert!(huba::verbs::parse_command("/tab_da29807e_hai").is_none());
}
