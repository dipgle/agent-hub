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
        hub::keys::has_chooser_footer(&man)
    );
    let choices = hub::keys::parse_choices(&man);
    println!("parse_choices → {} lựa chọn", choices.len());
    for (n, s) in &choices {
        println!("  {n}. {s}");
    }
    println!("--- ô nhập ---");
    println!("{:?}", hub::keys::input_box_text(&man));
    println!("--- look ---");
    match hub::keys::look_from_screen(&man, 6) {
        hub::keys::Look::Saw { choices, .. } => println!("Saw, {} lựa chọn", choices.len()),
        hub::keys::Look::Withheld { choices, risk } => {
            println!("Withheld ({risk:?}), {choices} lựa chọn")
        }
        hub::keys::Look::Blind { why } => println!("Blind: {why}"),
    }
}
