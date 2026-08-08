//! Child-process helper. No shell: every command is argv-exact so untrusted
//! message text can never become shell syntax.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub struct RunOut {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub ms: u128,
}

impl RunOut {
    pub fn ok(&self) -> bool {
        self.code == Some(0) && !self.timed_out
    }
}

#[derive(Debug, Default)]
pub struct RunOpts<'a> {
    pub cwd: Option<&'a Path>,
    pub input: Option<String>,
    pub timeout: Option<Duration>,
    /// Extra environment for the child. `None` as the value REMOVES the
    /// variable — which is load-bearing for the `claude` CLI: an account is
    /// selected by `CLAUDE_CONFIG_DIR`, and the default account is selected by
    /// the variable being ABSENT, not by pointing it at the default directory.
    pub env: Vec<(String, Option<String>)>,
}

const POLL: Duration = Duration::from_millis(50);
const MAX_BYTES: usize = 8 * 1024 * 1024;

/// Run a command, capture stdout/stderr, enforce a hard timeout.
/// Never errors on a non-zero exit — the caller decides what failure means.
/// Only a spawn failure is an `Err`.
pub fn run(cmd: &str, args: &[&str], opts: RunOpts) -> Result<RunOut> {
    let started = Instant::now();
    let timeout = opts.timeout.unwrap_or(Duration::from_secs(60));

    let mut command = Command::new(cmd);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = opts.cwd {
        command.current_dir(dir);
    }
    for (key, value) in &opts.env {
        match value {
            Some(v) => command.env(key, v),
            None => command.env_remove(key),
        };
    }

    let mut child = command
        .spawn()
        .map_err(|e| anyhow!("spawn {cmd} failed: {e}"))?;

    // stdin in its own thread: a big prompt must not deadlock against a child
    // that is already writing to stdout.
    if let Some(input) = opts.input {
        if let Some(mut stdin) = child.stdin.take() {
            thread::spawn(move || {
                // A write error here means the child closed stdin early (it
                // exited or rejected the input); that shows up as its exit code
                // and stderr, which the caller already inspects. Nothing is
                // hidden by not propagating it out of this thread.
                let _ = stdin.write_all(input.as_bytes());
                // Dropping closes the pipe — the child sees EOF.
            });
        }
    } else {
        drop(child.stdin.take());
    }

    let (tx_out, rx_out) = mpsc::channel::<String>();
    let (tx_err, rx_err) = mpsc::channel::<String>();

    // Reader threads: a read error (or a send onto a dropped receiver after the
    // grace period) can only mean truncated capture, and the caller sees that
    // as an unparseable/short output plus the process exit code — never as a
    // silent success.
    if let Some(out) = child.stdout.take() {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = out.take(MAX_BYTES as u64).read_to_end(&mut buf);
            let _ = tx_out.send(String::from_utf8_lossy(&buf).to_string());
        });
    }
    if let Some(err) = child.stderr.take() {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = err.take(MAX_BYTES as u64).read_to_end(&mut buf);
            let _ = tx_err.send(String::from_utf8_lossy(&buf).to_string());
        });
    }

    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if started.elapsed() >= timeout {
                    // `timed_out` is what the caller acts on; a kill/wait error
                    // means the child is already gone, which is the same
                    // outcome. The timeout itself is never hidden.
                    timed_out = true;
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                thread::sleep(POLL);
            }
            Err(e) => {
                return Ok(RunOut {
                    code: None,
                    stdout: String::new(),
                    stderr: format!("wait failed: {e}"),
                    timed_out: false,
                    ms: started.elapsed().as_millis(),
                })
            }
        }
    };

    // Readers finish once the pipes close (kill closes them too).
    let grace = Duration::from_secs(5);
    let stdout = rx_out.recv_timeout(grace).unwrap_or_default();
    let stderr = rx_err.recv_timeout(grace).unwrap_or_default();

    Ok(RunOut {
        code: status.and_then(|s| s.code()),
        stdout,
        stderr,
        timed_out,
        ms: started.elapsed().as_millis(),
    })
}

/// Run and parse stdout as JSON.
pub fn run_json(cmd: &str, args: &[&str], opts: RunOpts) -> Result<serde_json::Value> {
    let r = run(cmd, args, opts)?;
    if r.timed_out {
        return Err(anyhow!("{cmd} timed out after {}ms", r.ms));
    }
    if r.code != Some(0) {
        let detail = if r.stderr.trim().is_empty() {
            &r.stdout
        } else {
            &r.stderr
        };
        return Err(anyhow!(
            "{cmd} exit {:?}: {}",
            r.code,
            truncate(detail, 500)
        ));
    }
    serde_json::from_str(&r.stdout).map_err(|e| {
        anyhow!(
            "{cmd} returned unparseable JSON: {e}; head={}",
            truncate(&r.stdout, 200)
        )
    })
}

pub fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    s.chars().take(n).collect::<String>() + "…"
}
