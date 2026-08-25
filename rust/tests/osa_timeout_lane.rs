//! Trần `osascript` phải TUỲ ai đang chờ.
//!
//! 🔴 Hà 2026-08-25, ảnh tin mang `⚠ không đọc được màn: osascript quá 20s`:
//! *"Sao quá 20s lại không chụp được màn"*.
//!
//! Trần cũ là MỘT con số cho mọi lượt gọi, và lý do của nó — *"một cái treo sẽ
//! giữ cả vòng chạy của daemon"* — chỉ đúng cho vòng NỀN, nơi không ai đang
//! chờ. Với `/shot` thì chủ máy đang ngồi nhìn điện thoại, nên bỏ cuộc ở giây
//! thứ 20 là đem câu trả lời của anh đi đổi lấy một vòng quét chẳng có gì gấp.
//!
//! Đo được cái giá: **386 lượt `osascript quá 20s` trong một ngày** (log 23/08).

use huba::exec::{urgent, Lane};
use huba::keys::osa_timeout;

#[test]
fn the_background_sweep_keeps_its_short_ceiling() {
    assert_eq!(huba::exec::lane(), Lane::Background, "mặc định phải là nền");
    assert_eq!(osa_timeout().as_secs(), 20);
}

#[test]
fn a_waiting_person_gets_a_longer_ceiling() {
    let nen = osa_timeout();
    let _g = urgent();
    let gap = osa_timeout();
    assert!(
        gap > nen,
        "có người đang chờ mà trần vẫn bằng vòng nền ({}s) — bản vá không làm gì",
        nen.as_secs()
    );
    assert_eq!(gap.as_secs(), 45);
}

/// Guard rời tầm thì trần phải trở lại — nếu không, một cú bấm làm mọi vòng
/// nền sau đó cũng chờ lâu, và đúng cái lý do trần 20s tồn tại thì mất.
#[test]
fn the_ceiling_returns_to_normal_when_the_press_is_over() {
    {
        let _g = urgent();
        assert_eq!(osa_timeout().as_secs(), 45);
    }
    assert_eq!(osa_timeout().as_secs(), 20, "làn không trả về nền");
}
