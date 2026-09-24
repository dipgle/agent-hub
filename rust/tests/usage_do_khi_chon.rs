//! Số hạn mức `/usage`: đo ĐÚNG LÚC SẮP CHỌN tài khoản, chỉ những tài khoản có số
//! cũ — không còn nhịp đo nền.
//!
//! 🔴 Hà 2026-09-24: *"trước khi mở phiên mới thì đo lại các phiên có lịch sử đo cũ
//! hơn 15 phút"* · *"riêng lệnh accounts thì cũ hơn 5 phút thì đo"*. Đo cùng ngày:
//! nhịp nền 5′ cũ là **1.127 lượt `claude -p /usage` trong một ngày** (6 tài khoản
//! nối đuôi, 49–80 s một lượt, cứ ~7′).
//!
//! Bài dựng một `claude` GIẢ (tệp shell): mỗi lần bị gọi nó ghi một dòng vào
//! `<config_dir>/goi.log` rồi in câu `/usage` với số tuần đọc từ `<config_dir>/pct`
//! — nên đo được ĐÚNG tài khoản nào bị đo lại, không đụng tài khoản thật của máy.

mod common;

use std::path::{Path, PathBuf};

use huba::config::{ClaudeAccountCfg, Config};
use huba::db::Db;
use huba::runtime::{
    can_do_lai, chi_so_moi_hon_tep, tuoi_so_do, usage_cached, usage_lam_moi, USAGE_CU_ACCOUNTS_MS,
    USAGE_CU_MO_PHIEN_MS, USAGE_KEY,
};

const PHUT: i64 = 60_000;

fn claude_gia(dir: &Path) -> PathBuf {
    let p = dir.join("claude-gia.sh");
    std::fs::write(
        &p,
        "#!/bin/bash\n\
         echo x >> \"$CLAUDE_CONFIG_DIR/goi.log\"\n\
         p=$(cat \"$CLAUDE_CONFIG_DIR/pct\" 2>/dev/null || echo 50)\n\
         printf '{\"result\":\"Current session: 3%% used\\\\nCurrent week (all models): %s%% used\"}\\n' \"$p\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    p
}

/// Hai tài khoản `a` (tuần 30 %) và `b` (tuần 80 %), mỗi cái một thư mục cấu hình.
fn may(dir: &Path) -> (Db, Config) {
    let db = Db::open(&dir.join("huba.sqlite")).expect("open db");
    let mut cfg = common::cfg_for_tests();
    cfg.claude_cli = claude_gia(dir).display().to_string();
    cfg.claude_accounts = ["a", "b"]
        .iter()
        .map(|n| {
            let d = dir.join(n);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("pct"), if *n == "a" { "30" } else { "80" }).unwrap();
            ClaudeAccountCfg {
                name: n.to_string(),
                config_dir: Some(d.display().to_string()),
                ..Default::default()
            }
        })
        .collect();
    (db, cfg)
}

fn so_lan_goi(dir: &Path, acc: &str) -> usize {
    std::fs::read_to_string(dir.join(acc).join("goi.log"))
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

fn bay_gio() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ── Hàm thuần ───────────────────────────────────────────────────────────────

#[test]
fn tuoi_la_cua_so_moi_nhat_trong_hai_nguon() {
    let now = 10_000_000;
    assert_eq!(tuoi_so_do(None, None, now), None);
    assert_eq!(
        tuoi_so_do(Some(now - 20 * PHUT), None, now),
        Some(20 * PHUT)
    );
    assert_eq!(
        tuoi_so_do(Some(now - 20 * PHUT), Some(now - 2 * PHUT), now),
        Some(2 * PHUT),
        "sổ CLI ghi 2′ trước mới hơn lượt dò 20′ trước"
    );
    assert_eq!(
        tuoi_so_do(Some(now + 5), None, now),
        Some(0),
        "đồng hồ lệch ≠ tuổi âm"
    );
}

#[test]
fn nguong_15_phut_mo_phien_5_phut_accounts() {
    assert_eq!(USAGE_CU_MO_PHIEN_MS, 15 * PHUT);
    assert_eq!(USAGE_CU_ACCOUNTS_MS, 5 * PHUT);
    assert!(
        can_do_lai(None, USAGE_CU_MO_PHIEN_MS),
        "chưa có số nào thì phải đo"
    );
    assert!(can_do_lai(Some(16 * PHUT), USAGE_CU_MO_PHIEN_MS));
    assert!(!can_do_lai(Some(14 * PHUT), USAGE_CU_MO_PHIEN_MS));
    assert!(can_do_lai(Some(6 * PHUT), USAGE_CU_ACCOUNTS_MS));
    assert!(!can_do_lai(Some(4 * PHUT), USAGE_CU_ACCOUNTS_MS));
}

fn quota(acc: &str, fetched: Option<i64>) -> huba::quota::Quota {
    huba::quota::Quota {
        account: acc.into(),
        week_pct: Some(10),
        week_resets_at: None,
        hour5_pct: None,
        hour5_resets_at: None,
        fetched_at_ms: fetched,
        why_unknown: None,
        chua_dung_duoc: None,
        models: vec![],
        model_use: None,
    }
}

#[test]
fn so_do_cu_hon_so_cli_thi_khong_de() {
    let usage = serde_json::json!({
        "a": { "week_pct": 90, "at_ms": 1_000 },   // dò cũ hơn sổ CLI ⟹ bỏ
        "b": { "week_pct": 90, "at_ms": 5_000 },   // dò mới hơn sổ CLI ⟹ giữ
        "c": { "week_pct": 90, "at_ms": 1_000 },   // không có sổ CLI ⟹ giữ
        "d": { "err": "hết giờ" },                  // không có số ⟹ giữ (không đè được gì)
    });
    let so_cli = [
        quota("a", Some(2_000)),
        quota("b", Some(2_000)),
        quota("d", Some(9_000)),
    ];
    let giu = chi_so_moi_hon_tep(Some(&usage), &so_cli);
    let ten: Vec<&String> = giu.as_object().unwrap().keys().collect();
    assert_eq!(ten, ["b", "c", "d"], "{giu}");
}

// ── Hành vi với `claude` giả ─────────────────────────────────────────────────

#[test]
fn chua_co_so_thi_do_ca_hai_va_ghi_so_vao_db() {
    let dir = tempfile::tempdir().unwrap();
    let (db, cfg) = may(dir.path());
    let v = usage_lam_moi(&cfg, &db, USAGE_CU_MO_PHIEN_MS);
    assert_eq!(
        (so_lan_goi(dir.path(), "a"), so_lan_goi(dir.path(), "b")),
        (1, 1)
    );
    assert_eq!(v["accounts"]["a"]["week_pct"], 30, "{v}");
    assert_eq!(v["accounts"]["b"]["week_pct"], 80, "{v}");
    assert!(v["accounts"]["a"]["at_ms"].as_i64().is_some(), "{v}");
    // Sổ nằm trong DB — một tiến trình khác (CLI `huba handover`, hubd sau khi cài
    // lại) đọc được mà không đo lại.
    let (db2, cfg2) = (
        Db::open(&dir.path().join("huba.sqlite")).unwrap(),
        cfg.clone(),
    );
    assert_eq!(usage_cached(&cfg2, &db2)["accounts"]["b"]["week_pct"], 80);
}

#[test]
fn so_con_moi_thi_khong_goi_claude() {
    let dir = tempfile::tempdir().unwrap();
    let (db, cfg) = may(dir.path());
    let now = bay_gio();
    db.set_cursor(
        USAGE_KEY,
        &serde_json::json!({
            "a": { "week_pct": 11, "at_ms": now - 2 * PHUT },
            "b": { "week_pct": 22, "at_ms": now - 14 * PHUT },
        })
        .to_string(),
    )
    .unwrap();
    let v = usage_lam_moi(&cfg, &db, USAGE_CU_MO_PHIEN_MS);
    assert_eq!(
        (so_lan_goi(dir.path(), "a"), so_lan_goi(dir.path(), "b")),
        (0, 0),
        "số còn mới (< 15′) mà vẫn gọi `claude`"
    );
    assert_eq!(v["accounts"]["b"]["week_pct"], 22);
}

#[test]
fn chi_do_lai_tai_khoan_co_so_cu() {
    let dir = tempfile::tempdir().unwrap();
    let (db, cfg) = may(dir.path());
    let now = bay_gio();
    db.set_cursor(
        USAGE_KEY,
        &serde_json::json!({
            "a": { "week_pct": 11, "at_ms": now - 2 * PHUT },
            "b": { "week_pct": 22, "at_ms": now - 40 * PHUT },
        })
        .to_string(),
    )
    .unwrap();
    let v = usage_lam_moi(&cfg, &db, USAGE_CU_MO_PHIEN_MS);
    assert_eq!(
        (so_lan_goi(dir.path(), "a"), so_lan_goi(dir.path(), "b")),
        (0, 1)
    );
    assert_eq!(v["accounts"]["a"]["week_pct"], 11, "a còn mới — giữ số cũ");
    assert_eq!(v["accounts"]["b"]["week_pct"], 80, "b cũ 40′ — số mới đo");
    // Cùng sổ ấy, ngưỡng `/accounts` (5′): a (2′) vẫn mới, b vừa đo — không gọi thêm.
    usage_lam_moi(&cfg, &db, USAGE_CU_ACCOUNTS_MS);
    assert_eq!(
        (so_lan_goi(dir.path(), "a"), so_lan_goi(dir.path(), "b")),
        (0, 1)
    );
}

/// Sổ `.claude.json` do một phiên đang mở ghi 1′ trước cũng là một số đo — tài khoản
/// ấy không cần đo lại, dù huba chưa từng dò nó.
#[test]
fn so_cli_vua_ghi_cung_tinh_la_moi() {
    let dir = tempfile::tempdir().unwrap();
    let (db, cfg) = may(dir.path());
    let now = bay_gio();
    std::fs::write(
        dir.path().join("a").join(".claude.json"),
        serde_json::json!({ "cachedUsageUtilization": {
            "utilization": { "seven_day": { "utilization": 44 } },
            "fetchedAtMs": now - PHUT } })
        .to_string(),
    )
    .unwrap();
    usage_lam_moi(&cfg, &db, USAGE_CU_MO_PHIEN_MS);
    assert_eq!(
        (so_lan_goi(dir.path(), "a"), so_lan_goi(dir.path(), "b")),
        (0, 1)
    );
}

#[test]
fn do_hong_thi_giu_so_cu_va_lan_sau_do_lai() {
    let dir = tempfile::tempdir().unwrap();
    let (db, mut cfg) = may(dir.path());
    cfg.claude_cli = dir.path().join("khong-co").display().to_string();
    let now = bay_gio();
    db.set_cursor(
        USAGE_KEY,
        &serde_json::json!({ "a": { "week_pct": 11, "at_ms": now - 40 * PHUT } }).to_string(),
    )
    .unwrap();
    let v = usage_lam_moi(&cfg, &db, USAGE_CU_MO_PHIEN_MS);
    let a = &v["accounts"]["a"];
    assert_eq!(a["week_pct"], 11, "đo hỏng mà mất số cũ: {v}");
    assert!(a["err"].is_string(), "đo hỏng mà không ghi lỗi: {v}");
    assert_eq!(
        a["at_ms"].as_i64(),
        Some(now - 40 * PHUT),
        "đo hỏng mà vẫn đóng mốc 'mới' — lần chọn sau sẽ không đo lại: {v}"
    );
}

#[test]
fn usage_cached_khong_bao_gio_goi_claude() {
    let dir = tempfile::tempdir().unwrap();
    let (db, cfg) = may(dir.path());
    let v = usage_cached(&cfg, &db);
    assert_eq!(
        (so_lan_goi(dir.path(), "a"), so_lan_goi(dir.path(), "b")),
        (0, 0)
    );
    assert_eq!(v["accounts"], serde_json::json!({}), "{v}");
}

// ── Chỗ nối: KHÔNG còn nhịp đo nền ──────────────────────────────────────────

fn nguon(ten: &str) -> String {
    let p = format!("{}/src/{ten}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: {p}: {e}"))
}

/// Tên hàm chứa mỗi lời gọi `xep_hang_tai_khoan(…, Some(` / `usage_lam_moi(` trong
/// một tệp nguồn — mẫu số của "những chỗ được phép đi đo".
fn cho_di_do(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut ham = String::new();
    let dong: Vec<&str> = src.lines().collect();
    for (i, l) in dong.iter().enumerate() {
        let t = l.trim_start();
        if let Some(r) = t.strip_prefix("pub fn ").or_else(|| t.strip_prefix("fn ")) {
            ham = r.split('(').next().unwrap_or("").to_string();
        }
        let goi_do = t.contains("usage_lam_moi(")
            || (t.contains("xep_hang_tai_khoan(")
                && dong[i..(i + 5).min(dong.len())]
                    .iter()
                    .any(|x| x.contains("USAGE_CU_")));
        if goi_do && !t.starts_with("//") && !t.starts_with("pub fn") && !t.starts_with("fn ") {
            out.push(ham.clone());
        }
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn chi_nhung_cho_sap_chon_tai_khoan_moi_di_do() {
    let pipeline = cho_di_do(&nguon("pipeline.rs"));
    assert_eq!(
        pipeline,
        ["announce_changes", "auto_handover", "auto_switch_on_limit"],
        "có chỗ MỚI đi đo /usage — mỗi chỗ đo thêm là một nhịp đo nền tiềm tàng"
    );
    let runtime = cho_di_do(&nguon("runtime.rs"));
    assert_eq!(
        runtime,
        ["accounts_say", "xep_hang_tai_khoan"],
        "{runtime:?}"
    );
    assert_eq!(cho_di_do(&nguon("main.rs")), ["cmd_handover"]);
}

/// ĐỐI CHỨNG NGƯỢC: một hàm vòng chạy gọi đo ⟹ bộ đếm phải thấy nó.
#[test]
fn doi_chung_nguoc_them_mot_cho_do_nen() {
    let src = "fn run_once(db: &Db) {\n    let h = crate::runtime::xep_hang_tai_khoan(\n        cfg,\n        db,\n        Some(crate::runtime::USAGE_CU_MO_PHIEN_MS),\n    );\n}\nfn khac() {\n    let v = usage_cached(cfg, db);\n}\n";
    assert_eq!(cho_di_do(src), ["run_once"]);
}
