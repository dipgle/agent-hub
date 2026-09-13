//! `quiet` được phép IM, nhưng không được phép làm MẤT DẤU.
//!
//! 🔴 Hà 2026-09-13: *"vậy quiet thì sao tôi biết phiên nào nhờ chạy gì"*.
//!
//! Câu ấy đo được là đúng, và con số nằm trong nhật ký ngày 12/09 — ba lượt hòm
//! thư, cả ba `quiet`, cả ba xong trước khi người báo tin `⏳` kịp mở miệng
//! (`LONG_JOB_TICK_SEC` = 90 giây):
//!
//! ```text
//! 11:35:03Z  [huba]        python3 …hoan-tat-wizard-acc4.py   0,06s
//! 17:02:19Z  [dwork/dev]   git merge origin/main …            0,36s
//! 18:59:27Z  [huba]        git push origin main               2,58s
//! ```
//!
//! Hai chỗ đang đọc được từ điện thoại — `/doctor` (`⚡ lệnh chạy nền`) và chân
//! danh sách `/sessions` — đều chỉ nói về việc **ĐANG** chạy. Nên một việc 2,6
//! giây thì tới lúc chủ máy mở ra xem, nó đã xong từ lâu và không chỗ nào kể
//! lại. Cửa `quiet` (25/08) đúng ở chỗ nó tắt 21 tin một buổi; cái nó lấy đi
//! quá tay là **khả năng tra lại**.
//!
//! Bản vá không đổi `quiet` và không đẻ route mới: một cuốn sổ + một khối trong
//! `/doctor`. Bài kiểm dưới đây gác phần dựng chữ, vì đó là phần duy nhất chủ
//! máy thật sự đọc.

use huba::db::Db;
use huba::pipeline::{remember_runin_done, runin_done_book, runin_done_text, truoc_day, RuninDone};

/// Mốc giả: 2026-09-13T02:00:00Z, giây epoch. Cố định — hàm dựng chữ nhận `now`
/// làm THAM SỐ đúng để bài kiểm không tự đỏ theo đồng hồ (cây này đã trả giá một
/// lần cho chuyện ấy: `accounts_text`, PLAN.md 02/09).
const LUC: i64 = 1_789_264_800;

fn dong(w: &str, l: &str, c: Option<i32>, ms: u64, cach_day_giay: i64) -> RuninDone {
    RuninDone {
        w: w.to_string(),
        l: l.to_string(),
        c,
        ms,
        t: LUC - cach_day_giay,
    }
}

/// Sổ rỗng thì KHÔNG in gì — một dòng "chưa có lượt nào" trên màn 390px là chỗ
/// trống trả bằng chỗ.
#[test]
fn so_rong_thi_khong_in_gi() {
    assert_eq!(runin_done_text(&[], LUC), None);
}

/// Đúng ba lượt của ngày 12/09, và câu trả lời phải nói được **phiên nào nhờ
/// lệnh gì** — chính câu Hà hỏi.
#[test]
fn ba_luot_ngay_12_09_doc_ra_dung_ai_nho_gi() {
    let so = vec![
        dong(
            "[huba]",
            "python3 /Users/hanguyen/projects/huba/.tmp/hoan-tat-wizard-acc4.py",
            Some(0),
            60,
            27_000,
        ),
        dong(
            "[dwork/dev]",
            "cd /Users/hanguyen/projects/dwork/dev && git merge origin/main --no-edit",
            Some(0),
            356,
            7_000,
        ),
        dong(
            "[huba]",
            "git -C /Users/hanguyen/projects/huba push origin main",
            Some(0),
            2577,
            60,
        ),
    ];
    let t = runin_done_text(&so, LUC).expect("có ba lượt thì phải in");

    assert!(t.contains("[dwork/dev]"), "thiếu phiên đã nhờ:\n{t}");
    assert!(
        t.contains("git merge origin/main"),
        "thiếu lệnh nó nhờ:\n{t}"
    );
    assert!(
        t.contains("push origin main"),
        "thiếu lệnh của phiên còn lại:\n{t}"
    );

    // 🔴 Đây là ca bài kiểm này BẮT ĐƯỢC ở lượt đầu, và nó là một lỗi thật:
    // dòng của `[dwork/dev]` mở đầu bằng `cd /Users/hanguyen/projects/dwork/dev
    // && …` — 42 ký tự đường dẫn tuyệt đối — nên trần 60 cắt mất đúng ĐỘNG TỪ
    // và ĐÍCH, phần duy nhất trả lời "nó nhờ chạy gì". Thư mục thì nhãn phiên
    // đã nói. Nên `cd … &&` phải bị bóc TRƯỚC khi cắt, chứ không phải nới trần
    // (dòng sau sẽ lại dài hơn).
    assert!(
        !t.contains("cd /Users/hanguyen"),
        "khúc `cd <đường dẫn>` phải bị bóc, nó ăn hết chỗ của lệnh:\n{t}"
    );

    // MỚI NHẤT ĐỨNG TRƯỚC. Đảo thứ tự là bắt chủ máy đọc từ dưới lên trên một
    // màn điện thoại — và cái anh cần gần như luôn là lượt vừa xong.
    let i_push = t.find("push origin main").unwrap();
    let i_merge = t.find("git merge").unwrap();
    assert!(
        i_push < i_merge,
        "lượt mới nhất phải đứng trước lượt cũ hơn:\n{t}"
    );
}

/// 🔴 `⏱ hết giờ` KHÔNG được đọc thành `✅`. Trần thời gian cắt ⟹ **không có mã
/// thoát**, và `Some(0)` ở chỗ ấy sẽ tô xanh đúng ca tệ nhất (§13②: "không đo
/// được" là một trạng thái RIÊNG).
#[test]
fn het_gio_la_trang_thai_rieng_khong_phai_xanh() {
    let t = runin_done_text(
        &[dong("[tfl5]", "bash deploy.sh", None, 3_600_000, 120)],
        LUC,
    )
    .expect("một lượt thì phải in");
    assert!(t.contains("⏱"), "hết giờ phải có dấu riêng:\n{t}");
    assert!(!t.contains("✅"), "hết giờ mà in ✅ là nói dối:\n{t}");

    // ĐỐI CHỨNG NGƯỢC ngay trong bài: cùng khuôn ấy, lượt xong tốt thì PHẢI xanh
    // — không thì bài trên xanh vì hàm chẳng bao giờ in ✅.
    let ok = runin_done_text(&[dong("[tfl5]", "bash deploy.sh", Some(0), 900, 120)], LUC).unwrap();
    assert!(ok.contains("✅"), "lượt xong tốt phải xanh:\n{ok}");
    assert!(!ok.contains("⏱"), "xong tốt mà in hết giờ:\n{ok}");
}

/// Mã thoát khác 0 phải HIỆN RA con số: `❌` một mình thì còn phải đi tra log.
#[test]
fn ma_thoat_khac_khong_hien_nguyen_con_so() {
    let t = runin_done_text(
        &[dong("[dwork]", "git merge origin/main", Some(128), 400, 60)],
        LUC,
    )
    .unwrap();
    assert!(t.contains("exit 128"), "thiếu mã thoát:\n{t}");
}

/// Trần in ra là 5 dòng, và phần còn lại phải KHAI MẪU SỐ — "5 dòng" mà không
/// nói còn bao nhiêu thì đọc ra "chỉ có 5 lượt".
#[test]
fn tran_nam_dong_va_khai_phan_con_lai() {
    let so: Vec<RuninDone> = (0..8)
        .map(|i| {
            dong(
                "[huba]",
                &format!("lenh-{i}"),
                Some(0),
                100,
                (8 - i) as i64 * 60,
            )
        })
        .collect();
    let t = runin_done_text(&so, LUC).unwrap();

    assert!(t.contains("lenh-7"), "lượt mới nhất phải có:\n{t}");
    assert!(
        !t.contains("lenh-2"),
        "lượt thứ sáu tính từ cuối không được in:\n{t}"
    );
    assert!(
        t.contains("còn 3 lượt nữa"),
        "thiếu mẫu số phần không in:\n{t}"
    );

    // Đúng 5 dòng lượt + 1 dòng tiêu đề + 1 dòng mẫu số.
    assert_eq!(t.lines().count(), 7, "số dòng đổi:\n{t}");
}

/// Sổ vừa đúng 5 lượt thì KHÔNG có dòng mẫu số — nó sẽ là một dòng nói "còn 0".
#[test]
fn dung_nam_luot_thi_khong_co_dong_con_lai() {
    let so: Vec<RuninDone> = (0..5)
        .map(|i| {
            dong(
                "[huba]",
                &format!("lenh-{i}"),
                Some(0),
                100,
                (5 - i) as i64 * 60,
            )
        })
        .collect();
    let t = runin_done_text(&so, LUC).unwrap();
    assert!(
        !t.contains("còn"),
        "không được in dòng mẫu số khi đã in hết:\n{t}"
    );
    assert_eq!(t.lines().count(), 6, "1 tiêu đề + 5 lượt:\n{t}");
}

/// Nhãn rỗng là một trạng thái THẬT (phiên chưa kịp gán nhãn) — in một cặp
/// ngoặc trống là mất luôn câu trả lời.
#[test]
fn nhan_rong_thi_noi_ra_chu_khong_in_ngoac_trong() {
    let t = runin_done_text(&[dong("", "ls", Some(0), 10, 30)], LUC).unwrap();
    assert!(
        t.contains("[phiên không rõ]"),
        "nhãn rỗng phải nói ra:\n{t}"
    );
}

/// `truoc_day`: bốn bậc, và mốc chuyển bậc phải đúng cả hai bên.
#[test]
fn truoc_day_bon_bac_hai_chieu() {
    assert_eq!(truoc_day(0), "vừa xong");
    assert_eq!(truoc_day(59), "vừa xong");
    assert_eq!(truoc_day(60), "1 phút trước");
    assert_eq!(truoc_day(5399), "89 phút trước");
    assert_eq!(truoc_day(5400), "1 tiếng trước");
    assert_eq!(truoc_day(86_399), "23 tiếng trước");
    assert_eq!(truoc_day(86_400), "1 ngày trước");
    // Mốc âm (đồng hồ máy nhảy lùi) không được ra một con số âm đọc ra vô nghĩa.
    assert_eq!(truoc_day(-5), "vừa xong");
}

/// Lượt 2,6 giây của 18:59 đọc ra `vừa xong`, đúng cái mà `⏳` (trần 90 giây)
/// không bao giờ kể được.
#[test]
fn luot_ngan_hon_nguong_ticker_van_duoc_ke_lai() {
    let t = runin_done_text(
        &[dong(
            "[huba]",
            "git -C … push origin main",
            Some(0),
            2577,
            40,
        )],
        LUC,
    )
    .unwrap();
    assert!(t.contains("vừa xong"), "{t}");
    assert!(t.contains("[huba]"), "{t}");
}

// ───────────── trọn vòng: GHI vào sổ thật, rồi đọc lại, rồi dựng chữ ─────────

/// Một phép đo chỉ chấm phần dựng chữ thì xanh y nguyên cả khi không ai ghi sổ.
/// Bài này đi trọn vòng trên một DB thật (tạm).
#[test]
fn ghi_roi_doc_lai_duoc() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&tmp.path().join("t.sqlite")).expect("open");

    assert!(runin_done_book(&db).is_empty(), "sổ mới phải rỗng");

    remember_runin_done(
        &db,
        "[dwork/dev]",
        "git merge origin/main",
        Some(0),
        356,
        false,
        LUC,
    );
    remember_runin_done(
        &db,
        "[huba]",
        "git push origin main",
        Some(0),
        2577,
        false,
        LUC + 10,
    );

    let so = runin_done_book(&db);
    assert_eq!(so.len(), 2, "hai lượt phải nằm trong sổ");
    let t = runin_done_text(&so, LUC + 20).expect("có sổ thì phải in");
    assert!(t.contains("[dwork/dev]"), "{t}");
    assert!(t.contains("push origin main"), "{t}");
}

/// 🔴 Trần thời gian cắt ⟹ ghi `None`, KHÔNG ghi `Some(0)`. Đây là vế ghi của
/// bài `het_gio_la_trang_thai_rieng_khong_phai_xanh`: cửa ấy nằm ở chỗ GHI, nên
/// chấm nó ở chỗ dựng chữ thôi là chấm nhờ vào người khác.
#[test]
fn het_gio_vao_so_la_khong_co_ma_thoat() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&tmp.path().join("t.sqlite")).expect("open");

    // `code` vẫn là `Some(0)` — đúng hình dạng `exec::run` trả về khi nó tự cắt:
    // tiến trình bị giết nên mã thoát KHÔNG nói gì về việc lệnh có xong không.
    remember_runin_done(
        &db,
        "[tfl5]",
        "bash deploy.sh",
        Some(0),
        3_600_000,
        true,
        LUC,
    );

    let so = runin_done_book(&db);
    assert_eq!(
        so[0].c, None,
        "hết giờ mà ghi mã thoát là tô xanh ca tệ nhất"
    );
    let t = runin_done_text(&so, LUC).unwrap();
    assert!(t.contains("⏱"), "{t}");
    assert!(!t.contains("✅"), "{t}");
}

/// Trần giữ 20 lượt, và cái bị vứt phải là cái CŨ NHẤT.
#[test]
fn so_giu_hai_muoi_luot_va_vut_cai_cu_nhat() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&tmp.path().join("t.sqlite")).expect("open");

    for i in 0..25 {
        remember_runin_done(
            &db,
            "[huba]",
            &format!("lenh-{i}"),
            Some(0),
            1,
            false,
            LUC + i,
        );
    }
    let so = runin_done_book(&db);
    assert_eq!(so.len(), 20, "trần giữ là 20");
    assert_eq!(so[0].l, "lenh-5", "cái cũ nhất còn lại phải là lượt thứ 6");
    assert_eq!(so[19].l, "lenh-24", "lượt mới nhất phải còn");
}

/// 🔴 CỔNG: chỗ GHI phải nằm ngay cạnh dòng log `runin_ran`, không ở nhánh trả
/// lời. Vì sao cần một cổng đọc-mã cho việc này: đường `/runin` chỉ chạy được
/// khi có một cửa sổ Terminal thật và một phiên `claude` đang sống, nên không
/// bài kiểm nào gọi tới nó. Cổng đọc mã là cổng duy nhất với tới được.
///
/// Nhánh trả lời chia đôi theo `quiet` — nếu lượt ghi trôi vào một trong hai
/// nửa thì đúng nửa IM (cái lỗ Hà chỉ ra) sẽ mất sổ, và **cả 12 bài trên vẫn
/// xanh** vì chúng chỉ chấm phần dựng chữ.
#[test]
fn cho_ghi_so_phai_nam_canh_dong_log_runin_ran() {
    const NGUON: &str = include_str!("../src/pipeline.rs");

    let log = NGUON
        .find("\"runin_ran\"")
        .expect("không thấy `runin_ran` trong pipeline.rs — cổng này mù rồi, sửa mỏ neo");
    // 🔴 Tìm từ SAU dòng log, không tìm từ đầu tệp. Lượt đo đầu của cổng này
    // ĐỎ đúng vì thế: `find` bắt được chỗ ĐỊNH NGHĨA hàm (đứng trước, dòng
    // ~6236) rồi báo "cách 0 ký tự". Một cổng neo sai BỀ MẶT vẫn đỏ/xanh đều
    // đặn — nó chỉ không nói về thứ nó tưởng mình đang nói.
    let ghi = NGUON[log..]
        .find("remember_runin_done_for(")
        .map(|i| i + log)
        .expect("không thấy lượt GỌI ghi sổ sau `runin_ran` — `quiet` lại làm mất dấu");

    assert!(
        ghi > log && ghi - log < 900,
        "lượt ghi sổ phải nằm ngay sau `runin_ran` (cách {} ký tự) — xa thế thì \
         nó đã trôi vào một nhánh, mà nhánh trả lời chia đôi theo `quiet`",
        ghi.saturating_sub(log)
    );
}
