//! Đích chạm của tab nằm NGAY TẠI nhãn, không phải một khối nút ở đáy tin.
//!
//! 🔴 Hà 2026-08-19: *"Sao không chèn nút trực tiếp ở phần nội dung lại đi chèn
//! thêm nút ở cuối, bấm cũng chưa nhận"*.
//!
//! Bản đầu ném ba cái nút xuống đáy, và lý do thuần là lý do của MÃ:
//! `html_with_links` gắn **mỗi dòng một neo**, mà thanh tab là một dòng mang ba
//! nhãn. Cách đúng không phải sửa bộ gắn neo cho gắn được nhiều neo một dòng,
//! mà bẻ thanh tab thành mỗi tab một dòng — chữ vẫn là chữ TUI vẽ, chỉ đổi chỗ
//! xuống dòng.
//!
//! Thanh tab dưới đây chép NGUYÊN VĂN từ màn phiên `[AI/tcc/amm]`
//! (`tests/fixtures/*`, và cùng chuỗi ấy có trong `keys.rs::REAL_TAB_BAR`).

const BAR: &str = "←  ☒ RPC pool  ☐ NativeAssets v3  ☐ Việc tiếp  ✔ Submit  →";

/// Tên bot phải khai trước, không thì `deep_link` im lặng trả `None` và cả bài
/// kiểm đỏ vì một lý do MÔI TRƯỜNG chứ không vì sản phẩm — xem
/// `telegram::set_bot_username`.
fn arrange() {
    huba::telegram::set_bot_username("hub_test_bot");
}

fn data(sid: &str) -> huba::pipeline::SessionData {
    huba::pipeline::SessionData {
        sid: sid.to_string(),
        tabs: vec![
            (1, "RPC pool".to_string(), true),
            (2, "NativeAssets v3".to_string(), false),
            (3, "Việc tiếp".to_string(), false),
        ],
        ..Default::default()
    }
}

/// Mỗi tab một DÒNG, và mỗi dòng mang đúng đích chạm của nó.
#[test]
fn every_tab_gets_its_own_line_and_its_own_link() {
    arrange();
    let text = format!("Câu hỏi đây:\n{BAR}\nMặt ĐỌC của native pool…\n");
    let out = huba::pipeline::render_session_data(&text, &data("da29807e"));

    for label in ["RPC pool", "NativeAssets v3", "Việc tiếp"] {
        let line = out
            .lines()
            .find(|l| l.contains(label))
            .unwrap_or_else(|| panic!("mất nhãn {label} trong:\n{out}"));
        assert!(
            line.contains("<a href="),
            "nhãn {label} không có đích chạm ngay tại dòng của nó: {line}"
        );
        assert!(line.contains("↪"), "thiếu icon đi-tới ở {label}: {line}");
    }
    // Ba nhãn phải nằm trên BA dòng khác nhau — nếu còn chung một dòng thì chỉ
    // một cái neo bám được, đúng con bug đang vá.
    let lines_with_labels = out
        .lines()
        .filter(|l| {
            ["RPC pool", "NativeAssets v3", "Việc tiếp"]
                .iter()
                .any(|s| l.contains(s))
        })
        .count();
    assert_eq!(lines_with_labels, 3, "thanh tab chưa được bẻ dòng:\n{out}");
}

/// Trạng thái từng tab đi theo nhãn — `☒` đã trả lời, `☐` còn trống. Đó là con
/// số quyết định bảng có gửi đi được hay chưa, nên nó không được rụng khi bẻ dòng.
#[test]
fn the_marks_survive_the_split() {
    arrange();
    let out = huba::pipeline::render_session_data(&format!("{BAR}\n"), &data("da29807e"));
    let line_of = |s: &str| {
        out.lines()
            .find(|l| l.contains(s))
            .unwrap_or_default()
            .to_string()
    };
    assert!(line_of("RPC pool").contains('☒'), "{out}");
    assert!(line_of("NativeAssets v3").contains('☐'), "{out}");
    // …và nút gửi của bảng vẫn còn chữ `Submit` để cái neo ✅ có chỗ bám.
    assert!(out.contains("Submit"), "{out}");
}

/// Không có bảng nào thì KHÔNG được đụng vào chữ — một hàm định dạng chỉ được
/// phép im lặng khi không có việc của nó.
#[test]
fn a_screen_without_a_table_is_left_alone() {
    arrange();
    let plain = "chỉ là một dòng chữ ← → thường\n";
    let out = huba::pipeline::render_session_data(
        plain,
        &huba::pipeline::SessionData {
            sid: "da29807e".into(),
            ..Default::default()
        },
    );
    assert!(out.contains("chỉ là một dòng chữ"), "{out}");
    assert!(
        !out.contains("<a href="),
        "mọc đích chạm từ hư không: {out}"
    );
}
