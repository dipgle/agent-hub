//! Bốn dòng lệnh trong một tin — huba gắn được mấy nút, và cái nào rớt.
//!
//! 🔴 Hà 2026-08-16, ảnh chụp tin `/shot` của `[AI/mailler]`: *"rõ ràng có 4
//! dòng lệnh, nhưng chỉ có 4 nút chạy"* — trong ảnh đếm được **ba** icon ▶️ ở
//! ba dòng cuối; dòng lệnh ĐẦU TIÊN không có cái nào.
//!
//! Chữ dưới đây chép từ chính ảnh ấy, giữ nguyên thứ tự và cả mấy dòng văn xuôi
//! kẹp giữa — vì chính chúng là thứ bài kiểm phải chứng minh là KHÔNG ăn mất
//! một chỗ nào.

const MSG: &str = "Đã rà lại toàn cây: mọi chỗ còn tên cũ đều là cố ý — dòng gợi ý trong preflight (nếu deploy.sh còn đó thì in lệnh git mv), dòng \"Renamed from ...\" trong\n\
header, và các mục nhật ký theo ngày ở memory/ + docs/decision-log.md (chúng ghi thứ đã chạy hôm đó; TODO.md có mục nói rõ cách tra ngược).\n\
Anh chạy\n\
git -C ~/projects/AI/mailler mv deploy.sh upgrade.sh\n\
git -C ~/projects/AI/mailler mv scripts/deploy-guard-selfcheck.sh scripts/upgrade-guard-selfcheck.sh\n\
git -C ~/projects/AI/mailler mv web-spa/deploy-webmail.sh web-spa/upgrade-webmail.sh\n\
bash ~/projects/AI/mailler/scripts/upgrade-guard-selfcheck.sh\n\
Ba lệnh đầu ~1 giây, không đổi nội dung file nào.";

const CMDS: [&str; 4] = [
    "git -C ~/projects/AI/mailler mv deploy.sh upgrade.sh",
    "git -C ~/projects/AI/mailler mv scripts/deploy-guard-selfcheck.sh scripts/upgrade-guard-selfcheck.sh",
    "git -C ~/projects/AI/mailler mv web-spa/deploy-webmail.sh web-spa/upgrade-webmail.sh",
    "bash ~/projects/AI/mailler/scripts/upgrade-guard-selfcheck.sh",
];

/// Trước hết ĐO: hàng rào nhận ra mấy dòng là lệnh?
#[test]
fn the_fence_sees_all_four() {
    let got = huba::keys::commands_in_report(MSG, 8);
    println!("hàng rào bắt được {}:", got.len());
    for g in &got {
        println!("  · {g}");
    }
    for want in CMDS {
        assert!(
            got.iter().any(|g| g == want),
            "hàng rào bỏ sót một dòng lệnh: {want}"
        );
    }
}

/// Rồi mới hỏi phần dựng nút: bốn neo, bốn icon, mỗi cái ở dòng của nó.
#[test]
fn all_four_get_an_icon_on_their_own_line() {
    let anchors: Vec<(String, Vec<(String, String)>)> = CMDS
        .iter()
        .enumerate()
        .map(|(i, c)| {
            (
                c.to_string(),
                vec![(format!("https://t.me/b?start=run_{i}"), "▶️".to_string())],
            )
        })
        .collect();
    let (html, linked, unlinked) = huba::pipeline::html_with_links(MSG, &anchors);
    println!("{html}");
    assert!(
        unlinked.is_empty(),
        "có neo không dựng được liên kết: {unlinked:?}"
    );
    assert_eq!(linked, 4, "bốn dòng lệnh thì phải có bốn icon");
    for (i, c) in CMDS.iter().enumerate() {
        let want = format!("<code>{c}</code> <a href=\"https://t.me/b?start=run_{i}\">▶️</a>");
        assert!(
            html.contains(&want),
            "lệnh {i} không có icon ngay sau nó:\n{c}"
        );
    }
}

/// Và cái bẫy thật của tin này: dòng văn xuôi đầu tiên NHẮC tới `deploy.sh` và
/// `git mv`. Nó không được nuốt mất chỗ của lệnh nào.
#[test]
fn the_prose_that_mentions_git_mv_takes_no_slot() {
    let first_prose = MSG.lines().next().expect("dòng đầu");
    assert!(first_prose.contains("git mv") && first_prose.contains("deploy.sh"));
    let got = huba::keys::commands_in_report(first_prose, 8);
    assert!(
        got.is_empty(),
        "một câu văn kể về lệnh bị đọc thành lệnh: {got:?}"
    );
}
