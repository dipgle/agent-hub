// "sao không phải trả lời tin nào thì gắn được" — this checks exactly that,
// with NO command typed anywhere.
//
// Journey:
//   1. a line that names its project the ordinary way ("tfl5: …") lands with
//      project = tfl5,
//   2. press ↩ Trả lời on it and ask something that names NO project,
//   3. the new question must inherit tfl5 — because of the reply, not because
//      of a pin (the room is deliberately left unpinned).
//
// Usage: node fe-reply-uc.mjs
import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { readFileSync, mkdirSync } from "node:fs";
import { execFileSync } from "node:child_process";

const HERE = new URL("./", import.meta.url).pathname;
const env = Object.fromEntries(
  readFileSync(HERE + "hub.env", "utf8")
    .split("\n").filter((l) => l.includes("=") && !l.trim().startsWith("#"))
    .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim().replace(/^["']|["']$/g, "")])
);
const APP_TID = env.HUB_TFL5_APP_TID || "a-65dd60d3-624e-45a9-8fdf-62aa7d894d80";
const SHOTS = HERE + "ui-shots/";
mkdirSync(SHOTS, { recursive: true });

const checks = [];
const check = (name, ok, detail = "") => {
  checks.push({ name, ok });
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? ` — ${detail}` : ""}`);
};
const sql = (q) => execFileSync("sqlite3", [HERE + "data/hub.sqlite", q]).toString().trim();
const projectOf = (needle) =>
  sql(`SELECT ifnull(project,'') FROM messages WHERE body LIKE '%${needle}%' ORDER BY id DESC LIMIT 1;`);

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1000, height: 900 } });
const errors = [];
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text().slice(0, 160)); });
page.on("pageerror", (e) => errors.push(`uncaught: ${e.message.slice(0, 160)}`));

const say = async (text) => {
  await page.fill("#text", text);
  await page.press("#text", "Enter");
};
const waitProject = async (needle, want, tries = 40) => {
  for (let i = 0; i < tries; i++) {
    if (projectOf(needle) === want) return true;
    await page.waitForTimeout(3000);
  }
  return false;
};

try {
  await page.goto(`http://${APP_TID}.test.localhost:8090`, { waitUntil: "domcontentloaded" });
  await page.fill("#u", env.HUB_TFL5_OWNER_USER || "alice_local");
  await page.fill("#p", env.HUB_TFL5_ALICE_PASSWORD);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 15000 });
  // The conversation is its own tab now.
  await page.click('#panelTabs button[data-panel="chat"]');
  await page.waitForSelector("#foot:not(.hidden)", { timeout: 15000 });
  await page.waitForFunction(() => document.getElementById("status").dataset.state === "on", { timeout: 15000 });

  // Make sure nothing is pinned, so a pass can only come from the reply.
  await say("/project -");
  await page.waitForTimeout(4000);

  const stamp = Date.now();
  const first = `tfl5: mốc gốc ${stamp}`;
  await say(first);
  check("câu gốc (có nêu dự án) được gắn tfl5",
    await waitProject(`mốc gốc ${stamp}`, "tfl5"), projectOf(`mốc gốc ${stamp}`) || "trống");

  // Press ↩ on that message — no command typed.
  const row = page.locator(".msg", { hasText: `mốc gốc ${stamp}` }).first();
  await row.hover();
  await row.locator(".replybtn").click();
  check("bấm ↩ thì hiện thanh 'đang trả lời'", await page.locator("#replyBar").isVisible());
  check("thanh đó cho biết đang trả lời tin nào",
    (await page.locator("#replyWhat").innerText()).includes(`mốc gốc ${stamp}`));
  await page.screenshot({ path: `${SHOTS}reply-01-armed.png`, fullPage: true });

  const followUp = `câu tiếp không nêu dự án ${stamp}`;
  await say(followUp);
  check("gửi xong thì thanh trả lời tự tắt", !(await page.locator("#replyBar").isVisible()));
  // Wait for OUR message to come back down the socket, then read that block —
  // ".last()" on the whole page raced the round trip and read an older row.
  const sent = page.locator(".msg", { hasText: followUp }).last();
  await sent.waitFor({ timeout: 30000 });
  check("luồng chat hiện trích dẫn tin được trả lời",
    (await sent.locator(".quote").innerText()).includes(`mốc gốc ${stamp}`));
  check("KHÔNG lộ mã kỹ thuật ↩[…] trong nội dung tin",
    !(await sent.locator(".body").innerText()).includes("↩["));

  check("câu trả lời kế thừa dự án của tin được trả lời",
    await waitProject(followUp, "tfl5"), projectOf(followUp) || "trống");
  await page.screenshot({ path: `${SHOTS}reply-02-sent.png`, fullPage: true });
  check("0 lỗi console", errors.length === 0, errors.join(" | "));
} catch (e) {
  check("kịch bản chạy trọn vẹn", false, e.message.split("\n").slice(0, 3).join(" | "));
} finally {
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra qua`);
process.exit(passed === checks.length ? 0 : 1);
