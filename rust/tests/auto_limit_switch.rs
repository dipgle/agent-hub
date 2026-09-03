//! Hết hạn mức thì huba TỰ chuyển tài khoản — và cái cổng ấy phải đổi được màu.
//!
//! 🔴 Hà 2026-09-01: *"Tại sao acc bị limit không tự chuyển mà bắt tôi gõ lệnh,
//! sao không tạo lệnh để vào phiên đó chủ động gõ để chuyển"*.
//!
//! Trạng thái trước lượt vá, đo được: `keys::session_limit_on_screen` đọc ra
//! dòng chặn, `watch::suggest_account` chọn được tài khoản còn chỗ,
//! `sessions::start_fresh_after_handover` mở được cửa sổ bằng tài khoản khác,
//! `sessions::handover_from_journal` dựng được bản bàn giao **không tốn hạn
//! mức** — nhưng KHÔNG có đường nào nối chúng lại. Đường tự động duy nhất
//! (`pipeline::auto_handover`) vào bằng cửa **% ngữ cảnh** và chưa bao giờ đọc
//! `LiveSession.limited`. Thiếu một cái cò, không thiếu cơ chế.
//!
//! Tệp này giữ cái cò ấy, và giữ theo luật 13①: **mỗi cổng phải có đối chứng
//! ngược**. Cấy ca hỏng ⇒ phải nổ; cấy ca lành ⇒ phải KHÔNG nổ. Vì thế cửa
//! *"phiên có bị chặn không"* nằm TRONG `auto_limit_why` chứ không nằm ở chỗ
//! gọi — một cổng chỉ là phép đo khi chính nó nói được cả câu "không".

use huba::config::AutoHandoverCfg;
use huba::keys::session_limit_on_screen;
use huba::pipeline::{auto_limit_why, minutes_until_reset, LimitWhy};

/// Ca thật, nguyên văn dòng đo được trên máy này 2026-08-28.
const DONG_CHAN: &str = "You've hit your session limit · resets 10:30pm (Asia/Saigon)";

/// Hạn mức TUẦN — dạng không đọc ra phút nào, và là ca phải chuyển nhất.
const KHI_TUAN: &str = "resets Sep 1";

/// Tuổi phiên đủ để qua phanh chống dây chuyền (`AUTO_LIMIT_MIN_AGE_SEC` = 600s).
const DU_TUOI: u64 = 3600;

// ───────────────────────── cái cò: cấy ca hỏng thì phải NỔ ─────────────────

#[test]
fn a_blocked_session_with_somewhere_to_go_switches() {
    assert_eq!(
        auto_limit_why(Some(KHI_TUAN), false, DU_TUOI, true, Some("acc2"), None, 30),
        LimitWhy::Do,
        "phiên bị chặn, có cửa sổ, có tài khoản để sang — mà vẫn không chuyển"
    );
}

/// Hạn mức tuần không đọc ra phút nào, và **đó chính là ca phải chuyển**: chờ
/// đồng hồ ở đây là chờ nhiều ngày. Fail-closed đúng chỗ này là để phiên chết.
#[test]
fn an_unreadable_clock_does_not_block_the_switch() {
    assert_eq!(
        auto_limit_why(Some(KHI_TUAN), false, DU_TUOI, true, Some("acc2"), None, 30),
        LimitWhy::Do
    );
}

// ───────────────────────── đối chứng ngược: ca lành thì KHÔNG nổ ───────────

/// Đối chứng ngược của cả tính năng. Không có nó thì con số của cổng là lời đồn.
#[test]
fn a_healthy_session_never_fires() {
    assert_eq!(
        auto_limit_why(None, false, DU_TUOI, true, Some("acc2"), None, 30),
        LimitWhy::NotLimited,
        "phiên KHÔNG bị chặn mà cổng vẫn đòi chuyển tài khoản"
    );
    // Chuỗi rỗng cũng là "không bị chặn": một `Some("")` lọt qua sẽ mở một cửa
    // sổ mới cho một phiên hoàn toàn khoẻ mạnh.
    assert_eq!(
        auto_limit_why(Some("   "), false, DU_TUOI, true, Some("acc2"), None, 30),
        LimitWhy::NotLimited
    );
}

/// …và cái đối chứng ấy phải đúng từ tầng ĐỌC MÀN, không chỉ tầng quyết định.
#[test]
fn a_clean_screen_reads_as_not_limited() {
    let man_lanh = "⏺ đang chạy cargo test\n  Bash(cargo test)\n> ";
    assert_eq!(session_limit_on_screen(man_lanh), None);
    assert_eq!(
        auto_limit_why(
            session_limit_on_screen(man_lanh).as_deref(),
            false,
            DU_TUOI,
            true,
            Some("acc2"),
            None,
            30
        ),
        LimitWhy::NotLimited
    );
    // …và màn có dòng chặn thì đi tới `Do`, cùng một đường, không sửa gì.
    let khi = session_limit_on_screen(DONG_CHAN).expect("không đọc ra dòng chặn");
    assert_eq!(khi, "resets 10:30pm (Asia/Saigon)");
    assert_eq!(
        auto_limit_why(
            Some(&khi),
            false,
            DU_TUOI,
            true,
            Some("acc2"),
            Some(300),
            30
        ),
        LimitWhy::Do
    );
}

// ───────────────────────── ba cái phanh ────────────────────────────────────

/// Phanh 1: không có tài khoản nào để sang thì **không làm gì**, và nói ra.
/// `suggest_account` trả `None` là một câu trả lời, không phải chỗ trống để đoán
/// — bịa một cái tên là mở nhầm cả kho phiên (`sessions::account_launch`).
#[test]
fn no_account_to_move_to_means_hold_not_guess() {
    assert_eq!(
        auto_limit_why(Some(KHI_TUAN), false, DU_TUOI, true, None, None, 30),
        LimitWhy::NoAccount
    );
    assert_eq!(
        auto_limit_why(Some(KHI_TUAN), false, DU_TUOI, true, Some(""), None, 30),
        LimitWhy::NoAccount
    );
}

/// Phanh 2: đồng hồ sắp mở lại thì chờ — thay cửa sổ để đổi lấy 5 phút là lỗ.
#[test]
fn a_clock_about_to_reset_is_worth_waiting_for() {
    assert_eq!(
        auto_limit_why(
            Some(KHI_TUAN),
            false,
            DU_TUOI,
            true,
            Some("acc2"),
            Some(5),
            30
        ),
        LimitWhy::ResetsSoon(5)
    );
    // …nhưng đúng ở MỐC thì đi tiếp, không có vùng xám nào ở giữa.
    assert_eq!(
        auto_limit_why(
            Some(KHI_TUAN),
            false,
            DU_TUOI,
            true,
            Some("acc2"),
            Some(30),
            30
        ),
        LimitWhy::Do
    );
    // Ngưỡng 0 = tắt phanh này; phải thật sự tắt, không kẹt ở `ResetsSoon(0)`.
    assert_eq!(
        auto_limit_why(
            Some(KHI_TUAN),
            false,
            DU_TUOI,
            true,
            Some("acc2"),
            Some(0),
            0
        ),
        LimitWhy::Do
    );
}

/// 🔴 Ca này KHÔNG nghĩ ra được ở bàn giấy — nó do **lượt chạy thật đầu tiên**
/// lôi ra (2026-09-01 02:48 giờ máy): phiên `[dwork/A-DDRIVE]` mang
/// `resets 1:40am`, mốc ấy đã qua 70 phút, mà đồng hồ 12 giờ đọc lên thành
/// **1372 phút nữa** — nên cửa `ResetsSoon` mở toang và huba đổi một phiên 67%
/// ngữ cảnh lấy một bản tóm thô, trong khi hạn mức của nó đã tự mở.
///
/// Cái phân biệt được hai ca nằm ở hình dạng sản phẩm, không nằm trong dòng chữ:
/// cửa sổ hạn mức phiên là 5 giờ ⟹ mốc dạng đồng hồ không bao giờ xa hơn thế.
#[test]
fn a_reset_clock_that_already_passed_means_the_limit_lifted() {
    // Nguyên văn số đo tối ấy.
    assert_eq!(
        minutes_until_reset("resets 1:40am (Asia/Saigon)", 2 * 60 + 48),
        Some(1372)
    );
    assert_eq!(
        auto_limit_why(
            Some("resets 1:40am (Asia/Saigon)"),
            false,
            DU_TUOI,
            true,
            Some("acc1"),
            Some(1372),
            30
        ),
        LimitWhy::ResetAlreadyPassed(1372),
        "mốc đã qua mà vẫn chuyển — đúng lỗi đã trả giá bằng một phiên thật"
    );
    // Sát trần thì vẫn là mốc THẬT ⟹ chuyển. Không có vùng xám.
    assert_eq!(
        auto_limit_why(
            Some(KHI_TUAN),
            false,
            DU_TUOI,
            true,
            Some("acc1"),
            Some(360),
            30
        ),
        LimitWhy::Do
    );
    assert_eq!(
        auto_limit_why(
            Some(KHI_TUAN),
            false,
            DU_TUOI,
            true,
            Some("acc1"),
            Some(361),
            30
        ),
        LimitWhy::ResetAlreadyPassed(361)
    );
}

/// 🔴 DÂY CHUYỀN — ca đắt nhất buổi hôm ấy, và nó là một vòng lặp KÍN chứ không
/// phải một lượt lỡ tay. Đo trên log thật:
///
/// ```text
/// 19:50:21  mở phiên 34f57a63 (acc3 → acc2) mang bản bàn giao từ nhật ký
/// 19:52:39  auto_limit_firing session=34f57a63  khi="resets 11:50pm"
/// 19:52:42  mở tiếp phiên 3360dcc9 (acc2 → acc3)
/// ```
///
/// Gốc: `handover_from_journal` chép NGUYÊN VĂN lượt nói cuối, mà lượt nói cuối
/// của một phiên bị chặn chính là dòng `You've hit your session limit · …` —
/// ngắn dưới 120 ký tự nên nó qua cửa hình dạng của `session_limit_on_screen`.
/// Bản bàn giao vừa dán vào cửa sổ mới là phiên mới đọc lên "đang bị chặn".
///
/// Sổ `auto_limit:done` KHÔNG đỡ được, và đó là phần đáng nhớ: mỗi vòng là một
/// **id phiên mới**, nên cuốn sổ trả lời đúng câu của nó mà dây chuyền vẫn chạy.
/// Cái chặn được là TUỔI.
#[test]
fn a_freshly_opened_session_is_never_switched_again() {
    // Đúng hình dạng ca thật: phiên 138 giây tuổi, mang trên màn dòng chặn nó
    // vừa chép từ bản bàn giao của phiên trước.
    assert_eq!(
        auto_limit_why(
            Some("resets 11:50pm (Asia/Saigon)"),
            false,
            138,
            true,
            Some("acc3"),
            Some(60),
            30
        ),
        LimitWhy::TooYoung(138),
        "phiên vừa sinh ra mà đã bị chuyển tiếp — đúng dây chuyền 19:50→19:52"
    );
    // Đủ tuổi thì cửa này mở, không thì phanh biến thành một bức tường.
    assert_eq!(
        auto_limit_why(
            Some("resets 11:50pm (Asia/Saigon)"),
            false,
            601,
            true,
            Some("acc3"),
            Some(60),
            30
        ),
        LimitWhy::Do
    );
}

/// Phanh 4 nằm ở sổ `auto_limit:done` — một lượt chuyển rồi thì thôi, không thì
/// vòng sau mở cửa sổ thứ hai cho cùng một việc.
#[test]
fn a_session_already_moved_is_not_moved_twice() {
    assert_eq!(
        auto_limit_why(Some(KHI_TUAN), true, DU_TUOI, true, Some("acc2"), None, 30),
        LimitWhy::AlreadyDone
    );
}

/// Không có cửa sổ thật thì không có gì để đóng và không ai gõ được vào.
/// (`sessions::is_real_tty` đã loại `""` · `??` · `-` — xem luật 11b.)
#[test]
fn a_session_without_a_real_window_is_left_alone() {
    assert_eq!(
        auto_limit_why(
            Some(KHI_TUAN),
            false,
            DU_TUOI,
            false,
            Some("acc2"),
            None,
            30
        ),
        LimitWhy::NoWindow
    );
}

/// Thứ tự các cửa là một mệnh đề, không phải tình cờ: lý do RẺ và CHẮC đứng
/// trước, để log ghi đúng nguyên nhân ĐẦU TIÊN chứ không phải cái cuối cùng.
#[test]
fn the_cheapest_reason_wins() {
    // Vừa không bị chặn, vừa đã chuyển, vừa không cửa sổ, vừa không tài khoản.
    assert_eq!(
        auto_limit_why(None, true, DU_TUOI, false, None, Some(1), 30),
        LimitWhy::NotLimited
    );
    assert_eq!(
        auto_limit_why(Some(KHI_TUAN), true, DU_TUOI, false, None, Some(1), 30),
        LimitWhy::AlreadyDone
    );
}

// ───────────────────────── đọc đồng hồ mở lại ──────────────────────────────

#[test]
fn the_reset_clock_reads_in_minutes() {
    let khi = "resets 10:30pm (Asia/Saigon)";
    // 22:00 → còn 30 phút.
    assert_eq!(minutes_until_reset(khi, 22 * 60), Some(30));
    // 21:00 → còn 90.
    assert_eq!(minutes_until_reset(khi, 21 * 60), Some(90));
    // 23:00 → đã qua hôm nay ⟹ vòng sang mai, 23 tiếng rưỡi.
    assert_eq!(minutes_until_reset(khi, 23 * 60), Some(23 * 60 + 30));
    // Đồng hồ 12 giờ, hai mốc dễ sai nhất.
    assert_eq!(minutes_until_reset("resets 12am", 23 * 60), Some(60));
    assert_eq!(minutes_until_reset("resets 12pm", 11 * 60), Some(60));
    // Không có phút thì mặc định :00.
    assert_eq!(minutes_until_reset("resets 3pm", 14 * 60), Some(60));
}

/// Dòng nào KHÔNG có đồng hồ đọc được thì phải trả `None` — và `None` dẫn tới
/// `Do`, tức chuyển tài khoản (xem [`an_unreadable_clock_does_not_block_the_switch`]).
///
/// 🔴 Tên hàm cũ là `a_timezone_name_is_not_a_clock` và nó KHOE MỘT CÔNG KHÔNG
/// CÓ THẬT. Bản đầu cắt chuỗi ở dấu `(` để `am` trong `America/New_York` không
/// bị đọc thành một giờ sáng; cấy lỗi bỏ lượt cắt ấy đi (2026-09-01) thì bài
/// kiểm **vẫn xanh** — dấu `(` luôn đứng trước chữ `am`, nên lát cắt nuốt theo
/// `" ("` và `parse` chết ở đó. Mutant tương đương ⟹ lượt cắt đã gỡ khỏi mã, và
/// bài kiểm ở lại với đúng cái nó chứng minh được: **hình dạng nào ra `None`**.
///
/// Đây là chỗ ba dòng dưới đáng giá hơn dòng múi giờ: `13pm` · `0am` · `10:99pm`
/// là những ca mà một bản vá "cho dễ đọc" sẽ vô tình nhận, và lúc ấy bài kiểm
/// này đỏ.
#[test]
fn a_line_without_a_readable_clock_reads_as_unknown() {
    assert_eq!(
        minutes_until_reset("resets 10:30 (America/New_York)", 600),
        None
    );
    // Cùng họ: dòng hạn mức TUẦN không có giờ nào để đọc.
    assert_eq!(minutes_until_reset(KHI_TUAN, 600), None);
    assert_eq!(minutes_until_reset("resets in a while", 600), None);
    // Giờ vô lý thì thú nhận không biết, đừng đoán tiếp.
    assert_eq!(minutes_until_reset("resets 13pm", 600), None);
    assert_eq!(minutes_until_reset("resets 0am", 600), None);
    assert_eq!(minutes_until_reset("resets 10:99pm", 600), None);
}

// ───────────────────────── cái cờ Hà xin ───────────────────────────────────

/// Hà xin *"có cờ tắt được trong config"* — nên nó phải TẮT được thật, và phải
/// độc lập với `enabled` (cờ ấy gác nhánh ngữ cảnh đầy, một câu hỏi khác hẳn).
#[test]
fn the_flag_can_actually_be_turned_off() {
    let mac_dinh = AutoHandoverCfg::default();
    assert!(
        mac_dinh.on_limit,
        "mặc định phải BẬT — đó là điều Hà yêu cầu"
    );
    let tat = AutoHandoverCfg {
        on_limit: false,
        ..AutoHandoverCfg::default()
    };
    assert!(!tat.on_limit);
    // Tắt nhánh ngữ cảnh đầy KHÔNG được tắt lây nhánh hạn mức.
    let tat_ngu_canh = AutoHandoverCfg {
        enabled: false,
        ..AutoHandoverCfg::default()
    };
    assert!(tat_ngu_canh.on_limit);
}

/// `huba.config.json` đang chạy trên máy KHÔNG có hai khoá mới. Nạp một cấu hình
/// cũ mà rơi về `false` thì tính năng chết lặng ngay lúc cài — `#[serde(default)]`
/// là thứ giữ nó, và một mặc định là một mệnh đề nên phải có bài kiểm.
#[test]
fn an_old_config_file_still_gets_the_new_defaults() {
    let cu: AutoHandoverCfg =
        serde_json::from_str(r#"{"enabled":true,"at_percent":70,"idle_sec":120}"#)
            .expect("cấu hình cũ phải nạp được");
    assert_eq!(cu.at_percent, 70, "phải giữ nguyên giá trị đã khai");
    assert!(
        cu.on_limit,
        "khoá thiếu phải rơi về BẬT, không phải tắt lặng"
    );
    assert_eq!(cu.on_limit_wait_min, 30);
}
