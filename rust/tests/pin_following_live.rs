//! Gim THẬT một tin lên đỉnh buồng chat, rồi hỏi CHÍNH Telegram xem nó có lên
//! không — và dọn sạch sau khi đo.
//!
//! 🔴 Hà 2026-08-25: *"bật gim tin nhắn thông tin phiên đang đứng trước đi"*.
//!
//! Vì sao phải là bài kiểm SỐNG: `pinChatMessage` là một lời gọi mạng, và thứ
//! quyết định nó chạy hay không nằm ngoài mã này — quyền gim của bot trong buồng
//! chat. Một bài kiểm thuần chỉ chứng minh được huba GỌI đúng hàm, không chứng
//! minh được cái gim LÊN. Đó đúng là khoảng cách mà CLAUDE.md của huba nói:
//! *"đừng viết 'verified' dựa trên một lượt `cargo test` xanh"*.
//!
//! Và nó đọc lại từ ĐÚNG NGUỒN Hà nhìn (`getChat().pinned_message`), không đọc
//! lại thứ mình vừa gửi đi — DoD 4.
//!
//! Gắn `#[ignore]` vì nó gửi + gim + xoá thật. Chạy tay:
//!
//! ```
//! cd ~/projects/huba/rust
//! HUB_CONFIG=$HOME/projects/huba/huba.config.json \
//!   cargo test --offline --test pin_following_live -- --ignored --nocapture
//! ```

/// Gửi → gim → hỏi Telegram → gỡ gim → xoá. Trả buồng chat về đúng như trước.
#[test]
#[ignore = "gửi + gim + xoá một tin thật — chạy tay bằng --ignored"]
fn a_following_message_really_reaches_the_top_of_the_chat() {
    let cfg = huba::config::load(None).expect("HUB_CONFIG trỏ vào huba.config.json thật");
    let tg = huba::telegram::Inbox::start(&cfg, None).expect("có bot token + chat id");

    // Ghi lại tin đang gim TRƯỚC khi đo, để trả lại nguyên trạng ở cuối.
    let truoc = tg.pinned_message_id().expect("hỏi được getChat");
    println!("gim trước khi đo: {truoc:?}");

    let tin = "👁 (tin kiểm tra, sẽ tự xoá) Đang theo phiên projects-kiemtra (acc1)";
    let nut = vec![("📷 Xem màn".to_string(), "shot:kiemtra".to_string())];
    let mid = tg
        .send_buttons_id(tin, &nut)
        .expect("Telegram nhận tin")
        .expect("Telegram trả về message_id");
    println!("đã gửi, message_id = {mid}");

    tg.pin(mid).expect("Telegram nhận pinChatMessage");

    // 🔴 Đây là phép đo thật: hỏi lại buồng chat, không tin lời `pin()`.
    let dang_gim = tg.pinned_message_id().expect("hỏi được getChat lần hai");
    println!("gim sau khi đo: {dang_gim:?}");

    // Dọn TRƯỚC khi phán, để một lượt đỏ không bỏ lại rác trong buồng chat của
    // chủ máy — bài học 2026-08-25 (assert trước bước dọn ⟹ ba cửa sổ nằm lại).
    let _ = tg.unpin(mid);
    if let Some(cu) = truoc {
        let _ = tg.pin(cu);
    }
    let _ = tg.delete_message(mid);

    assert_eq!(
        dang_gim,
        Some(mid),
        "Telegram nói tin đang gim KHÔNG phải tin vừa gim — cái gim không lên \
         đỉnh buồng chat, và chủ máy sẽ không thấy phiên mình đang theo"
    );
}
