// hub.sqlite — the single normalized store for every channel.
//
// Tables:
//   messages     inbound items from every adapter, deduped by (source, external_id)
//   decisions    what the triage brain concluded for a message
//   outbox       replies/comments waiting to go out (retry + dead-letter)
//   runs         per-adapter poll health — an adapter that fails leaves a row
//   cursors      poll watermarks
//   dead_letter  anything we gave up on, kept for forensics
//
// Zero npm deps: node:sqlite (Node >= 22).

import { DatabaseSync } from "node:sqlite";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";

export const SCHEMA_VERSION = 1;

const SCHEMA = `
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
`;

const now = () => new Date().toISOString();

export function openDb(path) {
  mkdirSync(dirname(path), { recursive: true });
  const db = new DatabaseSync(path);
  db.exec("PRAGMA journal_mode = WAL");
  db.exec("PRAGMA busy_timeout = 5000");
  db.exec("PRAGMA foreign_keys = ON");
  db.exec(SCHEMA);
  db.prepare("INSERT INTO schema_meta (k, v) VALUES ('version', ?) ON CONFLICT(k) DO UPDATE SET v = excluded.v")
    .run(String(SCHEMA_VERSION));
  return db;
}

// ─── messages ────────────────────────────────────────────────────────────

/**
 * Insert an inbound message. Idempotent on (source, external_id) so replaying
 * a poll window never duplicates work.
 * @returns {{id:number|null, inserted:boolean}}
 */
export function insertMessage(db, m) {
  const res = db
    .prepare(
      `INSERT OR IGNORE INTO messages
         (source, external_id, thread_key, project, sender, sender_trust, subject, body, url, raw, received_at, ingested_at, status)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'new')`,
    )
    .run(
      m.source,
      m.external_id,
      m.thread_key ?? null,
      m.project ?? null,
      m.sender ?? null,
      m.sender_trust ?? "untrusted",
      m.subject ?? null,
      m.body ?? null,
      m.url ?? null,
      m.raw ? JSON.stringify(m.raw) : null,
      m.received_at ?? null,
      now(),
    );
  if (res.changes === 0) return { id: null, inserted: false };
  const row = db.prepare("SELECT id FROM messages WHERE source = ? AND external_id = ?").get(m.source, m.external_id);
  return { id: row?.id ?? null, inserted: true };
}

export function getMessage(db, id) {
  return db.prepare("SELECT * FROM messages WHERE id = ?").get(id) ?? null;
}

export function listMessages(db, { status, project, limit = 50 } = {}) {
  const where = [];
  const args = [];
  if (status) { where.push("status = ?"); args.push(status); }
  if (project) { where.push("project = ?"); args.push(project); }
  const sql = `SELECT * FROM messages ${where.length ? "WHERE " + where.join(" AND ") : ""} ORDER BY id DESC LIMIT ?`;
  return db.prepare(sql).all(...args, limit);
}

export function claimNewMessages(db, limit) {
  return db.prepare("SELECT * FROM messages WHERE status = 'new' ORDER BY id ASC LIMIT ?").all(limit);
}

/**
 * A crash during triage leaves rows in 'triaging'. Put them back in the queue
 * so nothing is silently stranded; attempts already guards against loops.
 * @returns {number} rows recovered
 */
export function resetTriaging(db) {
  const res = db.prepare("UPDATE messages SET status = 'new' WHERE status = 'triaging'").run();
  return res.changes;
}

export function setMessageStatus(db, id, status, patch = {}) {
  const sets = ["status = ?"];
  const args = [status];
  for (const k of ["project", "sender_trust", "last_error"]) {
    if (patch[k] !== undefined) { sets.push(`${k} = ?`); args.push(patch[k]); }
  }
  if (patch.bumpAttempts) sets.push("attempts = attempts + 1");
  db.prepare(`UPDATE messages SET ${sets.join(", ")} WHERE id = ?`).run(...args, id);
}

// ─── decisions ───────────────────────────────────────────────────────────

export function insertDecision(db, d) {
  const res = db
    .prepare(
      `INSERT INTO decisions
        (message_id, ts, tier, model, kind, severity, project, summary, reply_draft, actions,
         evidence, confidence, needs_human, tripwire, cost_usd, session_id, raw, status)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    )
    .run(
      d.message_id,
      now(),
      d.tier,
      d.model ?? null,
      d.kind ?? null,
      d.severity ?? null,
      d.project ?? null,
      d.summary ?? null,
      d.reply_draft ?? null,
      d.actions ? JSON.stringify(d.actions) : null,
      d.evidence ? JSON.stringify(d.evidence) : null,
      d.confidence ?? null,
      d.needs_human ? 1 : 0,
      d.tripwire?.length ? JSON.stringify(d.tripwire) : null,
      d.cost_usd ?? null,
      d.session_id ?? null,
      d.raw ? JSON.stringify(d.raw) : null,
      d.status ?? "pending",
    );
  return Number(res.lastInsertRowid);
}

export function getDecision(db, id) {
  return db.prepare("SELECT * FROM decisions WHERE id = ?").get(id) ?? null;
}

export function latestDecisionFor(db, messageId) {
  return db.prepare("SELECT * FROM decisions WHERE message_id = ? ORDER BY id DESC LIMIT 1").get(messageId) ?? null;
}

/**
 * An open (pending) decision on the same conversation/thread, newer than
 * `sinceIso`. Used to coalesce repeat notifications instead of paying for a
 * fresh triage call on every duplicate.
 */
export function pendingDecisionForThread(db, threadKey, sinceIso) {
  if (!threadKey) return null;
  return (
    db
      .prepare(
        `SELECT d.* FROM decisions d
           JOIN messages m ON m.id = d.message_id
          WHERE m.thread_key = ? AND d.status = 'pending' AND d.ts >= ?
          ORDER BY d.id DESC LIMIT 1`,
      )
      .get(threadKey, sinceIso) ?? null
  );
}

export function listDecisions(db, { status, limit = 50 } = {}) {
  const sql = status
    ? "SELECT * FROM decisions WHERE status = ? ORDER BY id DESC LIMIT ?"
    : "SELECT * FROM decisions ORDER BY id DESC LIMIT ?";
  return status ? db.prepare(sql).all(status, limit) : db.prepare(sql).all(limit);
}

export function setDecisionStatus(db, id, status, outcome) {
  db.prepare("UPDATE decisions SET status = ?, outcome = ? WHERE id = ?").run(status, outcome ?? null, id);
}

// ─── outbox ──────────────────────────────────────────────────────────────

export function enqueueOutbox(db, o) {
  const res = db
    .prepare(
      `INSERT INTO outbox (decision_id, message_id, channel, target, subject, body, status, created_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
    )
    .run(o.decision_id ?? null, o.message_id ?? null, o.channel, o.target, o.subject ?? null, o.body, o.status ?? "queued", now());
  return Number(res.lastInsertRowid);
}

export function queuedOutbox(db, limit = 20) {
  return db.prepare("SELECT * FROM outbox WHERE status = 'queued' ORDER BY id ASC LIMIT ?").all(limit);
}

export function markOutboxSent(db, id) {
  db.prepare("UPDATE outbox SET status = 'sent', sent_at = ?, attempts = attempts + 1, last_error = NULL WHERE id = ?").run(now(), id);
}

export function markOutboxFailed(db, id, err, maxAttempts = 5) {
  const row = db.prepare("SELECT attempts FROM outbox WHERE id = ?").get(id);
  const attempts = (row?.attempts ?? 0) + 1;
  const status = attempts >= maxAttempts ? "failed" : "queued";
  db.prepare("UPDATE outbox SET status = ?, attempts = ?, last_error = ? WHERE id = ?").run(status, attempts, String(err).slice(0, 2000), id);
  return { attempts, status };
}

export function cancelOutboxFor(db, decisionId) {
  db.prepare("UPDATE outbox SET status = 'cancelled' WHERE decision_id = ? AND status = 'queued'").run(decisionId);
}

// ─── cursors / runs / dead-letter ────────────────────────────────────────

export function getCursor(db, key) {
  return db.prepare("SELECT v FROM cursors WHERE k = ?").get(key)?.v ?? null;
}

/** Every cursor as one object — adapters receive the whole map. */
export function allCursors(db) {
  const out = {};
  for (const r of db.prepare("SELECT k, v FROM cursors").all()) out[r.k] = r.v;
  return out;
}

export function setCursor(db, key, value) {
  db.prepare("INSERT INTO cursors (k, v, updated_at) VALUES (?, ?, ?) ON CONFLICT(k) DO UPDATE SET v = excluded.v, updated_at = excluded.updated_at")
    .run(key, value === null || value === undefined ? null : String(value), now());
}

export function startRun(db, adapter, phase = "poll") {
  const res = db.prepare("INSERT INTO runs (adapter, phase, started_at) VALUES (?, ?, ?)").run(adapter, phase, now());
  return Number(res.lastInsertRowid);
}

export function finishRun(db, id, { ok, nNew = 0, err = null, skipped = null }) {
  db.prepare("UPDATE runs SET finished_at = ?, ok = ?, n_new = ?, err = ?, skipped = ? WHERE id = ?")
    .run(now(), ok ? 1 : 0, nNew, err ? String(err).slice(0, 2000) : null, skipped, id);
}

export function lastRuns(db, limit = 10) {
  return db.prepare("SELECT * FROM runs ORDER BY id DESC LIMIT ?").all(limit);
}

export function deadLetter(db, { source, external_id, stage, payload, err }) {
  db.prepare("INSERT INTO dead_letter (ts, source, external_id, stage, payload, err) VALUES (?, ?, ?, ?, ?, ?)")
    .run(now(), source ?? null, external_id ?? null, stage, payload ? JSON.stringify(payload).slice(0, 20000) : null, String(err).slice(0, 4000));
}

export function counts(db) {
  const byStatus = db.prepare("SELECT status, COUNT(*) n FROM messages GROUP BY status").all();
  const out = { messages: {}, outbox: {}, decisions: {} };
  for (const r of byStatus) out.messages[r.status] = r.n;
  for (const r of db.prepare("SELECT status, COUNT(*) n FROM outbox GROUP BY status").all()) out.outbox[r.status] = r.n;
  for (const r of db.prepare("SELECT status, COUNT(*) n FROM decisions GROUP BY status").all()) out.decisions[r.status] = r.n;
  out.dead_letter = db.prepare("SELECT COUNT(*) n FROM dead_letter").get().n;
  out.cost_usd_total = db.prepare("SELECT COALESCE(SUM(cost_usd), 0) c FROM decisions").get().c;
  return out;
}
