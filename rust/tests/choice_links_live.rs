//! Nút chọn phải nằm NGAY TẠI dòng option — hỏi chính Telegram, không nhìn mắt.
//!
//! 🔴 Hà 2026-08-16: *"nút chọn phải chèn ngay tại các dòng chọn tại chính chỗ
//! option chứ không phải ném thêm xuống cuối"*. Màn dùng ở đây là bản chụp THẬT
//! của phiên `[mailler]` (`HUB_SCREEN`).

#[test]
#[ignore = "gửi một tin thật vào buồng chat rồi xoá"]
fn every_option_line_carries_its_own_tick() {
    let cfg = huba::config::load(None).expect("HUB_CONFIG");
    let path = std::env::var("HUB_SCREEN").expect("HUB_SCREEN trỏ vào bản chụp màn");
    let man = std::fs::read_to_string(&path).expect("đọc được màn");

    let choices = huba::keys::parse_choices(&man);
    assert_eq!(choices.len(), 5, "màn phải có 5 lựa chọn: {choices:?}");

    let data = huba::pipeline::SessionData {
        sid: "f168de42-5cfb-44cb-a97e-1861cbe0b49c".to_string(),
        // `parse_choices` trả `(số, nhãn)`; `SessionData` mang MÃ dạng chuỗi
        // (`"3"` cho hộp một câu, `"1.3"` cho bảng nhiều câu) — xem
        // `pipeline::session_layout`.
        choices: choices
            .iter()
            .map(|(n, l)| (n.to_string(), l.clone()))
            .collect(),
        ..Default::default()
    };
    let tg = huba::telegram::Inbox::start(&cfg, None).expect("có bot token");
    // `getMe` chạy ở luồng nền; `deep_link` không có tên bot thì trả `None` —
    // đúng luật, nên bài kiểm phải CHỜ chứ không được đọc kết quả sớm.
    for _ in 0..40 {
        if huba::telegram::deep_link("k_x_1").is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    println!(
        "deep_link thử: {:?}",
        huba::telegram::deep_link("k_f168de42_1")
    );
    let sent = huba::pipeline::render_session_data(&man, &data);
    println!(
        "html dài {} ký tự, có thẻ a: {}",
        sent.len(),
        sent.contains("<a href")
    );
    let msg = tg.send_html_report(&sent, &[]).expect("Telegram nhận tin");

    println!("--- liên kết Telegram dựng ---");
    for (i, l) in msg.links.iter().enumerate() {
        println!(
            "  [{i}] {} → {}   @utf16 {}  (ngay sau: {:?})",
            l.label,
            l.url,
            l.at,
            msg.before_link(i)
        );
    }
    let removed = tg.delete_message(msg.message_id);

    assert_eq!(
        msg.links.len(),
        5,
        "phải có đúng 5 nút chọn: {:?}",
        msg.links
    );
    for (i, (n, label)) in choices.iter().enumerate() {
        assert_eq!(msg.links[i].label, "☑");
        assert!(
            msg.links[i].url.ends_with(&format!("_{n}")),
            "nút {n} trỏ sai: {}",
            msg.links[i].url
        );
        assert!(
            msg.before_link(i).contains(label.as_str()),
            "nút {n} không nằm trên dòng của nó — đứng sau: {:?}",
            msg.before_link(i)
        );
    }
    removed.expect("xoá được tin thử");
}
