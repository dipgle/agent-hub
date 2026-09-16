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

    let mut co_gio_reset = 0usize;
    for q in &doc {
        let dong = q.say(now);
        println!("{:<6} {dong}", q.account);
        if dong.contains("reset ") || dong.contains("mở lại ") {
            co_gio_reset += 1;
        }
    }
    println!(
        "\nMẪU SỐ: {}/{} hàng mang được giờ mở lại / giờ reset",
        co_gio_reset,
        doc.len()
    );
    // Không đòi 100%: một tài khoản chưa đăng nhập thì sổ của nó KHÔNG có cửa
    // sổ nào để mà nói giờ, và ép nó phải có là ép huba bịa. Đòi đúng cái đã
    // hỏng: trước bản vá, MỌI hàng chưa kịch trần đều trắng giờ.
    assert!(
        co_gio_reset > 0,
        "không hàng nào nói được giờ — đúng cái Hà đã chụp màn 16/09"
    );
}
