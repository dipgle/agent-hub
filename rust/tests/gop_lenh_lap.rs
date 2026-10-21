//! Loạt cú bấm GIỐNG HỆT nhau ⟹ MỘT lệnh (`pipeline::gop_lenh_lap`).
//!
//! 🔴 Ca thật 16/10 (`.tmp/close-26/tin_den-0216.tsv`): 26 tin «❌ Đóng» tới trong
//! một giây, `message_id` liền nhau 57073→57098 ⟹ 26 hộp «Đóng hẳn phiên
//! [dwork/review]?» nối nhau (02:22:15→02:31:48Z: Hà bấm Huỷ 23 hộp, 3 hộp hết giờ
//! 90 s), còn ~20 cú START, 📋 Phiên, /shot của Hà nằm chờ sau chúng tới 02:32:05Z.
//!
//! Mốc thời gian dưới đây chép từ `telegram_command_queued` trong `logs/huba.log`
//! — đúng lúc từng tin VÀO HÀNG, là thứ hàm này so.

use std::time::{Duration, Instant};

use huba::pipeline::{gop_lenh_lap, GopLap, LAP_CUA_SO_MS};
use huba::telegram::Incoming;

fn tin(text: &str, goc: Instant, ms: u64) -> Incoming {
    Incoming {
        text: text.to_string(),
        msg_id: None,
        quiet: false,
        at: goc + Duration::from_millis(ms),
        tin: None,
    }
}

fn chu(g: &GopLap) -> Vec<&str> {
    g.giu.iter().map(|i| i.text.as_str()).collect()
}

fn bo(g: &GopLap) -> usize {
    g.nhom.iter().map(|(_, n)| n).sum()
}

/// Ca 16/10 nguyên dạng: loạt bị cắt làm HAI lô (2 rồi 24 — `telegram_commands_run`
/// count 2 lúc 02:22:12.773Z, count 24 lúc 02:22:58.689Z). Lô sau phải nhớ cú bấm
/// cuối của lô trước, nếu không thì vẫn còn HAI hộp hỏi.
#[test]
fn hai_muoi_sau_cu_dong_trong_mot_giay_chi_hoi_mot_lan() {
    let goc = Instant::now();
    let lo1 = vec![tin("❌ Đóng", goc, 0), tin("❌ Đóng", goc, 9)];
    let lo2_ms = [
        244, 249, 252, 257, 262, 266, 272, 275, 280, 285, 603, 610, 613, 619, 623, 627, 631, 636,
        927, 936, 943, 947, 952, 958,
    ];
    assert_eq!(
        lo1.len() + lo2_ms.len(),
        26,
        "mẫu số phải đúng 26 tin như sổ"
    );
    let lo2: Vec<Incoming> = lo2_ms.iter().map(|&ms| tin("❌ Đóng", goc, ms)).collect();

    let g1 = gop_lenh_lap(lo1, None);
    assert_eq!(chu(&g1), vec!["❌ Đóng"], "lô đầu: chạy đúng một lần");
    assert_eq!(g1.nhom, vec![("/close".to_string(), 1)]);

    let g2 = gop_lenh_lap(lo2, g1.truoc.clone());
    assert!(
        g2.giu.is_empty(),
        "lô sau là phần đuôi của CÙNG loạt — không được mở thêm hộp hỏi nào, còn {} lệnh",
        g2.giu.len()
    );
    assert_eq!(g2.nhom, vec![("/close".to_string(), 24)]);
    assert_eq!(g1.giu.len() + g2.giu.len() + bo(&g1) + bo(&g2), 26);
}

/// Mười cú START Hà bấm 02:28:03–14Z vì không thấy gì — cách nhau 0,8–1,9 s.
/// Nối chuỗi từng cặp liền nhau, nên cả mười là một loạt.
#[test]
fn muoi_cu_start_lien_tiep_la_mot_loat() {
    let goc = Instant::now();
    let ms = [0, 1560, 3147, 5059, 6070, 6861, 7637, 8624, 9575, 10817];
    let items: Vec<Incoming> = ms.iter().map(|&m| tin("/start", goc, m)).collect();
    let g = gop_lenh_lap(items, None);
    assert_eq!(chu(&g), vec!["/start"]);
    assert_eq!(g.nhom, vec![("/start".to_string(), 9)]);
}

#[test]
fn nhan_ban_phim_va_lenh_go_tay_la_cung_mot_cu_bam() {
    let goc = Instant::now();
    let g = gop_lenh_lap(
        vec![tin("📋 Phiên", goc, 0), tin("/session", goc, 500)],
        None,
    );
    assert_eq!(chu(&g), vec!["📋 Phiên"]);
    assert_eq!(g.nhom, vec![("/session".to_string(), 1)]);
}

// ── ĐỐI CHỨNG NGƯỢC: những thứ KHÔNG được gộp ──────────────────────────────

/// Gõ "ok" hai lần có thể là trả lời hai câu hỏi — gộp nhầm là mất một câu trả lời.
#[test]
fn lenh_lap_co_nghia_khong_bao_gio_bi_gop() {
    let goc = Instant::now();
    for lenh in [
        "ok",
        "/type abc ok",
        "/key 1",
        "/pick 1.2",
        "/shot abc12345",
        "/new -s huba",
    ] {
        let g = gop_lenh_lap(vec![tin(lenh, goc, 0), tin(lenh, goc, 50)], None);
        assert_eq!(
            chu(&g),
            vec![lenh, lenh],
            "{lenh:?} lặp phải chạy CẢ HAI lần"
        );
        assert!(g.nhom.is_empty(), "{lenh:?}: không có loạt nào bị gộp");
    }
}

#[test]
fn cach_qua_cua_so_la_hai_lan_bam_that() {
    let goc = Instant::now();
    let xa = LAP_CUA_SO_MS as u64 + 1;
    let g = gop_lenh_lap(
        vec![tin("📷 Xem màn", goc, 0), tin("📷 Xem màn", goc, xa)],
        None,
    );
    assert_eq!(chu(&g), vec!["📷 Xem màn", "📷 Xem màn"]);
    assert!(g.nhom.is_empty());
    // …và đúng mép cửa sổ thì còn là lặp.
    let g = gop_lenh_lap(
        vec![
            tin("📷 Xem màn", goc, 0),
            tin("📷 Xem màn", goc, LAP_CUA_SO_MS as u64),
        ],
        None,
    );
    assert_eq!(chu(&g), vec!["📷 Xem màn"]);
}

/// Một lệnh khác chen giữa ⟹ thứ tự ý định đã đổi, không còn là "bấm lặp".
#[test]
fn lenh_khac_chen_giua_cat_loat() {
    let goc = Instant::now();
    let g = gop_lenh_lap(
        vec![
            tin("❌ Đóng", goc, 0),
            tin("📋 Phiên", goc, 100),
            tin("❌ Đóng", goc, 200),
        ],
        None,
    );
    assert_eq!(chu(&g), vec!["❌ Đóng", "📋 Phiên", "❌ Đóng"]);
    assert!(g.nhom.is_empty());
}

/// Bước phụ của chính huba (`quiet`) là ruột máy, không phải cú bấm của người.
#[test]
fn lenh_quiet_khong_bao_gio_bi_gop() {
    let goc = Instant::now();
    let mut a = tin("/shot", goc, 0);
    a.quiet = true;
    let mut b = tin("/shot", goc, 10);
    b.quiet = true;
    let g = gop_lenh_lap(vec![a, b], None);
    assert_eq!(g.giu.len(), 2);
}

/// Sổ nhớ của lô trước đã cũ ⟹ cú bấm này là việc mới.
#[test]
fn so_nho_lo_truoc_cu_thi_khong_gop() {
    let goc = Instant::now();
    let truoc = Some(("/close".to_string(), goc));
    let g = gop_lenh_lap(vec![tin("❌ Đóng", goc, 3000)], truoc);
    assert_eq!(chu(&g), vec!["❌ Đóng"]);
    // Và sổ nhớ của lô KHÁC lệnh thì không gộp dù sát giờ.
    let truoc = Some(("/session".to_string(), goc));
    let g = gop_lenh_lap(vec![tin("❌ Đóng", goc, 10)], truoc);
    assert_eq!(chu(&g), vec!["❌ Đóng"]);
}

/// NỐI DÂY: hàm thuần xanh mà lô Telegram không gọi nó thì Hà vẫn nhận 26 hộp.
/// Đọc thân `execute_telegram_commands` từ chính tệp nguồn (cùng cách
/// `tests/cycle_wiring.rs`): lô phải đi qua bộ gộp TRƯỚC khi đọc lệnh, thứ được
/// đọc phải là phần `giu`, và sổ nhớ phải chuyển sang lô sau.
#[test]
fn lo_telegram_di_qua_bo_gop_truoc_khi_doc_lenh() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs"))
        .expect("đọc src/pipeline.rs");
    let dau = src
        .find("fn execute_telegram_commands(")
        .expect("không thấy execute_telegram_commands — bài này phải đổi theo");
    let than = &src[dau..];
    let het = than[1..].find("\nfn ").map_or(than.len(), |i| i + 1);
    let than = &than[..het];
    let gop = than
        .find("gop_lenh_lap(pending")
        .expect("lô Telegram không đi qua gop_lenh_lap");
    let giu = than.find("g.giu").expect("kết quả gộp không được dùng");
    let doc = than
        .find("verbs::parse_command(")
        .expect("không thấy chỗ đọc lệnh");
    assert!(
        gop < giu && giu < doc,
        "phải gộp ⟹ lấy phần giữ ⟹ rồi mới đọc lệnh"
    );
    assert!(
        than.contains("*nho = g.truoc;"),
        "sổ nhớ không chuyển sang lô sau ⟹ loạt bị cắt hai lô vẫn ra hai hộp"
    );
}

/// Hai loạt cùng lệnh, có lệnh khác chen giữa ⟹ HAI dòng log, không dồn vào một.
#[test]
fn moi_loat_mot_dong_nhom() {
    let goc = Instant::now();
    let g = gop_lenh_lap(
        vec![
            tin("❌ Đóng", goc, 0),
            tin("❌ Đóng", goc, 10),
            tin("📋 Phiên", goc, 20),
            tin("❌ Đóng", goc, 30),
            tin("❌ Đóng", goc, 40),
            tin("❌ Đóng", goc, 50),
        ],
        None,
    );
    assert_eq!(chu(&g), vec!["❌ Đóng", "📋 Phiên", "❌ Đóng"]);
    assert_eq!(
        g.nhom,
        vec![("/close".to_string(), 1), ("/close".to_string(), 2)]
    );
}
