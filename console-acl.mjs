// Set who can use the huba chat app — through the tfl5 console UI, the same
// path a person takes: sign in as the app owner, open the app, "Manage
// access", edit the buckets, Save.
//
// Usage: node console-acl.mjs <u-...>
//
// Two things this script learned the hard way (2026-08-07):
//   1. A label-relative xpath ("the input after the text 'Can view'") matched
//      the SUMMARY line outside the dialog ("· 1 can view") and silently wrote
//      the user into "Can delete". The dialog's fields are now read by dumping
//      (label, value) pairs from the DOM and matching the label exactly.
//   2. Verifying by re-reading the same field we just typed into proves
//      nothing. The check below reads the INDEPENDENT summary line
//      ("N can view") that the app renders from the saved row.
import { chromium } from "/Users/hanguyen/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { readFileSync } from "node:fs";

const TARGET = process.argv[2];
if (!TARGET || !TARGET.startsWith("u-")) {
  console.error("usage: node console-acl.mjs <u-...>");
  process.exit(2);
}

const env = Object.fromEntries(
  readFileSync(new URL("./huba.env", import.meta.url), "utf8")
    .split("\n")
    .filter((l) => l.includes("=") && !l.trim().startsWith("#"))
    .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim().replace(/^["']|["']$/g, "")])
);

const SHOTS = new URL("./ui-shots/", import.meta.url).pathname;
const problems = [];
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
page.on("console", (m) => { if (m.type() === "error") problems.push(`console.error: ${m.text().slice(0, 200)}`); });
page.on("pageerror", (e) => problems.push(`uncaught: ${e.message.slice(0, 200)}`));

// Reads the access dialog as the user sees it: every field with its own label.
const dumpDialog = () =>
  page.evaluate(() => {
    const dlg = document.querySelector(".modal") || document.body;
    // Walk the dialog in document order: the label of a field is simply the
    // last piece of text seen before its <input>. Works regardless of how the
    // markup nests the label.
    const walker = document.createTreeWalker(dlg, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT);
    const out = [];
    let lastText = "";
    while (walker.nextNode()) {
      const n = walker.currentNode;
      if (n.nodeType === Node.TEXT_NODE) {
        const t = n.textContent.trim();
        if (t) lastText = t;
      } else if (n.tagName === "INPUT") {
        out.push({ label: lastText, value: n.value });
      }
    }
    return out;
  });

const openAccessDialog = async () => {
  await page.getByText(/Manage access/i).first().waitFor({ timeout: 15000 });
  await page.getByText(/Manage access/i).first().click();
  await page.getByText("Who can use huba").waitFor({ timeout: 10000 });
};

// The summary the app renders from the SAVED row — our independent check.
const summary = () => page.locator("text=/can view/").first().innerText();

const fieldIndex = (fields, label) => fields.findIndex((f) => f.label.startsWith(label));

try {
  await page.goto("http://localhost:8090/", { waitUntil: "domcontentloaded" });
  await page.getByText(/More sign-in options/i).first().click();
  await page.locator('input[name="username"]').fill(env.HUB_TFL5_USER);
  await page.locator('input[type="password"]').first().fill(env.HUB_TFL5_PASSWORD);
  await page.getByRole("button", { name: /Sign in with password/i }).click();
  await page.getByText("huba", { exact: true }).first().waitFor({ timeout: 20000 });
  await page.getByText(/^Open$/).first().click();

  const before = (await summary()).trim();
  console.log("tóm tắt TRƯỚC:", before);

  await openAccessDialog();
  const fields = await dumpDialog();
  console.log("--- các ô trong hộp thoại (nhãn → giá trị) ---");
  fields.forEach((f, i) => console.log(`  [${i}] ${f.label || "(không nhãn)"} → ${f.value || "(trống)"}`));

  const iView = fieldIndex(fields, "Can view");
  const iDel = fieldIndex(fields, "Can delete");
  if (iView < 0) throw new Error('không tìm thấy ô "Can view" trong hộp thoại');
  const inputs = page.locator(".modal input");

  // Put the user in "Can view", and take back the "Can delete" entry the
  // previous buggy run left behind (if it is there).
  const viewVal = fields[iView].value.trim();
  if (!viewVal.includes(TARGET)) {
    await inputs.nth(iView).fill(viewVal ? `${viewVal}, ${TARGET}` : TARGET);
  }
  if (iDel >= 0 && fields[iDel].value.includes(TARGET)) {
    const cleaned = fields[iDel].value
      .split(",").map((s) => s.trim()).filter((s) => s && s !== TARGET).join(", ");
    await inputs.nth(iDel).fill(cleaned);
    console.log(`dọn: gỡ ${TARGET} khỏi "Can delete"`);
  }
  await page.screenshot({ path: `${SHOTS}acl-01-dialog-filled.png`, fullPage: true });
  await page.getByRole("button", { name: /^Save$/ }).click();
  await page.waitForTimeout(3000);

  // Verify on a fresh load, against the summary the server feeds — not the
  // field we typed into.
  await page.reload({ waitUntil: "domcontentloaded" });
  await page.getByText(/Manage access/i).first().waitFor({ timeout: 15000 });
  const after = (await summary()).trim();
  console.log("tóm tắt SAU: ", after);

  await openAccessDialog();
  const finalFields = await dumpDialog();
  console.log("--- hộp thoại sau khi lưu ---");
  finalFields.forEach((f, i) => console.log(`  [${i}] ${f.label || "(không nhãn)"} → ${f.value || "(trống)"}`));
  await page.screenshot({ path: `${SHOTS}acl-02-dialog-after.png`, fullPage: true });

  const fView = finalFields[fieldIndex(finalFields, "Can view")];
  const fDel = finalFields[fieldIndex(finalFields, "Can delete")];
  if (!fView.value.includes(TARGET)) problems.push(`"Can view" vẫn không chứa ${TARGET}`);
  if (fDel && fDel.value.includes(TARGET)) problems.push(`${TARGET} còn sót trong "Can delete"`);
  const n = (s) => Number((s.match(/(\d+)\s+can view/) || [])[1] ?? -1);
  if (n(after) <= n(before)) {
    problems.push(`số "can view" không tăng: trước="${before}" sau="${after}"`);
  }
} catch (e) {
  problems.push(`ngoại lệ: ${e.message.split("\n")[0]}`);
} finally {
  await browser.close();
}

if (problems.length) {
  console.log("\nVẤN ĐỀ:");
  problems.forEach((p) => console.log("  - " + p));
  process.exit(1);
}
console.log("\nĐẠT: quyền xem đã cấp qua giao diện, xác nhận bằng dòng tóm tắt sau khi tải lại.");
