//! `/clean` chạy THẬT: dựng hàng chờ trong một cửa sổ nháp rồi dọn sạch nó.
//!
//! 🔴 Vì sao phải live: bài kiểm thuần (`tests/clean_queue.rs`) chỉ nói hub ĐẾM
//! đúng hàng chờ. Nó không nói cú `↑` có lấy được tin ra khỏi hàng không, cũng
//! không nói cái CR mà `do script` kèm sẵn có gửi tin ấy đi lại không — mà đúng
//! hai chuyện ấy mới quyết định `/clean` là dọn hay là làm loạn.
//!
//! KHÔNG chạy trên phiên của chủ máy: nó gõ phím và xoá chữ. Cửa sổ dùng để đo
//! phải do chính bài kiểm mở ra, và nó tự đóng lại.
//!
//! ```
//! cd ~/projects/hub/rust
//! cargo test --offline --test clean_queue_live -- --ignored --nocapture
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

/// Gõ một dòng vào cửa sổ ấy — đúng đường `do script` mà hub dùng.
fn say(w: i64, line: &str) {
    osa(&format!(
        "tell application \"Terminal\" to do script \"{line}\" in selected tab of window id {w}"
    ));
}

#[test]
#[ignore = "mở một cửa sổ Terminal nháp chạy `claude` rồi đóng — chạy tay bằng --ignored"]
fn the_queue_really_empties() {
    let dir = std::env::var("HOME").unwrap() + "/projects/hub/.tmp/queueprobe";
    std::fs::create_dir_all(&dir).ok();
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
    sleep(Duration::from_secs(6));

    // Cho phiên BẬN đủ lâu để hàng chờ còn nguyên trong lúc đo. `sleep` chạy
    // bằng Bash tool của chính nó, nên đây là trạng thái bận THẬT.
    //
    // ⚠ CHỜ THEO ĐIỀU KIỆN, KHÔNG THEO ĐỒNG HỒ. Lượt chạy thứ hai (18/08) dựng
    // được 0 tin vì 12 giây chưa đủ để phiên khởi động lượt `sleep`: chữ gõ vào
    // lúc nó còn rảnh thì chạy ngay chứ không xếp hàng. Một bài kiểm ngủ theo
    // giây là bài kiểm đỏ theo nhịp máy.
    // `sleep 90`, không phải 240: bài kiểm này ĐÃ để lại ba cửa sổ nháp đang
    // chạy một lượt bốn phút, và `/exit` gõ vào một phiên bận thì cũng chỉ xếp
    // hàng. Việc dọn phải nằm trong tầm tay của chính bài kiểm.
    say(w, "chay lenh bash: sleep 90");
    let mut busy = false;
    for _ in 0..40 {
        sleep(Duration::from_secs(1));
        if hub::keys::is_busy(&screen(w)) {
            busy = true;
            break;
        }
    }

    for line in ["hang cho mot", "hang cho hai", "hang cho ba"] {
        say(w, line);
        sleep(Duration::from_secs(2));
    }
    // Đợi màn KHAI ra hàng chờ trước khi đọc con số: `do script` trả về ngay,
    // TUI vẽ sau.
    let mut before = 0;
    for _ in 0..15 {
        before = hub::keys::queued_count(&screen(w));
        if before >= 2 {
            break;
        }
        sleep(Duration::from_secs(1));
    }
    println!("trước khi dọn: {before} tin trong hàng chờ");

    let cleaned = hub::keys::clear_queue(w);
    let after = hub::keys::queued_count(&screen(w));
    println!("sau khi dọn: {after} · kết quả clear_queue = {cleaned:?}");

    // 🔴 DỌN TRƯỚC KHI PHÁN, và dọn theo đúng thứ tự CLI đòi. Bản trước đặt một
    // `assert!` ở giữa (phiên có chịu bận không) nên lượt chạy đỏ **thoát khỏi
    // hàm trước bước dọn**, để lại ba cửa sổ `claude` lạ trong danh sách của chủ
    // máy — đúng thứ Hà vừa hỏi sáng nay (*"Sao tôi mở 3 phiên giờ nhảy lên
    // thành 4"*). Một bài kiểm dọn-dẹp-có-điều-kiện là một bài kiểm sẽ có ngày
    // không dọn.
    //
    // Thứ tự: ESC cắt lượt đang chạy → `/exit` → một Enter RỜI → chỉ đóng khi
    // tab đã rảnh (đóng lúc còn tiến trình sẽ bật hộp thoại "terminate running
    // processes?", và một hộp thoại như thế gags mọi lệnh automation sau nó).
    osa(&format!(
        "tell application \"Terminal\" to do script (ASCII character 27) in selected tab of window id {w}"
    ));
    sleep(Duration::from_secs(2));
    say(w, "/exit");
    sleep(Duration::from_secs(2));
    osa(&format!(
        "tell application \"Terminal\" to do script (ASCII character 13) in selected tab of window id {w}"
    ));
    let mut closed = false;
    for _ in 0..20 {
        sleep(Duration::from_secs(1));
        let still_busy = osa(&format!(
            "tell application \"Terminal\" to return busy of selected tab of window id {w}"
        ));
        if still_busy == "false" {
            osa(&format!(
                "tell application \"Terminal\" to close (every window whose id is {w})"
            ));
            closed = true;
            break;
        }
    }
    println!("cửa sổ nháp đã đóng: {closed}");

    assert!(busy, "phiên nháp không chịu bận — không dựng được hàng chờ");
    assert!(closed, "phải đóng được cửa sổ nháp, không để lại phiên lạ");

    // ⚠ KHÔNG đòi đúng 3. Lượt chạy đầu (18/08) dựng được 2: phiên tiêu thụ tin
    // xếp hàng NGAY trong lượt đang chạy khi nó chạm tới điểm đọc, nên số tin
    // còn nằm chờ lúc đo là chuyện của nhịp máy, không phải của `/clean`. Đòi
    // một con số cố định là làm bài kiểm đỏ vì phép đo trong khi sản phẩm đúng —
    // và nó đã đỏ đúng như thế một lần.
    assert!(before >= 2, "phải dựng được hàng chờ để mà đo: {before}");
    let (removed, left) = cleaned.expect("clear_queue không được lỗi");
    assert_eq!(left, 0, "hàng chờ phải sạch, còn {left}");
    assert_eq!(removed, before, "phải xoá đúng bấy nhiêu tin");
    assert_eq!(after, 0, "màn phải xác nhận hàng chờ trống");
}
