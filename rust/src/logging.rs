//! Structured JSONL logging.
//!
//! Charter rule #1 (no silent failure): every error path must produce a line
//! here AND a durable row (runs / dead_letter) in the DB. A swallowed error
//! that only returns a default is a bug.

use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

const DEBUG: u8 = 10;
const INFO: u8 = 20;
const WARN: u8 = 30;
const ERROR: u8 = 40;

static LEVEL: AtomicU8 = AtomicU8::new(INFO);

fn log_file() -> &'static Mutex<Option<PathBuf>> {
    static F: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(None))
}

pub fn set_log_file(path: &Path) {
    if let Some(dir) = path.parent() {
        // If this fails the first append fails too, and that path already
        // reports on stderr ("log_file_open_failed") — no silent sink.
        let _ = create_dir_all(dir);
    }
    if let Ok(mut slot) = log_file().lock() {
        *slot = Some(path.to_path_buf());
    }
}

pub fn set_level_from_name(name: &str) {
    let lvl = match name {
        "debug" => DEBUG,
        "info" => INFO,
        "warn" => WARN,
        "error" => ERROR,
        _ => return,
    };
    LEVEL.store(lvl, Ordering::Relaxed);
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn emit(level: u8, level_name: &str, msg: &str, fields: Value) {
    if level < LEVEL.load(Ordering::Relaxed) {
        return;
    }
    let mut line = json!({ "ts": now_iso(), "level": level_name, "msg": msg });
    if let (Some(obj), Value::Object(extra)) = (line.as_object_mut(), fields) {
        for (k, v) in extra {
            obj.insert(k, v);
        }
    }
    let text = line.to_string();

    // Every level goes to stderr: stdout is the DATA channel. `hub sessions
    // --json` and `portal-push --dry-run` are meant to be piped into a parser,
    // and a stray `hub_env_loaded` line on stdout breaks them (it did, the
    // first time `--json` was piped, 2026-08-08). Nothing reads these lines
    // from stdout — the console reads the log FILE, and hubd redirects both
    // streams into one file.
    eprintln!("{text}");

    let path = log_file().lock().ok().and_then(|g| g.clone());
    if let Some(path) = path {
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(mut f) => {
                if let Err(e) = writeln!(f, "{text}") {
                    eprintln!(
                        "{{\"level\":\"error\",\"msg\":\"log_file_write_failed\",\"err\":\"{e}\"}}"
                    );
                }
            }
            // Losing the file sink must still be visible on stderr.
            Err(e) => eprintln!(
                "{{\"level\":\"error\",\"msg\":\"log_file_open_failed\",\"err\":\"{e}\"}}"
            ),
        }
    }
}

pub fn debug(msg: &str, fields: Value) {
    emit(DEBUG, "debug", msg, fields);
}
pub fn info(msg: &str, fields: Value) {
    emit(INFO, "info", msg, fields);
}
pub fn warn(msg: &str, fields: Value) {
    emit(WARN, "warn", msg, fields);
}
pub fn error(msg: &str, fields: Value) {
    emit(ERROR, "error", msg, fields);
}

/// Flatten an error chain into one loggable string.
pub fn err_chain(e: &anyhow::Error) -> String {
    let mut parts = vec![e.to_string()];
    let mut src = e.source();
    while let Some(s) = src {
        parts.push(s.to_string());
        src = s.source();
    }
    parts.join(" | ")
}
