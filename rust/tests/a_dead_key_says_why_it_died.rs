//! Một phím không ăn: nói điều BIẾT CHẮC trước, phỏng đoán sau.
//!
//! 🔴 Hà 2026-08-30, dán nguyên tin huba vừa gửi rồi hỏi *"thứ tự bấm ở gợi ý bị
//! ngược à?"*:
//!
//! > ⚠ đã bấm 'enter' rồi bấm → để nhận gợi ý nhưng màn KHÔNG đổi · 🟪 [dwork]…
//! > Chữ trong ô nhập nhiều khả năng là GỢI Ý MỜ của TUI … Muốn gửi câu ấy thì
//! > gõ thẳng nó ở đây.
//!
//! **Thứ tự không ngược** — `keys::ghost_verdict` chép lý do: Enter đi trước vì
//! nó là PHÉP ĐO (màn không đổi ⟹ ô rỗng thật), `→` chỉ an toàn sau bằng chứng
//! ấy. Đảo lại thì `→` nhận nốt gợi ý vào một ô đang có chữ thật chủ máy gõ dở,
//! và cú CR đi kèm gửi một câu dài hơn câu anh gõ.
//!
//! Cái sai là CÂU CHẨN ĐOÁN. Đo màn của chính phiên ấy cùng lúc (`ttys018`,
//! window 18153, 24×80, 1687 ký tự): ô nhập **rỗng**, không gợi ý nào; phiên
//! đang giữa lượt — `✻ Meandering… (34m 7s · ↓ 51.5k tokens)`, kẹt ở một
//! `git push`. huba nói ra một phỏng đoán trong khi nó đang cầm sẵn một sự thật.

use huba::keys::{no_effect_reason, NoEffect};

/// 🔴 ĐỐI CHỨNG NGƯỢC của cả tệp: bản cũ nói `Ghost` ở MỌI ca, nên bài này là
/// bài duy nhất phân biệt được hai bản. Bỏ nó đi thì cổng đo bằng 0.
#[test]
fn a_running_session_is_never_diagnosed_as_a_ghost_suggestion() {
    assert_eq!(
        no_effect_reason(None, true, Some("Meandering… 34m7s")),
        NoEffect::Busy(Some("Meandering… 34m7s".to_string())),
        "phiên đang chạy thì 'màn không đổi' chưa nói được gì — đồng hồ TUI nhích MỖI \
         GIÂY, hai lượt đọc sát nhau bằng nhau vì lý do không liên quan tới cú bấm"
    );
    assert_eq!(
        no_effect_reason(None, true, None),
        NoEffect::Busy(None),
        "không đọc được dòng việc thì vẫn là 'đang chạy' — thiếu một chi tiết không \
         biến nó thành một chẩn đoán khác"
    );
}

/// Hết hạn mức thắng tất cả: đây là ca DUY NHẤT vừa biết chắc vừa có đường đi
/// tiếp. Nói nó sau một phỏng đoán là để chủ máy đi sửa nhầm chỗ.
#[test]
fn a_limited_account_outranks_every_other_reading() {
    assert_eq!(
        no_effect_reason(Some("resets Sep 1 at 1pm"), true, Some("Crafting… 2m04s")),
        NoEffect::Limited("resets Sep 1 at 1pm".to_string()),
        "vừa `working` vừa bị chặn thì HẠN MỨC là câu đúng — cờ `working` còn lại từ \
         lượt dở, cùng lý lẽ với `state_of`"
    );
    assert_eq!(
        no_effect_reason(Some("resets 10:30pm"), false, None),
        NoEffect::Limited("resets 10:30pm".to_string())
    );
}

/// Và `Ghost` vẫn phải sống — nếu bản vá nuốt luôn nó thì chẩn đoán ĐÚNG của
/// 16/08 (Hà: *"phải bấm nút right trước thì nó mới điền text"*) biến mất, và
/// một bản vá xoá một tính năng cũ không phải một bản vá.
#[test]
fn an_idle_session_still_gets_the_ghost_diagnosis() {
    assert_eq!(
        no_effect_reason(None, false, None),
        NoEffect::Ghost,
        "phiên đứng rảnh, không bị chặn: đây mới là chỗ nghi gợi ý mờ"
    );
    assert_eq!(
        no_effect_reason(None, false, Some("Brewing… 1m02s")),
        NoEffect::Ghost,
        "còn sót dòng việc cũ nhưng KHÔNG `working` thì vẫn là ca gợi ý mờ — cờ \
         `working` là nguồn, không phải chuỗi hiển thị"
    );
}
