//! Nghiệm thu THẬT cho `keys::screen_scrollback` — cuộn cửa sổ thật, đọc thật.
//!
//! `#[ignore]` vì nó động vào cửa sổ Terminal của chủ máy: chạy tay bằng
//! `cargo test --test scroll_read_live -- --ignored --nocapture`, đừng để nó
//! nằm trong lượt `cargo test` thường.
//!
//! 🔴 Vì sao phải có bài kiểm CHẠY THẬT chứ không chỉ unit test cho phép ghép:
//! `CLAUDE.md` của repo này nói thẳng rằng một lượt `cargo test` xanh chưa bao
//! giờ đủ ở đây, và ba thứ dưới đây KHÔNG cái nào quan sát được từ bộ nhớ —
//! bánh xe có tới TUI không, TUI có cuộn không, và cửa sổ có TRỞ VỀ ĐÁY không.
//! Cái thứ ba là cái đắt nhất khi hỏng: nó bỏ cửa sổ chủ máy ở lưng chừng quá
//! khứ, và không assert nào trong bộ nhớ thấy được điều đó.

use std::process::Command;

/// tty của cửa sổ đang chứa bài kiểm này — LEO NGƯỢC cây tiến trình để tìm.
///
/// Không hỏi thẳng pid của mình: bài kiểm có thể được khởi chạy từ một công cụ
/// mà tiến trình ấy KHÔNG gắn terminal nào (`ps` in `??`), và lúc đó cửa sổ thật
/// nằm ở một tổ tiên vài bậc phía trên. Hỏi sai chỗ rồi kết luận "không chạy
/// trong Terminal" là một câu SAI về thế giới — cửa sổ vẫn ở đó.
fn own_tty() -> Option<String> {
    let mut pid = std::process::id().to_string();
    for _ in 0..8 {
        let out = Command::new("ps")
            .args(["-o", "ppid=,tty=", "-p", &pid])
            .output()
            .ok()?;
        let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let mut it = line.split_whitespace();
        let ppid = it.next()?.to_string();
        let tty = it.next().unwrap_or("??");
        if tty != "??" && !tty.is_empty() {
            return Some(format!("/dev/{tty}"));
        }
        if ppid == "1" || ppid.is_empty() {
            return None;
        }
        pid = ppid;
    }
    None
}

#[test]
#[ignore = "động vào cửa sổ Terminal thật — chạy tay với --ignored"]
fn cuon_lay_them_chu_va_tra_man_ve_day() {
    let Some(tty) = own_tty() else {
        panic!("không đọc được tty của chính mình — chạy bài này TRONG một cửa sổ Terminal");
    };
    let window = huba::keys::window_of(&tty)
        .expect("hỏi được Terminal")
        .expect("phải tìm ra cửa sổ mang tty này");

    let truoc = huba::keys::screen_text(window).expect("đọc được màn");
    let cuon = huba::keys::screen_scrollback(window, 12, |_| false).expect("cuộn đọc được");

    // ① lấy thêm được chữ. Nếu phiên vừa mở và chưa có gì để cuộn thì bài kiểm
    //    này vô nghĩa — nói ra chứ không lặng lẽ xanh.
    assert!(
        cuon.chars().count() > truoc.chars().count(),
        "cuộn xong không thêm chữ nào ({} → {}). Nếu cửa sổ này vừa mở và chưa có \
         lịch sử thì chạy lại sau khi đã làm việc một lúc.",
        truoc.chars().count(),
        cuon.chars().count()
    );

    // ② phần đọc được phải CHỨA khung ban đầu — ghép mà nuốt mất phần đang hiện
    //    thì tin gửi đi sẽ thiếu đúng chỗ người đọc đang nhìn.
    let dong_cuoi = truoc
        .lines()
        .rfind(|l| l.trim().len() > 12)
        .expect("khung ban đầu phải có ít nhất một dòng có chữ");
    assert!(
        cuon.contains(dong_cuoi.trim_end()),
        "bản ghép đánh mất dòng cuối của khung ban đầu: {dong_cuoi:?}"
    );

    // ③ MÀN PHẢI VỀ ĐÁY. Đo bằng cách đọc lại khung: nó phải khớp khung ban đầu
    //    ở dòng cuối — nếu còn nằm lưng chừng quá khứ thì dòng cuối đã khác.
    std::thread::sleep(std::time::Duration::from_millis(600));
    let sau = huba::keys::screen_text(window).expect("đọc lại được màn");
    let cuoi_sau = sau.lines().rfind(|l| l.trim().len() > 12);
    assert_eq!(
        cuoi_sau.map(str::trim_end),
        Some(dong_cuoi.trim_end()),
        "cửa sổ KHÔNG trở về đáy — nó đang nằm ở lưng chừng quá khứ"
    );
}
