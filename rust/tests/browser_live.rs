//! Chrome THẬT trên máy — chạy thật, qua Apple Events.
//!
//! `#[ignore]` vì nó nói chuyện với trình duyệt đang mở của chủ máy, và vì lượt
//! ĐẦU TIÊN sẽ làm macOS bật hộp xin quyền Tự động hoá. Chạy tay:
//!
//! ```text
//! cargo test --offline --test browser_live -- --ignored --nocapture
//! ```
//!
//! 🔴 Vì sao phải có: `tests/browser.rs` kiểm được cái cổng địa chỉ, bảng đọc
//! về, và HÌNH DẠNG của bốn đoạn script — nhưng không đoạn nào trong số đó đã
//! từng được **AppleScript biên dịch**. Một dấu nháy lạc hay một từ khoá sai
//! chỉ lộ ở đây. Luật của repo: chưa chạy trên env thật thì chưa tính.
//!
//! Bài kiểm này KHÔNG đòi Chrome phải mở sẵn, và không tự mở nó: cả ba kết cục
//! (đọc được · Chrome tắt · chưa cấp quyền) đều là câu trả lời hợp lệ, và mỗi
//! câu in ra một dòng khác nhau — đúng lý do `browser::Loi` có bốn nhánh.

#[test]
#[ignore = "nói chuyện với Chrome thật, và lượt đầu bật hộp xin quyền — chạy tay bằng --ignored"]
fn the_real_chrome_answers_or_says_exactly_why_not() {
    match huba::browser::tabs() {
        Ok(tabs) => {
            println!("đọc được {} tab", tabs.len());
            for t in tabs.iter().take(8) {
                println!(
                    "  {}.{}{} {} · {}",
                    t.win,
                    t.idx,
                    if t.active { " ●" } else { "  " },
                    huba::pipeline::web_host(&t.url),
                    t.title.chars().take(60).collect::<String>()
                );
            }
            // ⚠ KHÔNG đòi danh sách phải khác rỗng. Bản đầu của bài kiểm này
            // đòi thế và ĐỎ trên một máy hoàn toàn bình thường: Chrome của chủ
            // máy chạy với `--no-startup-window` (đo 23/08, pid 94894), tức
            // tiến trình sống mà không cửa sổ nào — một trạng thái thật, không
            // phải một lỗi. Bài kiểm khẳng định một điều về THẾ GIỚI mà thế
            // giới không hứa thì nó đo chính giả định của người viết.
            // Và mỗi hàng phải đủ hai khoá tra cứu, không thì `/web 1.2` trỏ
            // vào hư không.
            for t in &tabs {
                assert!(
                    t.win >= 1 && t.idx >= 1,
                    "chỉ số 0 thì không bấm được: {t:?}"
                );
            }
            match huba::browser::front() {
                Ok(f) => println!("trước mặt: {} · {}", f.title, f.url),
                Err(e) => println!("(không lấy được tab trước mặt: {e})"),
            }
        }
        Err(e) => {
            // KHÔNG `panic!`: "chưa cấp quyền" và "Chrome đang tắt" là hai câu
            // trả lời THẬT về thế giới, không phải hai lỗi của mã. Bài kiểm này
            // đo *"script có chạy được không, và nếu không thì huba có nói đúng
            // lý do không"* — nên nó in ra rồi để người đọc quyết.
            println!("chưa đọc được, và lý do là:\n{e}");
            let s = e.to_string();
            assert!(
                !s.contains("syntax error") && !s.contains("Expected"),
                "AppleScript hỏng CÚ PHÁP — đây mới là lỗi của tôi:\n{s}"
            );
        }
    }
}

/// Và đường route thật — đúng chuỗi Telegram sẽ nhận cho `/web` trống.
///
/// Tách khỏi bài trên vì nó đo một tầng khác: `tabs()` trả dữ liệu, còn cái này
/// trả CHỮ, kèm cả cái chốt "trình duyệt ẩn đang chạy nên có thể trỏ nhầm".
#[test]
#[ignore = "đọc Chrome thật — chạy tay bằng --ignored"]
fn the_web_route_says_what_the_machine_browser_has() {
    let (chu, taps) = huba::pipeline::web_route("");
    println!("--- /web ---\n{chu}\n--- {} đích chạm ---", taps.len());
    assert!(!chu.trim().is_empty(), "route trả chuỗi rỗng");
    // Hàng nào cũng phải mang khoá tra cứu `<cửa sổ>.<tab>`, không thì cái đích
    // chạm dựng ra không trỏ vào đâu.
    for (neo, href) in &taps {
        assert!(neo.contains('.'), "neo không mang khoá tab: {neo:?}");
        assert!(href.contains("?start=wb_"), "{href}");
    }
}
