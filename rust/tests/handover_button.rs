//! Tin hết hạn mức phải BẤM ĐƯỢC, không bắt chủ máy gõ lại một dòng có tham số.
//!
//! 🔴 Hà 2026-09-09: *"Tại sao mấy tin này soạn không bao thành link 1 click để
//! chạy"* · *"Rất nhiều tin kiểu như vậy"*.
//!
//! Tin ấy đã mang sẵn câu trả lời (`/handover -a acc3 <id>`) từ lâu — nhưng chỉ
//! là CHỮ. Và để nguyên dạng chữ thì không có đường nào thành 1-click: Telegram
//! chỉ nhận diện token `/handover`, bấm vào là gửi lệnh **rụng hết tham số**,
//! tức tệ hơn không có nút.
//!
//! Vì sao ca này lọt suốt: lớp nút chỉ được nối cho hai ca — bảng hỏi
//! (`key:`/`pick:`) và đường vào phiên (`sess:`). Cơ chế `run:` (lệnh trong tin
//! → nút) thì cố ý KHÔNG nhận, và đúng như vậy: `run:` gõ lệnh **vào phiên**,
//! còn `/handover` là lệnh của bot — hơn nữa phiên ấy đang bị chặn.

use huba::telegram::callback_to_command;
use huba::watch::{Change, Idle};

/// Một id thật dài 36 ký tự; tin chỉ in 8 ký tự đầu.
const ID: &str = "931cd3fa-1f0c-4a55-9c2e-7b1d0e5a4c88";
const NGAN: &str = "931cd3fa";

fn tin_het_han_muc(goi_y: Option<&str>) -> String {
    Change::Limited {
        id: ID.to_string(),
        name: "[fbot]".to_string(),
        acc: "acc2".to_string(),
        when: "resets 10:50am (Asia/Saigon)".to_string(),
        goi_y: goi_y.map(str::to_string),
    }
    .say(&Idle::Unknown, None)
}

/// 🔴 HỢP ĐỒNG MÁY-ĐỌC: cái nút phải chạy ĐÚNG dòng lệnh mà tin đang hiện.
///
/// Neo vào chính chuỗi lệnh trong tin, không vào một hằng chép tay — nếu ngày
/// mai `watch.rs` đổi cú pháp (thêm cờ, đổi thứ tự tham số) mà nút không đổi
/// theo, bài này ĐỎ. Chép tay hai bên thì hai bên trôi khỏi nhau trong im lặng,
/// đúng lớp lỗi đã trả giá ở `open_window`.
#[test]
fn nut_chay_dung_dong_lenh_ma_tin_dang_hien() {
    let tin = tin_het_han_muc(Some("acc3"));
    let dong = tin
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("/handover"))
        .expect("tin hết hạn mức phải mang sẵn câu lệnh gạt tay");

    assert_eq!(
        callback_to_command(&format!("ho:acc3:{NGAN}")).as_deref(),
        Some(dong),
        "nút và chữ đang nói hai lệnh khác nhau — người bấm sẽ chạy thứ không đọc được"
    );
}

/// Chiều ngược: KHÔNG có tài khoản gợi ý thì tin cố ý in `<tài khoản>` để chủ
/// máy tự chọn — và một cái nút không hỏi được thì không có gì điền vào đó.
/// Bài này khoá lại lý do `pipeline` chỉ gắn nút ở nhánh `Some`.
#[test]
fn khong_co_goi_y_thi_cau_lenh_con_cho_trong() {
    let tin = tin_het_han_muc(None);
    assert!(
        tin.contains("/handover -a <tài khoản>"),
        "nhánh không gợi ý phải để chỗ trống cho chủ máy tự điền:\n{tin}"
    );
}

/// 🔴 ĐỐI CHỨNG NGƯỢC — thiếu vế nào cũng phải trả `None`, không được đoán.
///
/// Một `callback_data` méo mà vẫn dựng ra lệnh thì nó dựng ra lệnh SAI, và lệnh
/// ấy chạy thật. "Không đọc được" phải là một kết cục riêng.
#[test]
fn du_lieu_meo_thi_khong_dung_lenh() {
    for meo in [
        "ho:",          // rỗng cả hai vế
        "ho:acc3",      // thiếu dấu hai chấm ⟹ không tách được id
        "ho::931cd3fa", // thiếu tài khoản
        "ho:acc3:",     // thiếu id
    ] {
        assert_eq!(
            callback_to_command(meo),
            None,
            "`{meo}` méo mà vẫn dựng ra lệnh — đó là một lệnh sai chạy thật"
        );
    }
}

/// Tiền tố mới không được giẫm lên tiền tố cũ — `ho:` và `sess:` là hai route.
#[test]
fn khong_giam_len_cac_nut_da_co() {
    assert_eq!(
        callback_to_command("sess:931cd3fa").as_deref(),
        Some("/session 931cd3fa")
    );
    assert!(
        callback_to_command(&format!("ho:acc3:{NGAN}"))
            .is_some_and(|c| c.starts_with("/handover ")),
        "nút chuyển tài khoản phải đi route /handover"
    );
}

// ───────────── nửa TRÊN của đường đi: ai GẮN chuỗi vào cái nút ─────────────
//
// 🔴 Vì sao thêm mục này (2026-09-09, ngay sau khi dựng cái nút). Bốn bài trên
// gác trọn vẹn nửa DƯỚI: cho `ho:acc3:931cd3fa` thì `callback_to_command` phải
// ra đúng dòng lệnh mà tin đang hiện. Nhưng nửa TRÊN — `pipeline.rs` gắn chuỗi
// gì vào `callback_data` — thì không bài nào chạm tới. Đổi tiền tố
// (`ho:` → `hov:`), đảo hai vế (`ho:{ngan}:{acc}`), hay xoá hẳn dòng
// `quick.push` đều để CẢ BỐN bài trên xanh nguyên, trong khi cái nút trên điện
// thoại bấm vào không ra gì.
//
// Đúng hình dạng đã ghi trong sổ: *"cổng so khối mù với ĐIỂM GỌI — khối giống
// nhau ≠ người gọi truyền đúng"*. Hai phía chép tay cùng một chuỗi thì chúng
// trôi khỏi nhau trong im lặng.
//
// Bài này soi MÃ NGUỒN, không soi hành vi, vì `announce_changes` không với tới
// được từ đây (nó cần `Db` + `Config` + ảnh chụp phiên thật) — cùng lý lẽ
// `cycle_wiring.rs` đã chọn. Và nó ĐỎ ĐƯỢC: ba ca cấy lỗi ở cuối tệp chứng minh
// từng chiều một.
//
// MẪU SỐ — bài này đo cái gì, và KHÔNG đo cái gì: nó đo **chuỗi**
// `callback_data` cùng **độ dài id ngắn** mà điểm gọi cắt ra. Nó KHÔNG chứng
// minh nhánh `Some(acc)` được chạy (thân `announce_changes` nằm ngoài tầm), và
// cũng không thay được một lượt bấm thật trên máy.

/// Mã nguồn của chính `pipeline.rs`.
fn nguon_pipeline() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs"))
        .expect("đọc được src/pipeline.rs")
}

/// Chuỗi `callback_data` mà điểm gọi gắn vào nút, ĐÃ thay hai ô trống bằng giá
/// trị thật — tức đúng thứ Telegram sẽ gửi lại khi có người bấm.
///
/// `None` = không tìm thấy chỗ gắn. Đó là một trạng thái RIÊNG ("không đo
/// được"), và chỗ gọi phải đỏ vì nó: hoặc cái nút đã bị xoá, hoặc route đã đổi
/// tên mà bài kiểm không hay.
fn callback_data_cua_nut(src: &str, acc: &str, ngan: &str) -> Option<String> {
    const MO: &str = "format!(\"ho:";
    let i = src.find(MO)?;
    let sau = &src[i + "format!(\"".len()..];
    let lit = &sau[..sau.find('"')?];
    Some(lit.replace("{acc}", acc).replace("{ngan}", ngan))
}

/// Điểm gọi cắt id ngắn dài bao nhiêu ký tự (`…take(N)…` ngay trên dòng gắn
/// nút). `None` = không đọc được ⟹ đỏ, không được đoán lấy 8.
fn do_dai_id_ngan(src: &str) -> Option<usize> {
    let i = src.find("format!(\"ho:")?;
    let truoc = &src[..i];
    let j = truoc.rfind(".take(")?;
    let sau = &truoc[j + ".take(".len()..];
    sau[..sau.find(')')?].trim().parse().ok()
}

/// Dòng lệnh mà TIN đang hiện — nguồn sự thật chung của cả hai phía.
fn dong_lenh_trong_tin() -> String {
    tin_het_han_muc(Some("acc3"))
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("/handover"))
        .expect("tin hết hạn mức phải mang sẵn câu lệnh gạt tay")
        .to_string()
}

/// 🔴 VÒNG KHÉP KÍN: tin → id ngắn → chuỗi của ĐIỂM GỌI → bộ giải → phải ra
/// đúng dòng lệnh trong tin. Không chỗ nào trong vòng này chép tay chuỗi `ho:`.
#[test]
fn diem_goi_gan_dung_chuoi_ma_bo_giai_doc_duoc() {
    let dong = dong_lenh_trong_tin();
    let ngan = dong
        .split_whitespace()
        .next_back()
        .expect("dòng lệnh phải kết thúc bằng id");
    let src = nguon_pipeline();

    assert!(
        src.contains("goi_y: Some(acc)"),
        "không còn nhánh gắn nút ở `Change::Limited` — nút đã mất, hoặc đã dời đi chỗ khác"
    );
    let data = callback_data_cua_nut(&src, "acc3", ngan)
        .expect("không tìm thấy chỗ gắn `callback_data` cho nút chuyển tài khoản");
    assert!(
        !data.contains('{'),
        "còn ô trống chưa thay trong `{data}` — bài kiểm đang KHÔNG đo được thứ nó tưởng, \
         sửa `callback_data_cua_nut` cho khớp tên biến mới"
    );
    assert_eq!(
        callback_to_command(&data).as_deref(),
        Some(dong.as_str()),
        "chuỗi điểm gọi gắn (`{data}`) giải ra một lệnh KHÁC dòng đang hiện trong tin"
    );
}

/// Id ngắn của nút phải dài đúng bằng id in trong tin: nút làm đúng cái nó nói.
#[test]
fn id_ngan_cua_nut_dai_dung_bang_id_trong_tin() {
    let dong = dong_lenh_trong_tin();
    let ngan = dong
        .split_whitespace()
        .next_back()
        .expect("dòng lệnh phải kết thúc bằng id");
    let n = do_dai_id_ngan(&nguon_pipeline())
        .expect("không đọc được `.take(N)` của id ngắn ở điểm gọi");
    assert_eq!(
        n,
        ngan.chars().count(),
        "nút mang id {n} ký tự trong khi tin in {} ký tự",
        ngan.chars().count()
    );
}

// ───── đối chứng ngược cho CÁCH ĐỌC NGUỒN: cấy lỗi vào thì phải đỏ ─────

/// Nguồn giả, dựng đúng hình dạng thật.
const NGUON_LANH: &str = r#"
        if let crate::watch::Change::Limited { goi_y: Some(acc), .. } = &c {
            let ngan: String = id.chars().take(8).collect();
            quick.push((format!("🔄 Chuyển sang {acc}"), format!("ho:{acc}:{ngan}")));
        }
"#;

/// Cấy ca lành ⇒ KHÔNG được đỏ (nếu không, cổng đỏ với mọi thứ nên vô dụng).
#[test]
fn nguon_lanh_thi_doc_ra_dung_chuoi() {
    assert_eq!(
        callback_data_cua_nut(NGUON_LANH, "acc3", NGAN).as_deref(),
        Some("ho:acc3:931cd3fa")
    );
    assert_eq!(do_dai_id_ngan(NGUON_LANH), Some(8));
}

/// Cấy ca HỎNG ①: đảo hai vế ⇒ chuỗi đọc ra phải KHÁC, tức cổng đỏ.
#[test]
fn dao_hai_ve_thi_khong_con_khop() {
    let dao = NGUON_LANH.replace("ho:{acc}:{ngan}", "ho:{ngan}:{acc}");
    let data = callback_data_cua_nut(&dao, "acc3", NGAN).expect("vẫn đọc được chuỗi");
    assert_eq!(data, "ho:931cd3fa:acc3");
    assert_ne!(
        callback_to_command(&data).as_deref(),
        Some("/handover -a acc3 931cd3fa"),
        "đảo hai vế mà bộ giải vẫn ra đúng lệnh ⟹ cổng này không đỏ được, tức vô dụng"
    );
}

/// Cấy ca HỎNG ②: đổi tiền tố ⇒ "không đo được", và nó phải là một kết cục
/// RIÊNG (`None`), không được lặng lẽ đọc thành sạch.
#[test]
fn doi_tien_to_thi_khong_do_duoc() {
    let doi = NGUON_LANH.replace("format!(\"ho:", "format!(\"hov:");
    assert_eq!(callback_data_cua_nut(&doi, "acc3", NGAN), None);
    assert_eq!(do_dai_id_ngan(&doi), None);
}

/// Cấy ca HỎNG ③: xoá hẳn dòng gắn nút ⇒ `None`. Đây đúng ca mà bốn bài đầu
/// tệp này vẫn xanh trong khi cái nút không còn tồn tại.
#[test]
fn xoa_nut_thi_khong_do_duoc() {
    let xoa: String = NGUON_LANH
        .lines()
        .filter(|l| !l.contains("quick.push"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(callback_data_cua_nut(&xoa, "acc3", NGAN), None);
}

/// Cấy ca HỎNG ④: đổi độ dài id ngắn ⇒ phép đo phải THẤY, không được trả 8 theo
/// trí nhớ.
#[test]
fn doi_do_dai_id_ngan_thi_phep_do_thay() {
    let doi = NGUON_LANH.replace("take(8)", "take(4)");
    assert_eq!(do_dai_id_ngan(&doi), Some(4));
}
