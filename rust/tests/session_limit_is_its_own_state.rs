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
use huba::pipeline::old_window_note;
use huba::sessions::{should_close_old_window, state_of, LiveSession, ST_LIMIT, ST_WAIT};

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

/// 🔴 Hà 2026-08-28, ngay lượt chuyển tài khoản THẬT đầu tiên (21:14): *"Thế này
/// thì đóng mất phiên rồi à"*.
///
/// Anh hỏi đúng chỗ. Luật "mở phiên mới rồi đóng phiên cũ" sinh ra cho ca ĐẦY
/// NGỮ CẢNH (12/08) — ở đó phiên cũ đã cạn, giữ lại vô nghĩa. Ca HẾT HẠN MỨC
/// ngược hẳn: phiên cũ còn tốt nguyên, chỉ bị một cái đồng hồ chặn, mà bản bàn
/// giao mang sang lại là bản THÔ dựng từ nhật ký. Đóng nó là vứt một cửa sổ đang
/// sống để đổi lấy không gì cả.
///
/// Bài kiểm đo **câu chủ máy đọc**, vì đó là chỗ duy nhất anh biết được cửa sổ
/// còn hay mất: `start_fresh_after_handover` mở cửa sổ thật nên không gọi được
/// ở đây.
#[test]
fn a_kept_window_is_announced_with_the_way_back() {
    let cau = old_window_note(true, "/Users/hanguyen/projects/huba", "93479f95");
    assert!(
        cau.contains("claude --resume 93479f95"),
        "giữ cửa sổ mà không đưa đường về thì chủ máy vẫn phải tự mò: {cau:?}"
    );
    assert!(
        cau.contains("/Users/hanguyen/projects/huba"),
        "`--resume` chạy sai thư mục là mở nhầm phiên — `cd` phải đi kèm: {cau:?}"
    );
    assert!(
        cau.contains("VẪN CÒN"),
        "phải nói thẳng cửa sổ còn đó, đừng bắt suy ra từ việc KHÔNG nói gì: {cau:?}"
    );
}

/// 🔴 LUẬT MỘT CÂU — Hà 2026-08-29: *"Sao lằng nhằng thế? Mỗi phiên làm một dự
/// án thì đóng làm gì trong khi việc chưa hết"*.
///
/// Trước đó tôi dựng một cây điều kiện theo CA (hết hạn mức thì giữ · sắp hết
/// thì đóng · đầy ngữ cảnh thì đóng) rồi mời anh chọn. Cây ấy phải nuôi mãi: ca
/// mới nào cũng đẻ thêm một nhánh, và mỗi nhánh là một chỗ để quên. Luật đúng
/// không hỏi *"ca nào"*, nó hỏi *"AI LÀM"* — và nó cũng chính là câu mã của tệp
/// này đã viết ra từ 13/08 mà chỉ áp cho một nhánh: *"Hai cửa sổ thì chủ máy
/// đóng bớt được; một phiên đang làm dở bị đóng thì không lấy lại được"*.
///
/// Lượt TỰ ĐỘNG vẫn dọn, và lý do ấy đo được chứ không phải phỏng đoán:
/// `handover_window_opened` chạy 5–14 lượt MỖI NGÀY (110 lượt từ 21/08) ⟹ không
/// dọn thì một tuần đọng ~50 cửa sổ Terminal.
#[test]
fn huba_only_closes_windows_it_opened_by_itself() {
    assert!(
        should_close_old_window(true, true),
        "lượt TỰ ĐỘNG mà không dọn ⟹ ~50 cửa sổ đọng lại sau một tuần"
    );
    assert!(
        !should_close_old_window(false, true),
        "CHỦ MÁY GÕ lượt này — cửa sổ của anh là của anh, đây đúng là ca Hà bác 29/08"
    );
}

/// ĐỐI CHỨNG NGƯỢC của chính luật ấy: chưa thấy phiên mới chào đời thì **không
/// bao giờ** đóng, kể cả lượt tự động. Đây là vế đã trả giá thật 2026-08-13
/// 04:31 — id rỗng mà vẫn đóng cửa sổ đang làm việc của chủ máy, tức phá cái
/// chắc chắn để đổi lấy cái chưa chứng minh, đúng lúc mù nhất.
#[test]
fn a_session_that_never_appeared_never_costs_the_old_window() {
    assert!(
        !should_close_old_window(true, false),
        "tự động + phiên mới chưa chào đời ⟹ vẫn phải GIỮ, đây là ca 13/08"
    );
    assert!(!should_close_old_window(false, false));
}

/// ĐỐI CHỨNG NGƯỢC (§13①). Cửa sổ ĐÃ đóng mà vẫn mời `--resume` ở đó thì tệ hơn
/// im lặng: chủ máy đi tìm một cửa sổ không còn.
///
/// Đây cũng là vế giữ cho bản vá là CỘNG THÊM — lượt `/handover -a acc` chạy
/// trót lọt (đóng cửa sổ cũ như xưa nay) phải đọc y hệt trước.
#[test]
fn a_closed_window_says_nothing_at_all() {
    assert_eq!(
        old_window_note(false, "/Users/hanguyen/projects/huba", "93479f95"),
        "",
        "cửa sổ đã đóng mà còn mời gõ tiếp ở đó ⟹ sai một cách im lặng"
    );
}
