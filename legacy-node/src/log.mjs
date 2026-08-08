// Structured JSONL logging.
//
// Charter rule #1 (no silent failure): every error path in this project must
// produce a log line here AND a durable row (runs / dead_letter) in the DB.
// A caught exception that only logs at debug level is a bug.

import { appendFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";

const LEVELS = { debug: 10, info: 20, warn: 30, error: 40 };

let logFile = null;
let minLevel = LEVELS[process.env.HUB_LOG_LEVEL] ?? LEVELS.info;

export function setLogFile(path) {
  mkdirSync(dirname(path), { recursive: true });
  logFile = path;
}

export function setLogLevel(name) {
  if (LEVELS[name] === undefined) throw new Error(`unknown log level: ${name}`);
  minLevel = LEVELS[name];
}

/** Flatten an Error (or anything thrown) into loggable fields. */
export function errFields(err) {
  if (err instanceof Error) {
    return { err: err.message, err_kind: err.name, stack: err.stack?.split("\n").slice(0, 4).join(" | ") };
  }
  return { err: String(err), err_kind: typeof err };
}

function emit(level, msg, fields = {}) {
  if (LEVELS[level] < minLevel) return;
  const line = JSON.stringify({ ts: new Date().toISOString(), level, msg, ...fields });
  const out = LEVELS[level] >= LEVELS.warn ? process.stderr : process.stdout;
  out.write(line + "\n");
  if (logFile) {
    try {
      appendFileSync(logFile, line + "\n");
    } catch (e) {
      // Losing the file sink must still be visible on stderr — never silent.
      process.stderr.write(`{"ts":"${new Date().toISOString()}","level":"error","msg":"log_file_write_failed","err":${JSON.stringify(String(e))}}\n`);
    }
  }
}

export const log = {
  debug: (msg, f) => emit("debug", msg, f),
  info: (msg, f) => emit("info", msg, f),
  warn: (msg, f) => emit("warn", msg, f),
  error: (msg, f) => emit("error", msg, f),
};

/**
 * Suppress only the node:sqlite ExperimentalWarning, keep every other warning.
 * Call from a bin entrypoint BEFORE importing db.mjs.
 */
export function quietSqliteWarning() {
  const previous = process.listeners("warning");
  process.removeAllListeners("warning");
  process.on("warning", (w) => {
    if (w.name === "ExperimentalWarning" && /SQLite/i.test(w.message)) return;
    for (const l of previous) l(w);
  });
}
