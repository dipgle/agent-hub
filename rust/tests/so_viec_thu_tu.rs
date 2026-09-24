//! Sổ việc chỉ giữ được việc qua một cú khởi động lại nếu các vai đánh dấu
//! ĐÚNG THỨ TỰ. Bài này đọc MÃ NGUỒN (cùng kiểu `hai_hang_lenh.rs`), vì thứ cần
//! khoá là thứ tự giữa hai lời gọi — không có giá trị trả về nào mang nó.
//!
//! Ba thứ tự, mỗi cái đổi chỗ là quay về đúng một ca đã mất việc thật
//! (23/09 18:15:45Z — hai hòm thư, daemon khởi động lại giữa chừng):
//! ① hòm thư: GHI SỔ trước, ĐỔI TÊN tệp sau — ngược lại thì chết giữa hai bước
//!    là việc biến mất cùng tệp `.taken`;
//! ② chạy lệnh: đánh dấu `chay` TRƯỚC khi tiến trình con ra đời — ngược lại thì
//!    lượt khôi phục thấy `doc` và CHẠY LẠI một lệnh đã chạy;
//! ③ kết quả: vào sổ TRƯỚC khi dán — ngược lại thì kết quả chỉ sống trong bộ nhớ
//!    của luồng đang dán.

fn nguon() -> String {
    let p = std::env::var("HUBA_PIPELINE_SRC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs").to_string());
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: không đọc được {p}: {e}"))
}

/// Thân một hàm cấp cao nhất (mã đã qua `cargo fmt`). `None` ⟹ chỗ gọi ĐỎ.
fn than_ham<'a>(src: &'a str, ten: &str) -> Option<&'a str> {
    let dau = src.find(&format!("fn {ten}("))?;
    let cuoi = src[dau..].find("\n}\n")?;
    Some(&src[dau..dau + cuoi + 2])
}

/// `truoc` xuất hiện trước `sau` trong `than`. Thiếu một trong hai ⟹ `None`
/// (KHÔNG-ĐO-ĐƯỢC ⟹ bài ĐỎ, không phải xanh).
fn truoc(than: &str, truoc: &str, sau: &str) -> Option<bool> {
    Some(than.find(truoc)? < than.find(sau)?)
}

#[test]
fn hom_thu_ghi_so_truoc_doi_ten_sau() {
    let src = nguon();
    let than = than_ham(&src, "runin_inbox_tick").expect("KHÔNG ĐO ĐƯỢC: mất `runin_inbox_tick`");
    assert_eq!(
        truoc(than, "so_viec_nhan_hom_thu(", "std::fs::rename("),
        Some(true),
        "hòm thư phải vào SỔ trước khi tệp bị đổi tên `.taken`:\n{than}"
    );
}

#[test]
fn danh_dau_chay_truoc_khi_tien_trinh_con_ra_doi() {
    let src = nguon();
    let than = than_ham(&src, "watch_long_job").expect("KHÔNG ĐO ĐƯỢC: mất `watch_long_job`");
    assert_eq!(
        truoc(than, "Buoc::Chay", "crate::exec::run("),
        Some(true),
        "`chay` phải vào sổ TRƯỚC `exec::run` — không thì khôi phục chạy lại lệnh đã chạy"
    );
}

#[test]
fn ket_qua_vao_so_truoc_khi_dan() {
    let src = nguon();
    let than = than_ham(&src, "watch_long_job").expect("KHÔNG ĐO ĐƯỢC: mất `watch_long_job`");
    assert_eq!(
        truoc(than, "so_viec_ghi_ket_qua(", "dan_vao_phien("),
        Some(true),
        "kết quả phải vào sổ TRƯỚC cú dán đầu tiên"
    );
}

#[test]
fn so_chi_dung_sau_khi_khoi_phuc_xong() {
    // Cửa nhận và bên thực thi đọc sổ trước khi khôi phục xong ⟹ việc tiến trình
    // NÀY vừa đọc bị khôi phục nhận là của tiến trình cũ ⟹ chạy hai lần.
    let src = nguon();
    for ten in ["so_viec_nhan_hom_thu", "so_viec_doc_moi"] {
        let than = than_ham(&src, ten).unwrap_or_else(|| panic!("KHÔNG ĐO ĐƯỢC: mất `{ten}`"));
        assert_eq!(
            truoc(than, "so_viec_da_khoi_phuc()", "so_viec_mo("),
            Some(true),
            "`{ten}` phải hỏi `so_viec_da_khoi_phuc()` trước khi mở sổ"
        );
    }
}

/// Vòng chạy KHÔNG được tự trả lại (gõ vào Terminal) — chỉ đánh thức luồng riêng.
/// Đo 00:03→00:06Z 24/09: Terminal treo, 4 việc treo ⟹ một vòng 201 s, hòm thư
/// của mọi phiên nằm yên suốt quãng ấy.
fn vong_tu_tra(than_run_once: &str) -> Option<bool> {
    let nen = than_run_once.contains("so_viec_tra_nen(");
    let thang = than_run_once.contains("so_viec_tra_tick(");
    match (nen, thang) {
        (false, false) => None,
        _ => Some(thang),
    }
}

#[test]
fn vong_chay_khong_dung_cho_terminal_de_tra_lai() {
    let src = nguon();
    let than = than_ham(&src, "run_once").expect("KHÔNG ĐO ĐƯỢC: mất `run_once`");
    assert_eq!(
        vong_tu_tra(than),
        Some(false),
        "`run_once` phải gọi `so_viec_tra_nen` (luồng riêng), không gọi thẳng `so_viec_tra_tick`"
    );
}

#[test]
fn doi_chung_nguoc_vong_tu_tra() {
    assert_eq!(
        vong_tu_tra("fn run_once() {\n    so_viec_tra_tick(cfg, &live, now);\n}\n"),
        Some(true)
    );
    assert_eq!(
        vong_tu_tra("fn run_once() {\n    so_viec_tra_nen(cfg, &live, now);\n}\n"),
        Some(false)
    );
    assert_eq!(
        vong_tu_tra("fn run_once() {\n}\n"),
        None,
        "gỡ hẳn bên trả ⟹ KHÔNG-ĐO-ĐƯỢC"
    );
}

/// ĐỐI CHỨNG NGƯỢC: hình dạng SAI của từng thứ tự phải ra `Some(false)`, và
/// thiếu mỏ neo phải ra `None` — không được ra xanh.
#[test]
fn doi_chung_nguoc() {
    let hom_thu_sai = "fn runin_inbox_tick(db: &Db) -> usize {\n    std::fs::rename(&a, &b);\n    so_viec_nhan_hom_thu(&s, &c, &p);\n}\n";
    let t = than_ham(hom_thu_sai, "runin_inbox_tick").unwrap();
    assert_eq!(
        truoc(t, "so_viec_nhan_hom_thu(", "std::fs::rename("),
        Some(false)
    );

    let chay_sai = "fn watch_long_job() {\n    let out = crate::exec::run(x);\n    so_viec_danh_dau(id, crate::so_viec::Buoc::Chay);\n}\n";
    let t = than_ham(chay_sai, "watch_long_job").unwrap();
    assert_eq!(truoc(t, "Buoc::Chay", "crate::exec::run("), Some(false));

    let dan_sai = "fn watch_long_job() {\n    dan_vao_phien(&s, &block);\n    so_viec_ghi_ket_qua(id, c, &block);\n}\n";
    let t = than_ham(dan_sai, "watch_long_job").unwrap();
    assert_eq!(
        truoc(t, "so_viec_ghi_ket_qua(", "dan_vao_phien("),
        Some(false)
    );

    // Gỡ hẳn sổ khỏi hàm ⟹ KHÔNG-ĐO-ĐƯỢC, không phải "thứ tự đúng".
    let khong_so = "fn runin_inbox_tick(db: &Db) -> usize {\n    std::fs::rename(&a, &b);\n}\n";
    let t = than_ham(khong_so, "runin_inbox_tick").unwrap();
    assert_eq!(truoc(t, "so_viec_nhan_hom_thu(", "std::fs::rename("), None);
    assert!(than_ham("fn khac() {\n}\n", "runin_inbox_tick").is_none());
}
