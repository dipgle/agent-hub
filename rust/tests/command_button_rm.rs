//! Dòng `rm …` là một LỆNH — nó phải có ▶️/🖥 như mọi dòng lệnh khác.
//!
//! 🔴 Hà 2026-08-18, ảnh chụp một tin `/shot` của phiên `[AI/tfl5]`: *"Có lệnh
//! nhưng lại không có nút chạy"*. Dòng ấy là dòng duy nhất còn lại để đóng sổ:
//!
//! ```text
//! rm ~/projects/AI/tfl5/ide/src/__tests__/deploy_domains_by_role.test.jsx
//! ```
//!
//! Và nó không những mất ▶️ — đường dẫn trong nó còn mọc một nút 📎 TẢI VỀ,
//! đúng cái tệp dòng ấy bảo xoá. Hai triệu chứng, một gốc.
//!
//! **Gốc: cổng `destructive` gỡ 16/08 nhưng `KNOWN` thì không.** Hôm ấy Hà nói
//! *"tôi ở tele là phải gọi lệnh thao tác như ngồi máy thì chặn khác gì chặt
//! tay"*, và cái cổng chặn `rm`/`kill`/`git reset --hard` bị gỡ hẳn. Nhưng danh
//! sách động từ mà bộ bóc lệnh hỏi TRƯỚC cổng ấy chưa bao giờ có `rm`. Nên cái
//! chặn không hề biến mất, nó chỉ lùi lên một tầng — và im lặng hơn hẳn tầng cũ
//! (tầng cũ ít ra còn là một cổng có tên).
//!
//! Cùng hình dạng với ca `.docx` tối nay: **gỡ một luật mà quên gỡ những thứ
//! dựng lên để phục vụ nó.**
//!
//! Fixture là NGUYÊN VĂN tin ấy lấy từ `logs/hub.log`
//! (`channel_command_handled`, `kind: Shot`, 16:44:33Z, 1927 byte).

use std::path::Path;

fn shot_text() -> String {
    let p =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shot-rm-line-2026-08-18.txt");
    std::fs::read_to_string(p).expect("fixture nằm cạnh bài kiểm")
}

const RM_LINE: &str = "rm ~/projects/AI/tfl5/ide/src/__tests__/deploy_domains_by_role.test.jsx";
const TEST_FILE: &str = "~/projects/AI/tfl5/ide/src/__tests__/deploy_domains_by_role.test.jsx";

/// Triệu chứng Hà đọc được: dòng ấy phải nằm trong bộ lệnh bóc ra từ tin.
#[test]
fn a_remove_line_is_a_command() {
    let got = hub::keys::commands_in_report(&shot_text(), 8);
    assert!(
        got.iter().any(|c| c == RM_LINE),
        "dòng rm không được nhận là lệnh nên mất cả ▶️ lẫn 🖥: {got:?}"
    );
}

/// …và một khi nó là lệnh, đường dẫn bên trong thôi mọc nút TẢI VỀ.
///
/// Đo bằng chính bộ lệnh bộ bóc trả ra, KHÔNG tự tay dựng `Cmd`. Bài kiểm
/// 16/08 (`file_button_beside_command`) tự tay dựng, nên nó xanh suốt trong khi
/// sản phẩm đỏ — đúng chỗ mù đã để lọt con bug này hai ngày.
#[test]
fn the_file_it_deletes_gets_no_download_button() {
    let text = shot_text();
    let seen = hub::keys::paths_on_screen(&hub::keys::body_before_box(&text), 4);
    let cmds: Vec<hub::sessions::Cmd> = hub::keys::commands_in_report(&text, 8)
        .into_iter()
        .map(|line| hub::sessions::Cmd {
            line,
            cwd: String::new(),
        })
        .collect();
    let kept = hub::pipeline::paths_not_in_commands(&text, &seen, &cmds);
    assert!(
        !kept.iter().any(|p| p.contains("deploy_domains_by_role")),
        "tệp bị lệnh rm nhắc tới lại mọc nút tải: {kept:?}"
    );
    // Không phải "rỗng là xong": tin ấy còn nhắc `scripts/ci-local.sh` trong
    // một dòng VĂN, và dòng văn thì vẫn được mọc 📎. Khẳng định cái CÒN trước
    // khi khẳng định cái MẤT — một phép lọc trả rỗng cũng qua được assert trên.
    assert!(
        seen.iter().any(|p| p.contains(TEST_FILE) || p == TEST_FILE),
        "bộ dò đường dẫn không hề thấy tệp ấy — hỏng tầng trước, sửa nhầm chỗ: {seen:?}"
    );
}

/// Hàng rào vẫn là hàng rào: `rm` giữa một câu văn không phải lời mời chạy.
#[test]
fn prose_about_removing_is_not_a_command() {
    let prose = "Tôi đã rm mấy tệp thăm dò, giờ cây sạch rồi.\n\
                 Chưa xoá gì thêm.\n";
    let got = hub::keys::commands_in_report(prose, 4);
    assert!(
        got.is_empty(),
        "câu văn có chữ rm bị dựng thành nút chạy: {got:?}"
    );
}

/// Và câu đang CẤM một lệnh xoá vẫn không được mọc nút — cổng `forbids` phải
/// còn nguyên hiệu lực sau khi động từ được nhận.
#[test]
fn a_blocked_remove_line_still_gets_no_button() {
    let screen = "❌ hook chặn: rm -rf ~/projects/AI/tfl5/ide/src/__tests__\n";
    let got = hub::keys::commands_in_report(screen, 4);
    assert!(
        got.is_empty(),
        "câu báo BỊ CHẶN lại thành nút mời chạy: {got:?}"
    );
}
