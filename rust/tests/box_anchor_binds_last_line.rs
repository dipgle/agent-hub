//! Chữ trong ô nhập trùng với chữ phía trên ⟹ neo bám **dòng CUỐI**, không bỏ cuộc.
//!
//! 🔴 Hà 2026-08-25, ảnh một tin `/shot` có `❯ ssh vps-a "curl -s http://…"` mà
//! không có nút ⏎: *"sao ô chờ gợi ý mờ lại không có nút enter"*.
//!
//! Log của chính huba nói ra thủ phạm, 14:30:33Z:
//!
//! ```text
//! box_anchor_ambiguous {chars: 56, hits: 4,
//!   why: "chữ trong ô nhập trùng với chữ ở chỗ khác trên màn — giữ nút ở đáy"}
//! ```
//!
//! Phiên vừa CHẠY đúng lệnh ấy nên nó còn nằm trong phần hội thoại phía trên.
//! Cửa cũ đòi `hits == 1`, nên nó đóng và nút rơi xuống đáy tin — nơi nút không
//! nói được nó thuộc dòng nào, và trên màn 390px thì phải cuộn đi tìm.
//!
//! Cái mập mờ ấy là BÁO ĐỘNG GIẢ. Ô nhập không phải "một chỗ nào đó có chuỗi
//! này": theo đúng định nghĩa `prompt_line_text` dùng để đọc ra nó, đó là **dòng
//! dấu nhắc cuối cùng còn mang chữ**. Màn cuộn từ trên xuống, ô nhập nằm đáy,
//! nên mọi bản trùng đều ở PHÍA TRÊN.

use huba::pipeline::{render_session_data, SessionData};

/// Chuỗi trong ô nhập — đúng hình dạng thật, kèm dấu nháy và một URL.
const GO: &str = "ssh vps-a \"curl -s http://127.0.0.1:9100/api/v1/version\"";

fn man_co_ban_trung() -> String {
    let vach = "─".repeat(60);
    format!(
        "⏺ Tôi vừa hỏi phiên bản trên vps-a:\n\
         \x20 {GO}\n\
         \x20 ⎿  v4.2.1\n\
         ⏺ Máy trả về v4.2.1, khớp bản vừa cài.\n\
         {vach}\n\
         ❯ {GO}\n\
         {vach}\n\
         \x20 ⏵⏵ auto mode on\n"
    )
}

fn data() -> SessionData {
    SessionData {
        sid: "7bdb4f41-dc79-4b6f-9d04-45bf37d9fcaa".into(),
        box_text: Some(GO.into()),
        ..Default::default()
    }
}

/// 🔴 Ca chính: nút ⏎ phải CÓ, dù chuỗi ấy trùng chỗ khác trên màn.
#[test]
fn the_send_link_survives_the_same_text_appearing_earlier() {
    huba::telegram::set_bot_username("hub_test_bot");
    let html = render_session_data(&man_co_ban_trung(), &data());
    assert!(
        html.contains("send_7bdb4f41"),
        "mất nút ⏎ chỉ vì chuỗi ấy còn nằm trong phần hội thoại phía trên:\n{html}"
    );
}

/// 🔴 ĐỐI CHỨNG NGƯỢC — và đây mới là vế nguy hiểm: nút phải bám **ô nhập**,
/// không bám bản trùng ở trên. Neo sai dòng thì một cú Enter đi vào một dòng
/// KHÔNG phải ô nhập (luật 18/08) — thứ không lùi lại được.
#[test]
fn the_link_binds_the_input_box_not_the_earlier_copy() {
    huba::telegram::set_bot_username("hub_test_bot");
    let html = render_session_data(&man_co_ban_trung(), &data());
    let vi_tri_neo = html.find("send_7bdb4f41").expect("phải có nút ⏎");
    let ban_trung_dau = html
        .find("ssh vps-a")
        .expect("bản trùng phía trên phải còn");
    assert!(
        ban_trung_dau < vi_tri_neo,
        "neo bám lần khớp ĐẦU (dòng phiên đã chạy) thay vì ô nhập ở đáy:\n{html}"
    );
    assert_eq!(
        html.matches("ssh vps-a").count(),
        2,
        "phải còn đủ hai lần nhắc — một trong hội thoại, một trong ô nhập:\n{html}"
    );
}

/// Chuỗi chỉ xuất hiện MỘT lần thì vẫn neo như thường — bản vá không được đổi
/// hành vi của ca đơn giản.
#[test]
fn a_box_text_appearing_once_still_gets_its_link() {
    huba::telegram::set_bot_username("hub_test_bot");
    let vach = "─".repeat(60);
    let man = format!("⏺ Xong rồi anh.\n{vach}\n❯ {GO}\n{vach}\n ⏵⏵ auto mode on\n");
    let html = render_session_data(&man, &data());
    assert!(html.contains("send_7bdb4f41"), "{html}");
}

/// Ô nhập RỖNG thì không có gì để gửi — không được mọc nút.
#[test]
fn an_empty_input_box_gets_no_send_link() {
    huba::telegram::set_bot_username("hub_test_bot");
    let vach = "─".repeat(60);
    let man = format!("⏺ Tôi vừa chạy {GO} xong.\n{vach}\n❯ \n{vach}\n ⏵⏵ auto mode on\n");
    let mut d = data();
    d.box_text = None;
    let html = render_session_data(&man, &d);
    assert!(
        !html.contains("send_7bdb4f41"),
        "ô nhập rỗng mà vẫn mời gửi — cú Enter ấy đi vào chỗ trống:\n{html}"
    );
}
