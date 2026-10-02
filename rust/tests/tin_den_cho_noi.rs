//! Sổ tin đến — CHỖ NỐI ngoài `tin_den.rs`/`telegram.rs`, đọc thẳng MÃ NGUỒN.
//!
//! Hai chỗ nối không dựng được thành bài kiểm chạy thật mà không có Telegram
//! thật hay `launchctl` thật: lô lệnh trong `pipeline::execute_telegram_commands`
//! (cần hòm thư toàn cục đã dựng) và cú tự khởi động lại `runtime::restart_daemon`
//! (giết chính tiến trình). Cùng khuôn với `tests/chay_do_luc_chet.rs` (khối
//! "Chỗ nối"): neo vào chuỗi PHẢI CÒN nếu dây còn đúng, thiếu mỏ neo ⟹ ĐỎ, kèm
//! đối chứng ngược bằng hình dạng cũ.

fn doc(ten: &str, env: &str) -> String {
    let p =
        std::env::var(env).unwrap_or_else(|_| format!("{}/src/{ten}", env!("CARGO_MANIFEST_DIR")));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: không đọc được {p}: {e}"))
}

/// Thân một hàm: từ chữ ký tới chữ ký hàm kế tiếp ở cột 0.
fn than<'a>(src: &'a str, chu_ky: &str) -> Option<&'a str> {
    let a = src.find(chu_ky)?;
    let rest = &src[a + chu_ky.len()..];
    let b = ["\nfn ", "\npub fn ", "\npub(crate) fn "]
        .iter()
        .filter_map(|m| rest.find(m))
        .min()
        .unwrap_or(rest.len());
    Some(&rest[..b])
}

/// Lô Telegram: lấy hàng ⟹ đánh dấu `chay` ⟹ chạy ⟹ đánh dấu `xong`, đúng
/// thứ tự ấy. `None` = thiếu mỏ neo ⟹ ĐỎ.
fn lo_dung_thu_tu(src: &str) -> Option<bool> {
    let t = than(src, "fn execute_telegram_commands(")?;
    let lay = t.find("inbox.drain()")?;
    let chay = t.find("tin_den::bat_dau_lo(")?;
    let lam = t.find("execute_commands(db, cfg, crate::telegram::NAME, &cmds)")?;
    let xong = t.find("tin_den::ket_thuc_lo()")?;
    Some(lay < chay && chay < lam && lam < xong)
}

#[test]
fn lo_telegram_danh_dau_chay_roi_xong() {
    assert_eq!(
        lo_dung_thu_tu(&doc("pipeline.rs", "HUBA_PIPELINE_SRC")),
        Some(true)
    );
}

/// ĐỐI CHỨNG NGƯỢC: hình dạng trước 02/10 (không sổ) ⟹ thiếu mỏ neo; khép
/// `xong` TRƯỚC khi chạy ⟹ sai thứ tự.
#[test]
fn doi_chung_nguoc_lo_khong_so_hoac_khep_som() {
    let cu = "fn execute_telegram_commands(db: &Db, cfg: &Config) {\n let pending = inbox.drain();\n execute_commands(db, cfg, crate::telegram::NAME, &cmds);\n}\nfn sau() {}\n";
    assert_eq!(lo_dung_thu_tu(cu), None);
    let som = "fn execute_telegram_commands(db: &Db, cfg: &Config) {\n let pending = inbox.drain();\n crate::tin_den::bat_dau_lo(&cfg.db, tin);\n crate::tin_den::ket_thuc_lo();\n execute_commands(db, cfg, crate::telegram::NAME, &cmds);\n}\nfn sau() {}\n";
    assert_eq!(lo_dung_thu_tu(som), Some(false));
}

/// Tự khởi động lại: khép lô TRƯỚC `kickstart -k` — sau đó là chết.
fn khep_truoc_kickstart(src: &str) -> Option<bool> {
    let t = than(src, "pub fn restart_daemon()")?;
    let khep = t.find("tin_den::khep_lo_truoc_khi_tu_khoi_dong()")?;
    let chet = t.find("\"kickstart\", \"-k\"")?;
    Some(khep < chet)
}

#[test]
fn tu_khoi_dong_lai_khep_lo_truoc() {
    assert_eq!(
        khep_truoc_kickstart(&doc("runtime.rs", "HUBA_RUNTIME_SRC")),
        Some(true)
    );
}

#[test]
fn doi_chung_nguoc_khoi_dong_lai_khong_khep() {
    let cu = "pub fn restart_daemon() -> anyhow::Result<String> {\n let out = run(\"launchctl\", &[\"kickstart\", \"-k\", &target]);\n}\npub fn sau() {}\n";
    assert_eq!(khep_truoc_kickstart(cu), None);
    let muon = "pub fn restart_daemon() -> anyhow::Result<String> {\n let out = run(\"launchctl\", &[\"kickstart\", \"-k\", &target]);\n crate::tin_den::khep_lo_truoc_khi_tu_khoi_dong();\n}\npub fn sau() {}\n";
    assert_eq!(khep_truoc_kickstart(muon), Some(false));
}
