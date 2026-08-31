//! Bấm xong thì tin phải dựng lại TỪ MÀN, không suy ra từ phép đếm.
//!
//! 🔴 Hà 2026-08-31, hai câu trong một buổi:
//! · *"Lựa chọn type something nhưng bấm vào đó lại không nhập được nội dung mà
//!   nó lại bị bỏ qua luôn"*
//! · *"Nếu nó có nhiều tab thì nó sẽ nhảy sang tab tiếp theo"* ⟹ *"Vậy tin đó
//!   phải sửa lại từ nội dung chụp lại"*
//!
//! Bản trước ĐẾM số lựa chọn trước/sau rồi suy ra một câu, và phép đếm ấy mù
//! đúng hai ca trên: nhảy tab cho ra một con số y hệt, còn ô nhập chữ mở ra thì
//! cho `0` — đọc thành *"bảng đã đóng"*. Cả hai đều là câu SAI về việc chủ máy
//! sắp phải làm.
//!
//! ⚠ Bài kiểm này chấm ĐẦU RA của `press_ack_from_screen`, tức đúng chuỗi đi ra
//! Telegram và đúng bộ mã nút gắn kèm. Chấm "có gọi `parse_choices` không" là
//! chấm một cách viết; cái Hà nhìn thấy là một cái tin.

use huba::pipeline::press_ack_from_screen;

/// Màn THẬT của phiên `[dwork]` (ttys001, window 386), đọc 2026-08-31 09:2x —
/// đúng cái bảng Hà đang treo lúc báo lỗi. Giữ nguyên văn, kể cả dấu.
const BANG_NHIEU_CAU: &str = "\
⏺ Trong lúc chờ, xin Hà chốt luôn 2 câu đang chặn:
────────────────────────────────────────────────
←  ☐ Q2 chọn mốc  ☐ Q3 hai lượt  ✔ Submit  →
Q2 — Khi tìm mốc vào/ra để ĐO ĐỘ PHỦ, có bỏ qua lượt `is_valid = 0` không?
❯ 1. (a) KHÔNG lọc — dev đề xuất
  2. (b) Vẫn lọc `is_valid`
  3. Type something.
  4. Chat about this
Enter to select · Tab/Arrow keys to navigate · Esc to cancel";

/// Cùng bảng ấy sau khi trả lời xong một câu: con trỏ đã sang **Q3**, và số lựa
/// chọn vẫn ĐÚNG BẰNG NHAU — đây là chỗ phép đếm cũ mù hẳn.
const BANG_SANG_TAB_KE: &str = "\
⏺ Trong lúc chờ, xin Hà chốt luôn 2 câu đang chặn:
────────────────────────────────────────────────
←  ☒ Q2 chọn mốc  ☐ Q3 hai lượt  ✔ Submit  →
Q3 — Hai lượt chấm trong cùng một ca thì tính thế nào?
❯ 1. (a) Gộp thành một
  2. (b) Giữ cả hai
  3. Type something.
  4. Chat about this
Enter to select · Tab/Arrow keys to navigate · Esc to cancel";

/// Bấm `Type something` xong: bảng biến mất, `claude` mở một ô nhập chữ.
const O_NHAP_CHU: &str = "\
⏺ Trong lúc chờ, xin Hà chốt luôn 2 câu đang chặn:
────────────────────────────────────────────────
❯ Nhập câu trả lời của bạn
────────────────────────────────────────────────
⏵⏵ auto mode on";

fn cau_hoi() -> Vec<String> {
    vec![
        "Q1 — câu đã trả lời từ trước".to_string(),
        "Q2 — Khi tìm mốc vào/ra để ĐO ĐỘ PHỦ, có bỏ qua lượt `is_valid = 0` không?".to_string(),
        "Q3 — Hai lượt chấm trong cùng một ca thì tính thế nào?".to_string(),
    ]
}

/// ① Nhảy sang tab kế thì TIN phải đổi theo — cả chữ lẫn nút.
#[test]
fn nhay_sang_tab_ke_thi_tin_doi_theo() {
    let (truoc, nut_truoc, _) = press_ack_from_screen(BANG_NHIEU_CAU, "1", "[dwork]", &cau_hoi());
    let (sau, nut_sau, _) = press_ack_from_screen(BANG_SANG_TAB_KE, "1", "[dwork]", &cau_hoi());

    // MẪU SỐ: hai màn phải THẬT SỰ cùng số lựa chọn, không thì bài này chứng
    // minh nhầm một chuyện dễ hơn hẳn.
    assert_eq!(
        nut_truoc.len(),
        nut_sau.len(),
        "hai màn phải cùng số lựa chọn thì mới chặn được phép ĐẾM: {nut_truoc:?} / {nut_sau:?}"
    );
    assert_eq!(nut_truoc.len(), 4, "{nut_truoc:?}");

    // …và dù cùng số, tin phải khác — vì câu hỏi đã khác.
    assert_ne!(
        truoc, sau,
        "cùng số lựa chọn mà tin y hệt ⟹ vẫn đang đếm chứ chưa đọc màn"
    );
    assert!(
        sau.contains("Gộp thành một"),
        "tin phải mang lựa chọn của câu ĐANG đứng:\n{sau}"
    );
    assert!(
        !sau.contains("KHÔNG lọc"),
        "tin còn mang lựa chọn của câu CŨ:\n{sau}"
    );
}

/// ② Mã nút phải mang SỐ CÂU đang đứng — bấm tiếp không được sửa vào câu đã chốt.
#[test]
fn ma_nut_mang_so_cau_dang_dung() {
    let (_, nut, _) = press_ack_from_screen(BANG_SANG_TAB_KE, "1", "[dwork]", &cau_hoi());
    let ma: Vec<&str> = nut.iter().map(|(m, _)| m.as_str()).collect();
    assert_eq!(
        ma,
        vec!["3.1", "3.2", "3.3", "3.4"],
        "con trỏ đang ở Q3 (câu thứ 3 trong sổ) nên mã phải là `3.<n>`: {nut:?}"
    );

    // ĐỐI CHỨNG NGƯỢC: ở màn TRƯỚC, con trỏ ở Q2 ⟹ mã phải là `2.<n>`. Không có
    // vế này thì một hàm luôn trả `3.` cũng làm bài trên xanh.
    let (_, nut2, _) = press_ack_from_screen(BANG_NHIEU_CAU, "1", "[dwork]", &cau_hoi());
    let ma2: Vec<&str> = nut2.iter().map(|(m, _)| m.as_str()).collect();
    assert_eq!(ma2, vec!["2.1", "2.2", "2.3", "2.4"], "{nut2:?}");
}

/// ③ Bấm `Type something` ⟹ KHÔNG được nói "bảng đã đóng", phải đưa màn ra.
#[test]
fn o_nhap_chu_khong_bi_doc_thanh_bang_da_dong() {
    let (ack, nut, submit) = press_ack_from_screen(O_NHAP_CHU, "3", "[dwork]", &cau_hoi());
    assert!(
        nut.is_empty(),
        "màn hết lựa chọn thì không gắn nút: {nut:?}"
    );
    assert!(!submit);
    assert!(
        !ack.contains("bảng đã đóng"),
        "câu ấy là lời ĐOÁN, và nó sai đúng ca này:\n{ack}"
    );
    assert!(
        ack.contains("Nhập câu trả lời của bạn"),
        "phải ĐƯA LUÔN MÀN để chủ máy tự nhìn:\n{ack}"
    );
    assert!(
        ack.contains("gõ thẳng vào đây"),
        "và phải nói ra việc kế tiếp làm được:\n{ack}"
    );
}

/// ④ Bảng MỘT câu: mã nút là số trần, không phải `<câu>.<n>`.
#[test]
fn bang_mot_cau_dung_ma_so_tran() {
    let mot_cau = "\
Do you want to make this edit to billing.rs?
❯ 1. Yes
  2. Yes, and don't ask again
  3. No, tell Claude what to do differently

Enter to confirm · Esc to cancel";
    let (ack, nut, _) = press_ack_from_screen(mot_cau, "1", "[huba]", &[]);
    let ma: Vec<&str> = nut.iter().map(|(m, _)| m.as_str()).collect();
    assert_eq!(
        ma,
        vec!["1", "2", "3"],
        "không có thanh tab ⟹ số trần: {nut:?}"
    );
    assert!(ack.contains("còn 3 lựa chọn"), "{ack}");
}

/// ⑤ ĐỐI CHỨNG NGƯỢC cho cả tệp: phép đo này bắt được một màn bị đánh tráo.
///
/// Không có bài này thì mọi assert trên vẫn xanh khi `press_ack_from_screen`
/// lười đi và trả về một câu cố định — cùng họ với cái phép ĐẾM nó vừa thay.
#[test]
fn phep_do_nay_do_duoc_khi_man_doi() {
    let (a, _, _) = press_ack_from_screen(BANG_NHIEU_CAU, "1", "[x]", &cau_hoi());
    let (b, _, _) = press_ack_from_screen(O_NHAP_CHU, "1", "[x]", &cau_hoi());
    let (c, _, _) = press_ack_from_screen(BANG_SANG_TAB_KE, "1", "[x]", &cau_hoi());
    assert_ne!(a, b);
    assert_ne!(b, c);
    assert_ne!(a, c);
}

/// ⑥ Nhận đúng mục "gõ chữ tự do" — thứ KHÔNG được bấm bằng đường kèm Enter.
///
/// 🔴 Đo trên bảng thật của Hà lúc 2026-08-31 15:58: bấm `3` (`Type something.`)
/// qua `do script` ⟹ thanh tab đọc lại ra `☒ Q2` trong khi chưa ai gõ một chữ
/// nào — câu ấy bị trả lời bằng chuỗi RỖNG rồi bảng nhảy sang Q3.
#[test]
fn nhan_dung_muc_go_chu_tu_do() {
    for co in [
        "Type something.",
        "Type something",
        "  3. Type something.  ",
        "[ ] Type something",
        "☐ Type something else",
        "TYPE SOMETHING.",
    ] {
        assert!(
            huba::keys::free_text_choice(co),
            "phải nhận ra mục tự-do: {co:?}"
        );
    }
    // ĐỐI CHỨNG NGƯỢC: mọi mục THƯỜNG vẫn phải bấm được như cũ. Nhận nhầm ở đây
    // thì huba thôi bấm những mục hoàn toàn bình thường.
    for khong in [
        "Chat about this",
        "(a) KHÔNG lọc — dev đề xuất",
        "Yes, and don't ask again",
        "Something typed by the user",
        "Type the answer into billing.rs",
        "",
    ] {
        assert!(
            !huba::keys::free_text_choice(khong),
            "KHÔNG được nhận nhầm mục thường: {khong:?}"
        );
    }
}
