//! Bắt lúc một phiên **vừa xong việc** hoặc **vừa tắt hẳn**, và nói ra ĐÚNG MỘT LẦN.
//!
//! # Vì sao có tệp này
//!
//! Hà 2026-08-10: *"có bắt được trường hợp đang chạy và dừng lại hoàn toàn
//! không? nếu có thì thể hiện được trên ui và gửi vào tele"*.
//!
//! Đây là thứ ảnh chụp KHÔNG trả lời được, dù nó mang đủ dữ liệu: ảnh chụp nói
//! *"lúc này phiên đang rảnh"*, còn cái người ta cần biết là *"nó VỪA chuyển từ
//! chạy sang rảnh"* — một sự kiện, không phải một trạng thái. Sự kiện chỉ hiện
//! ra khi có hai lượt đo đặt cạnh nhau, nên phải có sổ ghi lượt trước.
//!
//! # Ba luật của cái loa này
//!
//! 1. **Nói một lần.** Vòng chạy lặp mỗi ~10 giây; báo theo trạng thái thay vì
//!    theo chuyển-trạng-thái là một cái điện thoại rung mãi không thôi, và một
//!    cái loa như thế thì người ta tắt — mất luôn cả những lần đáng nghe.
//! 2. **Lượt đầu im.** Khi hub vừa khởi động lại, sổ trống nên MỌI phiên đều
//!    "mới thấy lần đầu". Báo hết là một tràng tin cho những việc xảy ra lúc
//!    hub còn chưa chạy. Lượt đầu chỉ ghi sổ, không nói gì.
//! 3. **Biến mất cũng là kết thúc.** `claude agents` bỏ một phiên đã dừng khỏi
//!    danh sách sau vài giây (đã ghi ở `pipeline::STOPPED_KEY`), nên phần lớn
//!    lần "tắt hẳn" KHÔNG đi qua trạng thái `dead` — nó chỉ đơn giản là không
//!    còn trong danh sách nữa. Chỉ rình `host == "dead"` là bỏ lọt gần hết.

use std::collections::BTreeMap;

use crate::sessions::LiveSession;

/// Trạng thái của một phiên, rút gọn còn đúng thứ cần để so hai lượt.
pub const WORKING: &str = "working";
pub const IDLE: &str = "idle";
pub const DEAD: &str = "dead";

/// Một chuyện vừa xảy ra, đáng để làm phiền chủ máy.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Đang chạy → đứng lại ở dấu nhắc. Lượt việc đã xong.
    Finished { id: String, name: String },
    /// Còn sống → tắt hẳn (hoặc rời khỏi danh sách).
    Ended { id: String, name: String, was_working: bool },
}

impl Change {
    /// Câu nói cho phòng chat và cho Telegram — cùng một câu, vì hai nơi ấy
    /// phải kể cùng một chuyện. Khác câu là sau này không ai đối chiếu được.
    pub fn say(&self) -> String {
        match self {
            Change::Finished { name, .. } => {
                format!("✅ {name} vừa chạy xong — phiên đang đứng ở dấu nhắc, chờ lượt sau.")
            }
            Change::Ended { name, was_working: true, .. } => {
                format!("⏹ {name} đã TẮT HẲN khi đang chạy dở — nếu không phải bạn dừng thì nên xem lại.")
            }
            Change::Ended { name, .. } => format!("⏹ {name} đã tắt hẳn."),
        }
    }
}

/// Trạng thái rút gọn của một phiên trong ảnh chụp lúc này.
fn state_of(s: &LiveSession) -> &'static str {
    if s.host == "dead" {
        DEAD
    } else if s.working {
        WORKING
    } else {
        IDLE
    }
}

/// So sổ cũ với ảnh chụp mới → những chuyện đáng nói + sổ mới.
///
/// Thuần: không đọc đĩa, không gọi mạng, nên kiểm được đủ mọi ca mà không cần
/// một cái máy đang chạy `claude`.
///
/// `first_run` (sổ cũ rỗng) trả về **không sự kiện nào** — xem luật 2 ở đầu tệp.
pub fn changes(
    prev: &BTreeMap<String, String>,
    now: &[LiveSession],
) -> (Vec<Change>, BTreeMap<String, String>) {
    let mut next: BTreeMap<String, String> = BTreeMap::new();
    let mut out: Vec<Change> = Vec::new();
    let first_run = prev.is_empty();

    for s in now {
        let state = state_of(s);
        let before = prev.get(&s.session_id).map(String::as_str);
        // Phiên đã chết vẫn nằm trong danh sách vài giây; đừng ghi nó vào sổ
        // mới, nếu không lần sau nó lại "biến mất" và báo tắt lần thứ hai.
        if state != DEAD {
            next.insert(s.session_id.clone(), state.to_string());
        }
        if first_run {
            continue;
        }
        match (before, state) {
            (Some(WORKING), IDLE) => out.push(Change::Finished {
                id: s.session_id.clone(),
                name: s.name.clone(),
            }),
            (Some(was), DEAD) => out.push(Change::Ended {
                id: s.session_id.clone(),
                name: s.name.clone(),
                was_working: was == WORKING,
            }),
            _ => {}
        }
    }

    // Rời khỏi danh sách = đã tắt. Đây mới là đường CHÍNH, không phải `dead`.
    if !first_run {
        let seen: Vec<&String> = now.iter().map(|s| &s.session_id).collect();
        for (id, was) in prev {
            if seen.contains(&id) {
                continue;
            }
            out.push(Change::Ended {
                id: id.clone(),
                // Tên đã đi mất cùng danh sách; id ngắn còn hơn một chỗ trống.
                name: format!("phiên {}", &id[..id.len().min(8)]),
                was_working: was == WORKING,
            });
        }
    }
    (out, next)
}
