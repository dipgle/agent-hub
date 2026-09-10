//! Chữ đứng im trong ô nhập vì một nguồn KHÔNG PHẢI huba — tự bấm Enter bù.
//!
//! 🔴 Hà 2026-09-08: phiên `[dwork/a-chung]` nhận `/btw` (`SendMessage` giữa
//! hai phiên Claude Code, tiêm thẳng qua chính CLI `claude`, không qua
//! `do script`/`cgkeys` của huba) rồi đứng im — Hà phải tự gõ `/enter`. Safety
//! net cũ (`keys::type_and_send`'s `still_in_box`) chỉ chạy NGAY SAU lượt
//! CHÍNH HUBA gõ, nên một nguồn khác để chữ lại thì không ai kiểm lại.
//!
//! Tệp này giữ cái cò mới (`pipeline::unstick_why`) theo đúng luật 13①: mỗi
//! cổng phải có đối chứng ngược — cấy ca hỏng ⇒ phải nổ (`Do`); cấy ca lành ⇒
//! phải KHÔNG nổ, và phải nói ĐÚNG vì sao.

use huba::pipeline::{
    limit_still_biting, stuck_next, unstick_done, unstick_why, StuckNext, UnstickDone, UnstickWhy,
};

/// Ngưỡng ổn định thật của mã (`STUCK_BOX_STABLE_SEC` = 18s, riêng tư trong
/// `pipeline.rs`) — chép lại đây, cùng cách `auto_limit_switch.rs` chọn số đủ
/// xa mốc thay vì import hằng số riêng của module.
const NGUONG: i64 = 18;

// ───────────────────────── cái cò: cấy ca hỏng thì phải NỔ ─────────────────

#[test]
fn text_stable_past_the_threshold_fires() {
    assert_eq!(
        unstick_why(true, false, false, 0, Some("nội dung đứng im"), NGUONG),
        UnstickWhy::Do,
        "đủ mọi điều kiện — cửa sổ thật, không phải huba, không hộp chọn, đứng \
         đủ lâu — mà vẫn không bấm"
    );
    assert_eq!(
        unstick_why(
            true,
            false,
            false,
            0,
            Some("nội dung đứng im"),
            NGUONG + 100
        ),
        UnstickWhy::Do,
        "đứng lâu HƠN ngưỡng vẫn phải nổ — không phải một cửa sổ thời gian hẹp"
    );
}

// ───────────────────────── đối chứng ngược: ca lành thì KHÔNG nổ ───────────

/// Đối chứng ngược quan trọng nhất: người đang GÕ DỞ thì nội dung chưa kịp
/// đứng im — bấm Enter vào giữa câu chưa xong là gửi hộ một câu chưa xong.
#[test]
fn still_typing_never_fires() {
    assert_eq!(
        unstick_why(true, false, false, 0, Some("đang gõ dở"), 0),
        UnstickWhy::TooYoung(0),
        "vừa thấy chữ lần đầu (0 giây ổn định) mà đã nổ thì không phân biệt \
         được người đang gõ với chữ kẹt thật"
    );
    assert_eq!(
        unstick_why(true, false, false, 0, Some("đang gõ dở"), NGUONG - 1),
        UnstickWhy::TooYoung(NGUONG - 1),
        "chưa đủ ngưỡng dù chỉ thiếu 1 giây thì vẫn phải chờ, không làm tròn"
    );
}

/// Hộp chọn đang mở thì CR là một cú CHỐT/BẬT-TẮT, không phải gửi — cùng luật
/// `type_and_send` đã áp cho lượt huba tự gõ.
#[test]
fn an_open_choice_dialog_never_fires() {
    assert_eq!(
        unstick_why(
            true,
            false,
            false,
            3,
            Some("☐ lựa chọn nào đó"),
            NGUONG + 50
        ),
        UnstickWhy::HasChoices,
        "hộp chọn đang mở mà vẫn bấm Enter là chốt nhầm một lựa chọn"
    );
}

/// Không có cửa sổ thật (`is_real_tty` == false) thì không có gì để bấm vào.
#[test]
fn no_real_window_never_fires() {
    assert_eq!(
        unstick_why(false, false, false, 0, Some("chữ gì đó"), NGUONG + 50),
        UnstickWhy::NotRealTty,
        "không có cửa sổ thật thì bấm vào hư vô"
    );
}

/// Máy móc của chính huba (phép dò `/usage`…) không được tự bấm vào — cùng
/// luật `is_hub_own_probe` đã áp cho tiếng chuông (`watch.rs`, luật 11b).
#[test]
fn hubs_own_probe_never_fires() {
    assert_eq!(
        unstick_why(true, true, false, 0, Some("chữ gì đó"), NGUONG + 50),
        UnstickWhy::HubOwnProbe,
        "máy móc của chính huba không phải một phiên của người"
    );
}

/// Phiên đang bị chặn hạn mức thì CLI không nhận input — bấm Enter là bắn vào
/// chỗ ĐÃ BIẾT TRƯỚC là không phản hồi.
///
/// 🔴 Đo trên máy thật 2026-09-09, đêm bản vá "đọc lại ô nhập" lên daemon: 27
/// lượt bấm, **26** lượt đọc lại thấy chữ vẫn nằm nguyên, đúng **1** lượt đi
/// được. Mở màn một trong số ấy (`projects-0b`, cửa sổ 9783) thì thấy ngay
/// *"You've hit your session limit · resets 9:10pm"* nằm ngay trên ô nhập, còn
/// trong ô là câu 30 ký tự — khớp `text_len` trong log. Trạng thái ấy huba đã
/// đọc sẵn (`LiveSession::limited`) và còn in ra mỗi ~2 phút cho CHÍNH phiên ấy
/// (`auto_limit_held`); chỉ mỗi cái cò này chưa hỏi.
#[test]
fn a_rate_limited_session_never_fires() {
    assert_eq!(
        unstick_why(
            true,
            false,
            true,
            0,
            Some("tiếp tục quét việc và làm tiếp"),
            NGUONG + 50
        ),
        UnstickWhy::Limited,
        "màn khai phiên bị chặn hạn mức mà vẫn bấm Enter thì lượt nào cũng hụt"
    );
}

/// Đối chứng ngược của chính cái phanh trên: bỏ MỖI cờ `limited` ra thì cùng
/// bộ đầu vào ấy phải nổ trở lại. Thiếu ca này thì một cái phanh chặn tất cũng
/// làm bài kiểm trên xanh y hệt.
#[test]
fn the_same_box_fires_when_the_session_is_not_limited() {
    assert_eq!(
        unstick_why(
            true,
            false,
            false,
            0,
            Some("tiếp tục quét việc và làm tiếp"),
            NGUONG + 50
        ),
        UnstickWhy::Do,
        "không bị chặn hạn mức thì đúng bộ đầu vào ấy vẫn phải bấm"
    );
}

// ────────── dòng banner CÒN CẮN hay chỉ là vết cũ: `limit_still_biting` ──────
//
// 🔴 Cái cổng ở trên chỉ đúng khi câu hỏi *"phiên có đang bị chặn không"* được
// trả lời đúng. Lấy `limited.is_some()` làm câu trả lời là sai, và sai theo
// hướng tệ nhất: đo 2026-09-09, phiên `702acdc7` mang `resets 9:10pm` mà tới
// **21:46 giờ máy** — 36 phút SAU mốc ấy — `limited` vẫn `Some`, vì phiên bị
// chặn thì không in thêm gì nên banner nằm nguyên dưới đáy màn. Cổng đọc theo
// sự CÓ MẶT của dòng chữ sẽ khoá đúng vào phút hạn mức tự mở lại.

/// Mốc mở lại còn ở phía trước ⟹ còn cắn thật.
#[test]
fn a_reset_mark_still_ahead_is_still_biting() {
    assert!(
        limit_still_biting(Some("resets 9:10pm (Asia/Saigon)"), 20 * 60),
        "20:00 mà mốc mở lại là 21:10 thì còn 70 phút nữa — vẫn đang bị chặn"
    );
}

/// Đối chứng ngược, và là CHÍNH ca đo được: cùng dòng chữ ấy, đọc lúc 21:46 thì
/// mốc 21:10 đã qua ⟹ vết cũ, không được coi là còn bị chặn.
#[test]
fn a_reset_mark_already_passed_is_only_an_old_line() {
    assert!(
        !limit_still_biting(Some("resets 9:10pm (Asia/Saigon)"), 21 * 60 + 46),
        "mốc đã qua 36 phút mà vẫn đọc thành 'đang bị chặn' thì huba thôi bấm \
         đúng lúc phiên gõ được trở lại"
    );
}

/// Hạn mức TUẦN in NGÀY (`resets Sep 1`), không in đồng hồ ⟹ `minutes_until_reset`
/// trả `None`. Fail-closed: coi như còn cắn, vì nó còn cắn thật, hàng ngày.
#[test]
fn a_weekly_limit_has_no_clock_and_stays_biting() {
    assert!(limit_still_biting(Some("resets Sep 1"), 12 * 60));
}

/// Không có dòng nào (hoặc dòng rỗng) ⟹ không bị chặn. Ca này phải ĐÚNG, nếu
/// không thì mọi phiên bình thường đều bị cái phanh mới khoá lại.
#[test]
fn no_limit_line_means_not_limited() {
    assert!(!limit_still_biting(None, 12 * 60));
    assert!(!limit_still_biting(Some("   "), 12 * 60));
}

/// Ô nhập trống hoặc chỉ toàn khoảng trắng thì không có gì để gửi.
#[test]
fn an_empty_box_never_fires() {
    assert_eq!(
        unstick_why(true, false, false, 0, None, NGUONG + 50),
        UnstickWhy::NoText
    );
    assert_eq!(
        unstick_why(true, false, false, 0, Some("   "), NGUONG + 50),
        UnstickWhy::NoText,
        "toàn khoảng trắng cũng không phải một câu chờ gửi"
    );
}

// ─────────────── phép kiểm SAU cú bấm: cú Enter ấy có đi được không ─────────
//
// 🔴 Vì sao có mục này, đo 2026-09-08 ngay lượt tính năng chạy thật đầu tiên:
// 25 lượt `auto_unstick_box_firing` trong 11 phút vào 7 phiên,
// `auto_unstick_box_failed` **0 lần** — mà đọc thẳng màn ba cửa sổ ấy
// (`contents of selected tab`, đúng đường `keys::screen_text` đi) thì chữ vẫn
// nằm nguyên si trong ô: `projects-3d` 18 ký tự, `projects-fe` 24, `projects-ef`
// 55, khớp từng con số với `text_len` đã ghi trong log.
//
// Tức "trúng" và "hụt" cho ra CÙNG một dòng log, vì đường thất bại duy nhất là
// `osascript` trả lỗi — mà `osascript` trả 0 ngay khi byte tới được tab, nó
// không biết TUI có nhận byte ấy như một phím hay không. Một tín hiệu không bao
// giờ ở trạng thái ngược lại thì không phải phép đo (`CLAUDE.md` §13).

/// Cấy ca HỎNG: đọc lại thấy đúng chữ ấy còn nguyên ⇒ phải là `StillThere`.
/// Đây là ca đã xảy ra thật, nên nó là đối chứng ngược của cả tính năng.
#[test]
fn text_still_sitting_in_the_box_is_not_sent() {
    assert_eq!(
        unstick_done("tiếp tục quét việc", Some("tiếp tục quét việc")),
        UnstickDone::StillThere,
        "chữ còn nguyên trong ô mà đọc thành đã gửi — đúng lỗi 2026-09-08"
    );
    // TUI vẽ lại đổi phần đệm hai bên; một dấu cách không được biến "còn kẹt"
    // thành "đã đi".
    assert_eq!(
        unstick_done("tiếp tục quét việc", Some("  tiếp tục quét việc  ")),
        UnstickDone::StillThere,
        "khoảng trắng hai đầu không phải một thay đổi"
    );
}

/// Cấy ca LÀNH: ô trống sau cú bấm ⇒ phải là `Sent`, cổng KHÔNG được đỏ.
#[test]
fn an_empty_box_after_the_press_means_sent() {
    assert_eq!(
        unstick_done("tiếp tục quét việc", None),
        UnstickDone::Sent,
        "ô đã trống thì cú Enter ấy đi được — không được báo hụt"
    );
}

/// Ô mang chữ KHÁC ⇒ chữ cũ đã đi. Thứ nằm đó bây giờ là quan sát mới, và vòng
/// quét sau tự tính lại `stable_sec` từ đầu cho nó.
#[test]
fn different_text_in_the_box_means_the_old_one_left() {
    assert_eq!(
        unstick_done("tiếp tục quét việc", Some("câu chủ máy vừa gõ")),
        UnstickDone::Sent,
        "chữ cũ không còn ở đó nữa — đừng đếm nó là một lượt bấm hụt"
    );
}

// ────────── sau cú Enter mà chữ y nguyên: chẩn đoán, không phải bắn tiếp ─────
//
// 🔴 Đo 09–10/09: 31 lượt bấm, 30 lượt chữ y nguyên. Không phải sai đường gửi —
// `do script` vẫn đẩy đúng một `^M` tới tty (đo bằng cửa sổ nháp chạy
// `cat -vet`, kể cả lúc màn hình khoá). Chữ ấy KHÔNG nằm trong ô nhập: ghi
// `xin chao` vào cửa sổ `projects-ef` thì lượt gửi đi là đúng `❯ xin chao`, còn
// câu 55 ký tự "nằm trong ô" từ 08/09 biến mất mà không được gửi, và nhật ký
// `45101666-….jsonl` không có nó ở bất kỳ lượt nhập nào ⟹ nó là GỢI Ý MỜ.

#[test]
fn an_unchanged_box_on_an_idle_session_is_a_ghost() {
    assert_eq!(
        stuck_next(None, false),
        StuckNext::StopGhost,
        "Enter đã đo xong: không bị chặn hạn mức, không đang chạy, mà màn không \
         đổi ⟹ ô rỗng thật. Bắn tiếp là bắn vào chỗ không có gì để gửi"
    );
}

/// Đối chứng ngược thứ nhất: phiên ĐANG CHẠY thì "màn không đổi" chưa nói được
/// gì — TUI đang vẽ dở, chữ có thể đã vào hàng chờ. Ca này phải còn thử lại.
#[test]
fn a_working_session_still_gets_its_retries() {
    assert_eq!(stuck_next(None, true), StuckNext::Retry);
}

/// Đối chứng ngược thứ hai: bị chặn hạn mức là một chẩn đoán KHÁC (và cổng
/// `UnstickWhy::Limited` đã chặn từ trước) — không được đọc thành gợi ý mờ.
#[test]
fn a_limited_session_is_not_a_ghost() {
    assert_eq!(
        stuck_next(Some("resets 10:30pm (Asia/Saigon)"), false),
        StuckNext::Retry
    );
    assert_eq!(stuck_next(Some("resets Sep 1"), true), StuckNext::Retry);
}

/// Và ba đầu vào ấy phải cho ra ĐÚNG hai kết cục phân biệt được — một hàm trả
/// hằng số làm bài đầu xanh y hệt.
#[test]
fn the_ghost_call_is_not_a_constant() {
    assert_ne!(stuck_next(None, false), stuck_next(None, true));
    assert_ne!(
        stuck_next(None, false),
        stuck_next(Some("resets 1pm"), false)
    );
}
