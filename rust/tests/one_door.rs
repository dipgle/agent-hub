//! MỘT CỬA cho chữ của phiên ra Telegram, và cái nút 🖥 trả kết quả về.
//!
//! 🔴 Vì sao có tệp này. Hà 2026-08-16: *"lệnh `/shot` hay phản hồi tự động gửi
//! về tele đều phải qua định dạng trước khi gửi → cái nhận được ở tele phải
//! thao tác được với các lệnh link của phiên đó"* · *"mọi thứ nhìn thấy ở tele
//! phải đồng nhất"* · *"dành cho nội dung lấy từ phiên thôi"*.
//!
//! Hai phép đo dưới đây canh đúng hai chỗ dễ hỏng của bản vá ấy, và cả hai đều
//! đo HẬU QUẢ (chữ Telegram sẽ hiện) chứ không đo mỗi hàm lọc: một phép đo chỉ
//! hỏi "hàm có trả đúng danh sách không" vẫn xanh nguyên khi cái tin gửi đi đã
//! mọc thêm một khu chữ không ai hỏi.

use huba::pipeline::{
    cmds_present_in, needs_formatting, paths_not_in_commands, render_session_data,
    tail_after_command, SessionData,
};
use huba::sessions::Cmd;

/// Cửa hỏi *"tin này có mang chữ của phiên không"*, KHÔNG hỏi *"có nút không"*.
///
/// 🔴 Hai hồi quy liền trong một buổi, cùng một gốc — điều kiện "đi cửa nào"
/// nằm rải trong thân hàm nên mỗi bản vá lại đổi một mảnh: gỡ hai nút trống thì
/// mất liên kết `⏎` giữa chữ (*"Lại mất nút gửi nhanh gợi ý mờ rồi"*); bắt mọi
/// tin đi qua cửa thì ack `✓ vào hàng chờ` thôi thả emoji (*"Chỉnh thành phản
/// hồi bằng emoji rồi cơ mà"*). Nay một hàm, và đây là bài kiểm của nó.
#[test]
fn a_bare_acknowledgement_stays_an_emoji_and_never_goes_through_the_door() {
    for ack in [
        "✓ vào hàng chờ · 🟪 [huba]",
        "✓ đã gửi · 🟪 [huba]",
        "👁 Đang theo phiên [tfl5]",
        "⏹ đã tắt",
    ] {
        assert!(
            !needs_formatting(ack),
            "câu xác nhận trơn phải đi bằng một dấu, không chiếm dòng chữ: {ack}"
        );
    }
    for real in [
        "📷 Màn của 🟪 [huba]:\n\n❯ làm gì đó",
        "📋 Đã đóng sổ phiên [tfl5]. Tiếp tục bằng:\ncd /x && claude --resume 1",
        // 🔴 Ack của một cú bấm trong hộp chọn mang cả BẢNG trạng thái, và nó
        // vẫn mở đầu bằng `✓` — thiếu cổng "nhiều dòng thì không phải xác nhận
        // trơn" thì nó bị rút thành một mặt cười, mất sạch bảng (Hà 17/08:
        // *"Phản hồi nên thêm ô đã tích hay chưa và cho phép bấm được luôn"*).
        "✓ 🟥 [dwork] — 4/5 ô đang chọn\n1. [✓] Không xoá gì\n2. [ ] Rác build\nSubmit",
    ] {
        assert!(
            needs_formatting(real),
            "chữ của phiên phải qua cửa định dạng: {real}"
        );
    }
}

/// 📎 không mọc trên một đường dẫn NẰM TRONG dòng lệnh.
///
/// 🔴 Hà 2026-08-16, đọc chính tin tôi gửi: *"Mà dòng lệnh lại gắn nút tải file
/// là sao"* — dòng `rm ~/…/probe_prompt_anchor.rs` kèm một nút 📎 mời tải đúng
/// cái tệp mà dòng ấy bảo xoá.
#[test]
fn a_path_inside_a_command_line_gets_no_download_link() {
    // Phép lọc hỏi theo DÒNG (18/08), nên nó cần chính đoạn chữ ấy: một đường
    // dẫn chỉ sống trong dòng lệnh thì bị bỏ, một đường dẫn được nhắc như tệp
    // thì giữ.
    let text = "Ghi chú ở docs/flow-boc-tach-lenh.md\n\n\
                rm ~/projects/huba/rust/tests/probe_prompt_anchor.rs\n";
    let cmds = vec![cmd("rm ~/projects/huba/rust/tests/probe_prompt_anchor.rs")];
    let seen = vec![
        "~/projects/huba/rust/tests/probe_prompt_anchor.rs".to_string(),
        "docs/flow-boc-tach-lenh.md".to_string(),
    ];
    let kept = paths_not_in_commands(text, &seen, &cmds);
    assert_eq!(kept, vec!["docs/flow-boc-tach-lenh.md".to_string()]);
}

fn cmd(line: &str) -> Cmd {
    Cmd {
        line: line.to_string(),
        cwd: String::new(),
    }
}

/// Cửa định dạng gắn action cho lệnh CÓ TRONG chữ — và không đẻ thêm chữ.
///
/// `session_layout` cố ý nối thêm khu *"Lệnh phiên chạy không được"* cho lệnh
/// nó không thấy trong tin. Đúng cho `/shot`, tai hại cho một cái ack hai dòng:
/// tin ngắn sẽ mọc ra cả danh sách lệnh của lượt trước.
#[test]
fn the_one_door_formats_what_is_there_and_adds_nothing() {
    let text = "▶ đang chạy — cargo test --offline\ntrong 🟪 [huba] · báo lại khi xong.";
    let from_log = vec![cmd("cargo test --offline"), cmd("rm -rf /tmp/cũ")];

    let kept = cmds_present_in(text, from_log.clone());
    assert_eq!(kept.len(), 1, "chỉ giữ lệnh có mặt trong tin: {kept:?}");
    assert_eq!(kept[0].line, "cargo test --offline");

    // Hậu quả thật: tin đi ra KHÔNG được mọc thêm khu chữ nào.
    let shown = render_session_data(
        text,
        &SessionData {
            sid: "abc12345".to_string(),
            cmds: kept.iter().map(|c| c.line.clone()).collect(),
            ..Default::default()
        },
    );
    // Nhãn của khu ấy đổi 2026-08-17 ("cổng quyền chặn" → "không thấy trên
    // màn"): huba không đo được NGUYÊN NHÂN vắng mặt, nên nó thôi đoán. Bài kiểm
    // bám vào phần nói được — có mọc thêm khu chữ hay không.
    assert!(
        !shown.contains("không thấy trên màn"),
        "ack không được mọc thêm danh sách lệnh: {shown}"
    );
    assert!(
        !shown.contains("rm -rf /tmp/cũ"),
        "lệnh của lượt khác không được lọt vào tin này: {shown}"
    );

    // Và đây là bằng chứng phép đo trên KHÔNG mù: bỏ phép lọc đi thì tin mọc
    // thêm đúng cái khu ấy.
    let unfiltered = render_session_data(
        text,
        &SessionData {
            sid: "abc12345".to_string(),
            cmds: from_log.iter().map(|c| c.line.clone()).collect(),
            ..Default::default()
        },
    );
    assert!(
        unfiltered.contains("không thấy trên màn"),
        "không bỏ lọc thì phải thấy khu chữ thừa — nếu không, phép đo trên vô nghĩa: {unfiltered}"
    );
}

/// Chữ trong ô nhập phải LUÔN có đường gửi nhanh — đây là bản chụp màn THẬT.
///
/// 🔴 Hà 2026-08-16, sau khi tôi gỡ hai nút ⏎/⌫ trống ở đáy: *"Lại mất nút gửi
/// nhanh gợi ý mờ rồi, làm cái nọ hỏng cái kia thế"*. Đúng: tôi tin *"đường
/// chèn giữa chữ vẫn còn"* mà không đo. Chuỗi dưới đây lấy nguyên văn từ
/// `hubd.err` lượt 14:34:40Z — chú ý dấu cách sau `❯` là **U+00A0**, không phải
/// dấu cách thường.
#[test]
fn text_in_the_input_box_always_gets_a_send_link() {
    // Mọi liên kết giữa chữ đi qua `deep_link`, thứ trả `None` khi chưa biết
    // tên bot — không khai thì bài kiểm này đỏ vì môi trường, không vì sản phẩm.
    huba::telegram::set_bot_username("hub_test_bot");
    let screen = "📷 Màn của 🟪 [huba]:\n\n\
        ───────────────────────\n\
        \u{276f}\u{a0}Bỏ hẳn trần cắt lệnh đi\n\
        ───────────────────────\n\
        \u{23f5}\u{23f5} auto mode on (shift+tab to cycle) · ← 1 agent";
    let shown = render_session_data(
        screen,
        &SessionData {
            sid: "abc12345".to_string(),
            ..Default::default()
        },
    );
    assert!(
        shown.contains("⏎"),
        "chữ trong ô nhập phải có đường gửi nhanh ngay tại dòng của nó: {shown}"
    );
    // 🔄 ĐẢO CHIỀU 2026-08-25 — Hà: *"nút xóa ô nhập không cần thiết vì có lệnh
    // xóa rồi"*. Chủ đề bài kiểm này là câu Hà kêu 16/08 (*"Lại mất nút gửi"*),
    // và `⏎` ở trên đã đo trọn nó. Đường xoá vẫn còn nguyên bằng lệnh gõ
    // (`verbs.rs` vẫn nhận `clr_`), chỉ cái đích chạm cạnh nút GỬI là đi.
    assert!(
        !shown.contains("xoá ô nhập"),
        "đường xoá ô mọc lại cạnh nút gửi: {shown}"
    );
}

/// Ký hiệu của một lựa chọn nằm TRƯỚC chữ, và nằm TRONG thẻ `<a>`.
///
/// 🔴 Hà 2026-08-17, ảnh một tin tự phát có bốn lựa chọn và bốn nút `☐ 1 Khô`…
/// ở đáy: *"Sao không chèn icon thẳng vào các lựa chọn mà chèn phía dưới"* →
/// *"Chèn phía trước số mỗi dòng"*. Mắt chạy dọc cột ấy để chọn.
///
/// 🔴 VÀ NÓ ĐÃ ĐỔI HÌNH DẠNG 2026-08-27 (`a8b3dd4`), Hà: *"Đích chạm của một
/// lựa chọn to bằng 1 ký tự — mắt thấy, ngón tay không trúng"*. Từ lượt ấy neo
/// của một dòng lựa chọn được coi là **trọn dòng** (số thứ tự bị bóc ra khi so),
/// nên icon đi VÀO TRONG thẻ cùng với chữ: `1. <a …>☑ Không xoá gì</a>`. Đích
/// chạm to bằng cả nhãn thay vì một ký tự.
///
/// ⚠ Bài kiểm này **nằm đỏ từ 27/08 tới 30/08** vì nó còn khoá hình dạng cũ
/// (`☑` phải đứng trước chuỗi `1.`) — và không ai thấy, vì lượt 27/08 chỉ chạy
/// "10 suite vùng ảnh hưởng". Cùng ngày, cùng lý do với con `ctrl-c` trong
/// `commands.rs`. Nay nó khoá điều CÒN ĐÚNG ở cả hai lượt: icon đứng trước CHỮ,
/// và cả nhãn là đích chạm.
#[test]
fn a_choice_gets_its_tick_before_the_number() {
    huba::telegram::set_bot_username("hub_test_bot");
    let text = "⚠ [dwork] dừng lại HỎI — Xoá gì\n\n1. Không xoá gì\n2. Rác build";
    let shown = render_session_data(
        text,
        &SessionData {
            sid: "abc12345".to_string(),
            // Mã `"1"`/`"2"` = hộp MỘT câu (đi bằng `k_`). Bảng nhiều câu mang
            // mã `"1.2"` và đi bằng `pick_` — xem `session_layout`.
            choices: vec![
                ("1".to_string(), "Không xoá gì".to_string()),
                ("2".to_string(), "Rác build".to_string()),
            ],
            ..Default::default()
        },
    );
    let mut cham = 0;
    for line in shown.lines().filter(|l| l.contains("Không xoá gì")) {
        cham += 1;
        let tick = line.find('☑').expect("phải có ☑");
        let chu = line.find("Không xoá gì").expect("phải còn nhãn");
        assert!(tick < chu, "☑ phải đứng TRƯỚC chữ: {line}");
        assert!(
            line.find("<a ").is_some_and(|a| a < tick),
            "☑ phải nằm TRONG thẻ <a> — để ngoài là dựng lại đúng cái đích chạm \
             một ký tự mà 27/08 vừa bỏ: {line}"
        );
        let dong = line.find("</a>").expect("thẻ phải đóng");
        assert!(chu < dong, "cả nhãn phải nằm trong đích chạm: {line}");
        assert!(
            line.contains("1."),
            "số thứ tự vẫn phải còn trên dòng: {line}"
        );
    }
    // MẪU SỐ: không dòng nào khớp thì cả vòng `for` ở trên không chấm gì, và bài
    // kiểm xanh vì nó không chạy — đúng dạng phép đo mù.
    assert_eq!(
        cham, 1,
        "phải chấm đúng một dòng lựa chọn, chấm được {cham}"
    );
}

/// 🖥 trả về KẾT QUẢ của lệnh vừa gõ, không trả cả màn hình có sẵn từ trước.
#[test]
fn the_terminal_button_reports_only_what_the_command_printed() {
    let screen = "\
Last login: Sat Aug 16 18:00:00 on ttys009
~ % ls
cũ.txt
~ % cargo test --offline
test result: ok. 359 passed
~ %";
    let out = tail_after_command(screen, "cargo test --offline");
    assert!(out.contains("359 passed"), "phải có kết quả: {out}");
    assert!(
        !out.contains("cũ.txt"),
        "không lấy thứ có trên màn TRƯỚC khi lệnh chạy: {out}"
    );
    assert!(!out.contains("Last login"), "không lấy cả màn: {out}");
}

/// Không thấy dòng lệnh (màn đã cuộn, lệnh bị bẻ đôi) thì trả cả khúc đang có.
///
/// Trả chuỗi rỗng ở đây là nói dối bằng im lặng: người đọc hiểu thành "lệnh
/// chạy xong và không in ra gì", trong khi sự thật là huba không định vị được.
#[test]
fn a_command_line_that_scrolled_away_still_reports_something() {
    let screen = "dòng một\ndòng hai\nxong rồi";
    let out = tail_after_command(screen, "một lệnh không có trên màn");
    assert_eq!(out, "dòng một\ndòng hai\nxong rồi");
}
