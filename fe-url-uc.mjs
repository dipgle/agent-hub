// Địa chỉ LÀ trạng thái: bấm tab đổi URL, F5 về đúng chỗ đang làm, Back đi
// ngược đúng thứ tự.
//
// Hà chốt 2026-08-09: *"kích vào tab giống như menu phải thay đổi url theo →
// tải lại trang không mất trạng thái đang làm việc"*. Trước đó tab nằm trong
// localStorage: URL đứng yên, Back nhảy ra khỏi app, gửi link cho người khác
// thì họ mở ra một màn khác.
//
// Mọi bước ở đây đi qua đúng thứ người dùng chạm: bấm tab, bấm phiên, F5, Back.
// Không `goto()` thẳng vào URL có sẵn để "giả lập" trạng thái — cái cần chứng
// minh chính là *trang tự ghi ra địa chỉ ấy*.
//
// Usage: node fe-url-uc.mjs <app_tid> <username> <password>

import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { mkdirSync } from "node:fs";

const [appTid, username, password] = process.argv.slice(2);
if (!appTid || !username || !password) {
  console.error("usage: node fe-url-uc.mjs <app_tid> <username> <password>");
  process.exit(2);
}
const BASE = `http://${appTid}.test.localhost:8090`;
const SHOTS = new URL("./ui-shots/", import.meta.url).pathname;
mkdirSync(SHOTS, { recursive: true });

const checks = [];
const problems = [];
const check = (name, ok, detail = "") => {
  checks.push({ name, ok });
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) problems.push(`${name}${detail ? `: ${detail}` : ""}`);
};

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({
  viewport: { width: 390, height: 844 },
  deviceScaleFactor: 3,
  isMobile: true,
  hasTouch: true,
});
page.on("console", (m) => { if (m.type() === "error") problems.push(`console.error: ${m.text()}`); });
page.on("pageerror", (e) => problems.push(`uncaught: ${e.message}`));

const tabOf = () => new URL(page.url()).searchParams.get("tab");
const sessOf = () => new URL(page.url()).searchParams.get("session");

try {
  await page.goto(BASE, { waitUntil: "domcontentloaded" });
  await page.fill("#u", username);
  await page.fill("#p", password);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 20000 });

  // ---- 1. bốn tab nằm trong header ---------------------------------------
  const inHeader = await page.evaluate(
    () => !!document.querySelector("header #panelTabs")
  );
  check("4 tab nằm trong header, không chiếm một hàng riêng của thân trang", inHeader);
  const tabBox = await page.evaluate(() => {
    const b = [...document.querySelectorAll("#panelTabs .tab")];
    return { n: b.length, minH: Math.min(...b.map((e) => Math.round(e.getBoundingClientRect().height))) };
  });
  check("đủ 4 tab", tabBox.n === 4, `${tabBox.n} tab`);
  // Chuẩn vùng chạm 44px — hàng tab cũ cao 35px, fe-phone-uc vẫn báo đỏ chỗ này.
  check("tab đủ cao để chạm bằng ngón (>=44px)", tabBox.minH >= 44, `${tabBox.minH}px`);

  await page.waitForFunction(() => document.querySelectorAll("#sessList .sess").length > 0, { timeout: 25000 });
  const firstTop = await page.evaluate(() =>
    Math.round(document.querySelector("#sessList .sess").getBoundingClientRect().top)
  );
  check("phiên đầu tiên lên cao hơn nhờ dọn chrome", firstTop < 300, `${firstTop}px`);
  await page.screenshot({ path: `${SHOTS}url-01-header-tabs.png` });

  // ---- 2. KHÔNG còn phiên của editor -------------------------------------
  const hosts = await page.evaluate(() =>
    [...document.querySelectorAll("#sessList .sess")].map((e) => e.dataset.host)
  );
  check("không còn phiên chạy trong editor trên màn", !hosts.includes("editor"), hosts.join(","));
  const summary = (await page.locator("#sessSummary").innerText()).trim();
  // Đổi ý có căn cứ (2026-08-09): dòng tổng kết KHÔNG được đếm phiên editor như
  // phiên đang sống, nhưng PHẢI nói ra là đã ẩn bao nhiêu. Bản đầu của phép đo
  // này đòi "thôi nhắc editor" — đòi đúng cái làm nên danh sách nói dối: lúc ấy
  // hub ẩn 8 phiên mỗi vòng và chỉ ghi vào log.
  check(
    "dòng tổng kết không đếm editor như phiên đang sống",
    !/—[^·]*trong editor/.test(summary),
    summary
  );
  // Con số "ẩn bao nhiêu" được đối chiếu với máy ở `fe-sessions-uc`, nơi đã có
  // sẵn `hub sessions --json`. Ở đây chỉ giữ phần thuộc về màn này: danh sách
  // không có dòng editor nào.

  // ---- 3. bấm tab thì địa chỉ đổi theo ------------------------------------
  check("mở trang xong địa chỉ đã mang tab", tabOf() === "sessions", page.url());
  await page.click('#panelTabs button[data-panel="health"]');
  await page.waitForSelector("#panel-health:not(.hidden)", { timeout: 5000 });
  check("bấm tab Sức khoẻ → URL đổi theo", tabOf() === "health", page.url());

  // F5 ở đây phải về lại đúng tab đó, không rơi về màn mặc định.
  await page.reload({ waitUntil: "domcontentloaded" });
  await page.waitForSelector("#panel-health:not(.hidden)", { timeout: 20000 });
  check("F5 vẫn ở tab Sức khoẻ", tabOf() === "health" &&
    (await page.locator('#panelTabs button[data-panel="health"].on').count()) === 1);

  // Back phải quay về tab trước, không nhảy ra khỏi app.
  await page.goBack({ waitUntil: "domcontentloaded" });
  await page.waitForSelector("#panel-sessions:not(.hidden)", { timeout: 20000 });
  check("Back quay lại tab Phiên, không rời app", tabOf() === "sessions" && page.url().startsWith(BASE));

  // ---- 4. phiên đang theo cũng nằm trong địa chỉ -------------------------
  await page.waitForFunction(() => document.querySelectorAll("#sessList .sess").length > 0, { timeout: 25000 });
  // Chọn phiên còn SỐNG: dòng đã dừng không mở ra luồng để đo.
  const live = await page.evaluate(() => {
    const el = document.querySelector('#sessList .sess:not([data-host="dead"])');
    return el ? { id: el.dataset.session, name: el.querySelector("strong")?.textContent } : null;
  });
  check("có phiên đang sống để mở", !!live, live ? live.name : "(không có)");
  if (live) {
    await page.locator(`.sess[data-session="${live.id}"]`).click();
    await page.waitForSelector("#sessDetail:not(.hidden)", { timeout: 10000 });
    check("mở phiên → URL mang id phiên", sessOf() === live.id, page.url());

    // Đây là câu hỏi của Hà, đo thẳng: F5 giữa lúc đang theo một phiên.
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForSelector("#sessDetail:not(.hidden)", { timeout: 25000 });
    const title = (await page.locator("#sessTitle").innerText()).trim();
    check(
      "F5 giữa chừng vẫn ở trong phiên đang làm",
      sessOf() === live.id && title === live.name,
      `${title} · ${page.url()}`
    );
    await page.screenshot({ path: `${SHOTS}url-02-reload-keeps-session.png` });

    // Back từ trong phiên phải về danh sách, không văng ra ngoài.
    await page.goBack({ waitUntil: "domcontentloaded" });
    await page.waitForSelector("#sessListPanel:not(.hidden)", { timeout: 20000 });
    check("Back từ trong phiên → về danh sách", !sessOf() && page.url().startsWith(BASE), page.url());
  }

  const over = await page.evaluate(() => ({ w: document.documentElement.scrollWidth, inner: window.innerWidth }));
  check("trang không tràn ngang", over.w <= over.inner + 1, `${over.w}/${over.inner}`);
} catch (e) {
  check("kịch bản chạy trọn vẹn", false, e.message.split("\n").slice(0, 3).join(" | "));
} finally {
  check("0 lỗi console", problems.filter((p) => /console|uncaught/.test(p)).length === 0);
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra qua`);
if (problems.length) {
  console.log("\nVẤN ĐỀ:");
  problems.forEach((p) => console.log("  - " + p));
}
process.exit(passed === checks.length ? 0 : 1);
