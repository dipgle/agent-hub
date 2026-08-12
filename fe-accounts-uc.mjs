// UC-S19 — `/accounts`: ba tài khoản claude, và `/new` rơi vào tài khoản nào.
//
// Hà 2026-08-12: *"chưa có lệnh xem danh sách acc"* → *"vậy lệnh new chọn acc
// kiểu gì? hay đang để random?"*. Hai câu ấy là một câu: chọn tài khoản là một
// quyết định có hậu quả (tuần cạn hạn mức thì phiên mới chết giữa chừng), mà số
// liệu để quyết định chỉ nằm trên tab Sức khoẻ — thứ không với tới được khi
// đang gõ trên Telegram.
//
// Kịch bản này đi ĐÚNG đường người dùng: mở bundle đã deploy, đăng nhập bằng
// form, gõ lệnh vào ô nhập, bấm Enter, rồi ĐỌC câu trả lời hiện trên màn. Không
// gọi API, không đọc DB — nếu route hỏng thì màn phải nói ra.
//
// Usage:
//   node fe-accounts-uc.mjs <app_tid> <username> <password>

import { chromium } from "/Users/hanguyen/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { mkdirSync } from "node:fs";

const [appTid, username, password] = process.argv.slice(2);
if (!appTid || !username || !password) {
  console.error("usage: node fe-accounts-uc.mjs <app_tid> <username> <password>");
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

// Đúng cỡ màn nghiệm thu của dự án này — một câu trả lời dài phải đọc được trên
// điện thoại, không chỉ "có mặt trong DOM".
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 390, height: 844 } });

page.on("console", (m) => {
  if (m.type() === "error") problems.push(`console.error: ${m.text()}`);
});
page.on("pageerror", (e) => problems.push(`uncaught: ${e.message}`));

let ack = "";
try {
  await page.goto(BASE, { waitUntil: "domcontentloaded" });
  await page.fill("#u", username);
  await page.fill("#p", password);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 15000 });
  await page.click('#panelTabs button[data-panel="chat"]');
  await page.waitForSelector("#foot:not(.hidden)", { timeout: 15000 });
  await page.waitForSelector("#thread:not(.hidden)", { timeout: 15000 });
  check("vào được phòng bằng form đăng nhập thật", await page.locator("#thread").isVisible());

  // Gõ lệnh như người: vào ô nhập, Enter.
  const before = await page.locator(".msg:not(#pending) .body").count();
  await page.fill("#text", "/accounts");
  await page.press("#text", "Enter");

  // Chờ ĐÚNG câu trả lời của route, không chờ "có thêm một tin" — tin thêm có
  // thể là chính lệnh vừa gõ.
  await page.waitForFunction(
    () =>
      [...document.querySelectorAll(".msg .body")].some((e) =>
        e.textContent.includes("tài khoản claude")
      ),
    null,
    { timeout: 90000 }
  );
  ack = await page.evaluate(
    () =>
      [...document.querySelectorAll(".msg .body")]
        .map((e) => e.textContent)
        .filter((t) => t.includes("tài khoản claude"))
        .pop() || ""
  );
  check("route /accounts trả lời trong phòng", ack.length > 0, `${ack.length} ký tự`);
  check(
    "phòng nhận thêm tin (lệnh + câu trả lời)",
    (await page.locator(".msg:not(#pending) .body").count()) > before
  );

  // Nội dung: đủ ba tài khoản, và nói RÕ cái nào là mặc định của /new.
  for (const acc of ["acc1", "acc2", "acc3"]) {
    check(`có ${acc}`, ack.includes(acc));
  }
  check("nói ra tài khoản mặc định của /new", ack.includes("mặc định"));
  check("chỉ MỘT tài khoản được gắn nhãn mặc định", (ack.match(/mặc định của \/new/g) || []).length === 1);
  check("chỉ đường đổi tài khoản", ack.includes("@acc2") || ack.includes("-a"));

  // Luật của nhà: không con số tiền nào lên mặt tiền (CLAUDE.md §9).
  check("không có con số $ nào trên màn", !ack.includes("$"));

  // Hạn mức: hoặc có số thật, hoặc nói thẳng đang đo — KHÔNG được im.
  const hasQuota = /tuần \d+%|phiên \d+%|đang đo|chưa đo được/.test(ack);
  check("nói được tình trạng hạn mức (số thật hoặc 'đang đo')", hasQuota);

  // Đọc được bằng MẮT ở 390px, không chỉ có mặt trong DOM: câu trả lời nhiều
  // dòng nên phải kiểm tràn ngang — bài học 2026-08-10 (7/7 xanh trong khi màn
  // cắt cụt câu cảnh báo).
  const overflow = await page.evaluate(() => {
    const el = [...document.querySelectorAll(".msg .body")]
      .filter((e) => e.textContent.includes("tài khoản claude"))
      .pop();
    if (!el) return null;
    return { scroll: el.scrollWidth, client: el.clientWidth };
  });
  check(
    "câu trả lời không tràn ngang ở 390px",
    overflow && overflow.scroll <= overflow.client + 1,
    overflow ? `scrollWidth ${overflow.scroll} · clientWidth ${overflow.client}` : "không thấy ô"
  );

  await page.screenshot({ path: `${SHOTS}accounts-uc.png`, fullPage: true });
} catch (e) {
  problems.push(`ngoại lệ: ${e.message}`);
} finally {
  await browser.close();
}

if (ack) console.log(`\n--- câu trả lời thật ---\n${ack}\n------------------------`);
const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra qua · ${problems.length} vấn đề`);
if (problems.length) {
  console.log("\nVẤN ĐỀ:");
  problems.forEach((p) => console.log(`  - ${p}`));
  process.exit(1);
}
console.log("ảnh chụp: ui-shots/accounts-uc.png");
