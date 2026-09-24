//! "Cửa sổ ấy không còn" phải được nhận ra — kể cả khi AppleScript viết dấu nháy CONG.
//!
//! 🔴 Đo 2026-09-24 19:15→19:58Z trên log thật: 25 lần `window_of_from_cache` mang
//! câu dưới đây, và mã so `"Can't get"` (nháy THẲNG) không bao giờ khớp nó ⟹ cửa sổ
//! đã đóng bị đọc thành "Terminal bận" ⟹ id chết được dùng lại mãi ⟹ 61 lượt
//! "chưa tìm ra cửa sổ của phiên", 8 việc hòm thư bỏ cuộc sau ~1.000 s.

use huba::keys::cua_so_da_mat;

/// NGUYÊN VĂN từ `logs/huba.log` (trường `err` của `window_of_from_cache`).
const THAT: &str = "osascript hỏng: 38:41: execution error: Terminal got an error: \
                    Can’t get window id 10807. (-1728)";

#[test]
fn cau_that_trong_log_la_cua_so_da_mat() {
    assert!(
        THAT.contains('\u{2019}'),
        "mẫu phải giữ đúng dấu nháy cong của log"
    );
    assert!(cua_so_da_mat(THAT));
}

#[test]
fn nhan_theo_ma_loi_va_ca_hai_kieu_nhay() {
    assert!(cua_so_da_mat(
        "execution error: Can't get window id 1. (-1728)"
    ));
    assert!(cua_so_da_mat(
        "execution error: Impossible d’obtenir window id 1. (-1728)"
    ));
    assert!(cua_so_da_mat(
        "Terminal got an error: Can't get window id 3."
    ));
}

#[test]
fn terminal_ban_khong_phai_cua_so_mat() {
    for e in [
        "osascript quá 45s",
        "osascript quá 20s (máy đang tải nặng: lượt đọc gần nhất đã mất 6.2s)",
        "vòng nền đã tiêu hết ngân sách hỏi Terminal (10.7s/10.0s) — KHÔNG hỏi lượt này",
        "nhường Terminal cho một lượt hỏi đang có người chờ (còn 143ms)",
        "osascript hỏng: execution error: Terminal got an error: AppleEvent timed out. (-1712)",
    ] {
        assert!(
            !cua_so_da_mat(e),
            "đọc nhầm 'Terminal bận' thành 'cửa sổ mất': {e}"
        );
    }
}

/// ĐỐI CHỨNG NGƯỢC: phép so CŨ (`contains("Can't get")`) trượt đúng câu thật.
#[test]
fn doi_chung_nguoc_phep_so_cu_truot_cau_that() {
    let cu = |e: &str| e.contains("Can't get");
    assert!(
        !cu(THAT),
        "phép so cũ mà khớp thì bài này không chứng minh gì"
    );
}

/// Chỗ nối: nhánh "cửa sổ đã đóng" của `window_of` dùng đúng hàm này.
#[test]
fn window_of_dung_cua_so_da_mat() {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/src/keys.rs");
    let src = std::fs::read_to_string(p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: {p}: {e}"));
    assert!(
        src.contains("Err(e) if cua_so_da_mat(&e.to_string()) =>"),
        "nhánh cửa-sổ-đã-đóng của `window_of` không dùng `cua_so_da_mat`"
    );
    assert!(
        !src.contains(".contains(\"Can't get\") =>"),
        "còn chỗ so dấu nháy thẳng trong một nhánh match"
    );
}
