// One cycle of the hub: ingest → triage → policy → outbox flush.
//
// Ordering matters for durability: a poll cursor only advances AFTER the
// messages from that window are committed, so a crash re-polls instead of
// losing items.

import {
  allCursors, claimNewMessages, deadLetter, enqueueOutbox, getMessage, insertDecision, insertMessage,
  finishRun, pendingDecisionForThread, resetTriaging, setCursor, setMessageStatus, startRun,
} from "./db.mjs";
import { errFields, log } from "./log.mjs";
import { decideOutcome, effectiveTier, humanBrief, resolveProject, resolveTrust } from "./policy.mjs";
import { compileExtraPatterns, EXTERNAL_CHANNELS, leakScan } from "./redaction.mjs";
import { triage } from "./triage.mjs";
import { flush } from "./outbound.mjs";
import * as github from "./adapters/github.mjs";
import * as devlog from "./adapters/devlog.mjs";
import * as email from "./adapters/email.mjs";
import * as telegram from "./adapters/telegram.mjs";

export const ADAPTERS = { github, devlog, email, telegram };

const MAX_TRIAGE_ATTEMPTS = 3;

/** Project folder names, used by routing heuristics. */
export function knownProjects(cfg) {
  return devlog.discoverProjects(cfg.workspace_root);
}

export async function ingest(db, cfg) {
  const summary = {};
  const projects = knownProjects(cfg);

  for (const [name, mod] of Object.entries(ADAPTERS)) {
    const acfg = cfg.adapters[name];
    if (!acfg?.enabled) {
      summary[name] = { skipped: "disabled in config" };
      log.info("adapter_skipped", { adapter: name, reason: "disabled" });
      continue;
    }

    const runId = startRun(db, name, "poll");
    try {
      const res = await mod.poll({ cfg: acfg, cursors: allCursors(db), workspaceRoot: cfg.workspace_root });
      let inserted = 0;

      for (const m of res.messages ?? []) {
        const project = m.project ?? resolveProject(m, cfg, projects);
        const trust = m.sender_trust ?? resolveTrust(m, cfg);
        const { inserted: isNew } = insertMessage(db, { ...m, project, sender_trust: trust });
        if (isNew) inserted += 1;
      }

      // Cursors last: an insert failure above must not skip the window.
      for (const [k, v] of Object.entries(res.cursors ?? {})) setCursor(db, k, v);

      finishRun(db, runId, { ok: true, nNew: inserted, skipped: res.skipped ?? null });
      summary[name] = { polled: res.messages?.length ?? 0, new: inserted, partial: res.skipped ?? null };
      log.info("adapter_polled", { adapter: name, polled: res.messages?.length ?? 0, new: inserted, partial: res.skipped ?? null });
    } catch (e) {
      if (e?.skip) {
        // Missing credentials: a deliberate skip, recorded and logged (never silent).
        finishRun(db, runId, { ok: true, nNew: 0, skipped: e.message });
        summary[name] = { skipped: e.message };
        log.warn("adapter_skipped", { adapter: name, reason: e.message });
        continue;
      }
      finishRun(db, runId, { ok: false, err: e.message ?? String(e) });
      deadLetter(db, { source: name, stage: "ingest", err: e.message ?? String(e) });
      summary[name] = { error: e.message ?? String(e) };
      log.error("adapter_poll_failed", { adapter: name, ...errFields(e) });
    }
  }

  return summary;
}

/** Where a human brief should also be pushed (phone), when configured. */
function humanChannel(cfg) {
  const tg = cfg.adapters.telegram;
  if (tg?.enabled && (tg.allowed_chat_ids ?? []).length) {
    return { channel: "telegram", target: String(tg.allowed_chat_ids[0]) };
  }
  return { channel: "notify", target: "local" };
}

export const emptyTriageCounters = () => ({
  triaged: 0, auto_replied: 0, awaiting_human: 0, ignored: 0, coalesced: 0, failed: 0, cost_usd: 0,
});

/**
 * Triage exactly one message: coalesce → classify → policy → outbox.
 * Mutates `out` with the counters. Used by both the batch loop and `hub say`.
 */
export async function processMessage(db, cfg, row, { projects, out, allowCoalesce = true }) {
  {
    // Same thread, already waiting on a human? Attach, do not pay again.
    if (allowCoalesce && cfg.coalesce_hours > 0) {
      const since = new Date(Date.now() - cfg.coalesce_hours * 3600_000).toISOString();
      const open = pendingDecisionForThread(db, row.thread_key, since);
      if (open) {
        setMessageStatus(db, row.id, "coalesced", { last_error: null });
        out.coalesced += 1;
        log.info("message_coalesced", { message_id: row.id, into_decision: open.id, thread_key: row.thread_key });
        return out;
      }
    }

    const project = row.project ?? resolveProject(row, cfg, projects);
    const trust = row.sender_trust ?? resolveTrust(row, cfg);
    setMessageStatus(db, row.id, "triaging", { project, sender_trust: trust, bumpAttempts: true });

    const msg = { ...row, project, sender_trust: trust };
    const t = await triage(msg, cfg);
    out.cost_usd += t.cost_usd ?? 0;

    if (!t.ok) {
      const attempts = (row.attempts ?? 0) + 1;
      if (attempts >= MAX_TRIAGE_ATTEMPTS) {
        setMessageStatus(db, row.id, "failed", { last_error: t.error });
        deadLetter(db, { source: row.source, external_id: row.external_id, stage: "triage", payload: { subject: row.subject }, err: t.error });
        enqueueOutbox(db, {
          message_id: row.id,
          ...humanChannel(cfg),
          subject: `hub: triage failed ${attempts}× (${row.source})`,
          body: `message #${row.id} ${row.subject ?? ""}\n\nlast error: ${t.error}`,
        });
      } else {
        setMessageStatus(db, row.id, "new", { last_error: t.error });
      }
      out.failed += 1;
      return out;
    }

    const decision = t.decision;
    const tier = effectiveTier(project, trust, cfg);
    let outcome = decideOutcome({ msg, decision, tier, trust, tripwire: t.tripwire, cfg });

    // Last gate before anything leaves the machine: internal detail in an
    // outbound reply downgrades the item to human review.
    if (outcome.action === "auto_reply" && EXTERNAL_CHANNELS.has(outcome.channel)) {
      const leaks = leakScan(
        decision.reply_draft,
        compileExtraPatterns(cfg.leak_patterns, (src, e) => log.error("bad_leak_pattern", { pattern: src, err: e.message })),
      );
      if (leaks.length) {
        log.warn("outbound_leak_scan_blocked", { message_id: row.id, channel: outcome.channel, leaks });
        outcome = { ...outcome, action: "await_human", reason: `outbound leak scan: ${leaks.join(", ")}` };
      }
    }

    const decisionId = insertDecision(db, {
      message_id: row.id,
      tier,
      model: t.model,
      kind: decision.kind,
      severity: decision.severity,
      project: decision.project === "unknown" ? project : decision.project,
      summary: decision.summary,
      reply_draft: decision.reply_draft,
      actions: decision.proposed_actions,
      evidence: decision.evidence,
      confidence: decision.confidence,
      needs_human: outcome.action === "await_human",
      tripwire: t.tripwire,
      cost_usd: t.cost_usd,
      session_id: t.session_id,
      raw: { ...t.raw, outcome },
      status: outcome.action === "auto_reply" ? "auto" : outcome.action === "ignore" ? "auto" : "pending",
    });
    out.triaged += 1;

    const brief = humanBrief({ msg, decision, outcome, tier, decisionId });

    if (outcome.action === "auto_reply") {
      enqueueOutbox(db, {
        decision_id: decisionId,
        message_id: row.id,
        channel: outcome.channel,
        target: outcome.target,
        subject: row.source === "email" ? `Re: ${row.subject ?? ""}`.slice(0, 200) : null,
        body: decision.reply_draft,
      });
      // Always tell the human what went out under their name.
      enqueueOutbox(db, { decision_id: decisionId, message_id: row.id, ...humanChannel(cfg), subject: `hub auto-replied (${row.source})`, body: brief });
      setMessageStatus(db, row.id, "answered");
      out.auto_replied += 1;
    } else if (outcome.action === "ignore") {
      setMessageStatus(db, row.id, "closed");
      out.ignored += 1;
      log.info("message_ignored", { message_id: row.id, kind: decision.kind });
    } else {
      enqueueOutbox(db, { decision_id: decisionId, message_id: row.id, ...humanChannel(cfg), subject: `hub cần bạn xem (${decision.kind}/${decision.severity})`, body: brief });
      setMessageStatus(db, row.id, "awaiting_human");
      out.awaiting_human += 1;
    }

    log.info("message_triaged", {
      message_id: row.id, decision_id: decisionId, source: row.source, project,
      kind: decision.kind, severity: decision.severity, confidence: decision.confidence,
      tier, action: outcome.action, reason: outcome.reason, cost_usd: t.cost_usd,
      tripwire: t.tripwire.length ? t.tripwire : undefined,
    });
  }

  return out;
}

export async function triageNew(db, cfg) {
  const recovered = resetTriaging(db);
  if (recovered) log.warn("recovered_stuck_triaging", { rows: recovered });

  const batch = claimNewMessages(db, cfg.max_triage_per_cycle);
  const out = emptyTriageCounters();
  const projects = knownProjects(cfg);
  for (const row of batch) await processMessage(db, cfg, row, { projects, out });
  return out;
}

/** Triage one specific message now — what `hub say` needs. */
export async function triageMessageById(db, cfg, messageId, { allowCoalesce = false } = {}) {
  const row = getMessage(db, messageId);
  if (!row) throw new Error(`no message #${messageId}`);
  const out = emptyTriageCounters();
  await processMessage(db, cfg, row, { projects: knownProjects(cfg), out, allowCoalesce });
  return out;
}

export async function runOnce(db, cfg) {
  const started = Date.now();
  const ingested = await ingest(db, cfg);
  const triaged = await triageNew(db, cfg);
  const sent = await flush(db, cfg);
  const summary = { ms: Date.now() - started, ingested, triaged, sent };
  log.info("cycle_done", summary);
  return summary;
}
