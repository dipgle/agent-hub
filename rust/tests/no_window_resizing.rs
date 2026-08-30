//! huba KHÔNG đổi kích thước cửa sổ Terminal của chủ máy — trừ đúng một chỗ:
//! lúc nó tự DỰNG một cửa sổ mới.
//!
//! 🔴 Hà 2026-08-30: *"đừng thay đổi kích thước của sổ terminal nữa, bỏ hết các
//! chỗ đi, để nó luôn full màn hình nếu là cửa sổ mới, còn là tab thì không động
//! đến kích thước"*.
//!
//! Vì sao luật này cần một CỔNG chứ không chỉ một lượt xoá: thứ bị gỡ hôm nay
//! (`screen_text_tall` + `GROW_ASK`) giải một bài toán có thật — màn `24×80` cắt
//! mất hộp chọn và lời phiên vừa nói — nên nó có đủ lý do để mọc lại dưới một
//! cái tên khác, do chính một phiên sau này đọc thấy `⚠ Màn đang hẹp` rồi nghĩ
//! ra "cách chữa". Cổng này đứng đó để lượt ấy phải ĐỎ trước khi kịp lên máy.
//!
//! ⚠ MẪU SỐ (luật 13③). Bài kiểm quét MÃ NGUỒN, mà một phép quét mã nguồn rất dễ
//! xanh vì tìm nhầm chỗ: đường dẫn sai ⟹ 0 dòng khớp ⟹ "sạch". Nên trước khi
//! kết luận, nó phải chứng minh đã đọc đúng tệp — có `pub fn open_window`, có
//! câu AppleScript đọc màn, và tệp phải đủ lớn. Ba mốc ấy hỏng thì bài kiểm ĐỎ,
//! không phải xanh.

use std::path::{Path, PathBuf};

/// Hai câu AppleScript DUY NHẤT đổi được cỡ một cửa sổ Terminal.
const SETTERS: [&str; 2] = ["set number of rows of", "set number of columns of"];

/// Đếm số lần một đoạn mã ra lệnh đổi cỡ cửa sổ.
///
/// Tách thành hàm thuần để ĐỐI CHỨNG NGƯỢC chạy được: bài kiểm cuối tệp bơm vào
/// đây một đoạn mã có cấy lệnh đổi cỡ và đòi nó đếm ra — không có bước ấy thì
/// một hàm luôn trả 0 cũng làm cả tệp này xanh.
fn dem_lenh_doi_co(text: &str) -> usize {
    SETTERS.iter().map(|s| text.matches(s).count()).sum()
}

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn doc(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("không đọc được {}: {e}", p.display()))
}

/// Mọi tệp `.rs` dưới `src/`, kể cả trong thư mục con (`adapters/`).
fn moi_tep_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("không đọc được thư mục {}: {e}", dir.display()));
    for e in entries {
        let p = e.expect("đọc được mục trong thư mục").path();
        if p.is_dir() {
            moi_tep_rs(&p, out);
        } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
            out.push(p);
        }
    }
}

/// Cửa sổ ĐANG SỐNG thì huba không đụng vào cỡ — không một tệp nào trong `src/`
/// được mang lệnh đổi cỡ, trừ `keys.rs` (chỗ dựng cửa sổ mới, kiểm riêng bên dưới).
#[test]
fn khong_tep_nao_ngoai_keys_rs_duoc_doi_co_cua_so() {
    let mut teps = Vec::new();
    moi_tep_rs(&src_dir(), &mut teps);

    // MẪU SỐ: quét trượt cả cây mã thì con số 0 bên dưới không có nghĩa gì.
    assert!(
        teps.len() >= 15,
        "chỉ thấy {} tệp .rs dưới src/ — phép quét đang nhìn nhầm chỗ",
        teps.len()
    );

    let mut pham: Vec<String> = Vec::new();
    for p in &teps {
        if p.file_name().and_then(|x| x.to_str()) == Some("keys.rs") {
            continue;
        }
        let n = dem_lenh_doi_co(&doc(p));
        if n > 0 {
            pham.push(format!("{} ({n} lần)", p.display()));
        }
    }
    assert!(
        pham.is_empty(),
        "có tệp đang đổi cỡ cửa sổ Terminal: {}\n\
         Cửa sổ là của chủ máy — xem tấm bia `screen_text_tall` trong keys.rs.",
        pham.join(" · ")
    );
}

/// Trong `keys.rs`, lệnh đổi cỡ chỉ được nằm trong `open_window` — đúng lúc cửa
/// sổ vừa sinh ra và chưa thuộc về ai.
#[test]
fn doi_co_chi_duoc_nam_trong_open_window() {
    let p = src_dir().join("keys.rs");
    let text = doc(&p);

    // MẪU SỐ, ba mốc — hỏng cái nào cũng là "đang đọc nhầm tệp", và đó phải là ĐỎ.
    assert!(
        text.len() > 100_000,
        "keys.rs chỉ có {} byte — đọc nhầm tệp",
        text.len()
    );
    assert!(
        text.contains("contents of selected tab"),
        "keys.rs không còn câu AppleScript đọc màn — phép quét đang nhìn nhầm chỗ"
    );
    let dau = text
        .find("pub fn open_window")
        .expect("keys.rs phải còn `open_window` — nó là chỗ dựng cửa sổ mới");

    // Thân hàm = từ đầu `open_window` tới `pub fn` kế tiếp.
    let sau = text[dau + 1..]
        .find("\npub fn ")
        .map(|i| dau + 1 + i)
        .unwrap_or(text.len());
    let than = &text[dau..sau];

    let trong_ham = dem_lenh_doi_co(than);
    let ca_tep = dem_lenh_doi_co(&text);
    assert_eq!(
        trong_ham, 2,
        "`open_window` phải đặt ĐÚNG hai chiều (rows + columns) lúc dựng cửa sổ, đang thấy {trong_ham}"
    );
    assert_eq!(
        ca_tep,
        trong_ham,
        "keys.rs còn {} lệnh đổi cỡ NGOÀI `open_window` — cửa sổ đang sống là của chủ máy",
        ca_tep - trong_ham
    );

    // Và cỡ ấy phải là "hết cỡ", không phải một con số đo trên một cái màn.
    assert!(
        than.contains("FULL_SCREEN_ASK"),
        "`open_window` phải xin `FULL_SCREEN_ASK` (Terminal tự kẹp cho vừa màn hình), \
         không gõ cứng một con số"
    );
}

/// 🔴 ĐỐI CHỨNG NGƯỢC (luật 13①) — cấy một lệnh đổi cỡ vào thì phép đo phải THẤY.
///
/// Không có bài này thì `dem_lenh_doi_co` trả 0 vĩnh viễn cũng làm hai bài trên
/// xanh, và cả tệp trở thành một tờ giấy chứng nhận không đo gì.
#[test]
fn phep_do_nay_bat_duoc_mot_lenh_doi_co_cay_vao() {
    let sach = r#"tell application "Terminal"
  return contents of selected tab of window id 7
end tell"#;
    assert_eq!(
        dem_lenh_doi_co(sach),
        0,
        "đoạn mã chỉ ĐỌC màn thì không có gì để bắt"
    );

    let cay = r#"tell application "Terminal"
  set number of rows of selected tab of window id 7 to 999
  set number of columns of selected tab of window id 7 to 999
end tell"#;
    assert_eq!(
        dem_lenh_doi_co(cay),
        2,
        "cấy hai lệnh đổi cỡ mà phép đo không thấy ⟹ nó không đo gì cả"
    );
}

/// 🔴 PHÉP ĐO TRÊN MÁY THẬT: cửa sổ huba tự mở có HẾT CỠ không.
///
/// `#[ignore]` vì nó mở một cửa sổ Terminal thật rồi đóng lại. Chạy tay:
///
/// ```text
/// cargo test --offline --test no_window_resizing -- --ignored --nocapture
/// ```
///
/// Vì sao cần: hai bài trên chấm MÃ NGUỒN — chúng chứng minh không còn chỗ nào
/// đổi cỡ một cửa sổ đang sống, nhưng không chứng minh được câu Hà thật sự hỏi
/// (*"để nó luôn full màn hình nếu là cửa sổ mới"*). Câu ấy chỉ đo được bằng
/// cách mở một cửa sổ rồi hỏi Terminal nó bao nhiêu dòng bao nhiêu cột.
///
/// `open_window` là ĐÚNG hàm `/new` gọi (`sessions::start_bare_window` →
/// `keys::open_window`), nên đây không phải một đường vòng dựng riêng cho test.
#[test]
#[ignore = "mở một cửa sổ Terminal thật rồi đóng — chạy tay bằng --ignored"]
fn cua_so_moi_mo_ra_da_het_co() {
    let (w, tty) =
        huba::keys::open_window("echo huba-window-size-probe").expect("mở được cửa sổ Terminal");
    println!("cửa sổ {w} · tty {tty}");
    assert!(
        w != 0,
        "phải ghép được id cửa sổ — không có id thì không đo được gì"
    );

    let (rows, cols) = huba::keys::window_size(w).expect("hỏi được Terminal cỡ cửa sổ");
    println!("=> {rows} dòng × {cols} cột");

    // Dọn TRƯỚC khi assert: một assert đỏ mà bỏ lại cửa sổ trên máy chủ máy là
    // bản vá tự gây ra đúng thứ nó đi chữa.
    let _ = huba::keys::close_window(w);

    // Ngưỡng đọc từ ca hỏng THẬT, không phải một con số đẹp: cửa sổ mặc định của
    // Terminal trên máy này là `24×80`, và đó chính là cỡ đã cắt mất hộp chọn
    // suốt tháng 8. Hết cỡ đo được hôm 20/08 là `61×206`.
    assert!(
        rows > 24 && cols > 80,
        "cửa sổ mới ra {rows}×{cols} — vẫn là cỡ mặc định, tức lượt xin hết cỡ trong \
         `open_window` không ăn"
    );
}
