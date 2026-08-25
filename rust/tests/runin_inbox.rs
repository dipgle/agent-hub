//! Hòm thư của phiên: phiên tự xếp lệnh, huba đọc id TỪ ĐƯỜNG DẪN.
//!
//! 🔴 Hà 2026-08-24: *"Phiên a nhận được lệnh chạy → gửi lệnh runin cho huba →
//! huba đẩy vào hàng chờ để chạy → chạy xong lấy kết quả dán vào hàng chờ của
//! phiên a"*, sau khi hỏi *"phải có hướng dẫn để phiên tự lấy đúng id mà huba
//! đang quản lý nó gửi kèm"*.
//!
//! 📌 Không phiên nào phải khai id, và đó là cả thiết kế. Hai ràng buộc có sẵn
//! ghép đúng vào nhau:
//! ① luật workspace — một phiên chỉ được GHI trong thư mục của chính nó, nên
//!    hòm thư không thể nằm trong cây của huba;
//! ② scratchpad của mỗi phiên **mang sẵn id trong đường dẫn**
//!    (`…/claude-501/<slug>/<id phiên>/scratchpad`) — đo 2026-08-24: 4/4 thư
//!    mục lấy mẫu đều có nhật ký `.jsonl` trùng tên.
//!
//! Phiên ghi vào đúng chỗ nó được phép ghi, và chính chỗ ấy khai hộ nó là ai.
//! Một id gõ tay thì gõ sai được; một id đọc từ đường dẫn thì không.

use huba::pipeline::{scan_session_inboxes, RUNIN_INBOX_NAME};
use std::fs;
use std::path::{Path, PathBuf};

const SID: &str = "14b6b4b4-8886-4e56-868f-b1e878dde69d";

/// Dựng đúng hình dạng thật: `<gốc>/claude-501/<slug>/<id>/scratchpad/<tệp>`.
fn put(root: &Path, uid_dir: &str, slug: &str, sid: &str, body: &str) -> PathBuf {
    let dir = root.join(uid_dir).join(slug).join(sid).join("scratchpad");
    fs::create_dir_all(&dir).unwrap();
    let f = dir.join(RUNIN_INBOX_NAME);
    fs::write(&f, body).unwrap();
    f
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("huba-inbox-test-{name}"));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_command_is_read_and_the_session_id_comes_from_the_path() {
    let root = tmp("basic");
    put(
        &root,
        "claude-501",
        "-Users-hanguyen-projects",
        SID,
        "cd ~/projects/huba/rust && cargo test --offline\n",
    );
    let got = scan_session_inboxes(&root);
    assert_eq!(got.len(), 1, "{got:#?}");
    assert_eq!(got[0].0, SID, "id phải đọc từ ĐƯỜNG DẪN, không từ nội dung");
    assert_eq!(got[0].1, "cd ~/projects/huba/rust && cargo test --offline");
}

/// 🔴 Thư mục KHÔNG mang hình dạng uuid thì bỏ qua.
///
/// Nhận bừa là dán kết quả vào một phiên không tồn tại — và cái thư mục ấy có
/// thể do bất cứ thứ gì tạo ra dưới `/private/tmp`.
#[test]
fn a_directory_that_is_not_a_uuid_is_ignored() {
    let root = tmp("notuuid");
    for bad in ["scratch", "12345678", "not-a-uuid", ".."] {
        put(&root, "claude-501", "slug", bad, "rm -rf ~\n");
    }
    assert!(
        scan_session_inboxes(&root).is_empty(),
        "{:#?}",
        scan_session_inboxes(&root)
    );
}

/// Chỉ đi vào thư mục `claude-*` — không duyệt cả `/private/tmp`.
#[test]
fn only_claude_owned_directories_are_walked() {
    let root = tmp("scope");
    put(&root, "somebody-else", "slug", SID, "rm -rf ~\n");
    assert!(scan_session_inboxes(&root).is_empty());
    put(&root, "claude-501", "slug", SID, "ls\n");
    assert_eq!(scan_session_inboxes(&root).len(), 1);
}

/// Dòng đầu KHÔNG rỗng là lệnh; phần còn lại là ghi chú của phiên.
///
/// Lấy dòng đầu chứ không nối cả tệp: nối là tự dựng lại đúng phép đoán vừa bỏ.
#[test]
fn the_first_non_empty_line_is_the_command() {
    let root = tmp("firstline");
    put(
        &root,
        "claude-501",
        "slug",
        SID,
        "\n\n   \nls ~/projects\nghi chú: chạy hộ tôi rồi dán kết quả về\n",
    );
    let got = scan_session_inboxes(&root);
    assert_eq!(got[0].1, "ls ~/projects", "{got:#?}");
}

/// Tệp rỗng (hoặc toàn dòng trắng) không sinh ra lệnh nào.
#[test]
fn an_empty_file_asks_for_nothing() {
    let root = tmp("empty");
    put(&root, "claude-501", "slug", SID, "\n   \n\n");
    assert!(scan_session_inboxes(&root).is_empty());
}

/// Nhiều phiên thì mỗi phiên một lệnh, và id không lẫn sang nhau.
#[test]
fn two_sessions_keep_their_own_ids() {
    let root = tmp("two");
    let other = "98068e79-509c-4af6-99c1-aaa19a783230";
    put(&root, "claude-501", "slug-a", SID, "ls a\n");
    put(&root, "claude-501", "slug-b", other, "ls b\n");
    let mut got = scan_session_inboxes(&root);
    got.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(got.len(), 2, "{got:#?}");
    assert_eq!((got[0].0.as_str(), got[0].1.as_str()), (SID, "ls a"));
    assert_eq!((got[1].0.as_str(), got[1].1.as_str()), (other, "ls b"));
}

/// Gốc không tồn tại thì trả rỗng, không hoảng.
#[test]
fn a_missing_root_is_not_a_crash() {
    assert!(scan_session_inboxes(Path::new("/khong-co-thu-muc-nay-dau")).is_empty());
}
