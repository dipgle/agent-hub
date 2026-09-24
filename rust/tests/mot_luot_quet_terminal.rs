//! Cửa MỘT LƯỢT cho các phép hỏi Terminal đi qua từng tab, và phép NGHỈ sau khi
//! Terminal câm.
//!
//! 🔴 2026-09-24, báo của main dwork: Terminal không nhận Apple Event (-1712) từ
//! ~09:55Z, các lượt `window_script`/`tabs_script` của hubd treo tới trần rồi
//! chồng lên nhau. Bản vá đầu (cài 10:42:17Z, chưa commit) chặn chồng lượt — rồi
//! đo trên log 10:42→11:09Z lộ hai lỗi của chính nó, và bài này khoá cả hai:
//!
//! * `/shot` của Hà lúc 10:47:56Z và 10:49:59Z nhận ngay *"đang có một lượt quét
//!   cửa sổ khác"* — lượt kia là của vòng NỀN. Nay lượt có người chờ thì CHỜ.
//! * **11/15** lần `window_scan_paused` là lỗi huba TỰ sinh mà không hỏi Terminal
//!   (hết ngân sách vòng nền ×10, nhường lượt gấp ×1). Nay chỉ nghỉ khi Terminal
//!   đã thật sự bắt chờ.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use huba::exec::Lane;
use huba::keys::{
    cho_luot, luot_truoc_da_cam, osa_budget, quet_con_nghi_ms, quet_hong_la_terminal_cam, MotLuot,
    QUET_CAM_TU, QUET_NGHI_MS,
};

// ── Hành vi của cửa một lượt ────────────────────────────────────────────────
// Mỗi bài một cờ riêng: các bài chạy song song trong cùng tiến trình.

static CO_NEN: AtomicBool = AtomicBool::new(false);

#[test]
fn vong_nen_khong_cho_va_tra_luot_khi_roi_tam() {
    let dau = MotLuot::xin(&CO_NEN, Duration::ZERO).expect("lượt đầu phải được");
    let bat = Instant::now();
    assert!(
        MotLuot::xin(&CO_NEN, Duration::ZERO).is_none(),
        "lượt thứ hai CHỒNG lên lượt đang chạy"
    );
    assert!(
        bat.elapsed() < Duration::from_millis(50),
        "không chờ (`Duration::ZERO`) mà vẫn ngồi chờ {:?}",
        bat.elapsed()
    );
    drop(dau);
    assert!(
        MotLuot::xin(&CO_NEN, Duration::ZERO).is_some(),
        "lượt đầu rời tầm mà cửa vẫn khoá"
    );
}

static CO_CHO: AtomicBool = AtomicBool::new(false);

#[test]
fn luot_co_nguoi_cho_xep_hang_sau_luot_dang_chay() {
    let dau = MotLuot::xin(&CO_CHO, Duration::ZERO).expect("lượt đầu phải được");
    let (tx, rx) = mpsc::channel();
    let sau = std::thread::spawn(move || {
        let bat = Instant::now();
        let duoc = MotLuot::xin(&CO_CHO, Duration::from_secs(10)).is_some();
        tx.send((duoc, bat.elapsed())).unwrap();
    });
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        rx.try_recv().is_err(),
        "lượt thứ hai đã trả lời trong khi lượt đầu còn giữ cửa — nó phải CHỜ"
    );
    drop(dau);
    let (duoc, da_cho) = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("lượt chờ không bao giờ trả về sau khi cửa đã nhả");
    sau.join().unwrap();
    assert!(duoc, "cửa đã nhả mà lượt đang chờ vẫn không được");
    assert!(
        da_cho >= Duration::from_millis(250),
        "chờ {da_cho:?} — chưa hề chờ"
    );
}

static CO_HET_GIO: AtomicBool = AtomicBool::new(false);

#[test]
fn het_gio_cho_ma_luot_kia_chua_tra_thi_tra_none() {
    let _dau = MotLuot::xin(&CO_HET_GIO, Duration::ZERO).expect("lượt đầu phải được");
    let bat = Instant::now();
    assert!(MotLuot::xin(&CO_HET_GIO, Duration::from_millis(300)).is_none());
    assert!(
        bat.elapsed() >= Duration::from_millis(300),
        "trả `None` sau {:?}, chưa chờ đủ",
        bat.elapsed()
    );
}

static CO_PANIC: AtomicBool = AtomicBool::new(false);

#[test]
fn luot_chet_giua_chung_van_nha_cua() {
    let r = std::panic::catch_unwind(|| {
        let _luot = MotLuot::xin(&CO_PANIC, Duration::ZERO).expect("lượt đầu phải được");
        panic!("cố ý: lượt hỏng giữa chừng");
    });
    assert!(r.is_err());
    assert!(
        MotLuot::xin(&CO_PANIC, Duration::ZERO).is_some(),
        "một lượt panic đã khoá cửa MÃI — mọi lượt dò sau đó sẽ hỏng"
    );
}

static CO_TRANH: AtomicBool = AtomicBool::new(false);
static DANG_GIU: AtomicUsize = AtomicUsize::new(0);
static GIU_NHIEU_NHAT: AtomicUsize = AtomicUsize::new(0);
static DA_XONG: AtomicUsize = AtomicUsize::new(0);

#[test]
fn muoi_sau_luong_tranh_nhau_khong_bao_gio_co_hai_luot_cung_luc() {
    const LUONG: usize = 16;
    const MOI_LUONG: usize = 20;
    let luong: Vec<_> = (0..LUONG)
        .map(|_| {
            std::thread::spawn(|| {
                for _ in 0..MOI_LUONG {
                    let _luot = MotLuot::xin(&CO_TRANH, Duration::from_secs(30))
                        .expect("chờ 30 s vẫn không tới lượt");
                    let n = DANG_GIU.fetch_add(1, Ordering::SeqCst) + 1;
                    GIU_NHIEU_NHAT.fetch_max(n, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(1));
                    DANG_GIU.fetch_sub(1, Ordering::SeqCst);
                    DA_XONG.fetch_add(1, Ordering::SeqCst);
                }
            })
        })
        .collect();
    for l in luong {
        l.join().unwrap();
    }
    // Mẫu số trước: đủ mọi lượt đã chạy, không thì "tối đa 1" có thể chỉ là "ít lượt".
    assert_eq!(DA_XONG.load(Ordering::SeqCst), LUONG * MOI_LUONG);
    assert_eq!(
        GIU_NHIEU_NHAT.load(Ordering::SeqCst),
        1,
        "có lúc hai lượt cùng giữ cửa"
    );
}

// ── Các hàm thuần ───────────────────────────────────────────────────────────

#[test]
fn chi_luot_co_nguoi_cho_moi_cho() {
    let tran = Duration::from_secs(45);
    assert_eq!(cho_luot(Lane::Urgent, tran), tran);
    assert_eq!(cho_luot(Lane::Background, tran), Duration::ZERO);
}

/// Mốc giả, ms epoch — cố định để bài không tự đỏ theo đồng hồ.
const BAY_GIO: i64 = 1_790_248_000_000;

#[test]
fn con_nghi_bao_lau() {
    assert_eq!(
        quet_con_nghi_ms(0, BAY_GIO),
        0,
        "chưa hỏng lần nào mà vẫn nghỉ"
    );
    assert_eq!(quet_con_nghi_ms(BAY_GIO, BAY_GIO), QUET_NGHI_MS);
    assert_eq!(
        quet_con_nghi_ms(BAY_GIO - 10_000, BAY_GIO),
        QUET_NGHI_MS - 10_000
    );
    assert_eq!(quet_con_nghi_ms(BAY_GIO - QUET_NGHI_MS, BAY_GIO), 0);
    assert_eq!(quet_con_nghi_ms(BAY_GIO - 10 * QUET_NGHI_MS, BAY_GIO), 0);
}

#[test]
fn chi_nghi_khi_terminal_that_su_bat_cho() {
    // Hết ngân sách vòng nền / nhường lượt gấp: huba KHÔNG hỏi, trả về ~0 ms.
    assert!(!quet_hong_la_terminal_cam(Duration::ZERO));
    assert!(!quet_hong_la_terminal_cam(Duration::from_millis(3)));
    // Terminal trả lỗi ngay (cửa sổ đã đóng, lỗi cú pháp) — không phải câm.
    assert!(!quet_hong_la_terminal_cam(Duration::from_millis(900)));
    assert!(!quet_hong_la_terminal_cam(
        QUET_CAM_TU - Duration::from_millis(1)
    ));
    assert!(quet_hong_la_terminal_cam(QUET_CAM_TU));
    // Một lượt hết giờ THẬT ở trần nhỏ nhất vẫn phải tính là câm — nếu ai nâng
    // ngưỡng lên quá trần hỏi Terminal thì phép nghỉ chết lặng, bài này đỏ.
    for last_ok in [Duration::ZERO, Duration::from_millis(300)] {
        for lane in [Lane::Background, Lane::Urgent] {
            let tran = osa_budget(lane, last_ok);
            assert!(
                quet_hong_la_terminal_cam(tran),
                "hết giờ ở trần {tran:?} ({lane:?}) không được tính là Terminal câm"
            );
        }
    }
}

#[test]
fn nguoi_xep_hang_khong_hoi_chong_khi_luot_truoc_vua_cam() {
    // Chưa lượt nào câm ⟹ hỏi.
    assert!(!luot_truoc_da_cam(0, BAY_GIO));
    // Lượt mình chờ vừa hết giờ (sau lúc mình bắt đầu xin) ⟹ KHÔNG hỏi chồng.
    assert!(luot_truoc_da_cam(BAY_GIO + 19_000, BAY_GIO));
    assert!(luot_truoc_da_cam(BAY_GIO, BAY_GIO));
    // Câm từ TRƯỚC khi mình tới ⟹ chuyện cũ, hỏi lại (Terminal có thể đã hồi).
    assert!(!luot_truoc_da_cam(BAY_GIO - 1, BAY_GIO));
}

// ── Chỗ nối: đọc MÃ NGUỒN (cùng kiểu `window_of_dem_truoc.rs`) ──────────────
// Thứ cần khoá là THỨ TỰ và SỰ CÓ MẶT của lời gọi trong ba phép dò, không có giá
// trị trả về nào mang nó — phép đo thật cần một Terminal đang câm.

fn nguon() -> String {
    let p = std::env::var("HUBA_KEYS_SRC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/src/keys.rs").to_string());
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: không đọc được {p}: {e}"))
}

/// Thân của hàm có chữ ký `ky`, và có chứa `phai_co` (phân biệt bản macOS với bản
/// chuyển tiếp sang `keys_win`). `thut` = thụt lề của dấu `}` đóng hàm.
fn than<'a>(src: &'a str, ky: &str, phai_co: &str, thut: &str) -> Option<&'a str> {
    let dong = format!("\n{thut}}}\n");
    let mut tu = 0;
    while let Some(k) = src[tu..].find(ky) {
        let dau = tu + k;
        let cuoi = src[dau..].find(&dong)?;
        let t = &src[dau..dau + cuoi];
        if t.contains(phai_co) {
            return Some(t);
        }
        tu = dau + 1;
    }
    None
}

/// `truoc` đứng trước `sau` trong `t`. `None` = thiếu mỏ neo ⟹ ĐỎ.
fn dung_truoc(t: &str, truoc: &str, sau: &str) -> Option<bool> {
    Some(t.find(truoc)? < t.find(sau)?)
}

/// Ba phép dò đi qua từng tab đều xin lượt TRƯỚC khi hỏi Terminal.
fn ba_phep_do_xin_luot(src: &str) -> Vec<String> {
    let mut sai = Vec::new();
    for (ky, xin, hoi) in [
        (
            "pub fn window_of(tty: &str)",
            "QuetCuaSo::xin()",
            "window_script(",
        ),
        (
            "pub fn window_of_any(tty: &str)",
            "QuetCuaSo::xin()",
            "window_any_script(",
        ),
        (
            "fn probe_tabs(with_screens: bool)",
            "MotLuot::xin(&DO_TAB_DANG_CHAY",
            "tabs_script(",
        ),
    ] {
        match than(src, ky, hoi, "") {
            None => sai.push(format!("KHÔNG ĐO ĐƯỢC: mất `{ky}` (thân có `{hoi}`)")),
            Some(t) => {
                if dung_truoc(t, xin, hoi) != Some(true) {
                    sai.push(format!("`{ky}` không gọi `{xin}` trước `{hoi}`"));
                }
            }
        }
    }
    sai
}

/// Cả hai cửa đều chờ theo hạng việc (`cho_luot`), không từ chối người đang chờ.
fn cho_theo_hang(src: &str) -> Vec<String> {
    let mut sai = Vec::new();
    for (ky, phai_co, thut) in [
        (
            "fn xin() -> std::result::Result<MotLuot, String>",
            "MotLuot::xin(&QUET_DANG_CHAY",
            "    ",
        ),
        ("fn probe_tabs(with_screens: bool)", "tabs_script(", ""),
    ] {
        match than(src, ky, phai_co, thut) {
            None => sai.push(format!("KHÔNG ĐO ĐƯỢC: mất `{ky}`")),
            Some(t) => {
                if dung_truoc(t, "cho_luot(", "MotLuot::xin(") != Some(true) {
                    sai.push(format!("`{ky}` không tính `cho_luot(` trước khi xin lượt"));
                }
            }
        }
    }
    sai
}

/// `probe_tabs`: được lượt rồi thì hỏi "lượt mình vừa chờ có câm không" TRƯỚC khi
/// hỏi Terminal, và chỉ ghi mốc câm qua `quet_hong_la_terminal_cam`.
fn do_tab_khong_hoi_chong(src: &str) -> Option<bool> {
    let t = than(src, "fn probe_tabs(with_screens: bool)", "tabs_script(", "")?;
    Some(
        dung_truoc(t, "MotLuot::xin(", "luot_truoc_da_cam(")?
            && dung_truoc(t, "luot_truoc_da_cam(", "tabs_script(")?
            && dung_truoc(t, "quet_hong_la_terminal_cam(", "DO_TAB_CAM_MS.store(")?,
    )
}

#[test]
fn do_tab_xep_hang_roi_khong_hoi_chong() {
    assert_eq!(do_tab_khong_hoi_chong(&nguon()), Some(true));
}

/// Phép nghỉ chỉ ghi mốc khi Terminal đã bắt chờ.
fn nghi_co_cua(src: &str) -> Option<bool> {
    let t = than(src, "fn hong(", "QUET_HONG_MS.store(", "    ")?;
    dung_truoc(t, "quet_hong_la_terminal_cam(", "QUET_HONG_MS.store(")
}

/// Mỗi nhánh `window_cache_stale` (id đã ĐO RA là sai) đều quên id ấy — không thì
/// hai đường lùi "dùng id đã nhớ" phía dưới (quét bị hoãn · quét hết giờ) rơi về
/// đúng cái xác vừa đo. `None` = thiếu mỏ neo ⟹ ĐỎ.
fn nhanh_sai_deu_quen(src: &str) -> Option<bool> {
    let t = than(src, "pub fn window_of(tty: &str)", "window_script(", "")?;
    let dau_quet = t.find("window_script(")?;
    let truoc = &t[..dau_quet];
    let sai = truoc.matches("\"window_cache_stale\"").count();
    Some(sai > 0 && truoc.matches("forget_window(&dev)").count() == sai)
}

#[test]
fn id_da_do_ra_sai_thi_quen() {
    assert_eq!(
        nhanh_sai_deu_quen(&nguon()),
        Some(true),
        "nhánh `window_cache_stale` nào cũng phải `forget_window(&dev)`"
    );
}

#[test]
fn doi_chung_nguoc_nhanh_sai_khong_quen() {
    let cu = "pub fn window_of(tty: &str) -> Result<Option<i64>> {\n    if let Some(w) = recall_window(&dev) {\n        match selected_tab_tty(w) {\n            Ok(t) => logging::info(\"window_cache_stale\", json!({})),\n            Err(e) => logging::info(\"window_cache_stale\", json!({})),\n        }\n    }\n    let script = window_script(&dev);\n}\n";
    assert_eq!(nhanh_sai_deu_quen(cu), Some(false));
    let mot_nua = cu.replacen("Ok(t) => ", "Ok(t) => { forget_window(&dev); }\n", 1);
    assert_eq!(
        nhanh_sai_deu_quen(&mot_nua),
        Some(false),
        "quên ở 1/2 nhánh vẫn phải ĐỎ"
    );
}

#[test]
fn ba_phep_do_deu_qua_cua_mot_luot() {
    let sai = ba_phep_do_xin_luot(&nguon());
    assert!(sai.is_empty(), "{sai:#?}");
}

#[test]
fn hai_cua_deu_cho_theo_hang_viec() {
    let sai = cho_theo_hang(&nguon());
    assert!(sai.is_empty(), "{sai:#?}");
}

#[test]
fn nghi_chi_khi_terminal_cam() {
    assert_eq!(
        nghi_co_cua(&nguon()),
        Some(true),
        "`QuetCuaSo::hong` phải hỏi `quet_hong_la_terminal_cam` TRƯỚC khi ghi mốc nghỉ"
    );
}

/// ĐỐI CHỨNG NGƯỢC: đúng hình dạng bản cài 10:42:17Z (chưa commit) — ba bộ dò
/// trên phải ĐỎ ở cả ba chỗ nó sai, và thiếu mỏ neo thì cũng ĐỎ, không xanh.
#[test]
fn doi_chung_nguoc_ban_cai_1042z() {
    let cu = "\
fn xin() -> std::result::Result<QuetCuaSoGiu, String> {
        if QUET_DANG_CHAY.swap(true, Ordering::SeqCst) {
            return Err(String::new());
        }
        Ok(QuetCuaSoGiu)
    }
    fn hong(err: &str) {
        QUET_HONG_MS.store(0, SeqCst);
    }
fn probe_tabs(with_screens: bool) -> Result<Vec<Tab>> {
    if DO_TAB_DANG_CHAY.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err(anyhow!(\"\"));
    }
    let out = osascript(&tabs_script(with_screens))?;
}
";
    assert_eq!(nghi_co_cua(cu), None, "nghỉ vô điều kiện phải ĐỎ");
    let hoi_sau = "    fn hong(da_cho: Duration, err: &str) {\n        QUET_HONG_MS.store(0, SeqCst);\n        if !quet_hong_la_terminal_cam(da_cho) {}\n    }\n";
    assert_eq!(
        nghi_co_cua(hoi_sau),
        Some(false),
        "hỏi SAU khi đã ghi mốc nghỉ phải ĐỎ"
    );
    let sai = cho_theo_hang(cu);
    assert_eq!(sai.len(), 2, "{sai:#?}");
    let sai = ba_phep_do_xin_luot(cu);
    assert_eq!(sai.len(), 3, "{sai:#?}");
    assert!(
        sai.iter().filter(|s| s.starts_with("KHÔNG ĐO ĐƯỢC")).count() == 2,
        "mất `window_of`/`window_of_any` phải khai KHÔNG ĐO ĐƯỢC (đỏ), không lặng lẽ bỏ qua: {sai:#?}"
    );
    assert!(
        nghi_co_cua("fn khac() {}\n").is_none(),
        "mất `hong` phải ra None ⟹ bài chính ĐỎ"
    );
}
