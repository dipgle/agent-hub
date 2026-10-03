//! Phiên CHẾT THEO MÁY — máy khởi động lại thì báo MỘT tin kèm nút ▶ mở lại.
//!
//! 🔴 Hà 03/10, sau lần máy hết pin tối 02/10: *"Khi máy tắt đột ngột thì cli
//! cũng bị thoát, mở lại phiên đã có nạp lại được lịch sử đang làm giở không"*.
//! Đo: 6/6 nhật ký nguyên vẹn (bản ghi cuối cách lúc tắt 1 s) nên `--resume`
//! nạp lại được — nhưng lượt đầu sau khởi động huba im (sổ "cũ", 604 s) rồi ghi
//! đè sổ, nên từ điện thoại không còn biết phiên nào chết và `/new <id>` không
//! còn biết tài khoản. 0/6 phiên được mở lại; 2 phiên đang làm dở.

mod common;

use std::collections::BTreeMap;

use common::fresh_db;
use huba::watch::{self, Mark};

const ID_DORG: &str = "c3a28a4d-1111-4222-8333-944445555666";
const ID_HUB: &str = "28d6d836-1111-4222-8333-944445555666";
const ID_CON: &str = "aaaaaaaa-1111-4222-8333-944445555666";
const ID_DO: &str = "bbbbbbbb-1111-4222-8333-944445555666";
const ID_HUBA_MO: &str = "cccccccc-1111-4222-8333-944445555666";

const SO: i64 = 1_790_957_088; // lượt ghi sổ cuối: 16:04:48Z 02/10
const BOOT: i64 = 1_790_957_646; // kern.boottime đo thật: 16:14:06Z 02/10

fn mark(label: &str, acc: &str, f: i64) -> Mark {
    Mark {
        s: "working@1".into(),
        n: "projects-xx".into(),
        l: label.into(),
        a: acc.into(),
        c: "/Users/hanguyen/projects".into(),
        f,
        ..Default::default()
    }
}

fn so_truoc_khi_tat() -> BTreeMap<String, Mark> {
    let mut b = BTreeMap::new();
    b.insert(ID_DORG.into(), mark("[dwork/dorg]", "acc3", SO - 7200));
    b.insert(ID_HUB.into(), mark("[dwork/dhub]", "acc4", SO - 200));
    // Phiên CON: chết theo phiên cha, không kể riêng.
    let mut con = mark("[dwork/dorg] con", "acc3", SO - 600);
    con.p = ID_DORG.into();
    b.insert(ID_CON.into(), con);
    // Lượt dò hạn mức của huba: sống 5 giây tính tới lượt cuối.
    b.insert(ID_DO.into(), mark("", "acc5", SO - 5));
    // Phiên huba MỞ cho chủ máy (`h`), mới 5 giây: VẪN kể.
    let mut mo = mark("[mailler]", "acc6", SO - 5);
    mo.h = true;
    b.insert(ID_HUBA_MO.into(), mo);
    b
}

#[test]
fn doc_dung_moc_khoi_dong_cua_may() {
    let s = "{ sec = 1790957646, usec = 48942 } Fri Oct  2 23:14:06 2026\n";
    assert_eq!(huba::runtime::parse_boottime(s), Some(BOOT));
    assert_eq!(huba::runtime::parse_boottime("rác"), None);
    assert_eq!(huba::runtime::parse_boottime("{ sec = , usec = 1 }"), None);
}

#[test]
fn may_khoi_dong_lai_sau_luot_so_cuoi_thi_ke_ten_dung_phien() {
    let ds = watch::chet_theo_may(&so_truoc_khi_tat(), SO, BOOT, BOOT + 60);
    let ids: Vec<&str> = ds.iter().map(|(i, _)| i.as_str()).collect();
    assert_eq!(ids.len(), 3, "{ids:?}");
    for id in [ID_DORG, ID_HUB, ID_HUBA_MO] {
        assert!(ids.contains(&id), "thiếu {id}: {ids:?}");
    }
    assert!(
        !ids.contains(&ID_CON),
        "phiên con chết theo phiên cha — không kể riêng"
    );
    assert!(
        !ids.contains(&ID_DO),
        "lượt dò hạn mức của huba không phải tin"
    );
}

#[test]
fn may_khong_khoi_dong_lai_thi_khong_ke_ai() {
    // Máy khởi động TRƯỚC lượt ghi sổ cuối = hubd chỉ khởi động lại (cài bản mới).
    assert!(watch::chet_theo_may(&so_truoc_khi_tat(), SO, SO - 3600, SO + 60).is_empty());
    assert!(watch::chet_theo_may(&so_truoc_khi_tat(), SO, SO, SO + 60).is_empty());
    assert!(!huba::pipeline::may_vua_khoi_dong(Some(SO), Some(SO - 1)));
    assert!(!huba::pipeline::may_vua_khoi_dong(None, Some(BOOT)));
    assert!(!huba::pipeline::may_vua_khoi_dong(Some(SO), None));
    assert!(huba::pipeline::may_vua_khoi_dong(Some(SO), Some(BOOT)));
}

#[test]
fn so_qua_cu_thi_khong_ke_ai() {
    // huba vắng mặt hơn một ngày trước lần khởi động: phiên trong sổ có thể đã
    // tắt êm từ lâu — kể tên chúng là đoán.
    let now = SO + watch::CHET_THEO_MAY_TOI_DA_SEC + 1;
    assert!(watch::chet_theo_may(&so_truoc_khi_tat(), SO, now - 10, now).is_empty());
}

#[test]
fn cau_bao_dua_phien_lam_do_len_dau_va_noi_cach_mo_lai() {
    let b = so_truoc_khi_tat();
    let ds = vec![
        (ID_HUB.to_string(), b[ID_HUB].clone(), Some(false)),
        (ID_DORG.to_string(), b[ID_DORG].clone(), Some(true)),
        (ID_HUBA_MO.to_string(), b[ID_HUBA_MO].clone(), None),
    ];
    let t = watch::chet_theo_may_text(&ds, SO, BOOT);
    assert!(t.contains("3 phiên"), "{t}");
    assert!(
        t.contains("THEO MÁY") && t.contains("claude --resume"),
        "{t}"
    );
    let dorg = t.find("[dwork/dorg]").expect("thiếu dorg");
    let hub = t.find("[dwork/dhub]").expect("thiếu dhub");
    assert!(dorg < hub, "phiên ĐANG LÀM DỞ phải đứng trước: {t}");
    assert!(t.contains("acc3 — ⚠ ĐANG LÀM DỞ"), "{t}");
    assert!(t.contains("acc4 — đã xong lượt"), "{t}");
    assert!(t.contains("chưa rõ"), "{t}");
}

#[test]
fn nut_mo_lai_di_dung_route_new() {
    assert_eq!(
        huba::telegram::callback_to_command(&format!("moilai:{ID_DORG}")).as_deref(),
        Some(format!("/new {ID_DORG}").as_str())
    );
    assert_eq!(huba::telegram::callback_to_command("moilai:"), None);
    assert!(
        format!("moilai:{ID_DORG}").len() <= 64,
        "callback_data quá trần 64 byte của Telegram"
    );
}

/// CA CHÍNH: sau khi máy lên, `/new <id>` (nút ▶) PHẢI biết tài khoản của phiên
/// đã chết theo máy — dù sổ theo dõi đã bị ghi đè bằng danh sách mới.
#[test]
fn sau_khoi_dong_new_id_van_biet_tai_khoan_de_resume() {
    let (db, _d) = fresh_db();
    // Trước bản vá: sổ theo dõi đã bị ghi đè (rỗng) ⟹ không ai biết tài khoản.
    db.set_cursor(huba::pipeline::WATCH_KEY, "{}").unwrap();
    assert_eq!(huba::pipeline::resume_target(ID_DORG, &db), None);

    let ds = huba::pipeline::ghi_so_chet_theo_may(
        &db,
        &so_truoc_khi_tat(),
        Some(SO),
        Some(BOOT),
        BOOT + 60,
    );
    assert_eq!(ds.len(), 3);
    assert_eq!(
        huba::pipeline::resume_target(ID_DORG, &db),
        Some((ID_DORG.to_string(), "acc3".to_string(), String::new()))
    );
    assert_eq!(
        huba::pipeline::resume_target(&format!("{ID_HUB} làm tiếp đi"), &db),
        Some((
            ID_HUB.to_string(),
            "acc4".to_string(),
            "làm tiếp đi".to_string()
        ))
    );
    // Phiên con / lượt dò không vào sổ ⟹ không có nút, không mở lại được.
    assert_eq!(huba::pipeline::resume_target(ID_CON, &db), None);
}

/// Máy KHÔNG khởi động lại (chỉ hubd cài lại) ⟹ không ghi gì vào sổ ấy.
#[test]
fn khong_khoi_dong_lai_thi_khong_ghi_so() {
    let (db, _d) = fresh_db();
    let ds = huba::pipeline::ghi_so_chet_theo_may(
        &db,
        &so_truoc_khi_tat(),
        Some(SO),
        Some(SO - 3600),
        SO + 60,
    );
    assert!(ds.is_empty());
    assert_eq!(db.get_cursor(huba::pipeline::MAY_TAT_KEY).unwrap(), None);
    // Thiếu mốc khởi động (không đọc được sysctl) ⟹ không kết luận.
    assert!(huba::pipeline::ghi_so_chet_theo_may(
        &db,
        &so_truoc_khi_tat(),
        Some(SO),
        None,
        SO + 60
    )
    .is_empty());
}

// ── Chỗ nối trong `announce_changes`: đọc MÃ NGUỒN (cùng khuôn tests/chay_do_luc_chet.rs) ──

fn nguon() -> String {
    let p = std::env::var("HUBA_PIPELINE_SRC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs").to_string());
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: không đọc được {p}: {e}"))
}

/// Phép hỏi "máy vừa khởi động lại" phải đứng TRƯỚC cửa "sổ cũ" (nơi im theo
/// luật 11) — đứng sau thì lượt đầu sau khởi động đã im và ghi đè sổ mất rồi.
/// `None` = thiếu mỏ neo ⟹ ĐỎ.
fn thu_tu(src: &str) -> Option<bool> {
    let a = src.find("pub fn announce_changes(")?;
    let t = &src[a..];
    let hoi = t.find("if may_vua_khoi_dong(so_luc, boot)")?;
    let ghi = t.find("ghi_so_chet_theo_may(db, &prev, so_luc, boot, now)")?;
    let gui = t.find("tg.send_buttons(&text, &nut)")?;
    let im = t.find("if !watch_book_usable(prev.len(), age)")?;
    Some(hoi < ghi && ghi < gui && gui < im)
}

#[test]
fn announce_hoi_khoi_dong_truoc_cua_so_cu() {
    assert_eq!(thu_tu(&nguon()), Some(true));
}

#[test]
fn doi_chung_nguoc_hoi_sau_cua_so_cu() {
    let sai = "pub fn announce_changes(db) {\n if !watch_book_usable(prev.len(), age) { return; }\n if may_vua_khoi_dong(so_luc, boot) {\n ghi_so_chet_theo_may(db, &prev, so_luc, boot, now);\n tg.send_buttons(&text, &nut);\n }\n}\n";
    assert_eq!(thu_tu(sai), Some(false));
    assert_eq!(
        thu_tu("pub fn announce_changes(db) {\n if !watch_book_usable(prev.len(), age) {}\n}\n"),
        None
    );
}
