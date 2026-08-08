// GitHub ingest via the `gh` CLI — no webhook, no public endpoint, no PAT in
// config: `gh` already holds the auth. Two streams:
//
//   1. /notifications  — CI failures, mentions, review requests, comments
//   2. per-repo issues + issue comments (opt-in `repos: []`), so GitHub Issues
//      can be used as a user-feedback channel even after notifications are read
//
// Each item is normalized into a hub message. external_id embeds the item's
// updated_at, so an updated thread is a NEW message while a replayed poll
// window is deduped.

import { run, runJson } from "../exec.mjs";
import { errFields, log } from "../log.mjs";

export const name = "github";

const MAX_BODY = 20_000;

function truncate(s, n = MAX_BODY) {
  if (!s) return "";
  return s.length > n ? s.slice(0, n) + `\n…[truncated ${s.length - n} chars]` : s;
}

/** `gh api <path>` → parsed JSON, or {ok:false,error}. */
async function ghApi(path, timeoutMs = 45_000) {
  return runJson("gh", ["api", "-H", "Accept: application/vnd.github+json", path], { timeoutMs });
}

/** Verify the CLI is usable before we blame the network for empty results. */
export async function health() {
  const r = await run("gh", ["auth", "status"], { timeoutMs: 20_000 });
  return { ok: r.code === 0, detail: (r.stdout + r.stderr).split("\n").slice(0, 3).join(" ").trim() };
}

function apiPathToHtml(url) {
  // https://api.github.com/repos/o/r/issues/12 → https://github.com/o/r/issues/12
  if (!url) return null;
  return url.replace("https://api.github.com/repos/", "https://github.com/");
}

/**
 * Pure normalizer — a raw notification (+ optionally its fetched detail) into a
 * hub message. Kept separate from the fetching so it can be unit-tested against
 * captured real payloads.
 */
export function normalizeNotification(n, detail = null, detailError = null) {
  const repo = n.repository?.full_name ?? "unknown/unknown";
  const title = n.subject?.title ?? "(no title)";
  const type = n.subject?.type ?? "Unknown";

  let body = title;
  let sender = `github:${repo}`;
  let url = apiPathToHtml(n.subject?.url) ?? n.repository?.html_url ?? null;

  if (detail) {
    if (detail.body) body = truncate(detail.body);
    if (detail.user?.login) sender = detail.user.login;
    if (detail.html_url) url = detail.html_url;
  } else if (detailError) {
    body = `${title}\n\n[hub: could not fetch item body: ${detailError}]`;
  }

  return {
    source: name,
    external_id: `notif:${n.id}:${n.updated_at}`,
    thread_key: `${repo}:${type}:${n.subject?.url ?? title}`,
    sender,
    subject: `[${repo}] ${title}`,
    body,
    url,
    received_at: n.updated_at,
    raw: {
      stream: "notifications",
      reason: n.reason,
      type,
      repo,
      notification_id: n.id,
      detail: detail
        ? { number: detail.number, state: detail.state, user: detail.user?.login, html_url: detail.html_url }
        : null,
    },
  };
}

async function notificationStream(cfg, cursor) {
  const params = new URLSearchParams({
    all: String(Boolean(cfg.include_read)),
    per_page: String(cfg.per_page ?? 30),
  });
  if (cursor) params.set("since", cursor);

  const res = await ghApi(`/notifications?${params}`);
  if (!res.ok) throw new Error(`gh /notifications failed: ${res.error}`);
  const items = Array.isArray(res.value) ? res.value : [];

  const messages = [];
  let detailBudget = cfg.detail_limit ?? 12;

  for (const n of items) {
    let detail = null;
    let detailError = null;

    // Bodies live behind a second call; spend the budget on the newest items.
    const detailUrl = n.subject?.latest_comment_url || n.subject?.url;
    if (detailUrl && detailBudget > 0 && /^https:\/\/api\.github\.com\//.test(detailUrl)) {
      detailBudget -= 1;
      const d = await ghApi(detailUrl.replace("https://api.github.com", ""));
      if (d.ok) {
        detail = d.value;
      } else {
        // Detail fetch is best-effort, but must never vanish quietly.
        log.warn("github_detail_fetch_failed", { url: detailUrl, err: d.error });
        detailError = d.error;
      }
    }

    messages.push(normalizeNotification(n, detail, detailError));
  }

  const newest = items.map((n) => n.updated_at).filter(Boolean).sort().at(-1);
  return { messages, newestTs: newest ?? null };
}

async function repoStream(repo, since) {
  const messages = [];
  const sinceParam = since ? `&since=${encodeURIComponent(since)}` : "";

  // Issue comments (the actual feedback text people write).
  const comments = await ghApi(`/repos/${repo}/issues/comments?sort=updated&direction=desc&per_page=30${sinceParam}`);
  if (!comments.ok) throw new Error(`gh issue comments ${repo} failed: ${comments.error}`);
  for (const c of comments.value ?? []) {
    messages.push({
      source: name,
      external_id: `comment:${repo}:${c.id}:${c.updated_at}`,
      thread_key: `${repo}:Issue:${c.issue_url}`,
      sender: c.user?.login ?? "unknown",
      subject: `[${repo}] comment on ${c.issue_url?.split("/").pop() ? "#" + c.issue_url.split("/").pop() : "issue"}`,
      body: truncate(c.body ?? ""),
      url: c.html_url,
      received_at: c.updated_at,
      raw: { stream: "issue_comments", repo, comment_id: c.id, issue_url: c.issue_url },
    });
  }

  // Newly opened / updated issues.
  const issues = await ghApi(`/repos/${repo}/issues?state=open&sort=updated&direction=desc&per_page=20${sinceParam}`);
  if (!issues.ok) throw new Error(`gh issues ${repo} failed: ${issues.error}`);
  for (const i of issues.value ?? []) {
    if (i.pull_request) continue; // PRs arrive through notifications
    messages.push({
      source: name,
      external_id: `issue:${repo}:${i.number}:${i.updated_at}`,
      thread_key: `${repo}:Issue:${i.url}`,
      sender: i.user?.login ?? "unknown",
      subject: `[${repo}] #${i.number} ${i.title}`,
      body: truncate(i.body ?? i.title ?? ""),
      url: i.html_url,
      received_at: i.updated_at,
      raw: { stream: "issues", repo, number: i.number, labels: (i.labels ?? []).map((l) => l.name ?? l) },
    });
  }

  return messages;
}

/**
 * @returns {Promise<{messages:Array, cursors:Object, skipped?:string}>}
 */
export async function poll({ cfg, cursors }) {
  const h = await health();
  if (!h.ok) {
    // Not a silent skip: caller records this on the run row.
    throw new Error(`gh CLI not authenticated: ${h.detail}`);
  }

  const out = [];
  const nextCursors = {};

  const notif = await notificationStream(cfg, cursors["github:since"]);
  out.push(...notif.messages);
  if (notif.newestTs) nextCursors["github:since"] = notif.newestTs;

  for (const repo of cfg.repos ?? []) {
    const key = `github:repo:${repo}:since`;
    try {
      const msgs = await repoStream(repo, cursors[key]);
      out.push(...msgs);
      const newest = msgs.map((m) => m.received_at).filter(Boolean).sort().at(-1);
      if (newest) nextCursors[key] = newest;
    } catch (e) {
      // One bad repo must not sink the whole poll — but it is logged AND
      // rethrown-as-partial so `runs.err` shows it.
      log.error("github_repo_stream_failed", { repo, ...errFields(e) });
      nextCursors[`${key}:last_error`] = String(e.message ?? e);
    }
  }

  return { messages: out, cursors: nextCursors };
}

/** Post a comment back onto the originating issue/PR thread. */
export async function send({ target, body }) {
  // target = "owner/repo#123"
  const m = /^([^/]+\/[^#]+)#(\d+)$/.exec(target);
  if (!m) throw new Error(`github send: bad target "${target}", want owner/repo#123`);
  const [, repo, number] = m;
  const r = await run(
    "gh",
    ["api", "--method", "POST", `/repos/${repo}/issues/${number}/comments`, "-f", `body=${body}`],
    { timeoutMs: 45_000 },
  );
  if (r.code !== 0) throw new Error(`gh comment failed (exit ${r.code}): ${r.stderr.slice(0, 500)}`);
  try {
    const posted = JSON.parse(r.stdout);
    return { id: posted.id, url: posted.html_url };
  } catch (e) {
    // The comment did post (exit 0); only the confirmation is unreadable.
    log.warn("github_comment_response_unparseable", { target, err: e.message, head: r.stdout.slice(0, 200) });
    return { id: null, url: null };
  }
}
