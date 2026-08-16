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

use hub::pipeline::{cmds_present_in, render_session_data, tail_after_command, SessionData};
use hub::sessions::Cmd;

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
    let text = "▶ đang chạy — cargo test --offline\ntrong 🟪 [hub] · báo lại khi xong.";
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
    assert!(
        !shown.contains("chạy không được"),
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
        unfiltered.contains("chạy không được"),
        "không bỏ lọc thì phải thấy khu chữ thừa — nếu không, phép đo trên vô nghĩa: {unfiltered}"
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
/// chạy xong và không in ra gì", trong khi sự thật là hub không định vị được.
#[test]
fn a_command_line_that_scrolled_away_still_reports_something() {
    let screen = "dòng một\ndòng hai\nxong rồi";
    let out = tail_after_command(screen, "một lệnh không có trên màn");
    assert_eq!(out, "dòng một\ndòng hai\nxong rồi");
}
