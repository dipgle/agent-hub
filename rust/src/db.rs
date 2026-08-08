//! hub.sqlite — the little that has to survive a restart.
//!
//!   runs     per-poll health — a failed poll leaves a row
//!   cursors  poll watermarks, the followed session, the last handover/aside
//!   spend    what the owner's own calls cost, recorded and never shown
//!
//! It used to hold four more tables — `messages`, `decisions`, `outbox`,
//! `dead_letter` — the whole inbox. They went on 2026-08-08 with the product
//! that filled them. An existing `hub.sqlite` still HAS those tables and their
//! rows; nothing here drops them, because deleting a person's data to tidy up a
//! schema is not a decision code gets to make. New databases simply never grow
//! them, and no query here can see them.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

pub const SCHEMA_VERSION: i64 = 3;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS schema_meta (
  k TEXT PRIMARY KEY,
  v TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS runs (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  adapter     TEXT NOT NULL,
  phase       TEXT NOT NULL,
  started_at  TEXT NOT NULL,
  finished_at TEXT,
  ok          INTEGER,
  n_new       INTEGER DEFAULT 0,
  skipped     TEXT,
  err         TEXT
);
CREATE INDEX IF NOT EXISTS idx_runs_adapter ON runs(adapter, started_at);

-- Money spent OUTSIDE triage. Every `claude` call costs, and the daily ceiling
-- is only honest if it sees all of them: a handover that quietly spent $0.30
-- would make `daily_budget_usd` a number about one code path rather than about
-- the day (non-negotiable #9).
CREATE TABLE IF NOT EXISTS spend (
  id       INTEGER PRIMARY KEY AUTOINCREMENT,
  ts       TEXT NOT NULL,
  kind     TEXT NOT NULL,
  ref      TEXT,
  cost_usd REAL NOT NULL DEFAULT 0,
  detail   TEXT
);
CREATE INDEX IF NOT EXISTS idx_spend_ts ON spend(ts);

CREATE TABLE IF NOT EXISTS cursors (
  k          TEXT PRIMARY KEY,
  v          TEXT,
  updated_at TEXT NOT NULL
);

"#;

fn now() -> String {
    crate::logging::now_iso()
}

// ─── row types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct RunRow {
    pub id: i64,
    pub adapter: String,
    pub phase: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub ok: Option<i64>,
    pub n_new: Option<i64>,
    pub skipped: Option<String>,
    pub err: Option<String>,
}

#[derive(Debug)]
pub struct RunFinish {
    pub ok: bool,
    pub n_new: i64,
    pub err: Option<String>,
    pub skipped: Option<String>,
}

// ─── connection ──────────────────────────────────────────────────────────

pub struct Db {
    pub conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("cannot create {}", dir.display()))?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("cannot open db {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        conn.execute(
            "INSERT INTO schema_meta (k, v) VALUES ('version', ?1) ON CONFLICT(k) DO UPDATE SET v = excluded.v",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(Db { conn })
    }

    /// Explicit transaction control (rusqlite's `Transaction` needs `&mut`,
    /// and the whole store is shared behind `&self`). Used to make "decision +
    /// outbox rows + message status" ONE commit point: a crash mid-way used to
    /// leave a queued reply behind a message that would be triaged again —
    /// paying twice and sending twice.
    pub fn begin(&self) -> Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        Ok(())
    }

    pub fn commit(&self) -> Result<()> {
        self.conn.execute_batch("COMMIT")?;
        Ok(())
    }

    pub fn rollback(&self) -> Result<()> {
        self.conn.execute_batch("ROLLBACK")?;
        Ok(())
    }

    // ── decisions ──

    // ── cursors / runs / dead-letter ──

    pub fn get_cursor(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT v FROM cursors WHERE k = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()?
            .flatten())
    }

    pub fn all_cursors(&self) -> Result<BTreeMap<String, String>> {
        let mut stmt = self.conn.prepare("SELECT k, v FROM cursors")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (k, v) = row?;
            out.insert(k, v.unwrap_or_default());
        }
        Ok(out)
    }

    pub fn set_cursor(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO cursors (k, v, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(k) DO UPDATE SET v = excluded.v, updated_at = excluded.updated_at",
            params![key, value, now()],
        )?;
        Ok(())
    }

    pub fn start_run(&self, adapter: &str, phase: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO runs (adapter, phase, started_at) VALUES (?1, ?2, ?3)",
            params![adapter, phase, now()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn finish_run(&self, id: i64, f: RunFinish) -> Result<()> {
        self.conn.execute(
            "UPDATE runs SET finished_at = ?1, ok = ?2, n_new = ?3, err = ?4, skipped = ?5 WHERE id = ?6",
            params![now(), if f.ok { 1 } else { 0 }, f.n_new, f.err.map(|e| crate::exec::truncate(&e, 2000)), f.skipped, id],
        )?;
        Ok(())
    }

    pub fn last_runs(&self, limit: i64) -> Result<Vec<RunRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM runs ORDER BY id DESC LIMIT ?1")?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok(RunRow {
                id: r.get("id")?,
                adapter: r.get("adapter")?,
                phase: r.get("phase")?,
                started_at: r.get("started_at")?,
                finished_at: r.get("finished_at")?,
                ok: r.get("ok")?,
                n_new: r.get("n_new")?,
                skipped: r.get("skipped")?,
                err: r.get("err")?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Today's owner-initiated spend — the `spend` table only.
    pub fn owner_cost_on_day(&self, day: &str) -> Result<f64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM spend WHERE substr(ts, 1, 10) = ?1",
            params![day],
            |r| r.get(0),
        )?)
    }

    /// Book a non-triage `claude` call against the day.
    pub fn record_spend(
        &self,
        kind: &str,
        reference: &str,
        cost_usd: f64,
        detail: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO spend (ts, kind, ref, cost_usd, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![now(), kind, reference, cost_usd, detail],
        )?;
        Ok(())
    }
}
