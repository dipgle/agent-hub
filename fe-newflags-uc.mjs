// UC-S20 — `/new -a <acc> -s <dự án>`: cờ thay cho thứ tự, và mở xong thì THEO
// luôn phiên mới.
//
// Hà 2026-08-12: *"kiến trúc lại lệnh cho hợp lý, ví dụ: `/new -a acc2 -s
// dwork` thì sẽ tạo một phiên mới chạy acc2 cho dự án dwork và mặc định sẽ
// focus luôn vào phiên mới → đặt câu hỏi luôn vào luôn phiên mới này"*.
//
// Ba mệnh đề phải nghiệm thu, và cả ba chỉ đúng khi CHẠY THẬT:
//   1. cờ được đọc đúng (tài khoản + dự án), đề bài để trống vẫn mở được;
//   2. cửa sổ terminal THẬT mở ra trên máy (không phải phiên nền);
//   3. con trỏ "đang theo" chuyển sang phiên mới, và màn NÓI RA điều đó —
//      một tính năng không ai biết là một tính năng không tồn tại.
//
// Usage:
//   node fe-newflags-uc.mjs <app_tid> <username> <password> [acc] [dự án]

import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { mkdirSync } from "node:fs";

const [appTid, username, password, acc = "acc3", project = "hub"] = process.argv.slice(2);
if (!appTid || !username || !password) {
  console.error("usage: node fe-newflags-uc.mjs <app_tid> <user> <pass> [acc] [dự án]");
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
  await page.waitForSelector("#thread:not(.hidden)", { timeout: 15000 });

  // 🔴 Đếm TRƯỚC khi gõ, và chỉ đọc những tin MỚI HƠN lượt gõ này.
  //
  // Bản đầu quét cả luồng tìm "🚀" hoặc "⚠" và bắt trúng một tin ⚠ từ 12:45
  // còn nằm trong lịch sử — kịch bản xanh/đỏ theo một chuyện đã xảy ra ba
  // tiếng trước, trong khi lệnh vừa gõ CHẠY ĐÚNG (log: `new_window_opened
  // tty=ttys003`). Một phép đo nhìn nhầm chỗ thì kết quả của nó không nói gì
  // về sản phẩm — đúng bài "phép đo mù" của dự án này.
  const before = await page.locator(".msg .body").count();
  const cmd = `/new -a ${acc} -s ${project}`;
  await page.fill("#text", cmd);
  await page.press("#text", "Enter");
  console.log(`đã gõ: ${cmd} — chờ hub mở cửa sổ (tối đa 150s)`);

  const isReply = (t) => t.includes("🚀 Đã mở") || t.includes("⚠");
  await page.waitForFunction(
    ({ before, needle }) =>
      [...document.querySelectorAll(".msg .body")]
        .slice(before)
        .some((e) => e.textContent.includes(needle[0]) || e.textContent.includes(needle[1])),
    { before, needle: ["🚀 Đã mở", "⚠"] },
    { timeout: 150000 }
  );
  ack = await page.evaluate(
    (before) =>
      [...document.querySelectorAll(".msg .body")]
        .slice(before)
        .map((e) => e.textContent)
        .filter((t) => t.includes("🚀 Đã mở") || t.includes("⚠"))
        .pop() || "",
    before
  );
  void isReply;

  check("hub trả lời lệnh có cờ", ack.length > 0, ack.slice(0, 80));
  check("mở được, không phải lỗi", ack.includes("🚀 Đã mở"), ack.slice(0, 120));
  check("mở CỬA SỔ terminal thật, không phải phiên nền", ack.includes("cửa sổ terminal"));
  check(`đọc đúng cờ -s: dự án ${project}`, ack.includes(project));
  check(`đọc đúng cờ -a: tài khoản ${acc}`, ack.includes(acc));
  check("nói ra rằng đang THEO phiên mới", ack.includes("Đang theo phiên này"));
  check("chỉ đường gõ tiếp", ack.includes("gõ thẳng câu hỏi"));
  check("không con số $ nào", !ack.includes("$"));

  await page.screenshot({ path: `${SHOTS}newflags-uc.png`, fullPage: true });
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
console.log("ảnh chụp: ui-shots/newflags-uc.png");
