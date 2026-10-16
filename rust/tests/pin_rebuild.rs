//! Tin gim phải TỰ QUAY LẠI khi nó mất khỏi tầm mắt chủ máy.
//!
//! 🔴 Hà 2026-10-16: *"khi xóa hết lịch sử lại không thấy tự tạo lại tin gim ở
//! trên về phiên đang được chọn"*.
//!
//! Đo cùng ngày, trên máy Hà:
//!
//! * ~02:24Z Hà xoá lịch sử buồng chat. Tin gim 56699 vẫn SỐNG ở phía bot:
//!   `telegram_message_edited 56699` chạy tiếp suốt hai giờ, và `getChat` lúc
//!   04:20:37Z vẫn trả `pinned_message.message_id = 56699`. Xoá lịch sử với bot
//!   chỉ xoá ở phía chủ máy ⟹ không có tín hiệu nào từ Telegram, trừ nút START
//!   của buồng trống — nó gửi `/start` TRẦN. 20 cú bấm trong 5 phút; huba đáp cả
//!   20 bằng *"cú chạm ấy không mang lựa chọn nào"* và không gim lại gì.
//! * Đồng hồ tự xoá của buồng chat là 86400 s (`message_auto_delete_time`), và
//!   nó xoá cả tin ĐANG GIM: log 08/10–15/10 có đúng một `pin_edit_failed …
//!   message to edit not found` mỗi ngày, ~24 giờ sau khi tin gim được gửi — và
//!   vòng nền khi ấy bỏ cuộc (`tao_neu_thieu = false`), để đỉnh buồng trống tới
//!   khi chủ máy tự chạm vào một phiên.
//!
//! Tệp này khoá cả hai đường: hàm thuần quyết định (khi nào dựng lại, khi nào
//! KHÔNG), và chỗ nối (bài soi nguồn) — kèm đối chứng ngược cho từng chỗ nối.

use huba::db::Db;
use huba::pipeline::{
    bare_start_reply, pin_bg_rebuild_allowed, pin_book, pin_due_for_renew, pin_is_gone,
    pinned_message, pinned_sent_at, PIN_BG_REBUILD_GAP_MAX_SEC, PIN_BG_REBUILD_GAP_SEC,
    PIN_FOLLOWING_KEY, PIN_RENEW_BEFORE_SEC,
};

fn db() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&dir.path().join("t.sqlite")).expect("open db");
    (dir, db)
}

// ── "Đã mất" phải là BẰNG CHỨNG, không phải mọi lượt hỏng ────────────────────

#[test]
fn only_telegrams_not_found_reads_as_a_lost_pin() {
    // Đúng chuỗi trong log máy Hà (pin_edit_failed, 15/10 13:54:06Z).
    assert!(pin_is_gone("Bad Request: message to edit not found"));
    // Mỗi chuỗi dưới đây là "không sửa được LÚC NÀY", không phải "đã mất".
    // Đọc nhầm một cái là vòng nền đẻ một tin mới cho mỗi lượt hỏng.
    for khong_phai in [
        "Bad Request: message is not modified: specified new message content and reply markup are exactly the same as a current content and reply markup of the message",
        "Too Many Requests: retry after 7",
        "Forbidden: bot was blocked by the user",
        "error sending request for url (https://api.telegram.org/…)",
        "",
    ] {
        assert!(!pin_is_gone(khong_phai), "{khong_phai:?} KHÔNG phải bằng chứng tin đã mất");
    }
}

// ── Thay tin gim TRƯỚC khi đồng hồ của buồng chat xoá nó ─────────────────────

#[test]
fn a_pin_is_renewed_just_before_the_chat_timer_and_not_a_second_earlier() {
    let t = 86_400; // đo 16/10: message_auto_delete_time
    let gui = 1_000_000;
    let moc = gui + t - PIN_RENEW_BEFORE_SEC;
    assert!(!pin_due_for_renew(Some(gui), moc - 1, Some(t)));
    assert!(pin_due_for_renew(Some(gui), moc, Some(t)));
    // Đã quá hạn (huba nằm im qua mốc) vẫn là "phải thay".
    assert!(pin_due_for_renew(Some(gui), gui + 2 * t, Some(t)));
}

#[test]
fn no_known_age_or_no_timer_means_no_renewal() {
    // Sổ cũ chưa có mốc gửi ⟹ không đoán tuổi của một tin.
    assert!(!pin_due_for_renew(None, i64::MAX / 2, Some(86_400)));
    // Buồng không đặt đồng hồ (getChat không có trường / bằng 0).
    assert!(!pin_due_for_renew(Some(0), 10_000_000, None));
    assert!(!pin_due_for_renew(Some(0), 10_000_000, Some(0)));
}

#[test]
fn a_short_timer_still_waits_half_its_life() {
    // Đồng hồ ngắn hơn khoảng "thay sớm" ⟹ không được dựng lại ở MỌI vòng.
    let t = 600;
    assert!(!pin_due_for_renew(Some(0), t / 2 - 1, Some(t)));
    assert!(pin_due_for_renew(Some(0), t / 2, Some(t)));
}

// ── Nhịp của vòng nền: một lỗi dai dẳng không được thành một tin mỗi 10 giây ──

#[test]
fn the_background_rebuild_has_a_floor_and_backs_off_on_failure() {
    let at = 5_000_000;
    assert!(pin_bg_rebuild_allowed(None, at), "chưa thử lần nào ⟹ được");
    assert!(!pin_bg_rebuild_allowed(
        Some((at, 0)),
        at + PIN_BG_REBUILD_GAP_SEC - 1
    ));
    assert!(pin_bg_rebuild_allowed(
        Some((at, 0)),
        at + PIN_BG_REBUILD_GAP_SEC
    ));
    // Hỏng một lần ⟹ chờ gấp đôi.
    assert!(!pin_bg_rebuild_allowed(
        Some((at, 1)),
        at + 2 * PIN_BG_REBUILD_GAP_SEC - 1
    ));
    assert!(pin_bg_rebuild_allowed(
        Some((at, 1)),
        at + 2 * PIN_BG_REBUILD_GAP_SEC
    ));
    // Hỏng mãi ⟹ trần, không tràn số.
    assert!(!pin_bg_rebuild_allowed(
        Some((at, 40)),
        at + PIN_BG_REBUILD_GAP_MAX_SEC - 1
    ));
    assert!(pin_bg_rebuild_allowed(
        Some((at, u32::MAX)),
        at + PIN_BG_REBUILD_GAP_MAX_SEC
    ));
}

// ── Sổ gim nhớ mốc GỬI, và lượt SỬA không được đổi nó ────────────────────────

#[test]
fn the_pin_book_keeps_the_send_time_across_edits() {
    let (_dir, db) = db();
    pin_book(&db, 900, "👁 [huba] (acc3)");
    let d = pinned_sent_at(&db).expect("tin vừa gửi phải có mốc gửi");
    let now = chrono::Utc::now().timestamp();
    assert!(
        (now - d).abs() <= 5,
        "mốc gửi phải là BÂY GIỜ, được {d} (now {now})"
    );

    // Sổ giả mốc cũ rồi SỬA chữ của CÙNG tin ⟹ mốc giữ nguyên.
    db.set_cursor(PIN_FOLLOWING_KEY, r#"{"m":900,"t":"x","d":1234}"#)
        .expect("set");
    pin_book(&db, 900, "⚡ [huba] (acc3)");
    assert_eq!(
        pinned_sent_at(&db),
        Some(1234),
        "sửa chữ không được làm tin trẻ lại"
    );
    assert_eq!(
        pinned_message(&db),
        Some((900, "⚡ [huba] (acc3)".to_string()))
    );

    // Tin KHÁC ⟹ mốc mới.
    pin_book(&db, 901, "⚡ [huba] (acc3)");
    let d2 = pinned_sent_at(&db).expect("tin mới phải có mốc");
    assert!(d2 > 1234);
}

#[test]
fn old_books_have_no_send_time_and_still_read() {
    let (_dir, db) = db();
    // Số trần (sổ 25/08) và JSON chưa có `d` (sổ trước 16/10 — đúng giá trị
    // đọc được trên máy Hà lúc 04:20:35Z).
    for cu in [
        "12071",
        r#"{"m":56699,"t":"⚡ [remote] (acc6)"}"#,
        r#"{"m":5,"t":"x","d":0}"#,
    ] {
        db.set_cursor(PIN_FOLLOWING_KEY, cu).expect("set");
        assert_eq!(pinned_sent_at(&db), None, "{cu}");
        assert!(
            pinned_message(&db).is_some(),
            "{cu} vẫn phải đọc ra tin gim"
        );
    }
    // Một lượt SỬA trên sổ cũ không bịa ra mốc gửi.
    db.set_cursor(PIN_FOLLOWING_KEY, r#"{"m":56699,"t":"⚡ [remote] (acc6)"}"#)
        .expect("set");
    pin_book(&db, 56699, "💤 [remote] (acc6)");
    assert_eq!(pinned_sent_at(&db), None);
}

// ── Câu trả lời cho `/start` trần ────────────────────────────────────────────

#[test]
fn bare_start_says_what_was_done_before_any_guess_about_a_lost_tap() {
    let r = bare_start_reply(Some("[remote] (acc6)"), true, true);
    assert!(
        r.starts_with("↻ Đã gim lại phiên đang theo: [remote] (acc6)"),
        "{r}"
    );
    // Câu cũ phán thẳng "cú chạm ấy tới nơi nhưng KHÔNG mang theo lựa chọn nào"
    // với người chỉ vừa bấm START. Nay hướng dẫn ấy chỉ còn ở dạng CÓ ĐIỀU KIỆN.
    assert!(!r.contains("Cú chạm ấy tới nơi nhưng"), "{r}");
    assert!(r.contains("Vừa chạm một ☑ trong chữ?"), "{r}");
}

#[test]
fn bare_start_without_a_pin_says_why() {
    let khong_theo = bare_start_reply(None, false, true);
    assert!(
        khong_theo.starts_with("Chưa theo phiên nào"),
        "{khong_theo}"
    );
    let hong = bare_start_reply(None, true, true);
    assert!(hong.starts_with("⚠ Chưa gim lại được"), "{hong}");
    // Tắt cờ gim ⟹ không nhắc gì tới gim — chỉ còn hướng dẫn.
    let tat = bare_start_reply(None, true, false);
    assert!(tat.starts_with("Vừa chạm một ☑"), "{tat}");
    assert!(!tat.contains("gim"), "{tat}");
    // Mọi nhánh đều giữ đường cho nghĩa thứ hai (deep link mất payload).
    for r in [&khong_theo, &hong, &tat] {
        assert!(r.contains("/pick <câu>.<ô>"), "{r}");
    }
}

// ── Chỗ nối: soi nguồn, kèm đối chứng ngược ──────────────────────────────────

fn src() -> String {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs");
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: {p}: {e}"))
}

/// Thân một hàm ở cột 0: từ mỏ neo tới dòng `}` đầu tiên ở cột 0. `None` = mất
/// mỏ neo ⟹ ĐỎ (không được đọc thành "không có lời gọi nào phải kiểm").
fn than<'a>(src: &'a str, neo: &str) -> Option<&'a str> {
    let a = src.find(neo)?;
    let rest = &src[a..];
    let b = rest.find("\n}\n")?;
    Some(&rest[..b])
}

/// Nhánh `/start` trần trong lô lệnh Telegram: từ phép so tới `continue;` đầu.
fn nhanh_start(src: &str) -> Option<&str> {
    let a = src.find(r#"if item.text.trim() == "/start" {"#)?;
    let rest = &src[a..];
    let b = rest.find("continue;")?;
    Some(&rest[..b])
}

fn noi_dung(src: &str) -> Result<(), String> {
    let start = nhanh_start(src).ok_or("mất mỏ neo nhánh `/start` trần")?;
    if !start.contains("on_bare_start(") {
        return Err("nhánh `/start` trần không gọi on_bare_start".into());
    }
    let on = than(src, "fn on_bare_start(").ok_or("mất hàm on_bare_start")?;
    if !on.contains("pin_rebuild(") {
        return Err("on_bare_start không dựng lại tin gim".into());
    }
    let rp = than(src, "pub fn refresh_pin(").ok_or("mất hàm refresh_pin")?;
    if !(rp.contains("pin_due_for_renew(") && rp.contains("pin_rebuild_bg(")) {
        return Err("refresh_pin không thay tin gim sắp tới hạn".into());
    }
    let ap = than(src, "fn pin_apply(").ok_or("mất hàm pin_apply")?;
    if !(ap.contains("pin_is_gone(") && ap.contains("pin_rebuild_bg(")) {
        return Err("pin_apply không dựng lại tin gim đã mất".into());
    }
    let bg = than(src, "fn pin_rebuild_bg(").ok_or("mất hàm pin_rebuild_bg")?;
    if !(bg.contains("pin_bg_rebuild_allowed(")
        && bg.contains("pin_rebuild(db, tg, sid, line, true,"))
    {
        return Err("vòng nền dựng lại mà không qua nhịp, hoặc không im lặng".into());
    }
    Ok(())
}

#[test]
fn every_rebuild_path_is_wired() {
    let s = src();
    if let Err(e) = noi_dung(&s) {
        panic!("{e}");
    }
}

/// Đối chứng ngược: gỡ từng chỗ nối khỏi BẢN SAO nguồn ⟹ phép soi phải ĐỎ,
/// đúng ở chỗ đã gỡ. Không có bài này thì `every_rebuild_path_is_wired` có thể
/// xanh vì neo sai chỗ.
#[test]
fn removing_any_wire_turns_the_check_red() {
    let s = src();
    let ca = [
        (
            "on_bare_start(db, cfg, inbox);",
            "",
            "không gọi on_bare_start",
        ),
        (
            "if pin_due_for_renew(pinned_sent_at(db), now, dong_ho) {",
            "if false {",
            "refresh_pin không thay",
        ),
        (
            "return pin_rebuild_bg(db, tg, sid, line, VI_SAO_DA_MAT);",
            "return None;",
            "pin_apply không dựng lại",
        ),
        (
            "if !pin_bg_rebuild_allowed(*slot, now) {",
            "if false {",
            "không qua nhịp",
        ),
        (
            r#"if item.text.trim() == "/start" {"#,
            r#"if item.text.trim() == "/batdau" {"#,
            "mất mỏ neo",
        ),
    ];
    for (goc, thay, mong) in ca {
        assert!(
            s.contains(goc),
            "đối chứng không tìm thấy chuỗi để gỡ: {goc}"
        );
        let hong = s.replacen(goc, thay, 1);
        match noi_dung(&hong) {
            Ok(()) => panic!("gỡ {goc:?} mà phép soi vẫn XANH — nó mù"),
            Err(e) => assert!(e.contains(mong), "gỡ {goc:?} ⟹ đỏ SAI CHỖ: {e}"),
        }
    }
}
