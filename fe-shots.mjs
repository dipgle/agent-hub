// Chụp cả năm màn của bản ĐANG PHỤC VỤ, để còn nhìn bằng mắt.
//
// Vì sao có tệp này: các kịch bản `fe-*-uc.mjs` trả lời "đúng chưa", không trả
// lời "trông thế nào". Hai lỗi chỉ lòi ra khi nhìn ảnh chứ không kịch bản nào
// bắt được (2026-08-09): cột `adapter` chết hiện `—` ở mọi dòng, và hàng lệnh
// trong luồng phiên in nguyên tham số JSON. Cả hai đều "0 lỗi console".
//
// Usage: node fe-shots.mjs <app_tid> <user> <pass> [tag]
//   → ui-shots/<tag>-{sessions,chat,health,config,detail}.png
//
// Chỉ ĐỌC: đăng nhập, bấm tab, mở phiên. Không gõ lệnh, không gọi `claude` —
// nên chạy bao nhiêu lần cũng không tiêu hạn mức.

import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { mkdirSync } from "node:fs";

const [app, user, pass, tag = "shot"] = process.argv.slice(2);
if (!app || !user || !pass) {
  console.error("usage: node fe-shots.mjs <app_tid> <user> <pass> [tag]");
  process.exit(2);
}
const OUT = new URL("./ui-shots/", import.meta.url).pathname;
mkdirSync(OUT, { recursive: true });

const browser = await chromium.launch({ headless: true });
// Đúng khung máy Hà cầm: 390×844. Ảnh chụp ở 1280px không nói được gì về màn
// duy nhất sản phẩm này chạy trên đó.
const page = await browser.newPage({
  viewport: { width: 390, height: 844 },
  deviceScaleFactor: 2,
  isMobile: true,
  hasTouch: true,
});
const errors = [];
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text().slice(0, 160)); });
page.on("pageerror", (e) => errors.push(`uncaught: ${e.message.slice(0, 160)}`));

try {
  await page.goto(`http://${app}.test.localhost:8090`, { waitUntil: "domcontentloaded" });
  await page.fill("#u", user);
  await page.fill("#p", pass);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 20000 });
  await page.waitForFunction(
    () => document.querySelectorAll("#sessList .sess").length > 0,
    { timeout: 25000 }
  );

  for (const t of ["sessions", "chat", "health", "config"]) {
    await page.click(`#panelTabs button[data-panel="${t}"]`);
    await page.waitForSelector(`#panel-${t}:not(.hidden)`, { timeout: 5000 });
    await page.waitForTimeout(700);   // để bảng/khung kịp vẽ xong
    await page.screenshot({ path: `${OUT}${tag}-${t}.png` });
  }

  // Màn chi tiết là màn dày chữ nhất, nên cũng là màn dễ xấu nhất — chụp
  // fullPage để thấy cả phần phải cuộn mới tới.
  await page.click('#panelTabs button[data-panel="sessions"]');
  const id = await page.evaluate(
    () => document.querySelector('#sessList .sess:not([data-host="dead"])')?.dataset.session
  );
  if (!id) {
    console.log("không có phiên nào đang sống — bỏ qua ảnh màn chi tiết");
  } else {
    await page.locator(`.sess[data-session="${id}"]`).click();
    await page.waitForSelector("#sessDetail:not(.hidden)", { timeout: 10000 });
    // Luồng đọc từ nhật ký của phiên; phiên dài mất vài chục giây mới về.
    await page.waitForFunction(
      () => document.querySelectorAll("#sessStream .ev").length > 0,
      { timeout: 180000, polling: 1000 }
    );
    await page.waitForTimeout(1500);
    const n = await page.evaluate(() => document.querySelectorAll("#sessStream .ev").length);
    await page.screenshot({ path: `${OUT}${tag}-detail.png`, fullPage: true });
    console.log(`màn chi tiết: ${n} sự kiện`);
  }

  console.log(`chụp xong → ui-shots/${tag}-*.png`);
} finally {
  // Ảnh đẹp mà console đỏ thì vẫn là hỏng — nói ra, đừng để ảnh nói thay.
  if (errors.length) {
    console.log(`\n⚠ ${errors.length} lỗi console:`);
    errors.forEach((e) => console.log("  - " + e));
  } else {
    console.log("0 lỗi console");
  }
  await browser.close();
}
process.exit(errors.length ? 1 : 0);
