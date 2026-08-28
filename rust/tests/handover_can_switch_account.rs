//! `/handover -a acc2` — đóng sổ một phiên rồi mở lại nó bằng TÀI KHOẢN KHÁC.
//!
//! 🔴 Hà 2026-08-28: *"tài khoản đang dùng của phiên bị báo hết tokens thì muốn
//! chuyển sang acc khác được không — You've hit your session limit"*, rồi nói rõ
//! hình dạng: *"Giống như cách chuyển phiên khi sắp tới giới hạn context vẫn giữ
//! được ngữ cảnh và thêm là dùng acc khác"*.
//!
//! Đi được là nhờ hình dạng SẴN CÓ, không nhờ thêm máy móc: phiên sau bàn giao
//! **trắng ngữ cảnh**, chỉ mang bản bàn giao làm đề bài — cố ý, từ sự cố
//! 2026-08-12 (mở bằng `--resume` thì phiên mới đã 62% ngữ cảnh sau 3 phút, rồi
//! lại đủ điều kiện đóng sổ ⟹ vòng lặp). Một phiên trắng ngữ cảnh **không cần
//! nhật ký cũ**, nên nó mở bằng tài khoản nào cũng được.
//!
//! Bốn dữ kiện đo được ngày 28/08 trước khi viết một dòng nào:
//! * nhật ký hội thoại DÙNG CHUNG — 3/3 phiên acc3 nằm ở `~/.claude/projects/`,
//!   còn `~/.claude-acc2` và `~/.claude-acc3` có **0** tệp hội thoại;
//! * `claude agents` thì THEO TÀI KHOẢN — acc1 thấy 15 phiên, acc2 thấy 0,
//!   acc3 thấy 5;
//! * bẫy 12/08 (acc2/acc3 chưa "tin" `~/projects` nên cửa sổ đầu đứng ở hộp
//!   *"Quick safety check"*) **đã hết** — `.claude.json` của cả hai nay có mục
//!   cho `/Users/hanguyen/projects`;
//! * huba CHƯA nhận ra câu `You've hit your session limit` — 0 lần trong `src/`.
//!
//! ⚠ Việc còn dở, ghi ra để không ai tưởng là xong: bước VIẾT bản bàn giao
//! (`sessions::fork_call`) vẫn chạy trên tài khoản CŨ (`sessions.rs`, chỗ ghim
//! `CLAUDE_CONFIG_DIR`). Tài khoản đã hết hạn mức thì chính bước ấy chết trước,
//! nên `-a` cứu được ca "sắp hết" chứ chưa cứu được ca "đã hết".

use huba::pipeline::split_flags;

#[test]
fn the_account_flag_is_lifted_out_of_the_session_id() {
    let (co, con_lai) = split_flags("-a acc2 574e5be2", &["a", "acc"]);
    assert_eq!(co.get("a").map(String::as_str), Some("acc2"));
    assert_eq!(
        con_lai.trim(),
        "574e5be2",
        "id phiên phải còn nguyên sau khi bóc cờ — nuốt mất nó là đóng sổ nhầm phiên"
    );
}

#[test]
fn the_flag_may_come_after_the_id() {
    let (co, con_lai) = split_flags("574e5be2 -a acc3", &["a", "acc"]);
    assert_eq!(co.get("a").map(String::as_str), Some("acc3"));
    assert_eq!(con_lai.trim(), "574e5be2");
}

/// Không cờ ⟹ KHÔNG đổi gì. Đây là vế giữ cho bản vá này là CỘNG THÊM: route
/// `/handover` trơn phải hành xử y như trước, vì nó đã nằm trong tay quen.
#[test]
fn without_the_flag_nothing_changes() {
    let (co, con_lai) = split_flags("574e5be2", &["a", "acc"]);
    assert!(
        co.is_empty(),
        "không gõ cờ mà đọc ra cờ ⟹ một lượt đóng sổ bình thường bỗng đổi tài khoản"
    );
    assert_eq!(con_lai.trim(), "574e5be2");
}

/// ĐỐI CHỨNG NGƯỢC: cờ LẠ không được bóc ra khỏi phần còn lại. `split_flags` đã
/// có luật ấy (`/new` dựa vào nó), và ở đây nó gánh thêm một việc: `-x` bị nuốt
/// thì id phiên cụt mất một mẩu, và huba đóng sổ nhầm phiên.
#[test]
fn an_unknown_flag_stays_in_the_text() {
    let (co, con_lai) = split_flags("-x 574e5be2", &["a", "acc"]);
    assert!(co.is_empty(), "cờ lạ không được nhận");
    assert!(
        con_lai.contains("-x") && con_lai.contains("574e5be2"),
        "cờ lạ phải Ở LẠI nguyên văn: {con_lai:?}"
    );
}

/// Lệnh phải TÌM ĐƯỢC. Một đường thoát mà chủ máy không biết là có thì bằng
/// không có — đúng bài học `/win` (`CLAUDE.md`: con số "0 lượt dùng" đo sự VÔ
/// HÌNH, không đo sự vô dụng).
#[test]
fn the_route_advertises_the_account_flag() {
    let r = huba::commands::lookup("handover").expect("bảng phải có route handover");
    assert!(
        r.listed,
        "đóng sổ + đổi tài khoản là đường thoát lúc hết hạn mức — nó phải nằm trong menu ☰"
    );
    assert!(
        r.usage.contains("-a") || r.help.contains("-a"),
        "cú pháp `-a` phải hiện ra ở đâu đó chủ máy đọc được: usage={:?} help={:?}",
        r.usage,
        r.help
    );
}
