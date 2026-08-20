//! Đường dẫn tệp trong CÂU VĂN của phiên — nhận cái nào, bỏ cái nào.
//!
//! 🔴 Hà 2026-08-16, đọc một bản *"Xem đầy đủ"* có nhắc `docs/flow-boc-tach-lenh.md`:
//! *"nhận được tin có file nhưng chưa có nút tải hay xem"* · *"Có file .md đấy"*.
//! Luật cũ chỉ nhận đường TUYỆT ĐỐI, nên mọi đường tương đối trong báo cáo của
//! phiên đều là chữ chết trên điện thoại.

use huba::keys::paths_on_screen;
use huba::pipeline::sendable_file;

/// Một cây tạm để đo phần "hỏi đĩa" — không mock, vì chính phép hỏi đĩa là thứ
/// đang được kiểm.
fn tmp_tree(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("huba-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("docs/history")).unwrap();
    std::fs::create_dir_all(root.join("target/debug")).unwrap();
    root
}

/// Tên tệp TRẦN được tìm thấy trong cây phiên — Hà: *"phải tìm được file ở đĩa"*.
#[test]
fn a_bare_name_is_found_by_searching_the_session_tree() {
    let root = tmp_tree("find");
    std::fs::write(root.join("docs/history/ghi-chu.md"), "x").unwrap();
    let got = sendable_file("ghi-chu.md", &root, &root).expect("phải tìm ra");
    assert!(got.ends_with("docs/history/ghi-chu.md"), "{got:?}");
}

/// HAI tệp trùng tên ⟹ TỪ CHỐI. Đoán ở đây là gửi nhầm tệp, và người đọc không
/// có cách nào biết mình đang đọc sai.
#[test]
fn two_files_with_the_same_name_get_no_button() {
    let root = tmp_tree("ambig");
    std::fs::write(root.join("docs/README.md"), "a").unwrap();
    std::fs::write(root.join("docs/history/README.md"), "b").unwrap();
    assert!(sendable_file("README.md", &root, &root).is_none());
}

/// `target/` không được quét: nó nặng, và không ai nhắc tới tệp trong đó.
#[test]
fn the_search_skips_build_output() {
    let root = tmp_tree("skip");
    std::fs::write(root.join("target/debug/build-log.txt"), "x").unwrap();
    assert!(sendable_file("build-log.txt", &root, &root).is_none());
}

#[test]
fn a_relative_doc_path_in_prose_is_a_file() {
    let text = "Đây là lần thứ ba cùng một hình dạng (12/08 /type, 16/08 /runin, nay đường \
                gợi ý mờ) — tôi đã ghi nó thành luật trong mã.\n\n\
                Còn mỗi docs/flow-boc-tach-lenh.md chờ anh (giữ hay xoá).";
    let got = paths_on_screen(text, 4);
    assert_eq!(got, vec!["docs/flow-boc-tach-lenh.md".to_string()]);
}

/// …và câu văn thường KHÔNG được đẻ ra nút. Đây là cái giá phải trả cho việc
/// nới luật, nên nó phải có phép đo riêng.
///
/// Con số và ngày tháng giữa câu không được thành ứng viên.
///
/// `Node.js` thì CÓ (đuôi `js` hợp lệ) và đó là cái giá đã biết của bản
/// 2026-08-17: nó chết ở bước sau, khi đĩa trả lời "không có tệp nào tên vậy".
/// Đổi lại, `TODO.md` viết trần — thứ có thật và đáng bấm — mới đi qua được.
#[test]
fn ordinary_prose_makes_no_buttons() {
    let text = "Ngày 12/08 và 16/08, v.v. — Tỷ lệ 3.5 vs 4.0, xong rồi.";
    assert!(
        paths_on_screen(text, 4).is_empty(),
        "{:?}",
        paths_on_screen(text, 4)
    );
}

/// 🔴 ĐẢO CHIỀU 2026-08-17 (bản cũ: `a_bare_file_name_without_a_folder_is_not_enough`).
///
/// Hà, ảnh `/shot` phiên `[dwork]`: *"Trong nội dung có file *.md chưa chèn link
/// tải, phải tìm được file ở đĩa"*. Màn ấy có `TODO.md`, `active-context.md`
/// viết trần, và một đường bị cửa sổ bẻ đôi (`docs/` cuối dòng, tên tệp dòng
/// sau) — đòi token phải chứa `/` là loại sạch cả ba, mà cả ba đều có thật.
///
/// Ở ĐÂY chỉ là bước nhặt ứng viên. Câu "có thật không" do `sendable_file` trả
/// lời bằng đĩa: giải theo cây phiên, không thấy thì đi tìm, hai tệp trùng tên
/// thì từ chối.
#[test]
fn a_bare_file_name_is_a_candidate_for_the_disk_to_judge() {
    assert_eq!(
        paths_on_screen("xem README.md nhé", 4),
        vec!["README.md".to_string()]
    );
}

/// Đường tuyệt đối giữ nguyên đường cũ, kể cả đuôi lạ.
#[test]
fn absolute_paths_still_work_with_any_text_extension() {
    let text = "xem ~/projects/huba/huba.config.json và /etc/hosts.allow nhé";
    let got = paths_on_screen(text, 4);
    assert!(
        got.contains(&"~/projects/huba/huba.config.json".to_string()),
        "{got:?}"
    );
}

/// Tệp nhị phân thì không, dù đường tuyệt đối — bấm vào cũng không gửi được.
#[test]
fn binary_files_never_become_buttons() {
    let text = "ảnh ở ~/projects/huba/ui-shots/after.png";
    assert!(paths_on_screen(text, 4).is_empty());
}
