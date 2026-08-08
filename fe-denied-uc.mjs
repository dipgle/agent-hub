// Regression lock for the 2026-08-07 bug: an account WITHOUT permission was
// told "Mất kết nối, đang thử lại…" every 3 seconds, forever — the FE could
// not tell a refusal from an outage, so the user chased a phantom network
// problem while the real answer was "you are not on this app's ACL".
//
// The journey is the real one, both halves through the UI:
//   1. the app owner removes the test account from "Can view" in the console,
//   2. that account logs into the chat FE and must see WHY it cannot enter,
//   3. the owner puts it back (always, even if the assertions fail).
//
// Usage: node fe-denied-uc.mjs
import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { readFileSync, mkdirSync } from "node:fs";

const HERE = new URL("./", import.meta.url).pathname;
const env = Object.fromEntries(
  readFileSync(HERE + "hub.env", "utf8")
    .split("\n").filter((l) => l.includes("=") && !l.trim().startsWith("#"))
    .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim().replace(/^["']|["']$/g, "")])
);
const APP_TID = env.HUB_TFL5_APP_TID || "a-65dd60d3-624e-45a9-8fdf-62aa7d894d80";
const ALICE = env.HUB_TFL5_ALICE_USER || "alice_local";
const ALICE_TID = env.HUB_TFL5_ALICE_TID || "u-b08803e3-50b8-4fbf-84de-61c8d3b933d3";
const SHOTS = HERE + "ui-shots/";
mkdirSync(SHOTS, { recursive: true });

const checks = [];
const check = (name, ok, detail = "") => {
  checks.push({ name, ok, detail });
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? ` — ${detail}` : ""}`);
};

const browser = await chromium.launch({ headless: true });

// ---- half 1: the owner edits the ACL in the console -----------------------
const owner = await browser.newPage({ viewport: { width: 1400, height: 950 } });
const openAccessDialog = async () => {
  await owner.getByText(/Manage access/i).first().waitFor({ timeout: 15000 });
  await owner.getByText(/Manage access/i).first().click();
  await owner.getByText("Who can use hub").waitFor({ timeout: 10000 });
};
const dialogFields = () =>
  owner.evaluate(() => {
    const dlg = document.querySelector(".modal") || document.body;
    const walker = document.createTreeWalker(dlg, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT);
    const out = [];
    let lastText = "";
    while (walker.nextNode()) {
      const n = walker.currentNode;
      if (n.nodeType === Node.TEXT_NODE) { const t = n.textContent.trim(); if (t) lastText = t; }
      else if (n.tagName === "INPUT") out.push({ label: lastText, value: n.value });
    }
    return out;
  });

/// Rewrites the "Can view" box and saves. Returns the value it wrote.
const setCanView = async (value) => {
  await openAccessDialog();
  const fields = await dialogFields();
  const i = fields.findIndex((f) => f.label.startsWith("Can view"));
  if (i < 0) throw new Error('không tìm thấy ô "Can view"');
  await owner.locator(".modal input").nth(i).fill(value);
  await owner.getByRole("button", { name: /^Save$/ }).click();
  await owner.waitForTimeout(2500);
  return value;
};

let restored = false;
let viewBefore = "";
try {
  await owner.goto("http://localhost:8090/", { waitUntil: "domcontentloaded" });
  await owner.getByText(/More sign-in options/i).first().click();
  await owner.locator('input[name="username"]').fill(env.HUB_TFL5_USER);
  await owner.locator('input[type="password"]').first().fill(env.HUB_TFL5_PASSWORD);
  await owner.getByRole("button", { name: /Sign in with password/i }).click();
  await owner.getByText("hub", { exact: true }).first().waitFor({ timeout: 20000 });
  await owner.getByText(/^Open$/).first().click();

  await openAccessDialog();
  const fields = await dialogFields();
  viewBefore = fields[fields.findIndex((f) => f.label.startsWith("Can view"))].value;
  console.log("Can view ban đầu:", viewBefore || "(trống)");
  await owner.getByRole("button", { name: /^Cancel$/ }).click();

  const without = viewBefore.split(",").map((s) => s.trim())
    .filter((s) => s && s !== ALICE_TID).join(", ");
  await setCanView(without);
  console.log(`đã gỡ ${ALICE} khỏi "Can view" (còn: ${without || "trống"})`);

  // ---- half 2: the account without permission opens the chat -------------
  const user = await browser.newPage({ viewport: { width: 900, height: 820 } });
  const notes = [];
  const consoleErrors = [];
  user.on("console", (m) => { if (m.type() === "error") consoleErrors.push(m.text().slice(0, 160)); });
  user.on("pageerror", (e) => consoleErrors.push(`uncaught: ${e.message.slice(0, 160)}`));

  await user.goto(`http://${APP_TID}.test.localhost:8090`, { waitUntil: "domcontentloaded" });
  await user.fill("#u", ALICE);
  await user.fill("#p", env.HUB_TFL5_ALICE_PASSWORD);
  await user.click("#loginBtn");

  // The refusal must be on screen quickly, and must name the reason.
  await user.waitForFunction(
    () => [...document.querySelectorAll(".note.bad")].some((n) => /chưa được cấp quyền/.test(n.textContent)),
    { timeout: 15000 }
  );
  const shown = await user.locator(".note.bad").first().innerText();
  check("nói rõ là KHÔNG CÓ QUYỀN, không phải mất mạng", /chưa được cấp quyền/.test(shown), shown.slice(0, 120));
  check("nêu đúng tài khoản bị từ chối", shown.includes(ALICE));
  check("chỉ ra cách sửa (Can view)", /Can view/.test(shown));
  // #status is styled `text-transform: uppercase`, and innerText reports the
  // transformed text — compare case-insensitively or the check can never pass.
  check("nhãn trạng thái là 'không có quyền'",
    (await user.locator("#status").innerText()).trim().toLowerCase() === "không có quyền");
  check("ô nhập tin bị ẩn khi không có quyền", !(await user.locator("#foot").isVisible()));
  check("có lối thoát: nút đăng nhập tài khoản khác", await user.locator("#switchUser").isVisible());
  await user.screenshot({ path: `${SHOTS}denied-01.png`, fullPage: true });

  // The old build appended a "Mất kết nối" note every 3s. Wait through four
  // of those windows: the count must stay at zero.
  await user.waitForTimeout(13000);
  const lost = await user.evaluate(
    () => [...document.querySelectorAll(".note")].filter((n) => /Mất kết nối/.test(n.textContent)).length
  );
  check("KHÔNG lặp 'Mất kết nối' (13s ≈ 4 vòng thử lại cũ)", lost === 0, `${lost} dòng`);
  const denies = await user.evaluate(
    () => [...document.querySelectorAll(".note.bad")].filter((n) => /chưa được cấp quyền/.test(n.textContent)).length
  );
  check("thông báo chỉ hiện một lần", denies === 1, `${denies} dòng`);
  check("0 lỗi console", consoleErrors.length === 0, consoleErrors.join(" | "));
  await user.screenshot({ path: `${SHOTS}denied-02-after-13s.png`, fullPage: true });
  await user.close();
} catch (e) {
  check("kịch bản chạy trọn vẹn", false, e.message.split("\n")[0]);
} finally {
  // Always put the ACL back the way it was.
  try {
    await setCanView(viewBefore);
    // Saving closes the dialog, so re-open it before reading the field back.
    await openAccessDialog();
    const after = await dialogFields();
    const v = after[after.findIndex((f) => f.label.startsWith("Can view"))].value;
    restored = v.includes(ALICE_TID);
    check("ACL đã khôi phục nguyên trạng", restored, v);
  } catch (e) {
    check("ACL đã khôi phục nguyên trạng", false, e.message.split("\n")[0]);
  }
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra qua`);
if (!restored) console.log(`⚠ KIỂM TRA TAY: "Can view" phải chứa ${ALICE_TID}`);
process.exit(passed === checks.length ? 0 : 1);
