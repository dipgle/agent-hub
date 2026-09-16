//! Dòng `/accounts` dựng trên SỔ THẬT của máy này — in ra để đọc bằng mắt.
//!
//! 🔴 Vì sao phải có bài DÒ riêng, không dừng ở bài kiểm thuần. Luật của kho
//! này (`CLAUDE.md`, mục *"When you change something"*): *"Do not write
//! 'verified' off a green `cargo test`"* — 922 bài Rust đã từng xanh trọn trong
//! khi màn hình sai, hai lần trong một ngày. Bài kiểm thuần ở
//! `gio_sap_reset_tren_moi_hang_acc.rs` dựng `Quota` bằng tay, nên nó chứng
//! minh được CÔNG THỨC mà không chứng minh được rằng sổ thật có đủ trường để
//! công thức ấy chạy.
//!
//! Nó đọc, không ghi, không tốn hạn mức: `quota::read_all` chỉ mở
//! `<config_dir>/.claude.json` của từng tài khoản.
//!
//! ```
//! cd ~/projects/huba/rust
//! HUB_CONFIG=../huba.config.json \
//!   cargo test --offline --test gio_reset_tren_so_that_live -- --ignored --nocapture
//! ```

#[test]
#[ignore = "đọc sổ THẬT của máy này"]
fn moi_tai_khoan_that_deu_ra_duoc_mot_dong() {
    let cfg = huba::config::load(None).expect("HUB_CONFIG trỏ vào huba.config.json thật");
    let now = huba::quota::now_ms();
    let doc = huba::quota::read_all(&cfg);
    assert!(!doc.is_empty(), "không tài khoản nào trong cấu hình");

    // Đếm RIÊNG từng cửa sổ, không gộp: lượt vá đầu 16/09 đạt "hàng nào cũng
    // có giờ" mà vẫn thiếu đúng cái Hà hỏi lần thứ hai — giờ của cửa sổ PHIÊN.
    // Một phép đếm gộp hai cửa sổ sẽ xanh trong cả hai ca, tức nó không phân
    // biệt được bản vá đủ với bản vá thiếu.
    let mut co_tuan = 0usize;
    let mut co_phien = 0usize;
    for q in &doc {
        let dong = q.say(now);
        println!("{:<6} {dong}", q.account);
        for o in dong.split(" · ") {
            if o.starts_with("tuần ") && o.contains(" ↻ ") {
                co_tuan += 1;
            }
            if o.starts_with("5 tiếng ") && o.contains(" ↻ ") {
                co_phien += 1;
            }
        }
    }
    println!(
        "\nMẪU SỐ: {}/{} hàng có đồng hồ cửa sổ TUẦN · {}/{} hàng có đồng hồ cửa sổ PHIÊN",
        co_tuan,
        doc.len(),
        co_phien,
        doc.len()
    );
    // Không đòi 100%: một tài khoản chưa đăng nhập thì sổ của nó KHÔNG có cửa
    // sổ nào để mà nói giờ, và ép nó phải có là ép huba bịa. Đòi đúng hai thứ
    // đã hỏng, mỗi thứ một lần: trước lượt vá đầu MỌI hàng chưa kịch trần đều
    // trắng giờ; sau lượt ấy, cửa sổ phiên vẫn trắng ở mọi hàng mà tuần chật hơn.
    assert!(
        co_tuan > 0,
        "không hàng nào nói được giờ cửa sổ tuần — đúng cái Hà chụp màn lúc 07:46"
    );
    assert!(
        co_phien > 0,
        "không hàng nào nói được giờ cửa sổ PHIÊN — đúng cái Hà bắt lần thứ hai"
    );
}
