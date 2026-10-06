//! Dấu tệp DUY NHẤT: một dòng riêng `📎 /đường/dẫn/tuyệt/đối`.
//!
//! 🔴 Hà 2026-10-06: *"huba đọc đúng 1 định dạng đánh dấu duy nhất để bấm vào là
//! lệnh lấy file gửi ngược về tele"* — chốt *"📎 + bỏ phép đoán"*. Phép đoán cũ
//! đo trên `logs/huba.log`: 9.736 lần lùng cây tới trần, 3.906 lần mơ hồ, 90 tệp
//! gửi được — cái "lúc được lúc không" từ 24/09.

mod common;

use huba::keys::file_markers;
use huba::pipeline::{ghi_so_tep, marked_file, note_unsendable_marks};

#[test]
fn only_the_marker_line_is_read() {
    let text = "Báo cáo xong.\n\
                📎 /Users/x/projects/huba/.tmp/bao-cao.html\n\
                  ⏺ 📎 `/Users/x/projects/huba/docs/a b.md`\n\
                📎 ~/projects/huba/goi.zip\n\
                Xem /Users/x/projects/huba/README.md nhé.\n\
                /Users/x/projects/huba/CLAUDE.md\n\
                Câu có 📎 /Users/x/giua-cau.md thì không tính.\n\
                📎docs/tuong-doi.md\n\
                📎 docs/tuong-doi.md\n";
    assert_eq!(
        file_markers(text),
        vec![
            "/Users/x/projects/huba/.tmp/bao-cao.html".to_string(),
            "/Users/x/projects/huba/docs/a b.md".to_string(),
            "~/projects/huba/goi.zip".to_string(),
        ],
        "chỉ dòng `📎 <tuyệt đối>` — đường dẫn trần, `📎` giữa câu, tương đối: KHÔNG"
    );
    // Trùng thì một.
    assert_eq!(file_markers("📎 /a/b.md\n📎 /a/b.md").len(), 1);
    assert!(file_markers("").is_empty());
}

fn cay() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("projects");
    std::fs::create_dir_all(ws.join("huba/.tmp")).unwrap();
    std::fs::write(ws.join("huba/.tmp/goi.zip"), b"PK\x03\x04").unwrap();
    std::fs::write(ws.join("huba/.tmp/bao cao.html"), "<p>x</p>").unwrap();
    std::fs::write(dir.path().join("ngoai.txt"), "x").unwrap();
    (dir, ws)
}

#[test]
fn any_file_inside_the_workspace_is_sendable_and_failures_say_why() {
    let (dir, ws) = cay();
    let zip = ws.join("huba/.tmp/goi.zip");
    assert!(
        marked_file(&zip.to_string_lossy(), &ws).is_ok(),
        ".zip phải gửi được — trước 06/10 nó bị chặn mọc nút"
    );
    assert!(marked_file(&ws.join("huba/.tmp/bao cao.html").to_string_lossy(), &ws).is_ok());

    let ngoai = marked_file(&dir.path().join("ngoai.txt").to_string_lossy(), &ws);
    assert!(ngoai.unwrap_err().contains("nằm ngoài"));
    let mat = marked_file(&ws.join("huba/khong-co.md").to_string_lossy(), &ws);
    assert!(mat.unwrap_err().contains("không thấy tệp"));
    let thu_muc = marked_file(&ws.join("huba").to_string_lossy(), &ws);
    assert!(thu_muc.unwrap_err().contains("thư mục"));
    assert!(marked_file("huba/.tmp/goi.zip", &ws)
        .unwrap_err()
        .contains("tuyệt đối"));
}

#[test]
fn an_unsendable_marker_says_so_on_its_own_line() {
    let (_dir, ws) = cay();
    let tot = ws.join("huba/.tmp/goi.zip");
    let mat = ws.join("huba/khong-co.md");
    let text = format!("Đầu.\n📎 {}\n📎 {}\nCuối.", tot.display(), mat.display());
    let out = note_unsendable_marks(&text, &ws);
    let dong: Vec<&str> = out.lines().collect();
    assert_eq!(dong.len(), 4, "{out}");
    assert_eq!(dong[0], "Đầu.");
    assert_eq!(
        dong[1],
        format!("📎 {}", tot.display()),
        "dòng tốt giữ nguyên"
    );
    assert!(
        dong[2].starts_with(&format!("📎 {}", mat.display()))
            && dong[2].contains("⚠ không thấy tệp"),
        "{out}"
    );
    assert_eq!(dong[3], "Cuối.");
    // Không có dòng hỏng ⟹ chữ y nguyên.
    let sach = format!("📎 {}", tot.display());
    assert_eq!(note_unsendable_marks(&sach, &ws), sach);
}

#[test]
fn a_marked_zip_gets_a_number_in_the_file_book() {
    let (db, dir0) = common::fresh_db();
    let (_dir, ws) = cay();
    let mut cfg = common::cfg_for_tests();
    cfg.workspace_root = ws.clone();
    let zip = ws.join("huba/.tmp/goi.zip").to_string_lossy().to_string();
    let tep = ghi_so_tep(
        &db,
        &cfg,
        "0f3c2a11-1111-4111-8111-111111111111",
        std::slice::from_ref(&zip),
    );
    assert_eq!(tep.len(), 1, "{tep:?}");
    assert_eq!(tep[0].neo, zip);
    drop(dir0);
}

/// Chỗ nối: cả ba đường sản phẩm phải đọc DẤU, không gọi lại phép đoán. `None`
/// = thiếu mỏ neo ⟹ ĐỎ.
fn dem(src: &str) -> (usize, usize) {
    (
        src.matches("crate::keys::paths_on_screen(").count(),
        src.matches("crate::keys::file_markers(").count(),
    )
}

#[test]
fn every_product_path_reads_the_marker_not_the_guess() {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs");
    let src = std::fs::read_to_string(p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: {p}: {e}"));
    let (doan, dau) = dem(&src);
    assert_eq!(doan, 0, "còn đường sản phẩm gọi phép đoán");
    assert!(
        dau >= 3,
        "ba đường (báo tin · /shot · trả lời route) phải đọc dấu: {dau}"
    );
    // Đối chứng ngược: hình dạng trước 06/10.
    let cu = "let seen = paths_not_in_commands(scan, &crate::keys::paths_on_screen(scan, PATHS_SCAN_MAX), &cmds);";
    assert_eq!(dem(cu), (1, 0));
}
