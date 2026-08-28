//! Phiên hết hạn mức phải là một TRẠNG THÁI RIÊNG, huba tự nhận ra.
//!
//! 🔴 Hà 2026-08-28: *"Hiện tại đang có tk bị limit"* → *"Theo tôi hiểu là hub
//! tự kiểm soát khi bị limit thì xử lý luôn chứ?"*.
//!
//! Anh đúng, và chỗ tôi làm thiếu là chỗ căn bản: tôi dựng cái **cần gạt tay**
//! (`/handover -a acc1`) trước khi dựng cái **mắt**. Cần gạt chỉ dùng được khi
//! chủ máy ĐÃ BIẾT — mà biết là việc của huba.
//!
//! Trước bản vá này, một phiên hết hạn mức trông y hệt một phiên rảnh: nhật ký
//! đứng im, không lỗi, không hộp hỏi ⟹ `💤 đứng chờ`. huba nhìn thẳng vào nó mà
//! không thấy gì.
//!
//! Nguyên văn đo được trên máy này cùng ngày (acc3):
//! `You've hit your session limit · resets 10:30pm (Asia/Saigon)`

use huba::keys::session_limit_on_screen;
use huba::sessions::{state_of, LiveSession, ST_LIMIT, ST_WAIT};

const MAN_CHAN: &str = "\
⏺ Đang chạy bước cuối rồi tôi báo lại.

You've hit your session limit · resets 10:30pm (Asia/Saigon)
";

#[test]
fn the_limit_line_is_read_with_its_reset_time() {
    let khi = session_limit_on_screen(MAN_CHAN).expect("phải nhận ra dòng chặn");
    assert!(
        khi.contains("10:30pm"),
        "phải giữ GIỜ MỞ LẠI, không rút thành một bool: còn 10 phút thì chờ, còn 5 \
         tiếng thì chuyển tài khoản — hai việc khác nhau. Đọc ra: {khi:?}"
    );
}

/// 🔴 ĐỐI CHỨNG NGƯỢC, và nó không phải giả định: màn của CHÍNH phiên `[huba]`
/// hôm nay bàn về hạn mức suốt buổi. Bắt theo một chữ `limit` là dựng một cảnh
/// báo kêu oan, mà một cảnh báo kêu oan thì bị lướt qua — tệ hơn không có.
#[test]
fn prose_that_merely_talks_about_limits_is_not_a_limited_session() {
    for van_xuoi in [
        "⏺ acc3 đang hit your session limit nên tôi phải dựng bản bàn giao từ nhật ký, \
         rồi mở phiên mới bằng acc1 — đây là câu tôi KỂ LẠI sự cố, không phải dòng trạng thái.",
        "  · Bàn về session limit và cách xử lý khi tài khoản bị chặn",
        "⏺ Đang chạy bình thường, không có gì đặc biệt.",
    ] {
        assert!(
            session_limit_on_screen(van_xuoi).is_none(),
            "đoạn văn này KHÔNG phải một phiên bị chặn: {van_xuoi:?}"
        );
    }
}

/// Trạng thái phải ĐỔI ĐƯỢC (§13①) — và phải đổi sang một ký hiệu RIÊNG, không
/// mượn `💤 đứng chờ`: một phiên rảnh sẽ chạy tiếp khi được gõ, một phiên bị
/// chặn thì không, dù gõ gì.
#[test]
fn a_limited_session_no_longer_reads_as_idle() {
    let mut s = LiveSession {
        host: "terminal".to_string(),
        ..Default::default()
    };
    assert_eq!(
        state_of(&s).0,
        ST_WAIT,
        "chưa bị chặn thì vẫn là phiên đứng chờ — nếu không thì bài dưới xanh vô nghĩa"
    );
    s.limited = Some("resets 10:30pm (Asia/Saigon)".to_string());
    let (icon, chu) = state_of(&s);
    assert_eq!(icon, ST_LIMIT, "phải mang ký hiệu riêng, không phải 💤");
    assert!(
        chu.contains("HẠN MỨC"),
        "chữ phải nói ra vì sao nó đứng: {chu:?}"
    );
}

/// Bị chặn THẮNG cả `đang chạy`: một phiên vừa bị chặn có thể còn cờ `working`
/// từ lượt dở, và lúc ấy `⚡` nói *"cứ để đấy"* — đúng câu sai nhất có thể nói.
#[test]
fn being_blocked_outranks_looking_busy() {
    let s = LiveSession {
        host: "terminal".to_string(),
        working: true,
        limited: Some("resets 10:30pm".to_string()),
        ..Default::default()
    };
    assert_eq!(
        state_of(&s).0,
        ST_LIMIT,
        "`⚡ đang chạy` bảo chủ máy chờ, mà chờ thì không bao giờ xong — cái KẸT phải \
         thắng cái chạy, cùng luật với ❓ của ô mật khẩu"
    );
}
