//! Mỗi hàng `/accounts` phải nói GIỜ SẮP RESET, không chỉ hàng đã kịch trần.
//!
//! 🔴 Hà 2026-09-16 07:46, ảnh chụp `/accounts` trên Telegram: *"Danh sách acc
//! thiếu giờ sắp reset"*. Đúng — và chỗ thiếu đo được ngay trên ảnh ấy: trong
//! năm tài khoản, chỉ acc2 (đã kịch trần) mang `mở lại 17:59 16/09`; bốn hàng
//! còn lại dừng ở `hạn mức: tuần 61% · 5 tiếng 16% · hạng: … · đo 3 phút trước`.
//!
//! Dữ kiện thì huba ĐÃ CÓ: `quota_read` cùng đêm ấy ghi `week_resets_at` cho cả
//! năm tài khoản (acc1 `2026-09-20T08:00Z`, acc3 `09-22T06:00Z`, acc4
//! `09-22T15:00Z`, acc5 `09-21T09:00Z`). Nó chưa bao giờ đi ra tới màn.
//!
//! `now_ms` là THAM SỐ ở mọi bài dưới đây — không hàm nào được hỏi đồng hồ
//! thật, vì một bài kiểm hỏi đồng hồ thật là một bài kiểm tự đỏ vào một giờ nào
//! đó mà không ai đụng vào mã (đã trả giá 2026-09-02, xem `runtime.rs`).

use huba::quota::{sap_reset, Quota};

/// 2026-09-16T00:46:00Z — đúng khoảnh khắc ảnh chụp của Hà.
const BAY_GIO: i64 = 1_789_519_560_000;

fn moc(them_phut: i64) -> String {
    chrono::DateTime::from_timestamp_millis(BAY_GIO + them_phut * 60_000)
        .expect("mốc hợp lệ")
        .to_rfc3339()
}

fn q(week_pct: i64, week_in: i64, hour5_pct: i64, hour5_in: i64) -> Quota {
    Quota {
        account: "acc1".into(),
        week_pct: Some(week_pct),
        week_resets_at: Some(moc(week_in)),
        hour5_pct: Some(hour5_pct),
        hour5_resets_at: Some(moc(hour5_in)),
        fetched_at_ms: Some(BAY_GIO - 180_000),
        why_unknown: None,
        chua_dung_duoc: None,
    }
}

#[test]
fn hang_binh_thuong_van_phai_co_gio_reset() {
    // acc1 của ảnh chụp: tuần 61%, 5 tiếng 16% ⟹ cửa sổ TUẦN là cái chật nhất.
    let s = sap_reset(&q(61, 4 * 1440, 16, 200), BAY_GIO).expect("phải có giờ reset");
    assert!(s.starts_with("reset tuần "), "sai cửa sổ: {s}");
    assert!(s.contains("còn 4 ngày"), "thiếu khoảng cách còn lại: {s}");
}

/// Mốc in ra phải là mốc của ĐÚNG cửa sổ đã chấm hạng — cái CHẬT NHẤT.
///
/// acc4 trong ảnh: tuần 16%, 5 tiếng 45% ⟹ hàng in `hạng: đã dùng 45%`, nên mốc
/// đi kèm phải là mốc 5 tiếng. In mốc tuần ở đó là đặt hai phép đo nói về hai
/// thứ khác nhau cạnh nhau như một.
#[test]
fn moc_phai_theo_cua_so_chat_nhat_chu_khong_phai_luon_la_tuan() {
    let s = sap_reset(&q(16, 6 * 1440, 45, 137), BAY_GIO).expect("phải có giờ reset");
    assert!(s.starts_with("reset 5 tiếng "), "sai cửa sổ: {s}");
    assert!(s.contains("còn 2 tiếng"), "sai khoảng cách: {s}");
}

#[test]
fn hoa_thi_lay_cua_so_tuan() {
    let s = sap_reset(&q(40, 2 * 1440, 40, 90), BAY_GIO).expect("phải có giờ reset");
    assert!(s.starts_with("reset tuần "), "hoà phải nghiêng về tuần: {s}");
}

/// Mốc đã QUA thì thà im còn hơn in một cái hẹn hết hạn — luật `cua_so`,
/// fail-closed. Đây là ca đã trả giá thật: acc1 ghi `92%` kèm một `resets_at`
/// cũ hơn cả lúc đọc (đo 2026-08-30).
#[test]
fn moc_da_qua_thi_khong_in_gi() {
    assert_eq!(sap_reset(&q(61, -10, 16, -5), BAY_GIO), None);
    // …và một cửa sổ đã qua KHÔNG được kéo theo cửa sổ kia: tuần còn hạn, 5
    // tiếng đã quay vòng ⟹ chưa đọc đủ hai cửa sổ ⟹ im.
    assert_eq!(sap_reset(&q(61, 4 * 1440, 16, -5), BAY_GIO), None);
}

/// Mốc còn DƯỚI một phút vẫn phải nói — đây là phút đáng nói nhất.
///
/// 🔴 Bài này sinh ra từ tầng đối chứng ngược 2026-09-16: mutant *"bỏ cửa `..=0
/// => return None`"* ra **XANH**, tức không bài kiểm nào chạm tới dòng ấy. Đọc
/// lại thì dòng ấy còn sai hướng — `cua_so` đã loại mốc quá khứ, nên tới được
/// đó nghĩa là mốc còn ở phía trước và phép chia phút vừa làm tròn nó về 0. Một
/// tài khoản sắp mở lại mà hàng của nó im bặt là đúng thứ luật này cấm.
#[test]
fn moc_sap_toi_trong_vong_mot_phut_van_phai_noi() {
    let mut x = q(61, 4 * 1440, 16, 200);
    // 30 giây nữa: `cua_so` cho qua (mốc còn ở tương lai), phép chia ra 0 phút.
    x.week_resets_at = Some(
        chrono::DateTime::from_timestamp_millis(BAY_GIO + 30_000)
            .expect("mốc hợp lệ")
            .to_rfc3339(),
    );
    let s = sap_reset(&x, BAY_GIO).expect("mốc còn ở phía trước thì phải nói");
    assert!(s.contains("còn dưới 1 phút"), "{s}");
}

/// Không đọc được cửa sổ nào thì KHÔNG bịa ra một cái hẹn.
#[test]
fn thieu_so_thi_im() {
    let mut x = q(61, 4 * 1440, 16, 200);
    x.hour5_pct = None;
    assert_eq!(
        sap_reset(&x, BAY_GIO),
        None,
        "chưa đọc đủ hai cửa sổ thì không biết cái nào chật nhất — im, đừng đoán"
    );
}

/// Câu đi ra màn (`Quota::say`) phải MANG được nó — nếu không thì bản vá này
/// dừng lại ở một hàm không ai gọi, đúng con bug `errors_block` của kho này.
#[test]
fn cau_di_ra_man_phai_mang_gio_reset() {
    let s = q(61, 4 * 1440, 16, 200).say(BAY_GIO);
    assert!(s.contains("tuần 61%"), "{s}");
    assert!(s.contains("hạng:"), "{s}");
    assert!(
        s.contains("reset tuần "),
        "dòng `/accounts` vẫn thiếu đúng thứ Hà hỏi: {s}"
    );
}

/// Hàng ĐÃ KỊCH TRẦN giữ nguyên chữ `mở lại` — không được đổi thành `reset`.
///
/// Hai câu khác nhau: `mở lại` nói *"đang chặn, chờ tới lúc ấy"*, `reset` nói
/// *"còn dùng được, và đồng hồ quay vòng lúc ấy"*. Gộp một chữ là xoá mất phân
/// biệt mà chính dòng ấy sinh ra để giữ (15/09).
#[test]
fn hang_kich_tran_giu_nguyen_chu_mo_lai() {
    let s = q(100, 620, 0, 90).say(BAY_GIO);
    assert!(s.contains("mở lại "), "{s}");
    assert!(
        !s.contains("reset "),
        "hàng kịch trần không được mang thêm chữ `reset`: {s}"
    );
}
