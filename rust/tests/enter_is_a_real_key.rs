//! Cú Enter bù của huba phải là một phím RỜI, không phải một byte CR ghi kèm.
//!
//! 🔴 Đo trên máy thật (log `hubd.err`, 08–10/09): `auto_unstick_box` bấm Enter
//! bằng đường BYTE (`keys::press` → `do_script`) **31 lượt, 30 lượt đọc lại
//! thấy chữ vẫn nằm nguyên trong ô nhập**. Bốn lượt cuối rơi vào ba phiên
//! `limited = None` — tức không phải do hạn mức chặn. Một trong ba
//! (`projects-ef`, `ttys000`) giữ nguyên một câu **55 ký tự** trong ô nhập từ
//! 08/09 sang 10/09; không cú Enter nào đẩy nổi.
//!
//! Cùng khoảng ấy, đường phím RỜI (`send_bare` → `cgkeys::post` →
//! `CGEventPostToPid`) ăn thật ở hai cửa khác: `trust_dialog_answered` 24 lượt,
//! `free_text_opened_by_bare_key` 7 lượt.
//!
//! Vì sao byte hụt: `do_script` ghi cả gói MỘT lần, TUI đọc thành một lượt
//! **DÁN**, và trong lượt dán thì CR là ký tự xuống dòng của nội dung chứ không
//! phải cú gửi — đúng thứ `CLAUDE.md` §13 đã ghi cho lượt gõ chữ, chỉ chưa ai
//! đọc nó cho cú Enter trần.
//!
//! Luật này repo đã viết **hai lần** rồi: `/close` (2026-08-30) rồi `/key` +
//! `press_escape` (2026-09-01), và chú thích của `press_escape` đã tự cảnh báo
//! *"hai bản chép của một luật là hai bản sẽ lệch"*. Đây là cửa thứ ba, nên nó
//! đi vào cùng MỘT hàm: `keys::press_enter`.

use huba::keys::{enter_verdict, EnterHow};

#[test]
fn a_bare_key_that_lands_is_the_answer() {
    assert_eq!(
        enter_verdict(None, None),
        EnterHow::Bare,
        "phím rời đi được thì không được đụng tới đường byte"
    );
    // Kể cả khi đường byte cũng có chuyện gì đó để nói, phím rời vẫn thắng —
    // vì lúc phím rời `Ok` thì đường byte KHÔNG ĐƯỢC chạy.
    assert_eq!(
        enter_verdict(None, Some("thứ này không được phép xảy ra")),
        EnterHow::Bare
    );
}

#[test]
fn the_byte_path_is_a_fallback_and_says_so() {
    assert_eq!(
        enter_verdict(Some("không tin cậy Trợ năng"), None),
        EnterHow::WrittenByte,
        "hụt phím rời mà byte đi được thì kết cục phải NÓI RA là đã đi bằng byte \
         — hai đường ấy không đáng tin như nhau, gộp tên là mất phép đo"
    );
}

#[test]
fn losing_both_paths_is_a_failure_carrying_both_reasons() {
    let v = enter_verdict(Some("hụt cgkeys"), Some("hụt do script"));
    match v {
        EnterHow::Failed(vi_sao) => {
            assert!(
                vi_sao.contains("hụt cgkeys") && vi_sao.contains("hụt do script"),
                "phải mang CẢ HAI lý do, không rút gọn còn một: {vi_sao}"
            );
        }
        khac => panic!("hụt cả hai đường mà không phải Failed: {khac:?}"),
    }
}

/// Đối chứng ngược của chính bộ ba trên: ba đầu vào khác nhau phải cho ba kết
/// cục KHÁC nhau. Thiếu ca này thì một hàm trả hằng số cũng làm hai bài đầu
/// xanh một nửa.
#[test]
fn the_three_cases_are_three_different_answers() {
    let bare = enter_verdict(None, None);
    let byte = enter_verdict(Some("a"), None);
    let hong = enter_verdict(Some("a"), Some("b"));
    assert_ne!(bare, byte);
    assert_ne!(byte, hong);
    assert_ne!(bare, hong);
}

// ─────────────── ĐIỂM GỌI: cái enum đúng không cứu được lượt gõ sai ─────────
//
// MẪU SỐ — bài dưới đây đo **chuỗi trong mã nguồn** của thân `auto_unstick_box`.
// Nó KHÔNG chứng minh nhánh ấy được chạy, và không thay được một lượt bấm thật.
// Nhưng nó bịt đúng chỗ mù đã trả giá ở `handover_button.rs`: bộ ba bài trên
// khoá `enter_verdict` rất chặt, mà nếu chỗ gọi vẫn `keys::press(w, "enter")`
// thì cả ba vẫn xanh trong khi ô nhập ngoài kia không nhúc nhích.

/// Mã nguồn của chính `pipeline.rs`.
fn nguon_pipeline() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs"))
        .expect("đọc được src/pipeline.rs")
}

/// Thân hàm `auto_unstick_box` — cắt từ dòng khai báo tới hàm kế tiếp ở cột 0.
/// `None` = không tìm thấy ⟹ chỗ gọi phải ĐỎ: hàm đã đổi tên mà bài kiểm không
/// hay, và "không đo được" không được lẫn vào "sạch".
fn than_auto_unstick_box(src: &str) -> Option<&str> {
    let i = src.find("fn auto_unstick_box(")?;
    let sau = &src[i..];
    let j = sau[1..].find("\nfn ").map(|k| k + 1).unwrap_or(sau.len());
    Some(&sau[..j])
}

#[test]
fn the_unstick_press_goes_through_the_bare_key_ladder() {
    let src = nguon_pipeline();
    let than = than_auto_unstick_box(&src).expect(
        "không thấy `fn auto_unstick_box(` trong src/pipeline.rs — đổi tên rồi thì \
         bài này KHÔNG đo được nữa, và đó là trạng thái riêng, không phải xanh",
    );
    assert!(
        than.contains("keys::press_enter("),
        "thân `auto_unstick_box` phải gọi `keys::press_enter` — cái thang phím \
         rời-trước nằm trong đó:\n{than}"
    );
    assert!(
        !than.contains("keys::press(window, \"enter\")"),
        "vẫn còn đường byte trần trong thân `auto_unstick_box` — đúng cái đã hụt \
         30/31 lượt:\n{than}"
    );
}

/// Đối chứng ngược cho CHÍNH phép cắt trên: bơm một nguồn giả mang đường byte
/// trần thì phép đo phải nhìn thấy nó. Thiếu ca này thì một hàm cắt trả ra
/// chuỗi rỗng cũng làm bài trên xanh.
#[test]
fn the_call_site_probe_can_see_a_bare_byte_press() {
    let gia = "fn auto_unstick_box(cfg: &Config) {\n    crate::keys::press(window, \"enter\");\n}\nfn khac() {}\n";
    let than = than_auto_unstick_box(gia).expect("cắt được thân hàm trong nguồn giả");
    assert!(than.contains("keys::press(window, \"enter\")"), "{than}");
    assert!(!than.contains("keys::press_enter("), "{than}");
    // Và phép cắt phải DỪNG ở hàm kế tiếp, không nuốt cả tệp.
    assert!(!than.contains("fn khac"), "cắt lố sang hàm sau: {than}");
}
