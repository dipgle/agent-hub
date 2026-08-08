// The end-to-end proof for the action buttons: a click on the board must
// really change state, not just put text on a socket.
//
// Journey, all of it through the UI a person uses:
//   1. log in on the deployed bundle,
//   2. open the board, pick an item that is safe to close (a `coalesced` row —
//      it was already folded into another item's decision, so closing it sends
//      nothing to anybody),
//   3. press "Đóng",
//   4. watch hub's acknowledgement arrive in the chat room,
//   5. reload the board and confirm the row now reads `closed`.
//
// Step 5 is the one that matters: the snapshot comes from hub's own store, so
// it can only say `closed` if the command really ran.
//
// Usage: node fe-command-uc.mjs
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
const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
const errors = [];
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text().slice(0, 200)); });
page.on("pageerror", (e) => errors.push(`uncaught: ${e.message.slice(0, 200)}`));

try {
  await page.goto(`http://${APP_TID}.test.localhost:8090`, { waitUntil: "domcontentloaded" });
  // Log in as an OWNER account, not as hub itself: hub filters out messages
  // from its own user (`select_new`, so it never triages its own words), which
  // means a command typed by `hubbot` can never reach `parse_command`. The
  // owner list is `trust.tfl5_user_tids`.
  await page.fill("#u", env.HUB_TFL5_OWNER_USER || "alice_local");
  await page.fill("#p", env.HUB_TFL5_ALICE_PASSWORD);
  await page.click("#loginBtn");
  // This journey works the inbox; the conversation is read from the DOM, which
  // is populated whether or not its tab is on screen.
  await page.waitForSelector("#panelTabs", { timeout: 15000 });
  await page.click('#panelTabs button[data-panel="inbox"]');
  await page.waitForFunction(
    () => !/đang tải/.test(document.getElementById("boardStamp").textContent),
    { timeout: 20000 }
  );

  // A coalesced row: nothing is queued on it, so closing it cannot send mail
  // or a comment anywhere. Also skip anything already closed.
  const countCommandRows = () =>
    page.evaluate(() =>
      [...document.querySelectorAll("#boardRows tr")].filter((r) => /\/close /.test(r.textContent)).length
    );
  const commandRowsBefore = await countCommandRows();

  const row = page.locator("#boardRows tr", { hasText: "coalesced" }).first();
  const id = (await row.locator("td").first().textContent()).trim().replace("#", "");
  check("chọn được một mục an toàn để đóng (coalesced)", !!id, `#${id}`);
  await row.click();

  await page.locator("#boardDetail .reason").fill("nghiệm thu nút Đóng trên bảng điều khiển");
  await page.locator("#boardDetail .actions button", { hasText: "Đóng" }).first().click();
  check("bảng báo đã gửi lệnh", /Đã gửi/.test(await page.locator("#cmdStatus").innerText()));
  await page.screenshot({ path: `${SHOTS}cmd-01-sent.png`, fullPage: true });

  // hub answers in the room — the same place a person would look.
  await page.waitForFunction(
    (needle) => [...document.querySelectorAll(".msg .body")].some((e) => e.textContent.includes(needle)),
    `closed message #${id}`,
    { timeout: 90000, polling: 1000 }
  );
  check("hub trả lời trong phòng chat", true, `closed message #${id}`);
  await page.screenshot({ path: `${SHOTS}cmd-02-ack.png`, fullPage: true });

  // And the state really moved: reload the board from hub's own snapshot.
  // The board reads a snapshot hub pushes once per cycle, so pressing reload
  // once and waiting proves nothing — keep asking until the new snapshot
  // lands (or give up loudly).
  const rowState = async () =>
    page.evaluate(
      (wanted) => {
        const tr = [...document.querySelectorAll("#boardRows tr")].find(
          (r) => r.querySelector("td")?.textContent.trim() === `#${wanted}`
        );
        return tr ? tr.textContent.replace(/\s+/g, " ").trim() : null;
      },
      id
    );
  let closed = false;
  for (let i = 0; i < 15 && !closed; i++) {
    await page.click("#boardReload");
    await page.waitForTimeout(8000);
    closed = /closed/.test((await rowState()) || "");
  }
  check("ảnh chụp mới cho thấy mục đã chuyển sang closed", closed, (await rowState()) || "không thấy dòng");
  await page.screenshot({ path: `${SHOTS}cmd-03-closed.png`, fullPage: true });

  // The command must NOT come back as a new inbox item: that is how the live
  // socket used to charge $0.18 for classifying the word "close". Compare
  // against the count taken before the click — older rows from before the fix
  // are history, not a regression.
  const commandRowsAfter = await countCommandRows();
  check("lệnh không quay lại thành mục trong hộp việc (không tốn tiền triage)",
    commandRowsAfter === commandRowsBefore,
    `trước ${commandRowsBefore} → sau ${commandRowsAfter} dòng chứa "/close"`);

  check("0 lỗi console", errors.length === 0, errors.join(" | "));
} catch (e) {
  check("kịch bản chạy trọn vẹn", false, e.message.split("\n").slice(0, 4).join(" | "));
} finally {
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra qua`);
process.exit(passed === checks.length ? 0 : 1);
