#!/usr/bin/env node
// hub — CLI for the comms hub.
//
//   hub doctor                 check every channel + credential, honestly
//   hub once                   one full cycle (ingest → triage → send)
//   hub ingest|triage|flush    a single phase
//   hub inbox [--status s]     what came in and what was decided
//   hub show <id>              one message + its decision in full
//   hub say "text" [-p proj]   put your own message into the hub
//   hub approve <decision-id>  send the drafted reply / green-light the action
//   hub reject <decision-id>   drop it (cancels anything queued)
//   hub reply <msg-id> "text"  reply in your own words through that channel
//   hub act <decision-id>      implement an approved change on a branch
//   hub status                 counts, adapter health, spend

import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { quietSqliteWarning, setLogFile, setLogLevel, log } from "../src/log.mjs";

quietSqliteWarning();

const { HUB_DIR, loadConfig, projectDir, secretFromEnv, CONFIG_DEFAULTS } = await import("../src/config.mjs");
const dbm = await import("../src/db.mjs");
const { runOnce, ingest, triageNew, triageMessageById, ADAPTERS, knownProjects } = await import("../src/pipeline.mjs");
const { flush, notify } = await import("../src/outbound.mjs");
const { act } = await import("../src/act.mjs");
const { run } = await import("../src/exec.mjs");
const policy = await import("../src/policy.mjs");

// ─── args ────────────────────────────────────────────────────────────────

const argv = process.argv.slice(2);
const cmd = argv[0] ?? "help";
const positional = [];
const flags = {};
for (let i = 1; i < argv.length; i++) {
  const a = argv[i];
  if (a.startsWith("--")) {
    const [k, v] = a.slice(2).split("=");
    flags[k] = v ?? (argv[i + 1] && !argv[i + 1].startsWith("-") ? argv[++i] : true);
  } else if (a === "-p") {
    flags.project = argv[++i];
  } else {
    positional.push(a);
  }
}

if (flags.debug) setLogLevel("debug");

function out(s = "") {
  process.stdout.write(s + "\n");
}

function die(msg, code = 1) {
  process.stderr.write(`hub: ${msg}\n`);
  process.exit(code);
}

// ─── boot ────────────────────────────────────────────────────────────────

let cfg;
try {
  cfg = loadConfig(flags.config);
} catch (e) {
  die(e.message, 78);
}
setLogFile(cfg.log_file);
if (!flags.debug && cmd !== "once" && cmd !== "ingest" && cmd !== "triage" && cmd !== "flush") setLogLevel("warn");

const db = dbm.openDb(cfg.db);

// ─── helpers ─────────────────────────────────────────────────────────────

const short = (s, n = 60) => (s ? (String(s).length > n ? String(s).slice(0, n - 1) + "…" : String(s)) : "");
const jparse = (s, fallback) => {
  try {
    return s ? JSON.parse(s) : fallback;
  } catch (e) {
    // Display-only fallback, but a corrupt column still gets said out loud.
    process.stderr.write(`hub: warning — corrupt JSON column, showing fallback (${e.message})\n`);
    return fallback;
  }
};

function decisionOrDie(id) {
  const d = dbm.getDecision(db, Number(id));
  if (!d) die(`no decision #${id}`);
  const m = dbm.getMessage(db, d.message_id);
  if (!m) die(`decision #${id} has no message (db inconsistent)`);
  return { d, m };
}

// ─── commands ────────────────────────────────────────────────────────────

async function cmdDoctor() {
  out(`config      ${cfg.config_file}${existsSync(cfg.config_file) ? "" : "  (defaults — file not created yet)"}`);
  out(`db          ${cfg.db}`);
  out(`workspace   ${cfg.workspace_root}`);
  out(`autonomy    default=${cfg.autonomy.default}  per-project=${JSON.stringify(cfg.autonomy.projects)}`);
  out(`act stage   ${cfg.act.enabled ? `enabled (model=${cfg.act.model}, budget $${cfg.act.max_budget_usd})` : "disabled"}`);
  out("");

  const claude = await run("claude", ["--version"], { timeoutMs: 20_000 });
  out(`claude      ${claude.code === 0 ? "OK  " + claude.stdout.trim() : "FAIL " + (claude.stderr || claude.stdout).trim().slice(0, 120)}`);
  out(`triage      model=${cfg.triage.model} budget=$${cfg.triage.max_budget_usd} timeout=${cfg.triage.timeout_sec}s min_conf_auto=${cfg.triage.min_confidence_auto}`);
  out("");

  out("channels:");
  for (const [name, mod] of Object.entries(ADAPTERS)) {
    const acfg = cfg.adapters[name];
    if (!acfg?.enabled) {
      out(`  ${name.padEnd(9)} off        (adapters.${name}.enabled = false)`);
      continue;
    }
    if (typeof mod.health !== "function") {
      out(`  ${name.padEnd(9)} on         (no health probe)`);
      continue;
    }
    try {
      const h = await mod.health(acfg);
      out(`  ${name.padEnd(9)} ${h.ok ? "OK  " : "FAIL"}       ${short(h.detail, 90)}`);
      if (name === "telegram" && h.ok && !(acfg.allowed_chat_ids ?? []).length) {
        const ids = await mod.observedChatIds(acfg);
        out(`             ↳ allowed_chat_ids is empty. Chat ids seen recently: ${ids.length ? ids.join(", ") : "(none — send your bot a message first)"}`);
      }
    } catch (e) {
      out(`  ${name.padEnd(9)} ERROR      ${short(e.message, 90)}`);
    }
  }

  out("");
  out(`projects with a devlog: ${knownProjects(cfg).join(", ") || "(none found)"}`);
  const c = dbm.counts(db);
  out(`db state: messages=${JSON.stringify(c.messages)} outbox=${JSON.stringify(c.outbox)} dead_letter=${c.dead_letter} spend=$${c.cost_usd_total.toFixed(4)}`);
}

function cmdInit() {
  if (existsSync(cfg.config_file) && !flags.force) {
    out(`config already exists: ${cfg.config_file} (use --force to overwrite)`);
  } else {
    const seed = {
      poll_interval_sec: CONFIG_DEFAULTS.poll_interval_sec,
      autonomy: { default: "L0", projects: {} },
      adapters: {
        github: { enabled: true, repos: [] },
        devlog: { enabled: true, projects: [], kinds: CONFIG_DEFAULTS.adapters.devlog.kinds },
        email: { enabled: false, base_url: CONFIG_DEFAULTS.adapters.email.base_url, api_key_env: "HUB_MAILLER_API_KEY" },
        telegram: { enabled: false, token_env: "HUB_TELEGRAM_TOKEN", allowed_chat_ids: [] },
      },
      trust: { github_logins: [], emails: [], telegram_chat_ids: [], trusted_sources: ["devlog", "cli"] },
      routing: [],
    };
    writeFileSync(cfg.config_file, JSON.stringify(seed, null, 2) + "\n");
    out(`wrote ${cfg.config_file}`);
  }
  out(`db ready at ${cfg.db} (schema v${dbm.SCHEMA_VERSION})`);
}

function cmdInbox() {
  const rows = dbm.listMessages(db, { status: flags.status, project: flags.project, limit: Number(flags.limit ?? 20) });
  if (!rows.length) return out("(empty)");
  out("msg  status          source    project     kind/sev      conf  subject");
  for (const m of rows) {
    const d = dbm.latestDecisionFor(db, m.id);
    const kind = d ? `${d.kind}/${d.severity}` : "-";
    out(
      `${String(m.id).padEnd(4)} ${String(m.status).padEnd(15)} ${String(m.source).padEnd(9)} ${String(m.project ?? "-").padEnd(11)} ` +
        `${kind.padEnd(13)} ${d?.confidence != null ? String(d.confidence).padEnd(5) : "-    "} ${short(m.subject, 50)}` +
        (d ? `   [d#${d.id}${d.status === "pending" ? " pending" : ""}]` : ""),
    );
  }
  const c = dbm.counts(db);
  out("");
  out(`totals: ${JSON.stringify(c.messages)}  queued_out=${c.outbox.queued ?? 0}  spend=$${c.cost_usd_total.toFixed(4)}`);
}

function cmdShow() {
  const id = positional[0];
  if (!id) die("usage: hub show <message-id>  (or hub show d<decision-id>)");
  let m;
  let d;
  if (/^d\d+$/i.test(id)) {
    ({ d, m } = decisionOrDie(id.slice(1)));
  } else {
    m = dbm.getMessage(db, Number(id));
    if (!m) die(`no message #${id}`);
    d = dbm.latestDecisionFor(db, m.id);
  }

  out(`message #${m.id}  [${m.status}]`);
  out(`  source ${m.source} · sender ${m.sender} (${m.sender_trust}) · project ${m.project ?? "-"}`);
  out(`  external_id ${m.external_id}`);
  out(`  received ${m.received_at ?? "?"} · ingested ${m.ingested_at}`);
  if (m.url) out(`  url ${m.url}`);
  if (m.last_error) out(`  last_error ${m.last_error}`);
  out(`  subject: ${m.subject ?? "(none)"}`);
  out("");
  out("--- body ---");
  out((m.body ?? "").slice(0, 4000));
  out("--- end body ---");

  if (!d) return out("\n(no decision yet)");
  out("");
  out(`decision #${d.id}  [${d.status}] tier=${d.tier} model=${d.model} cost=$${(d.cost_usd ?? 0).toFixed(4)}`);
  out(`  ${d.kind}/${d.severity} · project ${d.project ?? "-"} · confidence ${d.confidence} · needs_human=${!!d.needs_human}`);
  if (d.tripwire) out(`  ⚠ tripwire: ${d.tripwire}`);
  out(`  summary: ${d.summary}`);
  const actions = jparse(d.actions, []);
  if (actions.length) {
    out("  proposed actions:");
    for (const a of actions) out(`    - ${a.type}: ${a.detail}`);
  }
  const ev = jparse(d.evidence, []);
  if (ev.length) {
    out("  evidence:");
    for (const e of ev) out(`    · ${e}`);
  }
  const outcome = jparse(d.raw, {})?.outcome;
  if (outcome) out(`  policy: ${outcome.action} — ${outcome.reason}${outcome.target ? ` → ${outcome.channel}:${outcome.target}` : ""}`);
  if (d.reply_draft?.trim()) {
    out("");
    out("--- reply draft ---");
    out(d.reply_draft.trim());
    out("--- end draft ---");
  }
  if (d.outcome) out(`\n  outcome: ${d.outcome}`);
  if (d.status === "pending") out(`\n  → hub approve ${d.id}   |   hub reject ${d.id}   |   hub act ${d.id}`);
}

async function cmdSay() {
  const text = positional.join(" ");
  if (!text) die('usage: hub say "your message" [-p project]');
  const stamp = Date.now();
  const { id, inserted } = dbm.insertMessage(db, {
    source: "cli",
    external_id: `cli:${stamp}`,
    // Each question is its own thread — otherwise every CLI question would
    // coalesce into the previous unanswered one.
    thread_key: `cli:${stamp}`,
    project: flags.project ?? null,
    sender: "cli:owner",
    sender_trust: "trusted",
    subject: short(text, 100),
    body: text,
    received_at: new Date().toISOString(),
    raw: { stream: "cli" },
  });
  if (!inserted) return out("duplicate — nothing inserted");
  out(`queued message #${id}`);
  if (!flags["no-triage"]) {
    const r = await triageMessageById(db, cfg, id);
    await flush(db, cfg);
    out(JSON.stringify(r));
    const d = dbm.latestDecisionFor(db, id);
    if (d) {
      out("");
      positional.length = 0;
      positional.push(String(id));
      cmdShow();
    } else {
      out("(no decision produced — see logs/hub.log)");
    }
  }
}

async function cmdApprove() {
  const { d, m } = decisionOrDie(positional[0]);
  if (d.status !== "pending") out(`note: decision #${d.id} is already '${d.status}' — re-approving`);

  const outcome = jparse(d.raw, {})?.outcome ?? {};
  const body = flags.body ?? d.reply_draft;
  let queued = 0;

  if (body?.trim() && outcome.target) {
    dbm.enqueueOutbox(db, {
      decision_id: d.id,
      message_id: m.id,
      channel: outcome.channel ?? m.source,
      target: outcome.target,
      subject: m.source === "email" ? `Re: ${m.subject ?? ""}`.slice(0, 200) : null,
      body: body.trim(),
    });
    queued += 1;
  } else if (body?.trim()) {
    out(`no reply target for source=${m.source} — nothing to send, marking approved only`);
  }

  dbm.setDecisionStatus(db, d.id, "approved", `approved via CLI${queued ? ", reply queued" : ""}`);
  dbm.setMessageStatus(db, m.id, queued ? "answered" : "closed");
  const res = await flush(db, cfg);
  out(`approved #${d.id}; outbox sent=${res.sent} failed=${res.failed}`);

  const actions = jparse(d.actions, []).map((a) => a.type);
  if (actions.includes("code_change")) {
    out(cfg.act.enabled ? `this decision proposes a code change → run: hub act ${d.id}` : `this decision proposes a code change, but act.enabled=false in config`);
  }
}

async function cmdReject() {
  const { d, m } = decisionOrDie(positional[0]);
  const reason = positional.slice(1).join(" ") || "rejected by owner";
  dbm.cancelOutboxFor(db, d.id);
  dbm.setDecisionStatus(db, d.id, "rejected", reason);
  dbm.setMessageStatus(db, m.id, "closed");
  out(`rejected #${d.id} (${reason}); queued replies cancelled`);
}

function cmdClose() {
  const id = Number(positional[0]);
  if (!id) die('usage: hub close <message-id> [reason]   (skip it without paying for triage)');
  const m = dbm.getMessage(db, id);
  if (!m) die(`no message #${id}`);
  const reason = positional.slice(1).join(" ") || "closed by owner";
  dbm.setMessageStatus(db, id, "closed", { last_error: null });
  const d = dbm.latestDecisionFor(db, id);
  if (d?.status === "pending") {
    dbm.cancelOutboxFor(db, d.id);
    dbm.setDecisionStatus(db, d.id, "rejected", reason);
  }
  out(`closed message #${id} (${reason})`);
}

async function cmdReply() {
  const id = Number(positional[0]);
  const text = positional.slice(1).join(" ");
  if (!id || !text) die('usage: hub reply <message-id> "text"');
  const m = dbm.getMessage(db, id);
  if (!m) die(`no message #${id}`);
  const raw = policy.parseRaw(m);
  const target =
    m.source === "github" ? policy.githubReplyTarget(m, raw)
    : m.source === "email" ? policy.emailAddress(m.sender)
    : m.source === "telegram" ? String(raw.chat_id ?? "")
    : null;
  if (!target) die(`cannot reply to source=${m.source} (no target)`);

  dbm.enqueueOutbox(db, {
    message_id: m.id,
    channel: m.source,
    target,
    subject: m.source === "email" ? `Re: ${m.subject ?? ""}`.slice(0, 200) : null,
    body: text,
  });
  dbm.setMessageStatus(db, m.id, "answered");
  const res = await flush(db, cfg);
  out(`reply queued to ${m.source}:${target}; sent=${res.sent} failed=${res.failed}`);
}

async function cmdAct() {
  if (!cfg.act.enabled) die("act.enabled = false in config — turn it on deliberately before running the code-change stage");
  const { d, m } = decisionOrDie(positional[0]);
  if (d.status !== "approved" && !flags.force) {
    die(`decision #${d.id} is '${d.status}' — approve it first (hub approve ${d.id}) or pass --force`);
  }
  out(`act stage: project=${d.project ?? m.project} model=${cfg.act.model} budget=$${cfg.act.max_budget_usd} timeout=${cfg.act.timeout_sec}s`);
  const r = await act({ msg: m, decision: d, cfg });
  out("");
  if (!r.ok) {
    dbm.setDecisionStatus(db, d.id, "approved", `act failed: ${r.error}`);
    die(`act failed: ${r.error}\nworktree: ${r.worktree ?? "(none)"}`);
  }
  dbm.setDecisionStatus(db, d.id, "executed", `branch ${r.branch} in ${r.worktree}`);
  out(`branch:   ${r.branch}`);
  out(`worktree: ${r.worktree}`);
  out(`cost:     $${r.cost_usd.toFixed(4)}`);
  out("");
  out("--- diffstat ---");
  out(r.diffstat || "(no changes committed)");
  out("--- agent report ---");
  out(r.report);
  out("");
  out(`review:  git -C ${r.worktree} diff HEAD~1..HEAD`);
  out(`discard: git -C ${projectDir(cfg, d.project ?? m.project)} worktree remove ${r.worktree} --force`);
}

function cmdStatus() {
  const c = dbm.counts(db);
  out(`messages     ${JSON.stringify(c.messages)}`);
  out(`decisions    ${JSON.stringify(c.decisions)}`);
  out(`outbox       ${JSON.stringify(c.outbox)}`);
  out(`dead_letter  ${c.dead_letter}`);
  out(`spend        $${c.cost_usd_total.toFixed(4)}`);
  out("");
  out("last polls:");
  for (const r of dbm.lastRuns(db, 12)) {
    out(
      `  ${r.started_at}  ${String(r.adapter).padEnd(9)} ${r.ok === null ? "running" : r.ok ? "ok " : "ERR"}  new=${r.n_new ?? 0}` +
        (r.skipped ? `  skipped: ${short(r.skipped, 60)}` : "") +
        (r.err ? `  err: ${short(r.err, 80)}` : ""),
    );
  }
}

function cmdHelp() {
  out(
    `hub — comms hub for email / GitHub / project events / chat → Claude triage

  hub doctor                    check channels + credentials (start here)
  hub init                      write hub.config.json + create the db
  hub once                      one cycle: ingest → triage → send
  hub ingest | triage | flush    single phase
  hub inbox [--status s] [--limit n] [-p project]
  hub show <msg-id | d<decision-id>>
  hub say "text" [-p project]   ask the hub something yourself
  hub approve <decision-id> [--body "..."]
  hub reject  <decision-id> [reason]
  hub close   <msg-id> [reason]  skip an item without paying for triage
  hub reply   <msg-id> "text"
  hub act     <decision-id>     implement an approved change on a branch
  hub status                    counts, poll health, spend

  flags: --config <file> --debug --limit n --status s --force
  daemon: bin/hubd.mjs (loop) — see README.md`,
  );
}

// ─── dispatch ────────────────────────────────────────────────────────────

try {
  switch (cmd) {
    case "doctor": await cmdDoctor(); break;
    case "init": cmdInit(); break;
    case "once": out(JSON.stringify(await runOnce(db, cfg), null, 2)); break;
    case "ingest": out(JSON.stringify(await ingest(db, cfg), null, 2)); break;
    case "triage": out(JSON.stringify(await triageNew(db, cfg), null, 2)); break;
    case "flush": out(JSON.stringify(await flush(db, cfg), null, 2)); break;
    case "inbox": cmdInbox(); break;
    case "show": cmdShow(); break;
    case "say": await cmdSay(); break;
    case "approve": await cmdApprove(); break;
    case "reject": await cmdReject(); break;
    case "close": cmdClose(); break;
    case "reply": await cmdReply(); break;
    case "act": await cmdAct(); break;
    case "status": cmdStatus(); break;
    case "help": case "--help": case "-h": cmdHelp(); break;
    default: die(`unknown command "${cmd}" — try: hub help`, 64);
  }
} catch (e) {
  log.error("cli_command_failed", { cmd, err: e?.message ?? String(e), stack: e?.stack?.split("\n").slice(0, 3).join(" | ") });
  die(e?.message ?? String(e));
} finally {
  db.close();
}
