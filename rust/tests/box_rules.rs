//! Khung của TUI không phải nội dung — gột nó trước khi chữ đi ra Telegram.
//!
//! 🔴 Hà 2026-08-23, ảnh một tin gần như toàn gạch ngang: *"sao nội dung tin
//! không cắt bỏ các ký tự thừa thãi này đi, để làm gì?"*.

use huba::pipeline::strip_box_rules;

/// Đo trên BẢN CHỤP MÀN THẬT đang nằm trong kho, không trên chuỗi tự bịa.
#[test]
fn the_real_screen_capture_loses_its_rules_and_keeps_its_words() {
    let raw = include_str!("fixtures/shot-screen-2026-08-18.txt");
    let sach = strip_box_rules(raw);

    // Hai vạch dài 97 ký tự của bản chụp ấy phải biến mất hẳn.
    let vach = |t: &str| {
        t.lines()
            .filter(|l| {
                let c = l.trim();
                !c.is_empty() && c.chars().all(|ch| matches!(ch as u32, 0x2500..=0x259F))
            })
            .count()
    };
    assert_eq!(
        vach(raw),
        2,
        "bản chụp gốc phải có đúng 2 vạch: {}",
        vach(raw)
    );
    assert_eq!(vach(&sach), 0, "còn vạch sau khi gột:\n{sach}");

    // …và KHÔNG mất chữ nào. Đây mới là vế đắt: một bộ gột hăng quá thì tin
    // ngắn lại thật, nhưng ngắn vì mất nội dung.
    for dong in raw.lines() {
        let chu: String = dong
            .chars()
            .filter(|c| !c.is_whitespace() && !matches!(*c as u32, 0x2500..=0x259F))
            .collect();
        if chu.chars().count() < 4 {
            continue;
        }
        assert!(
            sach.contains(chu.trim()) || sach.lines().any(|l| l.contains(dong.trim())),
            "gột mất chữ: {dong:?}"
        );
    }
    println!(
        "bản chụp thật: {} dòng → {} dòng · {} ký tự → {}",
        raw.lines().count(),
        sach.lines().count(),
        raw.chars().count(),
        sach.chars().count()
    );
}

#[test]
fn a_frame_around_words_goes_but_the_words_stay() {
    // Viền dọc hai bên.
    assert_eq!(strip_box_rules("│ Hợp đồng đã ký │"), "Hợp đồng đã ký");
    // Gạch trang trí ôm lấy một tiêu đề.
    assert_eq!(strip_box_rules("──── Kết quả ────"), "Kết quả");
    // Hộp rỗng: đi cả.
    assert_eq!(strip_box_rules("╭─────╮\n╰─────╯"), "");
    // Nhiều vạch liền nhau chỉ để lại MỘT dòng trống, không dính hai đoạn văn.
    assert_eq!(strip_box_rules("a\n───\n───\n───\nb"), "a\n\nb");
    // Chữ thường không bị đụng tới.
    assert_eq!(
        strip_box_rules("git push origin main\nTo github.com:dipgle/tfl5.git"),
        "git push origin main\nTo github.com:dipgle/tfl5.git"
    );
}

/// Dấu gạch ngang THƯỜNG (`-`, `—`) không phải khung — gột nhầm là ăn vào câu.
#[test]
fn an_ordinary_dash_is_not_a_frame() {
    assert_eq!(
        strip_box_rules("--- không phải khung ---"),
        "--- không phải khung ---"
    );
    assert_eq!(strip_box_rules("— gạch em —"), "— gạch em —");
}
