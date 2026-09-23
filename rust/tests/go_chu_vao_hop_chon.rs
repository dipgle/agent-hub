//! Gõ CHỮ vào một phiên đang mở HỘP CHỌN: không gõ, nói ra, gắn nút của hộp.
//!
//! 🔴 Hà 2026-09-23: *"Gửi file md vẫn chưa vào được phiên"*. Đo trên nhật ký
//! của CHÍNH phiên đích (`f3b764dd…jsonl`): 14:50:26Z huba gõ `Xem tệp:
//! …/bao-cao-ngay-23.md` vào cửa sổ 7949 lúc hộp hỏi quyền đang mở và báo
//! `✓ đã gửi` — câu ấy KHÔNG CÓ trong nhật ký, không lần nào. Phiên chỉ biết tệp
//! vì 14:51:11Z Hà gõ tiếp một câu hỏi, và phiên tự `find` ra tệp trong `.inbox`.
//!
//! Nhánh gõ SỐ và MŨI TÊN đã đọc màn trước khi gõ từ lâu (`arrow_verdict`, cổng
//! `digit`); nhánh gõ CHỮ thì không — chỗ hổng nằm đúng ở đó. Bài này đọc mã
//! nguồn lúc chạy (`HUBA_PIPELINE_SRC` trỏ được sang bản cũ để chạy đối chứng).

fn nguon() -> String {
    let p = std::env::var("HUBA_PIPELINE_SRC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/src/pipeline.rs").to_string());
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("KHÔNG ĐO ĐƯỢC: không đọc được {p}: {e}"))
}

/// Nhánh từ chối dành cho CHỮ (`!is_key`) trong chuỗi `let refusal = …`, tới
/// nhánh `else { None }` cuối cùng của chuỗi ấy.
fn nhanh_chu(src: &str) -> Option<&str> {
    let dau = src.find("} else if !is_key {")?;
    let cuoi = src[dau..].find("if let Some(msg) = refusal {")?;
    Some(&src[dau..dau + cuoi])
}

/// Nhánh ấy có đọc màn, hỏi hộp chọn, và gắn nút của hộp không.
fn chan_hop_chon(than: &str) -> bool {
    than.contains("keys::look(")
        && than.contains("parse_choices(")
        && than.contains("shot_choices =")
        && than.contains("Some(msg)")
}

#[test]
fn go_chu_luc_dang_mo_hop_chon_thi_khong_go() {
    let src = nguon();
    let than = nhanh_chu(&src).expect(
        "KHÔNG có nhánh từ chối cho CHỮ trong `let refusal = …` — chữ vẫn được gõ thẳng vào hộp chọn",
    );
    assert!(
        chan_hop_chon(than),
        "nhánh chữ phải đọc màn (`look`), hỏi `parse_choices`, gắn `shot_choices` và TỪ CHỐI:\n{than}"
    );
}

/// ĐỐI CHỨNG NGƯỢC: phép dò phải bắt được một nhánh chữ "có mặt mà không chặn".
#[test]
fn doi_chung_nguoc_nhanh_chu_khong_chan() {
    let gia = "} else if !is_key {\n None\n } else { None };\n if let Some(msg) = refusal {";
    let t = nhanh_chu(gia).expect("phải cắt được nhánh giả");
    assert!(
        !chan_hop_chon(t),
        "phép dò mù: nhánh không chặn gì mà vẫn qua"
    );
    let dung = "} else if !is_key {\n match crate::keys::look(&s.tty, 24) { Saw => { let hop = crate::keys::parse_choices(&body); shot_choices = x; Some(msg) } }\n if let Some(msg) = refusal {";
    assert!(
        chan_hop_chon(nhanh_chu(dung).unwrap()),
        "phép dò chặn cả hình dạng đúng"
    );
    // Không có nhánh chữ thì `None` ⟹ bài chính ĐỎ, không phải xanh im lặng.
    assert!(nhanh_chu("} else { None };\n if let Some(msg) = refusal {").is_none());
}
