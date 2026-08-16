//! Cú dò Terminal tốn bao lâu, và tiền nằm ở đâu — đo trên máy thật.
//!
//! 🔴 Vì sao có tệp này. `terminal_probe_failed` xuất hiện **19 lượt ngày
//! 2026-08-16** (`logs/hub.log`), 0 lượt mọi ngày trước đó. Phân bố thời gian
//! dò không phải "chậm dần" mà **nhị phân**: trung vị 500–820 ms, rồi nhảy
//! thẳng lên đúng 20.4xx ms — tức đụng trần `OSA_TIMEOUT`, không phải bò tới
//! đó. Và kích thước KHÔNG giải thích được: 9 hàng có 183 lượt và 0 lần hết
//! giờ, trong khi 8 hàng có 111 lượt và 16 lần.
//!
//! Nên câu hỏi không phải "có chậm không" mà "**ai chặn**". Bài kiểm này tách
//! hai nửa để hỏi đúng chỗ:
//!
//! * `terminal_tabs()` — hỏi tty/busy/tiến trình của mọi tab, KHÔNG đọc màn.
//! * `terminal_screens()` — thêm `contents of tab`, tức chữ đang hiện.
//!
//! Chênh lệch giữa hai cột là giá của `contents`. Chạy tay:
//!
//! ```
//! cd ~/projects/hub/rust
//! cargo test --offline --test probe_timing_live -- --ignored --nocapture
//! ```

use std::time::Instant;

const ROUNDS: usize = 6;

#[test]
#[ignore = "cần Terminal thật trên máy này"]
fn where_the_probe_spends_its_time() {
    let mut with = Vec::new();
    let mut without = Vec::new();

    for i in 0..ROUNDS {
        let t = Instant::now();
        let bare = hub::keys::terminal_tabs();
        let ms_bare = t.elapsed().as_millis();

        let t = Instant::now();
        let full = hub::keys::terminal_screens();
        let ms_full = t.elapsed().as_millis();

        let n_bare = bare.as_ref().map(|v| v.len());
        let n_full = full.as_ref().map(|v| v.len());
        // Tổng số ký tự màn đọc về: nếu `contents` là chỗ tốn tiền thì con số
        // này phải đi cùng thời gian, chứ không phải số tab.
        let chars: usize = full
            .as_ref()
            .map(|v| {
                v.iter()
                    .filter_map(|t| t.screen.as_ref())
                    .map(|s| s.len())
                    .sum()
            })
            .unwrap_or(0);

        println!(
            "lượt {i}: không-màn {ms_bare:>6} ms ({n_bare:?} tab) · có-màn {ms_full:>6} ms \
             ({n_full:?} tab, {chars} ký tự)  → giá của contents ≈ {} ms",
            ms_full as i128 - ms_bare as i128
        );
        if let Err(e) = &bare {
            println!("   ⚠ không-màn hỏng: {e}");
        }
        if let Err(e) = &full {
            println!("   ⚠ có-màn hỏng: {e}");
        }
        without.push(ms_bare);
        with.push(ms_full);
    }

    let stat = |v: &mut Vec<u128>| {
        v.sort_unstable();
        (v[0], v[v.len() / 2], v[v.len() - 1])
    };
    let (lo_b, mid_b, hi_b) = stat(&mut without);
    let (lo_f, mid_f, hi_f) = stat(&mut with);
    println!("\nkhông-màn: min {lo_b} · giữa {mid_b} · max {hi_b} ms");
    println!("có-màn   : min {lo_f} · giữa {mid_f} · max {hi_f} ms");

    // Không assert ngưỡng: đây là phép ĐO, và một ngưỡng đoán bừa ở đây sẽ đỏ
    // vì máy bận chứ không vì sản phẩm hỏng. Cái phải đúng là hàm trả về được.
    assert!(
        !with.is_empty() && !without.is_empty(),
        "không đo được lượt nào"
    );
}

/// Nhiều cú dò CÙNG LÚC — hub không có khoá nào tuần tự hoá `osascript`, mà
/// trong một vòng chạy có ít nhất ba chỗ hỏi Terminal (`trust_dialog_tick`,
/// ảnh chụp phiên, và handler của lệnh đang chạy). Trên 19 lượt hỏng ngày
/// 16/08, **7 lượt** có `trust_tick_probe_failed` đứng ngay cạnh — tức hai cú
/// dò cùng ngã một lúc. Bài này hỏi: gọi song song thì tốn bao nhiêu?
#[test]
#[ignore = "cần Terminal thật trên máy này"]
fn what_four_probes_at_once_cost() {
    const N: usize = 4;
    let t0 = Instant::now();
    let hands: Vec<_> = (0..N)
        .map(|i| {
            std::thread::spawn(move || {
                let t = Instant::now();
                let r = hub::keys::terminal_screens();
                (i, t.elapsed().as_millis(), r.is_ok())
            })
        })
        .collect();
    let mut worst = 0u128;
    for h in hands {
        let (i, ms, ok) = h.join().expect("luồng dò không được chết");
        println!("  song song #{i}: {ms:>6} ms · đọc được: {ok}");
        worst = worst.max(ms);
    }
    println!(
        "\n{N} cú cùng lúc: chậm nhất {worst} ms · tổng {} ms",
        t0.elapsed().as_millis()
    );
    println!(
        "(một cú đơn lẻ đo được ~440–500 ms; nếu chậm nhất ≈ N×500 thì Terminal \
         xếp hàng, nếu nó vọt lên 20.000 thì đây đúng là chỗ sinh ra terminal_probe_failed)"
    );
}
