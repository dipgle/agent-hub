//! Đích chạm của một lựa chọn phải to bằng **cả dòng**, không bằng một ký tự.
//!
//! 🔴 Hà 2026-08-27: *"Có option nhưng ko chọn được"* → *"Làm luôn cái ☑ hiện
//! ngay tại dòng lựa chọn đi"*.
//!
//! Câu thứ hai nghe như đòi một tính năng chưa có. Nó đã có từ 17/08, và đo
//! được trên chính tin Hà chụp (`telegram_html_sent` `10:23:45.896Z`):
//! **`text_links=7`** = 5 lựa chọn `☑` + 2 tab `↪`. Nút CÓ, và Hà vẫn không bấm
//! được — sổ ghi **0 cú chạm `pick_574e5be2_*`**, trong khi `k_93479f95_1` (hộp
//! một câu) cùng ngày thì chạm được.
//!
//! ⚠ Và tôi đã kết luận sai một lượt trước khi tới đây: đếm `/pick_` trong
//! `ack` rồi báo *"Câu 1 có 0 đích chạm"*. `ack` là chữ TRƯỚC khi định dạng, nên
//! phép đếm ấy **không nhìn thấy được** liên kết chèn trong chữ. Cùng họ với
//! phép đo tự khớp chính nó mà charter §2d đã ghi.
//!
//! Cái sai thật là **CỠ** của đích chạm. Dòng LỆNH đã được bọc cả dòng từ 25/08
//! (đo trên Telegram thật: `text_link` len 2 → 41). Dòng LỰA CHỌN thì không, vì
//! nhãn nằm sau `1. ` nên không bao giờ bằng ĐÚNG cả dòng — đúng ca mà lượt ấy
//! **cố ý để lại**: *"nới ra là đổi hình dạng của những chỗ chưa ai hỏi"*. Nay
//! đã có người hỏi.

use huba::pipeline::html_with_links;

/// Dòng lựa chọn thật, lấy nguyên văn từ màn `[dwork] 574e5be2`.
const DONG: &str = "❯ 1. Tách ship trước, gấp (Recommended)";
const NHAN: &str = "Tách ship trước, gấp (Recommended)";

fn neo() -> Vec<(String, Vec<(String, String)>)> {
    vec![(
        NHAN.to_string(),
        vec![(
            "https://t.me/ai_agents_bot?start=pick_574e5be2_1_1".to_string(),
            "\t☑".to_string(),
        )],
    )]
}

/// Đo CỠ, không đo sự tồn tại. "Có một liên kết" là điều đúng cả trước lẫn sau
/// bản vá, nên một bài kiểm hỏi câu ấy sẽ xanh ở cả hai bên và chẳng chứng minh gì.
#[test]
fn the_link_covers_the_label_not_just_the_tick() {
    let (html, _, _) = html_with_links(DONG, &neo());
    let trong_the: String = html
        .split_once("<a href=")
        .and_then(|(_, r)| r.split_once('>'))
        .and_then(|(_, r)| r.split_once("</a>"))
        .map(|(t, _)| t.to_string())
        .unwrap_or_default();
    assert!(
        trong_the.contains("Tách ship"),
        "thẻ <a> chỉ bọc {trong_the:?} — tức ngón tay phải trúng đúng một ký tự ☑. \
         Cả nhãn phải nằm TRONG thẻ, cùng khuôn với dòng lệnh từ 25/08.\n{html}"
    );
    assert!(
        trong_the.chars().count() > 10,
        "đích chạm dài {} ký tự — vẫn là một mục tiêu tí hon trên màn điện thoại",
        trong_the.chars().count()
    );
}

/// ĐỐI CHỨNG NGƯỢC: bọc cả dòng KHÔNG được nuốt những chỗ vốn không phải lựa
/// chọn. Một câu văn có đánh số là thứ luật "liền dòng" của `parse_choices` sinh
/// ra để chặn; nếu bản vá này biến nó thành nút thì tôi vừa mở lại cái bug ấy ở
/// một tầng khác.
#[test]
fn a_numbered_sentence_without_an_anchor_gets_no_link() {
    let van_xuoi = "  3. Đây chỉ là một câu văn có đánh số, không ai khai nó là lựa chọn";
    let (html, n, _) = html_with_links(van_xuoi, &neo());
    assert_eq!(
        n, 0,
        "không có neo nào khớp thì phải KHÔNG có liên kết nào — nếu không, mọi \
         dòng đánh số trên màn sẽ thành nút.\n{html}"
    );
}

/// Bóc số thứ tự không được tạo ra neo RỖNG: một dòng chỉ có mỗi "3." thì sau
/// khi bóc chẳng còn gì, và một neo rỗng khớp với mọi dòng.
#[test]
fn a_bare_number_line_does_not_become_an_empty_anchor() {
    let (_, n, _) = html_with_links("  3.", &neo());
    assert_eq!(
        n, 0,
        "dòng trống sau số thứ tự không được sinh liên kết nào"
    );
}
