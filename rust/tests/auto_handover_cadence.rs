//! Nhịp lấy mẫu khi đang canh một phiên đã quá ngưỡng ngữ cảnh.
//!
//! 🔴 Hà 2026-08-23: *"sao nó đủ điều kiện chuyển phiên mới nhưng không tự
//! chuyển, cho đến khi tôi chạy một lệnh bất kỳ mới vào luồng chuyển phiên"*.
//!
//! Không cửa nào sai — sai ở phép LẤY MẪU. Cửa nổ đòi phiên vừa không bận vừa
//! im ít nhất `idle_sec` **tại đúng khoảnh khắc** vòng chạy qua, mà lưới vòng
//! là `poll_interval_sec`. Khe hợp lệ hẹp hơn lưới ⟹ bắt hụt, và bắt hụt mãi
//! với phiên làm việc theo từng đợt ngắn.
//!
//! Số đo trên `logs/huba.log` từ 20/08 (26 phiên chạm ngưỡng), thứ đã bác bỏ
//! câu trả lời đầu tiên của tôi ("chờ tối đa 4–5 phút"):
//!
//! | | |
//! |---|---|
//! | chờ từ lúc chạm ngưỡng tới lúc chuyển | trung vị **15 phút** |
//! | số ca chờ > 10 phút | 14/24 |
//! | ca lâu nhất | **205 phút** (`4f7a06ae`) |
//! | chưa bao giờ chuyển | 2 phiên (`93faab89`: 30 lượt kiểm, `Busy` cả 30) |
//! | lượt nổ cưỡi lên vòng do LỆNH đánh thức | **8/24** |
//!
//! Dòng cuối là chỗ lời của Hà thành số: một phần ba số lần chuyển phiên xảy ra
//! trong một vòng mà lệnh Telegram vừa đánh thức — tức cái cảm giác "phải gõ
//! lệnh nó mới chạy" không phải mê tín.

use huba::config::Config;
use huba::pipeline::watch_slice_sec;

/// Đang canh thì lấy mẫu dày hơn hẳn lưới thường — nếu không thì bản vá này
/// không vá gì cả.
#[test]
fn watching_samples_far_denser_than_the_normal_grid() {
    let cfg = Config::default();
    let slice = watch_slice_sec(&cfg);
    assert!(
        slice < cfg.poll_interval_sec,
        "nhịp canh {slice}s không ngắn hơn lưới thường {}s",
        cfg.poll_interval_sec
    );
    // Ít nhất vài mẫu phải rơi vào trong một khe rộng `idle_sec`. Một mẫu thì
    // là may rủi — đúng cái đang hỏng.
    let per_window = cfg.auto_handover.idle_sec / slice;
    assert!(
        per_window >= 3,
        "chỉ {per_window} mẫu trong một khe {}s — vẫn là may rủi",
        cfg.auto_handover.idle_sec
    );
}

/// …và nó chỉ được RÚT NGẮN giấc ngủ, không bao giờ kéo dài.
///
/// Cấu hình có thể đặt vòng ngắn hơn cả nhịp canh (`poll_interval_sec` nhỏ);
/// lúc ấy trả về một số LỚN hơn là tự tay làm chậm daemon nhân danh làm nhanh.
#[test]
fn the_watch_slice_never_stretches_a_shorter_loop() {
    let mut cfg = Config::default();
    for poll in [1u64, 5, 10, 15, 30, 120, 600] {
        cfg.poll_interval_sec = poll;
        let slice = watch_slice_sec(&cfg);
        assert!(
            slice <= poll.max(15),
            "poll={poll}s mà nhịp canh {slice}s — dài hơn cả vòng thường"
        );
    }
}

/// Nhịp suy TỪ `idle_sec`, không phải một con số gõ cứng.
///
/// Đổi `idle_sec` trong cấu hình mà nhịp canh đứng im thì bản vá chỉ đúng cho
/// đúng một giá trị — và không ai biết lúc nó thôi đúng.
#[test]
fn the_slice_follows_the_idle_requirement_it_is_chasing() {
    let mut cfg = Config {
        poll_interval_sec: 600,
        ..Default::default()
    };
    cfg.auto_handover.idle_sec = 120;
    let at_120 = watch_slice_sec(&cfg);
    cfg.auto_handover.idle_sec = 600;
    let at_600 = watch_slice_sec(&cfg);
    assert!(
        at_600 > at_120,
        "đòi im lâu gấp 5 mà nhịp canh không đổi ({at_120}s → {at_600}s) ⟹ số gõ cứng"
    );
    // Cận dưới vẫn phải giữ: `idle_sec` bé tí không được kéo nhịp xuống mức mỗi
    // mẫu là một lượt đọc màn bằng osascript.
    cfg.auto_handover.idle_sec = 6;
    assert_eq!(watch_slice_sec(&cfg), 15, "cận dưới 15s bị phá");
}
