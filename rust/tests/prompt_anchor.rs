//! Hai NỬA của đường "chữ trong ô nhập → nút gửi nhanh", đo riêng từng nửa.
//!
//! 🔴 Vì sao tách đôi. Ngày 2026-08-16 Hà báo *"Lại mất nút gửi nhanh gợi ý mờ
//! rồi"*, và câu hỏi đầu tiên là hỏng ở đâu: đọc không ra chữ trong ô, hay đọc
//! ra rồi mà neo không bám được vào dòng? Hai nửa ấy hỏng vì hai lý do khác hẳn
//! nhau, nên gộp làm một phép đo là tự bịt mắt mình một nửa. (Lần ấy cả hai đều
//! đúng — thủ phạm là chỗ thứ ba: `/shot` chỉ đi qua bộ định dạng KHI có nút.)
//!
//! Bản chụp dưới đây lấy nguyên văn từ `hubd.err` lượt 14:34:40Z. Dấu cách sau
//! `❯` là **U+00A0**, không phải dấu cách thường — chính chi tiết ấy là thứ
//! khiến tôi nghi oan cho `prompt_line_text`.

use huba::pipeline::{html_with_links, prompt_line_text};

const SCREEN: &str = "📷 Màn của 🟪 [huba]:\n\n\
    ───────────────────────\n\
    \u{276f}\u{a0}Bỏ hẳn trần cắt lệnh đi\n\
    ───────────────────────\n\
    \u{23f5}\u{23f5} auto mode on (shift+tab to cycle) · ← 1 agent";

/// Nửa thứ nhất: đọc ra được chữ đang nằm trong ô nhập.
#[test]
fn the_prompt_line_is_read_even_with_a_non_breaking_space() {
    assert_eq!(
        prompt_line_text(SCREEN).as_deref(),
        Some("Bỏ hẳn trần cắt lệnh đi")
    );
}

/// Nửa thứ hai: neo ấy bám được vào đúng dòng của nó trong chữ Telegram.
#[test]
fn the_anchor_binds_to_the_line_it_came_from() {
    let anchors = vec![(
        "Bỏ hẳn trần cắt lệnh đi".to_string(),
        vec![(
            "https://t.me/bot?start=send_abc".to_string(),
            "⏎".to_string(),
        )],
    )];
    let (html, linked, unlinked) = html_with_links(SCREEN, &anchors);
    assert_eq!(linked, 1, "neo phải bám được vào dòng ô nhập:\n{html}");
    assert!(unlinked.is_empty(), "không neo nào được rơi xuống đáy tin");
}
