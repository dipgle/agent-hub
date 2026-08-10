// Kính lúp: mở trang thật, đăng nhập, rồi soi bất cứ thứ gì — không phải viết
// một kịch bản mới cho mỗi câu hỏi.
//
// Vì sao có tệp này (Hà 2026-08-10: *"tự mở playwright lên"*): mọi `fe-*-uc.mjs`
// đều là NGHIỆM THU — chúng khẳng định một điều đã biết trước. Còn lúc đang tìm
// hiểu thì câu hỏi đổi mỗi phút ("hàng ấy có hiện không", "chữ có bị cắt không",
// "console có kêu gì không"), và viết một kịch bản 100 dòng cho mỗi câu là lý do
// người ta thôi nhìn — rồi quay lại đoán. Cái này để HỎI, không để khẳng định.
//
//   node fe-probe.mjs                      # chụp màn Phiên
//   node fe-probe.mjs --tab health         # chụp một tab khác
//   node fe-probe.mjs --session <id>       # mở màn chi tiết một phiên
//   node fe-probe.mjs --eval "document.querySelectorAll('.sess').length"
//   HEADED=1 node fe-probe.mjs             # mở cửa sổ thật để nhìn tận mắt
//
// Ảnh luôn ghi ra `ui-shots/probe.png`. Console error của trang luôn được in —
// im lặng ở đây là bỏ sót đúng thứ hay hỏng nhất.
import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { readFileSync, mkdirSync } from "node:fs";

const HERE = new URL("./", import.meta.url).pathname;
const env = Object.fromEntries(
  readFileSync(HERE + "hub.env", "utf8")
    .split("\n").filter((l) => l.includes("=") && !l.trim().startsWith("#"))
    .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim().replace(/^["']|["']$/g, "")])
);
const APP_TID = env.HUB_TFL5_APP_TID || "a-65dd60d3-624e-45a9-8fdf-62aa7d894d80";
mkdirSync(HERE + "ui-shots", { recursive: true });

const arg = (name, dflt = null) => {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : dflt;
};
const tab = arg("tab", "sessions");
const session = arg("session");
const evalCode = arg("eval");
const full = process.argv.includes("--full");

const errors = [];
const browser = await chromium.launch({ headless: !process.env.HEADED });
const page = await browser.newPage({
  viewport: { width: 390, height: 844 },
  deviceScaleFactor: 3,
  isMobile: true,
  hasTouch: true,
});
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
page.on("pageerror", (e) => errors.push(`uncaught: ${e.message}`));

try {
  await page.goto(`http://${APP_TID}.test.localhost:8090`, { waitUntil: "domcontentloaded" });
  await page.fill("#u", "alice_local");
  await page.fill("#p", env.HUB_TFL5_ALICE_PASSWORD);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 20000 });
  await page.waitForFunction(
    () => !/đang tải/.test(document.getElementById("boardStamp")?.textContent || ""),
    null,
    { timeout: 25000 }
  ).catch(() => {});

  if (tab !== "sessions") {
    await page.click(`#panelTabs button[data-panel="${tab}"]`);
    await page.waitForTimeout(600);
  }

  // Mở một phiên bằng đúng cú CHẠM người dùng làm, không `goto` thẳng.
  if (session) {
    await page.waitForSelector("#sessList .sess", { timeout: 25000 });
    const sel = `#sessList .sess[data-session^="${session}"]`;
    if (await page.locator(sel).count()) {
      await page.locator(sel).first().click();
      await page.waitForSelector("#sessDetail:not(.hidden)", { timeout: 20000 });
      // Lý lịch phiên gấp trong `<details>`; mở nếu đang đóng.
      const open = await page.evaluate(() => document.getElementById("sessInfoBox")?.open === true);
      if (!open) await page.click("#sessInfoBox summary");
      await page.waitForTimeout(500);
    } else {
      console.log(`(không thấy thẻ phiên bắt đầu bằng "${session}")`);
    }
  }

  if (evalCode) {
    const out = await page.evaluate(evalCode);
    console.log(typeof out === "string" ? out : JSON.stringify(out, null, 2));
  }

  const shot = `${HERE}ui-shots/probe.png`;
  await page.screenshot({ path: shot, fullPage: full });
  console.log(`ảnh: ${shot}`);
} catch (e) {
  console.error(`hỏng: ${e.message}`);
} finally {
  console.log(errors.length ? `\nconsole error (${errors.length}):\n  ${errors.join("\n  ")}` : "\n0 lỗi console");
  if (process.env.HEADED) {
    console.log("HEADED=1 — cửa sổ mở 60 giây để nhìn.");
    await page.waitForTimeout(60000);
  }
  await browser.close();
}
