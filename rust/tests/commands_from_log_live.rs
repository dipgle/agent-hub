//! Đo nguồn lệnh MỚI trên NHẬT KÝ THẬT của máy này — không phải trên mẫu dựng.
//!
//! Gắn `#[ignore]` vì nó đọc `~/.claude/projects` thật: kết quả đổi theo từng
//! ngày, nên nó không phải một chốt canh mà là một **phép đo**. Gọi tay:
//!
//! ```text
//! cargo test --offline --test commands_from_log_live -- --ignored --nocapture
//! ```
//!
//! Vì sao phải có, thay vì tin vào bộ test thuần: bộ kia tôi TỰ dựng đầu vào,
//! nên nó chỉ chứng minh mã làm đúng thứ tôi nghĩ nhật ký trông như thế nào.
//! Đúng cái bẫy tệp này ra đời để tránh — nguồn cũ đọc màn cũng "đúng" theo
//! nghĩa ấy suốt hai tuần. Ở đây đầu vào là nhật ký thật, và thứ đáng nhìn là
//! HAI con số: có bao nhiêu phiên ra lệnh, và những lệnh ấy trông có giống một
//! dòng đáng bấm không.
//!
//! Đây là **thăm dò/đọc**, không phải nghiệm thu: nghiệm thu thật là một cú
//! `/shot` gõ trên Telegram và nhìn cái nút hiện ra.

use std::time::{Duration, SystemTime};

#[test]
#[ignore = "đọc nhật ký thật trên máy — chạy tay bằng --ignored"]
fn what_the_new_source_finds_in_real_transcripts() {
    let root = dirs_home().join(".claude/projects");
    let projects = std::fs::read_dir(&root).expect("đọc ~/.claude/projects");

    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for dir in projects.flatten() {
        let Ok(entries) = std::fs::read_dir(dir.path()) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                continue;
            }
            // Chỉ nhìn thứ còn sống: một phiên tuần trước không nói được gì về
            // cái nút hôm nay.
            let fresh = e
                .metadata()
                .and_then(|m| m.modified())
                .map(|t| {
                    SystemTime::now()
                        .duration_since(t)
                        .unwrap_or(Duration::ZERO)
                        < Duration::from_secs(2 * 86_400)
                })
                .unwrap_or(false);
            if fresh {
                files.push(p);
            }
        }
    }

    let mut with_cmds = 0usize;
    let mut samples: Vec<String> = Vec::new();
    for f in &files {
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        // Cùng cửa sổ đuôi mà `read_tail` dùng, để con số đo được là con số hub
        // thật sự nhìn thấy chứ không phải con số của cả tệp.
        let tail = tail_of(&text, 256 * 1024);
        let got = hub::sessions::commands_in_last_turn(&tail, 4);
        if got.is_empty() {
            continue;
        }
        with_cmds += 1;
        if samples.len() < 25 {
            let name = f.file_name().unwrap_or_default().to_string_lossy();
            samples.push(format!("  {}… → {:?}", &name[..8.min(name.len())], got));
        }
    }

    println!("nhật ký 2 ngày gần nhất: {}", files.len());
    println!("phiên có lệnh dựng được nút: {with_cmds}");
    for s in &samples {
        println!("{s}");
    }
    // Không assert một con số: nó đổi theo ngày, mà một chốt canh đổi theo ngày
    // là một chốt canh sẽ bị tắt. Thứ duy nhất phải đúng ở mọi ngày: đọc được.
    assert!(!files.is_empty(), "không thấy nhật ký nào — sai đường?");
}

fn dirs_home() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("HOME").expect("HOME"))
}

/// `read_tail` của `sessions` là private; đây là cùng phép cắt, trên chuỗi.
fn tail_of(text: &str, bytes: usize) -> String {
    if text.len() <= bytes {
        return text.to_string();
    }
    let cut = text.len() - bytes;
    // Cắt theo ranh giới ký tự — nhật ký đầy tiếng Việt.
    let start = (cut..text.len())
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(text.len());
    text[start..].to_string()
}
