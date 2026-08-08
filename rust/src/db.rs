//! hub.sqlite — the single normalized store for every channel.
//!
//! Schema is byte-compatible with the Node prototype so both binaries can open
//! the same file during the parity window.
//!
//!   messages     inbound items, deduped by (source, external_id)
//!   decisions    what the triage brain concluded for a message
//!   outbox       replies/briefs waiting to go out (retry + dead-letter)
//!   runs         per-adapter poll health — a failed poll leaves a row
//!   cursors      poll watermarks
//!   dead_letter  anything we gave up on, kept for forensics

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};
use serde::Serialize;
use serde_json::Value;

pub const SCHEMA_VERSION: i64 = 3;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS schema_meta (
  k TEXT PRIMARY KEY,
  v TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  source       TEXT NOT NULL,
  external_id  TEXT NOT NULL,
  thread_key   TEXT,
  project      TEXT,
  sender       TEXT,
  sender_trust TEXT NOT NULL DEFAULT 'untrusted',
  subject      TEXT,
  body         TEXT,
  url          TEXT,
  raw          TEXT,
  received_at  TEXT,
  ingested_at  TEXT NOT NULL,
  status       TEXT NOT NULL DEFAULT 'new',
  attempts     INTEGER NOT NULL DEFAULT 0,
  last_error   TEXT,
  claimed_at   TEXT,
  UNIQUE (source, external_id)
);
CREATE INDEX IF NOT EXISTS idx_messages_status ON messages(status);
CREATE INDEX IF NOT EXISTS idx_messages_project ON messages(project);
CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_key);

CREATE TABLE IF NOT EXISTS decisions (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  message_id   INTEGER NOT NULL REFERENCES messages(id),
  ts           TEXT NOT NULL,
  tier         TEXT NOT NULL,
  model        TEXT,
  kind         TEXT,
  severity     TEXT,
  project      TEXT,
  summary      TEXT,
  reply_draft  TEXT,
  actions      TEXT,
  evidence     TEXT,
  confidence   REAL,
  needs_human  INTEGER NOT NULL DEFAULT 1,
  tripwire     TEXT,
  cost_usd     REAL,
  session_id   TEXT,
  raw          TEXT,
  status       TEXT NOT NULL DEFAULT 'pending',
  outcome      TEXT
);
CREATE INDEX IF NOT EXISTS idx_decisions_msg ON decisions(message_id);
CREATE INDEX IF NOT EXISTS idx_decisions_status ON decisions(status);

CREATE TABLE IF NOT EXISTS outbox (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  decision_id INTEGER REFERENCES decisions(id),
  message_id  INTEGER REFERENCES messages(id),
  channel     TEXT NOT NULL,
  target      TEXT NOT NULL,
  subject     TEXT,
  body        TEXT NOT NULL,
  status      TEXT NOT NULL DEFAULT 'queued',
  attempts    INTEGER NOT NULL DEFAULT 0,
  last_error  TEXT,
  created_at  TEXT NOT NULL,
  sent_at     TEXT
);
CREATE INDEX IF NOT EXISTS idx_outbox_status ON outbox(status);

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

CREATE TABLE IF NOT EXISTS dead_letter (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  ts          TEXT NOT NULL,
  source      TEXT,
  external_id TEXT,
  stage       TEXT NOT NULL,
  payload     TEXT,
  err         TEXT NOT NULL
);
"#;

fn now() -> String {
    crate::logging::now_iso()
}

// ─── row types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct NewMessage {
    pub source: String,
    pub external_id: String,
    pub thread_key: Option<String>,
    pub project: Option<String>,
    pub sender: Option<String>,
    pub sender_trust: Option<String>,
    pub subject: Option<String>,
    pub body: Option<String>,
    pub url: Option<String>,
    pub raw: Option<Value>,
    pub received_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: i64,
    pub source: String,
    pub external_id: String,
    pub thread_key: Option<String>,
    pub project: Option<String>,
    pub sender: Option<String>,
    pub sender_trust: String,
    pub subject: Option<String>,
    pub body: Option<String>,
    pub url: Option<String>,
    pub raw: Option<String>,
    pub received_at: Option<String>,
    pub ingested_at: String,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    /// When a cycle took ownership of this row (see `claim_new_messages`).
    pub claimed_at: Option<String>,
    /// Set when this message was folded into another message's decision.
    pub coalesced_into: Option<i64>,
}

impl Message {
    fn from_row(r: &Row) -> rusqlite::Result<Message> {
        Ok(Message {
            id: r.get("id")?,
            source: r.get("source")?,
            external_id: r.get("external_id")?,
            thread_key: r.get("thread_key")?,
            project: r.get("project")?,
            sender: r.get("sender")?,
            sender_trust: r.get("sender_trust")?,
            subject: r.get("subject")?,
            body: r.get("body")?,
            url: r.get("url")?,
            raw: r.get("raw")?,
            received_at: r.get("received_at")?,
            ingested_at: r.get("ingested_at")?,
            status: r.get("status")?,
            attempts: r.get("attempts")?,
            last_error: r.get("last_error")?,
            claimed_at: r.get("claimed_at").unwrap_or(None),
            coalesced_into: r.get("coalesced_into").unwrap_or(None),
        })
    }

    /// Parsed `raw` payload — routing and reply targets live in here.
    pub fn raw_json(&self) -> Value {
        match &self.raw {
            None => Value::Object(Default::default()),
            Some(s) => serde_json::from_str(s).unwrap_or_else(|e| {
                // Losing raw silently degrades routing to "no target".
                crate::logging::warn(
                    "message_raw_unparseable",
                    serde_json::json!({ "message_id": self.id, "source": self.source, "err": e.to_string() }),
                );
                Value::Object(Default::default())
            }),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MessagePatch {
    pub project: Option<String>,
    pub sender_trust: Option<String>,
    /// `Some(None)` clears the column, `Some(Some(x))` sets it, `None` leaves it.
    pub last_error: Option<Option<String>>,
    pub bump_attempts: bool,
    /// Decision this message was folded into when it coalesced.
    pub coalesced_into: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct NewDecision {
    pub message_id: i64,
    pub tier: String,
    pub model: Option<String>,
    pub kind: Option<String>,
    pub severity: Option<String>,
    pub project: Option<String>,
    pub summary: Option<String>,
    pub reply_draft: Option<String>,
    pub actions: Option<Value>,
    pub evidence: Option<Value>,
    pub confidence: Option<f64>,
    pub needs_human: bool,
    pub tripwire: Vec<String>,
    pub cost_usd: Option<f64>,
    pub session_id: Option<String>,
    pub raw: Option<Value>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecisionRow {
    pub id: i64,
    pub message_id: i64,
    pub ts: String,
    pub tier: String,
    pub model: Option<String>,
    pub kind: Option<String>,
    pub severity: Option<String>,
    pub project: Option<String>,
    pub summary: Option<String>,
    pub reply_draft: Option<String>,
    pub actions: Option<String>,
    pub evidence: Option<String>,
    pub confidence: Option<f64>,
    pub needs_human: bool,
    pub tripwire: Option<String>,
    pub cost_usd: Option<f64>,
    pub session_id: Option<String>,
    pub raw: Option<String>,
    pub status: String,
    pub outcome: Option<String>,
}

impl DecisionRow {
    fn from_row(r: &Row) -> rusqlite::Result<DecisionRow> {
        Ok(DecisionRow {
            id: r.get("id")?,
            message_id: r.get("message_id")?,
            ts: r.get("ts")?,
            tier: r.get("tier")?,
            model: r.get("model")?,
            kind: r.get("kind")?,
            severity: r.get("severity")?,
            project: r.get("project")?,
            summary: r.get("summary")?,
            reply_draft: r.get("reply_draft")?,
            actions: r.get("actions")?,
            evidence: r.get("evidence")?,
            confidence: r.get("confidence")?,
            needs_human: r.get::<_, i64>("needs_human")? != 0,
            tripwire: r.get("tripwire")?,
            cost_usd: r.get("cost_usd")?,
            session_id: r.get("session_id")?,
            raw: r.get("raw")?,
            status: r.get("status")?,
            outcome: r.get("outcome")?,
        })
    }

    pub fn actions_json(&self) -> Value {
        self.actions
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(Value::Array(vec![]))
    }

    pub fn raw_json(&self) -> Value {
        self.raw
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(Value::Object(Default::default()))
    }
}

#[derive(Debug, Clone, Default)]
pub struct NewOutbox {
    pub decision_id: Option<i64>,
    pub message_id: Option<i64>,
    pub channel: String,
    pub target: String,
    pub subject: Option<String>,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutboxRow {
    pub id: i64,
    pub decision_id: Option<i64>,
    pub message_id: Option<i64>,
    pub channel: String,
    pub target: String,
    pub subject: Option<String>,
    pub body: String,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
}

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

#[derive(Debug, Clone, Serialize, Default)]
pub struct Counts {
    pub messages: BTreeMap<String, i64>,
    pub decisions: BTreeMap<String, i64>,
    pub outbox: BTreeMap<String, i64>,
    pub dead_letter: i64,
    pub cost_usd_total: f64,
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
        // Additive migration for databases created before claim ownership
        // existed. `CREATE TABLE IF NOT EXISTS` above cannot add a column.
        match conn.execute("ALTER TABLE messages ADD COLUMN claimed_at TEXT", []) {
            Ok(_) => crate::logging::info(
                "db_migrated",
                serde_json::json!({ "added": "messages.claimed_at" }),
            ),
            Err(e) if e.to_string().contains("duplicate column name") => {}
            Err(e) => return Err(e.into()),
        }
        // Which decision a coalesced message was attached to. Without it a
        // coalesced row is a dead end: the owner answers one message with no
        // way to see the three others folded into the same decision, and the
        // link exists only in the log.
        match conn.execute("ALTER TABLE messages ADD COLUMN coalesced_into INTEGER", []) {
            Ok(_) => crate::logging::info(
                "db_migrated",
                serde_json::json!({ "added": "messages.coalesced_into" }),
            ),
            Err(e) if e.to_string().contains("duplicate column name") => {}
            Err(e) => return Err(e.into()),
        }
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

    // ── messages ──

    /// Idempotent on (source, external_id): replaying a poll window is free.
    pub fn insert_message(&self, m: &NewMessage) -> Result<(Option<i64>, bool)> {
        let changed = self.conn.execute(
            "INSERT OR IGNORE INTO messages
               (source, external_id, thread_key, project, sender, sender_trust, subject, body, url, raw, received_at, ingested_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'new')",
            params![
                m.source,
                m.external_id,
                m.thread_key,
                m.project,
                m.sender,
                m.sender_trust.clone().unwrap_or_else(|| "untrusted".into()),
                m.subject,
                m.body,
                m.url,
                m.raw.as_ref().map(|v| v.to_string()),
                m.received_at,
                now(),
            ],
        )?;
        if changed == 0 {
            return Ok((None, false));
        }
        let id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM messages WHERE source = ?1 AND external_id = ?2",
                params![m.source, m.external_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok((id, true))
    }

    pub fn get_message(&self, id: i64) -> Result<Option<Message>> {
        Ok(self
            .conn
            .query_row(
                "SELECT * FROM messages WHERE id = ?1",
                params![id],
                Message::from_row,
            )
            .optional()?)
    }

    pub fn list_messages(
        &self,
        status: Option<&str>,
        project: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Message>> {
        let mut sql = String::from("SELECT * FROM messages");
        let mut wheres: Vec<&str> = vec![];
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        if let Some(s) = status {
            wheres.push("status = ?");
            args.push(Box::new(s.to_string()));
        }
        if let Some(p) = project {
            wheres.push("project = ?");
            args.push(Box::new(p.to_string()));
        }
        if !wheres.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&wheres.join(" AND "));
        }
        sql.push_str(" ORDER BY id DESC LIMIT ?");
        args.push(Box::new(limit));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params_from_iter(args.iter().map(|b| b.as_ref())),
            Message::from_row,
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Atomically take ownership of up to `limit` queued messages.
    ///
    /// This is a single UPDATE…RETURNING on purpose: a plain SELECT let a
    /// daemon cycle and a `hub once` (or the web console's "run a cycle")
    /// pick the same rows, pay for the same triage twice and send the same
    /// reply twice.
    pub fn claim_new_messages(&self, limit: i64) -> Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "UPDATE messages SET status = 'triaging', attempts = attempts + 1, claimed_at = ?2
              WHERE id IN (SELECT id FROM messages WHERE status = 'new' ORDER BY id ASC LIMIT ?1)
              RETURNING *",
        )?;
        let rows = stmt.query_map(params![limit, now()], Message::from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// A crash during triage leaves rows in 'triaging'. Put back only the ones
    /// claimed longer ago than a triage can possibly take — resetting every
    /// 'triaging' row would yank messages out from under a running sibling
    /// (daemon cycle vs. `hub once` vs. the console's "run a cycle").
    pub fn reset_triaging(&self, older_than_secs: i64) -> Result<usize> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::seconds(older_than_secs))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        Ok(self.conn.execute(
            "UPDATE messages SET status = 'new' WHERE status = 'triaging' AND (claimed_at IS NULL OR claimed_at < ?1)",
            params![cutoff],
        )?)
    }

    pub fn set_message_status(&self, id: i64, status: &str, patch: MessagePatch) -> Result<()> {
        let mut sets = vec!["status = ?".to_string()];
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(status.to_string())];
        if let Some(p) = patch.project {
            sets.push("project = ?".into());
            args.push(Box::new(p));
        }
        if let Some(t) = patch.sender_trust {
            sets.push("sender_trust = ?".into());
            args.push(Box::new(t));
        }
        if let Some(e) = patch.last_error {
            sets.push("last_error = ?".into());
            args.push(Box::new(e));
        }
        if patch.bump_attempts {
            sets.push("attempts = attempts + 1".into());
        }
        if let Some(into) = patch.coalesced_into {
            sets.push("coalesced_into = ?".into());
            args.push(Box::new(into));
        }
        let sql = format!("UPDATE messages SET {} WHERE id = ?", sets.join(", "));
        args.push(Box::new(id));
        self.conn
            .execute(&sql, params_from_iter(args.iter().map(|b| b.as_ref())))?;
        Ok(())
    }

    // ── decisions ──

    pub fn insert_decision(&self, d: &NewDecision) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO decisions
              (message_id, ts, tier, model, kind, severity, project, summary, reply_draft, actions,
               evidence, confidence, needs_human, tripwire, cost_usd, session_id, raw, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                d.message_id,
                now(),
                d.tier,
                d.model,
                d.kind,
                d.severity,
                d.project,
                d.summary,
                d.reply_draft,
                d.actions.as_ref().map(|v| v.to_string()),
                d.evidence.as_ref().map(|v| v.to_string()),
                d.confidence,
                if d.needs_human { 1 } else { 0 },
                if d.tripwire.is_empty() { None } else { Some(serde_json::to_string(&d.tripwire)?) },
                d.cost_usd,
                d.session_id,
                d.raw.as_ref().map(|v| v.to_string()),
                d.status,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_decision(&self, id: i64) -> Result<Option<DecisionRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT * FROM decisions WHERE id = ?1",
                params![id],
                DecisionRow::from_row,
            )
            .optional()?)
    }

    pub fn latest_decision_for(&self, message_id: i64) -> Result<Option<DecisionRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT * FROM decisions WHERE message_id = ?1 ORDER BY id DESC LIMIT 1",
                params![message_id],
                DecisionRow::from_row,
            )
            .optional()?)
    }

    /// An open (pending) decision on the same thread, newer than `since_iso` —
    /// used to coalesce repeats instead of paying for another triage call.
    /// An open decision on this thread that the incoming message belongs with.
    ///
    /// `since_iso` is measured against **when the earlier message was written**,
    /// not when hub got round to triaging it. That distinction is the whole
    /// point for chat: two questions typed 20 minutes apart are two questions
    /// even when one cycle picks both up seconds apart, and folding the second
    /// into the first means the second is never answered. (Observed on
    /// 2026-08-06 — the fix that only looked at `d.ts` still merged them.)
    /// `messages.received_at` is set by the adapter from the source's own
    /// timestamp; fall back to ingest time when a source has none.
    pub fn pending_decision_for_thread(
        &self,
        thread_key: Option<&str>,
        since_iso: &str,
    ) -> Result<Option<DecisionRow>> {
        let key = match thread_key {
            Some(k) if !k.is_empty() => k,
            _ => return Ok(None),
        };
        Ok(self
            .conn
            .query_row(
                "SELECT d.* FROM decisions d
                   JOIN messages m ON m.id = d.message_id
                  WHERE m.thread_key = ?1 AND d.status = 'pending'
                    AND COALESCE(m.received_at, m.ingested_at) >= ?2
                  ORDER BY d.id DESC LIMIT 1",
                params![key, since_iso],
                DecisionRow::from_row,
            )
            .optional()?)
    }

    /// The `claude` session to continue for this conversation, if there is a
    /// safe one.
    ///
    /// "Safe" is doing real work here. Resuming carries the previous turns'
    /// context forward — including the previous UNTRUSTED body. So a thread
    /// that ever tripped the injection wire is never resumed: an attacker who
    /// got one poisoned turn into the context must not get to build on it.
    /// Those threads start from a clean session every time.
    pub fn last_session_for_thread(
        &self,
        thread_key: &str,
        since_iso: &str,
    ) -> Result<Option<String>> {
        if thread_key.is_empty() {
            return Ok(None);
        }
        let row: Option<(Option<String>, Option<String>)> = self
            .conn
            .query_row(
                "SELECT d.session_id, d.tripwire FROM decisions d
                   JOIN messages m ON m.id = d.message_id
                  WHERE m.thread_key = ?1
                    AND d.session_id IS NOT NULL
                    AND COALESCE(m.received_at, m.ingested_at) >= ?2
                  ORDER BY d.id DESC LIMIT 1",
                params![thread_key, since_iso],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;

        Ok(match row {
            // A tripwire anywhere in the most recent turn poisons the context.
            Some((_, Some(tw))) if !tw.is_empty() && tw != "[]" => None,
            Some((sid, _)) => sid,
            None => None,
        })
    }

    /// How many other messages were folded into this decision. The owner is
    /// answering all of them at once, so the count belongs on screen — reading
    /// one message and replying while three more hide behind it is exactly the
    /// failure the coalescing window trades cost for.
    pub fn coalesced_count(&self, decision_id: i64) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE coalesced_into = ?1",
            params![decision_id],
            |r| r.get(0),
        )?)
    }

    /// What actually happened to the replies queued for a decision.
    ///
    /// Approving prints "sent=N" once and then the fact disappears. A send that
    /// failed five times and dead-lettered looks, in the inbox, exactly like
    /// one that went out — which is the "no silent failure" rule broken at the
    /// last mile.
    pub fn outbox_state_for(
        &self,
        decision_id: i64,
    ) -> Result<Option<(String, i64, Option<String>)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT status, attempts, last_error FROM outbox
                  WHERE decision_id = ?1
                  ORDER BY CASE status WHEN 'failed' THEN 0 WHEN 'queued' THEN 1 ELSE 2 END, id DESC
                  LIMIT 1",
                params![decision_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
    }

    pub fn set_decision_status(&self, id: i64, status: &str, outcome: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE decisions SET status = ?1, outcome = ?2 WHERE id = ?3",
            params![status, outcome, id],
        )?;
        Ok(())
    }

    // ── outbox ──

    pub fn enqueue_outbox(&self, o: &NewOutbox) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO outbox (decision_id, message_id, channel, target, subject, body, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queued', ?7)",
            params![o.decision_id, o.message_id, o.channel, o.target, o.subject, o.body, now()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn queued_outbox(&self, limit: i64) -> Result<Vec<OutboxRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM outbox WHERE status = 'queued' ORDER BY id ASC LIMIT ?1")?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok(OutboxRow {
                id: r.get("id")?,
                decision_id: r.get("decision_id")?,
                message_id: r.get("message_id")?,
                channel: r.get("channel")?,
                target: r.get("target")?,
                subject: r.get("subject")?,
                body: r.get("body")?,
                status: r.get("status")?,
                attempts: r.get("attempts")?,
                last_error: r.get("last_error")?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn mark_outbox_sent(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE outbox SET status = 'sent', sent_at = ?1, attempts = attempts + 1, last_error = NULL WHERE id = ?2",
            params![now(), id],
        )?;
        Ok(())
    }

    /// @returns (attempts, new status)
    pub fn mark_outbox_failed(
        &self,
        id: i64,
        err: &str,
        max_attempts: i64,
    ) -> Result<(i64, String)> {
        let prev: i64 = self
            .conn
            .query_row(
                "SELECT attempts FROM outbox WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let attempts = prev + 1;
        let status = if attempts >= max_attempts {
            "failed"
        } else {
            "queued"
        };
        self.conn.execute(
            "UPDATE outbox SET status = ?1, attempts = ?2, last_error = ?3 WHERE id = ?4",
            params![status, attempts, crate::exec::truncate(err, 2000), id],
        )?;
        Ok((attempts, status.to_string()))
    }

    pub fn cancel_outbox_for(&self, decision_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE outbox SET status = 'cancelled' WHERE decision_id = ?1 AND status = 'queued'",
            params![decision_id],
        )?;
        Ok(())
    }

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

    /// The project of one specific message, addressed the way the channel
    /// addresses it. Used when someone replies to a line: the answer inherits
    /// the context of what it answers, with no command to remember.
    pub fn project_for_external_id(
        &self,
        source: &str,
        external_id: &str,
    ) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT project FROM messages
                  WHERE source = ?1 AND external_id = ?2 AND project IS NOT NULL AND project <> ''",
                params![source, external_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// The project of the most recent message on this thread that had one.
    ///
    /// Chat carries no repo, so a question typed into a room resolves to no
    /// project at all — and hub then answers without knowing which codebase is
    /// being discussed. Rather than making the person re-state it every line,
    /// the conversation keeps the last project it was told about.
    ///
    /// Bounded by `since` so yesterday's topic does not silently colour today's
    /// question.
    pub fn last_project_for_thread(&self, thread_key: &str, since: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT project FROM messages
                  WHERE thread_key = ?1 AND project IS NOT NULL AND project <> ''
                    AND ingested_at >= ?2
                  ORDER BY id DESC LIMIT 1",
                params![thread_key, since],
                |r| r.get(0),
            )
            .optional()?)
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

    pub fn dead_letter(
        &self,
        source: Option<&str>,
        external_id: Option<&str>,
        stage: &str,
        payload: Option<&Value>,
        err: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO dead_letter (ts, source, external_id, stage, payload, err) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                now(),
                source,
                external_id,
                stage,
                payload.map(|p| crate::exec::truncate(&p.to_string(), 20_000)),
                crate::exec::truncate(err, 4000)
            ],
        )?;
        Ok(())
    }

    /// Triage spend recorded for a given UTC day (`YYYY-MM-DD`). Used by the
    /// daily ceiling so an unattended daemon cannot run up a bill.
    /// Everything spent today: triage decisions PLUS every other `claude` call
    /// hub makes. If this only counted decisions, a new spending path would be
    /// invisible to the very ceiling meant to stop it.
    pub fn cost_on_day(&self, day: &str) -> Result<f64> {
        let decisions: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM decisions WHERE substr(ts, 1, 10) = ?1",
            params![day],
            |r| r.get(0),
        )?;
        let other: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM spend WHERE substr(ts, 1, 10) = ?1",
            params![day],
            |r| r.get(0),
        )?;
        Ok(decisions + other)
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

    /// Today's spend attributable to ONE inbound source.
    ///
    /// A public-ish channel needs its own ceiling: without this, a busy chat
    /// room can eat the whole day's budget before a single GitHub item is
    /// looked at. `decisions` has no source column, so join back through the
    /// message that caused the spend.
    pub fn cost_on_day_for_source(&self, day: &str, source: &str) -> Result<f64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(d.cost_usd), 0)
               FROM decisions d JOIN messages m ON m.id = d.message_id
              WHERE substr(d.ts, 1, 10) = ?1 AND m.source = ?2",
            params![day, source],
            |r| r.get(0),
        )?)
    }

    pub fn counts(&self) -> Result<Counts> {
        let mut c = Counts::default();
        let group = |sql: &str, into: &mut BTreeMap<String, i64>| -> Result<()> {
            let mut stmt = self.conn.prepare(sql)?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            for row in rows {
                let (k, n) = row?;
                into.insert(k, n);
            }
            Ok(())
        };
        group(
            "SELECT status, COUNT(*) FROM messages GROUP BY status",
            &mut c.messages,
        )?;
        group(
            "SELECT status, COUNT(*) FROM decisions GROUP BY status",
            &mut c.decisions,
        )?;
        group(
            "SELECT status, COUNT(*) FROM outbox GROUP BY status",
            &mut c.outbox,
        )?;
        c.dead_letter = self
            .conn
            .query_row("SELECT COUNT(*) FROM dead_letter", [], |r| r.get(0))?;
        c.cost_usd_total = self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM decisions",
            [],
            |r| r.get(0),
        )?;
        Ok(c)
    }
}
