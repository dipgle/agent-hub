//! Ô hỏi mật khẩu là một TRẠNG THÁI RIÊNG, và phép đo nó phải đổi được cả hai
//! chiều.
//!
//! 🔴 Hà 2026-08-26: *"chạy ssh sudo thì sẽ hỏi mật khẩu, lệnh terminal thấy rồi
//! nhưng trạng thái lại là dấu nhắc trống là không đúng"*.
//!
//! Cái sai không phải một chuỗi chữ xấu — nó là một lời khai KHÔNG đi qua phép
//! đo nào: câu chào của cửa sổ trần gõ cứng *"dấu nhắc trống"* cho mọi cửa sổ,
//! trong khi nhật ký cùng lúc ghi `terminal_tab_busy_unmatched cli:ssh`. Nay
//! trạng thái ấy đọc từ MÀN, nên nó phải đỏ được khi màn nói ngược lại.
//!
//! Nửa dưới của tệp là ĐỐI CHỨNG NGƯỢC (điều 13①): những màn KHÔNG phải ô mật
//! khẩu mà chữ "password" vẫn nằm đâu đó. Thiếu nửa ấy thì một phép đo trả `true`
//! ở mọi màn cũng "đạt" — và nó sẽ dán nhãn ĐANG HỎI MẬT KHẨU lên một cửa sổ vừa
//! `cat` một tệp cấu hình.

use huba::keys::password_prompt;

/// Ba hình dạng đã gặp thật trên máy này.
#[test]
fn the_three_real_password_prompts_are_recognised() {
    for man in [
        "$ ssh vps-a\n[sudo] password for ha:",
        "Last login: Wed Aug 26\nha@vps-a's password:",
        "$ ssh -i ~/.ssh/id_ed25519 vps-a\nEnter passphrase for key '/Users/ha/.ssh/id_ed25519':",
        // Có khoảng trắng sau dấu hai chấm — con trỏ vẫn nằm ngay đó.
        "[sudo] password for ha: ",
    ] {
        assert!(
            password_prompt(man),
            "màn {man:?} là một ô hỏi mật khẩu — bỏ sót nó là mời chủ máy gõ lệnh vào ô ấy"
        );
    }
}

/// ĐỐI CHỨNG NGƯỢC: chữ "password" có mặt mà KHÔNG phải ô hỏi.
#[test]
fn prose_that_merely_mentions_a_password_is_not_a_prompt() {
    for man in [
        // Dòng cuối là dấu nhắc shell, ô hỏi đã xong từ đời nào.
        "[sudo] password for ha:\nDeploy xong.\n$ ",
        // Chính lệnh vừa gõ có chữ ấy.
        "$ grep -ri password .",
        // Kết quả một lệnh đọc tệp.
        "$ cat .env\nDB_PASSWORD=xxxx",
        // Câu kể, có dấu hai chấm ở giữa chứ không ở cuối.
        "$ echo 'password: đã đổi rồi'\npassword: đã đổi rồi",
        // Màn trống.
        "",
        "   \n\n  ",
    ] {
        assert!(
            !password_prompt(man),
            "màn {man:?} KHÔNG phải ô hỏi mật khẩu — gắn nhãn cho nó là một cảnh báo kêu oan, \
             và một cảnh báo kêu oan thì lượt sau không ai đọc"
        );
    }
}

/// Đo trên DÒNG CUỐI còn chữ, không quét cả màn — nên một ô hỏi thật vẫn nhận ra
/// dù phía trên có bao nhiêu chữ đi nữa, còn một ô đã trả lời xong thì không.
#[test]
fn only_the_last_non_empty_line_decides() {
    let dai = format!("{}\n[sudo] password for ha:", "dòng rác\n".repeat(80));
    assert!(
        password_prompt(&dai),
        "ô hỏi nằm ở dòng cuối thì phải nhận ra"
    );

    let xong = format!("[sudo] password for ha:\n{}", "dòng sau\n".repeat(80));
    assert!(
        !password_prompt(&xong),
        "ô hỏi đã cuộn lên trên thì KHÔNG còn là trạng thái hiện tại"
    );
}
