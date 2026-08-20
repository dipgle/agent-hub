//! Menu ☰ thôi lật chỗ vì một cú bấm.
//!
//! 🔴 Hà 2026-08-19: *"Sắp xếp ưu tiên menu đang theo flow nào mà tôi thấy cứ
//! nhảy loạn lên"*. Flow đúng là thứ anh đặt hôm 17/08 — tần suất có suy giảm
//! theo thời gian (nửa đời 7 ngày) — nhưng nó xếp bằng SO SÁNH TRẦN, nên hai
//! lệnh sát điểm nhau đổi chỗ sau mỗi lượt bấm.
//!
//! Con số trong bài kiểm này là con số THẬT, lấy từ `cursors.menu:usage` của
//! `data/huba.sqlite` sáng 19/08 — không phải số bịa cho vừa lời kết luận.

/// Bảng điểm thật, chép nguyên từ sổ (điểm đã nhân 1000 như `menu_reorder_if_needed`).
fn real_scores() -> Vec<(&'static str, &'static str, u64)> {
    vec![
        ("session", "phiên", 257_647),
        ("shot", "màn", 241_216),
        ("type", "gõ", 100_669),
        ("enter", "gửi", 97_325),
        ("runin", "chạy trong phiên", 28_509),
        ("right", "phải", 28_509),
        ("close", "đóng sổ", 23_056),
        ("new", "mở phiên", 9_240),
        ("pick", "chọn", 7_999),
        ("terminal", "cửa sổ", 7_802),
        ("anh", "ảnh", 5_785),
        ("accounts", "tài khoản", 1_000),
        ("clean", "dọn", 1_000),
        ("help", "trợ giúp", 1_000),
    ]
}

fn names(rows: &[(&'static str, &'static str)]) -> Vec<String> {
    rows.iter().map(|(n, _)| n.to_string()).collect()
}

/// Chính ca Hà đọc được: `/session` 257,6 và `/shot` 241,2 — hơn nhau 6,8%.
///
/// Bấm một phiên là chạy `/session` rồi `/shot` liền nhau, nên mỗi lượt một bên
/// +1 và bên ấy dẫn đầu. Log 18/08 có đủ bốn lần lật trong 13 phút, hai lần
/// **cách nhau 3 giây**. Sau khi hãm thì không lần nào được lật.
#[test]
fn two_commands_used_back_to_back_stop_trading_places() {
    let prev: Vec<String> = names(&huba::pipeline::menu_settled_order(&[], &real_scores()));
    assert_eq!(prev[0], "session", "thứ tự đầu tiên vẫn xếp theo điểm");

    // …rồi `/shot` được bấm thêm một lượt và vượt lên về ĐIỂM.
    let mut bumped = real_scores();
    bumped[1].2 = 258_647; // shot > session, hơn 0,4%
    bumped.sort_by_key(|(_, _, s)| std::cmp::Reverse(*s));
    let after = names(&huba::pipeline::menu_settled_order(&prev, &bumped));

    assert_eq!(
        after, prev,
        "menu lật chỗ vì một cú bấm — đúng thứ Hà thấy nhảy loạn"
    );
    assert_eq!(after[0], "session");
    assert_eq!(after[1], "shot");
}

/// …nhưng hãm KHÔNG phải đóng băng: hơn hẳn thì vẫn phải leo.
#[test]
fn a_real_lead_still_moves_up() {
    let prev: Vec<String> = names(&huba::pipeline::menu_settled_order(&[], &real_scores()));
    let mut heavy = real_scores();
    heavy[1].2 = 400_000; // shot bỏ xa session (257,6) — hơn 55%
    heavy.sort_by_key(|(_, _, s)| std::cmp::Reverse(*s));
    let after = names(&huba::pipeline::menu_settled_order(&prev, &heavy));
    assert_eq!(
        after[0], "shot",
        "hãm thành đóng băng: lệnh dùng nhiều hơn hẳn vẫn không lên được"
    );
    assert_eq!(after[1], "session");
}

/// Và nó leo TỪNG NẤC, không nhảy tám bậc một lượt.
///
/// Ca thật: `accounts` 12→8 lúc 18/08 01:57. Một lệnh nhảy tám bậc thì lần sau
/// tìm nó ở chỗ cũ là sai, mà chỗ mới thì chưa kịp nhớ.
#[test]
fn a_climb_is_one_step_at_a_time() {
    let prev: Vec<String> = names(&huba::pipeline::menu_settled_order(&[], &real_scores()));
    let before = prev.iter().position(|n| n == "accounts").expect("có mặt");

    let mut woken = real_scores();
    for row in woken.iter_mut() {
        if row.0 == "accounts" {
            row.2 = 9_000; // vượt `pick` 7 999 và `terminal` 7 802, dưới `new` 9 240
        }
    }
    woken.sort_by_key(|(_, _, s)| std::cmp::Reverse(*s));
    let after = names(&huba::pipeline::menu_settled_order(&prev, &woken));
    let now = after.iter().position(|n| n == "accounts").expect("có mặt");

    assert!(now < before, "điểm lên mà chỗ không lên: {before} → {now}");
    assert_eq!(
        now,
        before - 1,
        "nhảy nhiều hơn một bậc trong một lượt: {before} → {now} ({after:?})"
    );
}

/// Hoà nhau thì giữ nguyên — lời hứa cũ của 17/08, không được mất khi thêm hãm.
///
/// `runin` và `right` trong sổ thật bằng nhau đúng đến từng phần nghìn (28 509).
#[test]
fn a_tie_keeps_the_order_it_had() {
    let prev: Vec<String> = names(&huba::pipeline::menu_settled_order(&[], &real_scores()));
    let again = names(&huba::pipeline::menu_settled_order(&prev, &real_scores()));
    assert_eq!(again, prev, "cùng một bảng điểm mà ra hai thứ tự");
    let i = prev.iter().position(|n| n == "runin").unwrap();
    let j = prev.iter().position(|n| n == "right").unwrap();
    assert!(i < j, "hoà nhau thì giữ thứ tự bảng: {prev:?}");
}

/// Một lệnh MỚI thêm vào bảng không được xáo tung phần còn lại.
#[test]
fn a_new_command_lands_without_shuffling_the_rest() {
    let prev: Vec<String> = names(&huba::pipeline::menu_settled_order(&[], &real_scores()));
    let mut with_new = real_scores();
    with_new.push(("doctor", "sức khoẻ", 0));
    let after = names(&huba::pipeline::menu_settled_order(&prev, &with_new));
    assert_eq!(
        after.last().unwrap(),
        "doctor",
        "lệnh 0 điểm phải đứng cuối"
    );
    assert_eq!(&after[..prev.len()], &prev[..], "phần còn lại bị xáo");
}

/// Lệnh bị GỠ khỏi bảng thì rơi khỏi menu, không để lại chỗ trống.
#[test]
fn a_removed_command_leaves_the_menu() {
    let prev: Vec<String> = names(&huba::pipeline::menu_settled_order(&[], &real_scores()));
    let fewer: Vec<_> = real_scores()
        .into_iter()
        .filter(|(n, _, _)| *n != "clean")
        .collect();
    let after = names(&huba::pipeline::menu_settled_order(&prev, &fewer));
    assert!(!after.iter().any(|n| n == "clean"), "{after:?}");
    assert_eq!(after.len(), prev.len() - 1);
}
