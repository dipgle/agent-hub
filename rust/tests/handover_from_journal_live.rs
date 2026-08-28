//! Phép DÒ chạy thật: bản bàn giao dựng từ nhật ký của MỘT PHIÊN CÓ THẬT.
//!
//! Gắn `#[ignore]` vì nó đọc nhật ký thật trên máy này. Gọi tay:
//!
//! ```text
//! HUB_LIVE_SESSION=93479f95-17ce-43d1-b9df-83b1a091ca75 \
//!   cargo test --offline --test handover_from_journal_live -- --ignored --nocapture
//! ```
//!
//! 🔴 Vì sao phải DÒ chứ không tin bài kiểm dựng sẵn: `handover_from_journal`
//! sinh ra để chạy đúng lúc tài khoản đã hết hạn mức (Hà 2026-08-28, acc3:
//! *"You've hit your session limit"*). Một bản bàn giao THÔ mà vô dụng thì tệ
//! hơn không có — phiên mới sẽ mở ra rồi ngồi nhìn một mớ chữ không nói được
//! việc gì còn dở. Cái duy nhất trả lời được câu ấy là **đọc bản thật**.
//!
//! Bài này CHỈ ĐỌC: không mở cửa sổ, không đóng phiên nào, không tốn hạn mức.
//!
//! 🔴 Và nó GIỮ NGUYÊN như thế. Ngày 28/08 tôi định thêm một nhánh
//! `HUB_LIVE_MOVE_TO=acc1` để tự chuyển thật một phiên; bộ gác chặn, và nó chặn
//! ĐÚNG: mượn một bài kiểm để THI HÀNH một việc thật là chạy tắt, đúng thứ
//! `CLAUDE.md §12` cấm. Nghiệm thu việc chuyển tài khoản phải đi bằng đường
//! THẬT — chủ máy gõ `/handover -a acc1` trên Telegram.

#[test]
#[ignore = "đọc nhật ký một phiên thật — chạy tay bằng --ignored"]
fn what_a_journal_handover_actually_looks_like() {
    let sid = std::env::var("HUB_LIVE_SESSION").expect("cần HUB_LIVE_SESSION=<id phiên>");
    let cfg = huba::config::load(None).expect("đọc được cấu hình");
    let live = huba::sessions::snapshot(&cfg);
    let s = live
        .sessions
        .iter()
        .find(|s| s.session_id.starts_with(&sid) || sid.starts_with(&s.session_id))
        .unwrap_or_else(|| {
            panic!(
                "không thấy phiên {sid} trong {} phiên đang sống",
                live.sessions.len()
            )
        });

    println!("phiên : {} ({}) · acc {}", s.name, s.session_id, s.account);
    match huba::sessions::handover_from_journal(&cfg, s) {
        None => panic!(
            "nhật ký của {} không có lượt nói nào — đường lui này sẽ KHÔNG cứu được nó",
            s.name
        ),
        Some(cp) => {
            println!("--- BẢN BÀN GIAO ({} ký tự) ---\n{cp}", cp.chars().count());
            assert!(
                cp.contains("dựng TỪ NHẬT KÝ"),
                "bản thô phải TỰ KHAI là thô — đưa nó ra như bản phiên tự viết là để \
                 người đọc tưởng mình đang cầm thứ mình không cầm"
            );
            assert!(
                cp.chars().count() > 200,
                "bản bàn giao {} ký tự thì không bàn giao được gì",
                cp.chars().count()
            );
        }
    }
}
