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
use huba::pipeline::accounts_order;
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
fn cu_phap_ten_tran_va_detail_cu() {
    use huba::pipeline::AccountsOrder::*;
    // Dạng mới (Hà 10/10): tên trần = xem chi tiết — kể cả tên lạ, để route trả
    // DANH SÁCH tài khoản thay vì câu hướng dẫn.
    assert_eq!(accounts_order("acc3"), Detail("acc3".into()));
    assert_eq!(accounts_order("acc9"), Detail("acc9".into()));
    // Dạng cũ vẫn nhận.
    assert_eq!(accounts_order("detail acc3"), Detail("acc3".into()));
    assert_eq!(accounts_order("chitiet acc3"), Detail("acc3".into()));
    // Vế ngược: thiếu tên / thừa chữ ⟹ hướng dẫn.
    assert_eq!(accounts_order("detail"), Usage);
    assert_eq!(accounts_order("detail acc3 thêm"), Usage);

    let (k, _, arg) = huba::verbs::parse_command("/accounts acc3").unwrap();
    assert_eq!(k, huba::adapters::CommandKind::Accounts);
    assert_eq!(arg, "acc3");
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
        None,
        Ok(id),
        &dead,
        BAY_GIO,
    );
    assert!(t.contains("email: trogiup.gdk@gmail.com"), "{t}");
    assert!(t.contains("tổ chức trogiup's Organization"), "{t}");
    assert!(t.contains("🔒 ĐANG KHOÁ"), "{t}");
    assert!(t.contains("/accounts acc3 -a"), "{t}");
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
        None,
        Ok(identity_mau()),
        &dead,
        BAY_GIO,
    );
    assert!(!mo.contains("ĐANG KHOÁ"), "{mo}");
    assert!(mo.contains("/accounts acc3 -b"), "{mo}");
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
        None,
        Ok(identity_mau()),
        &dead,
        BAY_GIO,
    );
    assert!(la.contains("Không có tài khoản 'acc9'"), "{la}");
    assert!(la.contains("acc1 · acc3"), "{la}");
}

/// `claude` GIẢ: mỗi lần bị gọi ghi một dòng vào `<config_dir>/goi.log`, rồi in câu
/// `/usage` với tuần 42 % — cùng khuôn với `tests/usage_do_khi_chon.rs`.
fn claude_gia(dir: &Path) -> std::path::PathBuf {
    let p = dir.join("claude-gia.sh");
    std::fs::write(
        &p,
        "#!/bin/bash\n\
         echo x >> \"$CLAUDE_CONFIG_DIR/goi.log\"\n\
         printf '{\"result\":\"Current session: 3%% used\\\\nCurrent week (all models): 42%% used\"}\\n'\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

fn so_lan_goi(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("goi.log"))
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

/// Trọn đường: thư mục cấu hình thật (tạm) → sổ `.claude.json` → đo lại `/usage` → câu.
///
/// 🔴 Hà 2026-10-10: *"Lệnh `/accounts detail acc` vẫn phải đo lại được kể cả đang
/// khóa"*. Tài khoản ở đây ĐANG KHOÁ, và lượt dò phải chạy đúng MỘT lần: lần hỏi
/// thứ hai trong 5′ dùng lại số vừa đo (luật 5′ của `/accounts`, Hà 24/09).
#[test]
fn chi_tiet_do_lai_ca_tai_khoan_dang_khoa() {
    let d = tempfile::tempdir().unwrap();
    let acc3 = d.path().join("acc3");
    so(&acc3, SO_DAY_DU);
    let db = huba::db::Db::open(&d.path().join("huba.sqlite")).unwrap();
    let mut c = cfg_voi(Some(&acc3), true);
    c.claude_cli = claude_gia(d.path()).display().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let dead = std::collections::BTreeMap::new();

    let t = account_detail_say(&c, &db, &SessionsSnapshot::default(), "acc3", &dead, now);
    assert_eq!(so_lan_goi(&acc3), 1, "tài khoản khoá phải được đo lại: {t}");
    assert!(t.contains("🔒 ĐANG KHOÁ"), "{t}");
    assert!(t.contains("email: trogiup.gdk@gmail.com"), "{t}");
    assert!(
        t.contains("đo lại /usage (vừa đo): tuần 42% · 5 tiếng 3%"),
        "số vừa đo phải tới màn: {t}"
    );

    let t2 = account_detail_say(&c, &db, &SessionsSnapshot::default(), "acc3", &dead, now);
    assert_eq!(so_lan_goi(&acc3), 1, "số còn mới (<5′) mà vẫn đo lại: {t2}");
    assert!(t2.contains("tuần 42%"), "{t2}");

    // Vế ngược: đường TỰ chọn tài khoản vẫn không dò tài khoản khoá.
    let mut c_cu = c.clone();
    c_cu.claude_accounts.retain(|a| a.name == "acc3");
    let _ = huba::runtime::usage_lam_moi(
        &c_cu,
        &huba::db::Db::open(&d.path().join("khac.sqlite")).unwrap(),
        0,
    );
    assert_eq!(
        so_lan_goi(&acc3),
        1,
        "usage_lam_moi (tự chọn) đã dò tài khoản khoá"
    );
}

#[test]
fn dong_do_lai_chi_noi_khi_moi_hon_so_cli_va_noi_ra_khi_hong() {
    use huba::runtime::do_lai_line;
    let now = 1_790_000_000_000;
    let row = serde_json::json!({ "week_pct": 42, "session_pct": 3,
        "week_models": [{ "name": "Fable", "pct": 7 }], "at_ms": now - 3 * 60_000 });
    let l = do_lai_line(Some(&row), Some(now - 3_600_000), now).unwrap();
    assert!(
        l.contains("(3 phút trước): tuần 42% · 5 tiếng 3% · Fable 7%"),
        "{l}"
    );
    // Sổ CLI mới hơn lượt dò ⟹ dòng `hạn mức:` đã là số mới nhất ⟹ im.
    assert_eq!(do_lai_line(Some(&row), Some(now - 60_000), now), None);
    assert_eq!(do_lai_line(None, None, now), None);
    // Lượt dò gần nhất hỏng ⟹ nói ra, kể cả khi còn số cũ.
    let hong =
        serde_json::json!({ "err": "hết giờ 60 s", "week_pct": 40, "at_ms": now - 7_200_000 });
    let l = do_lai_line(Some(&hong), None, now).unwrap();
    assert!(l.contains("lượt gần nhất HỎNG — hết giờ 60 s"), "{l}");
    assert!(l.contains("(2 tiếng trước): tuần 40%"), "{l}");
}
