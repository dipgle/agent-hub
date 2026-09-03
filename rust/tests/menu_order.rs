//! Menu ☰ xếp bằng cách ĐẾM 200 lượt gõ gần nhất.
//!
//! 🔴 Luật hiện hành, Hà 2026-09-02: *"Mỗi lần gửi lệnh thì lưu timestamp lại,
//! mỗi lần sắp xếp thì lấy ra 200 bản ghi gần nhất rồi counting rồi sắp xếp"*.
//!
//! Nó thay CẢ BA tầng cũ, và tầng thứ ba mất đi là thứ phải nói rõ chứ không
//! được để im:
//!
//! 1. điểm theo `CommandKind` → nay đếm theo TÊN route;
//! 2. suy giảm nửa đời 7 ngày → nay là cửa sổ 200 lượt;
//! 3. **hãm 25% (`MENU_LEAD_MARGIN`) → BỎ HẲN.**
//!
//! ⚠ Tầng 3 sinh ra từ chính một câu của Hà — 2026-08-19: *"Sắp xếp ưu tiên menu
//! đang theo flow nào mà tôi thấy cứ nhảy loạn lên"* — và ba bài kiểm cũ ở tệp
//! này khoá đúng nó (`two_commands_used_back_to_back_stop_trading_places`,
//! `a_real_lead_still_moves_up`, `a_climb_is_one_step_at_a_time`). Chúng bị gỡ
//! ngày 02/09 vì luật mới không còn chỗ cho cái hãm, KHÔNG phải vì chúng sai.
//!
//! Cái giá, nói trước bằng số chứ không đợi nó tự lộ: `/session` và `/shot` được
//! bấm liền nhau và chiếm gần nửa lưu lượng, nên trong cửa sổ 200 chúng luôn sát
//! nhau và **một lượt gõ đủ làm hai đứa đổi chỗ** — đúng hiện tượng 19/08. Cửa
//! sổ 200 dập được dao động CHẬM, không dập được cặp dẫn đầu. Bài kiểm cuối tệp
//! này ĐO đúng điều đó, để nó là một sự thật có sổ chứ không phải một bất ngờ.

use std::collections::BTreeMap;

use huba::commands::{for_telegram_counted, route_name_for};
use huba::db::Db;

fn dem(pairs: &[(&str, usize)]) -> BTreeMap<String, usize> {
    pairs.iter().map(|(n, c)| (n.to_string(), *c)).collect()
}

fn names(rows: &[(&'static str, &'static str)]) -> Vec<String> {
    rows.iter().map(|(n, _)| n.to_string()).collect()
}

/// Đếm nhiều hơn thì đứng trên. Luật gốc, một dòng.
#[test]
fn more_uses_means_higher() {
    let order = names(&for_telegram_counted(&dem(&[
        ("shot", 41),
        ("session", 12),
        ("new", 3),
    ])));
    let i = |n: &str| order.iter().position(|x| x == n).expect("có mặt");
    assert!(i("shot") < i("session"), "{order:?}");
    assert!(i("session") < i("new"), "{order:?}");
    // Lệnh 0 lượt phải nằm dưới mọi lệnh có lượt.
    assert!(i("new") < i("help"), "{order:?}");
}

/// Hoà nhau thì giữ thứ tự bảng `ROUTES` — menu không tự đổi chỗ vì hai lệnh
/// cùng đếm 0. `sort_by_key` của Rust ổn định, và bài này khoá tính ổn định ấy.
#[test]
fn a_tie_keeps_the_table_order() {
    let a = names(&for_telegram_counted(&dem(&[])));
    let b = names(&for_telegram_counted(&dem(&[])));
    assert_eq!(a, b, "cùng một bảng đếm mà ra hai thứ tự");

    // …và hoà ở giữa bảng cũng vậy: hai lệnh cùng 5 lượt giữ nguyên thứ tự.
    let c = names(&for_telegram_counted(&dem(&[("pick", 5), ("tab", 5)])));
    let d = names(&for_telegram_counted(&dem(&[("tab", 5), ("pick", 5)])));
    assert_eq!(c, d, "thứ tự phụ thuộc thứ tự chèn vào map — không ổn định");
}

/// 🔴 Chính con bug Hà đọc ra 02/09: ba lệnh `Key` phải TÁCH được nhau.
///
/// Luật cũ đếm theo `CommandKind` nên `/enter`, `/right`, `/ctrlc` dùng chung
/// một ô — đo trên sổ thật sáng 02/09: cả ba đúng **280.19**, không phép đo nào
/// tách nổi. Nay mỗi cái một dòng sổ riêng.
#[test]
fn the_three_key_commands_no_longer_share_one_counter() {
    // Cùng `CommandKind::Key`, khác `Arg::Fixed` ⟹ khác tên route.
    let enter = route_name_for(huba::adapters::CommandKind::Key, "enter");
    let right = route_name_for(huba::adapters::CommandKind::Key, "right");
    assert_eq!(enter, Some("enter"));
    assert_eq!(right, Some("right"));
    assert_ne!(enter, right, "hai lệnh khác nhau mà quy về cùng một tên");

    // Và cái tên tách được ấy thật sự đổi được thứ tự menu.
    let order = names(&for_telegram_counted(&dem(&[("right", 30), ("enter", 2)])));
    let i = |n: &str| order.iter().position(|x| x == n).expect("có mặt");
    assert!(
        i("right") < i("enter"),
        "bấm /right 30 lần mà nó vẫn không vượt được /enter: {order:?}"
    );
}

/// Sổ THẬT: ghi rồi đếm, và chỉ đếm `limit` dòng GẦN NHẤT.
///
/// Cửa sổ phải cắt thật, nếu không thì "200 bản ghi gần nhất" chỉ là một câu
/// chữ — một lệnh dùng nhiều hồi tháng trước sẽ đứng đầu menu mãi mãi, đúng thứ
/// tầng suy giảm cũ sinh ra để chặn.
#[test]
fn the_window_really_cuts_off_the_old_ones() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&tmp.path().join("t.sqlite")).expect("open");

    // 10 lượt CŨ của `help`, rồi 3 lượt MỚI của `shot`.
    for _ in 0..10 {
        db.log_command("help").expect("ghi");
    }
    for _ in 0..3 {
        db.log_command("shot").expect("ghi");
    }

    // Cửa sổ rộng: thấy cả hai, `help` nhiều hơn.
    let rong = db.recent_commands(100);
    assert_eq!(rong.len(), 13);
    assert_eq!(rong.iter().filter(|n| *n == "help").count(), 10);

    // Cửa sổ hẹp = 3: chỉ còn `shot`, `help` đã TRÔI RA NGOÀI.
    let hep = db.recent_commands(3);
    assert_eq!(hep.len(), 3, "cửa sổ không cắt: {hep:?}");
    assert!(
        hep.iter().all(|n| n == "shot"),
        "cửa sổ lấy nhầm dòng cũ: {hep:?}"
    );
}

/// Chữ thường gõ vào phiên KHÔNG được chiếm ô trong cửa sổ đếm.
///
/// 🔴 Cửa này là một quyết định, nên nó phải có bài kiểm đứng sau. Chữ thường
/// chủ máy gõ đi qua `CommandKind::Type`, và `type` không nằm trong menu — nó là
/// đường gõ vào phiên, không phải một dòng bấm được. Ghi cả nó thì trong 200 ô
/// đếm, phần lớn là thứ không bao giờ hiện lên menu, và menu rốt cuộc xếp bằng
/// mấy chục ô còn sót: một phép đo có mẫu số thật khác hẳn mẫu số đã khai.
#[test]
fn free_text_typed_into_a_session_never_enters_the_counting_window() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&tmp.path().join("t.sqlite")).expect("open");

    // `type` không lên menu ⟹ không được để lại dòng nào.
    assert!(!huba::commands::is_listed("type"), "tiền đề của bài kiểm");
    huba::pipeline::menu_reorder_if_needed(&db, "type", 0);
    assert!(
        db.recent_commands(10).is_empty(),
        "chữ gõ vào phiên đã chiếm mất một ô đếm: {:?}",
        db.recent_commands(10)
    );

    // ĐỐI CHỨNG NGƯỢC: một lệnh CÓ lên menu thì phải ghi — nếu không thì bài
    // trên xanh chỉ vì chẳng có gì được ghi bao giờ.
    assert!(huba::commands::is_listed("shot"), "tiền đề của bài kiểm");
    huba::pipeline::menu_reorder_if_needed(&db, "shot", 0);
    assert_eq!(
        db.recent_commands(10),
        vec!["shot".to_string()],
        "lệnh lên menu mà không được ghi vào sổ đếm"
    );
}

/// ⚠ Sự thật KHÓ CHỊU của luật mới, ghi thành bài kiểm để nó có sổ.
///
/// Không còn hãm ⟹ cặp dẫn đầu đổi chỗ chỉ vì MỘT lượt gõ. Bài này không đòi
/// hành vi ấy phải đúng — nó ghi lại rằng hành vi ấy CÓ, để hôm nào Hà thấy menu
/// nhảy thì đây là chỗ đọc, và sửa thì sửa ở `for_telegram_counted`.
#[test]
fn without_the_brake_one_press_flips_the_top_pair() {
    let truoc = names(&for_telegram_counted(&dem(&[
        ("session", 60),
        ("shot", 59),
    ])));
    assert_eq!(truoc[0], "session");

    // Đúng MỘT lượt `/shot` nữa.
    let sau = names(&for_telegram_counted(&dem(&[
        ("session", 60),
        ("shot", 60),
    ])));
    // Hoà thì thứ tự bảng giữ `session` trên — chưa lật.
    assert_eq!(sau[0], "session", "hoà mà đã lật");

    let sau2 = names(&for_telegram_counted(&dem(&[
        ("session", 60),
        ("shot", 61),
    ])));
    assert_eq!(
        sau2[0], "shot",
        "hơn đúng 1 lượt là lật — đây là cái giá của việc bỏ hãm 25%"
    );
}
