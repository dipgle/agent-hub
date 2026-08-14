//! hub — the Claude CLI sessions running on this Mac, driven from a phone.
//!
//! One Telegram chat carries ORDERS (`/session`, `/ask`, `/new`, `/tell`,
//! `/stop`, `/handover`) and carries back what every session is doing. Nothing
//! here reads mail, and nothing here spends money unless the owner presses a
//! button.
//!
//! 🔴 It carried them alongside a tfl5 chat room and a phone page until
//! 2026-08-14 (Hà: *"tạm thời không dùng tfl5 để xem cứ xóa hết đi"*). The page
//! had been dead for two days without anyone noticing — which is what a
//! one-person product gets for keeping two front doors.
//!
//! It used to be an inbox: GitHub notifications, project devlogs, email and
//! Telegram all fed one queue that a bounded `claude -p` call triaged and
//! answered. That product was deleted on 2026-08-08 — 65% of what it carried
//! was CI noise, and the ceiling built to contain its spending ended up
//! standing between the owner and his own machine.

pub mod adapters;
pub mod commands;
pub mod config;
/// Xác nhận lần hai qua Telegram cho lệnh không lùi lại được.
pub mod confirm;
pub mod db;
pub mod exec;
/// Gõ phím vào cửa sổ terminal của một phiên, và chụp lại cửa sổ ấy.
///
/// Hà chốt 2026-08-09 sau khi tôi nêu rõ đánh đổi. Đọc đầu tệp `keys.rs` trước
/// khi sửa: đường này **bỏ qua `DENIED_TOOLS`**, và nó là đường DUY NHẤT nhìn
/// thấy một phiên đang dừng lại hỏi — câu hỏi ấy nằm trên màn hình chứ chưa vào
/// nhật ký.
pub mod keys;
pub mod logging;
pub mod pipeline;
/// Kept from the inbox era on purpose: `sessions.rs` runs every transcript
/// preview through `leak_scan` before it can travel to a doc on a server. The
/// product that needed a full outbound leak gate is gone; the gate that stops a
/// password from leaving this Mac is not optional.
pub mod redaction;
pub mod runtime;
pub mod sessions;
pub mod setup;
/// Telegram làm kênh RA LỆNH hai chiều, không chỉ cái loa.
pub mod telegram;
pub mod verbs;
pub mod watch;
