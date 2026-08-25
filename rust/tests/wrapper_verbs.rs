//! Lệnh thật nấp sau một lớp bọc (`nohup`, `env`, `time`…) vẫn phải có nút.
//!
//! 🔴 Hà 2026-08-24, ảnh `/shot` của `[tfl5]`: *"Sao ko có chạy đc lệnh"*.
//!
//! Dòng trong ảnh dài **180 ký tự** — dưới trần `BTN_CMD_REPORT_MAX` = 200 —
//! không một dấu hiệu văn xuôi nào, mà `commands_in_report` trả về **rỗng**
//! (chạy thẳng hàm sản phẩm để đo, không suy luận).
//!
//! Vì `after_cd` giao lại `nohup bash -c '…'`, và động từ đầu là `nohup` —
//! không có trong `KNOWN`. Hàng rào cố ý hẹp nên nó ĐÚNG khi từ chối `nohup`;
//! cái sai là hỏi nó về từ SAI. `nohup` không chạy gì cả, nó bọc quanh lệnh
//! thật — y hệt `cd X &&`.
//!
//! Bản vá đi đúng đường `after_cd` đã mở: **không nới hàng rào, chỉ hỏi đúng
//! chỗ**.

use huba::keys::commands_in_report;

/// Nguyên văn dòng trong ảnh.
const REAL: &str = "cd ~/projects/AI/tfl5 && nohup bash -c 'bash scripts/upgrade.sh uc2-qr-turnstile > .tmp/upgrade.log 2>&1; echo \"UPGRADE_EXIT=$?\" >> .tmp/upgrade.log' >/dev/null 2>&1 & echo started";

#[test]
fn the_real_line_from_the_screenshot_gets_a_button() {
    let got = commands_in_report(REAL, 8);
    assert!(
        got.iter()
            .any(|c| c.starts_with("cd ~/projects/AI/tfl5 &&")),
        "dòng 180 ký tự, dưới trần, mà vẫn 0 nút:\n{got:#?}"
    );
}

/// Cả họ lớp bọc, có `cd` mở đầu hay không.
#[test]
fn every_wrapper_hands_the_gate_the_real_verb() {
    for w in ["nohup", "env", "time", "stdbuf", "caffeinate", "nice"] {
        let bare = format!("{w} bash scripts/gate.sh");
        assert!(
            !commands_in_report(&bare, 8).is_empty(),
            "{w:?} trần: lệnh thật bên trong không tới được hàng rào"
        );
        let with_cd = format!("cd ~/projects/huba && {w} cargo test --offline");
        assert!(
            !commands_in_report(&with_cd, 8).is_empty(),
            "{w:?} sau cd: lệnh thật bên trong không tới được hàng rào"
        );
    }
}

/// Cờ CỦA CHÍNH lớp bọc không phải lệnh — phải nhìn xuyên qua chúng.
#[test]
fn the_wrappers_own_flags_are_skipped() {
    for line in [
        "env -i bash scripts/gate.sh",
        "nice -n 10 cargo test --offline",
        "time -p bash scripts/gate.sh",
        // Gán biến môi trường cũng thuộc về lớp bọc, không phải lệnh.
        "env RUST_LOG=debug cargo test --offline",
    ] {
        assert!(
            !commands_in_report(line, 8).is_empty(),
            "cờ của lớp bọc nuốt mất lệnh thật: {line:?}"
        );
    }
}

/// 🔴 HÀNG RÀO KHÔNG ĐƯỢC NỚI: lớp bọc quanh một động từ LẠ vẫn phải bị từ chối.
///
/// Đây là chỗ dễ hỏng nhất của bản vá. Nếu chỉ nhét `nohup` vào `KNOWN` thì
/// **mọi** dòng mở đầu bằng nó lọt qua mà lệnh thật bên trong chưa ai nhìn —
/// tức nới hàng rào bằng cửa sau, đúng thứ chú thích của `after_cd` đã cấm.
#[test]
fn a_wrapper_around_an_unknown_verb_is_still_refused() {
    for line in [
        "nohup blahblah --do-it",
        "env quackquack run",
        "time wibble --now",
        "cd ~/projects/huba && nohup zzzzz --go",
    ] {
        assert!(
            commands_in_report(line, 8).is_empty(),
            "hàng rào bị nới qua cửa sau: {line:?}"
        );
    }
}

/// `sudo` KHÔNG phải lớp bọc ở đây — nó đổi QUYỀN, không chỉ đổi cách chạy.
/// Chủ ý cũ, giữ nguyên.
#[test]
fn sudo_is_not_treated_as_a_wrapper() {
    let got = commands_in_report("sudo bash scripts/gate.sh", 8);
    assert!(
        got.is_empty(),
        "sudo lọt vào họ lớp bọc — nó đổi quyền, không chỉ đổi cách chạy: {got:#?}"
    );
}
