//! Hạn mức TUẦN cũng in ĐỒNG HỒ — và huba từng đọc nó thành "mốc đã qua".
//!
//! 🔴 Ca thật 19/10 (phiên `be02cd68` [dwork/review], acc3; Hà duyệt chuyển sang
//! huba lúc 23:1x): 19:25:47 CLI in `You've hit your weekly limit · resets 1pm
//! (Asia/Saigon)`; 19:26→19:41 huba ghi `auto_limit_held` 13 lần, cả 13
//! `ResetAlreadyPassed(1039)`; cùng giây ấy sổ hạn mức của chính huba ghi acc3
//! tuần 100 %, mở lại 2026-10-20T06:00Z (còn đúng 1039 phút), acc7 tuần 1 % ⟹ có
//! đích. Phiên đứng chết tới khi Hà tự bấm ❌ Đóng.
//!
//! RED đo trước lượt vá, trên API cũ (`.tmp/va-tuan-1910/zz_red_tuan.rs`):
//! `auto_limit_why(…, Some(1039), 30)` ra `ResetAlreadyPassed(1039)` — trùng log
//! thật — và `limit_still_biting(…, 19:41)` ra `false`. TEST_EXIT=101.
//!
//! Gốc: trần 360 phút (đo cho cửa sổ 5 giờ) áp lên mọi dòng dạng đồng hồ, dựa
//! trên tiền đề *"hạn mức tuần thì in NGÀY"*. Mã CLI 2.1.28x (`function Pc`) nói
//! khác: in đồng hồ cho MỌI loại khi mốc còn ≤ 24 giờ lúc in.
//!
//! Tệp này khoá cả ba tầng của bản vá, và mỗi tầng có đối chứng ngược (§13①):
//! nguồn mốc (`quotaLimits.resetsAt` · dấu giờ bản ghi · loại hạn mức) → đọc
//! mốc (`read_reset`) → quyết (`auto_limit_why`, `limit_still_biting`).

use huba::keys::{limit_line_on_screen, limit_window_of, LimitWindow};
use huba::pipeline::{
    auto_limit_why, limit_still_biting, read_reset, reset_left_min, LimitWhy, ResetRead,
};
use huba::sessions::{settle_limit_at, transcript_error_at, JournalError};

/// Nguyên văn bản ghi CLI 2.1.280 trong nhật ký `be02cd68…jsonl` (bỏ các trường
/// không ai đọc: uuid, cwd, requestId…). `resetsAt` 1792476000 = 2026-10-20T06:00Z.
const BAN_GHI_THAT: &str = r#"{"type":"assistant","timestamp":"2026-10-19T12:25:47.812Z","isApiErrorMessage":true,"error":"rate_limit","apiErrorStatus":429,"quotaLimits":{"status":"rejected","resetsAt":1792476000,"rateLimitType":"seven_day"},"message":{"role":"assistant","content":[{"type":"text","text":"You've hit your weekly limit · resets 1pm (Asia/Saigon)"}],"model":"<synthetic>"}}"#;

/// Cùng bản ghi, hình dạng CLI cũ (2.1.228): không có `quotaLimits`.
const BAN_GHI_CU: &str = r#"{"type":"assistant","timestamp":"2026-10-19T12:25:47.812Z","isApiErrorMessage":true,"message":{"role":"assistant","content":[{"type":"text","text":"You've hit your weekly limit · resets 1pm (Asia/Saigon)"}]}}"#;

const MAN_TUAN: &str = "\
⏺ Đang chạy QG lượt cuối.

You've hit your weekly limit · resets 1pm (Asia/Saigon)
";

const KHI: &str = "resets 1pm (Asia/Saigon)";
/// Giờ máy này: Asia/Saigon, UTC+7.
const SAI_GON: i64 = 420;
/// Đủ tuổi qua phanh dây chuyền — tuổi thật của phiên lúc ấy (log: 31281 s).
const TUOI: u64 = 31_281;
/// Lượt `auto_limit_held` cuối cùng: 12:41:39.920Z = 19:41 giờ máy.
const LUC_19_41: u64 = 19 * 60 + 41;

fn ms(rfc: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(rfc)
        .expect("mốc trong bài kiểm phải đúng RFC 3339")
        .timestamp_millis()
}

fn chot(ban_ghi: Option<&str>, man: Option<&str>, now: &str) -> huba::sessions::LimitSettled {
    settle_limit_at(
        ban_ghi.and_then(transcript_error_at),
        man.and_then(limit_line_on_screen),
        ms(now),
        SAI_GON,
    )
}

fn quyet(s: &huba::sessions::LimitSettled, now_min: u64) -> LimitWhy {
    let khi = s.limited.as_deref().expect("phải đọc ra dòng chặn");
    auto_limit_why(
        Some(khi),
        false,
        TUOI,
        true,
        Some("acc7"),
        read_reset(khi, s.window, s.left_min, now_min),
        30,
    )
}

// ───────────── ca thật: hạn mức tuần, mốc còn 1039 phút ⟹ CHUYỂN ─────────────

/// Đường chính trên CLI hiện tại: nhật ký mang `quotaLimits.resetsAt`.
#[test]
fn ca_that_19_10_doc_ra_con_1039_phut_va_phai_chuyen() {
    let nk = transcript_error_at(BAN_GHI_THAT).expect("bản ghi lỗi API");
    assert_eq!(nk.at_ms, Some(ms("2026-10-19T12:25:47.812Z")));
    assert_eq!(nk.resets_at_ms, Some(ms("2026-10-20T06:00:00Z")));

    let s = chot(
        Some(BAN_GHI_THAT),
        Some(MAN_TUAN),
        "2026-10-19T12:41:39.920Z",
    );
    assert_eq!(s.limited.as_deref(), Some(KHI));
    assert_eq!(s.window, LimitWindow::Longer);
    assert_eq!(
        s.left_min,
        Some(1039),
        "đúng số phút sổ hạn mức ghi cùng giây"
    );
    assert_eq!(s.error, None, "hết hạn mức thì không còn là LỖI");

    assert_eq!(
        read_reset(KHI, s.window, s.left_min, LUC_19_41),
        ResetRead::Ahead(1039)
    );
    assert_eq!(
        quyet(&s, LUC_19_41),
        LimitWhy::Do,
        "hạn mức tuần còn 17 tiếng mà không chuyển — đúng lỗi 19/10"
    );
    assert!(
        limit_still_biting(s.limited.as_deref(), s.window, s.left_min, LUC_19_41),
        "acc3 tuần 100 % mà bị coi là hết cắn ⟹ suggest_account đem nó làm đích"
    );
}

/// CLI cũ không có `quotaLimits` ⟹ neo bằng dấu giờ bản ghi + đồng hồ trên dòng.
/// Hai nguồn độc lập phải ra CÙNG một số — nếu lệch thì một trong hai đang sai.
#[test]
fn cli_cu_neo_bang_dau_gio_ra_cung_so() {
    let nk = transcript_error_at(BAN_GHI_CU).expect("bản ghi lỗi API");
    assert_eq!(nk.resets_at_ms, None);
    let s = chot(Some(BAN_GHI_CU), Some(MAN_TUAN), "2026-10-19T12:41:39.920Z");
    assert_eq!(s.left_min, Some(1039));
    assert_eq!(quyet(&s, LUC_19_41), LimitWhy::Do);
    // Mốc tuyệt đối mà phép neo dựng lại = đúng `resetsAt` của CLI mới.
    assert_eq!(
        reset_left_min(
            KHI,
            ms("2026-10-19T12:25:47.812Z"),
            ms("2026-10-20T06:00:00Z"),
            SAI_GON
        ),
        Some(0)
    );
}

/// Không có nhật ký nào mang dòng ấy (chỉ thấy trên MÀN) ⟹ lùi về loại hạn
/// mức: dòng `weekly` không được suy "đã qua" từ trần 5 giờ.
#[test]
fn chi_co_man_thi_lui_ve_loai_han_muc() {
    let s = chot(None, Some(MAN_TUAN), "2026-10-19T12:41:39.920Z");
    assert_eq!(s.window, LimitWindow::Longer);
    assert_eq!(s.left_min, None);
    assert_eq!(
        read_reset(KHI, s.window, None, LUC_19_41),
        ResetRead::Ahead(1039)
    );
    assert_eq!(quyet(&s, LUC_19_41), LimitWhy::Do);
}

// ───────────── đối chứng ngược: mốc ĐÃ qua thì KHÔNG chuyển ──────────────────

/// Vết banner nằm lại SAU giờ mở — đúng hình dạng 10/09 (`suggest_account`),
/// nay ở hạn mức tuần. Vá theo loại hạn mức một mình sẽ đọc nó là còn cắn mãi;
/// mốc neo thì biết nó đã qua 30 phút.
///
/// ⚠ Bài này KHÔNG tái hiện lỗi gốc, và cố ý: mã TRƯỚC lượt vá cũng ra "hết
/// cắn" ở đây (mặt đồng hồ đọc 1410 > 360 — đúng nhờ may). Thứ nó canh là một
/// bản vá NỬA VỜI chỉ dựa vào loại hạn mức; chỗ phân biệt là `left_min ==
/// Some(-30)`, và đột biến bỏ mốc neo (M3, `.tmp/va-tuan-1910/dot-bien.sh`) làm
/// nó đỏ. Lỗi gốc do `ca_that_19_10_…` ở trên khoá.
#[test]
fn vet_banner_tuan_sau_gio_mo_la_da_qua() {
    let s = chot(Some(BAN_GHI_THAT), Some(MAN_TUAN), "2026-10-20T06:30:00Z");
    assert_eq!(s.left_min, Some(-30));
    let luc_13_30 = 13 * 60 + 30;
    assert!(matches!(
        read_reset(KHI, s.window, s.left_min, luc_13_30),
        ResetRead::Passed(_)
    ));
    assert!(matches!(
        quyet(&s, luc_13_30),
        LimitWhy::ResetAlreadyPassed(_)
    ));
    assert!(!limit_still_biting(
        s.limited.as_deref(),
        s.window,
        s.left_min,
        luc_13_30
    ));
    // Cùng ca, CLI cũ: phép neo cũng phải biết.
    let cu = chot(Some(BAN_GHI_CU), Some(MAN_TUAN), "2026-10-20T06:30:00Z");
    assert_eq!(cu.left_min, Some(-30));
}

/// Ranh giới đúng giây: còn 30 giây vẫn là CÒN (1 phút), tới giây mở là đã qua.
#[test]
fn ranh_gioi_dung_giay_mo_lai() {
    let truoc = chot(Some(BAN_GHI_THAT), Some(MAN_TUAN), "2026-10-20T05:59:30Z");
    assert_eq!(truoc.left_min, Some(1));
    let dung = chot(Some(BAN_GHI_THAT), Some(MAN_TUAN), "2026-10-20T06:00:00Z");
    assert_eq!(dung.left_min, Some(0));
    assert!(matches!(
        read_reset(KHI, dung.window, dung.left_min, 13 * 60),
        ResetRead::Passed(_)
    ));
}

/// Ca 09/01 (`[dwork/A-DDRIVE]`, `resets 1:40am` đọc lúc 02:48) vẫn phải ra
/// "đã qua" — loại hạn mức nay đọc từ chính dòng, không còn mặc định.
#[test]
fn han_muc_phien_qua_tran_van_la_da_qua() {
    let man = "You've hit your session limit · resets 1:40am (Asia/Saigon)";
    let (loai, khi) = limit_line_on_screen(man).expect("dòng chặn");
    assert_eq!(loai, LimitWindow::Session);
    assert_eq!(
        read_reset(&khi, loai, None, 2 * 60 + 48),
        ResetRead::Passed(1372)
    );
    assert!(!limit_still_biting(Some(&khi), loai, None, 2 * 60 + 48));
    // Đối chứng: cùng số phút, nhưng là dòng TUẦN ⟹ không được suy "đã qua".
    assert_eq!(
        read_reset(&khi, LimitWindow::Longer, None, 2 * 60 + 48),
        ResetRead::Ahead(1372)
    );
}

// ───────────── mốc neo chỉ dùng khi nói về CÙNG một dòng ─────────────────────

/// Nhật ký mang một dòng chặn KHÁC dòng đang hiện (lần chặn trước, giờ khác)
/// ⟹ mốc của nó nói về sự kiện khác, không được neo vào.
#[test]
fn nhat_ky_mang_dong_khac_thi_khong_neo() {
    let khac = BAN_GHI_THAT.replace("resets 1pm", "resets 3pm");
    let s = chot(Some(&khac), Some(MAN_TUAN), "2026-10-19T12:41:39.920Z");
    assert_eq!(
        s.limited.as_deref(),
        Some(KHI),
        "màn thắng, như trước lượt vá"
    );
    assert_eq!(s.left_min, None);
}

/// Không có dòng chặn nào ⟹ lỗi giữ nguyên là lỗi, không đẻ ra mốc.
#[test]
fn loi_thuong_khong_phai_han_muc() {
    let loi = r#"{"type":"assistant","timestamp":"2026-10-19T12:25:47.812Z","isApiErrorMessage":true,"message":{"role":"assistant","content":[{"type":"text","text":"API Error: 500 Internal server error"}]}}"#;
    let s = chot(Some(loi), None, "2026-10-19T12:41:39.920Z");
    assert_eq!(s.limited, None);
    assert_eq!(s.left_min, None);
    assert_eq!(
        s.error.as_deref(),
        Some("API Error: 500 Internal server error")
    );
    // Mốc đi cùng chữ, không làm mất chữ.
    let nk: JournalError = transcript_error_at(loi).expect("bản ghi lỗi");
    assert_eq!(nk.text, "API Error: 500 Internal server error");
}

// ───────────── tên hạn mức ───────────────────────────────────────────────────

/// Tên đọc từ chính bản cài CLI 2.1.28x. Chỉ `session` là cửa sổ 5 giờ.
#[test]
fn chi_session_la_cua_so_5_gio() {
    assert_eq!(
        limit_window_of("You've hit your session limit · resets 10:30pm"),
        LimitWindow::Session
    );
    for ten in [
        "weekly",
        "Opus",
        "Sonnet",
        "Fable",
        "usage credit",
        "monthly spend",
    ] {
        assert_eq!(
            limit_window_of(&format!("You've hit your {ten} limit · resets 1pm")),
            LimitWindow::Longer,
            "{ten}"
        );
    }
    // Không tên ⟹ không chứng minh được là 5 giờ.
    assert_eq!(
        limit_window_of("You've hit your limit · resets 1pm"),
        LimitWindow::Longer
    );
}

/// Đồng hồ trùng đúng phút in ⟹ mốc là 24 giờ sau (CLI không in một mốc đã tới).
#[test]
fn dong_ho_trung_phut_in_la_24_gio_sau() {
    assert_eq!(
        reset_left_min(
            KHI,
            ms("2026-10-20T06:00:20Z"),
            ms("2026-10-20T06:00:20Z"),
            SAI_GON
        ),
        Some(1440)
    );
}
