//! Lựa chọn có MÔ TẢ dài phải bám được `☑` như lựa chọn một dòng.
//!
//! 🔴 Hà 2026-08-24, ảnh buồng `[social]`: *"Bắt kiểu gì mà option cái được cái
//! không"*. Bảng năm lựa chọn, chỉ **4 và 5** có `☑` — đúng hai cái nhãn NGẮN,
//! một dòng (`Type something.` · `Chat about this`). Ba cái đầu, mỗi cái một
//! tiêu đề kèm một đoạn mô tả, thì không.

use huba::pipeline::html_with_links;

/// Nguyên văn khối huba gửi, lấy từ ảnh.
const MSG: &str = "❓ 🟪 [social] dừng lại HỎI — cần bạn chọn:\n\
                   1. Seed qvt, gỡ qvt0484 (Recommended)\n\
                   2. Seed qvt, giữ cả qvt0484\n\
                   3. Chỉ gỡ qvt0484, chưa seed ai\n\
                   4. Type something.\n\
                   5. Chat about this\n";

fn anchors(labels: &[&str]) -> Vec<(String, Vec<(String, String)>)> {
    labels
        .iter()
        .enumerate()
        .map(|(i, l)| {
            (
                l.to_string(),
                vec![(format!("https://t.me/b?start=k_abcd1234_{}", i + 1), "\t☑".to_string())],
            )
        })
        .collect()
}

/// Nhãn = ĐÚNG dòng trên màn ⟹ cả năm phải bám. Đây là bản đối chứng: nếu ca
/// này cũng đỏ thì lỗi nằm ở phép bám, không phải ở nguồn nhãn.
#[test]
fn short_labels_that_match_the_line_all_bind() {
    let labels = [
        "Seed qvt, gỡ qvt0484 (Recommended)",
        "Seed qvt, giữ cả qvt0484",
        "Chỉ gỡ qvt0484, chưa seed ai",
        "Type something.",
        "Chat about this",
    ];
    let (html, linked, unlinked) = html_with_links(MSG, &anchors(&labels));
    assert!(unlinked.is_empty(), "neo rơi: {unlinked:?}\n{html}");
    assert_eq!(linked, 5, "{html}");
}

/// 🔴 CA CỦA HÀ: nhãn mang cả MÔ TẢ, nên không dòng nào chứa trọn nó.
///
/// `AskUserQuestion` cho mỗi lựa chọn một `label` và một `description`. Màn chỉ
/// in `label` ở dòng đầu rồi xuống dòng in mô tả. Nếu nhãn dùng làm NEO mang cả
/// hai thì `line_carries` không tìm thấy dòng nào chứa nó — và `☑` rơi mất, im
/// lặng, đúng ba cái đầu trong ảnh.
#[test]
fn a_label_carrying_its_description_still_finds_its_line() {
    let labels = [
        "Seed qvt, gỡ qvt0484 (Recommended) — Cấp admin cho qvt (tài khoản Hà thật sự dùng) và trả qvt0484 về false.",
        "Seed qvt, giữ cả qvt0484 — Hai tài khoản cùng là admin.",
        "Chỉ gỡ qvt0484, chưa seed ai — Trả prod về trạng thái 0 admin như trước.",
        "Type something.",
        "Chat about this",
    ];
    let (html, linked, unlinked) = html_with_links(MSG, &anchors(&labels));
    assert!(
        unlinked.is_empty(),
        "ba lựa chọn đầu mất ☑ — đúng ảnh Hà gửi: {unlinked:?}\n{html}"
    );
    assert_eq!(linked, 5, "{html}");
}
