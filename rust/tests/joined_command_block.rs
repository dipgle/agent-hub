//! Khối lệnh nối bằng `\` phải đi NGUYÊN KHỐI — nhất là cái `cd` mở đầu.
//!
//! 🔴 Hà 2026-08-23, ảnh chụp buồng `[dwork · A-CHUNG]`: *"Khối lệnh chạy gom
//! bị thiếu hẳn `cd` dẫn đến chạy không đúng thư mục"*.
//!
//! huba gắn `▶️` vào **dòng thứ hai** của khối (`git merge --no-edit
//! origin/main && \`) vì nó mở đầu bằng `git` ∈ `KNOWN`, còn dòng `cd …` ở trên
//! thì rơi mất: `after_cd` chỉ nhận `cd X && <lệnh>` khi hai vế nằm CÙNG một
//! dòng, mà ở đây vế sau `&&` chỉ là dấu `\`.
//!
//! Hậu quả không phải lỗi hiển thị: bấm là `git merge` **trên cây khác**.

use huba::keys::commands_in_report;

/// Nguyên văn khối trong ảnh Hà gửi.
const BLOCK: &str = "Một khối, dán một lần. && nối nên cổng đỏ là dừng ngay tại đó:\n\
     cd ~/projects/dwork/dev-chung && \\\n\
     git merge --no-edit origin/main && \\\n\
     bash ~/projects/scripts/dci-cong-tat-ca.sh dev-chung && \\\n\
     git -C ~/projects/dwork/dev merge --ff-only lan/a-chung && \\\n\
     git push origin lan/a-chung && echo \"=== XONG ===\"\n";

#[test]
fn the_block_never_yields_a_fragment_without_its_cd() {
    let got = commands_in_report(BLOCK, 8);
    for c in &got {
        assert!(
            !c.starts_with("git merge --no-edit"),
            "mẩu cụt LỌT RA — bấm nó là merge nhầm cây: {c:?}"
        );
        // Không mẩu nào được mang dấu nối còn treo: một dòng kết bằng `\` là
        // một câu chưa hết, chạy nó là chạy một nửa.
        assert!(!c.trim_end().ends_with('\\'), "còn dấu nối treo: {c:?}");
    }
}

#[test]
fn the_whole_block_arrives_as_one_command_with_its_cd_in_front() {
    let got = commands_in_report(BLOCK, 8);
    let one = got
        .iter()
        .find(|c| c.starts_with("cd ~/projects/dwork/dev-chung"))
        .unwrap_or_else(|| panic!("không thấy khối nào mở đầu bằng cd: {got:#?}"));
    // Đủ cả sáu chặng, đúng thứ tự người viết đã nối.
    for part in [
        "cd ~/projects/dwork/dev-chung &&",
        "git merge --no-edit origin/main &&",
        "bash ~/projects/scripts/dci-cong-tat-ca.sh dev-chung &&",
        "git -C ~/projects/dwork/dev merge --ff-only lan/a-chung &&",
        "git push origin lan/a-chung &&",
        "echo \"=== XONG ===\"",
    ] {
        assert!(one.contains(part), "khối thiếu {part:?}\ntrong: {one:?}");
    }
    assert!(!one.contains('\\'), "dấu nối còn sót trong lệnh: {one:?}");
    assert!(!one.contains('\n'), "lệnh còn xuống dòng: {one:?}");
}

/// Câu văn xung quanh không được dính vào khối.
#[test]
fn the_prose_above_the_block_stays_out_of_it() {
    let got = commands_in_report(BLOCK, 8);
    for c in &got {
        assert!(!c.contains("Một khối, dán một lần"), "nuốt cả câu văn: {c:?}");
    }
}

/// Dòng thường vẫn đi đường cũ — bản vá này chỉ đụng dòng có dấu `\`.
#[test]
fn ordinary_lines_are_untouched() {
    let text = "Chạy giúp tôi:\ngit -C ~/projects/huba push origin main\nXong thì báo.";
    let got = commands_in_report(text, 8);
    assert!(
        got.iter().any(|c| c == "git -C ~/projects/huba push origin main"),
        "{got:#?}"
    );
}

/// `cd X && <lệnh>` trên MỘT dòng vẫn phải chạy — `after_cd` không được hỏng.
#[test]
fn the_single_line_cd_form_still_works() {
    let text = "cd ~/projects/AI/codetrail && git push\n";
    let got = commands_in_report(text, 8);
    assert!(
        got.iter().any(|c| c == "cd ~/projects/AI/codetrail && git push"),
        "{got:#?}"
    );
}

/// Khối kết bằng `\` treo lơ lửng (màn bị cắt) không được biến mất không dấu
/// vết, nhưng cũng không được ra một mẩu cụt mang dấu nối.
#[test]
fn a_dangling_backslash_never_becomes_a_half_command() {
    let text = "cd ~/projects/huba && \\\ncargo test --offline && \\\n";
    for c in commands_in_report(text, 8) {
        assert!(!c.trim_end().ends_with('\\'), "mẩu cụt mang dấu nối: {c:?}");
    }
}
