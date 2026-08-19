//! In ra NGUYÊN VĂN tin `/shot` sau khi gắn neo — để nhìn đúng thứ Hà nhìn.
//!
//! 🔴 Hà 2026-08-19, lần thứ hai: *"Sao không chèn trực tiếp vào nội dung tab
//! của phiên mà chèn xuống cuối thông tin phiên, có đang hiểu nhầm ý của tôi
//! không"*. Hai lần liên tiếp cùng một lời chê thì thứ phải kiểm không phải mã,
//! mà là **tin đã dựng xong** — nên bài kiểm này in nó ra.
//!
//! Fixture là nguyên văn tin của phiên `[AI/tcc/amm]` lấy từ `logs/hub.log`.
//!
//! ```text
//! cargo test --offline --test tab_anchor_live_shape -- --ignored --nocapture
//! ```

use std::path::Path;

fn ack() -> String {
    let p =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shot-amm-tabbar-2026-08-19.txt");
    std::fs::read_to_string(p).expect("fixture nằm cạnh bài kiểm")
}

#[test]
#[ignore = "in ra để NHÌN — chạy tay bằng --ignored --nocapture"]
fn show_me_the_message_as_telegram_gets_it() {
    hub::telegram::set_bot_username("hub_test_bot");
    let data = hub::pipeline::SessionData {
        sid: "da29807e".into(),
        tabs: vec![
            (1, "RPC pool".into(), true),
            (2, "NativeAssets v3".into(), false),
            (3, "Việc tiếp".into(), false),
        ],
        submit: true,
        ..Default::default()
    };
    let out = hub::pipeline::render_session_data(&ack(), &data);
    for (i, l) in out.lines().enumerate() {
        println!("{i:>3} | {l}");
    }
}
