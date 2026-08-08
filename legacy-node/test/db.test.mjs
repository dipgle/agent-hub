import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  cancelOutboxFor, counts, deadLetter, enqueueOutbox, finishRun, getCursor, insertDecision,
  insertMessage, lastRuns, latestDecisionFor, listMessages, markOutboxFailed, markOutboxSent,
  openDb, queuedOutbox, resetTriaging, setCursor, setMessageStatus, startRun,
} from "../src/db.mjs";

function freshDb() {
  const dir = mkdtempSync(join(tmpdir(), "hub-test-"));
  const db = openDb(join(dir, "hub.sqlite"));
  return { db, cleanup: () => { db.close(); rmSync(dir, { recursive: true, force: true }); } };
}

const sample = (over = {}) => ({
  source: "github",
  external_id: "notif:1:2026-07-26T00:00:00Z",
  thread_key: "dipgle/tfl5:Issue:x",
  project: "tfl5",
  sender: "dipgle",
  sender_trust: "trusted",
  subject: "[dipgle/tfl5] CI failed",
  body: "log...",
  received_at: "2026-07-26T00:00:00Z",
  raw: { repo: "dipgle/tfl5" },
  ...over,
});

test("insertMessage is idempotent on (source, external_id)", (t) => {
  const { db, cleanup } = freshDb();
  t.after(cleanup);

  const a = insertMessage(db, sample());
  assert.equal(a.inserted, true);
  const b = insertMessage(db, sample());
  assert.equal(b.inserted, false, "replaying the same poll window must not duplicate");
  assert.equal(listMessages(db).length, 1);

  // A new updated_at is a genuinely new item.
  const c = insertMessage(db, sample({ external_id: "notif:1:2026-07-27T00:00:00Z" }));
  assert.equal(c.inserted, true);
  assert.equal(listMessages(db).length, 2);
});

test("status transitions, attempts and error text persist", (t) => {
  const { db, cleanup } = freshDb();
  t.after(cleanup);
  const { id } = insertMessage(db, sample());

  setMessageStatus(db, id, "triaging", { bumpAttempts: true, project: "tfl5" });
  setMessageStatus(db, id, "new", { last_error: "boom", bumpAttempts: true });
  const row = listMessages(db)[0];
  assert.equal(row.status, "new");
  assert.equal(row.attempts, 2);
  assert.equal(row.last_error, "boom");
});

test("resetTriaging rescues rows stranded by a crash", (t) => {
  const { db, cleanup } = freshDb();
  t.after(cleanup);
  const { id } = insertMessage(db, sample());
  setMessageStatus(db, id, "triaging");
  assert.equal(resetTriaging(db), 1);
  assert.equal(listMessages(db)[0].status, "new");
  assert.equal(resetTriaging(db), 0);
});

test("decisions attach to a message and the latest one wins", (t) => {
  const { db, cleanup } = freshDb();
  t.after(cleanup);
  const { id } = insertMessage(db, sample());

  const first = insertDecision(db, { message_id: id, tier: "L0", kind: "bug", severity: "p1", summary: "one", needs_human: true, actions: [{ type: "reply", detail: "x" }], evidence: ["a.ts:1"], confidence: 0.5, cost_usd: 0.01 });
  const second = insertDecision(db, { message_id: id, tier: "L1", kind: "question", severity: "p3", summary: "two", needs_human: false, confidence: 0.9, cost_usd: 0.02 });

  const latest = latestDecisionFor(db, id);
  assert.equal(latest.id, second);
  assert.equal(latest.summary, "two");
  assert.equal(latest.needs_human, 0);

  const firstRow = db.prepare("SELECT actions, evidence FROM decisions WHERE id = ?").get(first);
  assert.deepEqual(JSON.parse(firstRow.actions), [{ type: "reply", detail: "x" }]);
  assert.deepEqual(JSON.parse(firstRow.evidence), ["a.ts:1"]);

  const c = counts(db);
  assert.ok(Math.abs(c.cost_usd_total - 0.03) < 1e-9, `cost total was ${c.cost_usd_total}`);
});

test("outbox retries then dead-letters after max attempts", (t) => {
  const { db, cleanup } = freshDb();
  t.after(cleanup);
  const { id } = insertMessage(db, sample());
  const oid = enqueueOutbox(db, { message_id: id, channel: "github", target: "dipgle/tfl5#1", body: "hi" });

  for (let i = 1; i <= 4; i++) {
    const r = markOutboxFailed(db, oid, `net ${i}`, 5);
    assert.equal(r.attempts, i);
    assert.equal(r.status, "queued", "must stay retryable below the cap");
    assert.equal(queuedOutbox(db).length, 1);
  }
  const last = markOutboxFailed(db, oid, "net 5", 5);
  assert.equal(last.status, "failed");
  assert.equal(queuedOutbox(db).length, 0, "a failed row must stop being picked up");

  deadLetter(db, { source: "github", external_id: String(oid), stage: "outbound", err: "gave up" });
  assert.equal(counts(db).dead_letter, 1);
});

test("markOutboxSent clears the error and stamps sent_at", (t) => {
  const { db, cleanup } = freshDb();
  t.after(cleanup);
  const { id } = insertMessage(db, sample());
  const oid = enqueueOutbox(db, { message_id: id, channel: "notify", target: "local", body: "hi" });
  markOutboxFailed(db, oid, "temporary", 5);
  markOutboxSent(db, oid);
  const row = db.prepare("SELECT * FROM outbox WHERE id = ?").get(oid);
  assert.equal(row.status, "sent");
  assert.equal(row.last_error, null);
  assert.ok(row.sent_at);
  assert.equal(row.attempts, 2);
});

test("rejecting a decision cancels only its queued rows", (t) => {
  const { db, cleanup } = freshDb();
  t.after(cleanup);
  const { id } = insertMessage(db, sample());
  const did = insertDecision(db, { message_id: id, tier: "L1", kind: "question", severity: "p3", summary: "s", needs_human: false, confidence: 0.9 });
  const mine = enqueueOutbox(db, { decision_id: did, message_id: id, channel: "github", target: "dipgle/tfl5#1", body: "reply" });
  const other = enqueueOutbox(db, { message_id: id, channel: "notify", target: "local", body: "brief" });

  cancelOutboxFor(db, did);
  assert.equal(db.prepare("SELECT status FROM outbox WHERE id = ?").get(mine).status, "cancelled");
  assert.equal(db.prepare("SELECT status FROM outbox WHERE id = ?").get(other).status, "queued");
});

test("cursors round-trip and runs record health", (t) => {
  const { db, cleanup } = freshDb();
  t.after(cleanup);

  assert.equal(getCursor(db, "github:since"), null);
  setCursor(db, "github:since", "2026-07-26T00:00:00Z");
  setCursor(db, "github:since", "2026-07-27T00:00:00Z");
  assert.equal(getCursor(db, "github:since"), "2026-07-27T00:00:00Z");

  const okRun = startRun(db, "github", "poll");
  finishRun(db, okRun, { ok: true, nNew: 3 });
  const badRun = startRun(db, "email", "poll");
  finishRun(db, badRun, { ok: false, err: "HTTP 401" });
  const skipRun = startRun(db, "telegram", "poll");
  finishRun(db, skipRun, { ok: true, skipped: "HUB_TELEGRAM_TOKEN not set" });

  const runs = lastRuns(db, 5);
  assert.equal(runs.length, 3);
  const byAdapter = Object.fromEntries(runs.map((r) => [r.adapter, r]));
  assert.equal(byAdapter.github.n_new, 3);
  assert.equal(byAdapter.email.ok, 0);
  assert.match(byAdapter.email.err, /401/);
  assert.match(byAdapter.telegram.skipped, /not set/, "a credential skip must be recorded, not silent");
});
