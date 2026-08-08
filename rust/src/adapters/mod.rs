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

use crate::db::NewMessage;

#[derive(Debug, Default)]
pub struct PollResult {
    pub messages: Vec<NewMessage>,
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
    Approve,
    Reject,
    /// Not an action — a request hub answers with instructions. The act stage
    /// writes code and can run for half an hour; triggering it from a chat
    /// message would block the poll loop and put a code change one typo away
    /// from a phone keyboard. It stays a terminal command on purpose.
    ActRefused,
    Help,
    /// Close a MESSAGE (not a decision) and cancel anything pending on it.
    Close,
    /// Answer a MESSAGE by hand with the text in `arg`.
    Reply,
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
    /// Ask the focused session a question WITHOUT interrupting it. `arg` is the
    /// question; the target is whatever `/session` is following.
    ///
    /// The target is implicit on purpose: this is typed on a phone while
    /// looking at one session's stream, and asking a person to retype a uuid
    /// there is asking them not to use the feature.
    Ask,
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
