// Outbound dispatcher. Everything the hub says to the outside world goes
// through the outbox table first, so nothing is sent twice, every failure is
// retried with a visible attempt count, and giving up lands in dead_letter.

import { appendFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { deadLetter, markOutboxFailed, markOutboxSent, queuedOutbox } from "./db.mjs";
import { run } from "./exec.mjs";
import { errFields, log } from "./log.mjs";
import * as github from "./adapters/github.mjs";
import * as email from "./adapters/email.mjs";
import * as telegram from "./adapters/telegram.mjs";

const MAX_ATTEMPTS = 5;

/** Local channel that always works: a log file + a macOS banner. */
async function notify(cfg, { subject, body }) {
  mkdirSync(dirname(cfg.notify.file), { recursive: true });
  const entry = `\n===== ${new Date().toISOString()} ${subject ?? ""} =====\n${body}\n`;
  appendFileSync(cfg.notify.file, entry);

  if (cfg.notify.macos_notification && process.platform === "darwin") {
    const title = "hub";
    const text = (subject ?? body).slice(0, 200).replace(/["\\]/g, " ").replace(/\n/g, " ");
    const r = await run("osascript", ["-e", `display notification "${text}" with title "${title}"`], { timeoutMs: 10_000 });
    if (r.code !== 0) log.warn("osascript_notify_failed", { err: r.stderr.slice(0, 200) });
  }
  return { id: null };
}

/**
 * Send one outbox row. Throws on failure so flush() can record the attempt.
 */
export async function sendOne(cfg, row) {
  switch (row.channel) {
    case "github":
      return github.send({ target: row.target, subject: row.subject, body: row.body });
    case "email":
      return email.send({ cfg: cfg.adapters.email, target: row.target, subject: row.subject, body: row.body });
    case "telegram":
      return telegram.send({ cfg: cfg.adapters.telegram, target: row.target, subject: row.subject, body: row.body });
    case "notify":
      return notify(cfg, { subject: row.subject, body: row.body });
    case "devlog":
      // Nothing to reply to (the sender is a log file) — surface it locally.
      return notify(cfg, { subject: row.subject ?? "devlog item", body: row.body });
    default:
      throw new Error(`unknown outbound channel: ${row.channel}`);
  }
}

/** @returns {Promise<{sent:number, failed:number, gaveUp:number}>} */
export async function flush(db, cfg, limit = 20) {
  const rows = queuedOutbox(db, limit);
  let sent = 0;
  let failed = 0;
  let gaveUp = 0;

  for (const row of rows) {
    try {
      const res = await sendOne(cfg, row);
      markOutboxSent(db, row.id);
      sent += 1;
      log.info("outbox_sent", { outbox_id: row.id, channel: row.channel, target: row.target, remote_id: res?.id ?? null });
    } catch (e) {
      const { attempts, status } = markOutboxFailed(db, row.id, e.message ?? e, MAX_ATTEMPTS);
      failed += 1;
      log.error("outbox_send_failed", { outbox_id: row.id, channel: row.channel, target: row.target, attempts, status, ...errFields(e) });
      if (status === "failed") {
        gaveUp += 1;
        deadLetter(db, {
          source: row.channel,
          external_id: String(row.id),
          stage: "outbound",
          payload: { channel: row.channel, target: row.target, subject: row.subject, body: row.body?.slice(0, 2000) },
          err: e.message ?? String(e),
        });
        // Losing an outbound message must be loud, not a row nobody reads.
        try {
          await notify(cfg, {
            subject: `hub: gave up sending ${row.channel} → ${row.target}`,
            body: `outbox #${row.id} failed ${attempts}×: ${e.message}\n\n${row.body?.slice(0, 500)}`,
          });
        } catch (e2) {
          log.error("deadletter_notify_failed", errFields(e2));
        }
      }
    }
  }

  return { sent, failed, gaveUp };
}

export { notify };
