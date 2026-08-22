//! Một lệnh in NHIỀU HƠN TRẦN phải bị CẮT, không bị giết.
//!
//! 🔴 Ca thật, 2026-08-22 — Hà: *"Lâu lắm rồi không chạy được lệnh"*.
//! `runtime::text_id` chạy `otool -s __TEXT __text` trên `hubad`, và output ấy
//! **19.015.353 byte** cho một binary 8.238.928 byte (bản kết xuất hex to gấp
//! 2,3 lần). Trần của `exec` là 8 MB, và nó được thi hành bằng
//! `out.take(8MB).read_to_end(…)` — tức huba **thôi đọc**, rồi luồng đọc kết
//! thúc và ĐÓNG đầu đọc của ống. Đo lại đúng hình dạng ấy trên chính `otool`:
//! **exit 1 · stderr RỖNG · 0,07 giây**. Nên câu báo lỗi là
//! *"otool -s __TEXT __text hỏng trên …:"*, cụt ngay sau dấu hai chấm.
//!
//! ⚠ Không phải treo tới hết giờ — giả thuyết đầu của tôi, và phép đo bác ngay.
//! Nó chết NHANH và IM, thứ khó lần hơn một cái treo.
//!
//! Ba cái sai chồng nhau, và bài kiểm này khoá cả ba: trần GIẾT lệnh thay vì
//! cắt output · cắt mà KHÔNG NÓI · và câu lỗi đổ oan cho một lệnh chưa bao giờ
//! hỏng.
//!
//! Dùng `dd` thay `otool`: nó có sẵn ở mọi máy, in đúng số byte mình xin, và
//! không cần một Mach-O nào nằm sẵn trên đĩa. Thứ đang kiểm là CÁI ỐNG, không
//! phải `otool`.

use std::time::Duration;

use huba::exec::{run, RunOpts};

/// 16 MiB — gấp đôi trần mặc định 8 MiB, đủ để chắc chắn tràn.
const MB16: usize = 16 * 1024 * 1024;

fn dd_16mb(max_bytes: Option<usize>) -> huba::exec::RunOut {
    run(
        "/bin/dd",
        &["if=/dev/zero", "bs=1048576", "count=16"],
        RunOpts {
            // Ngắn có chủ ý: bản CŨ treo tới hết giờ, nên nếu ai đó trả lại
            // `take()` thì bài kiểm đỏ sau 20 giây chứ không đứng mãi.
            timeout: Some(Duration::from_secs(20)),
            max_bytes,
            ..Default::default()
        },
    )
    .expect("chạy được dd")
}

/// Trần MẶC ĐỊNH: phải CẮT, và phải NÓI đã cắt bao nhiêu — **không được giết
/// tiến trình con**.
#[test]
fn a_command_that_floods_the_pipe_is_cut_not_killed() {
    let r = dd_16mb(None);
    assert!(!r.timed_out, "không được treo (ms={})", r.ms);
    // 🔴 Đây là assert MẤU CHỐT. Bản cũ trả `code: None` (dd chết vì SIGPIPE)
    // hoặc `Some(1)` (otool thật) — tức trần được thi hành bằng cách GIẾT lệnh,
    // và chỗ gọi đọc kết quả ấy thành "lệnh hỏng".
    assert_eq!(
        r.code,
        Some(0),
        "lệnh in quá trần bị GIẾT thay vì bị cắt — đầu đọc của ống đang đóng sớm: {r:?}"
    );
    // Giữ đúng trần, vứt phần còn lại, và ĐẾM phần vứt.
    assert_eq!(
        r.stdout.len(),
        8 * 1024 * 1024,
        "phần giữ lại phải đúng bằng trần"
    );
    assert_eq!(
        r.cut_bytes,
        (MB16 - 8 * 1024 * 1024) as u64,
        "phần bị vứt phải được đếm, không được im"
    );
}

/// Khai trần RIÊNG thì lấy được trọn — đúng thứ `runtime::text_id` cần cho
/// `otool`, và là nửa còn lại của bản vá.
#[test]
fn a_caller_that_knows_it_asks_for_a_lot_can_raise_the_cap() {
    let r = dd_16mb(Some(64 * 1024 * 1024));
    assert!(!r.timed_out, "không được treo: {r:?}");
    assert_eq!(r.stdout.len(), MB16, "phải lấy trọn 16 MiB");
    assert_eq!(r.cut_bytes, 0, "không cắt gì thì phải báo 0");
}

/// Phép đo phải BIẾT NÓI KHÔNG: một lệnh in ít hơn trần thì `cut_bytes` phải là
/// `0`. Thiếu ca này thì `assert_eq!(cut_bytes, 0)` ở trên có thể xanh chỉ vì
/// trường ấy không bao giờ được điền.
#[test]
fn a_small_command_reports_nothing_cut() {
    let r = run(
        "/bin/echo",
        &["xin chào"],
        RunOpts {
            timeout: Some(Duration::from_secs(10)),
            ..Default::default()
        },
    )
    .expect("chạy được echo");
    assert_eq!(r.cut_bytes, 0);
    assert!(r.stdout.contains("xin chào"));
    // …và ca trên PHẢI đếm ra khác 0, nếu không hai assert cùng nhìn một hằng số.
    assert_ne!(dd_16mb(None).cut_bytes, 0);
}
