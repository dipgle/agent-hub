//! Đọc màn của MỘT cửa sổ bằng đúng mắt của hub, in ra để người đọc tự chấm.
//!
//! ```
//! HUB_TTY=ttys000 cargo test --offline --test read_one_screen_live -- --ignored --nocapture
//! ```

#[test]
#[ignore = "cần một cửa sổ Terminal thật"]
fn read_it() {
    let tty = std::env::var("HUB_TTY").unwrap_or_else(|_| "ttys000".to_string());
    let lines: usize = std::env::var("HUB_LINES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(45);

    match hub::keys::screen_of(&tty, lines) {
        Some((screen, choices)) => {
            println!("── màn {tty} ({} ký tự) ──", screen.len());
            println!("{screen}");
            println!("── hub đọc ra {} lựa chọn ──", choices.len());
            for (n, l) in &choices {
                println!("  {n}. {l}");
            }
            println!(
                "── có dòng chân hộp chọn: {} ──",
                hub::keys::has_chooser_footer(&screen)
            );
        }
        None => println!(
            "⚠ hub KHÔNG đọc được màn {tty} (không có cửa sổ, osascript hỏng, hoặc màn bị giữ lại)"
        ),
    }
}
