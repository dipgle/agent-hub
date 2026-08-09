//! Portal snapshot — the console's read-only view, pushed to the tfl5 app so
//! the chat page can show it.
//!
//! **Why a pushed snapshot instead of letting the page call the console.**
//! `web.rs` binds 127.0.0.1 and hands out a per-boot token embedded in its own
//! HTML; that token living only on a loopback page is what keeps CSRF and DNS
//! rebinding away from the approve button (see the `web.rs` module docs). A
//! page served from tfl5 is a different origin, so wiring it to the console
//! would mean opening CORS and moving the token somewhere a script on that
//! origin can read — dismantling exactly that defence, and it would only ever
//! work on the one machine running `hubd` anyway.
//!
//! So the data travels the other way: hub writes a snapshot into the app's own
//! storage on tfl5, and the page reads it same-origin through `/app/file/get`,
//! which gates on the app's ACL (Reader+ plus the per-file row check). Nothing
//! new is exposed: whoever can already read the hub room can read the
//! snapshot, and nobody else.
//!
//! The snapshot is **read-only by construction**. Approving, rejecting and
//! replying stay where they already are — the `/approve` `/reject` commands in
//! the chat room and the console — so there is still exactly one approve path
//! (non-negotiable #7).
//!
//! **Why a doc and not a file.** The first cut wrote `hub-status.json` through
//! `/app/file/save`. Files live under the app's public asset tree, and
//! `public.rs::row_acl_evaluate` treats an EMPTY per-file ACL as "anyone may
//! fetch it" — safe only while a bundle is published (every other path 404s),
//! which is a condition an operator can undo with one click. Filling the ACL
//! in is not a fix either: `file_row_visible` uses the same rosters, so a
//! hard-coded list would also lock out every app member added later. Docs have
//! no static serve path at all: the only way in is the API, gated by the app's
//! Reader check. So the snapshot is one doc in the `hub_status` resource.

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::adapters::{tfl5, Skip};
use crate::config::Config;
use crate::db::Db;
use crate::logging;

/// The resource that holds the snapshot doc, created on first push.
pub const RESOURCE_MA: &str = "hub_status";
/// Identifies the one row inside that resource — every push updates it in
/// place rather than appending, so the doc count stays at 1.
pub const SNAPSHOT_KEY: &str = "snapshot";
/// Leftover from the first design (a public-tree file); removed on push so an
/// installation that ran the earlier build does not keep serving it.
pub const LEGACY_FILE_PATH: &str = "hub-status.json";
/// How stale a channel-health probe may be before it is measured again. The
/// probe logs into tfl5 and shells out to `claude --version`, so running it on
/// every cycle would spend real time to redraw a panel nobody is looking at
/// most minutes.
const HEALTH_TTL_MS: i64 = 10 * 60 * 1000;

/// Cache for the deep health probe: `(measured_at_ms, value)`. Lives for the
/// life of the process, which is what makes it useful in `hubd` and a no-op
/// for a one-shot CLI push.
static HEALTH_CACHE: std::sync::OnceLock<std::sync::Mutex<Option<(i64, Value)>>> =
    std::sync::OnceLock::new();

/// What a push did, so callers can log something truthful.
#[derive(Debug)]
pub struct Pushed {
    pub bytes: usize,
    pub items: usize,
    pub resource_created: bool,
}

/// Build the snapshot from the local store. Pure read — no network.
///
/// `cfg` rides along so the page can show the same Config and channel health
/// the console does; pass `deep_health = false` to reuse a recent probe.
pub fn build(db: &Db, cfg: &Config) -> Result<Value> {
    // No spend-per-day series travels any more: the page that drew it (the
    // "Chi phí" tab) is gone, and a producer whose only consumer was deleted is
    // just weight on every push. The query still exists in the console, which is
    // where a spend question belongs.

    // The Claude CLI sessions running on this machine — the thing the owner
    // actually opens his phone for. Read-only, and already leak-gated at the
    // source (`sessions::snapshot`), so nothing here needs a second gate.
    let live = crate::sessions::snapshot(cfg);

    // Only the session being read carries its full stream. Pushing every
    // transcript every cycle would be megabytes for the one screen anybody is
    // looking at; `/session <id>` says which one that is.
    let focus = match db.get_cursor(crate::pipeline::FOCUS_SESSION_KEY) {
        Ok(Some(id)) if !id.is_empty() => {
            match live.sessions.iter().find(|s| s.session_id == id) {
                Some(s) => Some(crate::sessions::stream(cfg, &s.session_id, &s.cwd, 120)),
                // Followed a session that has since ended: say so rather than
                // leaving the page on a stream that silently stopped growing.
                None => Some(crate::sessions::SessionStream {
                    session_id: id,
                    note: Some("phiên này không còn chạy".into()),
                    ..Default::default()
                }),
            }
        }
        _ => None,
    };

    Ok(json!({
        // Bump when the shape changes so an old page can say "too new to read"
        // instead of rendering half a screen of `undefined`.
        "schema": 4,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "sessions": {
            "list": live.sessions,
            // An account that failed to answer is NOT an account with zero
            // sessions; the page must be able to say which.
            "notes": live.notes,
            // Số phiên bị bỏ khỏi danh sách vì chạy trong editor. Trước đây con
            // số này chỉ đi vào log — nơi Hà không đọc — nên màn hình rút ngắn
            // danh sách mà không nói vì sao.
            "hidden_editor": live.hidden_editor,
            "focus": focus,
            // The last closing note, so the phone can show where the thread
            // went and how to pick it up on the machine.
            "handover": db
                .get_cursor(crate::pipeline::HANDOVER_KEY)
                .ok()
                .flatten()
                .filter(|s| !s.is_empty())
                .and_then(|s| serde_json::from_str::<Value>(&s).ok()),
            // The last side question and its answer. Kept next to `handover`
            // because they are the same shape of thing: something the phone
            // asked for that landed in a fork, not in the session itself.
            "aside": db
                .get_cursor(crate::pipeline::ASIDE_KEY)
                .ok()
                .flatten()
                .filter(|s| !s.is_empty())
                .and_then(|s| serde_json::from_str::<Value>(&s).ok()),
        },
        "health": health(db, cfg)?,
        // No inbox, no counts, no money. Everything this snapshot used to carry
        // about a queue of mail went with the queue (2026-08-08), and the two
        // numbers that outlived it — a ceiling, then a price tag — were thrown
        // out by name: *"sao vẫn nhắc tới tiền vậy, đã bảo xóa hết github rồi
        // mà"*. `spend` in the database keeps recording, silently, so the
        // question can still be ANSWERED if it is ever asked; it just stops
        // being asked on every screen.
        // Config carries env var NAMES only (non-negotiable #3), never values,
        // so showing it to app members leaks no credential. Read-only here:
        // writing it stays in the console, which is the one surface that
        // validates + backs up + temp-renames the file.
        "config": {
            "file": cfg.config_file.display().to_string(),
            "value": serde_json::to_value(cfg)?,
            "projects": crate::pipeline::known_projects(cfg),
        },
        // The page must never offer buttons this snapshot cannot back.
        "read_only": true,
    }))
}

/// Health for the snapshot: cheap parts every time, network probes at most
/// once per [`HEALTH_TTL_MS`]. `checked_at` travels with the probe so the page
/// can say how old the reading is instead of implying it is live.
fn health(db: &Db, cfg: &Config) -> Result<Value> {
    let now = chrono::Utc::now().timestamp_millis();
    let cell = HEALTH_CACHE.get_or_init(|| std::sync::Mutex::new(None));

    let fresh = {
        let guard = cell.lock().map_err(|_| anyhow!("health cache poisoned"))?;
        match &*guard {
            Some((at, v)) if now - at < HEALTH_TTL_MS => Some(v.clone()),
            _ => None,
        }
    };

    let probe = match fresh {
        Some(v) => v,
        None => {
            let v = probe_channels(cfg, now);
            let mut guard = cell.lock().map_err(|_| anyhow!("health cache poisoned"))?;
            *guard = Some((now, v.clone()));
            v
        }
    };

    Ok(json!({
        "probe": probe,
        // Adapter runs come from the local store, so these are always current.
        "runs": db.last_runs(12)?,
    }))
}

/// Run the probes NOW, refresh the cache, and return a one-line summary for
/// the chat room. This is the console's "Kiểm tra" button: the point is a
/// fresh reading on demand, so it deliberately ignores [`HEALTH_TTL_MS`].
pub fn probe_now(cfg: &Config) -> String {
    let now = chrono::Utc::now().timestamp_millis();
    let probe = probe_channels(cfg, now);
    if let Ok(mut guard) = HEALTH_CACHE
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
    {
        *guard = Some((now, probe.clone()));
    }

    let claude = probe["claude"]["ok"].as_bool().unwrap_or(false);
    let mut parts = vec![format!("claude: {}", if claude { "ok" } else { "HỎNG" })];
    if let Some(map) = probe["channels"].as_object() {
        for (name, c) in map {
            if c["enabled"].as_bool() != Some(true) {
                continue;
            }
            let ok = c["ok"].as_bool().unwrap_or(false);
            let detail = c["detail"].as_str().unwrap_or("");
            parts.push(if ok {
                format!("{name}: ok")
            } else {
                format!("{name}: HỎNG ({detail})")
            });
        }
    }
    format!(
        "🩺 {}\n(bảng điều khiển sẽ hiện số liệu này ở ảnh chụp kế tiếp)",
        parts.join(" · ")
    )
}

/// The parts that cost a network round trip or a subprocess.
fn probe_channels(cfg: &Config, now: i64) -> Value {
    let claude = crate::exec::run(
        "claude",
        &["--version"],
        crate::exec::RunOpts {
            timeout: Some(std::time::Duration::from_secs(20)),
            ..Default::default()
        },
    )
    .map(|r| json!({ "ok": r.code == Some(0), "detail": r.stdout.trim() }))
    .unwrap_or_else(|e| json!({ "ok": false, "detail": e.to_string() }));

    // One channel left. The four ingest adapters (github, devlog, email,
    // telegram) went with the inbox product on 2026-08-08.
    let mut channels = serde_json::Map::new();
    channels.insert(
        "tfl5".into(),
        if cfg.adapters.tfl5.enabled {
            let h = crate::adapters::tfl5::health(&cfg.adapters.tfl5);
            json!({ "enabled": true, "ok": h.ok, "detail": h.detail })
        } else {
            json!({ "enabled": false })
        },
    );

    json!({ "checked_at": now, "claude": claude, "channels": channels })
}

/// Build and upload. Returns `Skip` (not an error) when the tfl5 channel is
/// off or its credentials are absent — same contract as every adapter.
pub fn push(cfg: &Config, db: &Db) -> Result<Pushed> {
    let t = &cfg.adapters.tfl5;
    if !t.enabled {
        return Err(Skip("tfl5 channel disabled".into()).into());
    }
    if t.app_tid.trim().is_empty() {
        return Err(anyhow!(
            "adapters.tfl5.app_tid is empty — nothing to push to"
        ));
    }

    let snap = build(db, cfg)?;
    // What "items" means now: live Claude sessions. It used to be inbox rows,
    // and a log line that keeps counting a thing that no longer exists is a log
    // line that quietly reads zero forever.
    let items = snap["sessions"]["list"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    let bytes = serde_json::to_vec(&snap)
        .context("cannot serialise the snapshot")?
        .len();

    let session = tfl5::login(t)?;
    let created = tfl5::resource_ensure(
        t,
        &session,
        &t.app_tid,
        RESOURCE_MA,
        "hub status",
        "Ảnh chụp chỉ-đọc của hộp việc / sức khoẻ / chi phí, do hubd đẩy lên",
    )?;

    tfl5::doc_upsert(
        t,
        &session,
        &t.app_tid,
        RESOURCE_MA,
        json!({ "key": SNAPSHOT_KEY }),
        json!({ "key": SNAPSHOT_KEY, "snapshot": snap }),
    )?;

    // Best-effort cleanup of the earlier file-based design. A failure here is
    // logged, never swallowed, and never fails the push: the file may simply
    // not exist on a fresh install.
    if let Err(e) = tfl5::delete_file(t, &session, &t.app_tid, LEGACY_FILE_PATH, "test") {
        logging::info(
            "portal_legacy_file_cleanup_skipped",
            json!({ "path": LEGACY_FILE_PATH, "detail": e.to_string() }),
        );
    }

    logging::info(
        "portal_snapshot_pushed",
        json!({ "resource": RESOURCE_MA, "bytes": bytes, "items": items, "resource_created": created }),
    );
    Ok(Pushed {
        bytes,
        items,
        resource_created: created,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    /// Every adapter off: `build` must not shell out or hit the network in a
    /// unit test, and the health probe is what would do it.
    fn test_cfg() -> crate::config::Config {
        crate::config::Config::default()
    }

    fn mem_db() -> Db {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hub.sqlite");
        // Keep the tempdir alive for the process — the handle drops here but
        // the file stays until the test binary exits, which is all we need.
        std::mem::forget(dir);
        Db::open(&path).unwrap()
    }

    #[test]
    fn snapshot_carries_the_shape_the_page_expects() {
        let db = mem_db();
        let snap = build(&db, &test_cfg()).unwrap();
        assert_eq!(snap["schema"], 4);
        assert_eq!(snap["read_only"], true);

        // The sessions block must exist even when nothing is running, and it
        // must carry `notes` — an account that failed to answer has to be
        // distinguishable from an account with no sessions.
        assert!(
            snap["sessions"]["list"].is_array(),
            "sessions.list must always be an array, not absent"
        );
        assert!(
            snap["sessions"]["notes"].is_array(),
            "sessions.notes must travel so a failed account is visible"
        );
        assert!(
            snap["generated_at"].as_str().unwrap().len() >= 20,
            "generated_at must be a real timestamp"
        );
        assert!(
            snap["health"]["runs"].is_array(),
            "health needs the poll history"
        );
        assert!(
            snap["health"]["probe"]["channels"].is_object(),
            "health needs the channel probe"
        );
        assert!(
            snap["health"]["probe"]["checked_at"].is_i64(),
            "the probe must say WHEN it was measured — it can be minutes old"
        );
        assert!(snap["config"]["value"].is_object(), "config must travel");
        assert!(snap["config"]["file"].is_string());
    }

    /// The inbox and the money both left this snapshot on 2026-08-08, and both
    /// left by request. Absence is the assert, because each of them grew back
    /// once already: the ceiling came back as a price tag, and a price tag is
    /// one well-meant number away from being a ceiling again.
    #[test]
    fn the_snapshot_carries_no_inbox_and_no_money() {
        let db = mem_db();
        let snap = build(&db, &test_cfg()).unwrap();
        for gone in [
            "items",
            "counts",
            "cost_days",
            "budget",
            "owner_budget",
            "owner_spend",
            "chat",
        ] {
            assert!(
                snap.get(gone).is_none(),
                "hub is a session channel, not an inbox or a meter — `{gone}` must not travel"
            );
        }
        let as_text = snap.to_string();
        assert!(
            !as_text.contains("cost_usd"),
            "no price may reach the page, at any depth"
        );
    }

    #[test]
    fn config_carries_no_secret_values_only_env_var_names() {
        let cfg = test_cfg();
        let db = mem_db();
        let snap = build(&db, &cfg).unwrap();
        let as_text = snap["config"].to_string();
        // Rule #3: the config file holds env var NAMES. If a value ever leaks
        // into Config, this snapshot would publish it to every app member.
        for marker in ["_TOKEN\":\"", "_PASSWORD\":\"", "_KEY\":\"", "sk-"] {
            assert!(
                !as_text.contains(marker),
                "config block looks like it carries a secret value ({marker})"
            );
        }
    }
}
