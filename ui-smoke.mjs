// Playwright smoke test for the hub web UI.
//
// Loads every tab, clicks the real controls, and fails on ANY console error or
// uncaught exception — the house rule for UI work in this workspace.
//
// Usage (server must already be running):
//   ./hub web --port 9247 &
//   node ui-smoke.mjs http://127.0.0.1:9247
//
// Playwright is borrowed from a sibling project that already has it installed;
// nothing is added to hub's own dependencies.

import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { mkdirSync } from "node:fs";

const BASE = process.argv[2] || "http://127.0.0.1:9247";
const SHOTS = new URL("./ui-shots/", import.meta.url).pathname;
mkdirSync(SHOTS, { recursive: true });

const problems = [];
let passed = 0;

function check(name, cond, detail = "") {
  if (cond) {
    passed += 1;
    console.log(`  ✓ ${name}`);
  } else {
    problems.push(`${name} ${detail}`.trim());
    console.log(`  ✗ ${name} ${detail}`);
  }
}

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });

const consoleErrors = [];
page.on("console", (m) => {
  if (m.type() === "error") consoleErrors.push(m.text());
});
page.on("pageerror", (e) => consoleErrors.push(`uncaught: ${e.message}`));
page.on("requestfailed", (r) => consoleErrors.push(`request failed: ${r.url()} ${r.failure()?.errorText}`));

try {
  console.log(`hub UI smoke → ${BASE}`);

  // ── inbox ──
  await page.goto(BASE, { waitUntil: "networkidle" });
  await page.waitForSelector("#inbox-body tr", { timeout: 15000 });
  const rows = await page.locator("#inbox-body tr.row").count();
  check("inbox lists messages", rows > 0, `(rows=${rows})`);
  const counts = await page.locator("#counts").textContent();
  check("header shows counts", /\d/.test(counts), `("${counts}")`);
  const spend = await page.locator("#spend").textContent();
  check("header shows spend", /\$\d/.test(spend), `("${spend}")`);
  await page.screenshot({ path: `${SHOTS}01-inbox.png`, fullPage: false });

  // ── detail ──
  await page.locator("#inbox-body tr.row").first().click();
  await page.waitForSelector("#detail pre", { timeout: 15000 });
  const detail = await page.locator("#detail").textContent();
  check("detail panel renders the message body", detail.includes("Message #"), "");
  const hasDraftOrNote = (await page.locator("#draft").count()) > 0 || detail.includes("Chưa có decision");
  check("detail shows a reply draft or says there is none", hasDraftOrNote);
  await page.screenshot({ path: `${SHOTS}02-detail.png` });

  // ── config ──
  await page.locator('nav button[data-tab="config"]').click();
  await page.waitForSelector("#c-model", { timeout: 15000 });
  const model = await page.locator("#c-model").inputValue();
  check("config form is populated from the live config", model.length > 0, `(model="${model}")`);
  const tier = await page.locator("#c-tier").inputValue();
  check("autonomy tier is one of L0/L1/L2", ["L0", "L1", "L2"].includes(tier), `(tier=${tier})`);
  const raw = await page.locator("#c-raw").inputValue();
  let rawOk = false;
  try {
    const parsed = JSON.parse(raw);
    rawOk = !!parsed.adapters && !!parsed.triage;
  } catch {}
  check("raw JSON mirror is valid config JSON", rawOk);
  check("no credential value is exposed in the form", !/sk-|gho_|\d{8,10}:[A-Za-z0-9_-]{30,}/.test(raw));
  await page.screenshot({ path: `${SHOTS}03-config.png`, fullPage: true });

  // config round-trip: flip coalesce_hours and save, then read it back
  const before = await page.locator("#c-coalesce").inputValue();
  const probe = String((parseInt(before, 10) || 12) === 11 ? 12 : 11);
  await page.locator("#c-coalesce").fill(probe);
  await page.locator("#btn-save-cfg").click();
  await page.waitForSelector("#cfg-msg.ok", { timeout: 15000 });
  await page.reload({ waitUntil: "networkidle" });
  await page.locator('nav button[data-tab="config"]').click();
  await page.waitForSelector("#c-coalesce", { timeout: 15000 });
  const after = await page.locator("#c-coalesce").inputValue();
  check("config save persists to disk and reloads", after === probe, `(wrote ${probe}, read ${after})`);
  // put it back
  await page.locator("#c-coalesce").fill(before);
  await page.locator("#btn-save-cfg").click();
  await page.waitForSelector("#cfg-msg.ok", { timeout: 15000 });

  // ── health ──
  await page.locator('nav button[data-tab="health"]').click();
  await page.locator("#btn-doctor").click();
  await page.waitForSelector("#health-body table", { timeout: 60000 });
  const health = await page.locator("#health-body").textContent();
  check("doctor probes real channels", health.includes("github") && health.includes("claude"));
  await page.screenshot({ path: `${SHOTS}04-health.png`, fullPage: true });

  // ── cost (ECharts) ──
  await page.locator('nav button[data-tab="cost"]').click();
  await page.waitForTimeout(1200);
  const canvases = await page.locator("#chart-cost canvas, #chart-status canvas").count();
  check("ECharts rendered both charts", canvases >= 2, `(canvas=${canvases})`);
  await page.screenshot({ path: `${SHOTS}05-cost.png`, fullPage: true });

  // ── auth ──
  // Probed through a bare request context, not the page: a deliberate 401 in
  // the page would show up as a console error and mask real ones.
  const unauth = await page.request.get(`${BASE}/api/inbox`, { failOnStatusCode: false });
  check("API rejects a call without the token", unauth.status() === 401, `(status=${unauth.status()})`);

  check("no console errors", consoleErrors.length === 0, consoleErrors.join(" | "));
} catch (e) {
  problems.push(`threw: ${e.message}`);
  console.log(`  ✗ threw: ${e.message}`);
  await page.screenshot({ path: `${SHOTS}99-failure.png` }).catch(() => {});
} finally {
  await browser.close();
}

console.log(`\n${passed} passed, ${problems.length} failed`);
if (consoleErrors.length) console.log("console errors:\n  " + consoleErrors.join("\n  "));
console.log(`screenshots → ${SHOTS}`);
process.exit(problems.length ? 1 : 0);
