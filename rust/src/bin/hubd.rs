//! hubd — the always-on loop. One process, one machine: a pid lock keeps two
//! daemons from double-replying to the same message.
//!
//!   hubd                                     run in the foreground
//!   launchctl load …com.dipgle.hubd.plist    run supervised (see README)
//!
//! Failures never kill the loop silently: each cycle error is logged, counted,
//! and backed off exponentially (up to 10 min), and after 5 consecutive
//! failures the local notify channel gets a heads-up.
//!
//! On SIGTERM/SIGINT the process dies with the default action and leaves its
//! lock file behind; the next start detects that the recorded pid is gone and
//! reclaims it. That is deliberately simpler than installing signal handlers —
//! no unsafe code anywhere in this crate.

use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::json;

use hub::adapters::Skip;
use hub::config;
use hub::db::Db;
use hub::exec::{run, RunOpts};
use hub::logging;
use hub::pipeline::run_once;
use hub::portal;

/// The one alarm that must reach a person who is not looking at the logs: a
/// file line plus a macOS banner.
///
/// Was `outbound::notify`, the "local channel" of a five-channel send path.
/// That module went with the inbox on 2026-08-08 and this is all that outlived
/// it, because the loop can still fail five times in a row while nobody reads
/// `hubd.out`. Errors here are returned, never swallowed — an alarm that fails
/// quietly is worse than no alarm.
fn notify(cfg: &hub::config::Config, subject: Option<&str>, body: &str) -> Result<()> {
    use std::fs::{create_dir_all, OpenOptions};
    use std::io::Write;

    if let Some(dir) = cfg.notify.file.parent() {
        create_dir_all(dir)?;
    }
    let entry = format!(
        "\n===== {} {} =====\n{}\n",
        logging::now_iso(),
        subject.unwrap_or(""),
        body
    );
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cfg.notify.file)
        .and_then(|mut f| f.write_all(entry.as_bytes()))
        .map_err(|e| anyhow::anyhow!("cannot append to {}: {e}", cfg.notify.file.display()))?;

    if cfg.notify.macos_notification && cfg!(target_os = "macos") {
        let raw = subject.unwrap_or(body);
        let text: String = raw
            .chars()
            .take(200)
            .map(|c| {
                if c == '"' || c == '\\' || c == '\n' {
                    ' '
                } else {
                    c
                }
            })
            .collect();
        let script = format!("display notification \"{text}\" with title \"hub\"");
        match run(
            "osascript",
            &["-e", &script],
            RunOpts {
                timeout: Some(Duration::from_secs(10)),
                ..Default::default()
            },
        ) {
            Ok(r) if r.code != Some(0) => logging::warn(
                "osascript_notify_failed",
                json!({ "err": hub::exec::truncate(&r.stderr, 200) }),
            ),
            Err(e) => logging::warn("osascript_notify_failed", json!({ "err": e.to_string() })),
            _ => {}
        }
    }
    Ok(())
}

/// `kill -0 <pid>` — exit 0 means the process exists.
///
/// Fail CLOSED: if the probe itself fails (spawn error, timeout under load) we
/// report "alive" so a second daemon refuses to start. Guessing "dead" would
/// let two loops run, and two loops means paying twice and replying twice.
fn pid_alive(pid: &str) -> bool {
    match run(
        "kill",
        &["-0", pid],
        RunOpts {
            timeout: Some(Duration::from_secs(5)),
            ..Default::default()
        },
    ) {
        Ok(r) if r.timed_out => {
            logging::warn(
                "pid_check_timed_out",
                json!({ "pid": pid, "assumed": "alive" }),
            );
            true
        }
        Ok(r) => r.code == Some(0),
        Err(e) => {
            logging::warn(
                "pid_check_failed",
                json!({ "pid": pid, "err": e.to_string(), "assumed": "alive" }),
            );
            true
        }
    }
}

enum LockState {
    /// The file holds our pid.
    Ours,
    /// The file holds someone else's pid — a real takeover.
    Taken(String),
    /// The file could not be read at all, after retries.
    Unreadable(String),
}

/// Read the pid out of the lock, retrying a transient failure.
///
/// Three tries over ~1.5s: long enough to ride out a package swap under the
/// process tree, short enough that a real takeover is noticed within a cycle.
fn read_lock_pid(lock: &std::path::Path) -> LockState {
    let mut last = String::from("unknown");
    for attempt in 0..3 {
        match fs::read_to_string(lock) {
            Ok(s) => {
                let holder = s.trim().to_string();
                return if holder == std::process::id().to_string() {
                    LockState::Ours
                } else {
                    LockState::Taken(holder)
                };
            }
            Err(e) => {
                last = e.to_string();
                if attempt < 2 {
                    thread::sleep(Duration::from_millis(500));
                }
            }
        }
    }
    LockState::Unreadable(last)
}

/// mtime as (secs, nanos); `None` when the file is missing or unreadable.
fn config_mtime(path: &std::path::Path) -> Option<(i64, u32)> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let dur = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some((dur.as_secs() as i64, dur.subsec_nanos()))
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("hubd: {}", logging::err_chain(&e));
        std::process::exit(70);
    }
}

fn real_main() -> Result<()> {
    // Mốc khởi động: để màn nói được "chạy liên tục N phút" thay vì đoán từ
    // vòng đầu tiên.
    hub::runtime::mark_start();
    let cfg = config::load(None)?;
    logging::set_log_file(&cfg.log_file);
    if let Ok(level) = std::env::var("HUB_LOG_LEVEL") {
        logging::set_level_from_name(&level);
    }

    // launchd does not read your shell profile, so secrets come from
    // <hub_home>/hub.env (chmod 600). Names only in the log, never values.
    let loaded = config::load_env_file(&cfg.hub_home);
    if !loaded.is_empty() {
        logging::info("hub_env_loaded", json!({ "keys": loaded }));
    }

    let lock: PathBuf = cfg.hub_home.join("data").join("hubd.lock");
    if let Some(dir) = lock.parent() {
        fs::create_dir_all(dir)?;
    }

    let my_pid = std::process::id().to_string();
    if lock.exists() {
        let existing = fs::read_to_string(&lock)
            .unwrap_or_default()
            .trim()
            .to_string();
        if !existing.is_empty() && existing != my_pid && pid_alive(&existing) {
            logging::error(
                "hubd_already_running",
                json!({ "pid": existing, "lock": lock.display().to_string() }),
            );
            eprintln!(
                "hubd already running (pid {existing}); lock: {}",
                lock.display()
            );
            std::process::exit(3);
        }
        logging::warn(
            "stale_lock_removed",
            json!({ "lock": lock.display().to_string(), "pid": existing }),
        );
        // A removal failure surfaces immediately: the create_new below returns
        // the real error and the daemon refuses to start.
        let _ = fs::remove_file(&lock);
    }
    // create_new = O_EXCL: two daemons racing to claim the lock cannot both
    // win. The loser fails here, before it has ingested or spent anything.
    {
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
            .map_err(|e| {
                anyhow::anyhow!(
                    "cannot take the lock at {} ({e}) — another hubd is starting",
                    lock.display()
                )
            })?;
        f.write_all(my_pid.as_bytes())?;
    }

    let db = Db::open(&cfg.db)?;

    // No console thread any more. `web.rs` served the inbox — a list, a detail
    // pane, approve/reject buttons and two spend charts — on 127.0.0.1 behind a
    // per-boot token. The inbox is gone (2026-08-08), and everything that
    // outlived it (sessions, health, config) is on the phone page, which is the
    // surface the owner actually uses. A second UI for the same three things is
    // a second thing to keep true.

    // The room pushes instead of hub asking: one socket held open, and a wake
    // so a message that lands mid-sleep does not wait out the poll interval.
    // The poller stays enabled as the durable backstop — see `live.rs`.
    let waker = hub::live::Waker::new();
    if cfg.adapters.tfl5.enabled && cfg.adapters.tfl5.live {
        hub::live::spawn(cfg.clone(), waker.clone());
    }

    logging::info(
        "hubd_started",
        json!({
            "pid": my_pid,
            "db": cfg.db.display().to_string(),
            "interval_sec": cfg.poll_interval_sec,
            "adapters": {
                "tfl5": cfg.adapters.tfl5.enabled,
            }
        }),
    );

    let mut cfg = cfg;
    let mut config_seen = config_mtime(&cfg.config_file);
    let mut consecutive_failures: u32 = 0;
    loop {
        // Re-read the config when the file changes. Without this, every switch
        // in the console (disable an adapter, lower daily_budget_usd, drop a
        // tier) reported success while the running loop kept its boot-time
        // copy — a kill-switch that does not switch anything off.
        let now_mtime = config_mtime(&cfg.config_file);
        if now_mtime != config_seen {
            match config::load(Some(&cfg.config_file.clone())) {
                Ok(fresh) => {
                    logging::info(
                        "config_reloaded",
                        json!({
                            "file": fresh.config_file.display().to_string(),
                            "poll_interval_sec": fresh.poll_interval_sec,
                        }),
                    );
                    cfg = fresh;
                }
                // Keep running on the last good config rather than dying: a
                // half-written file must not take the daemon down.
                Err(e) => logging::error(
                    "config_reload_failed",
                    json!({ "err": logging::err_chain(&e) }),
                ),
            }
            config_seen = now_mtime;
        }

        let mut delay = Duration::from_secs(cfg.poll_interval_sec);
        match run_once(&db, &cfg) {
            Ok(_) => consecutive_failures = 0,
            Err(e) => {
                consecutive_failures += 1;
                logging::error(
                    "cycle_failed",
                    json!({ "consecutive": consecutive_failures, "err": logging::err_chain(&e) }),
                );
                let backoff = cfg
                    .poll_interval_sec
                    .saturating_mul(1u64 << consecutive_failures.min(6));
                delay = Duration::from_secs(backoff.min(600));
                if consecutive_failures == 5 {
                    if let Err(e2) = notify(
                        &cfg,
                        Some("hubd: 5 cycles failed in a row"),
                        &format!("last error: {e}\nbacking off to {}s", delay.as_secs()),
                    ) {
                        logging::error("failure_notify_failed", json!({ "err": e2.to_string() }));
                    }
                }
            }
        }

        // Publish the read-only snapshot the tfl5 chat page renders. A failure
        // here must not stop the loop — the pipeline already did its work and
        // the console still shows everything — but it is logged, never
        // swallowed, and `Skip` (channel off / no credentials) is not an error.
        match portal::push(&cfg, &db) {
            Ok(p) => logging::info(
                "portal_push_ok",
                json!({ "items": p.items, "bytes": p.bytes }),
            ),
            Err(e) if e.downcast_ref::<Skip>().is_some() => {}
            Err(e) => logging::error(
                "portal_push_failed",
                json!({ "err": logging::err_chain(&e) }),
            ),
        }

        // If someone else took the lock (manual override), step aside — but
        // only when the file actually SAYS so.
        //
        // `unwrap_or(true)` used to treat "cannot read the lock" as "someone
        // took it", which killed the daemon twice on 2026-08-08: the `claude`
        // CLI auto-updated (11:16 and 15:16) and, for the seconds that took,
        // every file read in this process tree returned EPERM. A transient
        // filesystem error is not a takeover. Retry a few times, and only give
        // up when the file is readable and holds a different pid.
        match read_lock_pid(&lock) {
            LockState::Ours => {}
            LockState::Taken(other) => {
                logging::warn(
                    "hubd_lock_lost",
                    json!({ "lock": lock.display().to_string(), "holder": other }),
                );
                return Ok(());
            }
            LockState::Unreadable(err) => {
                // Loud, and keep going: dying here means the daemon is down
                // until somebody notices, for a blip that fixed itself.
                logging::error(
                    "hubd_lock_unreadable",
                    json!({ "lock": lock.display().to_string(), "err": err, "action": "tiếp tục chạy" }),
                );
            }
        }

        // Interruptible: a chat message arriving now starts the next cycle
        // immediately instead of sitting out up to `poll_interval_sec`.
        //
        // While somebody is reading a session on their phone, the sleep is cut
        // into slices so the stream can follow the file instead of waiting out
        // a two-minute cycle (UC-S03: "thấy đủ mà chậm hai phút thì vẫn không
        // giống ngồi máy").
        follow_sleep(&cfg, &db, &waker, delay);
    }
}

/// How often the follow loop looks at the transcript, and the floor between two
/// pushes of the same stream. Short enough to feel live, long enough that a
/// busy session cannot turn into a push flood.
const FOLLOW_TICK: Duration = Duration::from_secs(2);
const FOLLOW_MIN_GAP: Duration = Duration::from_secs(4);

/// Sleep for `delay`, but while a session is focused, wake every couple of
/// seconds and re-publish as soon as its transcript grows.
///
/// Only the transcript's mtime is read per tick — no `claude` calls, no
/// pipeline, no money. A push happens only when the file actually changed, so
/// an idle session costs one stat() every two seconds.
fn follow_sleep(cfg: &hub::config::Config, db: &Db, waker: &hub::live::Waker, delay: Duration) {
    let focus = db
        .get_cursor(hub::pipeline::FOCUS_SESSION_KEY)
        .ok()
        .flatten()
        .filter(|id| !id.is_empty());
    let Some(session_id) = focus else {
        waker.sleep(delay);
        return;
    };
    // A session hub just started has NO transcript for the first few seconds.
    // This used to give up here and sleep the whole cycle, so pressing "Mở
    // phiên" on the phone and then waiting **122 seconds** for it to appear was
    // the normal experience (measured 2026-08-08) — for the one action that
    // should feel immediate. Keep looking instead; the file shows up.
    let root = cfg.claude_transcript_root();
    let mut path = hub::sessions::find_transcript(&root, &session_id);

    let started = Instant::now();
    let mut seen = path.as_deref().and_then(hub::sessions::transcript_mtime);
    // `tty` của phiên đang theo, đọc MỘT LẦN: nó không đổi trong đời phiên, và
    // hỏi lại mỗi 2 giây là một lần `claude agents` vô ích.
    let tty = hub::sessions::snapshot(cfg)
        .sessions
        .iter()
        .find(|s| s.session_id == session_id)
        .map(|s| s.tty.clone())
        .filter(|t| !t.is_empty() && t != "??");
    let mut screen_seen: Option<u64> = None;
    let mut last_push = Instant::now();
    while started.elapsed() < delay {
        let slice = FOLLOW_TICK.min(delay.saturating_sub(started.elapsed()));
        if slice.is_zero() {
            break;
        }
        // A chat message woke us: hand back so the main loop runs a full
        // cycle now instead of ticking through the rest of the slices.
        if waker.sleep(slice) {
            return;
        }
        // Someone closed the session (or opened another one): stop following.
        match db.get_cursor(hub::pipeline::FOCUS_SESSION_KEY) {
            Ok(Some(id)) if id == session_id => {}
            _ => return,
        }

        // Still waiting for the file to exist. `find_transcript` is a shallow
        // readdir, cheap enough to repeat every couple of seconds, and only
        // while there is nothing to watch.
        if path.is_none() {
            path = hub::sessions::find_transcript(&root, &session_id);
            if path.is_none() {
                continue;
            }
            // It just appeared — that IS the news. Push now rather than waiting
            // for a second write.
            seen = None;
        }
        // Hai nguồn tin, không phải một.
        //
        // `mtime` của nhật ký bắt được việc phiên LÀM; nó không bắt được việc
        // phiên DỪNG LẠI HỎI — lúc ấy tệp đứng yên hàng phút trong khi màn hình
        // đang chờ một phím. Nên vòng bám nhìn cả chữ trên màn, và đổi chữ cũng
        // là tin đáng đẩy. Băm cho rẻ: so chuỗi vài KB mỗi 2 giây thì không
        // đáng gì, nhưng giữ nguyên chuỗi trong bộ nhớ thì đáng.
        let now = path.as_deref().and_then(hub::sessions::transcript_mtime);
        let screen_now = tty
            .as_deref()
            .and_then(|t| hub::keys::screen_of(t, 16))
            .map(|(s, _)| {
                let mut h: u64 = 1469598103934665603;
                for b in s.as_bytes() {
                    h ^= *b as u64;
                    h = h.wrapping_mul(1099511628211);
                }
                h
            });
        let changed = now != seen || (screen_now.is_some() && screen_now != screen_seen);
        if !changed || last_push.elapsed() < FOLLOW_MIN_GAP {
            continue;
        }
        seen = now;
        screen_seen = screen_now;
        last_push = Instant::now();
        match portal::push(cfg, db) {
            Ok(p) => logging::info(
                "portal_push_follow",
                json!({ "session": session_id, "bytes": p.bytes }),
            ),
            Err(e) if e.downcast_ref::<Skip>().is_some() => {}
            Err(e) => logging::error(
                "portal_push_failed",
                json!({ "err": logging::err_chain(&e), "follow": true }),
            ),
        }
    }
}
