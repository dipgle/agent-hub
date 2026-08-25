//! Telegram có cho **cả khối lệnh** làm đích chạm không — hỏi chính Telegram.
//!
//! 🔴 Hà 2026-08-23: *"Tại sao các nút chạy khối lệnh không bao cả khối như
//! cách làm ở danh sách phiên cho dễ bấm"*.
//!
//! Hàng phiên bọc CẢ HÀNG vào `<a>` được (`pipeline::tap_rows_html`) vì nó là
//! chữ thường. Dòng lệnh thì không giống thế: phần lệnh nằm trong `<code>` —
//! cố ý, để vẽ ranh giới "lệnh ăn mấy dòng" và để Telegram thôi tự biến
//! `deploy.sh` thành liên kết web. Nên câu hỏi thật là:
//!
//! > `<a href="…"><code>lệnh</code></a>` — Telegram giữ CẢ HAI định dạng, hay
//! > nuốt một cái?
//!
//! Đây là câu hỏi ĐO ĐƯỢC từ máy này: `sendMessage` trả về đối tượng Message
//! kèm `entities`, tức bản dịch của chính Telegram cho chuỗi HTML vừa gửi.
//! `Sent::spans` giữ MỌI entity (không riêng `text_link`) đúng vì phép đo này —
//! nhìn qua `links` thì "còn `code`" và "mất `code`" giống hệt nhau.
//!
//! ## Vòng 1 (2026-08-23 06:10Z) — Telegram NUỐT `<a>` khi nó bọc `<code>`
//!
//! Gửi ba hình dạng trong cùng một tin, đọc `entities` trả về:
//!   A — bản đang chạy: `<code>lệnh</code>` rồi hai icon ở ngoài → `code` +
//!       hai `text_link` dài **2 ký tự** (đúng bằng cái emoji).
//!   B — bọc cả khối: `<a href="…"><code>lệnh</code></a>` → **chỉ còn `code`,
//!       KHÔNG có `text_link` nào phủ nó**. Cái link biến mất không một lời
//!       báo: khối trông y hệt mà bấm không ăn. Đây là lý do câu hỏi này phải
//!       ĐO chứ không suy — hai kết cục nhìn từ ngoài giống hệt nhau.
//!   C — một DÒNG RIÊNG chạm được ở dưới → `text_link` dài **22 ký tự**.
//!
//! ⟹ `<code>` và `<a>` không chồng nhau được. Chọn một trong hai, hoặc tách ra
//! hai chỗ khác nhau trên màn.
//!
//! ## Vòng 2 — vậy bỏ `<code>` đi thì cả dòng lệnh chạm được chứ?
//!
//!   D — `<a href="…">lệnh</a>` + `🖥` ở ngoài: cả dòng lệnh là đích chạm,
//!       KHÔNG tốn thêm dòng nào. Câu phải trả lời kèm: mất `<code>` thì
//!       Telegram có tự nối liên kết vào `gate.sh` nữa không (`.sh` là TLD
//!       thật — đúng con bug 16/08), hay cái `<a>` bọc ngoài đã chặn rồi.
//!   E — như D, và cửa thứ hai (`🖥`) xuống một dòng riêng MANG CHỮ, thay vì
//!       một emoji trần cạnh dòng lệnh.
//!
//! Chạy tay (nó gửi thật rồi xoá thật):
//!
//! ```
//! cd ~/projects/huba/rust
//! HUB_CONFIG=$HOME/projects/huba/huba.config.json \
//!   cargo test --offline --test cmd_block_tap_live -- --ignored --nocapture
//! ```

/// In ra thứ Telegram nói nó vừa hiển thị, cho cả ba hình dạng.
///
/// Bài kiểm này KHÔNG khẳng định hình dạng nào đúng — nó đi lấy số đo để chọn.
/// Khẳng định duy nhất là khẳng định của phép đo: tin gửi được và Telegram có
/// dựng ít nhất một `text_link`. Nếu ngay cả thế cũng sai thì mọi kết luận rút
/// ra từ lượt chạy này đều vô nghĩa.
#[test]
#[ignore = "gửi một tin thật vào buồng chat rồi xoá — chạy tay bằng --ignored"]
fn telegram_says_whether_a_whole_command_block_can_be_tapped() {
    let cfg = huba::config::load(None).expect("HUB_CONFIG trỏ vào huba.config.json thật");
    let tg = huba::telegram::Inbox::start(&cfg, None).expect("có bot token + chat id");

    // Một lệnh có đuôi `.sh` LÀ CHỦ Ý: `.sh` là TLD có thật, nên nếu `<code>`
    // bị nuốt thì Telegram tự nối liên kết web vào `gate.sh` — và cái đó hiện
    // ra trong `spans` dưới dạng `url`. Tức phép đo bắt được cả tác dụng phụ,
    // không chỉ bắt cái entity mình hỏi.
    let cmd = "bash ./gate.sh --all";
    let run = "https://t.me/ai_angles_bot?start=run_deadbeef";
    let term = "https://t.me/ai_angles_bot?start=term_deadbeef";

    let html = format!(
        "🧪 (tin đo, sẽ tự xoá)\n\
         A <code>{cmd}</code> <a href=\"{run}\">▶️</a> <a href=\"{term}\">🖥</a>\n\
         D <a href=\"{run}\">{cmd}</a> <a href=\"{term}\">🖥</a>\n\
         E <a href=\"{run}\">{cmd}</a>\n\
         <a href=\"{term}\">🖥 chạy trong cửa sổ riêng</a>\n"
    );

    let sent = tg.send_html_report(&html, &[]).expect("Telegram nhận tin");
    println!("--- Telegram hiển thị ---\n{}\n---", sent.text);
    println!("--- entities ({}) ---", sent.spans.len());
    let u: Vec<u16> = sent.text.encode_utf16().collect();
    for (kind, at, len) in &sent.spans {
        let end = (at + len).min(u.len());
        println!(
            "  {kind:<12} @{at:<4} len {len:<4} = {:?}",
            String::from_utf16_lossy(&u[(*at).min(u.len())..end])
        );
    }
    println!("--- text_link ({}) ---", sent.links.len());
    for (i, l) in sent.links.iter().enumerate() {
        println!("  [{i}] {:?} → {}  @{}", l.label, l.url, l.at);
    }

    let removed = tg.delete_message(sent.message_id);
    assert!(!sent.spans.is_empty(), "Telegram không dựng entity nào");
    assert!(!sent.links.is_empty(), "không có liên kết nào bấm được");
    removed.expect("xoá được tin đo");
}

/// Nghiệm thu: chuỗi do CHÍNH HÀM SẢN PHẨM dựng, đi qua Telegram thật.
///
/// 🔴 Hai bài kiểm trên đo *hình dạng HTML tôi tự gõ vào bài kiểm* — nó trả lời
/// "Telegram chịu hình dạng nào", KHÔNG trả lời "huba có dựng ra đúng hình dạng
/// ấy không". Hai câu ấy đã lệch nhau một lần thật: 16/08 bài kiểm thuần ghim
/// đúng luật mà khâu render vẫn hỏng, và chỗ hỏng chỉ lòi ra khi hỏi `entities`.
///
/// Nên bài này gọi `html_with_command_links` — đúng hàm đường thật đi qua — rồi
/// đo trên thứ Telegram trả về: nhãn liên kết phải là **cả dòng lệnh**, không
/// phải hai ký tự emoji.
#[test]
#[ignore = "gửi một tin thật vào buồng chat rồi xoá — chạy tay bằng --ignored"]
fn the_real_render_path_makes_the_whole_command_line_tappable() {
    let cfg = huba::config::load(None).expect("HUB_CONFIG trỏ vào huba.config.json thật");
    let tg = huba::telegram::Inbox::start(&cfg, None).expect("có bot token + chat id");

    let cmd = "bash ~/projects/huba/install_update.sh";
    let text = format!("🧪 (tin đo, sẽ tự xoá) Cài bản mới:\n{cmd}\nXong thì báo lại.");
    let cmds = vec![cmd.to_string()];
    let link = |i: usize| -> Option<(String, String)> {
        Some((
            format!("https://t.me/ai_angles_bot?start=run_{i:x}"),
            "▶️".to_string(),
        ))
    };
    let (html, linked, unlinked) = huba::pipeline::html_with_command_links(&text, &cmds, &link);
    assert_eq!(linked, 1, "{html}");
    assert!(unlinked.is_empty(), "{unlinked:?}");

    let sent = tg.send_html_report(&html, &[]).expect("Telegram nhận tin");
    println!("--- Telegram hiển thị ---\n{}\n---", sent.text);
    for (kind, at, len) in &sent.spans {
        println!("  {kind:<12} @{at:<4} len {len}");
    }
    let removed = tg.delete_message(sent.message_id);

    let l = sent.links.first().cloned();
    removed.expect("xoá được tin đo");
    let l = l.expect("Telegram phải dựng đúng một text_link cho dòng lệnh");
    // Nhãn = icon + CẢ dòng lệnh. Đây là con số Hà hỏi: đích chạm rộng bằng
    // dòng lệnh chứ không bằng cái emoji.
    assert_eq!(
        l.label,
        format!("▶️ {cmd}"),
        "đích chạm không phủ trọn dòng lệnh"
    );
    assert!(
        l.label.chars().count() > 20,
        "đích chạm chỉ {} ký tự — vẫn là cỡ emoji",
        l.label.chars().count()
    );
}
