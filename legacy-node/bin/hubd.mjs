#!/usr/bin/env node
// hubd — the always-on loop. One process, one machine: a pid lock keeps two
// daemons from double-replying to the same message.
//
//   node bin/hubd.mjs                 run in the foreground
//   launchctl load deploy/…plist      run under launchd (see README)
//
// Failures never kill the loop silently: each cycle error is logged, counted,
// and backed off exponentially (up to 10 min), and after 5 consecutive
// failures the local notify channel gets a heads-up.

import { existsSync, mkdirSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { errFields, log, quietSqliteWarning, setLogFile } from "../src/log.mjs";

quietSqliteWarning();

const { HUB_DIR, loadConfig } = await import("../src/config.mjs");
const dbm = await import("../src/db.mjs");
const { runOnce } = await import("../src/pipeline.mjs");
const { notify } = await import("../src/outbound.mjs");

const cfg = loadConfig(process.env.HUB_CONFIG);
setLogFile(cfg.log_file);

const LOCK = join(HUB_DIR, "data", "hubd.lock");
mkdirSync(dirname(LOCK), { recursive: true });

/**
 * Signal 0 probes existence without delivering anything. Here the throw IS the
 * answer ("no such process"), so catching it is the intent, not a swallow.
 */
function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

if (existsSync(LOCK)) {
  const pid = Number(readFileSync(LOCK, "utf8").trim());
  if (Number.isFinite(pid) && pid !== process.pid && alive(pid)) {
    log.error("hubd_already_running", { pid, lock: LOCK });
    process.stderr.write(`hubd already running (pid ${pid}); lock: ${LOCK}\n`);
    process.exit(3);
  }
  log.warn("stale_lock_removed", { lock: LOCK, pid });
  unlinkSync(LOCK);
}
writeFileSync(LOCK, String(process.pid));

const db = dbm.openDb(cfg.db);
let stopping = false;
let consecutiveFailures = 0;

function cleanup(reason) {
  if (stopping) return;
  stopping = true;
  log.info("hubd_stopping", { reason });
  try {
    db.close();
  } catch (e) {
    log.warn("db_close_failed", errFields(e));
  }
  try {
    if (existsSync(LOCK) && readFileSync(LOCK, "utf8").trim() === String(process.pid)) unlinkSync(LOCK);
  } catch (e) {
    log.warn("lock_cleanup_failed", errFields(e));
  }
}

for (const sig of ["SIGTERM", "SIGINT", "SIGHUP"]) {
  process.on(sig, () => {
    cleanup(sig);
    process.exit(0);
  });
}
process.on("uncaughtException", (e) => {
  log.error("hubd_uncaught", errFields(e));
  cleanup("uncaughtException");
  process.exit(70);
});
process.on("unhandledRejection", (e) => {
  log.error("hubd_unhandled_rejection", errFields(e));
});

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

log.info("hubd_started", {
  pid: process.pid,
  db: cfg.db,
  interval_sec: cfg.poll_interval_sec,
  adapters: Object.entries(cfg.adapters).filter(([, a]) => a.enabled).map(([n]) => n),
  autonomy_default: cfg.autonomy.default,
});

while (!stopping) {
  let delay = cfg.poll_interval_sec * 1000;
  try {
    await runOnce(db, cfg);
    consecutiveFailures = 0;
  } catch (e) {
    consecutiveFailures += 1;
    log.error("cycle_failed", { consecutive: consecutiveFailures, ...errFields(e) });
    delay = Math.min(cfg.poll_interval_sec * 1000 * 2 ** consecutiveFailures, 600_000);
    if (consecutiveFailures === 5) {
      try {
        await notify(cfg, { subject: "hubd: 5 cycles failed in a row", body: `last error: ${e?.message ?? e}\nbacking off to ${Math.round(delay / 1000)}s` });
      } catch (e2) {
        log.error("failure_notify_failed", errFields(e2));
      }
    }
  }
  if (stopping) break;
  await sleep(delay);
}

cleanup("loop_exit");
