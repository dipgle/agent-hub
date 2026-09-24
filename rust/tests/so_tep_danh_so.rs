//! Sổ tệp 📎 đánh số TĂNG DẦN — một tin mới có tệp không được đổi đích của 📎 ở
//! tin cũ; và tin TỰ PHÁT cũng neo 📎 ngay tại tên tệp như `/shot`.
//!
//! 🔴 Hà 2026-09-24, ảnh tin `💤 [dwork/blocked]` có dòng
//! `📎 /Users/…/HOP-QUYET-DINH.html` in chữ đơn cách, không bấm được: *"Vấn đề tải
//! file trong nội dung tin vẫn lúc được lúc không"*. Hai gốc, cùng một triệu chứng:
//!
//! * Sổ tệp chỉ có MỘT ô (`{ "s", "p": [...] }`), ghi đè ở mỗi tin có tệp, còn nút
//!   chỉ mang chỉ số (`file:0`). Đo log 21/09→24/09: lần 08:11:26Z bấm liên kết số
//!   0 nhận về `CLAUDE.md` của một tin KHÁC, 26 s sau bấm lại mới ra tệp `.html`.
//! * Tin tự phát (`announce_changes`) dựng nút ở đáy nhưng để `SessionData.files`
//!   trống ⟹ cả ba tin `[dwork/blocked]` hôm ấy `text_links: 0`.

mod common;

use huba::pipeline::{
    file_anchors, ghi_so_tep, quick_file, remember_files, render_session_data, SessionData,
    FILE_GIU, FILE_SO_DAU, WATCH_KEY,
};

const A: &str = "0f3c2a11-1111-4111-8111-111111111111";
const B: &str = "0f3c2a22-2222-4222-8222-222222222222";

/// Hai phiên, mỗi phiên một cây thật — `sendable_file` hỏi ĐĨA.
fn hai_cay(tep: &[&str]) -> (huba::db::Db, tempfile::TempDir, huba::config::Config) {
    let (db, dir) = common::fresh_db();
    let mut cfg = common::cfg_for_tests();
    cfg.workspace_root = dir.path().to_path_buf();
    for (sid, cay) in [(A, "AI/mot"), (B, "AI/hai")] {
        for f in tep {
            let p = dir.path().join(cay).join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, format!("{sid} {f}\n")).unwrap();
        }
    }
    let book = serde_json::json!({
        A: { "s": "idle", "d": "AI/mot" },
        B: { "s": "idle", "d": "AI/hai" },
    });
    db.set_cursor(WATCH_KEY, &book.to_string())
        .expect("ghi sổ theo dõi");
    (db, dir, cfg)
}

/// Số trong `file:<n>`.
fn so(nut: &(String, String)) -> usize {
    nut.1
        .strip_prefix("file:")
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("nút không phải `file:<n>`: {nut:?}"))
}

/// ĐÚNG ca Hà gặp, và chỉ dùng API có từ trước lượt sửa (`remember_files` +
/// `quick_file`) — nên bài này đỏ được trên sổ một ô.
#[test]
fn tin_moi_co_tep_khong_doi_dich_cua_tin_cu() {
    let (db, _dir, cfg) = hai_cay(&["docs/HOP-QUYET-DINH.html", "CLAUDE.md"]);
    let nut_a = remember_files(&db, &cfg, A, &["docs/HOP-QUYET-DINH.html".to_string()]);
    let nut_b = remember_files(&db, &cfg, B, &["CLAUDE.md".to_string()]);
    assert_eq!((nut_a.len(), nut_b.len()), (1, 1), "{nut_a:?} {nut_b:?}");
    assert_ne!(
        so(&nut_a[0]),
        so(&nut_b[0]),
        "hai tin, hai tệp, CÙNG một số"
    );
    assert_eq!(
        quick_file(&db, so(&nut_a[0])),
        Some((A.to_string(), "docs/HOP-QUYET-DINH.html".to_string())),
        "bấm 📎 của tin CŨ sau khi tin mới tới — phải ra tệp của tin cũ"
    );
    assert_eq!(
        quick_file(&db, so(&nut_b[0])),
        Some((B.to_string(), "CLAUDE.md".to_string()))
    );
}

#[test]
fn cung_phien_cung_tep_thi_dung_lai_so() {
    let (db, _dir, cfg) = hai_cay(&["a.md", "b.md"]);
    let dau = remember_files(&db, &cfg, A, &["a.md".to_string()]);
    let giua = remember_files(&db, &cfg, A, &["b.md".to_string()]);
    let lai = remember_files(&db, &cfg, A, &["b.md".to_string(), "a.md".to_string()]);
    assert_eq!(so(&lai[1]), so(&dau[0]), "a.md nhắc lại phải giữ số cũ");
    assert_eq!(so(&lai[0]), so(&giua[0]), "b.md nhắc lại phải giữ số cũ");
    // Cùng TÊN tệp ở phiên KHÁC là tệp khác (cây khác) ⟹ số khác.
    let b = remember_files(&db, &cfg, B, &["a.md".to_string()]);
    assert_ne!(so(&b[0]), so(&dau[0]));
}

#[test]
fn so_cua_so_mot_o_cu_doc_ra_da_cu() {
    let (db, _dir, cfg) = hai_cay(&["a.md"]);
    // Sổ một ô cũ còn nằm trong DB của máy thật dưới khoá cũ.
    db.set_cursor("quick:files", r#"{"s":"x","p":["/etc/passwd"]}"#)
        .unwrap();
    let nut = remember_files(&db, &cfg, A, &["a.md".to_string()]);
    assert!(so(&nut[0]) >= FILE_SO_DAU, "{nut:?}");
    for n in 0..4 {
        assert_eq!(
            quick_file(&db, n),
            None,
            "số {n} của sổ một ô không được trúng tệp nào của sổ mới"
        );
    }
}

#[test]
fn so_vong_bo_muc_cu_nhat_va_moi_so_vua_phat_deu_tra_ra_dung_tep() {
    let ten: Vec<String> = (0..FILE_GIU + 3).map(|i| format!("t/{i}.md")).collect();
    let refs: Vec<&str> = ten.iter().map(String::as_str).collect();
    let (db, _dir, cfg) = hai_cay(&refs);
    let mut dau = None;
    for t in &ten {
        let nut = remember_files(&db, &cfg, A, std::slice::from_ref(t));
        assert_eq!(nut.len(), 1, "{t}: {nut:?}");
        assert_eq!(
            quick_file(&db, so(&nut[0])),
            Some((A.to_string(), t.clone())),
            "số vừa phát cho {t} không tra ra đúng tệp"
        );
        dau.get_or_insert(so(&nut[0]));
    }
    let dau = dau.unwrap();
    assert_eq!(quick_file(&db, dau), None, "mục cũ nhất phải rơi khỏi sổ");
    assert_eq!(
        quick_file(&db, dau + 3),
        Some((A.to_string(), "t/3.md".to_string())),
        "mục cũ nhất CÒN giữ phải vẫn đúng số của nó"
    );
    // Mục vừa bị đẩy tới mép: nhắc lại nó cùng một tệp mới — số trả về phải
    // còn sống ngay sau lượt ghi, kể cả khi lượt ghi ấy phải bỏ bớt mục cũ.
    let mep = ten[3].clone();
    let nut = remember_files(&db, &cfg, A, &[mep.clone(), ten[0].clone()]);
    for (n, t) in nut.iter().zip([&mep, &ten[0]]) {
        assert_eq!(
            quick_file(&db, so(n)),
            Some((A.to_string(), t.clone())),
            "{n:?}"
        );
    }
}

/// Neo 📎 giữa chữ và nút ở đáy mang CÙNG số — sinh ra từ một lượt ghi.
#[test]
fn neo_va_nut_cung_so() {
    let (db, _dir, cfg) = hai_cay(&["docs/x.md", "docs/y.md"]);
    let paths = vec!["docs/x.md".to_string(), "docs/y.md".to_string()];
    assert_eq!(file_anchors(&db, &cfg, A, &paths), paths);
    let tep = ghi_so_tep(&db, &cfg, A, &paths);
    assert_eq!(tep.len(), 2);
    for t in &tep {
        assert_eq!(t.nut().1, format!("file:{}", t.so));
        assert_eq!(t.neo_so(), (t.neo.clone(), t.so));
        assert_eq!(quick_file(&db, t.so), Some((A.to_string(), t.neo.clone())));
    }
}

/// Neo trong chữ dựng liên kết `f_<số của sổ>` — không phải một số đếm lại từ 0.
#[test]
fn lien_ket_giua_chu_mang_so_cua_so() {
    huba::telegram::set_bot_username("hub_test_bot");
    let (db, _dir, cfg) = hai_cay(&["docs/HOP-QUYET-DINH.html"]);
    // Một tin có tệp tới trước, để số của tin sau KHÁC số thứ tự 0.
    let _truoc = remember_files(&db, &cfg, B, &["docs/HOP-QUYET-DINH.html".to_string()]);
    let tep = ghi_so_tep(&db, &cfg, A, &["docs/HOP-QUYET-DINH.html".to_string()]);
    assert_eq!(tep.len(), 1);
    let text = "Lô vẫn chờ Hà trả lời.\n📎 docs/HOP-QUYET-DINH.html";
    let html = render_session_data(
        text,
        &SessionData {
            files: tep.iter().map(|t| t.neo_so()).collect(),
            ..Default::default()
        },
    );
    let can = format!("f_{}", tep[0].so);
    assert!(
        html.contains(&can),
        "liên kết phải mang `{can}` (số của sổ): {html}"
    );
    assert!(tep[0].so > FILE_SO_DAU, "{:?}", tep[0]);
}

// ── Chỗ nối: đọc MÃ NGUỒN (cùng kiểu `window_of_dem_truoc.rs`) ──────────────

fn nguon() -> String {
    let p = std::env::var("HUBA_PIPELINE_SRC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs").to_string());
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: không đọc được {p}: {e}"))
}

/// Tin tự phát ghi sổ tệp và đưa `files` vào CẢ HAI nhánh có định dạng.
/// `None` = thiếu mỏ neo ⟹ ĐỎ.
fn tu_phat_co_neo(src: &str) -> Option<bool> {
    let dau = src.find("pub fn announce_changes(")?;
    let cuoi = dau + src[dau..].find("\n}\n")?;
    let than = &src[dau..cuoi];
    let ghi = than.matches("ghi_so_tep(").count();
    let dua_vao = than.matches("        files,\n").count();
    Some(ghi == 1 && dua_vao == 2 && !than.contains("say_with_command_icons("))
}

#[test]
fn tin_tu_phat_neo_tep_ngay_trong_chu() {
    assert_eq!(
        tu_phat_co_neo(&nguon()),
        Some(true),
        "`announce_changes` phải `ghi_so_tep` rồi đưa `files` vào cả hai nhánh `SessionData`"
    );
}

/// Không chỗ nào dựng số 📎 bằng `file_anchors` nữa — số chỉ sinh ra ở sổ.
#[test]
fn khong_cho_nao_tu_dem_so_tep() {
    let src = nguon();
    let goi = src.matches("file_anchors(").count() - src.matches("pub fn file_anchors(").count();
    assert_eq!(
        goi, 0,
        "có chỗ gọi `file_anchors` trong pipeline.rs — dựng liên kết thì dùng `ghi_so_tep`"
    );
}

/// ĐỐI CHỨNG NGƯỢC: đúng hình dạng HEAD `3f33175` — nhánh tự phát không có
/// `files`, nhánh thứ hai đi `say_with_command_icons`.
#[test]
fn doi_chung_nguoc_tin_tu_phat_cu() {
    let cu = "pub fn announce_changes(db: &Db) {\n    quick.extend(remember_files(db, cfg, &id, &p));\n    let data = SessionData {\n        cmds,\n        ..Default::default()\n    };\n    say_with_command_icons(tg, &text, &c, &b, \"k\");\n}\n";
    assert_eq!(tu_phat_co_neo(cu), Some(false));
    assert_eq!(tu_phat_co_neo("fn khac() {}\n"), None);
}
