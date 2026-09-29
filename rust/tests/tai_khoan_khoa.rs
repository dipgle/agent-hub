//! 🔒 Tài khoản KHOÁ thì huba không đưa vào sử dụng.
//!
//! Hà 2026-09-29: *"thêm thuộc tính khóa vào danh sách tài khoản, nếu đang khóa
//! thì không đưa vào sử dụng"*. "Sử dụng" ở đây là MỌI đường huba làm một tài
//! khoản tiêu hạn mức: tự chọn nó (`/new` không `-a`, gợi ý khi bị chặn, tự
//! chuyển, bàn giao tự động), mở phiên trên nó khi chủ máy gõ `-a`, và dò
//! `/usage` bằng nó. Liệt kê phiên ĐANG chạy trên nó thì vẫn làm — nhìn không
//! phải là dùng.
//!
//! Mỗi bài dưới đây chấm một cửa, và mỗi bài có vế NGƯỢC (cùng tài khoản, mở
//! khoá ⟹ được dùng lại), để một bài xanh không thể xanh vì cửa chặn tất cả.

mod common;

use std::path::{Path, PathBuf};

use huba::config::{ClaudeAccountCfg, Config};
use huba::db::Db;
use huba::quota::{overlay_live, rank_all, Rank, Ranked};
use huba::sessions::{account_locked_text, ensure_account_usable, SessionsSnapshot};
use huba::watch::suggest_account;

const LUC_12_45: u64 = 12 * 60 + 45;

fn hang(ten: &str, locked: bool, dir: Option<&Path>) -> ClaudeAccountCfg {
    ClaudeAccountCfg {
        name: ten.into(),
        config_dir: dir.map(|d| d.display().to_string()),
        locked,
        ..Default::default()
    }
}

/// acc1 = mặc định (không `config_dir`), acc2/acc3 có thư mục riêng.
fn cfg_ba(khoa: &[&str]) -> Config {
    let mut c = common::cfg_for_tests();
    c.claude_accounts = vec![
        hang("acc1", khoa.contains(&"acc1"), None),
        ClaudeAccountCfg {
            config_dir: Some("~/.claude-acc2".into()),
            ..hang("acc2", khoa.contains(&"acc2"), None)
        },
        ClaudeAccountCfg {
            config_dir: Some("~/.claude-acc3".into()),
            ..hang("acc3", khoa.contains(&"acc3"), None)
        },
    ];
    c
}

// ── Cấu hình ────────────────────────────────────────────────────────────────

/// Cấu hình cũ (không có trường) đọc ra `false`; `false` không ghi ra tệp, nên
/// một lượt `/set` bất kỳ không làm mọc thêm `"locked": false` vào mọi hàng.
#[test]
fn truong_locked_mac_dinh_false_va_false_khong_ghi_ra_tep() {
    let cu: ClaudeAccountCfg =
        serde_json::from_str(r#"{"name":"acc3","config_dir":"~/.claude-acc3"}"#).unwrap();
    assert!(!cu.locked);
    let khoa: ClaudeAccountCfg = serde_json::from_str(r#"{"name":"acc3","locked":true}"#).unwrap();
    assert!(khoa.locked);

    let ra_false = serde_json::to_string(&hang("acc3", false, None)).unwrap();
    assert!(ra_false.contains("\"name\""), "{ra_false}");
    assert!(!ra_false.contains("locked"), "false bị ghi ra: {ra_false}");
    let ra_true = serde_json::to_string(&hang("acc3", true, None)).unwrap();
    assert!(ra_true.contains("\"locked\":true"), "{ra_true}");
}

#[test]
fn account_locked_va_ten_mac_dinh() {
    let c = cfg_ba(&["acc2"]);
    assert!(c.account_locked("acc2"));
    assert!(!c.account_locked("acc3"));
    assert!(!c.account_locked("acc9"), "tên lạ không phải 'đang khoá'");
    assert_eq!(c.default_account_name().as_deref(), Some("acc1"));
}

// ── Xếp hạng + tự chọn ─────────────────────────────────────────────────────

/// `rank_all` là cửa chung của MỌI đường tự chọn — khoá phải đóng dấu ở đây.
#[test]
fn rank_all_dong_dau_locked_chi_cho_tai_khoan_khoa() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = common::cfg_for_tests();
    c.claude_accounts = vec![
        hang("a", true, Some(&dir.path().join("a"))),
        hang("b", false, Some(&dir.path().join("b"))),
    ];
    let r = rank_all(&c, 1_790_000_000_000);
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].rank, Rank::Locked, "{r:?}");
    assert_ne!(r[1].rank, Rank::Locked, "{r:?}");
    assert_eq!(Rank::Locked.say(), "ĐANG KHOÁ");
}

/// Phép dò sống đọc ra 0% cũng KHÔNG được mở lại một tài khoản chủ máy khoá.
#[test]
fn so_do_song_khong_mo_lai_tai_khoan_khoa() {
    let vao = vec![
        Ranked {
            name: "a".into(),
            rank: Rank::Locked,
        },
        Ranked {
            name: "b".into(),
            rank: Rank::Unknown,
        },
    ];
    let song = serde_json::json!({
        "a": { "session_pct": 0, "week_pct": 0 },
        "b": { "session_pct": 0, "week_pct": 0 },
    });
    let ra = overlay_live(vao, &song);
    assert_eq!(ra[0].rank, Rank::Locked, "số sống đè mất khoá: {ra:?}");
    assert_eq!(
        ra[1].rank,
        Rank::Free(0),
        "vế ngược: tài khoản mở thì số sống vẫn đè"
    );
}

#[test]
fn goi_y_khong_bao_gio_chon_tai_khoan_khoa() {
    // `a` là cái rộng cửa nhất nếu không khoá.
    let khoa_a = vec![
        Ranked {
            name: "a".into(),
            rank: Rank::Locked,
        },
        Ranked {
            name: "b".into(),
            rank: Rank::Free(60),
        },
    ];
    assert_eq!(
        suggest_account("", &khoa_a, &[], LUC_12_45).as_deref(),
        Some("b")
    );
    let chi_con_a = vec![Ranked {
        name: "a".into(),
        rank: Rank::Locked,
    }];
    assert_eq!(suggest_account("", &chi_con_a, &[], LUC_12_45), None);
    // Vế ngược: cùng dữ liệu, a mở khoá ⟹ a được chọn (0% < 60%).
    let mo_a = vec![
        Ranked {
            name: "a".into(),
            rank: Rank::Free(0),
        },
        Ranked {
            name: "b".into(),
            rank: Rank::Free(60),
        },
    ];
    assert_eq!(
        suggest_account("", &mo_a, &[], LUC_12_45).as_deref(),
        Some("a")
    );
}

// ── Cửa chặn mở phiên ───────────────────────────────────────────────────────

#[test]
fn cua_mo_phien_chan_tai_khoan_khoa_ke_ca_mac_dinh() {
    let c = cfg_ba(&["acc2"]);
    let e = ensure_account_usable(&c, Some("acc2"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("acc2 đang KHOÁ"), "{e}");
    assert!(
        e.contains("/accounts mo acc2"),
        "câu từ chối phải chỉ đường mở: {e}"
    );
    assert!(
        e.contains("acc1 · acc3"),
        "phải kể tài khoản còn dùng được: {e}"
    );
    assert!(ensure_account_usable(&c, Some("acc3")).is_ok());
    assert!(
        ensure_account_usable(&c, None).is_ok(),
        "mặc định acc1 đang mở"
    );

    // `None` = tài khoản mặc định — khoá acc1 thì `None` cũng bị chặn.
    let c1 = cfg_ba(&["acc1"]);
    let e1 = ensure_account_usable(&c1, None).unwrap_err().to_string();
    assert!(e1.contains("acc1 đang KHOÁ"), "{e1}");
}

/// Cửa phải đứng TRONG `start_background` (chỗ thắt của `/new`), không chỉ ở
/// route. Thư mục không tồn tại để vế NGƯỢC dừng ở phép kiểm thư mục ngay sau
/// cửa — tức không bài nào ở đây mở một cửa sổ Terminal thật.
#[test]
fn start_background_chan_tai_khoan_khoa_truoc_moi_viec() {
    let c = cfg_ba(&["acc2"]);
    let khong_co = Path::new("/khong/co/thu/muc/nay");
    let loi = |acc: &str| match huba::sessions::start_background(
        &c,
        "x",
        khong_co,
        "việc",
        Some(acc),
        None,
    ) {
        Ok(_) => panic!("{acc}: start_background không từ chối"),
        Err(e) => e.to_string(),
    };
    let e = loi("acc2");
    assert!(e.contains("acc2 đang KHOÁ"), "{e}");
    // Vế ngược: tài khoản mở ⟹ qua cửa, rơi xuống phép kiểm thư mục.
    let e = loi("acc3");
    assert!(e.contains("không thấy thư mục dự án"), "{e}");
}

#[test]
fn khoa_het_thi_cau_tu_choi_noi_thang() {
    let c = cfg_ba(&["acc1", "acc2", "acc3"]);
    let t = account_locked_text(&c, "acc3");
    assert!(t.contains("Không còn tài khoản nào đang mở"), "{t}");
}

// ── Khoá / mở từ điện thoại ─────────────────────────────────────────────────

#[test]
fn route_accounts_nhan_khoa_mo() {
    use huba::adapters::CommandKind;
    use huba::pipeline::account_lock_order;
    use huba::verbs::parse_command;

    let (k, _, arg) = parse_command("/accounts khoa acc3").unwrap();
    assert_eq!(k, CommandKind::Accounts);
    assert_eq!(arg, "khoa acc3");
    let (_, _, trong) = parse_command("/accounts").unwrap();
    assert!(trong.is_empty(), "gõ trơn vẫn là XEM");

    assert_eq!(account_lock_order("khoa acc3"), Some(("acc3".into(), true)));
    assert_eq!(account_lock_order("khoá acc3"), Some(("acc3".into(), true)));
    assert_eq!(account_lock_order("mo acc3"), Some(("acc3".into(), false)));
    assert_eq!(account_lock_order("mở acc3"), Some(("acc3".into(), false)));
    assert_eq!(account_lock_order("khoa"), None, "thiếu tên");
    assert_eq!(
        account_lock_order("xoa acc3"),
        None,
        "động từ lạ không được đoán"
    );
    assert_eq!(
        account_lock_order("khoa acc3 acc4"),
        None,
        "một lệnh một tài khoản"
    );
}

fn tep_cau_hinh(dir: &Path) -> Config {
    let path = dir.join("huba.config.json");
    let mut c = cfg_ba(&[]);
    c.config_file = path.clone();
    c.hub_home = dir.to_path_buf();
    huba::config::save(&c).unwrap();
    c
}

fn doc_hang(path: &Path, ten: &str) -> serde_json::Value {
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v["claude_accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == ten)
        .cloned()
        .unwrap()
}

#[test]
fn khoa_roi_mo_ghi_dung_mot_hang_trong_tep() {
    let dir = tempfile::tempdir().unwrap();
    let c = tep_cau_hinh(dir.path());
    let path = c.config_file.clone();

    let msg = huba::pipeline::set_account_locked(&c, "acc3", true).unwrap();
    assert!(msg.contains("acc3 đã KHOÁ"), "{msg}");
    assert_eq!(doc_hang(&path, "acc3")["locked"], true);
    assert!(
        doc_hang(&path, "acc2").get("locked").is_none(),
        "đụng nhầm hàng khác"
    );
    let nap_lai = huba::config::load(Some(&path)).unwrap();
    assert!(
        nap_lai.account_locked("acc3"),
        "nạp lại tệp không thấy khoá"
    );

    let msg = huba::pipeline::set_account_locked(&c, "acc3", false).unwrap();
    assert!(msg.contains("acc3 đã MỞ"), "{msg}");
    assert!(
        doc_hang(&path, "acc3").get("locked").is_none(),
        "mở khoá xong phải về đúng hình dạng cũ"
    );

    let e = huba::pipeline::set_account_locked(&c, "acc9", true)
        .unwrap_err()
        .to_string();
    assert!(e.contains("không có tài khoản 'acc9'"), "{e}");
    assert!(e.contains("acc1 · acc2 · acc3"), "{e}");
}

// ── /accounts nói ra ────────────────────────────────────────────────────────

#[test]
fn accounts_noi_ra_tai_khoan_nao_dang_khoa() {
    let snap = SessionsSnapshot::default();
    let dead = std::collections::BTreeMap::new();
    let t = huba::runtime::accounts_text(
        &cfg_ba(&["acc3"]),
        &snap,
        &serde_json::json!({}),
        &[],
        &dead,
        1_790_000_000_000,
    );
    assert!(t.contains("🔒 1 đang khoá"), "{t}");
    assert!(t.contains("🔒 ĐANG KHOÁ"), "{t}");
    assert!(t.contains("/accounts mo acc3"), "{t}");
    assert_eq!(t.matches("🔒 ĐANG KHOÁ").count(), 1, "chỉ acc3 khoá: {t}");

    let mo = huba::runtime::accounts_text(
        &cfg_ba(&[]),
        &snap,
        &serde_json::json!({}),
        &[],
        &dead,
        1_790_000_000_000,
    );
    assert!(!mo.contains('🔒'), "không khoá gì mà vẫn in dấu khoá: {mo}");
}

// ── /usage không dò bằng tài khoản khoá ────────────────────────────────────

/// `claude` GIẢ — cùng khuôn với `usage_do_khi_chon.rs`: mỗi lượt gọi ghi một
/// dòng vào `<config_dir>/goi.log`, nên đếm được tài khoản nào đã bị dùng.
fn claude_gia(dir: &Path) -> PathBuf {
    let p = dir.join("claude-gia.sh");
    std::fs::write(
        &p,
        "#!/bin/bash\n\
         echo x >> \"$CLAUDE_CONFIG_DIR/goi.log\"\n\
         printf '{\"result\":\"Current session: 3%% used\\\\nCurrent week (all models): 40%% used\"}\\n'\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    p
}

fn so_lan_goi(dir: &Path, acc: &str) -> usize {
    std::fs::read_to_string(dir.join(acc).join("goi.log"))
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

#[test]
fn usage_khong_do_bang_tai_khoan_khoa() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("huba.sqlite")).unwrap();
    let mut c = common::cfg_for_tests();
    c.claude_cli = claude_gia(dir.path()).display().to_string();
    c.claude_accounts = ["a", "b"]
        .iter()
        .map(|n| {
            let d = dir.path().join(n);
            std::fs::create_dir_all(&d).unwrap();
            hang(n, *n == "a", Some(&d))
        })
        .collect();
    let v = huba::runtime::usage_lam_moi(&c, &db, huba::runtime::USAGE_CU_MO_PHIEN_MS);
    assert_eq!(
        (so_lan_goi(dir.path(), "a"), so_lan_goi(dir.path(), "b")),
        (0, 1),
        "tài khoản khoá vẫn bị gọi `claude -p /usage`: {v}"
    );
    // Vế ngược: mở khoá ⟹ lượt sau đo nó (số của nó chưa từng có ⟹ phải đo).
    c.claude_accounts[0].locked = false;
    huba::runtime::usage_lam_moi(&c, &db, huba::runtime::USAGE_CU_MO_PHIEN_MS);
    assert_eq!(
        so_lan_goi(dir.path(), "a"),
        1,
        "mở khoá rồi mà vẫn không đo"
    );
}
