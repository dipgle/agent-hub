// Playwright smoke test for the hub chat FE, run against the bundle ACTUALLY
// DEPLOYED on tfl5 — not a local file, not a dev server. The page is fetched
// from `<app_tid>.test.localhost:8090`, which is the same path a real user
// takes.
//
// Everything here goes through the UI the way a person would: type into the
// login fields, press the button, type into the composer, press Enter. No
// direct API calls to reach a state — the point is to exercise the parts that
// only break in a browser (form validation, the socket, rendering).
//
// Fails on ANY console error or uncaught exception — the house rule.
//
// Usage:
//   node fe-smoke.mjs <app_tid> <username> <password>

import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { mkdirSync } from "node:fs";

const [appTid, username, password] = process.argv.slice(2);
if (!appTid || !username || !password) {
  console.error("usage: node fe-smoke.mjs <app_tid> <username> <password>");
  process.exit(2);
}
const BASE = `http://${appTid}.test.localhost:8090`;
const SHOTS = new URL("./ui-shots/", import.meta.url).pathname;
mkdirSync(SHOTS, { recursive: true });

const problems = [];
const checks = [];
const check = (name, ok, detail = "") => {
  checks.push({ name, ok, detail });
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) problems.push(`${name}${detail ? `: ${detail}` : ""}`);
};

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 900, height: 800 } });

page.on("console", (m) => {
  if (m.type() === "error") problems.push(`console.error: ${m.text()}`);
});
page.on("pageerror", (e) => problems.push(`uncaught: ${e.message}`));

try {
  // ---- 1. the page a user actually gets ---------------------------------
  const res = await page.goto(BASE, { waitUntil: "domcontentloaded" });
  check("trang tải được từ bundle đã deploy", res.status() === 200, `HTTP ${res.status()}`);
  check("tiêu đề đúng", (await page.title()) === "hub");
  check("màn đăng nhập hiện trước", await page.locator("#login").isVisible());
  check("chưa đăng nhập thì không có ô nhập tin", !(await page.locator("#foot").isVisible()));
  await page.screenshot({ path: `${SHOTS}fe-01-login.png` });

  // ---- 2. a wrong password must fail visibly, not silently --------------
  await page.fill("#u", username);
  await page.fill("#p", "sai-mat-khau-co-tinh");
  await page.click("#loginBtn");
  await page.waitForSelector("#loginErr:not(.hidden)", { timeout: 10000 });
  check("mật khẩu sai báo lỗi ngay trên màn", (await page.locator("#loginErr").innerText()).length > 0);
  check("đăng nhập hỏng thì vẫn ở màn đăng nhập", await page.locator("#login").isVisible());
  await page.screenshot({ path: `${SHOTS}fe-02-login-error.png` });

  // ---- 3. real login ----------------------------------------------------
  await page.fill("#p", password);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 15000 });
  // The conversation is its own tab now.
  await page.click('#panelTabs button[data-panel="chat"]');
  await page.waitForSelector("#foot:not(.hidden)", { timeout: 15000 });
  check("đăng nhập đúng thì vào phòng", await page.locator("#thread").isVisible());

  // ---- 4. the socket really opens --------------------------------------
  await page.waitForFunction(() => document.getElementById("status").dataset.state === "on", { timeout: 15000 });
  check("WebSocket mở thật", (await page.locator("#status").getAttribute("data-state")) === "on");

  // ---- 5. scrollback is there ------------------------------------------
  const before = await page.locator(".msg:not(#pending)").count();
  check("lịch sử hội thoại tải được", before > 0, `${before} tin`);
  await page.screenshot({ path: `${SHOTS}fe-03-thread.png`, fullPage: true });

  // ---- 6. send by typing + Enter, like a person -------------------------
  const text = `smoke ${Date.now()} — kiểm tra gửi tin qua giao diện thật`;
  await page.fill("#text", text);
  await page.press("#text", "Enter");

  await page.waitForFunction(
    (t) => [...document.querySelectorAll(".msg .body")].some((e) => e.textContent === t),
    text,
    { timeout: 15000 }
  );
  check("tin vừa gõ hiện trong luồng", true);
  check("ô nhập được dọn sau khi gửi", (await page.inputValue("#text")) === "");
  // KHÔNG còn ô "đang xử lý". Nó theo tin nhắn vào hộp việc để báo "đang phân
  // loại / chờ bạn duyệt" — hai trạng thái nay không tồn tại, mà một cái quay
  // mãi thì tệ hơn là không có gì (đã trả giá 08-07). Câu thường không được
  // trả lời, và màn phải nói đúng như vậy: im lặng.
  check("KHÔNG còn ô 'đang xử lý' quay vô tận", !(await page.locator("#pending").isVisible()));
  await page.screenshot({ path: `${SHOTS}fe-04-sent.png`, fullPage: true });

  // ---- 7. it survives a reload (persisted, not just painted) ------------
  await page.reload({ waitUntil: "domcontentloaded" });
  await page.waitForSelector("#foot:not(.hidden)", { timeout: 15000 });
  check("F5 xong vẫn còn đăng nhập (cookie phiên)", await page.locator("#thread").isVisible());
  await page.waitForFunction(
    (t) => [...document.querySelectorAll(".msg .body")].some((e) => e.textContent === t),
    text,
    { timeout: 15000 }
  );
  check("tin đã gửi vẫn còn sau khi tải lại — đã ghi thật, không chỉ vẽ ra", true);
  await page.screenshot({ path: `${SHOTS}fe-05-after-reload.png`, fullPage: true });

  // ---- 8. empty input must not create a phantom row --------------------
  const countBefore = await page.locator(".msg:not(#pending)").count();
  await page.fill("#text", "   ");
  await page.press("#text", "Enter");
  await page.waitForTimeout(1200);
  check("gửi khoảng trắng không tạo tin rỗng", (await page.locator(".msg:not(#pending)").count()) === countBefore);
} catch (e) {
  problems.push(`ngoại lệ: ${e.message}`);
} finally {
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra qua · ${problems.length} vấn đề`);
if (problems.length) {
  console.log("\nVẤN ĐỀ:");
  problems.forEach((p) => console.log(`  - ${p}`));
  process.exit(1);
}
console.log("ảnh chụp ở ui-shots/");
