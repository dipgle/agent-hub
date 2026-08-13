//! Phép DÒ chạy thật cho bảng hỏi nhiều câu (`AskUserQuestion`).
//!
//! Gắn `#[ignore]` vì nó đọc màn của một cửa sổ Terminal THẬT đang mở. Gọi tay:
//!
//! ```text
//! HUB_LIVE_TTY=ttys007 cargo test --offline --test ask_table_live -- --ignored --nocapture
//! ```
//!
//! Vì sao phải dò thay vì tra tài liệu: thanh tab của bảng hỏi vẽ trạng thái
//! từng câu bằng một ký tự ô (`☐` chưa trả lời / `⊠` đã trả lời trong ảnh Hà
//! gửi 2026-08-13), mà ảnh chụp điện thoại KHÔNG phân biệt được `⊠` với `☒`
//! hay `☑`. Đếm nhầm họ ký tự là dựng một phép đo luôn trả 0 — đúng cái bẫy
//! `OPERATING-CHARTER.md` §2d gọi là phép đo mù. Nên hàm đếm phải viết theo ký
//! tự ĐO ĐƯỢC trên máy này, và đây là chỗ đo.
//!
//! Nó KHÔNG bấm gì cả: chỉ đọc.

#[test]
#[ignore = "đọc màn một cửa sổ Terminal thật — chạy tay bằng --ignored"]
fn what_characters_does_the_question_table_actually_draw() {
    let tty = std::env::var("HUB_LIVE_TTY").unwrap_or_else(|_| "ttys007".to_string());
    let look = hub::keys::look(&tty, 40);
    let body = match look {
        hub::keys::Look::Saw { body, choices } => {
            println!("hộp chọn đọc được: {} mục", choices.len());
            for (n, label) in &choices {
                println!("  {n}. {label}");
            }
            body
        }
        hub::keys::Look::Withheld { choices, risk } => {
            panic!("màn bị giữ lại ({choices} mục, dấu hiệu {risk:?}) — dò trên màn khác");
        }
        hub::keys::Look::Blind { why } => panic!("không đọc được màn: {why}"),
    };
    // Phép đo phải trỏ đúng chỗ: hàm đọc bảng chạy trên MÀN THẬT, không chỉ
    // trên hằng số chép tay trong test đơn vị.
    match hub::keys::ask_table(&body) {
        Some(t) => {
            println!(
                "bảng đọc từ màn thật: {} câu · còn trống {} · nhãn {:?}",
                t.answered.len(),
                t.left(),
                t.headers
            );
            assert!(!t.answered.is_empty(), "thanh tab có ô thì phải đếm ra ô");
        }
        None => println!("(màn này không có thanh tab — phiên đang hỏi bảng MỘT câu)"),
    }
    // Chỉ in NHỮNG DÒNG mang ký tự lạ, và cắt ngắn: đây là màn làm việc thật
    // của chủ máy, không phải bãi đổ nhật ký (điều 5).
    println!("--- các dòng có ký tự ngoài ASCII ---");
    for line in body.lines() {
        if !line.is_ascii() {
            let shown: String = line.chars().take(90).collect();
            println!("{shown}");
            let marks: Vec<String> = line
                .chars()
                .filter(|c| !c.is_ascii())
                .map(|c| format!("{c} U+{:04X}", c as u32))
                .collect();
            println!("      ↳ {}", marks.join(" · "));
        }
    }
}
