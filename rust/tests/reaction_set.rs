//! Mọi dấu huba định THẢ lên một tin nhắn phải nằm trong bộ Telegram cho phép.
//!
//! 🔴 Hà 2026-08-23: *"sao phản hồi đã gửi của tin nhắn cứ nhảy đi nhảy lại khi
//! làm có các cập nhật mới thế"*. Nhìn thì tưởng lỗi cách gửi, gốc lại nằm ở
//! đây: `setMessageReaction` trả `Bad Request: REACTION_INVALID` **11 lần trong
//! một buổi** (nhật ký 23/08), nên huba rơi về đường CHỮ — và dòng chữ ấy khi
//! thì bị sửa tại chỗ, khi thì gửi mới, tức nó *nhảy*.
//!
//! Thủ phạm là lượt nở bảng dấu dự án 20 → 59 ô ngày 2026-08-20: 39 ô thêm vào
//! là emoji hợp lệ nhưng KHÔNG phải reaction hợp lệ. Bảng ấy nở ra để hai dự án
//! đừng trùng dấu — mà một cái dấu không thả được thì chẳng phân biệt được gì,
//! nó chỉ đổi một va chạm hiếm lấy một lời từ chối chắc chắn.
//!
//! Bài kiểm hỏi qua CHÍNH `project_emoji`, không đọc bảng hằng bên trong nó:
//! đọc bảng là kiểm một danh sách, hỏi qua hàm là kiểm cái thật sự được gửi đi
//! — kể cả nhánh rơi về `👍`.

use huba::pipeline::{ack_emoji, project_emoji, Ack, REACTIONS};

fn hop_le(e: &str) -> bool {
    REACTIONS.contains(&e)
}

#[test]
fn every_project_mark_is_a_reaction_telegram_accepts() {
    // Tên dự án thật trên máy này, cộng một dải tên sinh ra để quét hết bảng:
    // một ô hỏng nằm ở giữa bảng thì vài cái tên có sẵn sẽ không chạm tới.
    let mut ten: Vec<String> = [
        "huba",
        "tfl5",
        "dwork",
        "sdvi",
        "social",
        "onghut",
        "amm",
        "tcc",
        "beta3",
        "codetrail",
        "mailler",
        "hub",
        "video",
        "uiux",
        "sso-user",
        "games",
        "dwork/A-DSIGN",
        "AI/tcc/amm",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for i in 0..600 {
        ten.push(format!("du-an-{i}"));
    }
    for t in &ten {
        let e = project_emoji(t);
        assert!(
            hop_le(e),
            "dấu {e:?} của dự án {t:?} KHÔNG nằm trong bộ reaction Telegram — \
             thả nó lên tin nhắn sẽ trả REACTION_INVALID rồi rơi về đường chữ"
        );
    }
}

/// Và cả những dấu KHÔNG phụ thuộc tên dự án.
#[test]
fn every_fixed_ack_mark_is_a_reaction_too() {
    for k in [
        Ack::Sent,
        Ack::Seen,
        Ack::Queued,
        Ack::Focused,
        Ack::Running,
        Ack::Stopped,
        Ack::Saved,
    ] {
        let e = ack_emoji(None, k);
        assert!(hop_le(e), "dấu cố định {e:?} của {k:?} không thả được");
    }
}

/// Phép đo phải CHỨNG MINH ĐƯỢC LÀ NÓ BẮT ĐƯỢC LỖI.
///
/// Một bài kiểm "mọi thứ đều hợp lệ" mà bộ so sánh rỗng thì luôn xanh. Đây là
/// ca đối chứng: đúng những ô đã gây ra 11 lần `REACTION_INVALID` hôm 23/08
/// phải bị bộ này bác.
#[test]
fn the_check_can_go_red() {
    for xau in [
        "🌵", "🍄", "🐙", "🦉", "🧲", "🪃", "🎯", "🧊", "🔔", "🪵", "🍿", "🧅",
    ] {
        assert!(
            !hop_le(xau),
            "{xau} lọt vào bộ reaction — bộ so sánh sai, và bài kiểm kia thành vô dụng"
        );
    }
    // …và bộ so sánh không rỗng, không thì mọi phép so đều "không hợp lệ".
    assert!(
        REACTIONS.len() > 50,
        "bộ reaction quá ngắn: {}",
        REACTIONS.len()
    );
    assert!(
        hop_le("👍") && hop_le("⚡"),
        "bộ reaction thiếu cả dấu cơ bản"
    );
}
