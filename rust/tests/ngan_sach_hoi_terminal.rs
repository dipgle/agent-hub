//! Một vòng nền được phép chờ Terminal **tổng cộng** bao lâu, và nó phải nhường
//! đường cho ngón tay chủ máy.
//!
//! 🔴 Hà 2026-09-13: *"Tại sao thi thoảng bị treo chờ rất lâu lệnh mới phản hồi,
//! có phải bị nghẽn do phần cứng yếu hay cách chạy của huba có vấn đề, chạy đơn
//! luồng bị nghẽn đọc ghi?"*
//!
//! Đo trước khi trả lời, và cả ba giả thuyết đều KHÔNG phải:
//!
//! * **phần cứng**: p50 mỗi vòng ổn định ~6 giây suốt 14 ngày — máy yếu thì
//!   trung vị phải trôi;
//! * **đọc-ghi**: bước chạm đĩa (`sessions_snapshot_ms`) p50 **1–2,7 giây**, ổn
//!   định mọi ngày;
//! * **đơn luồng**: có phần, nhưng là hệ quả. Lệnh đã có đường vượt vòng từ
//!   12/08 (`run_telegram_now`, luồng riêng + hạng gấp).
//!
//! Thứ thật sự tốn: **chờ Terminal.app trả lời AppleScript**, và Terminal trả
//! lời TUẦN TỰ. Hiện trường một vòng 101,5 giây:
//!
//! ```text
//! 02:49:53  terminal_probe_failed              ← khe 14,9s
//! 02:49:53  sessions_snapshot_ms  ms=45311     ← bình thường 890ms
//! 02:50:14  terminal_probe_failed              ← khe 18,6s
//! 02:50:35  trust_tick_probe_failed            ← khe 20,1s
//! 02:50:49  cycle_done  ms=101463
//! ```
//!
//! Vòng > 30 giây theo ngày, kèm mẫu số: `0/131` (30/08) · `22/1081` (01/09) ·
//! **`70/857` (09/09)** · **`73/733` (10/09)** · `61/881` (12/09). Chậm nhất
//! **448,2 giây**.

use huba::exec::Lane;
use huba::keys::{
    core_probe, dang_la_loi, probe_verdict, timeout_context, ProbeVerdict, PROBE_BUDGET_MS,
    PROBE_YIELD_MS,
};

/// Mốc giả, ms epoch. Cố định — hàm thuần nhận `now` làm tham số đúng để bài
/// kiểm không tự đỏ theo đồng hồ.
const BAY_GIO: i64 = 1_789_264_800_000;

/// Một phép dò nền **tuỳ chọn**.
fn nen(da_tieu: u64, gap_luc: i64) -> ProbeVerdict {
    probe_verdict(
        Lane::Background,
        false,
        da_tieu,
        PROBE_BUDGET_MS,
        BAY_GIO,
        gap_luc,
        PROBE_YIELD_MS,
    )
}

/// Một phép dò nền **LÕI** — `terminal_screens`, thứ cả danh sách phiên dựng lên.
fn loi(da_tieu: u64, gap_luc: i64) -> ProbeVerdict {
    probe_verdict(
        Lane::Background,
        true,
        da_tieu,
        PROBE_BUDGET_MS,
        BAY_GIO,
        gap_luc,
        PROBE_YIELD_MS,
    )
}

/// Vòng còn ngân sách, không ai chờ ⟹ cứ hỏi. Đây là đường đi của **mọi vòng
/// bình thường** (p50 của cả bước ảnh chụp là 1–2,7s, thừa chỗ trong 10s).
#[test]
fn vong_binh_thuong_van_hoi_nhu_cu() {
    assert_eq!(nen(0, 0), ProbeVerdict::Hoi);
    assert_eq!(nen(2_700, 0), ProbeVerdict::Hoi, "ảnh chụp p90 vẫn lọt");
    assert_eq!(
        nen(PROBE_BUDGET_MS - 1, 0),
        ProbeVerdict::Hoi,
        "sát trần vẫn hỏi"
    );
}

/// 🔴 Cửa ①: hết ngân sách thì thôi hỏi — đây là cái biến một vòng 448 giây
/// thành một vòng ~10 giây kém thông tin.
#[test]
fn het_ngan_sach_thi_thoi_hoi() {
    assert_eq!(
        nen(PROBE_BUDGET_MS, 0),
        ProbeVerdict::HetNganSach {
            da_tieu_ms: PROBE_BUDGET_MS
        },
        "đúng trần đã là hết — `>=`, không phải `>`"
    );
    // Đúng hình dạng đã đo: một lượt ảnh chụp 45,3 giây đốt sạch ngân sách, mọi
    // phép dò sau nó trong cùng vòng bị bỏ.
    assert_eq!(
        nen(45_311, 0),
        ProbeVerdict::HetNganSach { da_tieu_ms: 45_311 }
    );
}

/// 🔴 Cửa ②: vừa có một lượt hỏi GẤP thì việc nền nhường — kể cả khi ngân sách
/// còn nguyên. Terminal chỉ trả lời một câu một lúc.
#[test]
fn nhuong_duong_cho_luot_hoi_gap() {
    let vua_xong = BAY_GIO - 500;
    assert_eq!(
        nen(0, vua_xong),
        ProbeVerdict::Nhuong {
            con_ms: PROBE_YIELD_MS - 500
        },
        "ngân sách còn nguyên vẫn phải nhường"
    );
}

/// Nhường ĐỨNG TRƯỚC ngân sách: một vòng còn dư ngân sách mà chen ngang ngón
/// tay chủ máy thì vẫn là chen ngang.
#[test]
fn nhuong_thang_ngan_sach_khi_ca_hai_cung_dung() {
    let vua_xong = BAY_GIO - 100;
    assert!(
        matches!(nen(999_999, vua_xong), ProbeVerdict::Nhuong { .. }),
        "cả hai cửa cùng đúng thì phải đọc ra `Nhuong`, không phải `HetNganSach`"
    );
}

/// Cửa sổ nhường HẾT thì hỏi lại — không được nhường vĩnh viễn.
#[test]
fn het_cua_so_nhuong_thi_hoi_lai() {
    let het = BAY_GIO - PROBE_YIELD_MS;
    assert_eq!(nen(0, het), ProbeVerdict::Hoi, "đúng mốc hết là hết");
    assert_eq!(
        nen(0, het - 1),
        ProbeVerdict::Hoi,
        "quá mốc thì càng phải hỏi"
    );
}

/// Mốc `0` = *"chưa có lượt gấp nào từ lúc khởi động"*, và nó KHÔNG được đọc
/// thành một cửa sổ nhường đang mở.
///
/// 🪦 Bài này từng được viết như một cổng cho một chốt `gap_luc_ms > 0` riêng.
/// **Đối chứng ngược cho thấy chốt ấy là mutant TƯƠNG ĐƯƠNG** (`RED_G5=0`,
/// 13/09) nên chốt đã bị GỠ khỏi mã. Bài ở lại, và nó vẫn nói một điều thật —
/// chỉ là điều ấy đến từ SỐ HỌC chứ không từ một nhánh: `con = 0 + 2000 -
/// 1_789_264_800_000` âm sâu, nên không có cửa sổ nào để mở. Giữ lại vì nếu ai
/// đó đổi `bay_gio_ms` sang một thang đo khác (giây thay vì mili-giây, hay một
/// đồng hồ đơn điệu bắt đầu từ 0) thì phép trừ ấy đổi dấu và bài này đỏ.
#[test]
fn moc_chua_dung_khong_mo_cua_so_nhuong() {
    assert_eq!(nen(0, 0), ProbeVerdict::Hoi);
    assert_eq!(nen(0, -1), ProbeVerdict::Hoi);
}

/// 🔴 Hạng GẤP không bao giờ bị chặn — người đang nhìn màn hình chờ câu trả lời
/// thì không có ngân sách nào đáng hơn. Đây là vế giữ cho bản vá này không biến
/// thành một cái bẫy mới: cắt nhầm ở đây là `/shot` của chủ máy im lặng hỏng.
#[test]
fn hang_gap_di_thang_qua_ca_hai_cua() {
    for (tieu, gap) in [
        (0, 0),
        (999_999, 0),
        (0, BAY_GIO - 1),
        (999_999, BAY_GIO - 1),
    ] {
        assert_eq!(
            probe_verdict(
                Lane::Urgent,
                false,
                tieu,
                PROBE_BUDGET_MS,
                BAY_GIO,
                gap,
                PROBE_YIELD_MS
            ),
            ProbeVerdict::Hoi,
            "hạng gấp bị chặn ở (tiêu={tieu}, gấp lúc={gap})"
        );
    }
}

/// 🔴 Cửa ③: `-1.0` là **không đọc được**, phải in ra `?`. Một con số âm trên
/// dòng chẩn đoán là một phép đo hỏng đội lốt một phép đo.
#[test]
fn khong_doc_duoc_thi_in_dau_hoi_khong_in_so_am() {
    let t = timeout_context(-1.0, -1.0, -1.0, 3, 340);
    assert!(t.contains("Terminal ?"), "{t}");
    assert!(t.contains("WindowServer ?"), "{t}");
    assert!(t.contains("load ?"), "{t}");
    assert!(!t.contains("-1"), "số âm lọt ra dòng chẩn đoán:\n{t}");

    // ĐỐI CHỨNG NGƯỢC trong bài: đo được thì phải ra SỐ, không thì bài trên
    // xanh nhờ hàm chẳng bao giờ in số nào.
    let d = timeout_context(39.1, 33.5, 8.64, 2, 45_311);
    assert!(d.contains("Terminal 39%"), "{d}");
    assert!(d.contains("WindowServer 34%"), "{d}");
    assert!(d.contains("load 9"), "{d}");
    assert!(
        d.contains("45.3s"),
        "lượt đọc trót lọt gần nhất phải hiện:\n{d}"
    );
}

/// Số thứ tự lượt hết giờ TRONG VÒNG phải có mặt: một lượt hết giờ đơn lẻ khác
/// hẳn lượt thứ tư liên tiếp, và hai thứ ấy dẫn tới hai kết luận khác nhau.
#[test]
fn dem_luot_het_gio_trong_vong() {
    let t = timeout_context(10.0, 5.0, 1.0, 4, 300);
    assert!(t.contains("lượt thứ 4"), "{t}");
}

/// 🔴 CỔNG: ngân sách phải được MỞ LẠI ở đầu mỗi vòng. Không có bài kiểm nào
/// gọi được `run_once` (nó cần một DB, một Telegram, một Terminal), nên cổng
/// đọc-mã là cổng duy nhất với tới được — và thiếu lượt mở lại thì huba chỉ
/// chạy đủ phép dò trong **một** vòng đầu tiên rồi câm vĩnh viễn, một hỏng
/// không bài kiểm thuần nào thấy.
#[test]
fn ngan_sach_phai_duoc_mo_lai_dau_moi_vong() {
    const NGUON: &str = include_str!("../src/pipeline.rs");
    let vong = NGUON
        .find("pub fn run_once(db: &Db, cfg: &Config)")
        .expect("không thấy `run_once` — cổng này mù rồi, sửa mỏ neo");
    let mo = NGUON[vong..]
        .find("probe_budget_reset()")
        .expect("không thấy lượt mở lại ngân sách trong `run_once`");
    assert!(
        mo < 800,
        "lượt mở lại phải nằm ở ĐẦU vòng (cách {mo} ký tự) — muộn hơn thì những \
         phép dò chạy trước nó tiêu vào ngân sách của vòng TRƯỚC"
    );
}

/// 🔴 CỔNG: lệnh của chủ máy phải chạy ở hạng GẤP dù tới bằng cửa nào.
///
/// `execute_telegram_commands` chạy ở hai chỗ — đầu mỗi vòng (luồng nền) và
/// ngay lúc bấm. Thiếu lượt nâng hạng ở NGUỒN thì đường thứ nhất là hạng nền,
/// tức từ 13/09 nó bị chính ngân sách này chặn: lệnh gõ đúng lúc Terminal câm
/// sẽ im lặng không đọc được màn.
#[test]
fn lenh_cua_chu_may_luon_o_hang_gap() {
    const NGUON: &str = include_str!("../src/pipeline.rs");
    let ham = NGUON
        .find("fn execute_telegram_commands(db: &Db, cfg: &Config) {")
        .expect("không thấy `execute_telegram_commands` — sửa mỏ neo");
    let nang = NGUON[ham..]
        .find("crate::exec::urgent()")
        .expect("`execute_telegram_commands` không nâng hạng gấp");
    assert!(
        nang < 900,
        "lượt nâng hạng phải nằm ngay đầu hàm (cách {nang} ký tự)"
    );
}

// ───────────── phép dò LÕI: miễn cửa nhường, KHÔNG miễn ngân sách ────────────

/// 🔴 Đo được trên bản vừa cài lúc 09:11–09:15, và nó ngược hẳn ý định: trong
/// số lượt bị bỏ, **32 là NHƯỜNG · 23 là hết ngân sách**, và lượt nhường trúng
/// đúng `terminal_screens` — phép dò dựng nên CẢ danh sách phiên:
///
/// ```text
/// "err":    "nhường Terminal cho một lượt hỏi đang có người chờ (còn 947ms)"
/// "msg":    "terminal_probe_failed"
/// "effect": "cửa sổ rảnh không lên danh sách · mọi phiên tạm coi là không gõ
///            vào được · không đọc được dòng đang-làm-gì"
/// ```
///
/// Tức nó làm hỏng đúng cái màn Hà đang nhìn, đúng lúc Hà đang bấm. Nhường một
/// phép dò tuỳ chọn là mất một dòng chi tiết; nhường phép dò lõi là trả về một
/// danh sách SAI.
#[test]
fn phep_do_loi_khong_nhuong_duong() {
    let vua_xong = BAY_GIO - 500;
    assert!(
        matches!(nen(0, vua_xong), ProbeVerdict::Nhuong { .. }),
        "đối chứng: phép dò TUỲ CHỌN thì vẫn nhường"
    );
    assert_eq!(
        loi(0, vua_xong),
        ProbeVerdict::Hoi,
        "phép dò LÕI phải đi thẳng — nhường nó là trả về một danh sách phiên sai"
    );
}

/// Nhưng lõi KHÔNG được miễn ngân sách: một vòng mà riêng ảnh chụp đã đốt 45
/// giây thì vẫn phải nghỉ, không thì cửa ① mất tác dụng ở đúng vòng tệ nhất.
#[test]
fn phep_do_loi_van_bi_ngan_sach_chan() {
    assert_eq!(loi(0, 0), ProbeVerdict::Hoi, "còn ngân sách thì cứ hỏi");
    assert_eq!(
        loi(45_311, 0),
        ProbeVerdict::HetNganSach { da_tieu_ms: 45_311 },
        "lõi cũng phải nghỉ khi vòng đã tiêu hết"
    );
}

/// 🔴 CỔNG: `terminal_screens` phải được đánh dấu là LÕI tại chỗ gọi. Không bài
/// kiểm nào gọi được nó (nó cần một Terminal thật), nên cổng đọc-mã là cổng duy
/// nhất với tới — và thiếu dấu ấy thì lỗi 09:11 quay lại nguyên vẹn, im lặng.
#[test]
fn anh_chup_phien_phai_duoc_danh_dau_la_loi() {
    const NGUON: &str = include_str!("../src/sessions.rs");
    let goi = NGUON
        .find("crate::keys::terminal_screens()")
        .expect("không thấy `terminal_screens()` — cổng này mù rồi, sửa mỏ neo");
    let truoc = &NGUON[..goi];
    let dau = truoc
        .rfind("crate::keys::core_probe()")
        .expect("`terminal_screens` không được đánh dấu là phép dò lõi");
    assert!(
        goi - dau < 400,
        "dấu lõi phải đứng NGAY TRƯỚC lượt gọi (cách {} ký tự)",
        goi - dau
    );
}

/// 🔴 Đo được 13/09 `09:34–09:53`: cấy `CoreGuard::drop` đặt `true` thay vì trả
/// cờ về ⇒ **`RED_L6 = 0`** — không bài nào trong 14 bài đỏ. Mutant ấy im lặng
/// vì nó chỉ hỏng KỂ TỪ lượt lõi thứ nhất: sau đó mọi phép dò tuỳ chọn đều đội
/// lốt lõi, cửa nhường không chặn ai nữa, còn dòng `probe_budget_spent` vẫn in
/// ra một con số trông bình thường. Nên cổng phải hỏi cả ba vế: ĐẶT · LỒNG ·
/// TRẢ VỀ.
#[test]
fn dau_loi_tra_co_ve_khi_roi_tam() {
    assert!(!dang_la_loi(), "mặc định: một luồng không phải phép dò lõi");
    {
        let _ngoai = core_probe();
        assert!(dang_la_loi(), "trong tầm guard thì là lõi");
        {
            let _trong = core_probe();
            assert!(dang_la_loi(), "guard lồng vẫn là lõi");
        }
        assert!(
            dang_la_loi(),
            "guard LỒNG rời tầm không được tắt cờ của guard đang bao ngoài"
        );
    }
    assert!(
        !dang_la_loi(),
        "rời tầm phải trả cờ về — không thì mọi phép dò sau đều đội lốt lõi"
    );
}

/// 🔴 CỔNG: lượt gọi thật trong `osascript` phải ĐỌC cờ, không truyền hằng số.
/// Đo 13/09: cấy `false` vào đúng chỗ ấy ⇒ **`RED_L5 = 0`** — cả tính năng chết
/// mà 14 bài vẫn xanh, vì `osascript` đòi một Terminal thật nên không bài kiểm
/// nào gọi tới. Cùng họ `feedback_parity_gate_blind_to_call_sites`: một khối mã
/// đúng KHÔNG nói rằng người gọi truyền đúng.
///
/// Cổng này và [`dau_loi_tra_co_ve_khi_roi_tam`] bọc nhau: truyền `dang_la_loi()`
/// mà hàm ấy luôn trả `false` thì bài kia đỏ.
#[test]
fn cho_goi_that_phai_doc_co_loi() {
    const NGUON: &str = include_str!("../src/keys.rs");
    let goi = NGUON.find("match probe_verdict(").expect(
        "không thấy lượt gọi `probe_verdict` trong `osascript` — cổng này mù rồi, sửa mỏ neo",
    );
    let than = &NGUON[goi..(goi + 300).min(NGUON.len())];
    assert!(
        than.contains("dang_la_loi()"),
        "lượt gọi thật phải truyền `dang_la_loi()`, không phải một hằng số:\n{than}"
    );
}
