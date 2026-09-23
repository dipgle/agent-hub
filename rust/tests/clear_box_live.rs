//! `clear_box` chạy THẬT trên ô nhập NHIỀU DÒNG: xoá hết, hay chỉ dòng cuối?
//!
//! 🔴 Hà 2026-09-23: *"chỉnh lại lệnh clean để xóa toàn bộ ô nhập của phiên,
//! hiện tại xóa mỗi dòng thì phải"*. Gốc đo được hôm ấy nằm ở phép ĐỌC (bảng
//! subagent dưới ô bị đếm thành chữ — `tests/o_nhap_giua_hai_vien.rs`). Bài này
//! đo vế còn lại, thứ bài thuần không nói được: DEL (0x7f) qua `do script` có
//! xoá lùi QUA dấu xuống dòng trong ô của `claude` không. Nếu không, `clear_box`
//! dừng ở đầu dòng cuối và đúng là "xoá mỗi dòng".
//!
//! KHÔNG chạy trên phiên của chủ máy: cửa sổ đo do chính bài kiểm mở ra, và nó
//! tự đóng lại — dọn TRƯỚC khi phán (bài học của `clean_queue_live.rs`).
//!
//! ```
//! cd ~/projects/huba/rust
//! cargo test --offline --test clear_box_live -- --ignored --nocapture
//! ```

use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

fn osa(script: &str) -> String {
    let out = Command::new("osascript")
        .args(["-e", script])
        .output()
        .expect("chạy được osascript");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn screen(w: i64) -> String {
    osa(&format!(
        "tell application \"Terminal\" to return contents of selected tab of window id {w}"
    ))
}

/// Số dòng có chữ nằm GIỮA HAI VIỀN của ô — đếm trên màn, không qua bộ lọc
/// của `input_box_text`, để phép đo không dựa vào chính thứ đang bị nghi.
fn box_lines(scr: &str) -> usize {
    huba::keys::box_inner(scr)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().trim_start_matches('❯').trim().is_empty())
        .count()
}

/// Màn đã tới Ô NHẬP THẬT chưa — không phải hộp thoại khởi động.
///
/// ⚠ Lượt chạy đầu (2026-09-23) trượt đúng chỗ này: thư mục nháp cũ mang một
/// `.claude/settings.local.json` nên `claude` bật hộp *"Is this a project you
/// … trust?"* (`❯ No, exit` / `Yes, I trust this folder`). Hộp ấy có vạch kẻ và
/// `❯` nên `box_start` nhận, còn lựa chọn KHÔNG đánh số nên `parse_choices` rỗng
/// — bài kiểm gõ thẳng vào hộp thoại và "đo" chữ của hộp. Dòng chân của hộp
/// thoại là thứ phân biệt được, nên chờ tới khi nó biến mất.
fn at_real_prompt(s: &str) -> bool {
    huba::keys::box_start(s).is_some()
        && huba::keys::parse_choices(s).is_empty()
        && !s.contains("Enter to confirm")
        && !s.contains("Esc to cancel")
}

#[test]
#[ignore = "mở một cửa sổ Terminal nháp chạy `claude` rồi đóng — chạy tay bằng --ignored"]
fn del_xoa_qua_cho_xuong_dong() {
    // Thư mục phải là thứ `claude` ĐÃ TIN, không thì nó bật hộp thoại tin-thư-
    // mục và bài này không đo được gì (xem `at_real_prompt`). Đo 2026-09-23:
    // mọi thư mục dưới `huba/` đều bị hỏi lại, vì `huba/.claude/settings.local.json`
    // thêm một thư mục vào workspace (`additionalDirectories`); gốc `~/projects`
    // thì `hasTrustDialogAccepted: true` và không thêm gì. Mở `claude` ở đó chỉ
    // `cd`, không ghi tệp nào vào thư mục.
    let dir = std::env::var("HUB_LIVE_DIR")
        .unwrap_or_else(|_| std::env::var("HOME").unwrap() + "/projects");
    let w: i64 = osa(&format!(
        "tell application \"Terminal\"
  do script \"cd {dir} && claude\"
  delay 2
  return id of window 1
end tell"
    ))
    .parse()
    .expect("mở được cửa sổ nháp");
    println!("cửa sổ nháp: {w}");

    // Chờ THEO ĐIỀU KIỆN: ô nhập thật, không hộp thoại nào (gõ vào hộp tin-thư-
    // mục là trả lời hộ nó). Hết giờ mà vẫn là hộp thoại ⟹ KHÔNG gõ gì, dọn rồi
    // khai KHÔNG ĐO ĐƯỢC.
    let mut ready = false;
    for _ in 0..40 {
        sleep(Duration::from_secs(1));
        if at_real_prompt(&screen(w)) {
            ready = true;
            break;
        }
    }

    let mut before = String::new();
    let mut lines_before = 0;
    let mut cleared = None;
    let mut after = String::new();
    if ready {
        // Ba dòng, một lượt ghi, `ESC` cuối payload chặn cái CR `do script`
        // kèm theo (đo trong `clear_box`) — chữ nằm lại trong ô, không gửi.
        osa(&format!(
            "tell application \"Terminal\" to do script (\"dong mot XQZ\" & linefeed & \"dong hai XQZ\" & linefeed & \"dong ba XQZ\" & (ASCII character 27)) in selected tab of window id {w}"
        ));
        sleep(Duration::from_millis(1500));
        before = screen(w);
        lines_before = box_lines(&before);
        println!(
            "--- ô trước khi xoá ({lines_before} dòng có chữ) ---\n{}",
            huba::keys::box_inner(&before).unwrap_or_default()
        );
        cleared = Some(huba::keys::clear_box(w));
        after = screen(w);
        println!(
            "--- ô sau khi xoá: clear_box = {cleared:?} ---\n{}",
            huba::keys::box_inner(&after).unwrap_or_default()
        );
    }

    // Dọn: TERM đúng tiến trình `claude` trên tty của CHÍNH tab này, rồi chỉ
    // đóng khi tab đã rảnh.
    //
    // ⚠ KHÔNG gõ `/exit`. Đo hai lượt liền 2026-09-23 (claude 2.1.280): `/exit`
    // gõ qua `do script` rồi Enter rời bị nhận như TIN NHẮN THƯỜNG — phiên nháp
    // chạy một lượt model để trả lời nó (57 giây, rồi 4 giây) và cửa sổ đứng đó.
    // Lượt đầu tôi tưởng do hook `SessionStart` còn chạy; lượt hai chờ hook xong
    // rồi vẫn y nguyên, nên gốc không phải hook. Còn hộp thoại tin-thư-mục thì
    // gõ gì vào đó cũng là trả lời nó. TERM theo PID thì đúng ở cả hai ca, và nó
    // chỉ chạm tiến trình trên tty của tab bài kiểm tự mở — không `pkill -f`
    // (khớp cả argv của phiên khác).
    let tty = osa(&format!(
        "tell application \"Terminal\" to return tty of selected tab of window id {w}"
    ));
    if let Some(t) = tty.strip_prefix("/dev/") {
        let ps = Command::new("ps")
            .args(["-t", t, "-o", "pid=,comm="])
            .output()
            .expect("chạy được ps");
        for line in String::from_utf8_lossy(&ps.stdout).lines() {
            let mut it = line.split_whitespace();
            if let (Some(pid), Some(comm)) = (it.next(), it.next()) {
                if comm == "claude" {
                    let kq = Command::new("kill").args(["-TERM", pid]).status();
                    println!("TERM claude pid {pid} trên {t} → {kq:?}");
                }
            }
        }
    }
    let mut closed = false;
    for _ in 0..60 {
        sleep(Duration::from_secs(1));
        let busy = osa(&format!(
            "tell application \"Terminal\" to return busy of selected tab of window id {w}"
        ));
        if busy == "false" {
            osa(&format!(
                "tell application \"Terminal\" to close (every window whose id is {w})"
            ));
            closed = true;
            break;
        }
    }
    println!("cửa sổ nháp đã đóng: {closed}");

    assert!(ready, "cửa sổ nháp không lên tới ô nhập — KHÔNG ĐO ĐƯỢC");
    assert!(closed, "phải đóng được cửa sổ nháp, không để lại phiên lạ");
    // Tiền đề: ô phải thật sự NHIỀU dòng, không thì bài này không đo được câu hỏi
    // của nó (TUI có thể gói khối dán thành một nhãn `[Pasted text …]`).
    assert!(
        lines_before >= 2,
        "ô không hiện nhiều dòng ({lines_before}) — KHÔNG ĐO ĐƯỢC vế xuống dòng:\n{before}"
    );
    assert!(
        matches!(cleared, Some(Ok(true))),
        "clear_box phải khai đã sạch: {cleared:?}"
    );
    assert_eq!(
        box_lines(&after),
        0,
        "màn phải xác nhận ô trống — còn chữ tức DEL không xoá qua chỗ xuống dòng:\n{after}"
    );
}
