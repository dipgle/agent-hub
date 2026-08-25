//! Icon ▶️ phải dính vào ĐÚNG dòng lệnh, và phải thấy được nó ăn tới đâu.
//!
//! 🔴 Hà 2026-08-16, hai ảnh chụp tin của `[mailler]` cách nhau vài phút:
//! *"chỗ này tại sao chỉ rend được một lệnh, mà không biết lệnh đó ăn 1 dòng
//! hay cả 2?"* rồi *"màn mailler lại đang hiện lại đúng kịch bản 2 dòng 1 nút
//! lệnh -> không biết nút lệnh ăn 1 hay cả 2"*.
//!
//! Ảnh thứ hai đọc được rõ hơn và nó lòi ra HAI lỗi, không phải một:
//!
//! 1. Một icon ▶️ **đứng trơ một mình** ngay sau câu văn *"…đã thử bash -n,
//!    cũng bị chặn. Lần chạy của anh sẽ là lần đầu."* — một dòng văn xuôi. Thủ
//!    phạm là nhánh khớp mờ trong `line_carries`: "40 ký tự đầu là đủ". Nhánh
//!    ấy ra đời hồi lệnh còn đọc từ MÀN (cửa sổ bẻ dòng nên không dòng nào chứa
//!    trọn lệnh); từ 08-15 lệnh lấy nguyên văn từ nhật ký, lý do hết, nhánh ở
//!    lại — và một phép khớp mờ ở lại quá hạn thì nó đi khớp nhầm.
//! 2. `deploy.sh` và `update.sh` hiện ra thành **liên kết xanh dẫn ra web**:
//!    `.sh` là một TLD có thật nên Telegram tự nối liên kết. Một dòng lệnh mà
//!    một nửa là link lạ thì không đọc được nó là lệnh nữa.
//!
//! Cả hai vá bằng một chỗ: khớp phải NGUYÊN VẸN, và phần khớp được bọc
//! `<code>` — Telegram không tự nối liên kết bên trong `<code>`, và cái khung
//! ấy chính là câu trả lời cho *"ăn 1 dòng hay cả 2"*.

use huba::pipeline::{html_with_links, SessionData};

const GIT_MV: &str = "git -C ~/projects/AI/mailler mv deploy.sh update.sh";
const SELFCHECK: &str = "bash ~/projects/AI/mailler/scripts/deploy-guard-selfcheck.sh";

/// Nguyên văn khúc tin trong ảnh, giữ cả dòng văn xuôi có chữ `bash -n`.
const REPORT: &str = "Bản vá này tôi chưa chạy được — hook chặn mọi lệnh Bash nêu tên file đó (đã thử bash -n, cũng bị chặn). Lần chạy của anh sẽ là lần đầu.\n\
                      \n\
                      Chạy đúng thứ tự:\n\
                      \n\
                      git -C ~/projects/AI/mailler mv deploy.sh update.sh\n\
                      \n\
                      bash ~/projects/AI/mailler/scripts/deploy-guard-selfcheck.sh\n\
                      \n\
                      Lệnh đầu ~1 giây. Lệnh sau ~10-20 giây.";

fn anchors() -> Vec<(String, Vec<(String, String)>)> {
    vec![
        (
            GIT_MV.to_string(),
            vec![("https://t.me/b?start=run_0".into(), "▶️".into())],
        ),
        (
            SELFCHECK.to_string(),
            vec![("https://t.me/b?start=run_1".into(), "▶️".into())],
        ),
    ]
}

#[test]
fn both_commands_get_their_own_icon() {
    let (html, linked, unlinked) = html_with_links(REPORT, &anchors());
    println!("{html}");
    assert_eq!(linked, 2, "hai dòng lệnh thì phải có hai icon");
    assert!(unlinked.is_empty(), "không lệnh nào bị rơi: {unlinked:?}");
}

/// Lỗi số 1: không một icon nào được rơi vào dòng văn xuôi.
#[test]
fn no_icon_lands_on_a_prose_line() {
    let (html, _, _) = html_with_links(REPORT, &anchors());
    for line in html.lines() {
        if line.contains("Lần chạy của anh sẽ là lần đầu") {
            assert!(
                !line.contains("<a href"),
                "icon dán vào một dòng văn xuôi — đúng cái Hà chụp được:\n{line}"
            );
        }
    }
}

/// Lỗi số 2: phần lệnh phải được BỌC, để mắt đọc được nó ăn tới đâu — và cái
/// bọc ấy nay là `<a>`, không còn là `<code>`.
///
/// 🔴 ĐẢO CHIỀU, KHÔNG XOÁ (2026-08-23). Bài kiểm này ra đời để khoá `<code>`,
/// và nó khoá đúng hai thứ: **ranh giới nhìn thấy được** và **`deploy.sh`
/// không bị Telegram tự nối thành link**. Hà 2026-08-23: *"Tại sao các nút
/// chạy khối lệnh không bao cả khối như cách làm ở danh sách phiên cho dễ
/// bấm"* — và `<a>` giữ được CẢ HAI thứ trên trong khi `<code>` thì không cho
/// bấm.
///
/// Đo thật, `tests/cmd_block_tap_live.rs` (06:10Z và 06:16Z cùng ngày):
/// `<a href="…"><code>lệnh</code></a>` ⟹ Telegram nuốt link, chỉ còn `code`.
/// `<a href="…">lệnh</a>` ⟹ `text_link` phủ trọn 20 ký tự dòng lệnh, và
/// **không** có entity `url` nào mọc ra ở `gate.sh` — thẻ `<a>` bọc ngoài chặn
/// sẵn phép tự-nối-liên-kết. Nên đổi bọc là đổi cơ chế, không phải bỏ lời hứa.
#[test]
fn the_command_is_wrapped_so_the_whole_line_can_be_tapped() {
    let (html, _, _) = html_with_links(REPORT, &anchors());
    let want = format!("<a href=\"https://t.me/b?start=run_0\">▶️ {GIT_MV}</a>");
    assert!(
        html.contains(&want),
        "cả dòng lệnh phải nằm trong <a> — đích chạm to bằng dòng, icon nằm BÊN TRONG:\n{html}"
    );
    // …và không còn `<code>` quanh chính dòng ấy: hai thẻ chồng nhau thì
    // Telegram bỏ cái `<a>` (đo 06:10Z), tức đích chạm chết im.
    assert!(
        !html.contains(&format!("<code>{GIT_MV}</code>")),
        "vẫn còn <code> bọc dòng lệnh ⟹ Telegram sẽ nuốt link:\n{html}"
    );
}

/// …nhưng khi KHÔNG dựng được liên kết thì `<code>` phải ở lại.
///
/// 🔴 Đây là nửa còn lại của lượt 16/08, và nó không được đi theo cái bọc mới:
/// chưa biết tên bot (chưa kịp `getMe`, hoặc mạng hỏng) thì `link_of` trả về
/// rỗng — lúc ấy dòng lệnh không có `<a>` nào bọc, nên nếu bỏ luôn `<code>`
/// thì `deploy.sh` lại thành liên kết xanh dẫn ra web. Đổi lấy không gì cả.
#[test]
fn a_command_with_no_link_keeps_its_code_box() {
    let anchors = vec![(GIT_MV.to_string(), Vec::new())];
    let (html, linked, unlinked) = html_with_links(REPORT, &anchors);
    assert_eq!(linked, 0, "không có liên kết nào để mà đếm");
    assert_eq!(unlinked, vec![0], "neo không bấm được phải được nói ra");
    assert!(
        html.contains(&format!("<code>{GIT_MV}</code>")),
        "mất <code> ở nhánh không có link ⟹ deploy.sh lại tự thành link:\n{html}"
    );
}

/// Văn xuôi chỉ NHẮC tới một lệnh (không chứa trọn nó) thì không được bọc
/// `<code>` — bọc nhầm là đổi nghĩa câu người khác viết.
#[test]
fn prose_that_merely_mentions_a_command_is_left_alone() {
    let (html, linked, unlinked) = html_with_links(
        "hook chặn mọi lệnh Bash nêu tên file đó (đã thử bash -n, cũng bị chặn).",
        &anchors(),
    );
    assert_eq!(linked, 0);
    assert_eq!(unlinked.len(), 0, "không khớp thì cũng không tính là rơi");
    assert!(!html.contains("<code>"), "bọc nhầm văn xuôi:\n{html}");
}

/// Lệnh KHÔNG có trong chữ thì phải rơi xuống nút ở đáy, và nói ra là nó rơi —
/// đây là đường lùi thay cho phép khớp mờ vừa gỡ.
#[test]
fn a_command_absent_from_the_text_falls_to_a_bottom_button() {
    let (_html, linked, unlinked) = html_with_links("Không có dòng lệnh nào ở đây cả.", &anchors());
    assert_eq!(linked, 0);
    // `unlinked` chỉ đếm neo ĐÃ khớp dòng mà không dựng được liên kết; neo
    // không khớp dòng nào thì chỗ gọi tự lo (xem `say_session_data`).
    assert!(unlinked.is_empty());
}

/// Nhãn LỰA CHỌN thì KHÔNG được bọc: *"Set it up"* là một câu tiếng Anh, bọc
/// `<code>` là biến nó thành thứ trông như mã. Khung chỉ dành cho dòng lệnh.
#[test]
fn a_choice_label_is_never_boxed() {
    let (html, _, _) = html_with_links(
        "❯ 1. Set it up\n  2. Not now",
        &[(
            "Set it up".to_string(),
            vec![("https://t.me/b?start=k_x_1".into(), "☑".into())],
        )],
    );
    println!("{html}");
    assert!(!html.contains("<code>"), "nhãn lựa chọn bị bọc:\n{html}");
    assert!(html.contains("<a href"), "vẫn phải có nút chọn:\n{html}");
}

/// Chữ trong Ô NHẬP cũng vậy — nó là câu người ta gõ, không phải lệnh.
#[test]
fn typed_box_text_is_never_boxed() {
    let (html, _, _) = html_with_links(
        "❯ làm việc 1, deploy dev rồi nghiệm thu UI",
        &[(
            "làm việc 1, deploy dev rồi nghiệm thu UI".to_string(),
            vec![("https://t.me/b?start=send_x".into(), "⏎".into())],
        )],
    );
    assert!(!html.contains("<code>"), "chữ trong ô nhập bị bọc:\n{html}");
}

/// Và bảng dữ liệu phiên vẫn đi qua đúng đường ấy.
#[test]
fn session_data_still_renders() {
    let data = SessionData {
        sid: "f168de42".into(),
        choices: vec![("1".to_string(), "Set it up".into())],
        ..Default::default()
    };
    let html = huba::pipeline::render_session_data("❯ 1. Set it up\n  2. Not now", &data);
    println!("{html}");
    // Không có tên bot trong bài kiểm ⟹ `deep_link` trả None ⟹ không có thẻ
    // `<a>`. Cái phải đúng ở đây là hàm chạy được và không bịa ra liên kết.
    assert!(!html.contains("<a href"));
}
