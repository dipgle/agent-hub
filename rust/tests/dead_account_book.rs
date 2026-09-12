//! 🔴 Hà 2026-09-02: `/new` lại nhảy vào acc1 — tài khoản đã bị TỔ CHỨC KHOÁ.
//!
//! Cổng cũ (`tests/dead_account_and_focus.rs`) đã chặn đúng ca nó dựng ra, và
//! vẫn xanh. Nó chặn được vì mỗi bài đều đưa vào một `LiveSession` CÒN MỞ mang
//! dấu `account_dead`. Tức thứ nó thật sự chứng minh là: *"đang NHÌN THẤY một
//! cửa sổ báo khoá thì đừng chọn"* — không phải *"tài khoản này đã chết"*.
//!
//! Cái chết của một tài khoản sống lâu hơn cửa sổ báo nó. Đóng cửa sổ ấy đi —
//! hoặc chính huba đóng, vì phiên chết ngay dòng đầu thì không có gì giữ nó lại
//! — và `now` rỗng: không còn ai mang dấu, `quota` vẫn xếp acc1 `Unknown` vì sổ
//! `.claude.json` của nó ghi `92%` từ ba ngày trước, và `Unknown` đứng TRƯỚC
//! `Full`. huba chọn lại đúng tài khoản vừa chết, và tốn của chủ máy đúng cái
//! thứ đang thiếu: một lượt gọi `claude`.
//!
//! Nên trí nhớ phải nằm ở SỔ, không nằm ở một cửa sổ đang mở.
//!
//! ⚠ Ghi rõ giới hạn của chính bài này: nó chấm `suggest_account` + `rank_all`
//! trên một cuốn sổ dựng bằng tay. Việc *ghi* vào sổ (ai gọi `mark_account_dead`
//! và ở vòng nào) do `dead_book_is_written_and_cleared` chấm, và cả hai chưa
//! chứng minh được đường chạy thật — chỗ ấy là `install` + một lượt `/new`.

use std::collections::BTreeMap;

use huba::db::Db;
use huba::quota::{Rank, Ranked};
use huba::sessions::LiveSession;
use huba::watch::suggest_account;

/// Giờ máy dùng cho mọi lượt gọi `suggest_account` trong tệp này: **12:45**.
///
/// Cố định, không bao giờ `Local::now()` — `suggest_account` từ 10/09 hỏi MỐC
/// mở lại của dòng hạn mức, nên một bài kiểm lấy giờ thật sẽ đổi phán quyết
/// theo lúc chạy. Ở 12:45 thì `resets 1pm/2pm/3pm` đều còn ở phía trước (dưới
/// trần 360 phút của `pipeline::limit_still_biting`) ⟹ đọc ra là CÒN cắn, đúng
/// ý mọi bài đã viết trước đó.
const LUC_12_45: u64 = 12 * 60 + 45;

fn xep(rows: &[(&str, Rank)]) -> Vec<Ranked> {
    rows.iter()
        .map(|(n, r)| Ranked {
            name: n.to_string(),
            rank: *r,
        })
        .collect()
}

/// Cái chết SỐNG LÂU HƠN cửa sổ báo nó.
///
/// Đây là ca cổng cũ để lọt, và là đúng ca Hà gặp: không còn phiên nào của acc1
/// trên màn, nên `now` rỗng.
#[test]
fn a_dead_account_stays_dead_after_its_window_closes() {
    let hang = xep(&[("acc1", Rank::Unknown), ("acc3", Rank::Full)]);

    // ĐỐI CHỨNG NGƯỢC — sổ TRỐNG thì acc1 vẫn được chọn. Không có dòng này thì
    // bài dưới có thể xanh vì một lý do khác (acc1 bị loại sẵn), và cổng thành
    // lời đồn (§13①).
    assert_eq!(
        suggest_account("", &hang, &[], LUC_12_45).as_deref(),
        Some("acc1"),
        "sổ trống ⟹ `Unknown` vẫn hơn `Full`, acc1 là câu trả lời đúng"
    );

    // Sổ nhớ, dù KHÔNG cửa sổ nào còn mở: `rank_all` đóng dấu `Dead` lên hạng.
    let mut so = BTreeMap::new();
    so.insert(
        "acc1".to_string(),
        "Your organization has disabled Claude subscription access".to_string(),
    );
    let hang_co_so = huba::quota::apply_dead_book(hang.clone(), &so);
    assert_eq!(
        hang_co_so
            .iter()
            .find(|a| a.name == "acc1")
            .map(|a| &a.rank),
        Some(&Rank::Dead),
        "sổ ghi acc1 chết ⟹ hạng của nó phải đọc ra là chết, không phải `Unknown`"
    );
    assert_eq!(
        suggest_account("", &hang_co_so, &[], LUC_12_45),
        None,
        "acc1 chết trong sổ + acc3 kịch trần ⟹ KHÔNG có gì để gợi ý, và nói thẳng"
    );
}

/// `Dead` phải đọc ra được cho CHỦ MÁY, vì `/accounts` là chỗ duy nhất soi lại
/// luật chọn tài khoản. Một tài khoản bị loại mà không nói vì sao thì luật ấy
/// không soi được.
#[test]
fn the_owner_can_read_why_an_account_was_skipped() {
    let noi = Rank::Dead.say();
    assert!(
        noi.to_lowercase().contains("khoá") || noi.to_lowercase().contains("khóa"),
        "câu phải nói ra là bị KHOÁ, không lẫn với `chưa đo được`: {noi:?}"
    );
    assert_ne!(
        Rank::Dead.say(),
        Rank::Unknown.say(),
        "hai trạng thái khác nhau thì phải đọc ra khác nhau"
    );
}

/// SỔ GHI VÀ SỔ XOÁ — và cửa xoá phải là BẰNG CHỨNG SỐNG, không phải đồng hồ.
///
/// 🔴 Vì sao không dùng TTL: khoá tổ chức không tự mở theo giờ. Một hạn dùng
/// (kiểu "quên sau 24h") sẽ ÂM THẦM hồi sinh một tài khoản vẫn đang chết, và
/// lỗi quay lại y nguyên vào đúng ngày thứ hai. Thứ chứng minh tài khoản sống
/// lại được chỉ có một: một phiên của nó CHẠY ĐƯỢC.
#[test]
fn dead_book_is_written_and_cleared() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&tmp.path().join("t.sqlite")).expect("open");

    assert!(db.dead_accounts().is_empty(), "sổ mới phải rỗng");

    let phien = |acc: &str, dead: bool, working: bool| LiveSession {
        host: "terminal".to_string(),
        account: acc.to_string(),
        working,
        account_dead: dead.then(|| "Your organization has disabled…".to_string()),
        ..Default::default()
    };

    // Thấy dấu khoá trên màn ⟹ vào sổ.
    huba::watch::reconcile_dead_book(&db, &[phien("acc1", true, false)]);
    assert_eq!(
        db.dead_accounts().keys().collect::<Vec<_>>(),
        vec!["acc1"],
        "thấy dấu khoá thì phải GHI, không chỉ dùng cho vòng này"
    );

    // Một phiên acc1 KHÔNG chạy và không mang dấu thì CHƯA phải bằng chứng
    // sống — cửa sổ vừa mở chưa kịp in gì cũng trông y hệt.
    huba::watch::reconcile_dead_book(&db, &[phien("acc1", false, false)]);
    assert_eq!(
        db.dead_accounts().keys().collect::<Vec<_>>(),
        vec!["acc1"],
        "im lặng không phải bằng chứng sống"
    );

    // CHẠY ĐƯỢC mới là bằng chứng ⟹ xoá khỏi sổ.
    huba::watch::reconcile_dead_book(&db, &[phien("acc1", false, true)]);
    assert!(
        db.dead_accounts().is_empty(),
        "một phiên acc1 chạy được ⟹ tài khoản sống lại, sổ phải buông ra"
    );
}
