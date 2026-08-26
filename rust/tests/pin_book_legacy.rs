//! Sổ gim phải đọc được CẢ DẠNG CŨ — nếu không, tính năng chết câm trên đúng
//! cái máy nó sinh ra để phục vụ.
//!
//! 🔴 Vì sao có tệp này. Cursor `pin:following` ra đời 2026-08-25 giữ mỗi
//! `message_id`. Hôm sau nó đổi sang JSON (`{"m":…,"t":…}`) để nhớ luôn CHỮ đang
//! gim, và bản đọc đầu tiên chỉ hiểu JSON. Đo trên máy Hà lúc 16:21 ngày 26/08,
//! trước khi kịp cài bản mới:
//!
//! ```text
//! sqlite3 data/huba.sqlite 'select * from cursors'
//! pin:following|12071|2026-08-26T09:23:45.582Z
//! ```
//!
//! Một con số trần. Với bản đọc chỉ-JSON thì `serde_json` phân tích ra một
//! `Number`, `.get("m")` trả `None`, và cả hàm trả `None` — nghĩa là **"chưa gim
//! gì"**, một trạng thái hợp lệ, nên không một dòng log nào được ghi. Hậu quả,
//! cả hai đều câm:
//!
//! * `refresh_pin` về sớm ở mọi vòng ⟹ icon trạng thái trên tin gim đứng yên
//!   mãi mãi — đúng thứ `CLAUDE.md` gọi là phép đo mù: nhìn thì có tin, mà tin
//!   ấy không đổi được theo sự thật;
//! * `pin_following` thôi gỡ tin cũ ⟹ mỗi lần đổi phiên buồng chat mọc thêm một
//!   cái gim.
//!
//! Đây là **đối chứng ngược** của cửa ấy (điều 13① của workspace): trả `pin:
//! following` về dạng chỉ-JSON là hai bài kiểm dưới đây phải ĐỎ.

use huba::db::Db;
use huba::pipeline::{pinned_message, PIN_FOLLOWING_KEY};

fn db() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&dir.path().join("t.sqlite")).expect("open db");
    (dir, db)
}

#[test]
fn a_bare_number_in_the_pin_book_is_still_a_pinned_message() {
    let (_dir, db) = db();
    // Đúng giá trị đọc được từ máy Hà, không phải một con số bịa ra.
    db.set_cursor(PIN_FOLLOWING_KEY, "12071").expect("set");

    let (mid, chu) = pinned_message(&db)
        .expect("sổ dạng CŨ (số trần) vẫn phải đọc ra một tin gim — nếu None thì cả tính năng câm");
    assert_eq!(mid, 12071, "phải lấy đúng message_id trong sổ cũ");
    assert_eq!(
        chu, "",
        "sổ cũ không giữ chữ ⟹ phải trả RỖNG, để lượt quét sau thấy 'có gì mới' \
         và sửa tin gim về đúng dạng — tự chuyển hệ, không cần bước di trú riêng"
    );
}

#[test]
fn the_json_shape_carries_both_the_id_and_the_pinned_text() {
    let (_dir, db) = db();
    db.set_cursor(PIN_FOLLOWING_KEY, r#"{"m":42,"t":"⚡ [social] (acc1)"}"#)
        .expect("set");

    let (mid, chu) = pinned_message(&db).expect("dạng JSON phải đọc được");
    assert_eq!(mid, 42);
    assert_eq!(chu, "⚡ [social] (acc1)");
}

/// Sổ hỏng thì phải là "chưa gim gì", không phải một `message_id` bịa ra.
#[test]
fn a_book_that_is_neither_shape_reads_as_no_pin() {
    let (_dir, db) = db();
    for rac in ["", "   ", "khong-phai-so", r#"{"t":"thiếu m"}"#] {
        db.set_cursor(PIN_FOLLOWING_KEY, rac).expect("set");
        assert_eq!(
            pinned_message(&db),
            None,
            "sổ {rac:?} không đọc ra được thì phải là None — gim theo một id đoán \
             bừa là gim nhầm tin của người khác"
        );
    }
}

/// Đối chứng cho chỗ `refresh_pin` dựa vào: chữ của sổ cũ (`""`) phải KHÁC mọi
/// dòng gim thật, nếu không thì lượt quét đầu tiên tưởng "không có gì mới" và
/// tin gim đứng nguyên ở dạng cũ mãi.
#[test]
fn the_empty_legacy_text_never_equals_a_real_pin_line() {
    let s = huba::sessions::LiveSession {
        session_id: "b1e46802".into(),
        account: "acc1".into(),
        ..Default::default()
    };
    assert_ne!(
        huba::pipeline::pin_line(&s),
        "",
        "pin_line luôn mở đầu bằng một icon trạng thái, nên nó không bao giờ rỗng"
    );
}

/// Dòng của ĐƯỜNG NHANH (chạm vào phiên) và dòng của `refresh_pin` chỉ được khác
/// nhau ĐÚNG ở cái icon.
///
/// 🔴 Vì sao đây là một cái cổng, không phải một chi tiết thẩm mỹ: `refresh_pin`
/// so CHUỖI để biết "lần này có gì mới". Lệch thêm bất cứ chỗ nào — một cặp `()`
/// rỗng, một khoảng trắng — là tin gim bị `editMessageText` ở MỌI vòng quét, mười
/// giây một lần, mãi mãi, và không có gì kêu lên cả.
#[test]
fn the_fast_path_line_and_the_refresh_line_differ_only_in_the_icon() {
    let s = huba::sessions::LiveSession {
        session_id: "b1e46802".into(),
        label: "[social]".into(),
        account: "acc1".into(),
        ..Default::default()
    };
    let (icon, _) = huba::sessions::state_of(&s);
    let nhanh = huba::pipeline::pin_line_from("\u{1f441}", &huba::sessions::shown(&s), &s.account);
    let nen = huba::pipeline::pin_line(&s);
    assert_eq!(
        nhanh.strip_prefix("\u{1f441}"),
        nen.strip_prefix(icon),
        "hai dòng phải giống hệt nhau sau khi bóc icon — nếu không, refresh_pin \
         sửa tin gim ở mọi vòng quét"
    );
}

/// Không biết tài khoản thì đừng mở ngoặc — một cặp `()` rỗng nói rằng huba biết
/// một điều gì đó rồi bỏ trống.
#[test]
fn an_unknown_account_leaves_no_empty_parens() {
    for tk in ["", "   "] {
        let dong = huba::pipeline::pin_line_from("\u{26a1}", "[social]", tk);
        assert_eq!(
            dong, "\u{26a1} [social]",
            "tài khoản {tk:?} không được đẻ ra ngoặc rỗng"
        );
    }
    assert_eq!(
        huba::pipeline::pin_line_from("\u{26a1}", "[social]", "acc1"),
        "\u{26a1} [social] (acc1)"
    );
}

/// ĐỐI CHỨNG NGƯỢC, đo được ngay tại đây: bản đọc CŨ (chỉ hiểu JSON) mù đúng với
/// giá trị đang nằm trong sổ trên máy thật.
///
/// Không phải một lời kể — đây là chính hai bước bản cũ làm, chạy trên chính con
/// số `12071`. Nó chứng minh cửa này ĐỔI ĐƯỢC TRẠNG THÁI: bỏ nhánh số-trần trong
/// `pinned_message` là quay lại đúng cái `None` câm lặng ở dưới.
#[test]
fn the_json_only_reader_goes_blind_on_the_real_book_value() {
    let so_that = "12071";
    let j: serde_json::Value = serde_json::from_str(so_that)
        .expect("một con số vẫn là JSON hợp lệ — nên `.ok()?` KHÔNG cứu");
    assert!(
        j.get("m").is_none(),
        "đây là chỗ bản cũ chết câm: JSON phân tích XONG (một Number), nhưng \
         `.get(\"m\")` không có gì, và `?` biến nó thành 'chưa gim gì'"
    );
}
