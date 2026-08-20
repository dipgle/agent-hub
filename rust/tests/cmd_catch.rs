//! Hai lệnh trong tin `[AI/mailler]` 2026-08-16 — huba bắt được cái nào, vì sao.
//!
//! 🔴 Hà: *"Tin nhắn này có 2 lệnh nhưng chưa thấy bắt được"*. Chữ dưới đây chép
//! từ chính ảnh chụp ấy.

#[test]
fn the_two_commands_in_that_mailler_message() {
    let prose = "Hai thứ rác cần anh gõ tay (tool rm bị hook chặn, cả hai đã gitignore nên không lọt vào commit):\n\
                 rm -f ~/projects/AI/mailler/crates/smtp-in/src/lib.rs.bak\n\
                 git -C ~/projects/AI/mailler worktree remove --force .claude/worktrees/agent-a7032931bd987bb73 && git -C ~/projects/AI/mailler branch -D worktree-agent-a7032931bd987bb73\n\
                 Lệnh thứ hai thu lại 1,4 GB.";
    let got = huba::keys::commands_in_report(prose, 4);
    println!("bắt được {} lệnh:", got.len());
    for g in &got {
        println!("  · {g}");
    }

    // 🔄 ĐẢO CHIỀU 2026-08-18 — bài kiểm này khoá một luật đã bị bãi bỏ.
    //
    // Bản cũ đọc: *"lệnh xoá KHÔNG được thành nút"*, lý do là `keys::destructive`.
    // Cổng ấy Hà gỡ hẳn ngày 16/08 (*"tôi ở tele là phải gọi lệnh thao tác như
    // ngồi máy thì chặn khác gì chặt tay"*) — nhưng `rm` vẫn không có trong
    // `KNOWN`, nên hành vi không đổi và bài kiểm này vẫn xanh, che đúng chỗ hở
    // suốt hai ngày. Nó xanh vì đo cái cổng đã chết, không phải vì sản phẩm đúng.
    //
    // Giữ bài kiểm, đảo lời hứa: từ nay "vá lại cho an toàn" là làm ĐỎ một bài
    // kiểm có chủ, không phải bịt một chỗ hở. Cùng cách xử lý ba bài kiểm của
    // luật 5 hôm 16/08 (`tests/sessions.rs`).
    //
    // Và chính dòng này là ca đáng có nút nhất: câu ngay trên nó nói *"tool rm
    // bị hook chặn"*, tức phiên KHÔNG tự chạy được và đang nhờ chủ máy gõ tay —
    // trên điện thoại thì gõ tay một đường dẫn dài là đúng thứ cây cầu sinh ra
    // để khỏi phải làm.
    assert!(
        got.iter().any(|c| c.starts_with("rm ")),
        "lệnh xoá phải có nút — cổng chặn nó đã gỡ từ 16/08: {got:?}"
    );

    // Lệnh `git … worktree remove` thì phải bắt được — nó không nằm trong danh
    // sách phá huỷ, và đây đúng loại việc chủ máy muốn bấm một cái là xong.
    assert!(
        got.iter().any(|c| c.contains("worktree remove")),
        "lệnh git worktree KHÔNG được nhận: {got:?}"
    );
}

/// Một câu BÀN VỀ mật khẩu không được làm mất nút của cả lượt.
///
/// 🔴 Đây là thứ đã xảy ra thật: log `cmd_source_prose_withheld
/// {"why":["credential_word"]}` lúc 09:33, vì báo cáo có cụm *"2FA cho
/// IMAP/POP3/SMTP — cần app-password"*. Cửa cũ quét CẢ LƯỢT nên bỏ sạch lệnh —
/// trong khi chính câu ấy vẫn đi ra Telegram qua `/shot`. Cửa nay đứng trên
/// từng DÒNG LỆNH, chỗ bí mật thật sự có thể rời khỏi máy.
#[test]
fn a_sentence_about_passwords_does_not_kill_the_whole_turn() {
    let prose = "Còn OPEN: V12 (2FA cho IMAP/POP3/SMTP — cần app-password, là thay đổi sản phẩm).\n\
                 git -C ~/projects/AI/mailler worktree remove --force .claude/worktrees/agent-a70329\n\
                 Chạy mất vài giây.";
    let got = huba::keys::commands_in_report(prose, 4);
    assert_eq!(
        got.len(),
        1,
        "câu bàn về app-password không được bỏ mất lệnh: {got:?}"
    );

    // …còn một dòng lệnh MANG bí mật thì vẫn phải bị giữ lại. Cửa ấy nay nằm ở
    // `sessions::commands_in_last_turn`, cân bằng `redaction::file_risk`.
    assert!(
        !huba::redaction::file_risk("PGPASSWORD=hunter2 psql -h db.example.com -U admin")
            .is_empty(),
        "dòng lệnh mang mật khẩu phải bị cân là có rủi ro"
    );
    // …và cân ấy KHÔNG được cắn một lệnh chỉ có đường dẫn tuyệt đối — đó là
    // hình dạng thường nhất của lệnh trong workspace này.
    assert!(
        huba::redaction::file_risk(
            "git -C /Users/hanguyen/projects/AI/mailler worktree remove --force x"
        )
        .is_empty(),
        "đường dẫn tuyệt đối KHÔNG phải bí mật — cân rộng thì mọi lệnh đều mất nút"
    );
}
