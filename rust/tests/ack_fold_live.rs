//! Câu xác nhận trơn lặp lại KHÔNG được đẻ thêm dòng — đo trên buồng chat THẬT.
//!
//! 🔴 Vì sao phải chạy thật: bài kiểm thuần (`tests/telegram.rs::the_same_
//! confirmation_twice_edits_the_first_one`) chỉ nói `fold_ack` chọn đúng đường.
//! Nó không nói `editMessageText` có ăn không, cũng không nói chữ Telegram
//! hiển thị sau khi sửa là chữ nào — mà đúng hai chuyện ấy mới là thứ Hà nhìn
//! thấy trên điện thoại. Cùng lý do `telegram_live.rs` tồn tại: `sendMessage`
//! và `editMessageText` đều TRẢ VỀ đối tượng Message, nên "tin ấy giờ ghi gì"
//! là con số đọc được từ máy này.
//!
//! Đo được cái nó chặn (nhật ký hubd 17/08): **73 dòng `✓ đã gửi · …`** cho 73
//! cú bấm phím đi qua một liên kết trong chữ, mỗi dòng kèm một
//! `telegram_reaction_failed` — vì tin sinh ra lệnh là tiếng vọng `/start`, mà
//! hub xoá ngay tiếng vọng ấy.
//!
//! Gắn `#[ignore]` vì nó gửi thật (và xoá thật). Chạy tay:
//!
//! ```
//! cd ~/projects/hub/rust
//! HUB_CONFIG=$HOME/projects/hub/hub.config.json \
//!   cargo test --offline --test ack_fold_live -- --ignored --nocapture
//! ```

/// Hai câu xác nhận GIỐNG HỆT ⟹ một tin duy nhất, đếm lên `×2`.
#[test]
#[ignore = "gửi một tin thật vào buồng chat rồi xoá — chạy tay bằng --ignored"]
fn the_second_identical_ack_edits_the_first_message() {
    let cfg = hub::config::load(None).expect("HUB_CONFIG trỏ vào hub.config.json thật");
    let tg = hub::telegram::Inbox::start(&cfg, None).expect("có bot token + chat id");

    // Câu thật của đường `/key`, chỉ thêm dấu hiệu "đây là tin kiểm tra".
    let ack = "✓ đã gửi · 🧪 [kiểm tra, tin này tự xoá]";

    tg.send_ack(ack).expect("gửi được câu xác nhận đầu");
    let first = tg.ack_live_now().expect("sổ phải nhớ tin vừa gửi");
    assert_eq!(first.times, 1, "lần đầu phải là 1");

    tg.send_ack(ack).expect("câu thứ hai phải đi lọt");
    let second = tg.ack_live_now().expect("sổ vẫn phải còn");

    // 🔴 Phép đo trỏ đúng chỗ: KHÔNG đẻ tin mới (cùng message_id), và chữ trên
    // tin ấy đúng là chữ có đuôi đếm. Chỉ so `times` là đo cái sổ của chính
    // mình — sổ đúng mà Telegram không sửa được thì màn hình vẫn nói `×1`.
    assert_eq!(
        second.message_id, first.message_id,
        "câu y hệt phải sửa tin cũ, không đẻ tin mới"
    );
    assert_eq!(second.times, 2);

    let shown = tg
        .edit_html(
            second.message_id,
            &hub::telegram::html_escape(&format!("{ack} ×2 (đọc lại)")),
            &[],
        )
        .expect("đọc lại được tin sau khi sửa");
    println!("--- Telegram hiển thị ---\n{}", shown.text);
    assert!(
        shown.text.contains("×2"),
        "tin trên Telegram phải mang đuôi đếm: {:?}",
        shown.text
    );

    let removed = tg.delete_message(second.message_id);

    // Một tin khác đi ra ⟹ sổ phải rỗng, nếu không thì lượt sau sửa một dòng đã
    // trôi lên giữa màn.
    tg.forget_ack_live();
    assert!(tg.ack_live_now().is_none());

    removed.expect("xoá được tin thử");
}

/// Câu KHÁC đi thì có dòng riêng — gộp nó là ghi đè thứ vừa nói.
#[test]
#[ignore = "gửi hai tin thật vào buồng chat rồi xoá — chạy tay bằng --ignored"]
fn a_different_ack_gets_its_own_message() {
    let cfg = hub::config::load(None).expect("HUB_CONFIG trỏ vào hub.config.json thật");
    let tg = hub::telegram::Inbox::start(&cfg, None).expect("có bot token + chat id");

    tg.send_ack("✓ đã gửi · 🧪 [kiểm tra A, tự xoá]")
        .expect("tin A");
    let a = tg.ack_live_now().expect("sổ nhớ A");
    tg.send_ack("✓ đã gửi · 🧪 [kiểm tra B, tự xoá]")
        .expect("tin B");
    let b = tg.ack_live_now().expect("sổ nhớ B");

    let cleanup_a = tg.delete_message(a.message_id);
    let cleanup_b = tg.delete_message(b.message_id);

    assert_ne!(a.message_id, b.message_id, "câu khác phải có dòng riêng");
    assert_eq!(b.times, 1, "dòng mới bắt đầu đếm lại từ 1");
    cleanup_a.expect("xoá được tin A");
    cleanup_b.expect("xoá được tin B");
}
