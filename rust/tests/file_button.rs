//! Nút 📎 chỉ mọc khi có TỆP THẬT — và tệp trong workspace vẫn tính.
//!
//! 🔴 Hà 2026-08-16, ảnh chụp tin của `[tcc/browser]`: *"Rõ ràng trong nội dung
//! có file .html nhưng lại không có nút để tải được về"*. Tệp hôm ấy có thật
//! (`~/projects/AI/tcc/danh-gia-tccbrowser.html`, 28 KB) nhưng nằm ở `tcc/`
//! trong khi phiên đứng ở `tcc/browser/` — và chính phiên đã nói vì sao: *"để
//! không làm bẩn cây git của kho công khai"*.
//!
//! Log lúc ấy đã kể đúng chuyện: `quick_files_filtered {"kept":0,"seen":1}`.

use huba::pipeline::sendable_file;

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("huba-file-btn-{name}"));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(d.join("AI/tcc/browser")).unwrap();
    d
}

#[test]
fn a_real_file_beside_the_session_folder_still_gets_a_button() {
    let ws = tmp("beside");
    let root = ws.join("AI/tcc/browser");
    let bao_cao = ws.join("AI/tcc/danh-gia.html");
    std::fs::write(&bao_cao, "<h1>bản đánh giá</h1>").unwrap();

    // Đúng hình dạng phiên viết ra: đường dẫn tuyệt đối, ngoài thư mục phiên,
    // trong workspace.
    assert_eq!(
        sendable_file(bao_cao.to_str().unwrap(), &root, &ws),
        Some(bao_cao.clone()),
        "tệp có thật trong workspace phải gửi được"
    );

    // Tệp NẰM TRONG thư mục phiên thì vẫn như cũ.
    let trong = root.join("ghi-chu.md");
    std::fs::write(&trong, "x").unwrap();
    assert!(
        sendable_file("ghi-chu.md", &root, &ws).is_some(),
        "đường dẫn tương đối"
    );
    assert!(sendable_file(trong.to_str().unwrap(), &root, &ws).is_some());

    // 🔴 Ca GỐC của cửa này (Hà 2026-08-14: *"Com.dipgle.hubd.plist đâu phải là
    // file"*): một cái TÊN nhắc giữa câu văn — không có tệp nào để gửi.
    assert!(sendable_file("com.dipgle.hubd.plist", &root, &ws).is_none());

    // …và hàng rào thật vẫn đứng: ngoài workspace thì không gửi, dù có thật.
    let ngoai = std::env::temp_dir().join("huba-file-btn-ngoai.txt");
    std::fs::write(&ngoai, "bí mật").unwrap();
    assert!(
        sendable_file(ngoai.to_str().unwrap(), &root, &ws).is_none(),
        "tệp ngoài workspace KHÔNG được rời khỏi máy"
    );

    // Thư mục cũng không phải tệp.
    assert!(sendable_file(root.to_str().unwrap(), &root, &ws).is_none());

    std::fs::remove_dir_all(&ws).ok();
    std::fs::remove_file(&ngoai).ok();
}
