import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { insertDecision, insertMessage, openDb, pendingDecisionForThread, setDecisionStatus } from "../src/db.mjs";

function freshDb() {
  const dir = mkdtempSync(join(tmpdir(), "hub-coalesce-"));
  const db = openDb(join(dir, "hub.sqlite"));
  return { db, cleanup: () => { db.close(); rmSync(dir, { recursive: true, force: true }); } };
}

const THREAD = "dipgle/tfl5:CheckSuite:ci";
const long_ago = "1990-01-01T00:00:00Z";
const soon = "2999-01-01T00:00:00Z";

function seedPending(db, { thread = THREAD, status = "pending" } = {}) {
  const { id } = insertMessage(db, {
    source: "github",
    external_id: `notif:${Math.random()}`,
    thread_key: thread,
    sender: "github:dipgle/tfl5",
    subject: "CI failed",
    body: "CI failed",
  });
  const did = insertDecision(db, { message_id: id, tier: "L0", kind: "ci_failure", severity: "p1", summary: "s", needs_human: true, confidence: 0.8, status });
  return { messageId: id, decisionId: did };
}

test("a repeat on the same thread finds the open decision (so triage can be skipped)", (t) => {
  const { db, cleanup } = freshDb();
  t.after(cleanup);
  const { decisionId } = seedPending(db);
  const found = pendingDecisionForThread(db, THREAD, long_ago);
  assert.equal(found?.id, decisionId);
});

test("a different thread never coalesces", (t) => {
  const { db, cleanup } = freshDb();
  t.after(cleanup);
  seedPending(db);
  assert.equal(pendingDecisionForThread(db, "dipgle/other:Issue:1", long_ago), null);
});

test("an answered (non-pending) decision does not swallow new items", (t) => {
  const { db, cleanup } = freshDb();
  t.after(cleanup);
  const { decisionId } = seedPending(db);
  setDecisionStatus(db, decisionId, "approved", "sent");
  assert.equal(pendingDecisionForThread(db, THREAD, long_ago), null, "resolved threads must triage again");
});

test("the coalescing window is respected — an old decision does not silence a fresh report", (t) => {
  const { db, cleanup } = freshDb();
  t.after(cleanup);
  seedPending(db);
  assert.equal(pendingDecisionForThread(db, THREAD, soon), null, "outside the window means triage again");
});

test("no thread_key means no coalescing", (t) => {
  const { db, cleanup } = freshDb();
  t.after(cleanup);
  seedPending(db);
  assert.equal(pendingDecisionForThread(db, null, long_ago), null);
  assert.equal(pendingDecisionForThread(db, "", long_ago), null);
});
