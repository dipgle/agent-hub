//! Chạy ĐÚNG lời gọi `otool` mà `runtime::text_id` chạy, qua chính `exec::run`
//! của huba, trên chính binary thật.
//!
//! `#[ignore]` vì nó cần `rust/target/release/hubad` nằm sẵn trên đĩa — máy
//! chưa build release thì không có gì để đo, và một bài kiểm đỏ vì thiếu điều
//! kiện là bài kiểm kêu oan. Nó CHỈ ĐỌC.
//!
//! ```text
//! cargo build --release --offline
//! cargo test --offline --test otool_capture_live -- --ignored --nocapture
//! ```
//!
//! Vì sao phải có, bên cạnh `big_output_is_cut_not_killed`: bài kia dựng lại
//! CÁI ỐNG bằng `dd`, đủ để khoá cả lớp. Bài này trả lời đúng một câu hẹp mà
//! bài kia không trả lời được — *"con số 64 MB có thật sự đủ cho binary CỦA
//! MÁY NÀY không"* — và câu ấy chỉ đo được trên tệp thật.

use std::path::PathBuf;
use std::time::Duration;

use huba::exec::{run, RunOpts};

#[test]
#[ignore = "cần bản build release trên đĩa — chạy tay bằng --ignored"]
fn otool_on_the_real_binary_comes_back_whole() {
    let bin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/hubad");
    if !bin.is_file() {
        println!(
            "BỎ QUA — chưa có {} (chạy `cargo build --release` trước)",
            bin.display()
        );
        return;
    }
    let size = std::fs::metadata(&bin).map(|m| m.len()).unwrap_or(0);

    let mut tong = 0usize;
    for sect in ["__text", "__cstring"] {
        let r = run(
            "otool",
            &["-s", "__TEXT", sect, &bin.display().to_string()],
            RunOpts {
                timeout: Some(Duration::from_secs(60)),
                // ĐÚNG con số `runtime::text_id` khai. Gõ lại ở đây là có chủ
                // ý: bài kiểm phải đỏ nếu ai đó hạ trần bên kia mà quên chỗ này.
                max_bytes: Some(64 * 1024 * 1024),
                ..Default::default()
            },
        )
        .expect("chạy được otool");
        println!(
            "{sect:>10}: {} byte · exit {:?} · cắt {} · {} ms",
            r.stdout.len(),
            r.code,
            r.cut_bytes,
            r.ms
        );
        assert!(!r.timed_out, "otool {sect} hết giờ");
        assert_eq!(
            r.code,
            Some(0),
            "otool {sect} không trả 0: stderr={:?}",
            r.stderr
        );
        assert_eq!(
            r.cut_bytes, 0,
            "otool {sect} bị cắt mất {} byte",
            r.cut_bytes
        );
        assert!(!r.stdout.trim().is_empty(), "otool {sect} không ra chữ nào");
        tong += r.stdout.len();
    }
    println!(
        "binary {} byte · tổng output otool {} byte · gấp {:.1} lần",
        size,
        tong,
        tong as f64 / size.max(1) as f64
    );
    // Cái phải đúng: output LỚN HƠN trần cũ 8 MB. Nếu không, bài kiểm này chạy
    // xanh trên một binary bé và chẳng chứng minh gì về ca đã hỏng.
    assert!(
        tong > 8 * 1024 * 1024,
        "output chỉ {tong} byte — nhỏ hơn trần cũ, nên lượt chạy này KHÔNG đi qua chỗ đã hỏng"
    );
}
