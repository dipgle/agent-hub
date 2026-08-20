//! `/anh` chạy THẬT: chụp, đo, và nói đúng vì sao khi khung hình rỗng.
//!
//! 🔴 Bài kiểm thuần (`tests/black_frame.rs`) chỉ nói hàm chọn đúng câu cho mỗi
//! trạng thái. Nó không nói `screencapture` trên máy NÀY ra cái gì, cũng không
//! nói `sips` có đo được tấm ảnh ấy không — mà đó mới là thứ Hà nhận trên điện
//! thoại.
//!
//! ```
//! cd ~/projects/huba/rust
//! cargo test --offline --test black_frame_live -- --ignored --nocapture
//! ```

use std::process::Command;

/// Cửa sổ nào cũng được — hàm chỉ dùng nó để đưa phiên ra trước khi bấm máy.
fn any_window() -> Option<i64> {
    let out = Command::new("osascript")
        .args([
            "-e",
            "tell application \"Terminal\" to return id of window 1",
        ])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[test]
#[ignore = "chụp màn hình thật — chạy tay bằng --ignored"]
fn a_blank_frame_is_refused_with_the_right_reason() {
    let w = any_window().expect("có ít nhất một cửa sổ Terminal");
    let path = std::env::temp_dir().join("huba-anh-live-test.png");
    let locked = huba::keys::screen_locked();
    println!("màn đang khoá: {locked:?}");

    match huba::keys::photograph_window(w, &path) {
        Ok(()) => {
            // Máy đang mở khoá VÀ có quyền ⟹ ảnh thật. Phải không rỗng, và
            // `frame_is_blank` phải nói đúng như thế.
            let blank = huba::keys::frame_is_blank(&path);
            println!("chụp được; khung rỗng: {blank:?}");
            assert_eq!(blank, Some(false), "ảnh gửi đi mà rỗng thì là ảnh vô dụng");
            let _ = std::fs::remove_file(&path);
        }
        Err(e) => {
            let msg = e.to_string();
            println!("huba từ chối gửi, và nói:\n{msg}");
            // 🔴 Điều quan trọng nhất: KHÔNG để lại tấm ảnh đen trên đĩa, và
            // câu từ chối phải khớp trạng thái ĐO ĐƯỢC — không đổ bừa cho quyền.
            assert!(!path.exists(), "ảnh rỗng phải bị xoá, không được gửi đi");
            match locked {
                Some(true) => assert!(
                    msg.contains("màn hình đăng nhập") && !msg.contains("Screen Recording"),
                    "máy đang khoá mà lại đổ cho quyền: {msg}"
                ),
                Some(false) => assert!(
                    msg.contains("Screen Recording"),
                    "màn không khoá mà không nhắc quyền: {msg}"
                ),
                None => assert!(msg.contains("không đo được"), "{msg}"),
            }
        }
    }
}
