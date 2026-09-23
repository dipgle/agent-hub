//! Phiên ĐANG THEO đã tắt: nói ra sớm, nói ra nhanh, và nói điều dùng được.
//!
//! 🔴 Hà 2026-09-23: *"Tại sao gửi ảnh thì được mà file md thì không gửi vào
//! phiên?"* rồi *"Làm luôn cả ba lỗi đó đi"*. Đo trên `logs/huba.log` cùng ngày:
//! không phải do loại tệp — 12:13Z cùng tệp `.md` ấy vào phiên được. Lúc 08:51Z
//! tệp đã về máy (`telegram_file_received`), nhưng:
//!
//! 1. phiên đang theo (`[dwork] Dwork đợt 41`) đã tắt từ 08:37Z, và tin báo tắt
//!    khi ấy không nói đó là phiên con trỏ đang trỏ vào;
//! 2. câu trả lời chỉ là `⚠ không thấy phiên '<uuid 36 ký tự>' trong danh sách`
//!    — không nói tệp đã nằm trên máy, không đưa đường sang phiên khác;
//! 3. câu ấy tới sau **46 giây**: sổ theo dõi xoá phiên đã tắt, nên `/type` dựng
//!    NGUYÊN một ảnh chụp đọc chữ mọi tab Terminal (hết giờ 20 giây lúc máy nặng)
//!    chỉ để nhận lại "không thấy".
//!
//! Phần CÂU CHỮ kiểm bằng hàm thuần. Phần NỐI DÂY đọc mã nguồn lúc chạy
//! (`HUBA_PIPELINE_SRC` trỏ được sang bản cũ để chạy đối chứng ĐỎ), cùng khuôn
//! `hai_hang_lenh.rs`.

use huba::pipeline::{file_path_in_typed, gone_reply};

const UUID: &str = "4f956d2a-defb-4c4e-b022-19c296f39157";

fn links() -> Vec<(String, String)> {
    vec![
        (
            "[dwork]·Main dwork phiên 43".to_string(),
            "https://t.me/bot?start=s_f3b764dd-1bdb-4e60-a682-23109d438418".to_string(),
        ),
        (
            "[huba]".to_string(),
            "https://t.me/bot?start=s_c6923d05-7bd4-4fa2-b127-13c229e1c559".to_string(),
        ),
    ]
}

#[test]
fn doc_duong_dan_tep_trong_cau_xem_tep() {
    assert_eq!(
        file_path_in_typed(
            "Xem tệp: /Users/h/projects/.inbox/4f956d2a/bao-cao-ngay-23.md — đọc giúp"
        ),
        Some("/Users/h/projects/.inbox/4f956d2a/bao-cao-ngay-23.md")
    );
    assert_eq!(file_path_in_typed("Xem tệp: /a/b.jpg"), Some("/a/b.jpg"));
    // Câu thường không phải câu gửi tệp.
    assert_eq!(file_path_in_typed("xem giúp tệp này"), None);
    assert_eq!(file_path_in_typed("Xem tệp: "), None);
}

/// ② Ca 08:51Z: tệp đã về máy, phiên đang theo đã tắt.
#[test]
fn cau_tra_loi_noi_tep_da_luu_phien_da_tat_va_duong_sang_phien_khac() {
    let s = gone_reply(
        Some(("[dwork]·Dwork đợt 41 giao nhận", "15:37 23/09")),
        UUID,
        true,
        "Xem tệp: /Users/h/projects/.inbox/4f956d2a/bao-cao-ngay-23.md",
        &links(),
    );
    assert!(
        s.contains("Dwork đợt 41 giao nhận đã tắt lúc 15:37 23/09"),
        "{s}"
    );
    assert!(s.contains("CHƯA gõ vào đâu cả"), "{s}");
    assert!(
        s.contains("📎 Tệp ĐÃ lưu trên máy: /Users/h/projects/.inbox/4f956d2a/bao-cao-ngay-23.md"),
        "không nói tệp đã nằm trên máy — đọc lên y hệt 'tệp không gửi được':\n{s}"
    );
    assert!(s.contains("Gửi lại nguyên câu này"), "{s}");
    assert!(
        s.contains("👁 [dwork]·Main dwork phiên 43: https://t.me/bot?start=s_f3b764dd"),
        "{s}"
    );
    assert!(
        !s.contains(UUID),
        "uuid 36 ký tự không phải thứ người đọc dùng được:\n{s}"
    );
}

/// Sổ phiên-đã-tắt không nhớ ⟹ không bịa giờ tắt; nói id NGẮN.
#[test]
fn khong_biet_gio_tat_thi_khong_bia() {
    let s = gone_reply(None, UUID, true, "chạy tiếp đi", &links());
    assert!(
        s.contains("phiên 4f956d2a không còn trong danh sách"),
        "{s}"
    );
    assert!(!s.contains("đã tắt lúc"), "bịa giờ tắt: {s}");
    assert!(!s.contains("📎"), "câu thường không có tệp: {s}");
}

/// Lệnh không phải `/type` ⟹ không kể "câu chưa gửi"; không còn phiên nào ⟹ nói ra.
#[test]
fn lenh_khac_va_khong_con_phien_nao() {
    let s = gone_reply(None, UUID, false, "", &[]);
    assert!(s.contains("lệnh vừa rồi CHƯA làm gì cả"), "{s}");
    assert!(!s.contains("Gửi lại"), "{s}");
    assert!(s.contains("Không thấy phiên nào khác đang chạy"), "{s}");
}

// ───────────── NỐI DÂY — đọc mã nguồn ─────────────

fn nguon() -> String {
    let p = std::env::var("HUBA_PIPELINE_SRC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs").to_string());
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: không đọc được {p}: {e}"))
}

/// Khúc tra phiên của nhóm `/type`·`/key`·`/shot`… — từ chỗ tách đích tới chỗ
/// giữ `shot_sid` (cả hai đều là chuỗi duy nhất trong tệp, đo 2026-09-23).
fn khuc_tra_phien(src: &str) -> Option<&str> {
    let dau = src.find("let (want, typed) = target_and_rest(db, &cmd.arg);")?;
    let cuoi = src[dau..].find("let shot_sid = target")?;
    Some(&src[dau..dau + cuoi])
}

/// ③ Báo NGAY: phép tra sổ nhẹ phải đứng TRƯỚC ảnh chụp và quyết được việc bỏ nó.
fn bao_nhanh(khuc: &str) -> bool {
    match (
        khuc.find("listed_in_books("),
        khuc.find("sessions::snapshot("),
    ) {
        (Some(so), Some(anh)) => so < anh && khuc.contains("(None, true) => None"),
        _ => false,
    }
}

#[test]
fn tra_phien_bao_nhanh_khong_chup_terminal_khi_phien_da_di() {
    let src = nguon();
    let khuc = khuc_tra_phien(&src).expect("KHÔNG ĐO ĐƯỢC: không tìm thấy khúc tra phiên");
    assert!(
        bao_nhanh(khuc),
        "`/type` với phiên đã đi vẫn dựng ảnh chụp Terminal trước khi báo"
    );
}

#[test]
fn khong_con_cau_uuid_tron_trong_danh_sach() {
    let src = nguon();
    assert!(
        !src.contains("trong danh sách\","),
        "câu cũ `⚠ không thấy phiên '<uuid>' trong danh sách` vẫn còn trong pipeline.rs"
    );
    assert!(
        src.contains("focus_gone_reply("),
        "không thấy chỗ gọi câu trả lời mới"
    );
}

/// ① Tin "đã tắt" của phiên ĐANG THEO phải nói ra đúng điều ấy.
#[test]
fn tin_da_tat_cua_phien_dang_theo_noi_ra() {
    let src = nguon();
    let dau = src
        .find("pub fn announce_changes(")
        .expect("KHÔNG ĐO ĐƯỢC: không thấy `announce_changes`");
    let cuoi = src[dau..].find("\n}\n").expect("không cắt được thân hàm");
    let than = &src[dau..dau + cuoi];
    assert!(
        than.contains("same_session(&id, &focused)") && than.contains("ĐANG THEO"),
        "tin tắt của phiên đang theo không nói gì khác tin tắt thường"
    );
}

/// ĐỐI CHỨNG NGƯỢC cho phép dò `bao_nhanh`: hình dạng cũ phải bị bắt.
#[test]
fn doi_chung_nguoc_phep_do_bao_nhanh() {
    let cu = "let (want, typed) = target_and_rest(db, &cmd.arg);\n let live = match &booked { Some(_) => None, None => Some(crate::sessions::snapshot(cfg)) };\n let shot_sid = target";
    assert!(
        !bao_nhanh(khuc_tra_phien(cu).unwrap()),
        "phép dò mù với hình dạng cũ"
    );
    let moi = "let (want, typed) = target_and_rest(db, &cmd.arg);\n listed_in_books(cfg, &want);\n (Some(_), _) | (None, true) => None,\n (None, false) => Some(crate::sessions::snapshot(cfg)),\n let shot_sid = target";
    assert!(
        bao_nhanh(khuc_tra_phien(moi).unwrap()),
        "phép dò chặn cả hình dạng đúng"
    );
}
