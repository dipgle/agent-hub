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

/// 🔴 ĐỐI CHỨNG NGƯỢC LẦN HAI — trần 120 ký tự ở trên KHÔNG đỡ được, và đây
/// không phải giả định: ba dòng dưới đây là hình dạng thật đã làm huba đóng sổ
/// nhầm, đo trên `logs/huba.log` đêm 2026-09-01.
///
/// Dòng đầu là dòng khớp lúc **22:05:13Z** — sổ ghi `auto_limit_firing` với
/// `session=5dac6ac5`, còn `khi` đọc ra đúng phần đuôi của nó. Nó là một dòng
/// **chú thích trong mã nguồn của chính bản vá**,
/// `rust/tests/auto_limit_switch.rs:182`, dài 76 ký tự nên lọt trần 120: một câu
/// TRÍCH bao giờ cũng ngắn hơn câu gốc nó trích.
///
/// Phiên bị đóng sổ hôm ấy chính là phiên đang viết bản vá cho lượt dây chuyền
/// 19:52 — nên nếu bài kiểm này đỏ trở lại, cái mất không phải một cửa sổ, mà
/// là buổi làm việc đang sửa đúng chỗ ấy.
#[test]
fn a_line_that_merely_quotes_the_banner_is_not_the_banner() {
    for trich in [
        // Khớp thật lúc 22:05:13Z — câu trích nằm GIỮA dòng.
        "/// của một phiên bị chặn chính là dòng `You've hit your session limit · …` —",
        // Mở đầu bằng chính câu ấy, nhưng trong dấu nháy ngược: đây là ca cửa
        // "mở đầu dòng" để lọt, và nó nằm sẵn trong `keys.rs`.
        "/// `You've hit your session limit · resets 10:30pm (Asia/Saigon)`",
        // Bản bàn giao dựng từ nhật ký kể lại sự cố.
        "chép nguyên văn dòng `You've hit your session limit`, phiên mới hiện dòng ấy",
    ] {
        assert!(
            session_limit_on_screen(trich).is_none(),
            "đây là một câu TRÍCH, không phải dòng CLI vẽ ra: {trich:?}"
        );
    }
}

/// 🔴 Cửa "MỞ ĐẦU DÒNG" phải TỰ ĐỨNG ĐƯỢC — và bài kiểm này sinh ra vì lượt đo
/// đầu tiên nói nó chưa đứng được.
///
/// Cấy lỗi lần một: nới cửa ấy về `contains("hit your")` ⟹ `RED_B_EXIT=0`,
/// **không bài nào đỏ**. Lý do đọc ra ngay: cả ba mẫu ở bài trên đều mang dấu
/// nháy ngược, nên cửa 2 bắt trọn và cửa 1 chưa một lần phải làm gì. Một cửa
/// không ai chứng minh được thì con số của nó là lời đồn (§13①) — nên đây là
/// mẫu **không có dấu nháy nào**, chỉ cửa 1 chặn nổi.
///
/// Hai mẫu đều là hình dạng THẬT: dòng dưới là đúng thứ
/// [`huba::sessions::handover_from_journal`] dựng ra (`**Phiên:** …` + lượt nói
/// nguyên văn), tức đúng thứ được dán vào cửa sổ vừa mở.
#[test]
fn prose_without_backticks_still_is_not_a_limit_line() {
    for van_xuoi in [
        "chép nguyên văn dòng You've hit your session limit rồi dán vào cửa sổ mới",
        "**Phiên:** acc3 báo You've hit your session limit nên tôi dựng bản bàn giao",
    ] {
        assert!(
            session_limit_on_screen(van_xuoi).is_none(),
            "câu ấy nằm GIỮA dòng ⟹ là lời kể, không phải dòng trạng thái: {van_xuoi:?}"
        );
    }
}

/// Cấy ca LÀNH thì phải KHÔNG đỏ (§13①): hai cửa mới không được che mất dòng
/// thật khi TUI chèn một dấu trang trí ở đầu.
#[test]
fn decoration_in_front_does_not_hide_a_real_limit_line() {
    let khi =
        session_limit_on_screen("· You've hit your session limit · resets 10:30pm (Asia/Saigon)")
            .expect("dấu trang trí đầu dòng không được che mất dòng thật");
    assert!(
        khi.contains("10:30pm"),
        "giờ mở lại phải đọc ra sau khi bỏ dấu trang trí, đọc ra: {khi:?}"
    );
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
    let cau = old_window_note(true, None, false, "/Users/hanguyen/projects/huba", "93479f95");
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

/// 🔴 LUẬT MỘT CÂU, VÀ NÓ ĐÃ ĐẢO CHIỀU HAI LẦN.
///
/// * **29/08** — Hà: *"Sao lằng nhằng thế? Mỗi phiên làm một dự án thì đóng làm
///   gì trong khi việc chưa hết"*. Tôi vừa dựng một cây điều kiện theo CA rồi
///   mời anh chọn; anh bác cả cây, và luật thành *"AI LÀM"* (`auto`).
/// * **30/08** — Hà: *"chuyển xong cửa sổ cũ không đóng được"*. Vế `auto` đi.
///   Nó hỏi *ai gõ*, trong khi điều đáng hỏi là *việc đã chuyển đi chưa* — mà
///   một lượt bàn giao thành công thì theo định nghĩa là việc đã sang phiên
///   mới, và cửa sổ cũ chỉ còn là một ngõ cụt trông y hệt một phiên đang sống.
///
/// Lý do dọn thì đo được và không đổi qua cả hai lần: `handover_window_opened`
/// chạy 5–14 lượt MỖI NGÀY (110 lượt từ 21/08) ⟹ không dọn thì một tuần đọng
/// ~50 cửa sổ Terminal.
#[test]
fn ban_giao_xong_thi_cua_so_cu_dong() {
    assert!(
        should_close_old_window(true),
        "phiên mới đã chào đời ⟹ cửa sổ cũ là ngõ cụt, đóng"
    );
}

/// ĐỐI CHỨNG NGƯỢC của chính luật ấy: chưa thấy phiên mới chào đời thì **không
/// bao giờ** đóng. Đây là vế đã trả giá thật 2026-08-13 04:31 — id rỗng mà vẫn
/// đóng cửa sổ đang làm việc của chủ máy, tức phá cái chắc chắn để đổi lấy cái
/// chưa chứng minh, đúng lúc mù nhất.
///
/// Không có bài này thì `should_close_old_window` trả `true` cứng cũng xanh, tức
/// cổng không đo gì cả.
#[test]
fn a_session_that_never_appeared_never_costs_the_old_window() {
    assert!(
        !should_close_old_window(false),
        "phiên mới chưa chào đời ⟹ vẫn phải GIỮ, đây là ca 13/08"
    );
}

/// ĐÓNG HỤT PHẢI ĐỌC KHÁC ĐÓNG-CÓ-CHỦ-Ý — thêm 2026-08-30 cùng lượt bỏ vế `auto`.
///
/// Từ nay cửa sổ cũ mặc định đóng, nên *"đã thử và không đóng được"* thành ca
/// thường gặp. Gộp nó vào câu *"giữ có chủ ý"* là để chủ máy tưởng huba cố tình
/// chừa cửa sổ lại, trong khi thật ra nó vừa thất bại — và anh sẽ không đi dọn.
///
/// Ca này có thật, đo được: 30/08 lúc 07:27 và 07:40, `/close` gõ `/exit` rồi
/// thấy `Busy` bảy lượt liền, `close_gave_up` sau 650 và 697 giây.
#[test]
fn dong_hut_thi_noi_thang_la_hut() {
    let cau = old_window_note(
        false,
        Some("sau 30 giây tab vẫn Busy"),
        false,
        "/Users/hanguyen/projects/huba",
        "93479f95",
    );
    assert!(
        cau.contains("CHƯA đóng được"),
        "phải nói thẳng là thất bại: {cau:?}"
    );
    assert!(
        cau.contains("sau 30 giây tab vẫn Busy"),
        "phải mang theo LÝ DO đo được, không phải một câu chung chung: {cau:?}"
    );
    assert!(
        cau.contains("claude --resume 93479f95"),
        "cửa sổ còn đó thì vẫn phải có đường về: {cau:?}"
    );
    assert!(
        !cau.contains("VẪN CÒN — chưa mất gì"),
        "đừng đọc như thể huba cố ý giữ: {cau:?}"
    );
}

/// ĐANG-THỬ-LẠI PHẢI ĐỌC KHÁC SANG-TAY-ANH — thêm 2026-09-03.
///
/// 🔴 Hà: *"Đợi rất lâu nhưng cửa sổ terminal cũ không đóng mặc dù cli thoát hết
/// rồi, đang ở dấu nhắc lệnh của terminal"*. Đọc nhật ký thì máy móc chạy đúng
/// từ đầu tới cuối và không tin nào thất lạc:
///
/// ```text
/// 16:37:53  handover_old_window_not_closed  → rồi NGAY SAU đó `remember_closing`
/// 16:37:54  Telegram 17779: "chưa đóng được … đóng tay, hoặc /close sau"
/// 16:40:07  close_done waited_sec=130
/// 16:40:08  Telegram 17781: "⏹ … cửa sổ đã đóng (chờ 130s)"
/// ```
///
/// Cái sai nằm ở tin 17779: huba sai chủ máy đi làm tay đúng việc nó đang làm.
/// Một câu báo mâu thuẫn với việc mình đang làm tệ hơn im lặng — im lặng chỉ
/// thiếu tin, câu này ĐIỀU một người đi làm việc thừa.
///
/// Bài kiểm giữ cả HAI vế trên cùng một `closed_err`, vì chỉ đổi mỗi cái cờ:
/// không đổi được câu thì cờ ấy chưa phải một phép đo (§13①).
#[test]
fn dang_thu_lai_thi_dung_sai_chu_may_di_dong_tay() {
    let ly_do = "sau 30 giây tab vẫn Busy";
    let thu_lai = old_window_note(
        false,
        Some(ly_do),
        true,
        "/Users/hanguyen/projects/huba",
        "93479f95",
    );
    let sang_tay = old_window_note(
        false,
        Some(ly_do),
        false,
        "/Users/hanguyen/projects/huba",
        "93479f95",
    );

    assert_ne!(
        thu_lai, sang_tay,
        "cùng lý do, chỉ khác `retrying`, mà câu y hệt ⟹ cái cờ ấy không đo gì cả"
    );
    // Vế ĐANG THỬ LẠI: nói rõ huba đang làm, và TUYỆT ĐỐI không sai đi dọn tay.
    assert!(
        thu_lai.contains("đang tự thử lại"),
        "phải nói ra là huba còn làm: {thu_lai:?}"
    );
    assert!(
        thu_lai.contains("không phải làm gì"),
        "phải nói thẳng chủ máy khỏi động tay — đây đúng chỗ tin 17779 nói ngược: {thu_lai:?}"
    );
    assert!(
        !thu_lai.contains("đóng tay"),
        "đúng con bug 03/09: đang thử lại mà vẫn mời đóng tay: {thu_lai:?}"
    );
    // Cả hai vế vẫn phải mang LÝ DO đo được và đường quay lại phiên.
    for (ten, cau) in [("thu_lai", &thu_lai), ("sang_tay", &sang_tay)] {
        assert!(cau.contains(ly_do), "{ten} mất lý do đo được: {cau:?}");
        assert!(
            cau.contains("claude --resume 93479f95"),
            "{ten}: cửa sổ còn đó thì vẫn phải có đường về: {cau:?}"
        );
        assert!(
            cau.contains("CHƯA đóng được"),
            "{ten}: vẫn phải nói thẳng là chưa xong: {cau:?}"
        );
    }
    // Vế SANG TAY giữ nguyên lời mời cũ — bản vá phải là CỘNG THÊM, không phải
    // đổi hành vi của ca đã đúng.
    assert!(
        sang_tay.contains("đóng tay"),
        "mất dấu cửa sổ thì đúng là việc của chủ máy: {sang_tay:?}"
    );
}

/// ĐỐI CHỨNG NGƯỢC (§13①). Cửa sổ ĐÃ đóng mà vẫn mời `--resume` ở đó thì tệ hơn
/// im lặng: chủ máy đi tìm một cửa sổ không còn.
///
/// Đây cũng là vế giữ cho bản vá là CỘNG THÊM — lượt `/handover -a acc` chạy
/// trót lọt (đóng cửa sổ cũ như xưa nay) phải đọc y hệt trước.
#[test]
fn a_closed_window_says_nothing_at_all() {
    assert_eq!(
        old_window_note(false, None, false, "/Users/hanguyen/projects/huba", "93479f95"),
        "",
        "cửa sổ đã đóng mà còn mời gõ tiếp ở đó ⟹ sai một cách im lặng"
    );
}
