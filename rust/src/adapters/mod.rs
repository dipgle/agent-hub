//! The channel hub talks through.
//!
//! Was five: GitHub notifications, project devlogs, email and Telegram all fed
//! an inbox that a bounded `claude -p` call triaged. That product is gone
//! (2026-08-08) — hub is a management channel for the Claude CLI sessions on
//! this machine, and the only channel it needs is the tfl5 chat room the owner
//! opens on his phone. The four ingest adapters were removed with their wiring;
//! `git show backup/inbox-adapters` still has them.
//!
//! The adapter still returns normalized messages plus the cursors it earned;
//! the pipeline commits the messages first and the cursors second, so a crash
//! re-polls instead of skipping.

pub mod tfl5;

use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct PollResult {
    /// How many ordinary chat lines this poll walked past. A COUNT, not the
    /// lines: hub stopped storing what people type on 2026-08-08 when the inbox
    /// was deleted, and a number is all the run row ever needed.
    pub seen: usize,
    pub cursors: BTreeMap<String, String>,
    /// Partial trouble that is not an adapter failure (e.g. one project's
    /// devlog is uninitialized). Recorded on the run row — never silent.
    pub skipped: Option<String>,
    /// Button presses / slash commands that arrived on the channel. The
    /// adapter only parses them; the pipeline is what actually acts, so the
    /// approve path is identical for CLI, Telegram and the web UI.
    pub commands: Vec<ChannelCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Help,
    /// Dựng lại chính hub từ mã hiện tại (`runtime::self_install`).
    ///
    /// 🔴 Hà 2026-08-13: *"tại sao không phải là luồng chạy độc lập trên rust,
    /// tức là mọi lệnh và luồng xử lý phải nằm trong binary"*. Trước route này,
    /// mỗi bản vá của hub đều đòi một người ngồi ở máy gõ `deploy/install.sh` —
    /// tức cây cầu tự nó có một đoạn chỉ đi được khi chủ máy đang ở nhà.
    Upgrade,
    /// Poll every channel now (the console's "Poll kênh").
    Ingest,
    /// Run a full cycle now (the console's "Chạy 1 vòng").
    Run,
    /// Probe channels + tools for real, ignoring the cached reading (the
    /// console's "Kiểm tra").
    Doctor,
    /// Set ONE config field: `arg` is "<dotted.key> <value>".
    SetConfig,
    /// Pin / show / clear the project this conversation is about. `arg` is the
    /// project name, "-" to clear, or empty to report the current one.
    Project,
    /// Focus one Claude CLI session so the next snapshot carries its full
    /// stream. `arg` is the session id, or "-" to stop following.
    ///
    /// Focus rather than "fetch": the page cannot call this machine, so the
    /// only way it sees anything is what hubd pushes — and pushing every
    /// session's whole transcript every cycle would be megabytes for the one
    /// session being read.
    Session,
    /// Close the books on a Claude session and open a new one that continues
    /// its thread. `arg` is the session id, or empty for the focused one.
    Handover,
    /// Start a background session for a project. `arg` is "<project> <việc>".
    New,
    /// Stop the focused background session, keeping its conversation.
    Stop,
    /// Continue the focused background session IN PLACE — a real next turn on
    /// the same thread, unlike `Ask` which forks. `arg` is what to say.
    Tell,
    /// `/type <chữ>` — gõ THẲNG vào cửa sổ terminal của phiên đang theo.
    ///
    /// Khác `Tell` ở chỗ căn bản: `Tell` chạy `claude --resume`, tức mở một
    /// lượt mới trên nhật ký và chỉ dùng được cho phiên nền ĐÃ DỪNG. `Type` gõ
    /// phím vào phiên **đang chạy**, kể cả phiên interactive Hà mở tay — đó là
    /// đường DUY NHẤT trả lời một hộp chọn đang chờ, thứ không nằm trong nhật
    /// ký nên không có API nào chạm tới.
    ///
    /// ⚠ Đường này **bỏ qua `DENIED_TOOLS`**: chữ gõ vào terminal không đi qua
    /// bộ khoá nào. Hà chốt 2026-08-09 sau khi được nêu rõ đánh đổi.
    Type,
    /// `/shot` — chụp cửa sổ terminal của phiên đang theo và đẩy ảnh lên màn.
    ///
    /// Đây là đường DUY NHẤT nhìn thấy thứ đang hiện trên màn mà chưa vào nhật
    /// ký: hộp chọn đang chờ, thanh tiến trình, lỗi vừa in ra. `sessions::
    /// stream` đọc tệp, mà tệp chỉ có sau khi lượt kết thúc.
    Shot,
    /// `/key <tên phím>` — một phím điều khiển: up · down · enter · esc · tab ·
    /// space · 1-9. Hộp chọn của `claude` đi bằng mũi tên, gửi chữ "xuống" vào
    /// đó thì nó gõ ra chữ chứ không di chuyển.
    Key,
    /// Ask the focused session a question WITHOUT interrupting it. `arg` is the
    /// question; the target is whatever `/session` is following.
    ///
    /// The target is implicit on purpose: this is typed on a phone while
    /// looking at one session's stream, and asking a person to retype a uuid
    /// there is asking them not to use the feature.
    Ask,
    /// `/cmd <dòng lệnh>` — chạy ĐÚNG MỘT lệnh trên máy rồi trả kết quả về.
    ///
    /// Hà 2026-08-12: *"thêm một cổng chạy lệnh nữa… `/cmd lệnh`: ví dụ bash …
    /// chạy 1 command xong trả về kết quả rồi nó đóng luôn"*, gõ **từ Telegram**.
    ///
    /// Đây KHÔNG phải một quyền mới: `/type` đã gõ thẳng phím vào cửa sổ terminal
    /// từ 2026-08-09 và **bỏ qua `DENIED_TOOLS`** — nghĩa là từ điện thoại vẫn
    /// chạy được bất cứ thứ gì, chỉ là phải có một phiên đang mở và phải gõ mò.
    /// `/cmd` chỉ làm con đường ấy THẲNG và có kết quả trả về. Cổng gác vẫn là
    /// một: chỉ tid/chat_id của chủ máy mới ra lệnh được.
    ///
    /// Một lệnh, một lượt, không phiên nào ở lại — đúng chữ *"đóng luôn"*.
    Cmd,
    /// `/accounts` — ba tài khoản `claude` trên máy này: phiên nào đang chạy
    /// bằng tài khoản nào, còn bao nhiêu hạn mức, và **`/new` không nói `@acc`
    /// thì rơi vào tài khoản nào**.
    ///
    /// Hà 2026-08-12: *"chưa có lệnh xem danh sách acc"* → *"vậy lệnh new chọn
    /// acc kiểu gì? hay đang để random?"*. Hai câu ấy là một câu: chọn tài
    /// khoản là một quyết định có hậu quả (hạn mức tuần cạn thì phiên mới chết
    /// giữa chừng), mà dữ liệu để quyết định chỉ nằm trên tab Sức khoẻ — thứ
    /// không với tới được khi đang gõ trên Telegram.
    Accounts,
}

#[derive(Debug, Clone)]
pub struct ChannelCommand {
    pub kind: CommandKind,
    /// The id the command acts on: a DECISION id for `Approve`/`Reject`, a
    /// MESSAGE id for `Close`/`Reply` (the two CLI verbs take message ids), and
    /// 0 when the command needs neither (`Help`).
    pub decision_id: i64,
    /// Free text following the id — a reject reason, say.
    pub arg: String,
    /// Where to acknowledge the press (chat id).
    pub chat_id: String,
    /// Telegram requires answering the callback or the button spins forever.
    pub callback_id: String,
    /// The message carrying the buttons, so it can be edited after the action.
    pub message_id: Option<i64>,
}

/// A deliberate skip, not a failure: a credential the operator has not set yet.
/// The pipeline records it on the run row and logs it at warn.
#[derive(Debug)]
pub struct Skip(pub String);

impl std::fmt::Display for Skip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for Skip {}

#[derive(Debug, Clone)]
pub struct Health {
    pub ok: bool,
    pub detail: String,
}
