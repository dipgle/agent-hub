//! Phép đo chạy thật cho `/front`: gọi một cửa sổ Terminal ra trước mặt, rồi
//! hỏi lại chính Terminal xem nó có ra thật không.
//!
//! `#[ignore]` vì nó ĐỘNG VÀO MÀN HÌNH của chủ máy — đưa một cửa sổ ra trước và
//! `activate` Terminal. Nó KHÔNG gõ phím nào, không chụp ảnh, không đụng tới
//! nội dung phiên.
//!
//! ```text
//! HUB_LIVE_TTY=ttysNNN cargo test --offline --test front_window_live -- --ignored --nocapture
//! ```
//!
//! Vì sao phải đo thật: `osascript` trả 0 khi CÂU LỆNH chạy xong, không khi cửa
//! sổ đã ra trước — đúng điều 4 của charter (đừng đọc mã thoát của thứ chỉ khởi
//! chạy). Bài kiểm đơn vị chấm được bảng route; chỉ chỗ này chấm được cái việc.

#[test]
#[ignore = "đưa một cửa sổ Terminal thật ra trước mặt — chạy tay bằng --ignored"]
fn bringing_a_window_to_the_front_actually_moves_it() {
    let tty = std::env::var("HUB_LIVE_TTY").expect("cần HUB_LIVE_TTY");
    let w = huba::keys::window_of(&tty)
        .expect("hỏi được Terminal")
        .expect("tty phải gắn một cửa sổ");

    let before = huba::keys::front_window().expect("hỏi được cửa sổ đang đứng trước");
    println!("trước : cửa sổ đứng trước = {before:?} · cửa sổ của {tty} = {w}");

    huba::keys::bring_to_front(w).expect("gọi được ra trước");
    // WindowServer sắp xong sau một nhịp; đọc ngay là đọc thứ tự cũ rồi báo
    // hỏng cho một lượt ĐÚNG. Cùng con số route dùng.
    std::thread::sleep(std::time::Duration::from_millis(400));
    let after = huba::keys::front_window().expect("hỏi lại được");
    println!("sau   : cửa sổ đứng trước = {after:?}");

    // 🔴 TRẢ MÀN HÌNH VỀ NHƯ CŨ trước khi assert — assert có thể panic, và bỏ
    // cửa sổ của chủ máy nằm sai chỗ vì một bài kiểm là cái giá không đáng.
    // Cùng luật `keys::screen_text_tall` đang giữ với chiều cửa sổ.
    if let Some(b) = before {
        if Some(b) != after {
            let _ = huba::keys::bring_to_front(b);
        }
    }

    assert_eq!(
        after,
        Some(w),
        "gọi cửa sổ {w} ra trước mà Terminal vẫn nói {after:?} đang đứng trước"
    );
    println!("=> ĐÚNG: cửa sổ {w} đã ra trước, và màn hình đã trả về như cũ");
}

/// Phép đo phải BIẾT NÓI KHÔNG, nếu không nó chỉ là một dấu ✅ vô điều kiện.
///
/// Bài trên assert `after == Some(w)`. Nếu `front_window` trả bừa cửa sổ nào đó
/// thì assert ấy vẫn xanh ở một máy có đúng một cửa sổ. Bài này hỏi câu ngược
/// lại trên cùng phép đo: gọi một cửa sổ KHÁC ra trước thì con số đọc về phải
/// ĐỔI. Không đổi ⟹ phép đo mù, và bài trên không chứng minh gì cả.
///
/// Bỏ qua sạch (không đỏ) khi máy chỉ có một cửa sổ Terminal — lúc ấy không có
/// gì để so, và một bài kiểm đỏ vì thiếu điều kiện là một bài kiểm kêu oan.
#[test]
#[ignore = "đảo thứ tự hai cửa sổ Terminal thật — chạy tay bằng --ignored"]
fn the_probe_can_tell_two_windows_apart() {
    // Không thêm hàm liệt kê cửa sổ chỉ để phục vụ một bài kiểm — repo này đã
    // có `terminal_tabs()`, và `window_of` biến tty thành id cửa sổ. Dùng đúng
    // đường sản phẩm đang dùng.
    let mut ids: Vec<i64> = Vec::new();
    for t in huba::keys::terminal_tabs().expect("liệt kê được tab") {
        if let Ok(Some(w)) = huba::keys::window_of(&t.tty) {
            if !ids.contains(&w) {
                ids.push(w);
            }
        }
    }
    println!("cửa sổ Terminal đang mở: {ids:?}");
    if ids.len() < 2 {
        println!(
            "BỎ QUA — máy chỉ có {} cửa sổ, không có gì để so.",
            ids.len()
        );
        return;
    }
    let start = huba::keys::front_window().expect("hỏi được");
    let mut seen = Vec::new();
    for w in ids.iter().take(2) {
        huba::keys::bring_to_front(*w).expect("gọi được ra trước");
        std::thread::sleep(std::time::Duration::from_millis(400));
        let f = huba::keys::front_window().expect("hỏi lại được");
        println!("  gọi {w} ⟹ đọc về {f:?}");
        seen.push(f);
    }
    if let Some(s) = start {
        let _ = huba::keys::bring_to_front(s);
    }
    assert_ne!(
        seen[0], seen[1],
        "gọi hai cửa sổ KHÁC nhau mà phép đo trả cùng một số ⟹ nó không đo cái nó nói"
    );
}
