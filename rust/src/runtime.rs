//! What is running on this machine right now — daemon, accounts, recent errors.
//!
//! Hà asked for it in one line (2026-08-09): *"nên tạo 1 tool chụp được tình
//! trạng đang chạy, đã dừng, lỗi, options… liên tục để phản hồi lên ui"*. Until
//! now the phone could see SESSIONS but nothing about the thing watching them:
//! whether `hubd` was even alive, whether it would come back after a reboot,
//! which of the three accounts answered, what the last error was. Every one of
//! those questions had an answer on the machine and no way to reach a phone.
//!
//! Two rules shaped this module:
//!
//! * **Every cycle, not on a button.** A status page you have to ask for is a
//!   status page nobody reads. This runs inside `portal::push`, so it travels
//!   with the snapshot the page already polls.
//! * **Cheap enough to run every cycle.** Anything that spawns a process is
//!   cached (`SLOW_TTL_MS`); everything else is read from memory or the local
//!   database. A status collector that makes the daemon slower would be a
//!   status collector that changes what it measures.

use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};

use crate::config::Config;
use crate::db::Db;
use crate::exec::{run, RunOpts};
use crate::sessions::SessionsSnapshot;

/// Process spawns are cached this long. Autostart registration and account
/// login state change on the scale of days, not seconds.
const SLOW_TTL_MS: i64 = 10 * 60 * 1000;

static STARTED_AT: OnceLock<i64> = OnceLock::new();
static SLOW_CACHE: OnceLock<Mutex<Option<(i64, Value)>>> = OnceLock::new();

/// Called once by `hubd` at boot so "how long has it been up" is a fact rather
/// than a guess from the first cycle.
pub fn mark_start() {
    let _ = STARTED_AT.set(chrono::Utc::now().timestamp_millis());
}

/// Everything the phone needs to answer "is the thing watching my sessions
/// actually alive, and is anything broken?".
pub fn snapshot(cfg: &Config, db: &Db, live: &SessionsSnapshot) -> Value {
    let now = chrono::Utc::now().timestamp_millis();
    json!({
        "daemon": daemon_block(now),
        "accounts": accounts_block(cfg, live),
        "errors": errors_block(db),
        "slow": slow_block(cfg, now),
    })
}

fn daemon_block(now: i64) -> Value {
    let started = STARTED_AT.get().copied();
    json!({
        "pid": std::process::id(),
        // `None` means this snapshot was built by the CLI (`portal-push
        // --dry-run`), not by the daemon — say so instead of printing an uptime
        // of zero, which would read as "just crashed and restarted".
        "started_at_ms": started,
        "uptime_sec": started.map(|s| (now - s) / 1000),
    })
}

/// Per-account state, joined onto the sessions already listed this cycle.
///
/// The join matters: `sessions.notes` is where a failed `claude agents` lands,
/// and without pairing it with the account name a phone shows "3 accounts, 5
/// sessions" while one account has silently been logged out for a day.
fn accounts_block(cfg: &Config, live: &SessionsSnapshot) -> Value {
    let accounts = cfg.claude_accounts_or_ambient();
    let rows: Vec<Value> = accounts
        .iter()
        .map(|acc| {
            let mine: Vec<_> = live
                .sessions
                .iter()
                .filter(|s| s.account == acc.name)
                .collect();
            let alive = mine.iter().filter(|s| s.host != "dead").count();
            // A note is keyed by account name at the front (`"acc2: …"`), which
            // is how `sessions::snapshot` writes it.
            let note = live
                .notes
                .iter()
                .find(|n| n.starts_with(&format!("{}:", acc.name)))
                .cloned();
            let dir = acc
                .config_dir
                .as_ref()
                .map(|d| crate::config::expand_home(Path::new(d)));
            json!({
                "name": acc.name,
                // The PATH, never the contents: this travels to a server.
                "config_dir": dir.as_ref().map(|d| d.display().to_string()),
                "config_dir_exists": dir.as_ref().map(|d| d.exists()),
                "sessions": mine.len(),
                "alive": alive,
                "ok": note.is_none(),
                "note": note,
            })
        })
        .collect();
    Value::Array(rows)
}

/// The last handful of failed cycles, newest first.
///
/// Read from `runs`, not from the log file: the log is append-only text that
/// grows to megabytes, and tailing it every cycle would make the collector the
/// most expensive thing in the loop.
fn errors_block(db: &Db) -> Value {
    match db.last_runs(40) {
        Ok(rows) => {
            let bad: Vec<Value> = rows
                .into_iter()
                .filter(|r| r.ok == Some(0) || r.err.as_deref().is_some_and(|e| !e.is_empty()))
                .take(5)
                .map(|r| {
                    json!({
                        "at": r.started_at,
                        "adapter": r.adapter,
                        "phase": r.phase,
                        "err": r.err,
                    })
                })
                .collect();
            Value::Array(bad)
        }
        Err(e) => {
            crate::logging::warn("runtime_errors_unreadable", json!({ "err": e.to_string() }));
            Value::Array(vec![])
        }
    }
}

/// The part that costs a process spawn — cached.
fn slow_block(cfg: &Config, now: i64) -> Value {
    let cell = SLOW_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cell.lock() {
        if let Some((at, v)) = &*guard {
            if now - at < SLOW_TTL_MS {
                return v.clone();
            }
        }
    }
    let v = json!({
        "checked_at": now,
        "autostart": autostart_state(),
        "claude_cli": cfg.claude_cli.clone(),
    });
    if let Ok(mut guard) = cell.lock() {
        *guard = Some((now, v.clone()));
    }
    v
}

/// Will hub come back by itself after a reboot?
///
/// Measured 2026-08-09: the answer was NO — `deploy/com.dipgle.hubd.plist` sat
/// in the repo, never installed, and the daemon was alive only because someone
/// had started it by hand. That is exactly the kind of fact that is invisible
/// until the day it matters, so it belongs on the screen.
fn autostart_state() -> Value {
    let plist = crate::config::expand_home(Path::new(
        "~/Library/LaunchAgents/com.dipgle.hubd.plist",
    ));
    let installed = plist.exists();
    let loaded = if installed {
        match run(
            "launchctl",
            &["list"],
            RunOpts {
                timeout: Some(Duration::from_secs(10)),
                ..Default::default()
            },
        ) {
            Ok(r) if r.code == Some(0) => Some(r.stdout.contains("com.dipgle.hubd")),
            // A probe that failed is NOT a "no": say "unknown" rather than
            // telling the owner autostart is off when hub simply could not ask.
            Ok(_) => None,
            Err(e) => {
                crate::logging::warn("launchctl_probe_failed", json!({ "err": e.to_string() }));
                None
            }
        }
    } else {
        Some(false)
    };
    json!({
        "plist_installed": installed,
        "loaded": loaded,
        "plist_path": plist.display().to_string(),
        "how_to_install":
            "cp ~/Documents/projects/AI/hub/deploy/com.dipgle.hubd.plist ~/Library/LaunchAgents/ && \
             launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.dipgle.hubd.plist",
    })
}
