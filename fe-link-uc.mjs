// "nội dung hộp việc và trao đổi là khác nhau à" — yes, and this pins the
// overlap so the two panels stop reading as unrelated lists.
//
// The room holds hub's own lines and slash commands (neither is work); the
// inbox holds GitHub and devlog items (never in the room). Where they DO meet
// is a question typed here, and that link must be walkable both ways:
//   1. select the inbox row → the message it came from is highlighted in the
//      conversation,
//   2. from that message, "#<id> trong hộp việc" selects the row again.
//
// Usage: node fe-link-uc.mjs
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
const page = await browser.newPage({ viewport: { width: 1500, height: 1000 } });
const errors = [];
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text().slice(0, 160)); });
page.on("pageerror", (e) => errors.push(`uncaught: ${e.message.slice(0, 160)}`));

try {
  await page.goto(`http://${APP_TID}.test.localhost:8090`, { waitUntil: "domcontentloaded" });
  await page.fill("#u", env.HUB_TFL5_OWNER_USER || "alice_local");
  await page.fill("#p", env.HUB_TFL5_ALICE_PASSWORD);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 15000 });
  await page.waitForFunction(
    () => !/đang tải/.test(document.getElementById("boardStamp").textContent),
    { timeout: 30000 }
  );
  await page.waitForTimeout(1500);

  // A row that came from THIS room — read the source column rather than
  // pattern-matching the whole row text.
  const idx = await page.evaluate(() =>
    [...document.querySelectorAll("#boardRows tr")]
      .findIndex((tr) => /^tfl5/.test(tr.children[2]?.textContent.trim() || "")));
  check("có mục bắt nguồn từ chính phòng chat", idx >= 0, `dòng thứ ${idx + 1}`);
  const chatRow = page.locator("#boardRows tr").nth(idx);
  const id = (await chatRow.locator("td").first().textContent()).trim().replace("#", "");
  await chatRow.click();

  const linked = await page.evaluate(() => {
    const el = document.querySelector("#thread .msg.linked");
    return el ? el.querySelector(".body")?.textContent.slice(0, 40) : null;
  });
  check("chọn mục hộp việc thì tin gốc trong Trao đổi được đánh dấu", !!linked, linked || "không thấy");

  // The tabs are separate on purpose, so the inbox offers a way over rather
  // than yanking the reader out of the list.
  check("chi tiết mời sang xem tin gốc", await page.locator("#linkHint").isVisible());
  await page.locator("#linkHint").click();
  check("bấm vào đó thì mở tab Trao đổi", await page.locator("#panel-chat").isVisible());
  await page.screenshot({ path: `${SHOTS}link-01-highlighted.png`, fullPage: true });

  // …and back again, from the conversation.
  const back = page.locator("#thread .msg.linked .msgtools button", { hasText: "hộp việc" });
  check("tin đó cho biết nó là mục nào", (await back.count()) === 1,
    (await back.count()) ? await back.innerText() : "không có nút");
  await back.click();
  await page.waitForTimeout(500);
  const selected = await page.evaluate(() => {
    const tr = document.querySelector("#boardRows tr.on");
    return tr ? tr.querySelector("td").textContent.trim() : null;
  });
  check("bấm vào đó thì mục tương ứng được chọn lại", selected === `#${id}`, `${selected} vs #${id}`);

  // The panels are NOT the same list: the room carries lines that are not work.
  const roomOnly = await page.evaluate(() =>
    [...document.querySelectorAll("#thread .msg")]
      .filter((el) => !el.querySelector('.msgtools button:nth-child(2)')).length);
  check("trong Trao đổi vẫn có dòng KHÔNG phải việc (hub nói, lệnh)", roomOnly > 0, `${roomOnly} dòng`);
  await page.screenshot({ path: `${SHOTS}link-02-back.png`, fullPage: true });
  check("0 lỗi console", errors.length === 0, errors.join(" | "));
} catch (e) {
  check("kịch bản chạy trọn vẹn", false, e.message.split("\n").slice(0, 3).join(" | "));
} finally {
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra qua`);
process.exit(passed === checks.length ? 0 : 1);
