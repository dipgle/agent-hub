//! Nút `⏹ dừng lệnh này` phải DỪNG được, và phải nói đúng điều đo được.
//!
//! 🔴 Hà 2026-08-27, ảnh Telegram: bấm `⏹ dừng lệnh này` **bốn lần**, mỗi lần
//! huba đáp *"đã bảo dừng: node tools/cms-setup.mjs …"*, và lệnh vẫn chạy tiếp
//! 15 → 16 → 18 phút. *"Bấm dừng lệnh đang chạy không có tác dụng"*.
//!
//! Nhật ký nói rõ chuyện gì đã xảy ra, và nó không phải "kill hỏng":
//!
//! ```text
//! long_job_stop_asked  ok:true  pid:97020   01:48:12
//! long_job_stop_asked  ok:true  pid:97020   01:48:47
//! long_job_stop_asked  ok:true  pid:97020   01:57:42
//! long_job_stop_asked  ok:true  pid:97020   01:59:54
//! runin_ran  code:130  ms:1167242  timed_out:false   02:01:56
//! ```
//!
//! `/bin/kill` **thành công cả bốn lần**. Đích là `node` chạy Playwright, thứ
//! tự cài bộ bắt SIGTERM của riêng nó, nên tín hiệu tới nơi mà tiến trình không
//! chết. Hai mệnh đề *"đã gửi được tín hiệu"* và *"nó đã chết"* khác nhau, và
//! `stop_job` khẳng định cái thứ hai trong khi chỉ đo được cái thứ nhất.
//!
//! Ngược đời hơn: đường HẾT GIỜ tự động (`exec::kill_group`) vốn đã leo thang
//! TERM → KILL từ lâu. Cái nút một con người bấm lại YẾU HƠN cái đồng hồ.
//!
//! Tệp này khoá cả bậc thang lẫn phép đọc lại, và **cấy ca hỏng thật**: một
//! tiến trình `trap "" TERM` (đúng hình dạng con node hôm ấy) phải làm cổng đọc
//! ra `GoneAfterKill`, không phải `GoneAfterTerm`.

use huba::exec::{group_alive, kill_group_verified, GroupKill, RunOpts};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Đường dẫn cái mốc "đã sẵn sàng" cho một ca kiểm.
///
/// 🔴 Vì sao phải có (đo 28/08, cổng này ĐỎ một lượt rồi xanh lại khi chạy
/// riêng): `viec_nen` cũ chỉ chờ **nhóm tiến trình xuất hiện**, mà nhóm có ngay
/// lúc `fork` — TRƯỚC khi `zsh` kịp đọc và cài `trap '' TERM`. Máy đang tải
/// nặng thì TERM rơi đúng khe ấy, zsh chết như một tiến trình thường, và ca
/// "bỏ qua TERM" đọc ra `GoneAfterTerm`.
///
/// Sản phẩm không sai; PHÉP ĐO sai — nó tin một dấu hiệu (nhóm tồn tại) trả lời
/// một câu khác (cái bẫy đã cài chưa). Nay tiến trình con TỰ NÓI khi sẵn sàng,
/// và bài kiểm chờ đúng lời ấy. Một cổng chập chờn dạy người đọc lướt qua màu
/// đỏ — nó tệ hơn không có cổng.
fn moc(ten: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("huba-stopjob-{ten}-{}", std::process::id()))
}

/// Dựng một việc chạy nền ĐÚNG đường `watch_long_job` đi: `exec::run` trong một
/// luồng riêng, pid lấy ra bằng `pid_out`. Dựng bằng tay một `Command` khác thì
/// bài kiểm đo một đường mà sản phẩm đi một đường.
fn viec_nen_cho_moc(dong_lenh: &str, moc: Option<&std::path::Path>) -> u32 {
    let (tx, rx) = mpsc::channel();
    let cmd = dong_lenh.to_string();
    std::thread::spawn(move || {
        let _ = huba::exec::run(
            "/bin/zsh",
            &["-c", &cmd],
            RunOpts {
                timeout: Some(Duration::from_secs(120)),
                pid_out: Some(tx),
                ..Default::default()
            },
        );
    });
    let pid = rx
        .recv_timeout(Duration::from_secs(20))
        .expect("exec::run phải nói ra pid ngay khi dựng xong tiến trình");
    // Nhóm phải NHÌN THẤY ĐƯỢC trước khi đo, nếu không bài kiểm đo lúc nó chưa
    // kịp tồn tại rồi kết luận nhầm là "đã chết".
    let han = Instant::now() + Duration::from_secs(20);
    while Instant::now() < han {
        let nhom = group_alive(pid) == Some(true);
        // Có mốc thì phải đợi CHÍNH nó: nhóm tồn tại chỉ nói "đã fork", không
        // nói "cái bẫy đã cài".
        let san_sang = moc.is_none_or(|m| m.exists());
        if nhom && san_sang {
            return pid;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!(
        "nhóm {pid} chưa sẵn sàng sau 20s (nhóm sống: {:?}, mốc: {:?}) — phép đo hỏng, \
         không phải sản phẩm hỏng",
        group_alive(pid),
        moc.map(|m| m.exists())
    );
}

fn viec_nen(dong_lenh: &str) -> u32 {
    viec_nen_cho_moc(dong_lenh, None)
}

#[test]
fn a_plain_job_dies_on_the_polite_signal() {
    let pid = viec_nen("sleep 60");
    assert_eq!(
        kill_group_verified(pid),
        GroupKill::GoneAfterTerm,
        "một tiến trình bình thường phải chết ngay ở TERM — ra khác nghĩa là bậc thang \
         hoặc phép đọc lại đang sai"
    );
    assert_eq!(
        group_alive(pid),
        Some(false),
        "cổng khai đã dừng thì nhóm phải RỖNG — chính chỗ này là chỗ bản cũ không hề hỏi"
    );
}

/// ĐỐI CHỨNG NGƯỢC — ca cấy, đúng hình dạng đã cắn 27/08.
///
/// `trap "" TERM` là bản thu nhỏ của con `node` cài bộ bắt SIGTERM: tín hiệu tới
/// nơi, `/bin/kill` trả 0, tiến trình sống nhăn. Đo tay trên máy này trước khi
/// viết: sống qua TERM (`kill -0` rc=0), chết ngay khi KILL (rc=1).
///
/// Bản `stop_job` cũ trả *"đã bảo dừng"* cho đúng ca này. Nếu bài kiểm này ra
/// `GoneAfterTerm` thì hoặc bậc KILL đã bị gỡ, hoặc phép đọc lại đang nói dối.
#[test]
fn a_job_that_ignores_term_is_still_stopped_and_says_so() {
    let m = moc("bo-qua-term");
    let _ = std::fs::remove_file(&m);
    // Thứ tự trong dòng lệnh là cả phép đo: cài bẫy XONG mới đặt mốc, nên thấy
    // mốc là chắc chắn bẫy đã sống.
    let pid = viec_nen_cho_moc(
        &format!("trap '' TERM; : > {}; sleep 60", m.display()),
        Some(&m),
    );
    assert_eq!(
        kill_group_verified(pid),
        GroupKill::GoneAfterKill,
        "tiến trình bỏ qua TERM phải bị KILL, và kết cục phải NÓI RA rằng nó đã bỏ qua — \
         đây đúng là ca Hà bấm bốn lần mà không có gì xảy ra"
    );
    assert_eq!(group_alive(pid), Some(false), "KILL rồi thì nhóm phải rỗng");
}

/// Việc tự xong là một sự thật KHÁC "tôi vừa giết nó" — và người bấm nút cần
/// phân biệt được, vì hai câu ấy dẫn tới hai hành động khác nhau.
#[test]
fn a_job_that_already_finished_is_not_claimed_as_a_kill() {
    let pid = viec_nen("sleep 0.2");
    let han = Instant::now() + Duration::from_secs(10);
    while Instant::now() < han && group_alive(pid) != Some(false) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        kill_group_verified(pid),
        GroupKill::AlreadyGone,
        "nhóm đã rỗng từ trước thì cấm khai là vừa giết được nó"
    );
}

/// Phép đọc phải đổi được trạng thái, nếu không nó không phải phép đo (§13①).
#[test]
fn the_liveness_probe_reads_both_ways() {
    let pid = viec_nen("sleep 60");
    assert_eq!(
        group_alive(pid),
        Some(true),
        "nhóm vừa dựng mà đọc ra 'chết' thì mọi khẳng định dựa trên phép đọc này đều vô nghĩa"
    );
    assert_eq!(kill_group_verified(pid), GroupKill::GoneAfterTerm);
    assert_eq!(
        group_alive(pid),
        Some(false),
        "và nó phải đọc ra được CẢ chiều kia"
    );
}
