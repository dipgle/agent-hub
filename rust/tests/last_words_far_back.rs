//! Lời cuối phiên NÓI vẫn phải tìm ra khi nó nằm sâu dưới một núi lượt công cụ.
//!
//! 🔴 Hà 2026-08-17, `/shot` `[AI/onghut]` ra nguyên một tệp mã: *"Sao phiên này
//! hiện như vậy, biết đằng nào làm tiếp"*. Bản vá đầu (bù lời cuối từ nhật ký)
//! KHÔNG nổ, vì `read_tail` chỉ đọc 256 KB.
//!
//! Đo trên chính nhật ký ấy: 5,66 MB / 401 dòng, lượt CÓ CHỮ cuối cùng ở dòng
//! 172 — **cách cuối tệp 4,56 MB**, vì phiên vừa nuốt cả một tệp mã vào nhật ký.
//! Bài kiểm này dựng lại đúng hình dạng đó, nhỏ hơn nhưng vượt trần 256 KB.

use std::io::Write;

#[test]
fn the_last_spoken_words_are_found_under_a_pile_of_tool_turns() {
    let dir = std::env::temp_dir().join(format!("hub-lastsay-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tạo thư mục tạm");
    let path = dir.join("phien.jsonl");
    let mut f = std::fs::File::create(&path).expect("tạo tệp");

    // Lượt CÓ CHỮ — thứ phải tìm ra.
    writeln!(
        f,
        r#"{{"type":"assistant","timestamp":"2026-08-17T11:45:44Z","message":{{"content":[{{"type":"text","text":"Đã dựng xong site, còn đóng gói zip."}}]}}}}"#
    )
    .unwrap();

    // …rồi một núi lượt chỉ gọi công cụ, đủ dày để đẩy nó ra ngoài khung 256 KB.
    let junk = "x".repeat(4096);
    for i in 0..120 {
        writeln!(
            f,
            r#"{{"type":"assistant","timestamp":"2026-08-17T12:0{}:00Z","message":{{"content":[{{"type":"tool_use","name":"Read","input":{{"file":"{junk}{i}"}}}}]}}}}"#,
            i % 10
        )
        .unwrap();
    }
    drop(f);

    let size = std::fs::metadata(&path).unwrap().len();
    assert!(
        size > 256 * 1024,
        "bài kiểm phải vượt trần 256 KB mới đo đúng thứ cần đo: {size} bytes"
    );

    // 🔴 CHỨNG MINH PHÉP ĐO KHÔNG MÙ: đúng khung cũ (256 KB cuối tệp) thì KHÔNG
    // có lấy một chữ nào — nếu assert dưới vẫn xanh với khung ấy thì bài kiểm
    // này chẳng đo gì cả.
    let whole = std::fs::read_to_string(&path).unwrap();
    let cut = whole.len() - 256 * 1024;
    assert!(
        hub::sessions::last_prose(&whole[cut..], 600).is_none(),
        "khung 256 KB cũ phải mù với ca này, nếu không thì bài kiểm vô nghĩa"
    );

    let said = hub::sessions::last_prose_of_file(&path, 600)
        .expect("phải tìm ra lời cuối dù nó nằm ngoài khung 256 KB");
    assert!(said.contains("Đã dựng xong site"), "tìm nhầm chỗ: {said:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

/// …và phiên chưa nói câu nào thì vẫn là `None` — đừng bịa ra một câu để lấp chỗ.
#[test]
fn a_session_that_never_spoke_stays_silent() {
    let dir = std::env::temp_dir().join(format!("hub-lastsay-none-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tạo thư mục tạm");
    let path = dir.join("phien.jsonl");
    std::fs::write(
        &path,
        r#"{"type":"assistant","timestamp":"2026-08-17T12:00:00Z","message":{"content":[{"type":"tool_use","name":"Read","input":{}}]}}
"#,
    )
    .unwrap();
    assert!(hub::sessions::last_prose_of_file(&path, 600).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
