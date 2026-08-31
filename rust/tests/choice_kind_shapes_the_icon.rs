//! Chọn MỘT thì radio, chọn NHIỀU thì checkbox — và hàng phiên nói cùng một thứ.
//!
//! 🔴 Hà 2026-08-30: *"Nếu option chỉ chọn 1 thì để nút radio và chọn nhiều mới
//! để checkbox, icon trạng thái phiên thêm icon checkbox nếu đang có option
//! chờ"*.
//!
//! Hai câu, một bộ ký hiệu. Trước đó `pipeline` gõ cứng `☑` cho mọi lựa chọn,
//! nên một hộp CHỌN MỘT đọc lên y hệt một hộp CHỌN NHIỀU — mà hai thứ ấy đòi hai
//! thao tác khác hẳn: một bên bấm là gửi, bên kia bấm là bật/tắt rồi còn phải
//! Submit. Đó đúng là con bug 2026-08-13 (*"option này chọn nhiều chứ không phải
//! chọn 1"*), lần này soi từ phía ngược lại.

use huba::sessions::{state_of, Asking, ChoiceKind, LiveSession, ST_ASK};

/// Ba giá trị, không phải hai — và `Unknown` vẽ CHECKBOX.
///
/// 🔴 Đây là bài quan trọng nhất tệp này, vì nó khoá một lựa chọn có HƯỚNG. Hai
/// cái sai không ngang giá: radio trên một hộp chọn-nhiều nói *"bấm một cái là
/// xong"*, chủ máy bấm rồi ngồi chờ một việc không xảy ra; checkbox trên một hộp
/// chọn-một thì bấm một cái nó vẫn gửi. Nên chỗ không đo được phải nghiêng về
/// checkbox.
#[test]
fn khong_do_duoc_thi_ve_checkbox_chu_khong_ve_radio() {
    assert_eq!(
        ChoiceKind::default(),
        ChoiceKind::Unknown,
        "mặc định là ẩn số"
    );
    assert_eq!(
        ChoiceKind::Unknown.glyph(),
        "☑",
        "không đo được mà vẽ radio là hứa một điều chưa đo"
    );
    assert_eq!(ChoiceKind::Multi.glyph(), "☑");
    assert_eq!(ChoiceKind::Single.glyph(), "◉");
    assert_ne!(
        ChoiceKind::Single.glyph(),
        ChoiceKind::Multi.glyph(),
        "hai loại hộp mà cùng một ký hiệu thì cả bản vá này không đo gì"
    );
}

/// Chỉ NHẬT KÝ mới được phép nói "chọn một".
///
/// Màn hình nói được `Multi` (có dòng `Submit`), nhưng VẮNG dòng ấy không chứng
/// minh được gì — nó có thể đã trôi khỏi khung nhìn. "Vắng bằng chứng" đọc thành
/// "bằng chứng vắng" là đúng cái hướng sai đắt hơn ở bài trên.
#[test]
fn man_hinh_khong_bao_gio_duoc_ket_luan_la_chon_mot() {
    assert_eq!(ChoiceKind::from_journal(false), ChoiceKind::Single);
    assert_eq!(ChoiceKind::from_journal(true), ChoiceKind::Multi);

    assert_eq!(ChoiceKind::from_screen(true), ChoiceKind::Multi);
    assert_eq!(
        ChoiceKind::from_screen(false),
        ChoiceKind::Unknown,
        "không thấy `Submit` ⟹ chưa biết, KHÔNG phải chọn một"
    );
}

fn phien_hoi(options: &[&str], multi: bool) -> LiveSession {
    LiveSession {
        host: "terminal".to_string(),
        asking: Some(Asking {
            header: "Chọn cách vá".to_string(),
            question: "Vá ACL thế nào?".to_string(),
            options: options.iter().map(|s| s.to_string()).collect(),
            multi,
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Hàng phiên mang ký hiệu khi CÓ option chờ, và KHÔNG mang khi không có.
///
/// Chấm cả hai chiều: một hàm luôn gắn ký hiệu cũng "qua" nếu chỉ chấm chiều có.
#[test]
fn hang_phien_mang_ky_hieu_dung_khi_co_option_cho() {
    let co = phien_hoi(&["Vá ngay", "Để sau"], false);
    let hang = huba::pipeline::session_list_text(std::slice::from_ref(&co), "", 0);
    assert!(
        hang.contains('◉'),
        "hộp CHỌN MỘT phải ra radio trên hàng phiên:\n{hang}"
    );

    let nhieu = phien_hoi(&["ACL", "Đăng nhập"], true);
    let hang = huba::pipeline::session_list_text(std::slice::from_ref(&nhieu), "", 0);
    assert!(
        hang.contains('☑'),
        "hộp CHỌN NHIỀU phải ra checkbox trên hàng phiên:\n{hang}"
    );

    // ĐỐI CHỨNG NGƯỢC ①: hỏi mà KHÔNG có lựa chọn nào (câu hỏi chữ tự do) thì
    // không được gắn — ký hiệu ấy hứa "bấm được ngay từ đây".
    let chu = phien_hoi(&[], false);
    let hang = huba::pipeline::session_list_text(std::slice::from_ref(&chu), "", 0);
    assert!(
        !hang.contains('◉') && !hang.contains('☑'),
        "không có lựa chọn nào mà vẫn mời bấm:\n{hang}"
    );

    // ĐỐI CHỨNG NGƯỢC ②: phiên không hỏi gì thì hàng phải sạch.
    let ranh = LiveSession {
        host: "terminal".to_string(),
        ..Default::default()
    };
    let hang = huba::pipeline::session_list_text(std::slice::from_ref(&ranh), "", 0);
    assert!(
        !hang.contains('◉') && !hang.contains('☑'),
        "phiên không hỏi gì mà hàng vẫn có ký hiệu chọn:\n{hang}"
    );
}

/// Ký hiệu này KHÔNG thay `❓` — nó đứng cạnh.
///
/// `❓` nói *"phiên dừng lại hỏi"*, đúng cả với một câu hỏi chữ tự do; ký hiệu
/// chọn nói *"có lựa chọn bấm được ngay từ đây"*. Hai câu dẫn tới hai thao tác,
/// nên nuốt cái này vào cái kia là mất một dữ kiện.
#[test]
fn ky_hieu_chon_dung_canh_dau_hoi_chu_khong_thay_no() {
    let s = phien_hoi(&["A", "B"], true);
    assert_eq!(state_of(&s).0, ST_ASK, "vẫn phải là ❓");
    let hang = huba::pipeline::session_list_text(std::slice::from_ref(&s), "", 0);
    assert!(hang.contains(ST_ASK), "mất ❓:\n{hang}");
    assert!(hang.contains('☑'), "mất ký hiệu chọn:\n{hang}");
}

/// 🔴 TIN GIM PHẢI NÓI CÙNG MỘT THỨ VỚI DANH SÁCH — Hà 2026-08-30, ngay sau lượt
/// cài đầu: *"Chưa thấy icon trạng thái option ở ds phiên"* rồi *"Trên pinned
/// cũng chưa có"*.
///
/// Câu thứ hai là chỗ hỏng thật. Lượt đầu tôi gắn ký hiệu vào hàng danh sách rồi
/// quên tin gim — dù tin gim đã mang `👁` cho monitor, tức nó vốn là một dòng
/// tình trạng phiên y như hàng danh sách. Vá một chỗ, sót chỗ bên cạnh: hình
/// dạng đã lặp nhiều lần trong repo này, và lần này chính chủ máy bắt được
/// trong vòng một giờ.
///
/// Nên bài kiểm chấm CẢ HAI màn trên CÙNG một phiên: lệch nhau là đỏ.
#[test]
fn tin_gim_va_danh_sach_noi_cung_mot_thu() {
    for (multi, can) in [(false, '◉'), (true, '☑')] {
        let s = phien_hoi(&["A", "B"], multi);
        let gim = huba::pipeline::pin_line(&s);
        let hang = huba::pipeline::session_list_text(std::slice::from_ref(&s), "", 0);
        assert!(gim.contains(can), "tin gim thiếu `{can}`: {gim}");
        assert!(hang.contains(can), "danh sách thiếu `{can}`:\n{hang}");
    }

    // ĐỐI CHỨNG NGƯỢC: phiên không có lựa chọn nào thì CẢ HAI phải sạch — không
    // thì một hàm luôn gắn ký hiệu cũng làm vòng trên xanh.
    let ranh = LiveSession {
        host: "terminal".to_string(),
        ..Default::default()
    };
    let gim = huba::pipeline::pin_line(&ranh);
    assert!(
        !gim.contains('◉') && !gim.contains('☑'),
        "phiên không hỏi gì mà tin gim vẫn mời bấm: {gim}"
    );
}

/// Phép đo nằm ở MỘT chỗ — `sessions::waiting_choice`.
///
/// Bài này chấm chính cái predicate, vì hai màn ở bài trên chỉ khớp nhau chừng
/// nào chúng còn hỏi chung một hàm. Ngày ai đó chép tay lại nó ở một chỗ thứ ba
/// thì bài trên vẫn xanh cho tới lúc hai bản lệch.
#[test]
fn mot_phep_do_duy_nhat_cho_ca_hai_man() {
    use huba::sessions::waiting_choice;
    assert_eq!(
        waiting_choice(&phien_hoi(&["A"], false)),
        Some(ChoiceKind::Single)
    );
    assert_eq!(
        waiting_choice(&phien_hoi(&["A", "B"], true)),
        Some(ChoiceKind::Multi)
    );
    // Hỏi mà không có lựa chọn nào (câu hỏi chữ tự do) ⟹ không có gì để bấm.
    assert_eq!(waiting_choice(&phien_hoi(&[], true)), None);
    assert_eq!(
        waiting_choice(&LiveSession {
            host: "terminal".to_string(),
            ..Default::default()
        }),
        None
    );
}

/// 🔴 BẢN CHỤP MÀN THẬT — phiên `[dwork]`, ttys001, window 386, 2026-08-31.
///
/// Hà gửi ảnh buồng chat kèm câu *"Phiên đang có hỏi đây"*: tin gim đọc
/// `💤 🟪 [dwork] (acc2)` trong khi màn treo nguyên bảng dưới đây. Giữ NGUYÊN
/// VĂN (kể cả khoảng trắng thụt đầu dòng) — sửa cho "gọn" là vứt đúng cái bằng
/// chứng khiến phép đo chạy đúng.
const MAN_THAT: &str = "\
Trong lúc chờ, xin Hà chốt luôn 2 câu đang chặn:
←  ☐ Q2 chọn mốc  ☐ Q3 hai lượt  ✔ Submit  →
Q2 — Khi tìm mốc vào/ra để ĐO ĐỘ PHỦ, có bỏ qua lượt `is_valid = 0` không?
❯ 1. (a) KHÔNG lọc — dev đề xuất
     Lấy mọi lượt trong dải; `is_valid` chỉ quyết NHÃN của ca.
  2. (b) Vẫn lọc `is_valid`
     Giữ nguyên hành vi mã hiện tại.
  3. Type something.
  4. Chat about this
Enter to select · Tab/Arrow keys to navigate · Esc to cancel";

/// Nhật ký MÙ mà màn thì thấy ⟹ phiên vẫn phải đọc ra `❓`, không phải `💤`.
///
/// Đây là ca Hà bắt được, và nó không hiếm: `pending_question` đọc nhật ký, mà
/// một bảng ĐANG TREO chưa chắc đã được ghi vào đó (đo 19/08 trên phiên amm: 0
/// lần `AskUserQuestion` trong 3,59 MB nhật ký trong khi bảng nằm trên màn).
/// Chỗ mù ấy từng được khai như một ca hiếm — nó là ca thường gặp nhất.
#[test]
fn man_thay_hop_chon_thi_phien_khong_duoc_doc_ra_dung_cho() {
    // Trước hết: phép đo trên MÀN phải thật sự thấy — không thì cả bài này chỉ
    // chấm hai con số tôi tự gõ vào.
    let so = huba::keys::parse_choices(MAN_THAT).len();
    assert_eq!(so, 4, "màn thật có 4 lựa chọn, `parse_choices` đọc ra {so}");
    // ⚠ `has_submit` KHÔNG thấy màn này, và đó là một dữ kiện chứ không phải một
    // thiếu sót: nó nhận hộp MỘT CÂU chọn-nhiều (ô `[ ]` + dòng `Submit` riêng).
    // Màn thật ở đây là bảng NHIỀU CÂU — `Submit` nằm TRONG thanh tab
    // `←  ☐ Q2 …  ✔ Submit  →`, nên phải hỏi `ask_table`. Bản đầu của tôi chỉ
    // hỏi `has_submit` và bài kiểm này bắt được ngay.
    assert!(
        !huba::keys::has_submit(MAN_THAT),
        "nếu `has_submit` bỗng thấy màn này thì hai phép đo đã trùng nhau — đọc lại"
    );
    assert!(
        huba::keys::ask_table(MAN_THAT).is_some(),
        "thanh tab `←  ☐ Q2 …  ✔ Submit  →` phải đọc được ⟹ bảng NHIỀU CÂU"
    );

    let s = LiveSession {
        host: "terminal".to_string(),
        // NHẬT KÝ MÙ — đúng như `huba sessions --json` khai lúc 2026-08-31:
        // `working:false · asking:không`.
        asking: None,
        screen_choices: so,
        screen_multi: true,
        ..Default::default()
    };
    assert_eq!(
        state_of(&s).0,
        ST_ASK,
        "màn treo bảng hỏi mà đọc ra `{}` — `đứng chờ` là câu SAI về việc chủ máy sắp phải làm",
        state_of(&s).0
    );
    assert_eq!(huba::sessions::waiting_choice(&s), Some(ChoiceKind::Multi));

    let hang = huba::pipeline::session_list_text(std::slice::from_ref(&s), "", 0);
    let gim = huba::pipeline::pin_line(&s);
    assert!(hang.contains('☑'), "danh sách thiếu ký hiệu:\n{hang}");
    assert!(gim.contains('☑'), "tin gim thiếu ký hiệu: {gim}");
    assert!(gim.contains(ST_ASK), "tin gim vẫn phải đổi sang ❓: {gim}");
}

/// ĐỐI CHỨNG NGƯỢC: màn KHÔNG có hộp chọn thì không được đẻ ra `❓`.
///
/// Thiếu bài này thì một hàm luôn trả `true` cũng làm bài trên xanh, và mọi
/// phiên rảnh trên máy sẽ đọc ra "dừng lại HỎI".
#[test]
fn man_khong_co_hop_chon_thi_phien_van_dung_cho() {
    let s = LiveSession {
        host: "terminal".to_string(),
        screen_choices: 0,
        ..Default::default()
    };
    assert_eq!(state_of(&s).0, huba::sessions::ST_WAIT);
    assert_eq!(huba::sessions::waiting_choice(&s), None);

    // Và một màn văn xuôi có đánh số KHÔNG được đọc thành hộp chọn — đây là
    // con bug 21/08 (`huba gắn ☑ vào ba dòng 1. 2. 3. của một đoạn văn`), nên
    // phép đo dùng ở đây phải là phép đo đã trị nó.
    let van_xuoi = "Tôi đã làm ba việc:\n1. đọc mã\n2. sửa\n3. chạy test\nXong.";
    assert_eq!(
        huba::keys::parse_choices(van_xuoi).len(),
        0,
        "đoạn văn đánh số không phải hộp chọn"
    );
}

/// NHẬT KÝ THẮNG MÀN khi cả hai cùng nói — vì chỉ nhật ký phân biệt được
/// chọn-một với chọn-nhiều.
#[test]
fn nhat_ky_thang_man_khi_ca_hai_cung_noi() {
    let mut s = phien_hoi(&["A", "B"], false);
    s.screen_choices = 2;
    s.screen_multi = true; // màn nói "chọn nhiều"…
    assert_eq!(
        huba::sessions::waiting_choice(&s),
        Some(ChoiceKind::Single),
        "…nhưng nhật ký khai `multiSelect: false`, và nó là nguồn có CẤU TRÚC"
    );
}
