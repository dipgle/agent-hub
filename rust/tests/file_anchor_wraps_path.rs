//! Nút 📎 phải bọc CHÍNH tên tệp, không dán vào cuối dòng.
//!
//! 🔴 Hà 2026-08-25, ảnh một tin có hai lần `FEATURE-GAPS.md`: *"icon tải tệp
//! gắn không đúng chỗ trong nội dung tin vậy, và cũng chưa bao text đường dẫn
//! file"*.
//!
//! Hai lời ấy là MỘT gốc. Trong `html_with_links`, một đường dẫn nằm giữa câu
//! không phải lệnh và không chiếm trọn dòng, nên nó rơi vào nhánh
//! `_ => (line, "", "")`: `cmd_part` rỗng ⟹ không thẻ `<a>` nào bọc tên tệp ⟹
//! cái 📎 bị đẩy sang danh sách `after`, tức dán vào **cuối dòng nguồn**. Trên
//! màn 390px Telegram bẻ lại đoạn văn, nên "cuối dòng nguồn" hiện ra giữa câu.
//!
//! Và ca thứ ba không ai nhìn thấy: `tame_auto_links` chỉ soi `/` với `@`, nên
//! một tên tệp trần đi qua nguyên vẹn và **Telegram tự nối liên kết** — `.md`
//! là TLD thật của Moldova, y hệt con bug `.sh` ngày 16/08. Chữ xanh trong ảnh
//! trỏ ra một tên miền ngoài, không phải nút tải của huba.

use huba::pipeline::{render_session_data, SessionData};

/// Hình dạng thật, chép từ ảnh Hà gửi: tên tệp đứng ĐẦU dòng nhưng còn cả một
/// câu dài phía sau, nên nó không chiếm trọn dòng.
const SCREEN: &str = "⏺ Còn lại\n\
                      Một đỏ duy nhất: 7 tệp chưa commit — .claude/, memory/, _to_delete/, paint/,\n\
                      FEATURE-GAPS.md, 2 bản .runner-allowlist.bak. Đây là câu hỏi \"cái gì thuộc về\n\
                      repo\", không phải nợ chất lượng mã, nên tôi không tự quyết.";

fn data() -> SessionData {
    SessionData {
        sid: "7bdb4f41-dc79-4b6f-9d04-45bf37d9fcaa".into(),
        files: vec![("FEATURE-GAPS.md".into(), 0)],
        ..Default::default()
    }
}

/// Đích chạm to bằng cả tên tệp — cùng luật đã áp cho khối lệnh (23/08) và cho
/// ô nhập (25/08). Đây là ca thứ ba của đúng một luật.
#[test]
fn the_paperclip_wraps_the_file_name_itself() {
    huba::telegram::set_bot_username("hub_test_bot");
    let html = render_session_data(SCREEN, &data());
    assert!(
        html.contains("📎 FEATURE-GAPS.md</a>"),
        "tên tệp không nằm trong thẻ <a> ⟹ đích chạm vẫn to đúng hai ký tự:\n{html}"
    );
    assert!(html.contains("f_0"), "mất chính cái liên kết tải:\n{html}");
}

/// 🔴 ĐỐI CHỨNG NGƯỢC — hình dạng CŨ phải biến mất, không chỉ hình dạng mới có
/// mặt. Bản hỏng in `<a href="…">📎</a>` (icon trần) và dán nó sau cả câu;
/// khẳng định dưới đây đỏ ngay nếu ai đó gỡ `anchor_is_file`.
#[test]
fn the_old_shape_a_bare_icon_at_end_of_line_is_gone() {
    huba::telegram::set_bot_username("hub_test_bot");
    let html = render_session_data(SCREEN, &data());
    assert!(
        !html.contains(">📎</a>"),
        "📎 lại đứng một mình ⟹ nó đã rơi về cuối dòng:\n{html}"
    );
}

/// …và tên tệp KHÔNG được để trần cho Telegram tự nối liên kết. Thẻ `<a>` bọc
/// ngoài là thứ chặn sẵn ca ấy (đo 23/08 với `gate.sh`), nên phép kiểm là: mọi
/// lần tên tệp xuất hiện đều nằm sau một `<a href=`.
#[test]
fn the_file_name_never_appears_outside_a_link() {
    huba::telegram::set_bot_username("hub_test_bot");
    let html = render_session_data(SCREEN, &data());
    // Cắt tại từng lần tên tệp xuất hiện; phần ngay trước nó phải mở thẻ <a>
    // mà chưa đóng — nếu không, Telegram thấy một tên miền `.md` trần.
    for (i, _) in html.match_indices("FEATURE-GAPS.md") {
        let before = &html[..i];
        let opened = before.rfind("<a href=");
        let closed = before.rfind("</a>");
        assert!(
            matches!((opened, closed), (Some(o), c) if c.is_none_or(|c| c < o)),
            "tên tệp nằm ngoài thẻ <a> ⟹ Telegram sẽ nối nó thành liên kết \
             tới tên miền Moldova (.md):\n{html}"
        );
    }
}

/// Hàng rào ngược: một đường dẫn KHÔNG có trong sổ tệp thì không mọc neo nào —
/// nếu không thì bản vá này biến mọi chuỗi trông-như-tệp thành đích chạm.
#[test]
fn a_path_that_is_not_in_the_file_book_gets_no_anchor() {
    huba::telegram::set_bot_username("hub_test_bot");
    let mut d = data();
    d.files.clear();
    let html = render_session_data(SCREEN, &d);
    assert!(
        !html.contains("f_0"),
        "mọc neo cho tệp không có trong sổ:\n{html}"
    );
}
