//! Đo bộ nhận dạng hộp chọn trên một MÀN THẬT đã chụp ra tệp.
//!
//! ```
//! HUB_SCREEN=/đường/dẫn/màn.txt cargo test --offline --test screen_probe -- --ignored --nocapture
//! ```

#[test]
#[ignore = "cần HUB_SCREEN trỏ vào một bản chụp màn thật"]
fn read_a_real_screen() {
    let path = std::env::var("HUB_SCREEN").expect("đặt HUB_SCREEN");
    let man = std::fs::read_to_string(&path).expect("đọc được màn");

    println!(
        "có dòng chân hộp chọn: {}",
        huba::keys::has_chooser_footer(&man)
    );
    let choices = huba::keys::parse_choices(&man);
    println!("parse_choices → {} lựa chọn", choices.len());
    for (n, s) in &choices {
        println!("  {n}. {s}");
    }
    println!("--- ô nhập ---");
    println!("{:?}", huba::keys::input_box_text(&man));
    println!("--- look ---");
    match huba::keys::look_from_screen(&man, 6) {
        huba::keys::Look::Saw { choices, .. } => println!("Saw, {} lựa chọn", choices.len()),
        // 🪦 Nhánh `Withheld` gỡ 2026-08-16 — xem bia mộ trong `keys::Look`.
        huba::keys::Look::Blind { why } => println!("Blind: {why}"),
    }
}
