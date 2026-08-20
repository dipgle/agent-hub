//! Hai dòng lệnh liền nhau — huba gắn được mấy cái nút, và vì sao không phải hai.
//!
//! 🔴 Hà 2026-08-16, ảnh chụp tin của `[mailler]`: *"chỗ này tại sao chỉ rend
//! được một lệnh, mà không biết lệnh đó ăn 1 dòng hay cả 2?"*. Trong ảnh, icon
//! ▶️ chỉ đứng ở cuối dòng THỨ HAI; dòng thứ nhất trần trụi. Một icon lẻ giữa
//! hai dòng lệnh thì người đọc không đọc ra nó thuộc về dòng nào — mà bấm nhầm
//! ở đây là chạy một lệnh khác lệnh mình định chạy.
//!
//! Chữ dưới đây chép nguyên văn từ chính ảnh ấy.

const REPORT: &str = "chết. Tôi thử git mv hai lần, hook reviewer chặn cả hai (mọi lệnh nêu tên file đó đều bị từ chối), nên theo luật 2-lần thì dừng và đưa anh:\n\
                      git -C ~/projects/AI/mailler mv deploy.sh update.sh\n\
                      bash ~/projects/AI/mailler/scripts/deploy-guard-selfcheck.sh\n\
                      Lệnh đầu ~1 giây; lệnh sau ~10-20 giây (clone tạm, không đụng mạng) và phải in ra các dòng ✓.";

#[test]
fn how_many_of_the_two_are_caught() {
    let got = huba::keys::commands_in_report(REPORT, 4);
    println!("bắt được {} lệnh:", got.len());
    for g in &got {
        println!("  · {g}");
    }
    // Đây là phép ĐO trước đã: in ra rồi mới chốt. Cái phải đúng là huba không
    // được bắt NHẦM một dòng văn xuôi thành lệnh.
    for g in &got {
        assert!(
            !g.starts_with("Lệnh đầu") && !g.starts_with("chết."),
            "bắt nhầm văn xuôi thành lệnh: {g}"
        );
    }
}

/// Dòng `git … mv` KHÔNG nằm trong danh sách phá hoại (`git rm` có, `git mv`
/// không), nên nếu nó rớt thì rớt vì một hàng rào khác — và bài kiểm này nói ra
/// đúng hàng rào ấy thay vì để câu hỏi treo.
#[test]
fn the_git_mv_line_is_not_classed_as_destructive() {
    let cmd = "git -C ~/projects/AI/mailler mv deploy.sh update.sh";
    let got = huba::keys::commands_in_report(cmd, 4);
    println!("một mình dòng git mv → {got:?}");
    assert_eq!(
        got.len(),
        1,
        "đứng một mình thì dòng này PHẢI ra một lệnh — nếu không, hàng rào chặn \
         nó nằm ngoài chuyện 'phá hoại' và phải gọi tên được"
    );
}

/// Và dòng `bash …selfcheck.sh` cũng vậy — để so sánh hai bên trên cùng một
/// phép đo, chứ không suy từ ảnh chụp.
#[test]
fn the_bash_line_stands_alone_too() {
    let cmd = "bash ~/projects/AI/mailler/scripts/deploy-guard-selfcheck.sh";
    let got = huba::keys::commands_in_report(cmd, 4);
    println!("một mình dòng bash → {got:?}");
    assert_eq!(got.len(), 1);
}
