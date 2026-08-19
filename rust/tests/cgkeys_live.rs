//! Phép ĐO cho đường phím rời (`cgkeys`) — chạy trên một cửa sổ Terminal THẬT.
//!
//! Câu hỏi phải trả lời bằng đo, không bằng suy luận, vì cả thiết kế nút tab
//! đứng trên nó: **một mũi tên gửi bằng `CGEventPostToPid` có di được con trỏ
//! của bảng hỏi mà KHÔNG chốt câu nào không?**
//!
//! Đường cũ (`do script`) thì câu trả lời là KHÔNG, và đã trả giá bằng việc
//! thật: 2026-08-19, một cú Enter lạc chốt `☐ RPC pool` → `☒` trên bảng hỏi của
//! phiên `[AI/tcc/amm]`. Nếu đường mới cũng chốt thì nút tab không dựng được, và
//! phải nói đúng như thế thay vì dựng một cái nút trả lời hộ chủ máy.
//!
//! ```text
//! # chỉ đọc — không gửi phím nào:
//! cargo test --offline --test cgkeys_live -- --ignored --nocapture permission
//!
//! # GỬI PHÍM THẬT vào cửa sổ ấy:
//! HUB_LIVE_TTY=ttys003 HUB_LIVE_PRESS=1 \
//!   cargo test --offline --test cgkeys_live -- --ignored --nocapture arrow
//! ```

/// Trạng thái quyền — câu đầu tiên phải hỏi, vì thiếu quyền thì phím không tới
/// nơi và hệ thống KHÔNG báo lỗi.
#[test]
#[ignore = "hỏi trạng thái quyền của máy này — chạy tay"]
fn permission_status_of_this_process() {
    println!("AXIsProcessTrusted() = {}", hub::cgkeys::trusted());
    match hub::keys::terminal_pid() {
        Ok(pid) => println!("Terminal.app pid = {pid}"),
        Err(e) => println!("không tìm được pid Terminal: {e}"),
    }
}

/// Một mũi tên rời: tab có đổi không, và có câu nào bị CHỐT không.
///
/// Phép đo phải trỏ đúng chỗ (bài học 19/08): thứ nói lên "có câu bị chốt" là
/// **thanh tab** (`☐`→`☒`), không phải ô nhập — ô nhập không bao giờ đổi khi
/// màn đang mở hộp chọn, nên đo nó là đo mù.
#[test]
#[ignore = "GỬI PHÍM THẬT — cần HUB_LIVE_TTY và HUB_LIVE_PRESS=1"]
fn an_arrow_moves_the_tab_without_answering() {
    let tty = std::env::var("HUB_LIVE_TTY").expect("cần HUB_LIVE_TTY");
    if std::env::var("HUB_LIVE_PRESS").ok().as_deref() != Some("1") {
        println!("BỎ QUA — bài kiểm này GỬI PHÍM THẬT. Đặt HUB_LIVE_PRESS=1 nếu đúng là muốn bấm.");
        return;
    }
    let pid = hub::keys::terminal_pid().expect("Terminal phải đang chạy");
    let w = hub::keys::window_of(&tty)
        .expect("hỏi được Terminal")
        .expect("tty phải gắn một cửa sổ");

    let read = || {
        let body = hub::keys::screen_of(&tty, 40).expect("đọc được màn").0;
        let table = hub::keys::ask_table(&body);
        // Câu đang mở = dòng ngay dưới thanh tab. Lấy 60 ký tự làm dấu vân tay:
        // đủ để biết đã sang câu khác, không phụ thuộc bề ngang cửa sổ.
        let open: String = body
            .lines()
            .skip_while(|l| !(l.contains('←') && l.contains('→')))
            .nth(1)
            .unwrap_or_default()
            .chars()
            .take(60)
            .collect();
        (table, open)
    };

    let (tab_before, open_before) = read();
    println!("thanh tab TRƯỚC : {tab_before:?}");
    println!("câu đang mở TRƯỚC: {open_before:?}");

    hub::keys::focus_window(w).expect("đưa cửa sổ lên nhận phím");
    hub::cgkeys::post(pid, &["right".to_string()]).expect("gửi được mũi tên");
    std::thread::sleep(std::time::Duration::from_millis(2500));

    let (tab_after, open_after) = read();
    println!("thanh tab SAU  : {tab_after:?}");
    println!("câu đang mở SAU : {open_after:?}");

    let answered_changed = tab_before.as_ref().map(|t| t.answered.clone())
        != tab_after.as_ref().map(|t| t.answered.clone());
    println!(
        "=> con trỏ {} · câu bị chốt: {}",
        if open_before != open_after {
            "CÓ sang tab khác"
        } else {
            "KHÔNG nhúc nhích"
        },
        if answered_changed {
            "CÓ ❌"
        } else {
            "KHÔNG ✅"
        }
    );
}

/// VÒNG ĐI của con trỏ ngang: đi hết một vòng `→` rồi in ra từng chỗ nó dừng.
///
/// Cần đo vì cả phép "về đúng tab số n" đứng trên nó: hub không đọc được tab
/// nào đang mở (tab hiện hành vẽ bằng MÀU, mà đọc màn về chỉ có chữ trần), nên
/// nó phải **về một mốc biết chắc** rồi đếm bước từ đó. Mốc ứng viên là bước
/// `Review your answers`. Vòng có quấn lại không, và quấn về đâu — đo, đừng đoán.
#[test]
#[ignore = "GỬI PHÍM THẬT — cần HUB_LIVE_TTY và HUB_LIVE_PRESS=1"]
fn what_does_a_full_lap_of_right_arrows_look_like() {
    let tty = std::env::var("HUB_LIVE_TTY").expect("cần HUB_LIVE_TTY");
    if std::env::var("HUB_LIVE_PRESS").ok().as_deref() != Some("1") {
        println!("BỎ QUA — bài kiểm này GỬI PHÍM THẬT. Đặt HUB_LIVE_PRESS=1 nếu đúng là muốn bấm.");
        return;
    }
    let pid = hub::keys::terminal_pid().expect("Terminal phải đang chạy");
    let w = hub::keys::window_of(&tty)
        .expect("hỏi được Terminal")
        .expect("tty phải gắn một cửa sổ");
    hub::keys::focus_window(w).expect("đưa cửa sổ lên nhận phím");

    let read = || {
        let body = hub::keys::screen_of(&tty, 40).expect("đọc được màn").0;
        let open: String = body
            .lines()
            .skip_while(|l| !(l.contains('←') && l.contains('→')))
            .nth(1)
            .unwrap_or_default()
            .chars()
            .take(50)
            .collect();
        (hub::keys::ask_table(&body).map(|t| t.answered), open)
    };

    let dir = std::env::var("HUB_LIVE_DIR").unwrap_or_else(|_| "right".to_string());
    let (answered0, _) = read();
    for step in 0..6 {
        let (answered, open) = read();
        println!("bước {step} ({dir}): {open:?}");
        assert_eq!(
            answered, answered0,
            "một bước ngang đã CHỐT một câu ở bước {step}"
        );
        hub::cgkeys::post(pid, std::slice::from_ref(&dir)).expect("gửi được mũi tên");
        std::thread::sleep(std::time::Duration::from_millis(1200));
    }
    println!("=> không có câu nào bị chốt trong cả vòng");
}
