// Does the portal actually work ON A PHONE? That is the only device the owner
// uses, and until 2026-08-08 nothing had ever measured it: all 14 acceptance
// scripts run at 900–1500px wide, and the two-column layout only appears at
// >=960px, so every "green" run exercised a size the owner never sees.
//
// Everything goes through the UI like a person: type into the login form, tap
// the tabs. No goto shortcuts, no API calls to reach a state.
//
// What it measures — things that only break at 390px:
//   * horizontal overflow (the page must never scroll sideways)
//   * tap targets big enough to hit with a thumb
//   * text not smaller than 12px
//   * which panels force a sideways scroll INSIDE their own box (allowed for a
//     table, but it must be the table scrolling, not the page)
//   * 0 console errors, like every other script here
//
// Usage:
//   node fe-phone-uc.mjs <app_tid> <username> <password>

import { chromium } from "/Users/hanguyen/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { mkdirSync } from "node:fs";

const [appTid, username, password] = process.argv.slice(2);
if (!appTid || !username || !password) {
  console.error("usage: node fe-phone-uc.mjs <app_tid> <username> <password>");
  process.exit(2);
}
const BASE = `http://${appTid}.test.localhost:8090`;
const SHOTS = new URL("./ui-shots/", import.meta.url).pathname;
mkdirSync(SHOTS, { recursive: true });

// iPhone 14/15 CSS pixels. Narrow enough to catch what a tablet would hide.
const PHONE = { width: 390, height: 844 };
const MIN_TAP = 32; // hard fail below this
const WANT_TAP = 44; // Apple HIG; below this is reported, not failed
const MIN_FONT = 12;

const problems = [];
const notes = [];
const checks = [];
const check = (name, ok, detail = "") => {
  checks.push({ name, ok, detail });
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) problems.push(`${name}${detail ? `: ${detail}` : ""}`);
};
const note = (s) => {
  notes.push(s);
  console.log(`  · ${s}`);
};

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({
  viewport: PHONE,
  deviceScaleFactor: 3,
  isMobile: true,
  hasTouch: true,
});

page.on("console", (m) => {
  if (m.type() === "error") problems.push(`console.error: ${m.text()}`);
});
page.on("pageerror", (e) => problems.push(`uncaught: ${e.message}`));

/** Page-level sideways scroll: the thing that makes a page feel broken. */
const overflow = () =>
  page.evaluate(() => ({
    scrollW: document.documentElement.scrollWidth,
    innerW: window.innerWidth,
    // Which visible elements stick out past the right edge, and by how much.
    culprits: [...document.querySelectorAll("body *")]
      .map((el) => {
        const r = el.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) return null;
        const over = Math.round(r.right - window.innerWidth);
        if (over <= 1) return null;
        return {
          over,
          tag: el.tagName.toLowerCase(),
          id: el.id || "",
          cls: (el.className || "").toString().slice(0, 40),
        };
      })
      .filter(Boolean)
      .sort((a, b) => b.over - a.over)
      .slice(0, 6),
  }));

/** Tap targets and text size, for whatever is on screen right now. */
const ergonomics = () =>
  page.evaluate(
    ({ minTap, wantTap, minFont }) => {
      const vis = (el) => {
        const r = el.getBoundingClientRect();
        const s = getComputedStyle(el);
        return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none";
      };
      const label = (el) =>
        `${el.tagName.toLowerCase()}${el.id ? "#" + el.id : ""}: ${(el.innerText || el.value || el.placeholder || "").trim().slice(0, 24)}`;

      const tappable = [...document.querySelectorAll("button, a, input, select, textarea")].filter(vis);
      const tooSmall = [];
      const belowHig = [];
      for (const el of tappable) {
        // Ô tích nằm trong <label>: bấm vào CHỮ cũng đổi trạng thái (hành vi
        // sẵn có của label), nên vùng chạm thật là cả hàng nhãn. Đo riêng cái
        // hộp 18px rồi bắt nó ≥32px là bắt sản phẩm xấu đi để phép đo xanh —
        // đúng lỗi đã gây ra ô tích 32×32 "to đùng" (Hà, 2026-08-09).
        const target =
          el.type === "checkbox" || el.type === "radio" ? el.closest("label") || el : el;
        const h = Math.round(target.getBoundingClientRect().height);
        if (h < minTap) tooSmall.push(`${label(el)} = ${h}px`);
        else if (h < wantTap) belowHig.push(`${label(el)} = ${h}px`);
      }

      const tiny = [];
      for (const el of [...document.querySelectorAll("body *")].filter(vis)) {
        if (!el.childNodes.length) continue;
        const ownText = [...el.childNodes]
          .filter((n) => n.nodeType === 3)
          .map((n) => n.textContent.trim())
          .join("");
        if (ownText.length < 3) continue;
        const fs = parseFloat(getComputedStyle(el).fontSize);
        if (fs < minFont) tiny.push(`${el.tagName.toLowerCase()}.${(el.className || "").toString().slice(0, 20)} = ${fs}px "${ownText.slice(0, 20)}"`);
      }

      // Boxes that scroll sideways on their own. A table doing this is fine;
      // the page doing it is not.
      const innerScroll = [...document.querySelectorAll("body *")]
        .filter(vis)
        .filter((el) => el.scrollWidth > el.clientWidth + 1)
        .map((el) => `${el.tagName.toLowerCase()}${el.id ? "#" + el.id : "." + (el.className || "").toString().split(" ")[0]} ${el.clientWidth}→${el.scrollWidth}px`)
        .slice(0, 6);

      // Cuộn lồng cuộn: trang cuộn, mà khung bên trong cũng cuộn. Trên điện
      // thoại đó là cái bẫy "vuốt trúng khung trong thì trang không nhúc
      // nhích" — Hà 2026-08-09: *"ui đang bị hiện quá nhiều thanh cuộn"*. Đo
      // được: mỗi tab có 2–3 khối cuộn dọc cùng lúc.
      //
      // Bỏ qua ô nhập nhiều dòng: `textarea` tự cao lên theo chữ đang gõ, đó là
      // hành vi của ô nhập chứ không phải một vùng cuộn của trang.
      const nested = [...document.querySelectorAll("body *")]
        .filter(vis)
        .filter((el) => !/^(TEXTAREA|SELECT|INPUT)$/.test(el.tagName))
        .filter((el) => {
          const oy = getComputedStyle(el).overflowY;
          return (oy === "auto" || oy === "scroll") && el.scrollHeight > el.clientHeight + 2;
        })
        .map((el) => `${el.tagName.toLowerCase()}${el.id ? "#" + el.id : ""} ${el.clientHeight}→${el.scrollHeight}`);

      return { tapCount: tappable.length, tooSmall, belowHig, tiny: tiny.slice(0, 8), innerScroll, nested };
    },
    { minTap: MIN_TAP, wantTap: WANT_TAP, minFont: MIN_FONT }
  );

const auditScreen = async (name) => {
  const o = await overflow();
  check(
    `[${name}] trang không tràn ngang`,
    o.scrollW <= o.innerW + 1,
    `scrollWidth ${o.scrollW} / màn ${o.innerW}` +
      (o.culprits.length ? ` · thủ phạm: ${o.culprits.map((c) => `${c.tag}${c.id ? "#" + c.id : "." + c.cls}+${c.over}px`).join(", ")}` : "")
  );

  const e = await ergonomics();
  check(`[${name}] mọi nút/ô chạm được (>=${MIN_TAP}px)`, e.tooSmall.length === 0, e.tooSmall.slice(0, 4).join(" · "));
  check(`[${name}] không có chữ dưới ${MIN_FONT}px`, e.tiny.length === 0, e.tiny.slice(0, 3).join(" · "));
  check(
    `[${name}] chỉ MỘT khối cuộn dọc (là trang)`,
    e.nested.length <= 1,
    e.nested.join(" · ")
  );
  if (e.belowHig.length) note(`[${name}] ${e.belowHig.length} vùng chạm 32–43px (dưới chuẩn 44px): ${e.belowHig.slice(0, 3).join(" · ")}`);
  if (e.innerScroll.length) note(`[${name}] cuộn ngang bên trong: ${e.innerScroll.join(" · ")}`);
  await page.screenshot({ path: `${SHOTS}phone-${name}.png`, fullPage: false });
};

try {
  const res = await page.goto(BASE, { waitUntil: "domcontentloaded" });
  check("trang tải được từ bundle đã deploy", res.status() === 200, `HTTP ${res.status()}`);
  await auditScreen("01-login");

  await page.fill("#u", username);
  await page.fill("#p", password);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 20000 });
  await page.waitForTimeout(1500); // let the first snapshot paint

  // Every tab, tapped the way a thumb would.
  for (const [panel, label] of [
    ["sessions", "02-phien"],
    ["chat", "03-trao-doi"],
    ["health", "04-suc-khoe"],
    ["config", "05-cau-hinh"],
  ]) {
    await page.click(`#panelTabs button[data-panel="${panel}"]`);
    await page.waitForTimeout(900);
    check(`tab ${panel} mở được bằng một lần chạm`, await page.locator(`#panelTabs button[data-panel="${panel}"].on`).count() === 1);
    await auditScreen(label);
  }

  // The work the owner actually does from a phone: open a session and read it.
  // Was "open an inbox row" until 2026-08-08 — the inbox is gone, and the thing
  // he opens the phone for is the list of Claude sessions running on the Mac.
  await page.click('#panelTabs button[data-panel="sessions"]');
  await page.waitForTimeout(600);
  const rows = await page.locator("#sessList .sess").count();
  check("có phiên để mở", rows > 0, `${rows} phiên`);
  // The point of the whole product on a phone: see the work. If the first row
  // sits below the fold, every visit starts with a scroll past chrome.
  const fold = await page.evaluate(() => {
    const row = document.querySelector("#sessList .sess");
    if (!row) return null;
    return {
      top: Math.round(row.getBoundingClientRect().top),
      screen: window.innerHeight,
      pct: Math.round((row.getBoundingClientRect().top / window.innerHeight) * 100),
    };
  });
  if (fold) {
    check(
      "phiên đầu tiên nằm trong màn đầu, không phải cuộn mới thấy",
      fold.top < fold.screen * 0.5,
      `bắt đầu ở ${fold.top}px = ${fold.pct}% chiều cao màn (${fold.screen}px)`
    );
  }
  if (rows > 0) {
    await page.locator("#sessList .sess").first().click();
    await page.waitForTimeout(800);
    await auditScreen("06-chi-tiet-phien");
  }
} catch (e) {
  problems.push(`ngoại lệ: ${e.message}`);
} finally {
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra qua · ${problems.length} vấn đề · ${notes.length} ghi chú`);
if (problems.length) {
  console.log("\nVẤN ĐỀ:");
  problems.forEach((p) => console.log(`  - ${p}`));
  process.exit(1);
}
console.log(`ảnh chụp ở ui-shots/phone-*.png (${PHONE.width}×${PHONE.height})`);
