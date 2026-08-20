//! Mỗi trạng thái một HÌNH, và phiên còn lệnh nền thì phải NÓI RA.
//!
//! 🔴 Hà 2026-08-19, ảnh một `/shot` của `[AI/tfl5]` có dòng
//! `✻ Crunched for 5m 58s · 1 shell still running`: *"Tại sao phiên tfl5 vẫn
//! đang có shell đang chạy nhưng danh sách nút phiên thể hiện đã dừng, thay
//! icon hình tròn thành các icon khác nhau cho từng trạng thái để biết nhanh"*.
//!
//! Hai lỗi trong một câu, và cả hai đều là "nói đúng một nửa":
//! * danh sách bảo *đứng chờ* — đúng, lượt của phiên đã xong — rồi **im** về
//!   cái lệnh nền còn chạy, thứ đổi hẳn việc người đọc sắp làm;
//! * bốn tình trạng dùng bốn chấm TRÒN khác nhau đúng ở màu, trong một danh
//!   sách đã có sẵn `🟥 🟩 🟪 🟦` làm nhãn dự án.

use huba::sessions::{LiveSession, ST_ASK, ST_BG, ST_DEAD, ST_ERR, ST_RUN, ST_WAIT};

fn s() -> LiveSession {
    LiveSession {
        session_id: "da29807e-0000".into(),
        host: "interactive".into(),
        ..Default::default()
    }
}

/// Ca của Hà: lượt đã xong, nhưng còn một lệnh chạy nền.
#[test]
fn a_waiting_session_with_a_background_shell_says_so() {
    let mut x = s();
    x.working = false;
    x.bg_shell = true;
    let (icon, label) = huba::sessions::state_of(&x);
    assert_eq!(icon, ST_BG, "{label}");
    assert!(
        label.contains("lệnh nền"),
        "danh sách im về cái shell còn chạy: {label}"
    );
    // …và nó KHÁC hẳn phiên đứng chờ trơn, không chỉ khác màu.
    let mut idle = s();
    idle.bg_shell = false;
    assert_ne!(huba::sessions::state_of(&idle).0, icon);
}

/// Thứ tự ưu tiên: mỗi bậc là một bài học đã trả giá, nên nó phải đo được.
#[test]
fn the_priority_order_holds() {
    // đã tắt nuốt tất cả — mọi phép đo khác nói về một phiên không còn nữa.
    let mut dead = s();
    dead.host = "dead".into();
    dead.working = true;
    dead.bg_shell = true;
    dead.error = Some("bùm".into());
    assert_eq!(huba::sessions::state_of(&dead).0, ST_DEAD);

    // HỎI đứng trên "đang chạy": việc không tự đi tiếp được.
    let mut ask = s();
    ask.working = true;
    ask.asking = Some(Default::default());
    assert_eq!(huba::sessions::state_of(&ask).0, ST_ASK);

    // LỖI đứng trên "đang chạy": chết vì lỗi nhìn y hệt vừa xong.
    let mut err = s();
    err.working = true;
    err.error = Some("API 529".into());
    assert_eq!(huba::sessions::state_of(&err).0, ST_ERR);

    // đang chạy đứng trên lệnh nền: lượt của phiên quan trọng hơn việc nền.
    let mut run = s();
    run.working = true;
    run.bg_shell = true;
    assert_eq!(huba::sessions::state_of(&run).0, ST_RUN);

    // …còn lại là đứng chờ.
    assert_eq!(huba::sessions::state_of(&s()).0, ST_WAIT);
}

/// Không có hai trạng thái nào dùng chung một hình, và KHÔNG cái nào là chấm
/// tròn màu — đó là cả yêu cầu của Hà.
#[test]
fn every_state_has_its_own_shape() {
    let all = [ST_RUN, ST_WAIT, ST_BG, ST_ASK, ST_ERR, ST_DEAD];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a, b, "hai trạng thái dùng chung một hình: {a}");
        }
    }
    for c in ["🟢", "🟡", "🔴", "⚫", "🟠", "🔵"] {
        assert!(
            !all.contains(&c),
            "còn chấm tròn màu trong bảng tình trạng: {c}"
        );
    }
    // 🔴 Và KHÔNG dùng bộ ký hiệu máy phát nhạc — bài học 13/08: ở đó chúng là
    // NÚT BẤM (`▶` = *bấm để chạy*) nên làm tình trạng thì đọc ra nghĩa ngược,
    // và `▶️` đang là nút chạy lệnh thật của huba.
    for c in ["▶", "▶️", "⏸", "⏹"] {
        assert!(
            !all.contains(&c),
            "ký hiệu nút bấm dùng làm tình trạng: {c}"
        );
    }
}

/// Danh sách phiên phải dùng ĐÚNG bộ ấy — một chỗ quyết định, không hai bản chép.
#[test]
fn the_session_list_uses_the_same_table() {
    let mut x = s();
    x.account = "acc1".into();
    x.bg_shell = true;
    let out = huba::pipeline::session_list_text(std::slice::from_ref(&x), "", 0);
    assert!(out.contains(ST_BG), "{out}");
    assert!(out.contains("lệnh nền"), "{out}");
}
