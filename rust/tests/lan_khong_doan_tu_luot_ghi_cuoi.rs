//! Phiên ĐIỀU PHỐI ghi vào nhiều cây làn thì KHÔNG có làn — đừng lấy lượt ghi
//! cuối làm danh tính.
//!
//! 🔴 Hà 2026-09-16, ảnh thứ hai trong ngày: *"Giờ nhảy thành account rồi"*.
//! Hàng `1249d1ef` — phiên điều phối của dwork, việc của nó là *"Fix main branch
//! session disconnection"* — hiện ra `[dwork/account]`, trong khi nó không thuộc
//! làn nào.
//!
//! Sổ ràng buộc của chính phiên ấy, đọc nguyên văn lúc 07:31:18Z:
//!
//! ```json
//! {"project":"dwork","cay":"dwork/dev-account","nhanh":"lan/account",
//!  "paths":["dwork/dev/bo-moi","dwork/dev-tochuc/.tmp","dwork/dev-account/.tmp"]}
//! ```
//!
//! Nó ghi vào **ba** cây làn — đúng việc của một phiên điều phối (gộp, chạy cổng
//! hộ, để lại ghi chú) — còn `cay`/`nhanh` chỉ giữ cây **ghi GẦN NHẤT**. Cái làn
//! hiện ra vì thế không phải danh tính của phiên; nó là **dấu chân của lượt ghi
//! cuối cùng**.
//!
//! Cùng họ với ca `onghut` 18/08 ở `folder_from_tail`: phép đo không hỏng, nó
//! trả lời đúng câu hỏi *"phiên này vừa ghi vào cây nào"* — chỉ có điều đó không
//! phải câu hỏi *"phiên này thuộc làn nào"*. Cùng một bài học, một tầng thấp hơn.

use serde_json::json;

/// CA CỦA HÀ, nguyên văn bản ghi đã đo được.
#[test]
fn ghi_vao_ba_cay_thi_khong_khai_lan_nao() {
    let so = json!({
        "session": "1249d1ef-57ef-4d5a-ad51-02b9835bde3c",
        "project": "dwork",
        "cay": "dwork/dev-account",
        "nhanh": "lan/account",
        "paths": ["dwork/dev/bo-moi", "dwork/dev-tochuc/.tmp", "dwork/dev-account/.tmp"]
    });
    assert_eq!(
        huba::sessions::lan_tu_so_rang_buoc(&so, "dwork", "1249d1ef"),
        None,
        "phiên ghi vào ba cây làn mà vẫn nhận nhãn làn của lượt ghi cuối — \
         đúng cái Hà chụp màn 16/09 (`[dwork/account]`)"
    );
}

/// Chiều ngược, và nó là chiều phải giữ: phiên chỉ ghi vào MỘT cây thì sổ nói
/// được làn, y như trước.
///
/// Cổng mà chặn luôn ca này thì nó không sửa gì cả — nó chỉ xoá tính năng đã
/// đo được là đúng hôm 06/09 (bốn hàng, khớp cả bốn).
#[test]
fn ghi_vao_mot_cay_thi_van_lay_duoc_lan() {
    let so = json!({
        "project": "dwork",
        "cay": "dwork/dev-ddoc",
        "nhanh": "lan/a-ddoc",
        "paths": ["dwork/dev-ddoc/.tmp", "dwork/dev-ddoc/bo-moi/x.mjs"]
    });
    assert_eq!(
        huba::sessions::lan_tu_so_rang_buoc(&so, "dwork", "test-mot-cay").as_deref(),
        Some("a-ddoc"),
        "một cây thì sổ vẫn phải nói được làn"
    );
}

/// Sổ nói về DỰ ÁN KHÁC thì không phải dữ kiện về hàng này — luật cũ, khoá lại
/// vì bản vá này viết lại đúng đoạn ấy.
#[test]
fn so_cua_du_an_khac_thi_khong_tinh() {
    let so = json!({
        "project": "huba",
        "cay": "huba",
        "nhanh": "main",
        "paths": ["huba/rust/src/sessions.rs"]
    });
    assert_eq!(
        huba::sessions::lan_tu_so_rang_buoc(&so, "dwork", "test-khac-du-an"),
        None
    );
}

/// Không có `paths` thì cư xử như cũ: một cây, lấy làn.
///
/// Sổ cũ (ghi trước khi trường ấy ra đời) không được đọc thành "ghi nhiều cây".
#[test]
fn so_khong_co_paths_thi_van_lay_duoc_lan() {
    let so = json!({"project": "dwork", "cay": "dwork/dev-dci", "nhanh": "lan/a-dci"});
    assert_eq!(
        huba::sessions::lan_tu_so_rang_buoc(&so, "dwork", "test-so-cu").as_deref(),
        Some("a-dci"),
        "sổ cũ thiếu `paths` phải cư xử như trước, không được coi là ghi nhiều cây"
    );
}
