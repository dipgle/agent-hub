//! Hai thư mục cấu hình đăng nhập cùng MỘT tài khoản thì phải có người NÓI ra.
//!
//! 🔴 Hà 2026-10-19: *"Tại sao acc3 và acc5 lại là 1"*. Đo cùng ngày:
//! `~/.claude-acc3/.claude.json` và `~/.claude-acc5/.claude.json` cùng
//! `oauthAccount.accountUuid 29829ac3…`, cùng org `481fe180…`, cùng
//! `trogiup.gdk@gmail.com` ⟹ một tài khoản, một hạn mức, hai hàng số y hệt.
//!
//! Gốc: lượt `/accounts acc5 -l` ngày 13/10 00:20Z. huba điền sẵn
//! `help.me002022@gmail.com` (log: `login_hint=help.me002022%40gmail.com`, câu dẫn
//! *"Đăng nhập ĐÚNG help.me002022@gmail.com"*), CLI in «Login successful.», và huba
//! đáp đúng câu ấy rồi thôi. `login_hint` chỉ là gợi ý — trang dùng tài khoản đang mở
//! trên trình duyệt điện thoại. Sáu ngày không câu nào nói acc5 đã thành acc3.

use huba::account_login::{canh_bao_danh_tinh, nhom_trung, trung_line, trung_voi};

/// Đúng bảng đo được trên máy Hà lúc 2026-10-19 ~11:00 (acc1 không có sổ).
fn may_ha_19_10() -> Vec<(String, Option<String>)> {
    [
        ("acc1", None),
        ("acc2", Some("nguyenha.momochan@gmail.com")),
        ("acc3", Some("trogiup.gdk@gmail.com")),
        ("acc4", Some("vuid.gdk@gmail.com")),
        ("acc5", Some("trogiup.gdk@gmail.com")),
        ("acc6", Some("gdk.vui@gmail.com")),
        ("acc7", Some("developer.tvv@gmail.com")),
    ]
    .into_iter()
    .map(|(n, e)| (n.to_string(), e.map(str::to_string)))
    .collect()
}

#[test]
fn the_real_wrong_login_of_13_10_is_named_as_such() {
    let s = canh_bao_danh_tinh(
        "acc5",
        Some("help.me002022@gmail.com"),
        Some("trogiup.gdk@gmail.com"),
        &["acc3".to_string()],
    );
    assert!(s.starts_with("tài khoản: trogiup.gdk@gmail.com\n"), "{s}");
    assert!(s.contains("KHÔNG phải help.me002022@gmail.com"), "{s}");
    assert!(s.contains("acc5 và acc3 là MỘT tài khoản"), "{s}");
    assert!(
        s.contains("/accounts acc5 -l"),
        "phải chỉ đúng tài khoản cần làm lại: {s}"
    );
}

#[test]
fn a_right_login_says_who_and_nothing_else() {
    let s = canh_bao_danh_tinh(
        "acc5",
        Some("help.me002022@gmail.com"),
        Some("Help.Me002022@gmail.com"),
        &[],
    );
    assert_eq!(s, "tài khoản: Help.Me002022@gmail.com\n");
    // Không điền sẵn (sổ chưa từng đăng nhập) — vẫn nói ai, không kêu.
    assert_eq!(
        canh_bao_danh_tinh("acc8", None, Some("x@y.z"), &[]),
        "tài khoản: x@y.z\n"
    );
}

#[test]
fn no_email_in_the_book_is_not_read_as_success() {
    let s = canh_bao_danh_tinh("acc5", Some("help.me002022@gmail.com"), None, &[]);
    assert!(s.contains("CHƯA xác nhận"), "{s}");
}

#[test]
fn a_right_login_that_collides_with_another_does_not_blame_itself() {
    // acc3 đăng nhập lại ĐÚNG trogiup, nhưng acc5 đang nhầm sang trogiup.
    let s = canh_bao_danh_tinh(
        "acc3",
        Some("trogiup.gdk@gmail.com"),
        Some("trogiup.gdk@gmail.com"),
        &["acc5".to_string()],
    );
    assert!(s.contains("acc3 và acc5 là MỘT tài khoản"), "{s}");
    assert!(!s.contains("KHÔNG phải"), "{s}");
    assert!(
        !s.contains("/accounts acc3 -l"),
        "acc3 đúng người — đừng bảo làm lại acc3: {s}"
    );
}

#[test]
fn duplicates_are_found_on_the_real_table() {
    let ids = may_ha_19_10();
    assert_eq!(
        nhom_trung(&ids),
        vec![(
            "trogiup.gdk@gmail.com".to_string(),
            vec!["acc3".to_string(), "acc5".to_string()]
        )]
    );
    assert_eq!(
        trung_voi(&ids, "acc5", "TROGIUP.GDK@gmail.com"),
        vec!["acc3"]
    );
    assert!(trung_voi(&ids, "acc6", "gdk.vui@gmail.com").is_empty());
    let dong = trung_line(&nhom_trung(&ids)).expect("có nhóm trùng ⟹ phải có dòng");
    assert!(dong.contains("acc3 · acc5"), "{dong}");
}

#[test]
fn accounts_without_a_book_never_group_together() {
    let ids: Vec<(String, Option<String>)> = vec![
        ("acc1".into(), None),
        ("acc8".into(), None),
        ("acc9".into(), Some(String::new())),
    ];
    assert!(nhom_trung(&ids).is_empty());
    assert_eq!(trung_line(&[]), None);
}

// ── Chỗ nối: soi nguồn, kèm đối chứng ngược ──────────────────────────────────

fn doc(rel: &str) -> String {
    let p = format!("{}/{rel}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: {p}: {e}"))
}

/// Thân hàm ở cột 0, tới dòng `}` đầu tiên ở cột 0. `None` = mất mỏ neo ⟹ ĐỎ.
fn than<'a>(src: &'a str, neo: &str) -> Option<&'a str> {
    let a = src.find(neo)?;
    let rest = &src[a..];
    Some(&rest[..rest.find("\n}\n")?])
}

fn noi(login: &str, runtime: &str) -> Result<(), String> {
    let fin = than(login, "pub fn finish(").ok_or("mất hàm finish")?;
    let xong = &fin[fin.find("Ket::Xong(").ok_or("mất nhánh Ket::Xong")?..];
    if !(xong.contains("canh_bao_danh_tinh(") && xong.contains("{ai}")) {
        return Err("lượt xong không nói tài khoản vừa vào là ai".into());
    }
    let st = than(login, "pub fn start(").ok_or("mất hàm start")?;
    if !st.contains("\"email\": email") {
        return Err("lượt mở không ghi hòm thư điền sẵn".into());
    }
    let ds = than(runtime, "pub fn accounts_say(").ok_or("mất accounts_say")?;
    if !ds.contains("trung_line(") {
        return Err("/accounts không báo trùng".into());
    }
    let ct = than(runtime, "pub fn account_detail_say(").ok_or("mất account_detail_say")?;
    if !ct.contains("trung_voi(") {
        return Err("/accounts <tên> không báo trùng".into());
    }
    Ok(())
}

#[test]
fn every_identity_check_is_wired() {
    if let Err(e) = noi(&doc("src/account_login.rs"), &doc("src/runtime.rs")) {
        panic!("{e}");
    }
}

#[test]
fn removing_any_wire_turns_the_check_red() {
    let login = doc("src/account_login.rs");
    let runtime = doc("src/runtime.rs");
    let ca_login = [
        (
            "{ai}{do_lai}{dong}",
            "{do_lai}{dong}",
            "không nói tài khoản",
        ),
        ("\"email\": email", "\"e\": email", "không ghi hòm thư"),
    ];
    for (goc, thay, mong) in ca_login {
        assert!(
            login.contains(goc),
            "đối chứng không thấy chuỗi để gỡ: {goc}"
        );
        match noi(&login.replacen(goc, thay, 1), &runtime) {
            Ok(()) => panic!("gỡ {goc:?} mà vẫn XANH — phép soi mù"),
            Err(e) => assert!(e.contains(mong), "gỡ {goc:?} ⟹ đỏ SAI CHỖ: {e}"),
        }
    }
    let ca_runtime = [
        (
            "account_login::trung_line(",
            "account_login::khong(",
            "/accounts không báo",
        ),
        (
            "account_login::trung_voi(",
            "account_login::khong(",
            "/accounts <tên> không",
        ),
    ];
    for (goc, thay, mong) in ca_runtime {
        assert!(
            runtime.contains(goc),
            "đối chứng không thấy chuỗi để gỡ: {goc}"
        );
        match noi(&login, &runtime.replacen(goc, thay, 1)) {
            Ok(()) => panic!("gỡ {goc:?} mà vẫn XANH — phép soi mù"),
            Err(e) => assert!(e.contains(mong), "gỡ {goc:?} ⟹ đỏ SAI CHỖ: {e}"),
        }
    }
}
