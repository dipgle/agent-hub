//! Gửi THẬT một tin vào buồng chat, rồi đọc lại thứ Telegram nói nó đã hiển
//! thị — và xoá tin đi.
//!
//! 🔴 Hà 2026-08-16: *"Ko nhìn thấy sao gửi tele tôi lại thấy, cách bạn nhìn là
//! gì"* · *"Rõ ràng nhận được tin thấy text, bạn nhìn kiểu gì? Cứ cãi quanh co
//! mãi"* — hỏi sau khi tôi khai "chưa nghiệm thu được vì phải xem trên điện
//! thoại". Khai như thế là sai: `sendMessage` **trả về đối tượng Message**, kèm
//! `entities` — bản dịch của chính Telegram cho chuỗi HTML vừa gửi. Cái link có
//! nằm đúng sau dòng lệnh hay không là một con số đọc được từ máy này.
//!
//! Gắn `#[ignore]` vì nó gửi thật (và xoá thật). Chạy tay:
//!
//! ```
//! cd ~/projects/huba/rust
//! HUB_CONFIG=$HOME/projects/huba/huba.config.json \
//!   cargo test --offline --test telegram_live -- --ignored --nocapture
//! ```

/// Hai nút ⏎/⌫ rơi đúng dòng ô nhập — hỏi CHÍNH Telegram, không nhìn bằng mắt.
///
/// 🔴 Hà 2026-08-16, ảnh chụp 08:01: *"sao lại chèn 2 nút vào cuối thế này"*.
/// Bài kiểm thuần đã ghim luật, nhưng thứ hỏng lần trước là ở đúng khâu render:
/// nên hỏi lại bằng `entities` — Telegram nói link nằm ở ký tự thứ mấy, và chữ
/// ngay trước nó là gì.
#[test]
#[ignore = "gửi một tin thật vào buồng chat rồi xoá — chạy tay bằng --ignored"]
fn the_two_keys_really_land_on_the_prompt_line() {
    let cfg = huba::config::load(None).expect("HUB_CONFIG trỏ vào huba.config.json thật");
    let tg = huba::telegram::Inbox::start(&cfg, None).expect("có bot token + chat id");

    let tin = "📷 (tin kiểm tra, sẽ tự xoá) Màn của [tfl5]:\n\
               ✻ Sautéed for 6m 36s\n\
               ────────────────────────\n\
               ❯ chạy deploy đi\n\
               ────────────────────────\n\
               ⏵⏵ auto mode on · ← 1 agent\n\
               \n\
               Lệnh phiên chạy không được (cổng quyền chặn):";
    let anchors = vec![(
        "chạy deploy đi".to_string(),
        vec![
            (
                "https://t.me/ai_angles_bot?start=send_bab47095".to_string(),
                "⏎".to_string(),
            ),
            (
                "https://t.me/ai_angles_bot?start=clr_bab47095".to_string(),
                "⌫".to_string(),
            ),
        ],
    )];
    let (html, linked, _) = huba::pipeline::html_with_links(tin, &anchors);
    assert_eq!(linked, 2);

    let sent = tg.send_html_report(&html, &[]).expect("Telegram nhận tin");
    println!("--- Telegram hiển thị ---\n{}", sent.text);
    for (i, l) in sent.links.iter().enumerate() {
        println!(
            "  [{i}] {:?} → {}  @utf16 {}  (ngay sau: {:?})",
            l.label,
            l.url,
            l.at,
            sent.before_link(i)
        );
    }
    let removed = tg.delete_message(sent.message_id);

    assert_eq!(sent.links.len(), 2, "{:?}", sent.links);
    // ⚠ Nút thứ hai đứng SAU nút thứ nhất, nên chữ ngay trước nó là
    // `"❯ chạy deploy đi ⏎"`. So bằng `==` ở đây là bắt hai nút cùng đứng ở một
    // chỗ — một đòi hỏi vô lý, và nó làm bài kiểm ĐỎ trong khi mã đúng. Đúng
    // câu hỏi cần hỏi: cả hai có nằm trên DÒNG ấy không.
    for i in 0..2 {
        assert!(
            sent.before_link(i).starts_with("❯ chạy deploy đi"),
            "nút {i} không nằm trên dòng ô nhập: {:?}",
            sent.before_link(i)
        );
    }
    assert!(sent.links[0].url.contains("send_bab47095"));
    assert!(sent.links[1].url.contains("clr_bab47095"));
    removed.expect("xoá được tin thử");
}

#[test]
#[ignore = "gửi một tin thật vào buồng chat rồi xoá — chạy tay bằng --ignored"]
fn a_command_line_really_carries_its_run_link_on_telegram() {
    let cfg = huba::config::load(None).expect("HUB_CONFIG trỏ vào huba.config.json thật");
    let tg =
        huba::telegram::Inbox::start(&cfg, None).expect("có bot token + chat id trong huba.env");

    // Chuỗi đi qua ĐÚNG hàm sản phẩm, không phải một bản chép cho dễ.
    let text = "Kiểm tra đường gửi (tin này sẽ tự xoá):\n\
                cd ~/projects/huba && ./huba doctor\n\
                dòng sau lệnh, để xem link có bám đúng chỗ không.";
    let cmds = vec!["cd ~/projects/huba && ./huba doctor".to_string()];
    let (html, linked, unlinked) = huba::pipeline::html_with_command_links(text, &cmds, &|i| {
        Some((
            format!("https://t.me/hub_probe?start=run_{i}"),
            "▶️".to_string(),
        ))
    });
    assert_eq!(linked, 1);
    assert!(unlinked.is_empty());

    let sent = tg
        .send_html_report(
            &html,
            &[("👁 Vào phiên".to_string(), "enter:abc".to_string())],
        )
        .expect("Telegram nhận tin");

    println!("message_id={}", sent.message_id);
    println!("--- chữ Telegram HIỂN THỊ ---\n{}", sent.text);
    println!("--- liên kết Telegram DỰNG ---");
    for (i, l) in sent.links.iter().enumerate() {
        println!(
            "  [{i}] {:?} → {}   @utf16 {}  (ngay sau: {:?})",
            l.label,
            l.url,
            l.at,
            sent.before_link(i)
        );
    }

    // Dọn trước khi assert: một bài kiểm đỏ không được để lại rác trong buồng
    // chat của chủ máy.
    let removed = tg.delete_message(sent.message_id);

    // 1) Thẻ KHÔNG được hiện ra thành chữ — nếu `parse_mode` không ăn thì người
    //    đọc thấy `<a href=...>` giữa câu.
    assert!(
        !sent.text.contains("<a href"),
        "thẻ hiện ra thành chữ ⟹ Telegram không parse HTML: {}",
        sent.text
    );
    // 2) Đúng MỘT liên kết, và nó là cái icon.
    assert_eq!(sent.links.len(), 1, "{:?}", sent.links);
    assert_eq!(sent.links[0].label, "▶️");
    assert!(sent.links[0].url.contains("start=run_0"));
    // 3) …và nó nằm NGAY SAU dòng lệnh, không phải cuối tin. Đây là câu hỏi
    //    Hà hỏi bằng mắt, hỏi lại bằng số.
    assert_eq!(
        sent.before_link(0),
        "cd ~/projects/huba && ./huba doctor",
        "link không bám vào dòng lệnh"
    );
    // 4) Cả tin là MỘT tin: dòng sau lệnh phải nằm trong chính nó.
    assert!(
        sent.text.contains("dòng sau lệnh"),
        "tin bị cắt mất phần đuôi: {}",
        sent.text
    );
    removed.expect("xoá được tin thử");
}
