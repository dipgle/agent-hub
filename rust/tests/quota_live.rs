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

/// % HẠN MỨC THEO MODEL, đọc từ sổ THẬT (Hà 2026-09-20).
///
/// Cùng lý do bài trên phải chạy trên máy thật: thứ dễ sai nhất không phải phép
/// tính mà là **đường đi tới dữ liệu**. `doc_models` bóc theo con trỏ
/// `/scope/model/display_name` trong `utilization.limits[]`; trỏ nhầm một nhịp là
/// nó trả `Vec` rỗng ở MỌI tài khoản, mà rỗng lại là một trạng thái hợp lệ ("nguồn
/// không có hàng nào") — nên hỏng kiểu ấy im lặng hoàn toàn. Bài này đòi ÍT NHẤT
/// MỘT hàng model đọc ra được từ đĩa.
///
/// 🔴 CỐ Ý KHÔNG chấm "phải có đúng 1 hàng, tên Fable". Đó là hình dạng dữ liệu
/// của HÔM NAY (đo 20/09: cả 5 tài khoản đúng một hàng `Fable`, và
/// `seven_day_opus`/`seven_day_sonnet` đều `null`). Ghim nó vào bài kiểm thì ngày
/// nhà cung cấp thêm hàng Opus/Sonnet, bài này ĐỎ OAN trong khi sản phẩm đúng —
/// đúng lớp "phép đo neo vào nhãn tĩnh".
#[test]
#[ignore = "đọc sổ tài khoản thật trong $HOME — chạy tay bằng --ignored"]
fn doc_duoc_han_muc_theo_model_tu_so_that() {
    let cfg_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rust/ phải có thư mục cha")
        .join("huba.config.json");
    let cfg = huba::config::load(Some(&cfg_path)).expect("đọc được huba.config.json");

    let all = huba::quota::read_all(&cfg);
    let mut co_hang = 0usize;
    for q in &all {
        let ten: Vec<String> = q
            .models
            .iter()
            .map(|m| format!("{} {}%", m.name, m.pct))
            .collect();
        println!(
            "{:<8} {} hàng model: {}",
            q.account,
            q.models.len(),
            if ten.is_empty() {
                "(nguồn không có hàng nào)".to_string()
            } else {
                ten.join(" · ")
            }
        );
        // Dòng thật sẽ đi ra Telegram — in luôn để người đọc thấy bằng mắt.
        println!("         ↳ {}", q.say(huba::quota::now_ms()));
        if !q.models.is_empty() {
            co_hang += 1;
        }
    }
    // MẪU SỐ: khai cả tử và mẫu, vì "0/6" và "6/6" đọc rất khác nhau.
    println!(
        "=> {}/{} tài khoản có ít nhất một hàng model",
        co_hang,
        all.len()
    );
    assert!(
        co_hang > 0,
        "KHÔNG tài khoản nào bóc ra hàng model ⟹ nhiều khả năng con trỏ \
         `/scope/model/display_name` trong `doc_models` trỏ nhầm, chứ không phải \
         nguồn thật sự rỗng — đã đo 20/09 là mọi tài khoản đều có hàng `weekly_scoped`"
    );
}
