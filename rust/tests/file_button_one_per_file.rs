//! MỘT TỆP MỘT NÚT 📎 — nhắc hai lần vẫn là một tệp.
//!
//! 🔴 Hà 2026-08-19, ảnh chụp một tin `[tfl5]`: hai nút `📎 docs/du-toan.md`
//! giống hệt nhau nằm cạnh nhau. Cùng một tệp được nhắc hai lần trong một tin
//! là chuyện thường — một lần trong câu văn, một lần nữa trong dòng lệnh `mv` —
//! nhưng hai cái nút đưa về CÙNG một tệp thì cái thứ hai không nói thêm gì, chỉ
//! tốn một hàng bàn phím và làm người đọc tưởng có hai tệp khác nhau.
//!
//! Bản vá `1f08f47` khử trùng ở `kept_paths`, tức ĐÚNG chỗ hai chỗ dùng chung
//! (`remember_files` cho nút đáy tin, `file_anchors` cho 📎 giữa chữ) — nhưng
//! nó đi một mình: commit ấy đụng đúng một tệp `pipeline.rs`, không bài kiểm
//! nào tái hiện được lỗi. Tệp này là cái khoá còn thiếu.
//!
//! Hai điều nó ghim, và điều thứ hai mới là chỗ dễ vỡ khi ai đó viết lại phép
//! khử trùng: trùng tính theo ĐƯỜNG ĐÃ GIẢI (hai chuỗi khác nhau trỏ vào một
//! tệp là MỘT), và **không** tính theo tên tệp — hai `README.md` ở hai thư mục
//! là hai tệp, gộp chúng là mất một nút có thật.

mod common;

use huba::pipeline::{file_anchors, remember_files, WATCH_KEY};

const SID: &str = "0f3c2a11-1111-4111-8111-111111111111";

/// Sổ theo dõi vừa đủ để `session_root` trả lời được: phiên này làm ở đâu.
fn ghi_so(db: &huba::db::Db, folder: &str) {
    let book = serde_json::json!({ SID: { "s": "idle", "d": folder } });
    db.set_cursor(WATCH_KEY, &book.to_string())
        .expect("ghi sổ theo dõi");
}

/// Một cây làm việc thật, vì `sendable_file` hỏi ĐĨA chứ không hỏi hình dạng.
fn cay(files: &[&str]) -> (huba::db::Db, tempfile::TempDir, huba::config::Config) {
    let (db, dir) = common::fresh_db();
    let mut cfg = common::cfg_for_tests();
    cfg.workspace_root = dir.path().to_path_buf();
    let root = dir.path().join("AI/tfl5");
    for f in files {
        let p = root.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "nội dung\n").unwrap();
    }
    ghi_so(&db, "AI/tfl5");
    (db, dir, cfg)
}

#[test]
fn mot_tep_nhac_hai_lan_van_chi_mot_nut() {
    let (db, dir, cfg) = cay(&["docs/du-toan.md", "docs/phu-luc.md"]);

    // Đúng hình dạng của tin đã sinh ra lỗi: tệp được nhắc một lần giữa câu văn
    // (đường tương đối) và một lần nữa trong dòng lệnh (đường tuyệt đối).
    let tuyet_doi = dir
        .path()
        .join("AI/tfl5/docs/du-toan.md")
        .to_string_lossy()
        .to_string();
    let paths = vec![
        "docs/du-toan.md".to_string(),
        tuyet_doi,
        "docs/phu-luc.md".to_string(),
    ];

    let neo = file_anchors(&db, &cfg, SID, &paths);
    assert_eq!(
        neo,
        vec![
            ("docs/du-toan.md".to_string(), 0),
            ("docs/phu-luc.md".to_string(), 1)
        ],
        "ba lần nhắc, hai tệp ⟹ hai neo — và giữ lần nhắc ĐẦU nên thứ tự nút \
         vẫn là thứ tự đọc"
    );

    // Hai chỗ dùng chung phải ra CÙNG một danh sách. Chỉ số của neo 📎 giữa chữ
    // chính là chỉ số của nút ở đáy tin (`file:<i>`), nên lệch một bậc nghĩa là
    // bấm 📎 trên tên tệp này lại tải về tệp khác.
    let nut = remember_files(&db, &cfg, SID, &paths);
    assert_eq!(nut.len(), neo.len(), "{nut:?}");
    assert_eq!(nut[0].1, "file:0", "{nut:?}");
    assert_eq!(nut[1].1, "file:1", "{nut:?}");
    assert!(nut[0].0.contains("du-toan.md"), "{nut:?}");
    assert!(nut[1].0.contains("phu-luc.md"), "{nut:?}");
}

#[test]
fn hai_tep_khac_nhau_trung_ten_thi_van_hai_nut() {
    // Chiều ngược lại, và là chỗ một phép khử trùng viết vội sẽ sập: khử theo
    // TÊN thì hai tệp này thành một, mất hẳn một nút có thật. Trong một cây mã,
    // `README.md` / `Cargo.toml` / `mod.rs` trùng tên là chuyện thường ngày.
    let (db, _dir, cfg) = cay(&["docs/README.md", "tools/README.md"]);
    let paths = vec!["docs/README.md".to_string(), "tools/README.md".to_string()];

    let neo = file_anchors(&db, &cfg, SID, &paths);
    assert_eq!(
        neo,
        vec![
            ("docs/README.md".to_string(), 0),
            ("tools/README.md".to_string(), 1)
        ],
        "hai tệp thật, hai neo — trùng TÊN không phải trùng TỆP"
    );

    // Và nhãn phải phân biệt được, không chỉ đọc được: trùng tên thì mang thêm
    // thư mục cha (luật cũ của `remember_files`, ghim lại ở đây vì phép khử
    // trùng đứng ngay trước nó).
    let nut = remember_files(&db, &cfg, SID, &paths);
    assert_eq!(nut.len(), 2, "{nut:?}");
    assert!(nut[0].0.contains("docs/README.md"), "{nut:?}");
    assert!(nut[1].0.contains("tools/README.md"), "{nut:?}");
}
