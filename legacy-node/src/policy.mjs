// Policy gate — decides what the hub is ALLOWED to do with a decision.
//
// Autonomy tiers (per project, `autonomy.projects`):
//   L0  draft only — nothing leaves the machine without a human pressing approve
//   L1  may auto-send an informational reply on the same channel
//   L2  may additionally run the act stage (code change on a branch → PR)
//
// Invariants that no config can loosen:
//   * an untrusted sender caps the tier at L0
//   * deploy / merge / force-push / data deletion / secret rotation always
//     require a human (config.ALWAYS_HUMAN_ACTIONS)
//   * a tripwire hit (prompt injection) forces human review
//   * the act stage never touches main and never deploys

import { ALWAYS_HUMAN_ACTIONS, TIERS } from "./config.mjs";
import { log } from "./log.mjs";

export const TIER_RANK = { L0: 0, L1: 1, L2: 2 };

const AUTO_REPLY_KINDS = new Set(["question", "status_update", "feature_request"]);

/** Extract "owner/repo#123" from a github message, when it has a thread. */
export function githubReplyTarget(msg, raw) {
  const repo = raw?.repo;
  if (!repo) return null;
  const num =
    raw?.detail?.number ??
    raw?.number ??
    (typeof raw?.issue_url === "string" ? Number(raw.issue_url.split("/").pop()) : null) ??
    (typeof msg.url === "string" && /\/(issues|pull)\/(\d+)/.test(msg.url) ? Number(/\/(issues|pull)\/(\d+)/.exec(msg.url)[2]) : null);
  return Number.isFinite(num) && num > 0 ? `${repo}#${num}` : null;
}

/** Bare address out of "Name <a@b.com>". */
export function emailAddress(sender) {
  if (!sender) return null;
  const m = /<([^>]+)>/.exec(sender);
  return (m ? m[1] : sender).trim().toLowerCase();
}

export function parseRaw(msg) {
  if (!msg?.raw) return {};
  if (typeof msg.raw === "object") return msg.raw;
  try {
    return JSON.parse(msg.raw);
  } catch (e) {
    // Losing raw means losing the reply target — routing silently degrades to
    // "no target", so this must be visible.
    log.warn("message_raw_unparseable", { message_id: msg.id, source: msg.source, err: e.message });
    return {};
  }
}

/**
 * Which project does this belong to? Order: explicit → routing rules →
 * repo/subject heuristics → null (unknown).
 */
export function resolveProject(msg, cfg, knownProjects = []) {
  if (msg.project) return msg.project;
  const raw = parseRaw(msg);

  for (const rule of cfg.routing ?? []) {
    const w = rule.when ?? {};
    const checks = [
      w.source === undefined || w.source === msg.source,
      w.repo === undefined || w.repo === raw.repo,
      w.sender === undefined || (msg.sender ?? "").toLowerCase().includes(String(w.sender).toLowerCase()),
      w.chat_id === undefined || String(w.chat_id) === String(raw.chat_id ?? ""),
      w.subject_contains === undefined || (msg.subject ?? "").toLowerCase().includes(String(w.subject_contains).toLowerCase()),
      w.body_contains === undefined || (msg.body ?? "").toLowerCase().includes(String(w.body_contains).toLowerCase()),
    ];
    if (checks.every(Boolean)) return rule.project;
  }

  // "dipgle/tfl5" → tfl5 when a project folder of that name exists.
  if (raw.repo) {
    const repoName = String(raw.repo).split("/").pop();
    if (knownProjects.includes(repoName)) return repoName;
  }

  // "[tfl5] ..." or "tfl5:" at the start of a subject.
  const tag = /^\s*[\[(]([a-z0-9._-]{2,30})[\])]|^\s*([a-z0-9._-]{2,30})\s*:/i.exec(msg.subject ?? "");
  const candidate = (tag?.[1] ?? tag?.[2] ?? "").toLowerCase();
  if (candidate && knownProjects.includes(candidate)) return candidate;

  return null;
}

/** 'trusted' | 'untrusted' — who is allowed to make the hub act, not just answer. */
export function resolveTrust(msg, cfg) {
  const trust = cfg.trust ?? {};
  if ((trust.trusted_sources ?? []).includes(msg.source)) return "trusted";

  if (msg.source === "github") {
    const logins = (trust.github_logins ?? []).map((s) => s.toLowerCase());
    const sender = (msg.sender ?? "").replace(/^github:/, "").toLowerCase();
    if (logins.includes(sender)) return "trusted";
    // Repo-level notifications (CI, check suites) have no author — the sender is
    // "github:owner/repo". Our own repos count as trusted senders.
    const owner = sender.includes("/") ? sender.split("/")[0] : null;
    if (owner && logins.includes(owner)) return "trusted";
  }
  if (msg.source === "email") {
    const addr = emailAddress(msg.sender);
    if (addr && (trust.emails ?? []).map((s) => s.toLowerCase()).includes(addr)) return "trusted";
  }
  if (msg.source === "telegram") {
    const raw = parseRaw(msg);
    if ((trust.telegram_chat_ids ?? []).map(String).includes(String(raw.chat_id))) return "trusted";
    // The adapter already filtered to allowed_chat_ids; honour its verdict.
    if (msg.sender_trust === "trusted") return "trusted";
  }
  return "untrusted";
}

export function effectiveTier(project, trust, cfg) {
  const configured = (project && cfg.autonomy.projects?.[project]) || cfg.autonomy.default;
  const base = TIERS.includes(configured) ? configured : "L0";
  // Untrusted senders can never move the hub beyond drafting.
  return trust === "trusted" ? base : "L0";
}

/** Actions in the decision that a human must green-light no matter what. */
export function humanOnlyActions(decision) {
  return (decision?.proposed_actions ?? [])
    .map((a) => a.type)
    .filter((t) => ALWAYS_HUMAN_ACTIONS.has(t));
}

/**
 * @returns {{action:'auto_reply'|'await_human'|'ignore', channel:string|null,
 *            target:string|null, reason:string, human_only:string[]}}
 */
export function decideOutcome({ msg, decision, tier, trust, tripwire = [], cfg }) {
  const raw = parseRaw(msg);
  const human_only = humanOnlyActions(decision);
  const rank = TIER_RANK[tier] ?? 0;

  // Where an answer would go, per source. `cli` talks back through the local
  // notify channel so asking the hub something in the terminal actually returns
  // an answer; `devlog` has nobody to answer, so it always needs a human.
  const target =
    msg.source === "github" ? githubReplyTarget(msg, raw)
    : msg.source === "email" ? emailAddress(msg.sender)
    : msg.source === "telegram" ? String(raw.chat_id ?? "")
    : msg.source === "cli" ? "local"
    : null;

  const base = { channel: msg.source === "cli" ? "notify" : msg.source, target, human_only };

  if (tripwire.length) {
    return { ...base, action: "await_human", reason: `tripwire: ${tripwire.join(", ")}` };
  }
  if (["spam", "noise"].includes(decision.kind) && !decision.needs_human) {
    return { ...base, action: "ignore", reason: `kind=${decision.kind}` };
  }
  if (decision.needs_human) {
    return { ...base, action: "await_human", reason: "model set needs_human" };
  }
  if (human_only.length) {
    return { ...base, action: "await_human", reason: `action requires human: ${human_only.join(", ")}` };
  }
  if (decision.kind === "security") {
    return { ...base, action: "await_human", reason: "security items are never auto-answered" };
  }
  if (rank < TIER_RANK.L1) {
    return { ...base, action: "await_human", reason: `tier ${tier} (trust=${trust}) drafts only` };
  }
  if (!AUTO_REPLY_KINDS.has(decision.kind)) {
    return { ...base, action: "await_human", reason: `kind=${decision.kind} is not auto-repliable` };
  }
  if (!(decision.confidence >= cfg.triage.min_confidence_auto)) {
    return { ...base, action: "await_human", reason: `confidence ${decision.confidence} < ${cfg.triage.min_confidence_auto}` };
  }
  if (!decision.reply_draft?.trim()) {
    return { ...base, action: "await_human", reason: "empty reply_draft" };
  }
  if (!target) {
    return { ...base, action: "await_human", reason: `no reply target for source=${msg.source}` };
  }
  return { ...base, action: "auto_reply", reason: `tier ${tier}, confidence ${decision.confidence}` };
}

/** Compact human-facing brief used for the notify channel. */
export function humanBrief({ msg, decision, outcome, tier, decisionId }) {
  const acts = (decision.proposed_actions ?? []).map((a) => `${a.type}: ${a.detail}`).join("\n  ");
  return [
    `#${decisionId} [${decision.kind}/${decision.severity}] ${decision.project ?? "unknown"} — ${msg.source}`,
    `từ: ${msg.sender ?? "?"} (${msg.sender_trust})  tier=${tier}  conf=${decision.confidence}`,
    msg.subject ? `chủ đề: ${msg.subject}` : null,
    msg.url ? `link: ${msg.url}` : null,
    ``,
    `tóm tắt: ${decision.summary}`,
    acts ? `đề xuất:\n  ${acts}` : null,
    decision.reply_draft?.trim() ? `\nnháp trả lời:\n${decision.reply_draft.trim()}` : null,
    ``,
    `→ ${outcome.action} (${outcome.reason})`,
    outcome.action === "await_human" ? `duyệt: hub approve ${decisionId}   |   bỏ: hub reject ${decisionId}` : null,
  ]
    .filter((l) => l !== null)
    .join("\n");
}
