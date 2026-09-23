//! Một khối `/accounts` không được tự cãi nhau.
//!
//! 🔴 Hà 2026-09-23, ảnh `/accounts` khối `acc1`: *"Sao các thông số này lại mâu
//! thuẫn thế"*. Ba chỗ trong cùng khối ấy, mỗi chỗ đúng về số mà đọc thành sai:
//!
//! 1. `tuần 32% … Fable 0%` rồi ngay dưới `model dùng (…): Opus 5 100%` — cùng
//!    hình dạng "tên N%", mà dòng trên là % TRẦN, dòng dưới là % TOKEN của MỘT
//!    phiên. Đọc ra "Opus đã dùng hết 100%" trong khi tuần mới 32%.
//! 2. Phiên ấy `mở 187 tiếng trước` — hơn 7 ngày, tức mở TRƯỚC cả cửa sổ tuần
//!    đang tính 32%, nên nó không nói gì về con số tuần.
//! 3. `↻ 14:59 27/09 (còn 3 ngày)` đọc lúc 16:1x ngày 23/09 — còn 3 ngày 22
//!    tiếng, phép chia nguyên cắt thành 3, người đọc đếm lịch ra 4.
//!
//! (Chỗ thứ tư — `không có phiên nào` cạnh `phiên gần nhất` — sửa ở
//! `runtime.rs`, thành `không có phiên nào đang chạy`.)
//!
//! `now_ms` là THAM SỐ ở mọi bài dưới đây, không bài nào hỏi đồng hồ thật.

use huba::quota::{moc_cua_so, ModelUse, ModelUseSnapshot};

/// 2026-09-23T09:15:00Z — 16:15 giờ máy, lúc Hà gửi ảnh.
const BAY_GIO: i64 = 1_790_154_900_000;

fn luot_chup(tuoi_phut: i64) -> ModelUseSnapshot {
    ModelUseSnapshot {
        project: "/Users/hanguyen/projects/dwork/dev".into(),
        at_ms: Some(BAY_GIO - tuoi_phut * 60_000),
        rows: vec![
            ModelUse {
                name: "Opus 5".into(),
                tokens: 1_000,
                pct: 100,
            },
            ModelUse {
                name: "Haiku 4.5".into(),
                tokens: 1,
                pct: 0,
            },
        ],
    }
}

/// ① Dòng tỉ trọng phải TỰ NÓI nó là họ nào — không mang hình dạng một dòng hạn
/// mức. Neo vào nhãn họ số, không neo vào cả câu.
#[test]
fn dong_ti_trong_token_tu_noi_ho_so_cua_no() {
    let s = luot_chup(3 * 60).say(BAY_GIO);
    assert!(s.contains("tỉ trọng token"), "thiếu nhãn họ số: {s}");
    assert!(
        !s.contains("model dùng"),
        "nhãn cũ đọc như một dòng hạn mức: {s}"
    );
    assert!(s.contains("phiên gần nhất"), "thiếu PHẠM VI: {s}");
    assert!(s.contains("Opus 5 100%"), "{s}");
}

/// ② Cũ hơn cửa sổ tuần ⟹ nói ra. Và chiều ngược: còn trong tuần thì KHÔNG nói
/// — một cảnh báo in ở mọi dòng là một cảnh báo không ai đọc.
#[test]
fn luot_chup_cu_hon_cua_so_tuan_thi_noi_ra() {
    let cu = luot_chup(187 * 60).say(BAY_GIO);
    assert!(cu.contains("NGOÀI cửa sổ tuần"), "{cu}");
    assert!(
        cu.contains("mở 7 ngày trước"),
        "187 tiếng phải đọc ra ngày: {cu}"
    );

    for (tuoi, cho) in [
        (3 * 60, "mở 3 tiếng trước"),
        (47 * 60, "mở 47 tiếng trước"),
        (3 * 1440, "mở 3 ngày trước"),
        (7 * 1440, "mở 7 ngày trước"),
    ] {
        let s = luot_chup(tuoi).say(BAY_GIO);
        assert!(s.contains(cho), "{tuoi} phút: {s}");
        assert!(!s.contains("NGOÀI"), "còn trong tuần mà bị gắn NGOÀI: {s}");
    }
}

fn moc(them_phut: i64) -> String {
    chrono::DateTime::from_timestamp_millis(BAY_GIO + them_phut * 60_000)
        .expect("mốc hợp lệ")
        .to_rfc3339()
}

/// ③ Số ngày còn lại không được cắt mất gần một ngày.
#[test]
fn con_lai_nhieu_ngay_thi_kem_tieng_le() {
    // Đúng ca trong ảnh: 3 ngày 22 tiếng 44 phút.
    let s = moc_cua_so(
        Some(32),
        Some(&moc(3 * 1440 + 22 * 60 + 44)),
        BAY_GIO,
        false,
    )
    .expect("mốc còn ở phía trước");
    assert!(s.contains("(còn 3 ngày 22 tiếng)"), "{s}");

    // Tròn ngày thì không in `0 tiếng`.
    let s = moc_cua_so(Some(32), Some(&moc(4 * 1440)), BAY_GIO, false).expect("mốc");
    assert!(s.contains("(còn 4 ngày)"), "{s}");
}
