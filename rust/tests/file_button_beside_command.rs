//! Một tệp được nhắc như MỘT TỆP vẫn phải có nút tải, dù chỗ khác có dòng lệnh
//! mở chính nó.
//!
//! 🔴 Hà 2026-08-18, ảnh chụp một tin `/shot` của phiên dwork: *"Nội dung này có
//! file html nhưng lại không có nút tải"*. Tin ấy nhắc tệp HAI lần, và đó là cả
//! vấn đề:
//!
//! ```text
//! ## Báo cáo
//!
//! **`~/projects/dwork/dev/docs/bao-cao/bao-cao-ra-soat-2026-08-18.html`**   ← tệp
//!
//! ```
//! open ~/projects/dwork/dev/docs/bao-cao/bao-cao-ra-soat-2026-08-18.html    ← lệnh
//! ```
//! ```
//!
//! `paths_not_in_commands` (16/08, đúng cho ca của nó: `rm ~/…/x.rs` không được
//! mọc nút TẢI trên tệp mà dòng ấy bảo XOÁ) lọc theo ĐƯỜNG DẪN chứ không theo
//! DÒNG. Nên lần nhắc độc lập ở trên mất nút, chỉ vì bên dưới có một dòng lệnh
//! nhắc lại cùng tệp — mà "nhắc lại cùng tệp" chính là hình dạng thường gặp
//! nhất: phiên nào viết xong báo cáo cũng đưa đường dẫn rồi đưa lệnh mở.
//!
//! Fixture là NGUYÊN VĂN tin ấy, lấy từ `logs/huba.log` (`channel_command_handled`,
//! `kind: Shot`, 1964 byte).

use std::path::Path;

fn shot_text() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/shot-html-report-2026-08-18.txt");
    std::fs::read_to_string(p).expect("fixture nằm cạnh bài kiểm")
}

const REPORT: &str = "~/projects/dwork/dev/docs/bao-cao/bao-cao-ra-soat-2026-08-18.html";

/// Tầng 1 — huba CÓ nhìn thấy đường dẫn ấy trên màn.
///
/// Tách riêng vì hai tầng hỏng cho ra cùng một triệu chứng ("không có nút"), và
/// đoán nhầm tầng là sửa nhầm chỗ: đường dẫn nằm trong `**`…`**`, nên nếu bộ dò
/// nuốt cả dấu nháy ngược thì `is_file` trượt và mọi thứ sau đó vô nghĩa.
#[test]
fn the_path_is_seen_on_the_screen() {
    let seen = huba::keys::paths_on_screen(&huba::keys::body_before_box(&shot_text()), 4);
    assert!(
        seen.iter().any(|p| p == REPORT),
        "bộ dò không đọc ra đường dẫn báo cáo: {seen:?}"
    );
}

/// Tầng 2 — và phép lọc "đường dẫn nằm trong dòng lệnh" không được nuốt nó.
#[test]
fn a_path_mentioned_on_its_own_line_keeps_its_button() {
    let text = shot_text();
    let seen = huba::keys::paths_on_screen(&huba::keys::body_before_box(&text), 4);
    let cmds = huba::keys::commands_in_report(&text, 8);
    let cmds: Vec<huba::sessions::Cmd> = cmds
        .into_iter()
        .map(|line| huba::sessions::Cmd {
            line,
            cwd: String::new(),
        })
        .collect();
    let kept = huba::pipeline::paths_not_in_commands(&text, &seen, &cmds);
    assert!(
        kept.iter().any(|p| p == REPORT),
        "tệp được nhắc riêng một dòng vẫn mất nút tải: {kept:?}"
    );
}

/// Ca 16/08 phải giữ nguyên: đường dẫn CHỈ xuất hiện trong dòng lệnh thì không
/// mọc nút tải — dòng `rm` có đích chạm riêng (▶️/🖥), và 📎 ở đó là mời tải về
/// đúng cái tệp vừa được bảo xoá.
#[test]
fn a_path_only_inside_a_command_still_gets_no_button() {
    let text = "Dọn tệp thăm dò:\n\nrm ~/projects/huba/rust/tests/probe_prompt_anchor.rs\n";
    let seen = huba::keys::paths_on_screen(&huba::keys::body_before_box(text), 4);
    let cmds = vec![huba::sessions::Cmd {
        line: "rm ~/projects/huba/rust/tests/probe_prompt_anchor.rs".into(),
        cwd: String::new(),
    }];
    let kept = huba::pipeline::paths_not_in_commands(text, &seen, &cmds);
    assert!(
        kept.is_empty(),
        "dòng lệnh lại mọc nút tải file — đúng thứ Hà chê 16/08: {kept:?}"
    );
}
