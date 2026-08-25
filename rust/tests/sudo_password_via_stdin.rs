//! Mật khẩu `sudo` đi bằng **stdin**, không bao giờ qua `argv` — kể cả sudo ở
//! đầu kia của một lệnh `ssh`.
//!
//! 🔴 Hà 2026-08-25: *"trường hợp chạy ssh xong có yc mật khẩu thì với lệnh chạy
//! từ tele sẽ làm thế nào?"* → *"ừ làm đi, tôi sẽ tự thêm vào huba.env"*; rồi
//! 26/08 đặt tên khoá **`HUB_VPS_A_SUDO_PASSWORD`**.
//!
//! Chính cái tên ấy lộ ra chỗ tôi làm hụt: bản đầu chỉ phủ `sudo` CỤC BỘ, và
//! còn có một bài kiểm khẳng định ca của anh — `ssh vps-a 'sudo …'` — **bị
//! loại**. Đọc hụt câu hỏi, không phải mã sai.
//!
//! ĐO ĐƯỢC TRƯỚC KHI VÁ, trên chính máy này:
//! · `hubad` chạy với tty `??` — không có terminal điều khiển;
//! · tiến trình như thế mở `/dev/tty` ra `[Errno 6] Device not configured`.
//!
//! Mà `/dev/tty` đúng là chỗ `sudo` mở để hỏi. Nên nút ▶️ gặp `sudo` là hỏng
//! ngay — không treo (phanh 1 tiếng không bị chạm), nhưng vẫn là việc ngồi ở máy
//! làm được mà từ xa thì không.
//!
//! Ca XA giải được vì `ssh` từ chối đọc mật khẩu **của chính nó** từ stdin,
//! nhưng **chuyển tiếp stdin cho lệnh ở đầu kia**. Không cần PTY, không cần đoán
//! lời nhắc, không cần `sshpass`.

use huba::pipeline::sudo_stdin_plan;

/// `sudo` ngay trên máy này ⟹ host rỗng.
#[test]
fn a_local_sudo_is_planned_for_this_machine() {
    let (host, chay) = sudo_stdin_plan("sudo systemctl restart mailler").expect("phải nhận");
    assert_eq!(host, "");
    assert_eq!(chay, "sudo -S -p '' systemctl restart mailler");
}

/// 🔴 CA CỦA HÀ: `sudo` ở đầu kia của `ssh` — host phải đọc ra `vps-a`, và phần
/// viết lại phải nằm **bên trong** cặp nháy, giữ nguyên cặp nháy ấy.
#[test]
fn a_remote_sudo_over_ssh_is_planned_for_that_host() {
    let (host, chay) =
        sudo_stdin_plan("ssh vps-a \"sudo systemctl restart mailler\"").expect("phải nhận");
    assert_eq!(host, "vps-a", "đọc sai tên host ⟹ tra nhầm khoá");
    assert_eq!(
        chay,
        "ssh vps-a \"sudo -S -p '' systemctl restart mailler\""
    );
}

/// Cờ của `ssh` không được đọc nhầm thành host — kể cả cờ ăn theo một giá trị.
#[test]
fn ssh_flags_are_skipped_when_reading_the_host() {
    let (host, _) = sudo_stdin_plan("ssh -p 2222 -o BatchMode=yes vps-a 'sudo ls'").expect("nhận");
    assert_eq!(host, "vps-a", "đọc `2222` hay `BatchMode=yes` thành host");
}

/// `user@host` giữ nguyên cả chuỗi — chỗ tra khoá sẽ thử chuỗi đầy đủ trước, vì
/// hai tài khoản trên cùng một máy có thể có hai mật khẩu.
#[test]
fn a_user_at_host_keeps_both_halves() {
    let (host, _) = sudo_stdin_plan("ssh deploy@vps-a 'sudo ls'").expect("nhận");
    assert_eq!(host, "deploy@vps-a");
}

/// Giữ `cd …` ở đầu — thư mục quyết định lệnh chạy ở đâu.
#[test]
fn the_cd_prefix_survives() {
    let (host, chay) =
        sudo_stdin_plan("cd /opt/mailler && sudo docker compose up -d").expect("nhận");
    assert_eq!(host, "");
    assert!(chay.starts_with("cd /opt/mailler &&"), "{chay}");
    assert!(
        chay.contains("sudo -S -p '' docker compose up -d"),
        "{chay}"
    );
}

/// 🔴 HÀNG RÀO NGƯỢC — vế đắt nếu sai. Bơm mật khẩu vào stdin của một chuỗi mà
/// `sudo` nằm GIỮA là đưa mật khẩu cho lệnh đứng trước đọc trước.
#[test]
fn the_gate_stays_shut_when_sudo_is_not_first() {
    for l in [
        "cat /etc/hosts && sudo reboot",
        "ssh vps-a 'cat /etc/hosts && sudo reboot'",
        "echo sudo",
        "sudoedit /etc/hosts",
        "git push origin main",
        "",
    ] {
        assert!(
            sudo_stdin_plan(l).is_none(),
            "cổng mở cho một dòng không được phép: {l}"
        );
    }
}

/// 🔴 Điều quan trọng nhất, phát biểu thành khẳng định đọc được: chuỗi ĐEM CHẠY
/// không mang mật khẩu ở bất kỳ đâu.
///
/// huba HIỆN dòng lệnh ra Telegram, đặt nó làm nhãn nút và ghi vào sổ
/// (`remember_quick`). Mật khẩu trong `argv` không chỉ rời khỏi máy — nó rời
/// khỏi máy KÈM CẢ CÁCH DÙNG, và nằm lại trong lịch sử buồng chat.
#[test]
fn the_rewritten_line_never_carries_the_secret() {
    for goc in [
        "sudo -n systemctl restart mailler",
        "ssh vps-a \"sudo -n systemctl restart mailler\"",
    ] {
        let (_, chay) = sudo_stdin_plan(goc).expect("nhận");
        assert!(
            !chay.contains("echo"),
            "dựng đường ống bơm mật khẩu: {chay}"
        );
        assert!(!chay.contains('|'), "dựng đường ống bơm mật khẩu: {chay}");
        // Phép viết lại chỉ được THÊM đúng hai cờ; bóc chúng ra thì phải còn
        // lại y hệt dòng gốc.
        assert_eq!(chay.replace("-S -p '' ", ""), goc, "đã đổi thêm thứ khác");
    }
}
