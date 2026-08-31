//! Ký hiệu lựa chọn chạy trọn đường trên một NHẬT KÝ THẬT của máy này.
//!
//! `#[ignore]` vì nó đọc một tệp `.jsonl` trong `$HOME` — nhật ký thật của chủ
//! máy, không dựng lại được bằng fixture. Chạy tay:
//!
//! ```text
//! HUB_ASK_JSONL=/Users/…/<id>.jsonl \
//!   cargo test --offline --test choice_icon_on_a_real_transcript -- --ignored --nocapture
//! ```
//!
//! 🔴 Vì sao bài này phải có, và vì sao nó KHÔNG thừa so với
//! `choice_kind_shapes_the_icon`.
//!
//! Hà 2026-08-30, ngay sau khi cài bản mới: *"Chưa thấy icon trạng thái option ở
//! ds phiên"*. Đo trên log: đúng là chưa thấy — nhưng vì **chưa có phiên nào ở
//! trạng thái hỏi kể từ lúc cài** (tin `/session` lúc 23:10 có 2 phiên, cả hai
//! `💤`; hàng `❓` gần nhất là 11:07, trước lúc cài). Tức là *"chưa đo được"*,
//! không phải *"chạy rồi"* — và hai câu ấy không được nhìn giống nhau.
//!
//! Bài kiểm đơn vị kia dựng `Asking` **bằng tay**, nên nó chấm được khúc
//! `Asking → ký hiệu` mà mù hẳn khúc trước đó: *nhật ký thật có sinh ra một
//! `Asking` có `options` không*. Nếu `pending_question` trả `options` rỗng trên
//! dữ liệu thật thì cổng `!options.is_empty()` đóng vĩnh viễn và ký hiệu KHÔNG
//! BAO GIỜ hiện — mà mọi bài kiểm vẫn xanh. Đây là chỗ bịt đúng khoảng ấy.

use huba::sessions::{pending_question, LiveSession};

/// Nhật ký mặc định: phiên `f421daf8` — hàng `❓` cuối cùng đọc được trên máy này
/// (tin `/session` lúc 2026-08-30T11:07:33Z), và nó có đúng một lời gọi
/// `AskUserQuestion` trong 4,6 MB nhật ký.
const MAC_DINH: &str =
    "/Users/hanguyen/.claude/projects/-Users-hanguyen-projects/f421daf8-0211-4c61-a049-529917520640.jsonl";

#[test]
#[ignore = "đọc nhật ký thật trong $HOME — chạy tay bằng --ignored"]
fn hop_hoi_that_sinh_ra_ky_hieu_that() {
    let path = std::env::var("HUB_ASK_JSONL").unwrap_or_else(|_| MAC_DINH.to_string());
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("không đọc được nhật ký {path}: {e}"));

    // MẪU SỐ (luật 13③): tệp phải THẬT SỰ chứa một hộp hỏi. Không có thì bài này
    // không đo gì, và nó phải ĐỎ chứ không được xanh trong im lặng.
    let so_lan = text.matches("AskUserQuestion").count();
    assert!(
        so_lan > 0,
        "nhật ký {path} không có lời gọi `AskUserQuestion` nào — bài kiểm đang \
         nhìn nhầm tệp, đừng đọc con số 0 thành 'sạch'"
    );
    println!(
        "{path}\n  {so_lan} lời gọi AskUserQuestion · {} byte",
        text.len()
    );

    let hoi = pending_question(&text).expect(
        "nhật ký có `AskUserQuestion` mà `pending_question` trả None ⟹ khúc \
         nhật ký→Asking đứt, và ký hiệu sẽ không bao giờ hiện",
    );
    println!(
        "  header={:?} · {} lựa chọn · multi={}",
        hoi.header,
        hoi.options.len(),
        hoi.multi
    );
    for (i, o) in hoi.options.iter().enumerate() {
        println!("    {}. {o}", i + 1);
    }

    // Đây là cái cổng thật sự quyết định ký hiệu có hiện không.
    assert!(
        !hoi.options.is_empty(),
        "hộp hỏi thật mà đọc ra 0 lựa chọn ⟹ cổng `!options.is_empty()` đóng \
         vĩnh viễn và ký hiệu không bao giờ hiện"
    );

    // …rồi chạy nốt tới CHỮ mà chủ máy đọc trên điện thoại.
    let s = LiveSession {
        host: "terminal".to_string(),
        account: "acc1".to_string(),
        asking: Some(hoi.clone()),
        ..Default::default()
    };
    let hang = huba::pipeline::session_list_text(std::slice::from_ref(&s), "", 0);
    println!("  hàng phiên: {}", hang.lines().nth(1).unwrap_or(""));
    let can = if hoi.multi { '☑' } else { '◉' };
    assert!(
        hang.contains(can),
        "hộp {} mà hàng phiên không có `{can}`:\n{hang}",
        if hoi.multi {
            "CHỌN NHIỀU"
        } else {
            "CHỌN MỘT"
        }
    );
}
