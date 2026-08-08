// Outbound leak scan — the last gate before text leaves this machine.
//
// WHY THIS EXISTS (observed, not theoretical): on 2026-07-26 a real triage run
// produced a reply draft whose evidence lines quoted the workspace auto-memory
// ("memory: tfl5 security hardening 2026-07 — CÒN OPEN: PG SPOF, …"). The
// triage subprocess has no tools, but `claude -p` still loads the workspace's
// memory and instruction files into its context. That is fine for a brief the
// owner reads; it is NOT fine in a reply to an outside sender.
//
// So: any auto-send to a channel that leaves the machine (github / email /
// telegram) is scanned first. A hit does not rewrite the text — it downgrades
// the item to human review, because silently truncating a reply is its own kind
// of failure.

const DEFAULT_PATTERNS = [
  [/\bvps-[ab]\b/i, "internal_host"],
  [/\b(?:\d{1,3}\.){3}\d{1,3}\b/, "ip_address"],
  [/\/Users\/[a-z0-9._-]+\//i, "local_filesystem_path"],
  [/\b(?:memory|MEMORY\.md|active-context\.md|CLAUDE\.md)\s*[::]/i, "internal_notes_citation"],
  [/\[\[[a-z0-9-]+\]\]/i, "memory_wikilink"],
  [/\b(api[_-]?key|bearer|password|secret|access[_-]?token|private[_-]?key)\b/i, "credential_word"],
  [/\b(sk-[A-Za-z0-9]{10,}|gh[pousr]_[A-Za-z0-9]{20,}|\d{8,10}:[A-Za-z0-9_-]{30,})\b/, "credential_literal"],
  [/-----BEGIN [A-Z ]*PRIVATE KEY-----/, "private_key_block"],
  [/\bSPOF\b|\bchưa fix\b|\bblocker\b/i, "internal_risk_language"],
];

/**
 * @param {string} text
 * @param {Array<[RegExp,string]>} [extra] additional [regex, label] pairs
 * @returns {string[]} labels of everything that must not go out unreviewed
 */
export function leakScan(text, extra = []) {
  if (!text) return [];
  const hits = new Set();
  for (const [re, label] of [...DEFAULT_PATTERNS, ...extra]) {
    if (re.test(text)) hits.add(label);
  }
  return [...hits];
}

/** Channels whose payload actually leaves this machine. */
export const EXTERNAL_CHANNELS = new Set(["github", "email", "telegram"]);

/**
 * Compile `leak_patterns: ["regex", …]` from config into [RegExp,label] pairs.
 * A bad pattern is reported, never swallowed.
 */
export function compileExtraPatterns(list, onError) {
  const out = [];
  for (const src of list ?? []) {
    try {
      out.push([new RegExp(src, "i"), `config:${src.slice(0, 30)}`]);
    } catch (e) {
      onError?.(src, e);
    }
  }
  return out;
}
