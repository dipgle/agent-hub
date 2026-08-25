//! Đích chạm của danh sách phiên là CẢ HÀNG, không phải một cái nút chép lại
//! hàng ấy — và cũng không phải một cái icon tí xíu.
//!
//! 🔴 Hai câu của Hà, cùng ngày 2026-08-22, và bản vá thứ hai sinh ra từ chỗ
//! bản thứ nhất đi được nửa đường:
//!
//! ① Ảnh chụp buồng chat 21:36 — *"Vẫn đang hiện cả danh sách lẫn nút thừa
//!    thãi"*. Mỗi phiên hiện HAI lần: một hàng chữ (`⌨ 🟪 [dwork/A-DDOC] · 💤
//!    đứng chờ · im 22 phút · 37% · acc3 · 7db02925`) rồi một cái nút mang lại
//!    đúng icon tình trạng + nguồn + tên + tài khoản của hàng ấy. Rút ngắn NHÃN
//!    không cứu được: Telegram cho nút một **chiều cao cố định**, nên sáu nút
//!    vẫn ăn chừng ấy màn hình dù nhãn còn ba chữ. Bỏ nút, đưa cái bấm được lên
//!    hàng.
//! ② Ngay sau đó — *"Nút nhỏ quá rất khó bấm"*. Lượt ① đổi sáu cái nút rộng hết
//!    bề ngang lấy sáu cái icon `👉` rộng chừng hai chục pixel. Hết trùng lặp
//!    thật, nhưng cái bấm được thì teo lại. Thứ rộng ĐÚNG BẰNG cái nút vừa bỏ
//!    là cả cái hàng.

use huba::adapters::CommandKind;
use huba::pipeline::{session_list_html, session_list_text};
use huba::sessions::LiveSession;
use huba::verbs::parse_command;

const NOW: i64 = 1_786_462_200_000;

fn sess(id: &str, label: &str) -> LiveSession {
    LiveSession {
        session_id: id.to_string(),
        name: label.to_string(),
        label: label.to_string(),
        folder: "dwork".to_string(),
        account: "acc3".to_string(),
        working: false,
        ..Default::default()
    }
}

/// ⚠ KHÔNG BỎ DÒNG NÀY. `deep_link` trả `None` khi chưa biết tên bot, nên mọi
/// bài kiểm về liên kết sẽ xanh-giả (0 neo, 0 khẳng định) vì môi trường chứ
/// không vì sản phẩm.
fn bot() {
    huba::telegram::set_bot_username("hub_test_bot");
}

const A: &str = "7db02925-1111-2222-3333-444444444444";
const B: &str = "0f6ba82b-5555-6666-7777-888888888888";

#[test]
fn a_whole_row_is_the_tap_target_not_a_20_pixel_icon() {
    bot();
    let mut a = sess(A, "[dwork/A-DDOC]");
    a.last_text = Some("Xong đợt. Tóm tắt: bàn giao nói hết việc tự đi được".into());
    let rows = [a, sess(B, "[dwork/A-DSIGN]")];
    let text = session_list_text(&rows, "", NOW);
    let (html, wrapped) = session_list_html(&text, &rows);
    assert_eq!(wrapped, 2, "mỗi hàng một đích chạm:\n{html}");

    for r in &rows {
        let open = format!(
            "<a href=\"https://t.me/hub_test_bot?start=s_{}\">👉 ",
            r.session_id
        );
        let i = html
            .find(&open)
            .unwrap_or_else(|| panic!("không thấy đích chạm của {}:\n{html}", r.session_id));
        let inner = &html[i + open.len()..];
        let inner = &inner[..inner.find("</a>").expect("thẻ phải đóng")];
        // CẢ hàng nằm trong thẻ, không phải mỗi cái icon: hàng nào cũng có mã
        // ngắn ở cuối và ít nhất một dấu `·` ngăn ô — bọc mỗi `👉` thì hai vế
        // này đều vắng, và đó đúng là hình dạng đã bị chê "nhỏ quá".
        assert!(
            inner.contains(&r.session_id[..8]),
            "thẻ đóng trước khi hết hàng: {inner}"
        );
        assert!(
            inner.contains('·'),
            "chỉ bọc một mẩu chứ không phải cả hàng: {inner}"
        );
        // …và không nuốt sang dòng khác: một thẻ trùm nhiều hàng thì chạm vào
        // phiên nào cũng ra phiên đầu.
        assert!(!inner.contains('\n'), "thẻ trùm nhiều hàng: {inner}");
    }

    // Đúng HAI thẻ. Dòng `💬` là chữ của phiên, không phải đích chạm — bọc nó
    // vào cùng một liên kết là biến một câu đọc dở thành một cú chạm nhầm.
    assert_eq!(html.matches("<a href=").count(), 2, "{html}");
    let cau_cuoi = html
        .lines()
        .find(|l| l.starts_with("💬"))
        .expect("phải còn câu cuối");
    assert!(!cau_cuoi.contains("<a href="), "{cau_cuoi}");
}

/// Cú chạm phải cởi ra thành ĐÚNG cái lệnh cái nút vẫn gửi — `/session <uuid>`.
///
/// Một đường thứ hai cho cùng một việc là chỗ hai đầu lệch nhau mà không ai
/// biết; đó là cách `run_` từng gãy (một đầu đổi sang hex, đầu đọc ở lại với
/// chữ số, bài kiểm vẫn xanh vì nó tự chọn `run_0`).
#[test]
fn a_tap_becomes_the_same_command_the_button_sent() {
    assert_eq!(
        parse_command(&format!("/start s_{A}")),
        Some((CommandKind::Session, 0, A.to_string()))
    );
    // Mã ngắn cũng đi được — cùng thứ `/session` nhận khi gõ tay.
    assert_eq!(
        parse_command("/start s_7db02925"),
        Some((CommandKind::Session, 0, "7db02925".to_string()))
    );
    // Cửa sổ Terminal trần (`win-ttysNNN`) vẫn là một hàng bấm được.
    assert_eq!(
        parse_command("/start s_win-ttys002"),
        Some((CommandKind::Session, 0, "win-ttys002".to_string()))
    );
    // …còn một chuỗi bịa thì không lọt vào chỗ con trỏ phiên.
    assert_eq!(parse_command("/start s_khong-phai-id-nao"), None);
    assert_eq!(parse_command("/start s_"), None);
}

/// Dựng không nổi liên kết ⟹ KHÔNG hàng nào được bọc, để chỗ gọi rơi về nút.
///
/// Không đặt lại được `OnceLock` tên bot trong cùng tiến trình, nên đo bằng
/// chính cái cửa `deep_link` phải đi qua: id sai luật payload của Telegram thì
/// nó trả `None`, và số hàng bọc được phải rụng theo — chứ không bịa ra một
/// liên kết chạm vào không đi đâu.
#[test]
fn a_link_that_cannot_be_built_never_becomes_a_dead_tap() {
    bot();
    let bad = sess("id có dấu cách", "[x]");
    let rows = [bad];
    let text = session_list_text(&rows, "", NOW);
    let (html, wrapped) = session_list_html(&text, &rows);
    assert_eq!(wrapped, 0, "{html}");
    assert!(!html.contains("<a href="), "{html}");
}
