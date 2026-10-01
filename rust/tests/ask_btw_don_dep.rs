//! `/ask` hỏi thẳng bằng `/btw` mà hỏng thì huba phải DỌN cái nó đã gõ vào phiên.
//!
//! 🔴 Hà 2026-10-01 09:5xZ: *"Tại sao lệnh ask lại bị dán vào ô chat và nằm ở đó"*
//! · *"Tôi phải dùng lệnh clean"*. Đo trên `logs/huba.log` cùng ngày:
//!
//! - `ask_via_btw` CHỈ GÕ `/btw <câu hỏi>` (`keys::type_into`), không có cú Enter
//!   rời — chú thích tại chỗ tự khai *"CÓ THỂ đang hỏng câm"*. Luật 13: chữ +
//!   xuống dòng trong một lượt ghi là một cú DÁN, nên câu nằm lại trong ô.
//! - 47 lượt `/ask` đi đường này từ 15/09: 34 trả lời được, **13
//!   `btw_no_answer_yet`** ⟹ rơi về fork, câu `/btw` bỏ lại trong ô.
//! - 11/13 lượt ấy được vòng tự gỡ kẹt bấm Enter hộ vài phút SAU (6 lần `sent`:
//!   câu hỏi vào phiên MUỘN, sau khi Hà đã nhận câu trả lời của bản sao). Vòng ấy
//!   tắt 01/10 07:50Z ⟹ lượt 09:33Z vào `[onghut]` nằm lại tới khi Hà `/clean`.
//! - Câu trả lời của lượt ấy vẫn khai *"hỏi trên bản sao — phiên gốc không bị
//!   đụng"*.

use huba::pipeline::cach_hoi_ben_le;
use huba::sessions::{btw_bo_do, Aside};

/// Màn thật: ô nhập TRỐNG, bảng subagent bên dưới (`o_nhap_giua_hai_vien.rs`).
const MAN_O_TRONG: &str = include_str!("fixtures/man-bang-subagent-duoi-o-nhap-2026-09-23.txt");
/// Dấu nhắc trống của ô trong màn ấy (sau `❯` là NBSP).
const DAU_NHAC_TRONG: &str = "❯\u{a0}\n\n";
/// Đúng câu Hà hỏi lượt hỏng 01/10 (phần đầu, như log ghi).
const CAU: &str = "/btw Tôi chưa hiểu cụ thể tele sử dụng để làm gì";

fn o_co(chu: &str) -> String {
    MAN_O_TRONG.replacen(DAU_NHAC_TRONG, &format!("❯\u{a0}{chu}\n\n"), 1)
}

#[test]
fn cau_btw_nam_trong_o_thi_phai_don() {
    assert!(
        MAN_O_TRONG.contains(DAU_NHAC_TRONG),
        "tệp mẫu đổi — bài kiểm đang đo nhầm thứ"
    );
    assert_eq!(
        btw_bo_do(&o_co(CAU), CAU),
        (false, true),
        "câu /btw nằm trong ô mà không thấy ⟹ rơi về fork bỏ lại nó, đúng lỗi 01/10"
    );
}

/// Đối chứng ngược: câu ấy đã GỬI đi thì `claude` vẽ lại nó PHÍA TRÊN ô — dọn
/// theo chỗ ấy là xoá chữ của chủ máy (ô đang trống, hoặc đang có bản nháp).
#[test]
fn cau_btw_da_gui_nam_phia_tren_o_thi_khong_don() {
    let tren = MAN_O_TRONG.replacen("────", &format!("{CAU}\n────"), 1);
    assert!(
        tren.contains(CAU),
        "phải cấy được câu vào phần hội thoại phía trên"
    );
    assert_eq!(btw_bo_do(&tren, CAU), (false, false));
    assert_eq!(btw_bo_do(MAN_O_TRONG, CAU), (false, false));
}

/// Bảng `/btw` đang mở (kể cả đang viết) ⟹ phải đóng. Đây là cửa DUY NHẤT cho
/// Esc — Esc lúc không có bảng là ngắt lượt đang chạy của phiên.
#[test]
fn bang_btw_dang_mo_thi_phai_dong_va_chi_khi_ay() {
    let bang = format!("{CAU}\n  ✳ Answering…\nEsc to close\n");
    assert!(btw_bo_do(&bang, CAU).0);
    assert!(
        !btw_bo_do(&o_co(CAU), CAU).0,
        "không có chân bảng mà đòi Esc ⟹ ngắt phiên"
    );
}

fn aside(new_id: &str, btw_hong: Option<&str>) -> Aside {
    Aside {
        source_id: "4ce96a0e".into(),
        source_name: "projects-1d".into(),
        new_session_id: new_id.into(),
        question: "tele dùng để làm gì".into(),
        answer: "…".into(),
        cost_usd: 0.0,
        ts: "2026-10-01T09:35:22Z".into(),
        btw_hong: btw_hong.map(str::to_string),
    }
}

#[test]
fn cau_tra_loi_khong_khai_khong_bi_dung_khi_da_go_vao_phien() {
    let hong = cach_hoi_ben_le(&aside("5bfd21a2", Some("đã xoá câu khỏi ô — ô nhập sạch")));
    assert!(
        !hong.contains("không bị đụng"),
        "huba ĐÃ gõ /btw vào phiên mà vẫn khai 'không bị đụng': {hong}"
    );
    assert!(
        hong.contains("ô nhập sạch"),
        "phải kể phiên còn lại gì: {hong}"
    );
    // Hai đường cũ giữ nguyên câu của chúng.
    assert!(cach_hoi_ben_le(&aside("5bfd21a2", None)).contains("phiên gốc không bị đụng"));
    assert!(cach_hoi_ben_le(&aside("4ce96a0e", None)).contains("bằng /btw"));
}

/// ⚠ KHOÁ YẾU — đọc MÃ NGUỒN, không chạy hàm (hàm cần một cửa sổ Terminal thật).
/// Chứng minh TỒN TẠI chứ không chứng minh ĐÚNG: đường `/btw` phải gửi qua cửa
/// chung có Enter rời, và gọi dọn dẹp ở cả ba nhánh hỏng sau khi đã gõ.
#[test]
fn khoa_yeu_duong_btw_gui_qua_type_and_send_va_don_o_moi_nhanh_hong() {
    let src = include_str!("../src/sessions.rs");
    let dau = src.find("fn ask_via_btw(").expect("còn hàm ask_via_btw");
    let than = &src[dau..];
    let than = &than[..than.find("\n}\n").expect("hết thân hàm")];
    assert!(
        than.contains("type_and_send("),
        "đường /btw phải gửi qua keys::type_and_send"
    );
    assert!(
        !than.contains("type_into("),
        "type_into là CHỈ GÕ — chữ nằm lại trong ô (luật 13), đúng lỗi 01/10"
    );
    assert!(
        than.matches("don_btw_bo_do(").count() >= 3,
        "ba nhánh hỏng sau khi gõ (không gửi được · lỗi gõ · hết giờ chờ) đều phải dọn"
    );
}
