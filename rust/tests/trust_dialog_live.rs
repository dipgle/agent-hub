//! Phép thử CHẠY THẬT cho việc huba tự trả lời hộp tin-thư-mục.
//!
//! Gắn `#[ignore]` vì nó **mở một cửa sổ Terminal thật** trên máy này — không
//! phải thứ được chạy kèm `cargo test`. Gọi tay:
//!
//! ```text
//! HUB_LIVE_ACCOUNT=acc3 cargo test --offline --test trust_dialog_live -- --ignored --nocapture
//! ```
//!
//! Vì sao phải có nó, thay vì tin vào test thuần: cả `trust_dialog_choice` lẫn
//! `auto_handover_notice` đều xanh trong khi cửa sổ thật vẫn có thể đứng im —
//! phần rủi ro nằm ở chỗ nối (đọc màn qua `tty`, `window_of`, `press`), và chỗ
//! nối thì chỉ Terminal thật mới trả lời. Đây đúng là bài học đã trả giá
//! 2026-08-13: hộp thoại chặn phiên mới sống được **22 phút** mà mọi test đều
//! xanh, vì không test nào chạm tới một cửa sổ.
//!
//! Đề bài để RỖNG có chủ ý: cửa sổ mở ra rồi đứng ở dấu nhắc, không tiêu một
//! đồng hạn mức nào. Thứ đang đo là *phiên có chào đời được không*, không phải
//! nó làm được gì.

use std::path::Path;

#[test]
#[ignore = "mở một cửa sổ Terminal thật — chạy tay bằng --ignored"]
fn a_new_window_gets_past_the_trust_dialog_on_its_own() {
    let cfg = huba::config::load(Some(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../huba.config.json"
    ))))
    .expect("nạp huba.config.json");

    let account = std::env::var("HUB_LIVE_ACCOUNT").unwrap_or_else(|_| "acc3".to_string());
    let dir = cfg.workspace_root.clone();
    println!("mở phiên trống: account={account} dir={}", dir.display());

    let started =
        huba::sessions::start_background(&cfg, "live-probe", &dir, "", Some(&account), None)
            .expect("phiên mới phải chào đời — nếu hỏng ở đây, cửa sổ đang kẹt ở một hộp thoại");

    println!("id phiên mới = {}", started.session_id);
    assert!(
        !started.session_id.is_empty(),
        "không ghép được id ⟹ phiên chưa chào đời"
    );
    assert!(started.window, "phải là phiên có CỬA SỔ, không phải --bg");
    println!("✅ qua được hộp thoại. Nhớ đóng cửa sổ ấy sau khi xem.");
}
