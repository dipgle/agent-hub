//! Trần `osascript` **tự giãn theo máy**, không gõ cứng.
//!
//! 🔴 Hà 2026-08-25, tối máy quá tải: *"sao giờ vào phiên nào cung báo quá 20s
//! ko xem được màn vậy?"* — mọi phiên cùng lúc, vì cả 11 phiên dùng CHUNG một
//! phép dò (`keys::tabs_script` quét mọi tab của mọi cửa sổ trong MỘT lượt gọi).
//!
//! ĐO ĐƯỢC đêm ấy: `load average` **41,5 trên máy 8 nhân** — quá tải gấp 5, do
//! mấy lượt `cargo test` của chính tôi cộng với Spotlight đi lập chỉ mục
//! `target/`. Ở mức ấy `osascript` không được cấp CPU trong 20 giây. Đo lại lúc
//! load 13: **chính kịch bản ấy mất 1,0 giây**. Không có gì hỏng — nó ĐÓI.
//!
//! Một con số gõ cứng không trả lời được *"máy đang bận tới đâu"*, vì câu ấy chỉ
//! đo được lúc chạy. Luật Hà đã chốt: vấn đề runtime thì tự điều chỉnh.

use huba::exec::Lane;
use huba::keys::osa_budget;
use std::time::Duration;

const MS: fn(u64) -> Duration = Duration::from_millis;
const S: fn(u64) -> Duration = Duration::from_secs;

/// Máy RẢNH thì trần đứng nguyên ở mức cũ — bản vá này chỉ được NỚI, không được
/// rút ngắn thứ đang chạy đúng.
#[test]
fn a_quiet_machine_keeps_the_old_ceilings() {
    // 0,3s là con số đo thật lúc máy rảnh.
    assert_eq!(osa_budget(Lane::Urgent, MS(300)), S(45));
    assert_eq!(osa_budget(Lane::Background, MS(300)), S(20));
    // Chưa có lượt nào thành công (mới khởi động) ⟹ vẫn là sàn, không phải 0.
    assert_eq!(osa_budget(Lane::Urgent, MS(0)), S(45));
    assert_eq!(osa_budget(Lane::Background, MS(0)), S(20));
}

/// 🔴 CA CHÍNH: máy tải nặng thì trần NỞ RA. Đây là thứ lẽ ra đã cứu được đêm
/// 25/08 — lượt đọc mất 10s thì trần 20s là quá sát, một nhịp tải là trượt.
#[test]
fn a_loaded_machine_widens_the_ceiling() {
    assert_eq!(osa_budget(Lane::Background, S(10)), S(60), "10s × 6 = 60s");
    assert_eq!(osa_budget(Lane::Urgent, S(10)), S(60));
    assert!(
        osa_budget(Lane::Background, S(5)) > S(20),
        "5s một lượt mà vẫn giữ trần 20s là tự dựng lại đúng cái bẫy vừa gỡ"
    );
}

/// 🔴 HÀNG RÀO NGƯỢC — phải có TRẦN CỨNG. Không thì một lượt chậm bất thường
/// đẩy trần lên vô hạn và huba ngồi chờ mãi một cửa sổ đã chết thật.
#[test]
fn one_freak_measurement_cannot_push_the_ceiling_to_infinity() {
    assert_eq!(osa_budget(Lane::Urgent, S(600)), S(180), "trần cứng 180s");
    assert_eq!(osa_budget(Lane::Background, S(600)), S(90), "trần cứng 90s");
    // Kể cả một con số vô lý.
    assert_eq!(osa_budget(Lane::Urgent, S(86_400)), S(180));
}

/// Vòng NỀN phải chặt hơn vòng có người chờ — một lượt treo ở đó giữ cả nhịp
/// quét của daemon, còn người đang chờ thì tự bỏ đi được.
#[test]
fn the_background_lane_stays_tighter_than_the_urgent_one() {
    for last in [MS(0), MS(300), S(3), S(10), S(60), S(600)] {
        assert!(
            osa_budget(Lane::Background, last) <= osa_budget(Lane::Urgent, last),
            "vòng nền rộng hơn vòng có người chờ, tại last_ok={last:?}"
        );
    }
}

/// Trần phải TĂNG ĐƠN ĐIỆU theo độ chậm đo được — máy càng bận, chờ càng lâu,
/// không có chỗ nào đảo chiều.
#[test]
fn the_ceiling_never_shrinks_as_the_machine_gets_slower() {
    let moc = [MS(0), MS(300), S(1), S(3), S(8), S(15), S(30), S(120)];
    for lane in [Lane::Urgent, Lane::Background] {
        for hai in moc.windows(2) {
            assert!(
                osa_budget(lane, hai[1]) >= osa_budget(lane, hai[0]),
                "{lane:?}: {:?} → {:?} mà trần lại rút xuống",
                hai[0],
                hai[1]
            );
        }
    }
}
