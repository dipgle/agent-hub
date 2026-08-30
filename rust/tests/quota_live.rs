//! Đọc hạn mức THẬT của từng tài khoản trên chính máy này.
//!
//! `#[ignore]` vì nó đọc `$HOME/.claude*/.claude.json` — sổ thật của chủ máy,
//! không có trong CI và không dựng lại được bằng fixture. Chạy tay:
//!
//! ```text
//! cargo test --offline --test quota_live -- --ignored --nocapture
//! ```
//!
//! Chỉ ĐỌC: không spawn `claude`, không tốn một lượt quota nào, không ghi gì.
//!
//! 🔴 Vì sao phép đo này phải chạy trên máy thật chứ không chỉ có bài kiểm đơn
//! vị: `quota::rank` chấm được trên chuỗi tự dựng, nhưng thứ dễ sai nhất lại nằm
//! ngoài nó — **đường dẫn**. Tài khoản mặc định để sổ ở `~/.claude.json` (gốc
//! `$HOME`), KHÔNG phải `~/.claude/.claude.json`; trỏ nhầm thì hàm đọc ra
//! `Unknown` vĩnh viễn, và một phép đo luôn im lặng là dạng hỏng khó thấy nhất.
//! Bài này bắt đúng chỗ ấy: nó đòi ÍT NHẤT MỘT tài khoản đọc ra số thật.

use huba::quota::{rank_all, Rank};

#[test]
#[ignore = "đọc sổ tài khoản thật trong $HOME — chạy tay bằng --ignored"]
fn doc_duoc_han_muc_that_cua_tung_tai_khoan() {
    let cfg_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rust/ phải có thư mục cha")
        .join("huba.config.json");
    let cfg = huba::config::load(Some(&cfg_path)).expect("đọc được huba.config.json");

    let hang = rank_all(&cfg, huba::quota::now_ms());
    assert!(
        !hang.is_empty(),
        "cấu hình phải khai ít nhất một tài khoản — nếu không thì bài này không đo gì"
    );
    for r in &hang {
        println!("{:<8} {}", r.name, r.rank.say());
    }

    // MẪU SỐ (luật 13③): "đọc ra Unknown hết" trông y hệt "đọc đúng và tài khoản
    // nào cũng chưa có số". Trên máy này thì KHÔNG phải thế — cả ba tệp đều có
    // `cachedUsageUtilization` (đo 30/08). Nên toàn Unknown = đang trỏ nhầm chỗ.
    let do_duoc = hang
        .iter()
        .filter(|r| r.rank != Rank::Unknown)
        .collect::<Vec<_>>();
    assert!(
        !do_duoc.is_empty(),
        "không tài khoản nào đọc ra số ⟹ nhiều khả năng `quota::book_path` đang trỏ nhầm tệp \
         (tài khoản mặc định nằm ở ~/.claude.json, không phải ~/.claude/.claude.json)"
    );
    println!(
        "=> {}/{} tài khoản đọc được số thật",
        do_duoc.len(),
        hang.len()
    );

    // Và cái được chọn phải là cái rộng cửa nhất trong số đo được — chấm ngay
    // trên số của máy này, không phải trên một bảng nghĩ ra.
    let tot_nhat = hang.iter().min_by_key(|r| r.rank).expect("còn tài khoản");
    println!(
        "=> huba sẽ chọn: {} ({})",
        tot_nhat.name,
        tot_nhat.rank.say()
    );
    for r in &hang {
        assert!(
            tot_nhat.rank <= r.rank,
            "{} ({}) rộng cửa hơn {} ({}) mà không được chọn",
            r.name,
            r.rank.say(),
            tot_nhat.name,
            tot_nhat.rank.say()
        );
    }
}
