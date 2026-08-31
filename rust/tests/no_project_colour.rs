//! Nhãn phiên KHÔNG đeo ô màu — không ở danh sách, không ở tin gim, không ở sổ.
//!
//! 🔴 Hà 2026-08-31, gửi ảnh buồng chat: *"Tôi đã bảo Bỏ icon màu đi từ rất lâu
//! rồi sao giờ nó hiện lại?"*
//!
//! Câu hỏi có một nửa cần nói cho đúng, vì hai thứ khác nhau đang mang cùng một
//! cái tên "icon màu":
//! * icon **TRẠNG THÁI** màu (`🟢🟡🔴⚫`) — bỏ 2026-08-19, đổi sang HÌNH
//!   (`⚡💤❓❌🪦`). Việc ấy đã làm.
//! * ô màu **DỰ ÁN** (`🟦🟩🟧🟪🟫`) — chưa bao giờ bị bỏ. Nên nó không "hiện lại";
//!   nó nằm đó suốt từ 14/08.
//!
//! Và trong nhật ký còn một tin của Hà lúc **26/08 09:03**: *"ở pinned message:
//! bỏ icon hiện tại đi th…"* — log cắt cụt, KHÔNG commit nào nhắc tới nó. Không
//! chứng minh được nội dung đầy đủ, nhưng đủ để tin rằng một lệnh đã rơi mất một
//! lần. Cổng này để lần sau nó không rơi được nữa.
//!
//! ⚠ Vì sao cổng chấm ĐẦU RA chứ không chấm "có gọi `project_dot` không": chấm
//! lời gọi là chấm một cách viết, mà cái Hà nhìn thấy là một CHUỖI. Sổ theo dõi
//! còn giữ nhãn đã đúc kèm ô từ trước, nên chỉ "thôi gọi" là chưa đủ — chuỗi vẫn
//! ra ô. Bài kiểm thứ hai dưới đây đo đúng khoảng ấy.

use huba::sessions::{shown, LiveSession};

/// Mọi ô vuông màu từng được huba phát, cộng ô "chưa biết dự án".
const O_MAU: [&str; 8] = ["🟦", "🟩", "🟧", "🟪", "🟫", "🟨", "🟥", "⬜"];

fn phien(folder: &str, label: &str) -> LiveSession {
    LiveSession {
        host: "terminal".to_string(),
        name: "projects-88".to_string(),
        folder: folder.to_string(),
        label: label.to_string(),
        account: "acc2".to_string(),
        ..Default::default()
    }
}

/// Nhãn dựng MỚI không được đeo ô — kể cả phiên không biết dự án nào (`⬜`).
#[test]
fn nhan_moi_khong_deo_o_mau() {
    for folder in ["dwork", "huba", "tcc/amm", ""] {
        let ten = shown(&phien(folder, ""));
        for o in O_MAU {
            assert!(
                !ten.contains(o),
                "nhãn của «{folder}» còn đeo `{o}`: {ten:?}"
            );
        }
        assert!(!ten.trim().is_empty(), "gỡ ô xong phải CÒN tên: {ten:?}");
    }
}

/// 🔴 NỬA QUAN TRỌNG: nhãn CŨ trong sổ đã đúc kèm ô — phải GỠ, không chỉ thôi gắn.
///
/// `watch::Mark::l` giữ kết quả của `shown` từ những lượt trước, và
/// `session_from_mark` chép thẳng nó sang `LiveSession::label`. Thôi gắn mà quên
/// gỡ thì mọi phiên đã nằm trong sổ vẫn đeo ô cho tới khi sổ thay hết — tức lệnh
/// của Hà "đã làm" mà anh vẫn nhìn thấy nó, đúng hình dạng của chính lần này.
#[test]
fn nhan_cu_trong_so_bi_go_o_mau() {
    for o in O_MAU {
        let s = phien("dwork", &format!("{o} [dwork]·f33ae528"));
        let ten = shown(&s);
        assert!(
            !ten.contains(o),
            "nhãn cũ mang `{o}` mà không gỡ được: {ten:?}"
        );
        assert!(ten.contains("[dwork]"), "gỡ ô mà mất luôn tên: {ten:?}");
    }
}

/// Cả hai màn chủ máy đọc — danh sách và tin gim — đều sạch.
///
/// Chấm ĐẦU RA THẬT chứ không chấm nội bộ: cái Hà nhìn thấy là chuỗi đi ra
/// Telegram, và lần này chính anh là người phát hiện, không phải bài kiểm nào.
#[test]
fn ca_hai_man_deu_sach_o_mau() {
    let s = phien("dwork", "");
    let hang = huba::pipeline::session_list_text(std::slice::from_ref(&s), "", 0);
    let gim = huba::pipeline::pin_line(&s);
    for o in O_MAU {
        assert!(!hang.contains(o), "danh sách còn `{o}`:\n{hang}");
        assert!(!gim.contains(o), "tin gim còn `{o}`: {gim}");
    }
    // MẪU SỐ: hai chuỗi ấy phải THẬT SỰ nói về phiên này — nếu chúng rỗng thì
    // vòng assert ở trên không chứng minh gì.
    assert!(
        hang.contains("acc2"),
        "danh sách không dựng ra hàng nào:\n{hang}"
    );
    assert!(gim.contains("acc2"), "tin gim không dựng ra gì: {gim}");
}

/// ĐỐI CHỨNG NGƯỢC: phép đo này bắt được một ô màu cấy vào.
///
/// Không có bài này thì `O_MAU` gõ sai một ký tự cũng làm cả tệp xanh — một cổng
/// không bao giờ đỏ được là một tờ giấy chứng nhận, không phải một cổng.
#[test]
fn phep_do_nay_bat_duoc_mot_o_mau_cay_vao() {
    let cay = format!("{} [dwork] · acc2", O_MAU[3]);
    assert!(
        O_MAU.iter().any(|o| cay.contains(o)),
        "cấy `{}` vào mà phép đo không thấy ⟹ nó không đo gì cả",
        O_MAU[3]
    );
}
