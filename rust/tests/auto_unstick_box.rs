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

use huba::pipeline::{unstick_why, UnstickWhy};

/// Ngưỡng ổn định thật của mã (`STUCK_BOX_STABLE_SEC` = 18s, riêng tư trong
/// `pipeline.rs`) — chép lại đây, cùng cách `auto_limit_switch.rs` chọn số đủ
/// xa mốc thay vì import hằng số riêng của module.
const NGUONG: i64 = 18;

// ───────────────────────── cái cò: cấy ca hỏng thì phải NỔ ─────────────────

#[test]
fn text_stable_past_the_threshold_fires() {
    assert_eq!(
        unstick_why(true, false, 0, Some("nội dung đứng im"), NGUONG),
        UnstickWhy::Do,
        "đủ mọi điều kiện — cửa sổ thật, không phải huba, không hộp chọn, đứng \
         đủ lâu — mà vẫn không bấm"
    );
    assert_eq!(
        unstick_why(true, false, 0, Some("nội dung đứng im"), NGUONG + 100),
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
        unstick_why(true, false, 0, Some("đang gõ dở"), 0),
        UnstickWhy::TooYoung(0),
        "vừa thấy chữ lần đầu (0 giây ổn định) mà đã nổ thì không phân biệt \
         được người đang gõ với chữ kẹt thật"
    );
    assert_eq!(
        unstick_why(true, false, 0, Some("đang gõ dở"), NGUONG - 1),
        UnstickWhy::TooYoung(NGUONG - 1),
        "chưa đủ ngưỡng dù chỉ thiếu 1 giây thì vẫn phải chờ, không làm tròn"
    );
}

/// Hộp chọn đang mở thì CR là một cú CHỐT/BẬT-TẮT, không phải gửi — cùng luật
/// `type_and_send` đã áp cho lượt huba tự gõ.
#[test]
fn an_open_choice_dialog_never_fires() {
    assert_eq!(
        unstick_why(true, false, 3, Some("☐ lựa chọn nào đó"), NGUONG + 50),
        UnstickWhy::HasChoices,
        "hộp chọn đang mở mà vẫn bấm Enter là chốt nhầm một lựa chọn"
    );
}

/// Không có cửa sổ thật (`is_real_tty` == false) thì không có gì để bấm vào.
#[test]
fn no_real_window_never_fires() {
    assert_eq!(
        unstick_why(false, false, 0, Some("chữ gì đó"), NGUONG + 50),
        UnstickWhy::NotRealTty,
        "không có cửa sổ thật thì bấm vào hư vô"
    );
}

/// Máy móc của chính huba (phép dò `/usage`…) không được tự bấm vào — cùng
/// luật `is_hub_own_probe` đã áp cho tiếng chuông (`watch.rs`, luật 11b).
#[test]
fn hubs_own_probe_never_fires() {
    assert_eq!(
        unstick_why(true, true, 0, Some("chữ gì đó"), NGUONG + 50),
        UnstickWhy::HubOwnProbe,
        "máy móc của chính huba không phải một phiên của người"
    );
}

/// Ô nhập trống hoặc chỉ toàn khoảng trắng thì không có gì để gửi.
#[test]
fn an_empty_box_never_fires() {
    assert_eq!(unstick_why(true, false, 0, None, NGUONG + 50), UnstickWhy::NoText);
    assert_eq!(
        unstick_why(true, false, 0, Some("   "), NGUONG + 50),
        UnstickWhy::NoText,
        "toàn khoảng trắng cũng không phải một câu chờ gửi"
    );
}
