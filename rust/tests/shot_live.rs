//! Chạy ĐÚNG đường `/shot` vào một phiên thật rồi đếm nút — nghiệm thu, không đoán.
//!
//! ```
//! HUB_CONFIG=$HOME/projects/huba/huba.config.json HUB_TTY=ttys005 \
//!   cargo test --offline --test shot_live -- --ignored --nocapture
//! ```

#[test]
#[ignore = "gửi một tin thật vào buồng chat — chạy tay bằng --ignored"]
fn shot_a_session_with_a_chooser_on_screen() {
    let cfg = huba::config::load(None).expect("HUB_CONFIG trỏ vào huba.config.json thật");
    let tty = std::env::var("HUB_TTY").unwrap_or_else(|_| "ttys005".to_string());

    let snap = huba::sessions::snapshot(&cfg);
    let s = snap
        .sessions
        .iter()
        .find(|s| s.tty.trim_start_matches("/dev/") == tty)
        .unwrap_or_else(|| panic!("không thấy phiên nào ở {tty}"));
    println!("phiên {} ({}) — {}", s.session_id, s.label, s.name);

    // Đọc màn qua đúng đường huba đọc, rồi hỏi bộ nhận dạng.
    let w = huba::keys::window_of(&s.tty)
        .expect("hỏi được Terminal")
        .expect("phiên có cửa sổ");
    let man = huba::keys::screen_text(w).expect("đọc được màn");
    let choices = huba::keys::parse_choices(&man);
    println!("màn có {} lựa chọn: {:?}", choices.len(), choices);
    println!(
        "bảng hỏi trong nhật ký: {:?}",
        huba::sessions::asking_of(&cfg, &s.session_id).map(|a| (
            a.header.clone(),
            a.options.len(),
            a.rest.len()
        ))
    );

    assert!(
        !choices.is_empty(),
        "màn đang mở hộp chọn mà huba đọc ra 0 — bộ nhận dạng hỏng"
    );

    // Và ĐẨY một lượt `/shot` thật qua kênh, để đếm nút trên tin gửi đi.
    let tg = huba::telegram::Inbox::start(&cfg, None).expect("có bot token");
    tg.push_text(&format!("/shot {}", s.session_id));
    huba::pipeline::run_telegram_now(&cfg);
    println!("đã đẩy /shot — xem log `telegram_buttons_sent` để đếm nút");
}
