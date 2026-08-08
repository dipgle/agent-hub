//! Live chat listener — the room pushes, hub stops asking.
//!
//! WHY A HELD-OPEN SOCKET IS NOT A NAT WORKAROUND
//! There is nothing to punch here: tfl5 already has a public address, so hub
//! simply dials it. The connection stays open so tfl5 can push back down the
//! same path hub opened, which is exactly what NAT, CGNAT and corporate
//! firewalls all permit without any configuration. Hole punching (STUN/TURN)
//! solves a different problem — two peers BOTH behind NAT — and would add a
//! relay for no gain. hub still opens no port, which is its own rule
//! (`PLAN.md` — "không mở cổng vào máy"), independent of any of this.
//!
//! THE POLLER STAYS. This listener is the FAST path, not the durable one. If
//! the socket drops, the process dies, or a burst is still buffered when hub
//! stops, `adapters::tfl5::poll` picks the messages up on the next cycle from
//! its cursor. `UNIQUE(source, external_id)` makes the overlap free — a message
//! delivered twice inserts once. Never remove the poller to "simplify".

use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::adapters::tfl5;
use crate::config::Config;
use crate::db::{Db, NewMessage};
use crate::logging;

/// Lets the listener tell the daemon loop "something arrived, do not sit out
/// the rest of your sleep".
#[derive(Default)]
pub struct Waker {
    inner: Mutex<bool>,
    cv: Condvar,
}

impl Waker {
    pub fn new() -> Arc<Waker> {
        Arc::new(Waker::default())
    }

    pub fn wake(&self) {
        *self.inner.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.cv.notify_all();
    }

    /// Sleep up to `d`, returning early when woken. Clears the flag so a wake
    /// that arrives mid-cycle is not lost but is also not counted twice.
    /// Sleep up to `d`. Returns **true** when a message woke it early.
    ///
    /// The caller needs to know which it was: the follow loop sleeps in short
    /// slices, and a slice that ended because a chat message arrived must hand
    /// control back for a full cycle instead of continuing to tick.
    pub fn sleep(&self, d: Duration) -> bool {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let (mut flagged, _) = self
            .cv
            .wait_timeout_while(guard, d, |woken| !*woken)
            .unwrap_or_else(|e| e.into_inner());
        let woken = *flagged;
        *flagged = false;
        woken
    }
}

/// Run the listener until the process ends. Reconnects on its own; every
/// failure is logged and backed off, never silently abandoned.
pub fn spawn(cfg: Config, waker: Arc<Waker>) {
    thread::spawn(move || {
        let db = match Db::open(&cfg.db) {
            Ok(d) => d,
            Err(e) => {
                // Without this the room would look permanently quiet, so say it
                // loudly rather than leaving a dead thread behind.
                logging::error(
                    "tfl5_live_db_open_failed",
                    json!({ "err": logging::err_chain(&e) }),
                );
                return;
            }
        };
        let mut backoff = 2u64;
        loop {
            match session_once(&cfg, &db, &waker) {
                // A clean end means the socket closed politely; reconnect promptly.
                Ok(()) => backoff = 2,
                Err(e) => {
                    logging::warn(
                        "tfl5_live_disconnected",
                        json!({ "err": logging::err_chain(&e), "retry_in_sec": backoff }),
                    );
                }
            }
            thread::sleep(Duration::from_secs(backoff));
            backoff = (backoff * 2).min(120);
        }
    });
}

/// One connection's lifetime: log in, hold the room, insert what arrives.
fn session_once(cfg: &Config, db: &Db, waker: &Waker) -> anyhow::Result<()> {
    let c = &cfg.adapters.tfl5;
    let s = tfl5::login(c)?;
    let mut live = tfl5::connect_live(c, &s)?;
    // Short reads so the debounce buffer flushes on time even in a silent room.
    live.set_read_timeout(Duration::from_secs(1));
    logging::info(
        "tfl5_live_connected",
        json!({ "app_tid": c.app_tid, "room": c.room, "as": s.username }),
    );

    // The same reason the poller has a silence window: three lines of one
    // thought must reach triage together, or the model answers a third of a
    // question. Held here in memory; the poller is the backstop if hub stops
    // before the buffer drains.
    let window = Duration::from_secs(c.silence_window_sec.max(1));
    let mut buf: Vec<NewMessage> = vec![];
    let mut last_seen = Instant::now();

    loop {
        match live.next_message() {
            Ok(Some(frame)) => {
                // Our own replies come back on this socket. Ingesting them
                // would triage hub's own words and bill for the privilege.
                if frame.get("from_user_tid").and_then(|v| v.as_str()) == Some(s.user_tid.as_str())
                {
                    continue;
                }
                let text = frame.get("text").and_then(|v| v.as_str()).unwrap_or("");
                if text.chars().count() < c.min_chars {
                    logging::info(
                        "tfl5_message_filtered",
                        json!({ "reason": "too_short", "via": "live", "len": text.chars().count() }),
                    );
                    continue;
                }
                // An owner's slash command is an ORDER, not something to pay a
                // model to classify. The poller already splits them out
                // (`tfl5::poll`); this path did not, so every `/close` typed in
                // the room was executed AND stored as a message AND triaged —
                // $0.18 to classify the word "close" (2026-08-07). Leave it for
                // the poller, which turns it into a command.
                let from_uid = frame
                    .get("from_user_tid")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if tfl5::parse_command(text, from_uid, &cfg.trust.tfl5_user_tids).is_some() {
                    logging::info(
                        "tfl5_live_command_deferred",
                        json!({ "head": crate::exec::truncate(text, 40), "from_user_tid": from_uid }),
                    );
                    waker.wake(); // run the cycle now so the order is not sitting idle
                    continue;
                }
                if let Some(m) = tfl5::message_from_frame(c, &frame) {
                    buf.push(m);
                    last_seen = Instant::now();
                }
            }
            Ok(None) => {}
            Err(e) => {
                // Flush before giving up the socket, so a buffered burst is not
                // stranded waiting on the next poll.
                flush(db, cfg, waker, &mut buf);
                live.close();
                return Err(e);
            }
        }

        if !buf.is_empty() && last_seen.elapsed() >= window {
            flush(db, cfg, waker, &mut buf);
        }
    }
}

/// Commit a settled burst and nudge the daemon so it does not sit out the rest
/// of its poll interval.
fn flush(db: &Db, cfg: &Config, waker: &Waker, buf: &mut Vec<NewMessage>) {
    if buf.is_empty() {
        return;
    }
    let projects = crate::pipeline::known_projects(cfg);
    let mut inserted = 0usize;
    for mut m in buf.drain(..) {
        // Same routing the poller applies. Without this the live path wrote
        // raw rows — no project, no trust rating — and since live usually wins
        // the race, that was EVERY chat message.
        if let Err(e) = crate::pipeline::enrich_message(db, cfg, &mut m, &projects) {
            logging::error(
                "tfl5_live_enrich_failed",
                json!({ "external_id": m.external_id, "err": logging::err_chain(&e) }),
            );
        }
        match db.insert_message(&m) {
            // Already there because the poller beat us to it — that is the
            // dedupe working, not a problem.
            Ok((_, true)) => inserted += 1,
            Ok((_, false)) => {}
            Err(e) => logging::error(
                "tfl5_live_insert_failed",
                json!({ "external_id": m.external_id, "err": logging::err_chain(&e) }),
            ),
        }
    }
    if inserted > 0 {
        logging::info("tfl5_live_ingested", json!({ "new": inserted }));
        waker.wake();
    }
}
