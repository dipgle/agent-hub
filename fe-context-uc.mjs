// The complaint, reproduced and then checked: "phải nhắc quá nhiều thông tin
// trong nội dung chat để biết đang nói về dự án nào".
//
// Journey, entirely through the chat box:
//   1. pin the room to a project with /project tfl5 (and read it back),
//   2. ask a question that names NO project,
//   3. that question must land in the inbox tagged with tfl5 — before this
//      change every chat row had project = NULL,
//   4. unpin, and confirm hub says so.
//
// Steps 1 and 4 are commands (free). Step 2 is an ordinary question, which
// hub triages — it lands in the same room thread as any open question, so the
// coalesce window usually folds it in rather than paying for a second call.
//
// Usage: node fe-context-uc.mjs
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
const PROJECT = "tfl5";
const SHOTS = HERE + "ui-shots/";
mkdirSync(SHOTS, { recursive: true });

const checks = [];
const check = (name, ok, detail = "") => {
  checks.push({ name, ok });
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? ` — ${detail}` : ""}`);
};

// Read the store directly — this is the observation, not the action.
const projectOf = (needle) => {
  const out = execFileSync("sqlite3", [
    HERE + "data/hub.sqlite",
    `SELECT ifnull(project,'') FROM messages WHERE body LIKE '%${needle}%' ORDER BY id DESC LIMIT 1;`,
  ]).toString().trim();
  return out;
};

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1000, height: 900 } });
const errors = [];
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text().slice(0, 160)); });
page.on("pageerror", (e) => errors.push(`uncaught: ${e.message.slice(0, 160)}`));

const say = async (text) => {
  await page.fill("#text", text);
  await page.press("#text", "Enter");
};
const waitForHub = async (re, timeout = 180000) => {
  await page.waitForFunction(
    (src) => [...document.querySelectorAll(".msg .body")].some((e) => new RegExp(src).test(e.textContent)),
    re.source,
    { timeout, polling: 1000 }
  );
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

  await say(`/project ${PROJECT}`);
  await waitForHub(new RegExp(`mặc định thuộc dự án ${PROJECT}`));
  check("ghim được dự án cho phòng", true, `/project ${PROJECT}`);

  // The header must SAY which project the room is about — the other half of
  // the complaint was not knowing without asking.
  // The snapshot refreshes once per cycle, so keep asking rather than reading
  // one stale copy.
  const keepReloading = setInterval(() => page.click("#boardReload").catch(() => {}), 10000);
  // Wait for the PIN specifically (📌). Matching just the project name passes
  // immediately on the inherited-context form left by earlier messages, which
  // proves nothing about the pin that was just set.
  await page.waitForFunction(
    (p) => {
      const t = document.getElementById("ctxLabel").textContent;
      return t.includes("📌") && t.includes(p);
    },
    PROJECT,
    { timeout: 240000, polling: 5000 }
  );
  clearInterval(keepReloading);
  const ctx = await page.locator("#ctxLabel").innerText();
  check("header hiện dự án đang áp dụng", ctx.includes(PROJECT), ctx);
  check("phân biệt ghim với suy ra", /📌/.test(ctx), ctx);

  await say("/project");
  await waitForHub(new RegExp(`Đang ghim dự án: ${PROJECT}`));
  check("hỏi lại thì hub nói đang ghim gì", true);

  // The actual point: a question with NO project in it.
  const marker = `ngu canh ${Date.now()}`;
  await say(`câu hỏi không nhắc tên dự án — ${marker}`);
  await page.screenshot({ path: `${SHOTS}context-01-asked.png`, fullPage: true });

  let got = "";
  for (let i = 0; i < 40 && !got; i++) {
    await page.waitForTimeout(3000);
    got = projectOf(marker);
  }
  check("câu hỏi KHÔNG nêu dự án vẫn được gắn đúng dự án", got === PROJECT,
    got ? `project = ${got}` : "vẫn trống sau 2 phút");

  await say("/project -");
  await waitForHub(/Đã bỏ ghim dự án/);
  check("bỏ ghim được và hub xác nhận", true);
  await page.screenshot({ path: `${SHOTS}context-02-done.png`, fullPage: true });

  check("0 lỗi console", errors.length === 0, errors.join(" | "));
} catch (e) {
  check("kịch bản chạy trọn vẹn", false, e.message.split("\n").slice(0, 3).join(" | "));
} finally {
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra qua`);
process.exit(passed === checks.length ? 0 : 1);
