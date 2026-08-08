// The config form must actually WRITE, and write the right thing.
//
// Journey: open the board's Cấu hình tab, change one number, press Lưu, watch
// hub confirm in the room, then reload and see the new value come back from
// hub's own snapshot. Finally put the original value back — a test that leaves
// the daemon differently configured is a test that broke production.
//
// The field chosen (`poll_interval_sec`) costs nothing to flip and changes no
// behaviour at the value used. It used to be `max_triage_per_cycle`, which went
// with the inbox on 2026-08-08 — a test pointed at a deleted knob fails for the
// wrong reason and teaches nothing.
//
// Usage: node fe-config-uc.mjs
import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { readFileSync, mkdirSync } from "node:fs";

const HERE = new URL("./", import.meta.url).pathname;
const env = Object.fromEntries(
  readFileSync(HERE + "hub.env", "utf8")
    .split("\n").filter((l) => l.includes("=") && !l.trim().startsWith("#"))
    .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim().replace(/^["']|["']$/g, "")])
);
const APP_TID = env.HUB_TFL5_APP_TID || "a-65dd60d3-624e-45a9-8fdf-62aa7d894d80";
const KEY = "poll_interval_sec";
const SHOTS = HERE + "ui-shots/";
mkdirSync(SHOTS, { recursive: true });

const checks = [];
const check = (name, ok, detail = "") => {
  checks.push({ name, ok });
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? ` — ${detail}` : ""}`);
};
const onDisk = () => JSON.parse(readFileSync(HERE + "hub.config.json", "utf8"))[KEY];

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1400, height: 1000 } });
const errors = [];
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text().slice(0, 160)); });
page.on("pageerror", (e) => errors.push(`uncaught: ${e.message.slice(0, 160)}`));

const original = onDisk();
let restored = false;
try {
  await page.goto(`http://${APP_TID}.test.localhost:8090`, { waitUntil: "domcontentloaded" });
  await page.fill("#u", env.HUB_TFL5_OWNER_USER || "alice_local");
  await page.fill("#p", env.HUB_TFL5_ALICE_PASSWORD);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 15000 });
  // The conversation is its own tab now.
  await page.click('#panelTabs button[data-panel="chat"]');
  await page.waitForSelector("#foot:not(.hidden)", { timeout: 15000 });
  await page.waitForFunction(
    () => !/đang tải/.test(document.getElementById("boardStamp").textContent),
    { timeout: 20000 }
  );
  await page.click('#panelTabs button[data-panel="config"]');
  await page.waitForSelector("#panel-config:not(.hidden)", { timeout: 5000 });

  const field = page.locator(`[data-cfg-key="${KEY}"]`);
  check("form dựng được từ cấu hình thật", (await field.count()) === 1, `${KEY} có mặt`);
  // The groups that still exist: the one channel, the loop, and who may give
  // orders. "≥ 4 kênh" and the triage/act/autonomy boxes were the shape of the
  // inbox — a measurement that demands the product be wrong never goes green.
  check("form có đủ các nhóm còn thật",
    (await page.locator("#cfgChannels .switch").count()) >= 1 &&
    /tfl5/.test(await page.locator("#cfgChannels").innerText()) &&
    (await page.locator("#cfgLoop [data-cfg-key]").count()) >= 2 &&
    (await page.locator("#cfgTrust [data-cfg-key]").count()) >= 1);
  await page.screenshot({ path: `${SHOTS}config-01.png`, fullPage: true });

  // Nothing touched → nothing sent. A form that fires writes on every press
  // would fill the room with noise.
  await page.click("#cfgSave");
  check("không sửa gì thì không gửi lệnh nào",
    /Không có gì thay đổi/.test(await page.locator("#cfgMsg").innerText()));

  const target = String(Number(original) + 1);
  // Count matching acks BEFORE pressing: the room keeps history, and an
  // identical line from an earlier run matched instantly — twice — making the
  // disk check race a write that had not happened yet.
  const ackCount = () =>
    page.evaluate(
      ([k, v]) => [...document.querySelectorAll(".msg .body")]
        .filter((e) => e.textContent.includes(`đã đặt ${k} = ${v}`)).length,
      [KEY, target]
    );
  const acksBefore = await ackCount();
  await page.click('#panelTabs button[data-panel="config"]');
  await field.fill(target);
  await page.click("#cfgSave");
  check("báo rõ đã gửi thay đổi nào", new RegExp(KEY).test(await page.locator("#cfgMsg").innerText()));

  // hub confirms in the room…
  // Match the VALUE too: an older "đã đặt <key> = …" from a previous run is
  // already in the scrollback, and keying on the name alone matched it
  // instantly — then the disk check ran before this write had happened.
  await page.waitForFunction(
    ([k, v, n]) => [...document.querySelectorAll(".msg .body")]
      .filter((e) => e.textContent.includes(`đã đặt ${k} = ${v}`)).length > n,
    [KEY, target, acksBefore],
    { timeout: 120000, polling: 1000 }
  );
  check("hub xác nhận trong phòng chat", true, `đã đặt ${KEY} = ${target}`);

  // …and the file on disk really changed.
  check("tệp cấu hình trên đĩa đã đổi", String(onDisk()) === target, `${original} → ${onDisk()}`);
  await page.screenshot({ path: `${SHOTS}config-02-saved.png`, fullPage: true });
} catch (e) {
  check("kịch bản chạy trọn vẹn", false, e.message.split("\n").slice(0, 3).join(" | "));
} finally {
  // Always put the original value back, through the same path.
  try {
    if (String(onDisk()) !== String(original)) {
      // Restore by typing the command into the chat box — the same thing a
      // person can do, and it does not wait on the snapshot the form is drawn
      // from (that refreshes once per cycle, so the form would still show the
      // old number and "no change" would be sent).
      //
      // The composer lives INSIDE the conversation panel, and we are standing
      // on the config panel — filling a hidden textarea just waits 30s and
      // dies, leaving the config file on the test's value. Go to the tab a
      // person would go to first.
      await page.click('#panelTabs button[data-panel="chat"]');
      await page.waitForSelector("#foot:not(.hidden)", { timeout: 15000 });
      await page.waitForFunction(
        () => document.getElementById("status").dataset.state === "on",
        { timeout: 30000 }
      );
      await page.fill("#text", `/set ${KEY} ${original}`);
      await page.press("#text", "Enter");
      for (let i = 0; i < 30 && String(onDisk()) !== String(original); i++) {
        await page.waitForTimeout(3000);
      }
    }
    restored = String(onDisk()) === String(original);
    check("đã trả cấu hình về nguyên trạng", restored, `${KEY} = ${onDisk()}`);
  } catch (e) {
    check("đã trả cấu hình về nguyên trạng", false, e.message.split("\n")[0]);
  }
  check("0 lỗi console", errors.length === 0, errors.join(" | "));
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra qua`);
if (!restored) console.log(`⚠ KIỂM TRA TAY: hub.config.json → ${KEY} phải là ${original}`);
process.exit(passed === checks.length ? 0 : 1);
