// Sits on the open chat page and waits for hub's reply to arrive by itself.
//
// No reload, no manual fetch: if the message shows up, it came down the live
// socket — which is the only thing that proves a user would actually see it.
//
// Usage: node fe-watch.mjs <app_tid> <username> <password> [timeout_ms]

import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { mkdirSync, writeFileSync } from "node:fs";

const [appTid, username, password, timeoutRaw] = process.argv.slice(2);
const timeout = Number(timeoutRaw || 300000);
const BASE = `http://${appTid}.test.localhost:8090`;
const SHOTS = new URL("./ui-shots/", import.meta.url).pathname;
mkdirSync(SHOTS, { recursive: true });

const problems = [];
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 900, height: 820 } });
page.on("console", (m) => { if (m.type() === "error") problems.push(`console.error: ${m.text()}`); });
page.on("pageerror", (e) => problems.push(`uncaught: ${e.message}`));

let ok = false;
try {
  await page.goto(BASE, { waitUntil: "domcontentloaded" });
  await page.fill("#u", username);
  await page.fill("#p", password);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 15000 });
  // The conversation is its own tab now.
  await page.click('#panelTabs button[data-panel="chat"]');
  await page.waitForSelector("#foot:not(.hidden)", { timeout: 15000 });
  await page.waitForFunction(() => document.getElementById("status").dataset.state === "on", { timeout: 15000 });

  const before = await page.evaluate(
    () => [...document.querySelectorAll(".msg .who span:first-child")].filter((e) => e.textContent === "hubbot").length
  );
  console.log(`trang đang mở, socket mở, hiện có ${before} tin từ hub — chờ tin mới…`);
  writeFileSync(new URL("./.tmp/watch-ready.flag", import.meta.url), "ready");

  await page.waitForFunction(
    (n) => [...document.querySelectorAll(".msg .who span:first-child")].filter((e) => e.textContent === "hubbot").length > n,
    before,
    { timeout, polling: 500 }
  );

  const reply = await page.evaluate(() => {
    const rows = [...document.querySelectorAll(".msg")].filter((m) => m.id !== "pending");
    const last = rows.filter((m) => m.querySelector(".who span:first-child").textContent === "hubbot").pop();
    return last ? last.querySelector(".body").textContent : null;
  });
  console.log("\n--- tin mới, đọc từ DOM của trang KHÔNG tải lại ---");
  console.log(reply);
  console.log("--- hết ---");
  await page.screenshot({ path: `${SHOTS}uc-live-reply.png`, fullPage: true });
  ok = !!reply && reply.length > 20;
  if (!ok) problems.push("tin đến nhưng nội dung rỗng/quá ngắn");
} catch (e) {
  problems.push(`ngoại lệ: ${e.message}`);
} finally {
  await browser.close();
}

if (problems.length) {
  console.log("VẤN ĐỀ:");
  problems.forEach((p) => console.log(`  - ${p}`));
  process.exit(1);
}
console.log("ĐẠT: câu trả lời tự hiện trên trang đang mở, không tải lại.");
