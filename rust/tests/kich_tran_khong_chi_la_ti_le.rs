//! **Kịch trần không được quyết bằng mỗi tỉ lệ** — mốc reset là điều kiện thứ hai.
//!
//! 🔴 Hà 2026-09-15: *"thêm vào phần kịch trần thời gian reset để thêm điều kiện
//! kiểm tra chứ chỉ dựa vào tỉ lệ là không đủ"* · *"vì tỉ lệ là số chết chưa
//! chắc đúng"*.
//!
//! Con số trong `.claude.json` đông cứng ở `fetchedAtMs`, và đo hai chiều cùng
//! ngày cho thấy nó **chỉ do phiên tương tác ghi**: `claude -p` chạy trót lọt
//! (exit 0, 38–41 giây), `mtime` của tệp ĐỔI, mà `cachedUsageUtilization` không
//! nhúc nhích — ở CẢ tài khoản đã có số (acc3, 3%) LẪN tài khoản chưa có (acc5).
//! Nên bản đọc nằm im được vô thời hạn.
//!
//! `quota::cua_so` mới hỏi được một nửa câu: *mốc mở lại đã qua chưa*. Ca thật
//! đo trên máy lúc 2026-09-15T11:17:30Z, và nó là ca lọt qua đúng nửa còn lại:
//!
//! ```text
//! acc2  fetched 09-14T07:32Z (già 27,8h)  seven_day 100%  resets 09-16T11:00Z (còn 23,7h)  ⟹ Full
//! acc4  fetched 09-15T08:44Z (già  2,5h)  seven_day 100%  resets 09-15T15:00Z (còn  3,7h)  ⟹ Full
//! ```
//!
//! Hai hàng cùng ra `Full` bằng cùng một con số `100`, mà một hàng tựa vào bản
//! đọc già hơn cả quãng còn lại của chính cửa sổ nó đang mô tả. Thước Hà chọn:
//! **cũ ⟺ tuổi > thời gian còn lại tới reset** — tự co giãn theo vị trí trong
//! cửa sổ, nên một ngưỡng phút cứng không thay được.
//!
//! Và điều kiện này **KHÔNG hạ hạng** (Hà chốt: dò sống trước, hết cách mới giữ
//! `Full`). Bài `khong_ha_hang_*` dưới đây khoá đúng chỗ ấy: hạ xuống `Unknown`
//! là mở một cửa sổ chết, vì `Unknown` đứng TRƯỚC `Full`.

use huba::quota::{kich_tran_can_xac_nhan, mo_lai_luc, rank, Quota, Rank};

/// Mốc đo thật của lượt dựng bài này.
const BAY_GIO: &str = "2026-09-15T11:17:30Z";

fn ms(s: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(s)
        .expect("mốc trong bài kiểm phải parse được")
        .timestamp_millis()
}

/// Một bản đọc trần — mọi trường đều khai rõ, cố ý không có `..Default`: thêm
/// trường mới thì compiler phải bắt được từng chỗ dựng mà hỏi lại.
fn doc(
    fetched: Option<&str>,
    week: Option<i64>,
    week_resets: Option<&str>,
    hour5: Option<i64>,
    hour5_resets: Option<&str>,
) -> Quota {
    Quota {
        account: "acc-thu".to_string(),
        week_pct: week,
        week_resets_at: week_resets.map(str::to_string),
        hour5_pct: hour5,
        hour5_resets_at: hour5_resets.map(str::to_string),
        fetched_at_ms: fetched.map(ms),
        why_unknown: None,
        chua_dung_duoc: None,
        models: Vec::new(),
        model_use: None,
    }
}

#[test]
fn ban_doc_gia_hon_quang_con_lai_thi_phai_doi_xac_nhan() {
    // acc2, số thật đo 15/09.
    let q = doc(
        Some("2026-09-14T07:32:06Z"),
        Some(100),
        Some("2026-09-16T11:00:00Z"),
        None,
        None,
    );
    let canh = kich_tran_can_xac_nhan(&q, ms(BAY_GIO));
    let canh = canh.expect("tuổi 27,8h > còn lại 23,7h ⟹ phải đòi xác nhận");
    assert!(canh.contains("tuần"), "phải nói đúng cửa sổ nào: {canh}");
    assert!(canh.contains("27"), "phải nói tuổi bằng SỐ: {canh}");
}

#[test]
fn ban_doc_tuoi_hon_quang_con_lai_thi_im() {
    // acc4, số thật cùng lượt đo: cùng `100`, khác mỗi tuổi.
    let q = doc(
        Some("2026-09-15T08:44:45Z"),
        Some(100),
        Some("2026-09-15T15:00:00Z"),
        None,
        None,
    );
    assert_eq!(
        kich_tran_can_xac_nhan(&q, ms(BAY_GIO)),
        None,
        "tuổi 2,5h < còn lại 3,7h ⟹ bản đọc còn tươi, đừng kêu"
    );
}

/// Hai chiều trên CÙNG một tỉ lệ — đây là bài chứng minh điều kiện mới thật sự
/// là điều kiện thứ hai, chứ không phải một cách viết khác của `pct >= 100`.
#[test]
fn cung_mot_ti_le_100_hai_ket_luan_khac_nhau() {
    let cu = doc(
        Some("2026-09-14T07:32:06Z"),
        Some(100),
        Some("2026-09-16T11:00:00Z"),
        None,
        None,
    );
    let moi = doc(
        Some("2026-09-15T08:44:45Z"),
        Some(100),
        Some("2026-09-15T15:00:00Z"),
        None,
        None,
    );
    assert_eq!(cu.week_pct, moi.week_pct, "tiền đề: cùng một tỉ lệ");
    assert!(kich_tran_can_xac_nhan(&cu, ms(BAY_GIO)).is_some());
    assert!(kich_tran_can_xac_nhan(&moi, ms(BAY_GIO)).is_none());
}

#[test]
fn khong_ha_hang_du_ban_doc_da_cu() {
    let q = doc(
        Some("2026-09-14T07:32:06Z"),
        Some(100),
        Some("2026-09-16T11:00:00Z"),
        None,
        None,
    );
    assert!(kich_tran_can_xac_nhan(&q, ms(BAY_GIO)).is_some());
    assert_eq!(
        rank(&q, ms(BAY_GIO)),
        Rank::Full,
        "cảnh báo KHÔNG được hạ hạng: Unknown đứng trước Full ⟹ huba sẽ mở một cửa sổ chết"
    );
}

#[test]
fn chua_kich_tran_thi_khong_canh_bao_du_ban_doc_rat_cu() {
    // acc3 thật: 7% tuần / 28% năm tiếng, nhưng bản đọc cố tình để rất cũ.
    let q = doc(
        Some("2026-09-01T00:00:00Z"),
        Some(7),
        Some("2026-09-22T06:00:00Z"),
        Some(28),
        Some("2026-09-15T13:30:00Z"),
    );
    assert_ne!(rank(&q, ms(BAY_GIO)), Rank::Full, "tiền đề: chưa kịch trần");
    assert_eq!(
        kich_tran_can_xac_nhan(&q, ms(BAY_GIO)),
        None,
        "câu này chỉ nói về verdict KỊCH TRẦN, không phải một cái đồng hồ đo tuổi chung"
    );
}

#[test]
fn khong_do_duoc_tuoi_thi_fail_closed() {
    let q = doc(None, Some(100), Some("2026-09-16T11:00:00Z"), None, None);
    let canh = kich_tran_can_xac_nhan(&q, ms(BAY_GIO))
        .expect("không có fetchedAtMs ⟹ không đo được tuổi ⟹ phải đòi xác nhận");
    assert!(
        canh.contains("không rõ tuổi"),
        "phải phân biệt 'không đo được tuổi' với 'đo được và nó cũ': {canh}"
    );
}

/// Chỉ cửa sổ LÀM NÊN verdict mới bị hỏi tuổi. Một bản đọc có thể tươi với cửa
/// sổ tuần và cũ với cửa sổ 5 tiếng cùng lúc, nên lấy nhầm cửa sổ là cảnh báo
/// sai chỗ — và nó sẽ chỉ chủ máy đi xác nhận một con số không chặn ai.
#[test]
fn hoi_tuoi_dung_cai_cua_so_dang_chan() {
    let q = doc(
        Some("2026-09-15T10:00:00Z"), // già 1,3h
        Some(50),
        Some("2026-09-22T06:00:00Z"), // tuần: còn 163h ⟹ tươi chán
        Some(100),
        Some("2026-09-15T12:00:00Z"), // 5 tiếng: còn 0,7h ⟹ 1,3h > 0,7h ⟹ CŨ
    );
    let canh = kich_tran_can_xac_nhan(&q, ms(BAY_GIO)).expect("cửa sổ 5 tiếng đang chặn và đã cũ");
    assert!(
        canh.contains("5 tiếng"),
        "phải gọi tên cửa sổ đang chặn, không phải cửa sổ còn rộng: {canh}"
    );
}

#[test]
fn kich_tran_phai_noi_mo_lai_luc_nao() {
    let q = doc(
        Some("2026-09-15T08:44:45Z"),
        Some(100),
        Some("2026-09-15T15:00:00Z"),
        None,
        None,
    );
    let m = mo_lai_luc(&q, ms(BAY_GIO)).expect("kịch trần thì phải có mốc mở lại");
    let mong = chrono::DateTime::parse_from_rfc3339("2026-09-15T15:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Local)
        .format("%H:%M %d/%m")
        .to_string();
    assert_eq!(m, mong, "mốc phải in theo giờ ĐỊA PHƯƠNG — CLI in giờ máy");
}

#[test]
fn chua_kich_tran_thi_khong_co_moc_mo_lai() {
    let q = doc(
        Some("2026-09-15T08:44:45Z"),
        Some(7),
        Some("2026-09-22T06:00:00Z"),
        Some(28),
        Some("2026-09-15T13:30:00Z"),
    );
    assert_eq!(
        mo_lai_luc(&q, ms(BAY_GIO)),
        None,
        "không cửa sổ nào chặn thì không có gì để 'mở lại'"
    );
}

/// Hai cửa sổ cùng kịch trần ⟹ lấy mốc SỚM NHẤT: đó mới là lúc tài khoản dùng
/// lại được.
#[test]
fn hai_cua_so_cung_chan_thi_lay_moc_som_nhat() {
    let q = doc(
        Some("2026-09-15T11:00:00Z"),
        Some(100),
        Some("2026-09-20T08:00:00Z"),
        Some(100),
        Some("2026-09-15T13:30:00Z"),
    );
    let m = mo_lai_luc(&q, ms(BAY_GIO)).unwrap();
    let som = chrono::DateTime::parse_from_rfc3339("2026-09-15T13:30:00Z")
        .unwrap()
        .with_timezone(&chrono::Local)
        .format("%H:%M %d/%m")
        .to_string();
    assert_eq!(m, som);
}

/// Dòng cho người đọc phải MANG cả hai thứ mới: mốc mở lại, và lời cảnh báo khi
/// verdict tựa vào số cũ. `/accounts` là chỗ duy nhất chủ máy soi lại được luật
/// chọn tài khoản, nên thiếu một trong hai là soi hụt.
///
/// 🔴 **ĐỔI CHỮ, GIỮ Ý — 2026-09-16.** Bài này khoá ý ấy bằng chuỗi `"mở lại"`,
/// một TRƯỜNG RIÊNG ở cuối dòng. Trường ấy đã bỏ khi mỗi cửa sổ hạn mức bắt đầu
/// mang đồng hồ của chính nó (Hà: *"Vẫn thiếu giờ reset của phiên"*), nên bài
/// kiểm đỏ — đúng việc của nó.
///
/// Ý thì KHÔNG mất, và dạng mới còn đúng hơn ở đúng chỗ dạng cũ sai: `mở lại`
/// gộp cả hai cửa sổ thành MỘT mốc, lấy mốc SỚM NHẤT trong các cửa sổ đã ≥100%.
/// Khi cả hai cùng chặn (tuần tới 20/09, 5 tiếng tới 13:30 hôm nay) thì "sớm
/// nhất" = 13:30 — trong khi tài khoản vẫn bị cửa sổ tuần chặn thêm bốn ngày.
/// Một đồng hồ đi kèm TỪNG con số không có chỗ cho sự mơ hồ ấy.
/// ⚠ Cái bug min/max ấy vẫn còn trong [`huba::quota::mo_lai_luc`] (nay chỉ còn
/// đi vào log, không còn lên màn) — chưa sửa, đã báo chủ máy.
#[test]
fn dong_cho_nguoi_doc_mang_ca_moc_lan_canh_bao() {
    let q = doc(
        Some("2026-09-14T07:32:06Z"),
        Some(100),
        Some("2026-09-16T11:00:00Z"),
        None,
        None,
    );
    let s = q.say(ms(BAY_GIO));
    assert!(s.contains("ĐÃ KỊCH TRẦN"), "{s}");
    assert!(
        s.contains("tuần 100% ↻ "),
        "hàng kịch trần vẫn phải nói ĐÓNG TỚI BAO GIỜ, nay bằng đồng hồ nằm cạnh \
         chính con số 100% đang chặn: {s}"
    );
    assert!(s.contains("cần xác nhận"), "thiếu cảnh báo số cũ: {s}");
}
