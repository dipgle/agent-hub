//! `/accounts detail <tên>` — xem MỘT tài khoản, kể cả email.
//!
//! Hà 2026-09-29: *"Lệnh nào để xem chi tiết thông tin một acc? Bao gồm cả
//! email? Ví dụ `/accounts detail acc3`"*. Trước lượt này không có đường nào:
//! email chỉ từng được đọc trong ảnh chụp của trang tfl5 đã gỡ.
//!
//! Danh tính đọc từ `oauthAccount` trong sổ `.claude.json` — đọc TỆP, không
//! spawn `claude`, nên xem được cả tài khoản đang khoá mà không tiêu hạn mức.

mod common;

use std::path::Path;

use huba::config::{ClaudeAccountCfg, Config};
use huba::pipeline::account_detail_order;
use huba::quota::{identity, Identity};
use huba::runtime::{account_detail_say, account_detail_text};
use huba::sessions::{LiveSession, SessionsSnapshot};

const BAY_GIO: i64 = 1_790_000_000_000;

fn so(dir: &Path, noi_dung: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(".claude.json"), noi_dung).unwrap();
}

const SO_DAY_DU: &str = r#"{"oauthAccount":{"emailAddress":"trogiup.gdk@gmail.com",
    "displayName":"Dwork","organizationName":"trogiup's Organization",
    "organizationRole":"admin","billingType":"stripe_subscription"}}"#;

fn cfg_voi(dir3: Option<&Path>, khoa3: bool) -> Config {
    let mut c = common::cfg_for_tests();
    c.claude_accounts = vec![
        ClaudeAccountCfg {
            name: "acc1".into(),
            launch: Some("claude".into()),
            ..Default::default()
        },
        ClaudeAccountCfg {
            name: "acc3".into(),
            config_dir: Some(
                dir3.map(|d| d.display().to_string())
                    .unwrap_or_else(|| "~/.claude-acc3".into()),
            ),
            launch: Some("claude3".into()),
            locked: khoa3,
        },
    ];
    c
}

// ── Đọc danh tính ───────────────────────────────────────────────────────────

#[test]
fn identity_doc_du_nam_truong_tu_oauthaccount() {
    let d = tempfile::tempdir().unwrap();
    so(d.path(), SO_DAY_DU);
    let i = identity(Some(d.path())).unwrap();
    assert_eq!(
        i,
        Identity {
            email: Some("trogiup.gdk@gmail.com".into()),
            display_name: Some("Dwork".into()),
            org: Some("trogiup's Organization".into()),
            role: Some("admin".into()),
            billing: Some("stripe_subscription".into()),
        }
    );
}

/// Ba cách hỏng đo được là ba câu KHÁC nhau — không gộp thành "email trống".
#[test]
fn identity_noi_ro_vi_sao_khong_doc_duoc() {
    let d = tempfile::tempdir().unwrap();
    let e = identity(Some(&d.path().join("khong-co"))).unwrap_err();
    assert!(e.contains("chưa đăng nhập") && e.contains("chưa có"), "{e}");

    so(d.path(), r#"{"hasCompletedOnboarding":true}"#);
    let e = identity(Some(d.path())).unwrap_err();
    assert!(e.contains("không có oauthAccount"), "{e}");

    so(d.path(), "{ hỏng");
    let e = identity(Some(d.path())).unwrap_err();
    assert!(e.contains("hỏng JSON"), "{e}");
}

// ── Cú pháp ─────────────────────────────────────────────────────────────────

#[test]
fn cu_phap_detail_va_ten_tran() {
    let biet = vec!["acc1".to_string(), "acc3".to_string()];
    assert_eq!(
        account_detail_order("detail acc3", &biet).as_deref(),
        Some("acc3")
    );
    assert_eq!(
        account_detail_order("chitiet acc3", &biet).as_deref(),
        Some("acc3")
    );
    assert_eq!(account_detail_order("acc3", &biet).as_deref(), Some("acc3"));
    // Tên gõ sai sau `detail` vẫn tới được route — để route trả DANH SÁCH.
    assert_eq!(
        account_detail_order("detail acc9", &biet).as_deref(),
        Some("acc9")
    );
    // Vế ngược: tên trần lạ, hay thiếu tên, KHÔNG được đoán thành một lệnh xem.
    assert_eq!(account_detail_order("acc9", &biet), None);
    assert_eq!(account_detail_order("detail", &biet), None);
    assert_eq!(account_detail_order("khoa", &biet), None);
    assert_eq!(account_detail_order("detail acc3 thêm", &biet), None);

    let (k, _, arg) = huba::verbs::parse_command("/accounts detail acc3").unwrap();
    assert_eq!(k, huba::adapters::CommandKind::Accounts);
    assert_eq!(arg, "detail acc3");
}

// ── Câu trả lời ─────────────────────────────────────────────────────────────

fn phien(acc: &str, ten: &str, host: &str) -> LiveSession {
    LiveSession {
        account: acc.into(),
        name: ten.into(),
        host: host.into(),
        ..Default::default()
    }
}

#[test]
fn chi_tiet_in_email_khoa_va_phien_dang_chay() {
    let snap = SessionsSnapshot {
        sessions: vec![
            phien("acc3", "projects-1a", ""),
            phien("acc3", "projects-da-tat", "dead"),
            phien("acc1", "projects-khac", ""),
        ],
        ..Default::default()
    };
    let dead = std::collections::BTreeMap::new();
    let id = identity_mau();
    let t = account_detail_text(
        &cfg_voi(None, true),
        &snap,
        "acc3",
        None,
        Ok(id),
        &dead,
        BAY_GIO,
    );
    assert!(t.contains("email: trogiup.gdk@gmail.com"), "{t}");
    assert!(t.contains("tổ chức trogiup's Organization"), "{t}");
    assert!(t.contains("🔒 ĐANG KHOÁ"), "{t}");
    assert!(t.contains("/accounts mo acc3"), "{t}");
    assert!(t.contains("~/.claude-acc3") && t.contains("claude3"), "{t}");
    assert!(t.contains("phiên đang chạy (1): projects-1a"), "{t}");
    assert!(
        !t.contains("projects-da-tat"),
        "phiên đã tắt không phải đang chạy: {t}"
    );
    assert!(
        !t.contains("projects-khac"),
        "phiên của tài khoản khác: {t}"
    );

    // Vế ngược: mở khoá ⟹ không dấu khoá, chỉ đường KHOÁ.
    let mo = account_detail_text(
        &cfg_voi(None, false),
        &SessionsSnapshot::default(),
        "acc3",
        None,
        Ok(identity_mau()),
        &dead,
        BAY_GIO,
    );
    assert!(!mo.contains("ĐANG KHOÁ"), "{mo}");
    assert!(mo.contains("/accounts khoa acc3"), "{mo}");
    assert!(mo.contains("phiên đang chạy: không có"), "{mo}");
}

fn identity_mau() -> Identity {
    Identity {
        email: Some("trogiup.gdk@gmail.com".into()),
        org: Some("trogiup's Organization".into()),
        ..Default::default()
    }
}

#[test]
fn chi_tiet_noi_ro_khi_khong_doc_duoc_email_va_ten_la() {
    let dead = std::collections::BTreeMap::new();
    let snap = SessionsSnapshot::default();
    let t = account_detail_text(
        &cfg_voi(None, false),
        &snap,
        "acc3",
        None,
        Err("chưa đăng nhập (sổ không có oauthAccount)".into()),
        &dead,
        BAY_GIO,
    );
    assert!(
        t.contains("email: chưa đọc được — chưa đăng nhập"),
        "phải nói VÌ SAO không có email: {t}"
    );
    let la = account_detail_text(
        &cfg_voi(None, false),
        &snap,
        "acc9",
        None,
        Ok(identity_mau()),
        &dead,
        BAY_GIO,
    );
    assert!(la.contains("Không có tài khoản 'acc9'"), "{la}");
    assert!(la.contains("acc1 · acc3"), "{la}");
}

/// Trọn đường đọc TỆP: thư mục cấu hình thật (tạm) → sổ `.claude.json` → câu.
#[test]
fn chi_tiet_doc_that_tu_so_cua_tai_khoan() {
    let d = tempfile::tempdir().unwrap();
    so(d.path(), SO_DAY_DU);
    let t = account_detail_say(
        &cfg_voi(Some(d.path()), false),
        &SessionsSnapshot::default(),
        "acc3",
        &std::collections::BTreeMap::new(),
        BAY_GIO,
    );
    assert!(t.contains("email: trogiup.gdk@gmail.com"), "{t}");
    assert!(t.contains("vai trò admin"), "{t}");
    assert!(t.contains("hạn mức:"), "{t}");
}
