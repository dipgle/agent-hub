// Email channel over the house mail server (mailler, live at mail.dipgle.com).
//
// Uses the webmail REST API with a per-user API key (Bearer):
//   GET  /api/v1/messages?folder=inbox&limit=N   → list          (main.rs:660)
//   GET  /api/v1/messages/:id                    → body          (main.rs:661)
//   POST /api/v1/messages {to,subject,text}      → compose/send  (main.rs:660)
//
// Mint a key in webmail → Settings → API keys, then export it:
//   export HUB_MAILLER_API_KEY=...
// Without the key this adapter SKIPS WITH A LOG (never silently).

import { secretFromEnv } from "../config.mjs";
import { log } from "../log.mjs";

export const name = "email";

const MAX_BODY = 20_000;

class SkipAdapter extends Error {
  constructor(reason) {
    super(reason);
    this.name = "SkipAdapter";
    this.skip = true;
  }
}

async function api(cfg, path, { method = "GET", body, key, timeoutMs = 30_000 } = {}) {
  const url = `${cfg.base_url.replace(/\/$/, "")}${path}`;
  const ac = new AbortController();
  const timer = setTimeout(() => ac.abort(), timeoutMs);
  try {
    const res = await fetch(url, {
      method,
      signal: ac.signal,
      headers: {
        Authorization: `Bearer ${key}`,
        Accept: "application/json",
        ...(body ? { "Content-Type": "application/json" } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    const text = await res.text();
    if (!res.ok) throw new Error(`${method} ${path} → HTTP ${res.status}: ${text.slice(0, 300)}`);
    try {
      return JSON.parse(text);
    } catch (e) {
      throw new Error(`${method} ${path} returned non-JSON: ${text.slice(0, 200)}`);
    }
  } finally {
    clearTimeout(timer);
  }
}

export async function health(cfg) {
  const key = secretFromEnv(cfg.api_key_env);
  if (!key) return { ok: false, detail: `${cfg.api_key_env} not set` };
  try {
    const me = await api(cfg, "/api/v1/auth/me", { key });
    return { ok: true, detail: `authenticated as ${me?.user?.address ?? me?.user?.email ?? "unknown"}` };
  } catch (e) {
    return { ok: false, detail: e.message };
  }
}

function pick(obj, keys) {
  for (const k of keys) if (obj?.[k] !== undefined && obj?.[k] !== null) return obj[k];
  return undefined;
}

export async function poll({ cfg, cursors }) {
  const key = secretFromEnv(cfg.api_key_env);
  if (!key) throw new SkipAdapter(`${cfg.api_key_env} not set — email ingest disabled`);

  const listed = await api(cfg, `/api/v1/messages?folder=${encodeURIComponent(cfg.folder ?? "inbox")}&limit=${cfg.limit ?? 30}`, { key });
  const items = Array.isArray(listed) ? listed : (listed.messages ?? listed.items ?? []);
  if (!Array.isArray(items)) throw new Error(`unexpected list shape: ${JSON.stringify(listed).slice(0, 200)}`);

  // First run takes the current inbox tip as the baseline: the hub answers mail
  // that arrives from now on, it does not re-litigate the whole mailbox.
  // Set `backfill: true` in the adapter config to ingest what is already there.
  if (cursors["email:last_id"] === undefined && !cfg.backfill) {
    const ids = items.map((m) => Number(pick(m, ["id", "message_id", "uid"]))).filter(Number.isFinite);
    const tip = ids.length ? Math.max(...ids) : 0;
    log.info("email_baseline_set", { last_id: tip, seen: ids.length });
    return { messages: [], cursors: { "email:last_id": String(tip) } };
  }

  const lastSeen = Number(cursors["email:last_id"] ?? 0);
  const messages = [];
  let maxId = lastSeen;

  for (const m of items) {
    const id = Number(pick(m, ["id", "message_id", "uid"]));
    if (!Number.isFinite(id)) {
      log.warn("email_item_without_id", { item: JSON.stringify(m).slice(0, 200) });
      continue;
    }
    if (id <= lastSeen) continue;
    maxId = Math.max(maxId, id);

    // The list endpoint returns headers + snippet; fetch the body.
    let body = pick(m, ["text", "snippet", "preview"]) ?? "";
    try {
      const full = await api(cfg, `/api/v1/messages/${id}`, { key });
      const msg = full?.message ?? full;
      body = pick(msg, ["text", "body_text", "body", "html"]) ?? body;
    } catch (e) {
      log.warn("email_body_fetch_failed", { id, err: e.message });
      body = `${body}\n\n[hub: body fetch failed: ${e.message}]`;
    }

    const from = pick(m, ["from", "from_address", "sender"]) ?? "unknown";
    messages.push({
      source: name,
      external_id: `mailler:${id}`,
      thread_key: pick(m, ["thread_id", "message_id_header"]) ?? `email:${pick(m, ["subject"]) ?? id}`,
      sender: typeof from === "string" ? from : (from.address ?? JSON.stringify(from)),
      subject: pick(m, ["subject"]) ?? "(no subject)",
      body: String(body).slice(0, MAX_BODY),
      url: `${cfg.base_url.replace(/\/$/, "")}/#/message/${id}`,
      received_at: pick(m, ["date", "received_at", "created_at"]) ?? null,
      raw: { stream: "mailler_inbox", id, folder: cfg.folder ?? "inbox" },
    });
  }

  return { messages, cursors: maxId > lastSeen ? { "email:last_id": String(maxId) } : {} };
}

/** target = recipient address */
export async function send({ cfg, target, subject, body }) {
  const key = secretFromEnv(cfg.api_key_env);
  if (!key) throw new Error(`cannot send email: ${cfg.api_key_env} not set`);
  const res = await api(cfg, "/api/v1/messages", {
    method: "POST",
    key,
    body: { to: target, cc: "", bcc: "", subject: subject ?? "(no subject)", text: body },
  });
  return { id: res?.id ?? null };
}

export { SkipAdapter };
