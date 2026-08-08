// Telegram channel — the human's control surface: you get pushed a summary on
// your phone and can answer/approve right there.
//
// Long-polling `getUpdates` means NO public endpoint and no inbound firewall
// hole (unlike a webhook). Bot token comes from the env var named in
// `adapters.telegram.token_env`; without it the adapter SKIPS WITH A LOG.
//
// Setup: talk to @BotFather → /newbot → copy token →
//   export HUB_TELEGRAM_TOKEN=123456:AA...
// then send your bot any message and run `hub doctor` to learn your chat id,
// and put that id in adapters.telegram.allowed_chat_ids.

import { secretFromEnv } from "../config.mjs";
import { log } from "../log.mjs";
import { SkipAdapter } from "./email.mjs";

export const name = "telegram";

async function api(token, method, payload, timeoutMs = 40_000) {
  const ac = new AbortController();
  const timer = setTimeout(() => ac.abort(), timeoutMs);
  try {
    const res = await fetch(`https://api.telegram.org/bot${token}/${method}`, {
      method: "POST",
      signal: ac.signal,
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload ?? {}),
    });
    const json = await res.json().catch(() => null);
    if (!res.ok || !json?.ok) {
      throw new Error(`telegram ${method} failed: HTTP ${res.status} ${JSON.stringify(json)?.slice(0, 200)}`);
    }
    return json.result;
  } finally {
    clearTimeout(timer);
  }
}

export async function health(cfg) {
  const token = secretFromEnv(cfg.token_env);
  if (!token) return { ok: false, detail: `${cfg.token_env} not set` };
  try {
    const me = await api(token, "getMe", {}, 15_000);
    return { ok: true, detail: `bot @${me.username}` };
  } catch (e) {
    return { ok: false, detail: e.message };
  }
}

/** Chat ids that have written to the bot recently — used by `hub doctor`. */
export async function observedChatIds(cfg) {
  const token = secretFromEnv(cfg.token_env);
  if (!token) return [];
  const updates = await api(token, "getUpdates", { timeout: 0, limit: 20 });
  const ids = new Set();
  for (const u of updates ?? []) {
    const chat = u.message?.chat ?? u.edited_message?.chat;
    if (chat?.id) ids.add(`${chat.id} (${chat.username ?? chat.title ?? chat.type})`);
  }
  return [...ids];
}

export async function poll({ cfg, cursors }) {
  const token = secretFromEnv(cfg.token_env);
  if (!token) throw new SkipAdapter(`${cfg.token_env} not set — telegram channel disabled`);

  const offset = cursors["telegram:offset"] ? Number(cursors["telegram:offset"]) : undefined;
  const allowed = (cfg.allowed_chat_ids ?? []).map(String);

  const updates = await api(
    token,
    "getUpdates",
    { timeout: cfg.poll_timeout_sec ?? 20, offset, allowed_updates: ["message"] },
    (cfg.poll_timeout_sec ?? 20) * 1000 + 15_000,
  );

  const messages = [];
  let maxUpdateId = offset ? offset - 1 : 0;

  for (const u of updates ?? []) {
    maxUpdateId = Math.max(maxUpdateId, u.update_id);
    const m = u.message;
    if (!m?.text) continue;
    const chatId = String(m.chat?.id);

    if (allowed.length && !allowed.includes(chatId)) {
      // Stranger messaging the bot: recorded, not ingested. Never silent.
      log.warn("telegram_chat_not_allowed", { chat_id: chatId, from: m.from?.username, text_len: m.text.length });
      continue;
    }

    messages.push({
      source: name,
      external_id: `tg:${u.update_id}`,
      thread_key: `telegram:${chatId}`,
      sender: `telegram:${m.from?.username ?? m.from?.id ?? chatId}`,
      // Only ids you put in allowed_chat_ids reach here, so this is you.
      sender_trust: allowed.includes(chatId) ? "trusted" : "untrusted",
      subject: m.text.split("\n")[0].slice(0, 120),
      body: m.text,
      url: null,
      received_at: new Date(m.date * 1000).toISOString(),
      raw: { stream: "telegram", chat_id: chatId, update_id: u.update_id, from: m.from?.username ?? null },
    });
  }

  // Telegram requires offset = last_update_id + 1 to acknowledge.
  const cursorsOut = updates?.length ? { "telegram:offset": String(maxUpdateId + 1) } : {};
  return { messages, cursors: cursorsOut };
}

/** target = chat id */
export async function send({ cfg, target, subject, body }) {
  const token = secretFromEnv(cfg.token_env);
  if (!token) throw new Error(`cannot send telegram: ${cfg.token_env} not set`);
  const text = subject ? `*${subject}*\n${body}` : body;
  const res = await api(token, "sendMessage", {
    chat_id: target,
    text: text.slice(0, 4000),
    parse_mode: "Markdown",
    disable_web_page_preview: true,
  });
  return { id: res?.message_id ?? null };
}
