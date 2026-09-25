//! `keys::window_of` hỏi BỘ ĐỆM trước (kiểm bằng một Apple Event), quét từng tab sau.
//!
//! 🔴 2026-09-24, Hà: *"Tại sao mỗi lần phiên chạy thì kênh tele treo vậy"*. Lượt gõ
//! chữ 08:54:50Z mất 71,5 s, trong đó **45 s** là `window_script` (đi qua từng tab,
//! hàng chục Apple Event) hết giờ lúc Terminal bận vẽ cho các phiên đang chạy — rồi
//! mới rơi về id cửa sổ đã nhớ, thứ đúng ngay từ đầu. Bài đọc MÃ NGUỒN (cùng kiểu
//! `hai_hang_lenh.rs`): thứ cần khoá là THỨ TỰ hai lời gọi, không có giá trị trả về
//! nào mang nó — phép đo thật cần một Terminal đang bận.

fn nguon() -> String {
    let p = std::env::var("HUBA_KEYS_SRC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/src/keys.rs").to_string());
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: không đọc được {p}: {e}"))
}

/// Thân bản macOS của `window_of` (bản Windows chỉ chuyển tiếp sang `keys_win`).
fn than_window_of_mac(src: &str) -> Option<&str> {
    let mut tu = 0;
    while let Some(k) = src[tu..].find("pub fn window_of(tty: &str)") {
        let dau = tu + k;
        let cuoi = src[dau..].find("\n}\n")?;
        let than = &src[dau..dau + cuoi];
        if than.contains("window_script(") {
            return Some(than);
        }
        tu = dau + 1;
    }
    None
}

/// Bộ đệm được hỏi TRƯỚC phép quét không. `None` = thiếu mỏ neo ⟹ ĐỎ.
fn dem_truoc(than: &str) -> Option<bool> {
    Some(than.find("recall_window(")? < than.find("window_script(")?)
}

#[test]
fn window_of_hoi_bo_dem_truoc_khi_quet() {
    let src = nguon();
    let than = than_window_of_mac(&src).expect("KHÔNG ĐO ĐƯỢC: mất bản macOS của `window_of`");
    assert_eq!(
        dem_truoc(than),
        Some(true),
        "`window_of` phải hỏi `recall_window` (kiểm 1 Apple Event) TRƯỚC `window_script` — quét trước là trả 45 s mỗi lần Terminal bận"
    );
}

/// ĐỐI CHỨNG NGƯỢC: hình dạng CŨ (quét trước, bộ đệm chỉ là đường lùi khi hỏng).
#[test]
fn doi_chung_nguoc_quet_truoc() {
    let cu = "pub fn window_of(tty: &str) -> Result<Option<i64>> {\n    let script = window_script(&dev);\n    match osascript(&script) {\n        Err(e) => match recall_window(&dev) { w => Ok(w) },\n    }\n}\n";
    assert_eq!(dem_truoc(than_window_of_mac(cu).unwrap()), Some(false));
    let khong_dem = "pub fn window_of(tty: &str) -> Result<Option<i64>> {\n    let script = window_script(&dev);\n}\n";
    assert_eq!(dem_truoc(than_window_of_mac(khong_dem).unwrap()), None);
}
