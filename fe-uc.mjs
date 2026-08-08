// Full user-journey acceptance for the hub chat.
//
// Everything a person does happens through the browser: log in, type a
// question, wait. Nothing here calls the hub API to fake a state. The reply is
// only accepted if it ARRIVES ON THE OPEN PAGE over the live socket — no
// reload, no polling by hand — because that is what the user experiences.
//
// The other half of the journey (triage → brief → owner approves) runs in the
// hub CLI while this script waits, exactly as it would in real life.
//
// Usage: node fe-uc.mjs <app_tid> <username> <password> "<question>"

import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { mkdirSync, writeFileSync } from "node:fs";

const [appTid, username, password, question] = process.argv.slice(2);
const BASE = `http://${appTid}.test.localhost:8090`;
const SHOTS = new URL("./ui-shots/", import.meta.url).pathname;
mkdirSync(SHOTS, { recursive: true });

const problems = [];
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 900, height: 800 } });
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

  // Count hub's messages BEFORE asking, so "a reply arrived" means a new one.
  const hubBefore = await page.evaluate(
    () => [...document.querySelectorAll(".msg .who span:first-child")].filter((e) => e.textContent === "hubbot").length
  );

  await page.fill("#text", question);
  await page.press("#text", "Enter");
  await page.waitForFunction(
    (t) => [...document.querySelectorAll(".msg .body")].some((e) => e.textContent === t),
    question,
    { timeout: 15000 }
  );
  console.log("ĐÃ GỬI — câu hỏi nằm trên màn, đang chờ hub trả lời trên chính trang này…");
  writeFileSync(new URL("./.tmp/uc-sent.flag", import.meta.url), "sent");
  await page.screenshot({ path: `${SHOTS}uc-01-asked.png`, fullPage: true });

  // The wait is the real one: ingest → silence window → triage → owner approve
  // → flush. Up to 8 minutes, and a timeout is a FAILURE, not a "probably fine".
  await page.waitForFunction(
    (n) => [...document.querySelectorAll(".msg .who span:first-child")].filter((e) => e.textContent === "hubbot").length > n,
    hubBefore,
    { timeout: 480000, polling: 1000 }
  );

  const reply = await page.evaluate(() => {
    const rows = [...document.querySelectorAll(".msg")].filter((m) => m.id !== "pending");
    const last = rows.filter((m) => m.querySelector(".who span:first-child").textContent === "hubbot").pop();
    return last ? last.querySelector(".body").textContent : null;
  });

  console.log("\n--- câu trả lời hub, đọc từ DOM của trang đang mở ---");
  console.log(reply);
  console.log("--- hết ---\n");
  await page.screenshot({ path: `${SHOTS}uc-02-answered.png`, fullPage: true });
  ok = reply && reply.length > 20;
  if (!ok) problems.push("hub trả lời nhưng nội dung rỗng hoặc quá ngắn");
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
console.log(ok ? "UC ĐẠT: câu trả lời tự hiện trên trang, không cần tải lại." : "UC KHÔNG ĐẠT");
process.exit(ok ? 0 : 1);
