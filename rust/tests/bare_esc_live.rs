//! Phím Esc RỜI có đóng được màn `/usage` không — chạy trên cửa sổ THẬT.
//!
//! 🔴 Hà 2026-09-01, ảnh buồng chat: gõ `/key esc` vào phiên `projects-f8`, huba
//! đáp *"đã bấm 'esc' nhưng màn KHÔNG đổi"*. Phím ĐÃ rời khỏi huba, mà màn đứng
//! nguyên.
//!
//! Giả thuyết: `/key` đi qua `keys::press` → `do_script`, mà lượt ghi ấy **kèm
//! một CR không tắt được**. Một Escape đứng MỘT MÌNH là phím Escape; `ESC` rồi
//! tới ngay một byte khác thì terminal đọc thành một CHUỖI THOÁT (đúng cách
//! `ESC[A` mang nghĩa mũi tên lên), nên `ESC`+`CR` bị nuốt như một chuỗi lạ.
//!
//! Repo đã trả giá đúng bài học này ngày 2026-08-30 ở đường `/close` (Hà: *"Nó
//! có phải là bấm phím thực sự như tôi ngồi máy bấm ở bàn phím không"*) và
//! chuyển chỗ ấy sang `send_bare` → `cgkeys::post` → `CGEventPostToPid`. Đường
//! `/key` thì còn nằm lại.
//!
//! Bài này ĐO giả thuyết ấy trên cửa sổ thật, thay vì sửa mã rồi hy vọng:
//! đọc màn trước, bắn Esc RỜI, đọc màn sau, và đòi màn phải ĐỔI.
//!
//! ```text
//! HUB_ESC_WINDOW=1131 cargo test --offline --test bare_esc_live -- --ignored --nocapture
//! ```
//!
//! `#[ignore]` vì nó gõ vào cửa sổ thật của chủ máy — không được chạy trong lượt
//! `cargo test` thường.

/// Dấu chỉ màn `/usage` đang mở. Neo vào chữ CLI in ra, không vào một con số.
const DAU_USAGE: &str = "Esc to cancel";

#[test]
#[ignore = "gõ vào cửa sổ THẬT — chạy tay bằng --ignored"]
fn esc_roi_dong_duoc_man_usage() {
    let w: i64 = std::env::var("HUB_ESC_WINDOW")
        .expect("cần HUB_ESC_WINDOW=<id cửa sổ>")
        .parse()
        .expect("HUB_ESC_WINDOW phải là một số");

    let truoc = huba::keys::screen_text(w).expect("không đọc được màn TRƯỚC");
    println!(
        "TRƯỚC: {} ký tự, có dấu usage = {}",
        truoc.chars().count(),
        truoc.contains(DAU_USAGE)
    );
    assert!(
        truoc.contains(DAU_USAGE),
        "cửa sổ {w} không đang mở màn /usage — bài này không đo được gì:\n{}",
        &truoc[truoc.len().saturating_sub(300)..]
    );

    // Quyền Trợ năng là điều kiện CẦN của phím rời, và thiếu nó thì sự kiện rơi
    // vào hư không TRONG IM LẶNG. Hỏi trước, để một lượt đo hỏng không bị đọc
    // thành "phím rời cũng không đóng được".
    assert!(
        huba::cgkeys::trusted(),
        "hubad/tiến trình này chưa có quyền Trợ năng — KHÔNG đo được, đừng kết luận"
    );

    huba::keys::send_bare(w, &["esc".to_string()]).expect("gửi phím rời hỏng");
    std::thread::sleep(std::time::Duration::from_millis(1200));

    let sau = huba::keys::screen_text(w).expect("không đọc được màn SAU");
    println!(
        "SAU:   {} ký tự, có dấu usage = {}",
        sau.chars().count(),
        sau.contains(DAU_USAGE)
    );
    assert!(
        !sau.contains(DAU_USAGE),
        "Esc RỜI cũng KHÔNG đóng được màn /usage ⟹ giả thuyết SAI, đừng sửa `/key` theo hướng này:\n{}",
        &sau[sau.len().saturating_sub(400)..]
    );
}
