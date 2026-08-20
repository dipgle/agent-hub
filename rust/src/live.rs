//! Live chat listener — the room pushes, huba stops asking.
//!
//! WHY A HELD-OPEN SOCKET IS NOT A NAT WORKAROUND
//! There is nothing to punch here: tfl5 already has a public address, so huba
//! simply dials it. The connection stays open so tfl5 can push back down the
//! same path huba opened, which is exactly what NAT, CGNAT and corporate
//! firewalls all permit without any configuration. Hole punching (STUN/TURN)
//! solves a different problem — two peers BOTH behind NAT — and would add a
//! relay for no gain. huba still opens no port, which is its own rule
//! (`PLAN.md` — "không mở cổng vào máy"), independent of any of this.
//!
//! THE POLLER STAYS. This listener is the FAST path, not the durable one. If
//! the socket drops, the process dies, or a burst is still buffered when huba
//! stops, `adapters::tfl5::poll` picks the messages up on the next cycle from
//! its cursor. `UNIQUE(source, external_id)` makes the overlap free — a message
//! delivered twice inserts once. Never remove the poller to "simplify".

use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::json;

use crate::adapters::tfl5;
use crate::config::Config;
use crate::db::Db;
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

/// One connection's lifetime: log in, hold the room, wake the cycle on orders.
fn session_once(cfg: &Config, db: &Db, waker: &Waker) -> anyhow::Result<()> {
    let _ = db;
    let c = &cfg.adapters.tfl5;
    let s = tfl5::login(c)?;
    let mut live = tfl5::connect_live(c, &s)?;
    live.set_read_timeout(Duration::from_secs(1));
    logging::info(
        "tfl5_live_connected",
        json!({ "app_tid": c.app_tid, "room": c.room, "as": s.username }),
    );

    loop {
        match live.next_message() {
            Ok(Some(frame)) => {
                // Our own replies come back on this socket.
                if frame.get("from_user_tid").and_then(|v| v.as_str()) == Some(s.user_tid.as_str())
                {
                    continue;
                }
                let text = frame.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let from_uid = frame
                    .get("from_user_tid")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                // The ONLY thing this socket is for now: notice that an order
                // arrived and start the cycle immediately, instead of letting it
                // sit for the rest of the poll interval. The poller reads it from
                // its cursor and executes it — this path never executes anything
                // itself, which is what keeps one order from running twice.
                //
                // Ordinary conversation is left alone. It used to be buffered,
                // routed and stored on its way to a paid classifier; a room where
                // typing costs money is a room nobody types in.
                if tfl5::parse_command(text, from_uid, &cfg.trust.tfl5_user_tids).is_some() {
                    logging::info(
                        "tfl5_live_command_deferred",
                        json!({ "head": crate::exec::truncate(text, 40), "from_user_tid": from_uid }),
                    );
                    waker.wake();
                }
            }
            Ok(None) => {}
            Err(e) => {
                live.close();
                return Err(e);
            }
        }
    }
}
