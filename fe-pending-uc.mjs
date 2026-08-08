// The spinner must resolve.
//
// At tier L0 hub writes a DRAFT and waits for the owner, so no reply ever
// arrives on its own — "đang xử lý" was true for about a minute and a lie
// after that. This checks the page follows the question into hub's snapshot
// and ends in one of the honest states, with the approve button in reach.
//
// The question sent here is short and lands in the same room thread as the
// owner's open question, so hub COALESCES it instead of paying for a second
// triage call — the test costs nothing and still exercises the real path.
//
// Usage: node fe-pending-uc.mjs
import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { readFileSync, mkdirSync } from "node:fs";

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

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1000, height: 900 } });
const errors = [];
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text().slice(0, 160)); });
page.on("pageerror", (e) => errors.push(`uncaught: ${e.message.slice(0, 160)}`));

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

  const q = `kiểm tra trạng thái chờ duyệt ${Date.now()}`;
  await page.fill("#text", q);
  await page.press("#text", "Enter");
  check("gửi xong thì hiện trạng thái đang xử lý", await page.locator("#pending").isVisible());
  await page.screenshot({ path: `${SHOTS}pending-01-sent.png`, fullPage: true });

  // The whole point: it must not stay spinning. Either it resolves into the
  // approve prompt, or the spinner stops with an explanation.
  // NOTE the `null`: waitForFunction's second parameter is the ARGUMENT passed
  // into the page function — putting the options there silently falls back to
  // the 30s default, which is shorter than one poll interval here.
  await page.waitForFunction(
    () => !!document.getElementById("awaitNote") ||
          !document.getElementById("pending").classList.contains("on"),
    null,
    { timeout: 300000, polling: 3000 }
  );
  const spinning = await page.evaluate(() =>
    document.getElementById("pending").classList.contains("on"));
  check("KHÔNG còn quay vô hạn", !spinning);

  const awaitNote = page.locator("#awaitNote");
  if (await awaitNote.count()) {
    const text = await awaitNote.innerText();
    check("nói rõ hub đã soạn nháp và đang chờ duyệt", /chờ bạn duyệt/.test(text), text.split("\n")[0]);
    check("có nút Duyệt ngay trong khung chat",
      /Duyệt & gửi #\d+/.test(await awaitNote.locator("button").first().innerText()));
    check("có lối mở sang Bảng điều khiển",
      (await awaitNote.locator("button", { hasText: "Bảng điều khiển" }).count()) === 1);
  } else {
    // The other honest ending: spinner stopped with a note saying why.
    const notes = await page.evaluate(() =>
      [...document.querySelectorAll(".note")].map((n) => n.textContent).join(" | "));
    check("nếu không phải chờ duyệt thì có giải thích", /trạng thái|hộp việc/.test(notes), notes.slice(-120));
  }
  await page.screenshot({ path: `${SHOTS}pending-02-resolved.png`, fullPage: true });
  check("0 lỗi console", errors.length === 0, errors.join(" | "));
} catch (e) {
  check("kịch bản chạy trọn vẹn", false, e.message.split("\n").slice(0, 3).join(" | "));
} finally {
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra qua`);
process.exit(passed === checks.length ? 0 : 1);
