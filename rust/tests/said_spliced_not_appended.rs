//! Lời cuối lấy từ nhật ký phải **ghép nối** với màn, không chép đè lên nó.
//!
//! 🔴 Hà 2026-08-25: *"tại sao mục này lại không tự viết thuật toán xử lý để
//! ghép nối luôn với màn hình chính lại cứ chèn thêm xuống cuối tin, nên nhiều
//! thông tin bị trùng nhau rất dài không hay lắm"*.
//!
//! Chỗ hở nằm ngay trong bản mô tả của `said_shown_on_screen`: *"đuôi của lượt
//! gần như luôn còn trên màn kể cả khi phần trên đã trôi mất"*. Câu ấy viết ra
//! để biện minh cho việc DÒ bằng đầu, rồi dừng lại — nên quyết định là nhị
//! phân: không bù gì, hoặc bù nguyên văn cả lượt. Mà theo đúng câu vừa trích,
//! ca "bù" gần như luôn kèm một cái đuôi đang nằm sờ sờ trên màn.

use huba::sessions::{said_missing_head, said_shown_on_screen};

/// Phần phiên nói TRƯỚC, đã cuộn khỏi khung nhìn.
const DAU: &str = "Xong cả hai việc — đã cài, đã push. Ảnh đen: huba nay đo ảnh \
                   trước khi gửi, và phân biệt màn bị khoá với thiếu quyền màn hình.";

/// Phần phiên nói SAU, vẫn còn trên màn.
const DUOI: &str = "Cần anh một cú bấm để nghiệm thu: gõ /clean khi có phiên đang \
                    bận mà anh vừa gõ chữ vào, rồi bấm một phím trong bảng chọn.";

fn said() -> String {
    format!("{DAU}\n{DUOI}")
}

/// Màn giữ ĐUÔI (cuộn mất từ trên xuống) — đúng hình dạng thường gặp nhất.
fn man_giu_duoi() -> String {
    format!("$ cargo test --offline\n   Compiling huba v0.1.0\n{DUOI}\n❯ \n")
}

/// 🔴 Ca chính: chỉ giao lại phần màn THIẾU, không giao cả lượt.
#[test]
fn only_the_scrolled_off_head_comes_back() {
    let head = said_missing_head(&said(), &man_giu_duoi()).expect("màn thiếu phần đầu");
    assert!(
        head.contains("đã push"),
        "mất phần đã cuộn khỏi màn — đây là thứ duy nhất nhật ký cần bù:\n{head}"
    );
}

/// 🔴 ĐỐI CHỨNG NGƯỢC — hành vi CŨ (bù nguyên văn cả lượt) phải biến mất.
/// Gỡ thuật toán ghép nối là bài này đỏ ngay.
#[test]
fn the_part_still_on_screen_is_not_repeated() {
    let said = said();
    let head = said_missing_head(&said, &man_giu_duoi()).expect("màn thiếu phần đầu");
    assert!(
        !head.contains("bảng chọn"),
        "chép lại đúng thứ đang nằm trên màn ⟹ tin dài gấp đôi mà không thêm tin gì:\n{head}"
    );
    assert_ne!(
        head.trim(),
        said.trim(),
        "vẫn giao nguyên văn cả lượt — đúng hành vi vừa đi gỡ"
    );
    assert!(
        head.chars().count() < said.chars().count(),
        "phần bù phải NGẮN HƠN cả lượt"
    );
}

/// Cắt nới tới hết TỪ đang dở, nên không bao giờ cụt giữa chữ.
#[test]
fn the_cut_never_lands_inside_a_word() {
    let said = said();
    let head = said_missing_head(&said, &man_giu_duoi()).expect("màn thiếu phần đầu");
    assert!(
        said.starts_with(&head),
        "phần bù phải là một tiền tố ĐÚNG của lượt, không phải chuỗi dựng lại"
    );
    let ke_tiep = said[head.len()..].chars().next();
    assert!(
        ke_tiep.is_none_or(char::is_whitespace),
        "cắt cụt giữa một từ — ký tự ngay sau chỗ cắt là {ke_tiep:?}, đáng lẽ phải là khoảng trắng"
    );
}

/// Màn hiện TRỌN (thấy được cả phần đầu) ⟹ không bù gì. Màn chỉ cuộn mất từ
/// trên xuống, nên thấy đầu nghĩa là không mất gì.
#[test]
fn a_fully_visible_turn_gets_nothing_appended() {
    let man = format!("⏺ {DAU}\n{DUOI}\n❯ \n");
    assert_eq!(said_missing_head(&said(), &man), None);
    assert!(said_shown_on_screen(&said(), &man));
}

/// Màn KHÔNG mang lời nào của lượt (ca 17/08 — `/shot` ra nguyên một tệp mã)
/// ⟹ bù trọn, như cũ. Bản vá này không được làm mất đường lùi ấy.
#[test]
fn a_screen_with_none_of_the_turn_still_gets_all_of_it() {
    let man = "$ cat src/main.rs\nfn main() { println!(\"chao\"); }\n… +35 lines\n❯ \n";
    let head = said_missing_head(&said(), man).expect("màn không có lời nào");
    assert_eq!(head.trim(), said().trim());
    assert!(!said_shown_on_screen(&said(), man));
}

/// Lượt quá ngắn ⟹ "coi như đã hiện, đừng bù" — luật cũ, giữ nguyên.
#[test]
fn a_very_short_turn_is_never_backfilled() {
    assert_eq!(said_missing_head("Xong rồi.", "màn nào đó"), None);
    assert!(said_shown_on_screen("Xong rồi.", "màn nào đó"));
}

/// Lượt dài hơn `SAID_PROBE_MIN` nhưng ngắn hơn `SAID_PROBE` vẫn phải chạy —
/// mốc dò rút ngắn theo. Không có hàng rào này thì lát cắt trong hàm nổ.
#[test]
fn a_turn_between_the_two_probe_bounds_does_not_panic() {
    let ngan = "Đã cài xong bản mới và khởi động lại daemon rồi nhé anh.";
    assert!(ngan.chars().filter(|c| c.is_alphanumeric()).count() > 24);
    let _ = said_missing_head(ngan, "một màn không liên quan gì cả");
    let _ = said_missing_head(ngan, ngan);
}
