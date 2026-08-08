// Child-process helper. No shell: every command is argv-exact so untrusted
// message text can never become shell syntax.

import { spawn } from "node:child_process";

/**
 * Run a command, capture stdout/stderr, enforce a hard timeout.
 * Never throws on non-zero exit — the caller decides what a failure means.
 *
 * @returns {Promise<{code:number|null, signal:string|null, stdout:string, stderr:string, timedOut:boolean, ms:number}>}
 */
export function run(cmd, args, opts = {}) {
  const { cwd, env, input, timeoutMs = 60_000, maxBytes = 8 * 1024 * 1024 } = opts;
  const started = Date.now();

  return new Promise((resolve) => {
    let child;
    try {
      child = spawn(cmd, args, {
        cwd,
        env: env ? { ...process.env, ...env } : process.env,
        stdio: ["pipe", "pipe", "pipe"],
      });
    } catch (e) {
      resolve({ code: null, signal: null, stdout: "", stderr: `spawn failed: ${e.message}`, timedOut: false, ms: 0 });
      return;
    }

    let stdout = "";
    let stderr = "";
    let timedOut = false;
    let settled = false;

    const timer = setTimeout(() => {
      timedOut = true;
      child.kill("SIGKILL");
    }, timeoutMs);

    const cap = (buf, cur) => (cur.length >= maxBytes ? cur : cur + buf.toString());
    child.stdout.on("data", (b) => { stdout = cap(b, stdout); });
    child.stderr.on("data", (b) => { stderr = cap(b, stderr); });

    const finish = (code, signal) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve({ code, signal, stdout, stderr, timedOut, ms: Date.now() - started });
    };

    child.on("error", (e) => {
      stderr += `\nprocess error: ${e.message}`;
      finish(null, null);
    });
    child.on("close", (code, signal) => finish(code, signal));

    if (input !== undefined) {
      child.stdin.on("error", (e) => { stderr += `\nstdin error: ${e.message}`; });
      child.stdin.end(input);
    } else {
      child.stdin.end();
    }
  });
}

/** Convenience: run and JSON.parse stdout. Returns {ok, value, error}. */
export async function runJson(cmd, args, opts = {}) {
  const r = await run(cmd, args, opts);
  if (r.timedOut) return { ok: false, error: `timeout after ${opts.timeoutMs ?? 60_000}ms`, raw: r };
  if (r.code !== 0) return { ok: false, error: `exit ${r.code}: ${r.stderr.slice(0, 500)}`, raw: r };
  try {
    return { ok: true, value: JSON.parse(r.stdout), raw: r };
  } catch (e) {
    return { ok: false, error: `unparseable JSON: ${e.message}; head=${r.stdout.slice(0, 200)}`, raw: r };
  }
}
