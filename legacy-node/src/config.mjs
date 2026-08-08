// Config loading + validation.
//
// Secrets NEVER live in the config file — only the NAME of the env var that
// holds them (`token_env`, `api_key_env`). Charter DoD #8.

import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const HUB_DIR = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const WORKSPACE_ROOT = resolve(HUB_DIR, "..", "..");

export const TIERS = ["L0", "L1", "L2"];

/**
 * Actions the hub is NEVER allowed to perform without a human pressing
 * approve — regardless of tier or config. Deliberately not configurable.
 */
export const ALWAYS_HUMAN_ACTIONS = new Set(["deploy", "merge", "force_push", "delete_data", "rotate_secret"]);

const DEFAULTS = {
  db: join(HUB_DIR, "data", "hub.sqlite"),
  log_file: join(HUB_DIR, "logs", "hub.log"),
  workspace_root: WORKSPACE_ROOT,
  poll_interval_sec: 120,
  max_triage_per_cycle: 8,
  // A repeat item on a thread that already has an unanswered decision is
  // attached to it instead of triaged again (0 disables). Real numbers from
  // 2026-07-26: one triage call costs $0.05–$0.11, and CI spams the same
  // failure repeatedly — this is the difference between $0.11 and $1.10.
  coalesce_hours: 12,
  triage: {
    model: "sonnet",
    max_budget_usd: 0.5,
    timeout_sec: 240,
    min_confidence_auto: 0.75,
    context_bytes: 6000,
  },
  act: {
    // Code-change stage. Off until the user turns it on per project.
    enabled: false,
    model: "sonnet",
    max_budget_usd: 3,
    timeout_sec: 1800,
  },
  autonomy: {
    // L0 = draft only · L1 = auto-reply informational · L2 = may open a PR branch
    default: "L0",
    projects: {},
  },
  adapters: {
    github: { enabled: true, per_page: 30, detail_limit: 12, include_read: false },
    devlog: { enabled: true, projects: [], kinds: ["warning", "blocker", "bug", "test_fail", "question"], max_per_project: 20 },
    email: { enabled: false, base_url: "https://mail.dipgle.com", api_key_env: "HUB_MAILLER_API_KEY", folder: "inbox", limit: 30 },
    telegram: { enabled: false, token_env: "HUB_TELEGRAM_TOKEN", allowed_chat_ids: [], poll_timeout_sec: 20 },
  },
  trust: {
    github_logins: [],
    emails: [],
    telegram_chat_ids: [],
    // devlog events come from our own machine
    trusted_sources: ["devlog", "cli"],
  },
  routing: [],
  // Extra regexes (strings) that must never appear in an outbound auto-reply.
  // See src/redaction.mjs for the built-in list.
  leak_patterns: [],
  notify: {
    // Local fallback channel: always available, no credentials.
    file: join(HUB_DIR, "logs", "notify.log"),
    macos_notification: true,
  },
};

function isPlainObject(v) {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}

function deepMerge(base, override) {
  const out = Array.isArray(base) ? [...base] : { ...base };
  for (const [k, v] of Object.entries(override ?? {})) {
    if (isPlainObject(v) && isPlainObject(base?.[k])) out[k] = deepMerge(base[k], v);
    else out[k] = v;
  }
  return out;
}

function expandHome(p) {
  if (typeof p !== "string") return p;
  return p.startsWith("~/") ? join(homedir(), p.slice(2)) : p;
}

function absolutize(p, baseDir) {
  const e = expandHome(p);
  return isAbsolute(e) ? e : resolve(baseDir, e);
}

/** Throws on invalid config — a hub that starts with a broken policy is worse than one that refuses. */
export function validateConfig(cfg) {
  const problems = [];
  if (!TIERS.includes(cfg.autonomy.default)) problems.push(`autonomy.default must be one of ${TIERS.join("|")}`);
  for (const [proj, tier] of Object.entries(cfg.autonomy.projects)) {
    if (!TIERS.includes(tier)) problems.push(`autonomy.projects.${proj} = ${tier} is not a valid tier`);
  }
  if (!(cfg.poll_interval_sec >= 10)) problems.push("poll_interval_sec must be >= 10");
  if (!(cfg.triage.min_confidence_auto >= 0 && cfg.triage.min_confidence_auto <= 1)) {
    problems.push("triage.min_confidence_auto must be within 0..1");
  }
  if (!(cfg.triage.max_budget_usd > 0)) problems.push("triage.max_budget_usd must be > 0");
  for (const rule of cfg.routing) {
    if (!isPlainObject(rule.when) || typeof rule.project !== "string") {
      problems.push(`routing rule needs {when:{...}, project:"name"}: ${JSON.stringify(rule)}`);
    }
  }
  if (problems.length) throw new Error(`invalid hub config:\n  - ${problems.join("\n  - ")}`);
  return cfg;
}

/**
 * Load config from disk (defaults when the file is absent), expand paths,
 * validate. `HUB_CONFIG` env var overrides the path.
 */
export function loadConfig(path) {
  const file = path ?? process.env.HUB_CONFIG ?? join(HUB_DIR, "hub.config.json");
  let onDisk = {};
  if (existsSync(file)) {
    try {
      onDisk = JSON.parse(readFileSync(file, "utf8"));
    } catch (e) {
      throw new Error(`cannot parse config ${file}: ${e.message}`);
    }
  }
  const cfg = deepMerge(DEFAULTS, onDisk);
  cfg.config_file = file;
  cfg.db = absolutize(cfg.db, HUB_DIR);
  cfg.log_file = absolutize(cfg.log_file, HUB_DIR);
  cfg.notify.file = absolutize(cfg.notify.file, HUB_DIR);
  cfg.workspace_root = absolutize(cfg.workspace_root, HUB_DIR);
  return validateConfig(cfg);
}

/**
 * Where a project lives. Most projects sit under AI/, but some (dwork, social,
 * sso-user, uiux, video) sit directly in the workspace root — so never hardcode
 * `AI/<name>`.
 * @returns {string|null} absolute path, or null when the project is not found
 */
export function projectDir(cfg, project) {
  if (!project || project === "unknown") return null;
  for (const candidate of [join(cfg.workspace_root, "AI", project), join(cfg.workspace_root, project)]) {
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

/**
 * Read a secret by env-var name. Returns null when unset so the caller can
 * SKIP-WITH-LOG instead of crashing (charter DoD #6: log-on-skip).
 */
export function secretFromEnv(envName) {
  const v = envName ? process.env[envName] : null;
  return v && v.trim() ? v.trim() : null;
}

export { DEFAULTS as CONFIG_DEFAULTS };
