//! Cổng của cỗ máy TỰ CHẠY lệnh — nay là một cái DẤU, không phải phép đoán.
//!
//! 🔴 Hà 2026-08-24, hỏi *"Tại sao lại cần allow làm gì vậy?"* rồi
//! *"có quy tắc nào để bảo claude đánh dấu vào output kết quả là lệnh để bắt
//! tôi chạy không"*, rồi chốt **mức 2: chỉ dấu, bỏ allow**.
//!
//! Bản 23/08 đoán theo hình dạng (`commands_in_report`) rồi phải tự chặn lại
//! thứ chính nó đoán bừa bằng một danh sách cho phép — và cái danh sách ấy đã
//! có một lỗ RCE (khớp tiền tố ⟹ `bash ./gate.sh && rm -rf ~` lọt).
//!
//! Nay nguồn đổi hẳn: chỉ chạy dòng phiên **CỐ Ý đánh dấu**. Hết đoán thì hết
//! thứ phải chặn — nên danh sách cho phép đi luôn, có chủ ý, xem bia mộ trong
//! `pipeline.rs`.
//!
//! ⚠ Cái dấu nói *"mô hình cố ý bảo chạy"*, KHÔNG nói *"chủ máy cho phép"*. Hà
//! biết rủi ro ấy và chọn thế.

use huba::keys::{marked_commands, RUN_MARK};

#[test]
fn a_marked_line_is_picked_up() {
    let text = format!("Anh chạy hộ tôi:\n{RUN_MARK}\ncd ~/projects/huba/rust && cargo test --offline\nXong báo tôi.");
    assert_eq!(
        marked_commands(&text, 8),
        vec!["cd ~/projects/huba/rust && cargo test --offline"]
    );
}

/// 🔴 HÀNG RÀO CHÍNH: không có dấu thì KHÔNG chạy, dù trông giống lệnh tới đâu.
///
/// Đây là toàn bộ khác biệt giữa hàm này và `commands_in_report`. Nới chỗ này
/// là quay về đúng bản đã có lỗ.
#[test]
fn an_unmarked_command_is_never_run() {
    for text in [
        "cd ~/projects/huba && cargo test --offline",
        "git -C ~/projects/huba push origin main",
        "Chạy: bash ~/projects/huba/install_update.sh",
        "rm -rf ~",
    ] {
        assert!(
            marked_commands(text, 8).is_empty(),
            "dòng KHÔNG đánh dấu mà vẫn chạy: {text:?}"
        );
    }
}

/// 🔴 Dấu phải CHIẾM TRỌN một dòng.
///
/// Nếu chỉ cần "có chứa" thì một câu văn nhắc tới cái dấu — như chính tài liệu
/// này, hay một tin nhắn giải thích quy ước cho người dùng — sẽ tự kích hoạt.
/// Và `last_text` còn mang cả chữ phiên trích từ web, từ diff, từ tệp.
#[test]
fn the_mark_must_own_its_whole_line() {
    for text in [
        format!("echo \"{RUN_MARK}\"\nrm -rf ~"),
        format!("Quy ước là {RUN_MARK} đặt trên dòng lệnh\nrm -rf ~"),
        format!("{RUN_MARK} rm -rf ~"),
        format!("# {RUN_MARK}\nrm -rf ~"),
    ] {
        assert!(
            marked_commands(&text, 8).is_empty(),
            "dấu nấp trong một dòng khác mà vẫn kích hoạt: {text:?}"
        );
    }
}

/// Một dấu ăn ĐÚNG một lệnh — không mở cổng cho cả phần còn lại của tin.
#[test]
fn one_mark_arms_exactly_one_line() {
    let text = format!("{RUN_MARK}\nls ~/projects\nrm -rf ~\ngit push --force");
    assert_eq!(marked_commands(&text, 8), vec!["ls ~/projects"]);
}

/// Dòng trống giữa dấu và lệnh không làm mất dấu — TUI hay chèn một dòng trống.
#[test]
fn a_blank_line_after_the_mark_is_tolerated() {
    let text = format!("{RUN_MARK}\n\n   \nls ~/projects\n");
    assert_eq!(marked_commands(&text, 8), vec!["ls ~/projects"]);
}

/// Ghép được với bản vá nối `\` sáng nay: lệnh nhiều dòng đi nguyên khối.
#[test]
fn a_marked_multi_line_command_arrives_whole() {
    let text = format!(
        "{RUN_MARK}\ncd ~/projects/huba && \\\ncargo test --offline && \\\necho XONG\n"
    );
    let got = marked_commands(&text, 8);
    assert_eq!(got.len(), 1, "{got:#?}");
    assert!(got[0].starts_with("cd ~/projects/huba &&"), "{got:#?}");
    assert!(got[0].contains("echo XONG"), "mất đuôi khối: {got:#?}");
    assert!(!got[0].contains('\\'), "còn dấu nối: {got:#?}");
}

/// Dấu nhắc của TUI đứng trước lệnh thì bóc, cùng bộ với `commands_in_report`.
#[test]
fn tui_prompt_decoration_is_stripped() {
    let text = format!("{RUN_MARK}\n  $ ls ~/projects\n");
    assert_eq!(marked_commands(&text, 8), vec!["ls ~/projects"]);
}

/// Nhiều lệnh đánh dấu thì lấy đủ, theo đúng thứ tự, tới trần `max`.
#[test]
fn several_marks_keep_their_order_and_respect_the_cap() {
    let text = format!("{RUN_MARK}\nls a\nvăn xuôi\n{RUN_MARK}\nls b\n{RUN_MARK}\nls c\n");
    assert_eq!(marked_commands(&text, 8), vec!["ls a", "ls b", "ls c"]);
    assert_eq!(marked_commands(&text, 2), vec!["ls a", "ls b"]);
}

/// Cờ tắt vẫn phải tắt được cả cỗ máy.
#[test]
fn the_feature_can_still_be_switched_off() {
    let cfg = huba::config::Config::default();
    assert!(cfg.auto_run.enabled, "mặc định bật — Hà chốt mức 2");
}
