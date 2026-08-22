//! In ra ĐÚNG cái danh sách phiên mà Telegram sẽ nhận, dựng từ máy thật.
//!
//! `#[ignore]` vì nó chụp phiên thật (`sessions::snapshot` gọi `claude agents`
//! rồi dò Terminal, mất vài giây). Nó CHỈ ĐỌC — không gõ phím, không đổi kích
//! thước cửa sổ, không gửi gì lên Telegram.
//!
//! ```text
//! cargo test --offline --test session_list_live -- --ignored --nocapture
//! ```
//!
//! Vì sao phải có: `session_list_text` là một mặt GIAO DIỆN, và bài kiểm đơn vị
//! chỉ chấm được *"chuỗi này có chứa chữ kia không"* — nó không nói được danh
//! sách ấy dài mấy dòng trên màn 390px, thứ duy nhất Hà thật sự nhìn. Cùng bài
//! học `CLAUDE.md` rút ra ngày 10/08: *"An assertion tests what you thought to
//! check; a picture shows what you didn't"*.

#[test]
#[ignore = "chụp phiên thật rồi in danh sách — chạy tay bằng --ignored"]
fn what_the_phone_actually_receives() {
    let cfg = huba::config::load(None).expect("đọc được cấu hình");
    let snap = huba::sessions::snapshot(&cfg);
    let now = chrono::Utc::now().timestamp_millis();
    let focus = snap
        .sessions
        .first()
        .map(|s| s.session_id.clone())
        .unwrap_or_default();
    let text = huba::pipeline::session_list_text(&snap.sessions, &focus, now);

    println!("---------------- 8< ---------------- (đây là chữ Telegram nhận)");
    println!("{text}");
    println!("---------------- >8 ----------------");
    let lines = text.lines().count();
    let per = if snap.sessions.is_empty() {
        0.0
    } else {
        lines as f64 / snap.sessions.len() as f64
    };
    println!(
        "{} phiên · {} dòng · {:.1} dòng/phiên · {} ký tự",
        snap.sessions.len(),
        lines,
        per,
        text.chars().count()
    );
    // Dòng dài nhất: nó quyết định danh sách có bị bẻ dòng trên điện thoại
    // không. Đo cả hai, vì một hàng 90 ký tự bẻ làm hai thì "một dòng mỗi
    // phiên" trên giấy vẫn là hai dòng trên màn.
    if let Some(longest) = text.lines().max_by_key(|l| l.chars().count()) {
        println!(
            "dòng dài nhất: {} ký tự — {}",
            longest.chars().count(),
            longest
        );
    }
    // Không assert con số nào: máy này có mấy phiên là chuyện của lúc chạy, và
    // một bài kiểm đỏ vì hôm nay mở nhiều phiên là bài kiểm kêu oan. Trần dòng
    // đã có `MAX_SESSION_BUTTONS` gác, và có bài kiểm riêng cho nó.
    assert!(
        !text.is_empty(),
        "danh sách rỗng — kể cả 0 phiên cũng phải là một câu"
    );
}
