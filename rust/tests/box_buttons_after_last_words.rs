//! Huba NỐI THÊM chữ vào ảnh màn ⟹ vẫn phải giữ hai nút ⏎/⌫ của ô nhập.
//!
//! 🔴 Hà 2026-08-18, ảnh chụp tin `/shot`: *"ô chat có gợi ý tại sao lại không
//! có nút bấm, sao cứ update lại mất vài thứ"*.
//!
//! Đo được trong log cùng phút: tin ấy đi ra `text_links=0`, còn tin `/shot`
//! của một phiên khác có `text_links=2`. Khác nhau đúng một chỗ — phiên này màn
//! chỉ là đầu ra lệnh, nên huba nối thêm khối *"🗣 Lời cuối nó nói"*, và
//! `prompt_line_text` đọc "khối đóng khung CUỐI CÙNG" nên từ lúc ấy nó đọc phần
//! văn xuôi huba tự viết chứ không đọc ô nhập nữa.
//!
//! Ảnh màn dưới đây là NGUYÊN VĂN thứ huba gửi đi lúc 00:58:46Z (trích từ
//! `hubd.err`), không phải bản chép tay cho dễ.

use huba::pipeline::{prompt_line_text, render_session_data, SessionData};

const SCREEN: &str = include_str!("fixtures/shot-screen-2026-08-18.txt");

/// Khối huba nối thêm khi màn không có lời nào của phiên.
const LAST_WORDS: &str = "\n\n🗣 Màn đang là đầu ra của một lệnh, không có lời nào của phiên. \
     Lời cuối nó nói (lấy từ nhật ký):\nXong cả hai việc — đã cài, đã push.\n\
     Ảnh đen: huba nay đo ảnh trước khi gửi, và phân biệt khoá màn với thiếu quyền.";

fn data(box_text: Option<&str>) -> SessionData {
    SessionData {
        sid: "7bdb4f41-dc79-4b6f-9d04-45bf37d9fcaa".into(),
        box_text: box_text.map(str::to_string),
        ..Default::default()
    }
}

/// Trên ảnh màn GỐC thì phép đo vẫn đúng — nên lỗi không nằm ở đây.
#[test]
fn the_box_is_read_from_the_raw_screen() {
    let got = prompt_line_text(SCREEN);
    assert_eq!(
        got.as_deref().map(str::trim),
        Some("Đã bấm /clean rồi, không thấy phản hồi gì")
    );
}

/// 🔴 Bài kiểm TÁI HIỆN: nối khối "lời cuối" vào rồi mới dò ⟹ mất ô nhập.
/// RED với bản cũ (chỗ gọi không đo trước), GREEN khi chỗ gọi truyền `box_text`.
#[test]
fn the_appended_block_no_longer_hides_the_input_box() {
    huba::telegram::set_bot_username("hub_test_bot");
    let mixed = format!("{SCREEN}{LAST_WORDS}");

    // 🔄 ĐẢO CHIỀU 2026-08-19, và cái đảo chiều này là một bản vá chứ không phải
    // một lời nhân nhượng.
    //
    // Câu cũ ở đây là *"dò trên chuỗi ĐÃ TRỘN thì không thấy gì"* — đúng khi neo
    // ô nhập là *"khối đóng khung cuối cùng"* (`rfind('╭')`), vì phần văn xuôi
    // huba nối thêm không có khung nào nên phép dò rơi về "bốn dòng cuối", tức
    // rơi vào đúng khối văn xuôi ấy.
    //
    // Nay neo là VẠCH KẺ của ô nhập (bản `claude` hiện nay không vẽ khung nữa —
    // xem `keys::box_start`), và huba không nối thêm vạch nào, nên chữ nối vào
    // không giấu được ô nhập nữa. Phép đo hết mù ⟹ khẳng định phải đổi theo.
    //
    // Cái KHÔNG đổi, và là lý do phần còn lại của bài kiểm giữ nguyên: chỗ gọi
    // vẫn phải mang theo `box_text` đo trên ảnh màn GỐC. Một phép đo hết mù nhờ
    // hình dạng màn hôm nay không phải một lời hứa cho hình dạng màn hôm sau.
    assert_eq!(
        prompt_line_text(&mixed).as_deref().map(str::trim),
        Some("Đã bấm /clean rồi, không thấy phản hồi gì"),
        "neo mới bám vạch kẻ của ô nhập nên chữ nối thêm KHÔNG che được nó nữa"
    );

    // …nên chỗ gọi phải mang theo chữ đã đo trên ảnh màn gốc.
    let html = render_session_data(
        &mixed,
        &data(Some("Đã bấm /clean rồi, không thấy phản hồi gì")),
    );
    assert!(html.contains("send_7bdb4f41"), "mất nút ⏎:\n{html}");
    assert!(html.contains("clr_7bdb4f41"), "mất nút ⌫:\n{html}");
}

/// Không đo được ô nhập (chỗ gọi không truyền gì) thì vẫn dò như cũ — đường lùi
/// phải còn, vì không phải chỗ gọi nào cũng có ảnh màn trong tay.
#[test]
fn the_old_path_still_works_when_no_box_text_is_given() {
    huba::telegram::set_bot_username("hub_test_bot");
    let html = render_session_data(SCREEN, &data(None));
    assert!(html.contains("send_7bdb4f41"), "{html}");
}
