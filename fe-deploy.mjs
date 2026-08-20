// Ship fe/index.html to tfl5 as a new immutable bundle version — through the
// console UI (Deploy → Releases → Upload → Activate), the same path the app
// owner takes by hand. Replaces the ad-hoc curl calls used for v1/v2.
//
// Usage: node fe-deploy.mjs <version> "<notes>"
//   e.g. node fe-deploy.mjs v3 "phân biệt 403 với mất mạng"
//
// Verification is deliberately NOT "the form said ok": after activating, the
// script fetches the public URL and checks the bytes a visitor now receives.
import { chromium } from "/Users/hanguyen/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { readFileSync, readdirSync, mkdirSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";

const [version, notes = ""] = process.argv.slice(2);
if (!version || !/^[A-Za-z0-9._-]{1,64}$/.test(version)) {
  console.error('usage: node fe-deploy.mjs <version> "<notes>"   (version: A-Z a-z 0-9 . _ - , max 64)');
  process.exit(2);
}

const HERE = new URL("./", import.meta.url).pathname;
const env = Object.fromEntries(
  readFileSync(HERE + "huba.env", "utf8")
    .split("\n").filter((l) => l.includes("=") && !l.trim().startsWith("#"))
    .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim().replace(/^["']|["']$/g, "")])
);
const APP_TID = env.HUB_TFL5_APP_TID || "a-65dd60d3-624e-45a9-8fdf-62aa7d894d80";

// Verification material: the exact bytes we are shipping. Comparing the
// served page against THIS (rather than grepping for a marker that survives
// every version) is what makes "deployed" checkable instead of assumed.
const source = readFileSync(HERE + "fe/index.html", "utf8");

const TMP = HERE + ".tmp/";
mkdirSync(TMP, { recursive: true });
const zipPath = `${TMP}huba-fe-${version}.zip`;
rmSync(zipPath, { force: true });
// -j flattens: every file in fe/ lands at the bundle root, which is where
// index.html has to be and where the page expects echarts.min.js.
const assets = readdirSync(HERE + "fe").map((f) => HERE + "fe/" + f);
execFileSync("zip", ["-q", "-j", zipPath, ...assets]);
console.log(`zip: ${zipPath} (${assets.length} tệp: ${assets.map((a) => a.split("/").pop()).join(", ")})`);

const SHOTS = HERE + "ui-shots/";
mkdirSync(SHOTS, { recursive: true });
const problems = [];
// A bundle version is immutable on the server: uploading the same name again
// is a no-op, so re-using a name after editing the page ships nothing. Tracked
// here so the byte check below can say exactly that instead of "khác nhau".
let versionExisted = false;
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1400, height: 1100 } });
// These come from tfl5's OWN admin console, not from the bundle being
// shipped, so they are reported but do not fail the deploy. (Known one:
// the Releases form's `pattern` attribute is invalid under Chrome's `v`
// flag — a tfl5 bug, logged in huba's active-context.)
const hostErrors = [];
page.on("console", (m) => { if (m.type() === "error") hostErrors.push(m.text().slice(0, 200)); });
page.on("pageerror", (e) => hostErrors.push(`uncaught: ${e.message.slice(0, 200)}`));
// The console guards Activate with a confirm(). Playwright dismisses dialogs
// by default, which silently cancelled the activation — the click "worked"
// and no request was ever sent.
page.on("dialog", async (d) => {
  console.log(`hộp thoại: ${d.message()}`);
  await d.accept();
});

try {
  await page.goto("http://localhost:8090/", { waitUntil: "domcontentloaded" });
  await page.getByText(/More sign-in options/i).first().click();
  await page.locator('input[name="username"]').fill(env.HUB_TFL5_USER);
  await page.locator('input[type="password"]').first().fill(env.HUB_TFL5_PASSWORD);
  await page.getByRole("button", { name: /Sign in with password/i }).click();
  await page.getByText("huba", { exact: true }).first().waitFor({ timeout: 20000 });
  await page.getByText(/^Open$/).first().click();
  await page.getByText(/Manage access/i).first().waitFor({ timeout: 15000 });

  await page.locator("[data-goto-releases]").first().click();
  await page.getByText("UPLOAD A NEW BUNDLE").waitFor({ timeout: 15000 });
  const liveBefore = (await page.locator("text=/^Live: /").first().innerText()).trim();
  console.log("trước:", liveBefore);

  // Address the row by the button's own data attribute. Text-based locators
  // do not match this table at all (Playwright sees no visible text in it),
  // which cost two failed runs on 2026-08-07 — `[data-activate="<v>"]` is
  // both stable and unambiguous.
  const activateBtn = page.locator(`[data-activate="${version}"]`);
  versionExisted = await page.evaluate(
    (v) => [...document.querySelectorAll("tr")].some((r) => r.textContent.trim().startsWith(v)),
    version
  );
  if (versionExisted) {
    console.log(`${version} đã có trên máy chủ — bỏ qua bước tải lên`);
  } else {
    // Address the upload form by its own placeholders — the page has other
    // text inputs (Google client id, domains) and the Files tab has its own
    // "Extract ZIP…" input that index-based selectors would grab by mistake.
    await page.locator('input[placeholder="1.0.0"]').fill(version);
    await page.locator('input[placeholder="What changed in this release?"]').fill(notes);
    await page.locator('xpath=//input[@placeholder="1.0.0"]/following::input[@type="file"][1]')
      .setInputFiles(zipPath);
    await page.screenshot({ path: `${SHOTS}deploy-01-form.png`, fullPage: true });
    await page.getByRole("button", { name: /^Upload$/ }).click();
    await page.waitForTimeout(2500);
    await page.locator("#bundleReloadBtn").click();
    await activateBtn.waitFor({ timeout: 30000 });
  }
  // Already live? Activating again would push `previous` to this same
  // version and destroy the rollback target — so skip the activation, but do
  // NOT skip the verification below.
  //
  // This early `process.exit(0)` used to sit right here, and on 2026-08-08 it
  // paid for itself in the worst way: a v51 was already live, the local page
  // had been edited since (95069 → 95451 byte), the run skipped the upload
  // (bundles are immutable per version), skipped the activate, exited 0, and
  // never compared a single byte. "ĐẠT" for a deploy that shipped nothing.
  const alreadyLive =
    liveBefore.includes(`Live: ${version} `) || liveBefore.trim() === `Live: ${version}`;
  if (alreadyLive) {
    console.log(`${version} đang là bản LIVE — không activate lại (vẫn kiểm byte bên dưới)`);
  } else {
    await activateBtn.scrollIntoViewIfNeeded();
    // Read the server's answer to the activate call itself. Clicking and
    // hoping is how a failed activation gets reported as a successful deploy.
    const [activateRes] = await Promise.all([
      page.waitForResponse((r) => r.url().includes("/app/bundle/activate"), { timeout: 20000 }),
      activateBtn.click(),
    ]);
    const activateBody = await activateRes.text();
    console.log(`activate → HTTP ${activateRes.status()} ${activateBody.slice(0, 200)}`);
    if (activateRes.status() >= 400 || /"result"\s*:\s*false/.test(activateBody)) {
      problems.push(`máy chủ từ chối activate: HTTP ${activateRes.status()} ${activateBody.slice(0, 200)}`);
    }
    await page.waitForTimeout(2000);
    // Re-read on a fresh load. Navigate via the tab buttons (the banner's
    // "Manage versions" link is not clickable once a fresh bundle is live).
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.getByRole("button", { name: "Deploy", exact: true }).click();
    await page.getByRole("button", { name: "Releases", exact: true }).click();
    await page.getByText("UPLOAD A NEW BUNDLE").waitFor({ timeout: 15000 });
    const liveAfter = (await page.locator("text=/^Live: /").first().innerText()).trim();
    console.log("sau:  ", liveAfter);
    await page.screenshot({ path: `${SHOTS}deploy-02-activated.png`, fullPage: true });
    if (!liveAfter.includes(version)) problems.push(`bảng Releases vẫn không báo ${version} là LIVE: "${liveAfter}"`);
  }
} catch (e) {
  // Keep enough of the message to see WHY an action failed (intercepted by
  // another element, outside the viewport, …) — one line hides exactly that.
  problems.push(`ngoại lệ: ${e.message.split("\n").slice(0, 6).join(" | ")}`);
} finally {
  await browser.close();
}

// Independent check: fetch what a visitor gets and compare it byte-for-byte
// with what we just shipped. Also fetch every sibling asset — a bundle that
// serves index.html but 404s echarts.min.js is a half-deploy.
try {
  const res = await fetch(`http://${APP_TID}.test.localhost:8090/`, { redirect: "follow" });
  const body = await res.text();
  console.log(`trang công khai: HTTP ${res.status}, ${body.length} byte`);
  if (body.trim() !== source.trim()) {
    problems.push(
      `trang đang phục vụ KHÁC với fe/index.html vừa đóng gói (${body.length} vs ${source.length} byte)` +
        (versionExisted
          ? ` — tên "${version}" ĐÃ tồn tại trên máy chủ nên bản mới không được nhận; chạy lại với tên version MỚI`
          : "")
    );
  }
  for (const a of assets.map((p) => p.split("/").pop()).filter((n) => n !== "index.html")) {
    const r = await fetch(`http://${APP_TID}.test.localhost:8090/${a}`);
    console.log(`  ${a}: HTTP ${r.status}`);
    if (!r.ok) problems.push(`tài nguyên ${a} không phục vụ được (HTTP ${r.status})`);
  }
} catch (e) {
  problems.push(`không tải được trang công khai: ${e.message}`);
}

if (hostErrors.length) {
  console.log("\nGHI CHÚ — lỗi console của chính trang admin tfl5 (không phải bundle huba):");
  [...new Set(hostErrors)].forEach((h) => console.log("  · " + h));
}
if (problems.length) {
  console.log("\nVẤN ĐỀ:");
  problems.forEach((p) => console.log("  - " + p));
  process.exit(1);
}
console.log(`\nĐẠT: ${version} đang phục vụ thật tại http://${APP_TID}.test.localhost:8090/`);

// ẢNH CHỤP NGAY SAU MỖI LẦN DEPLOY — không chờ ai nhớ ra phải nhìn.
//
// Hà 2026-08-10: *"sao không sử dụng tool chụp ảnh để phân tích"*. Đúng, và
// hôm ấy có hai bằng chứng: lời cảnh báo "ảnh chụp cũ" bị CẮT CỤT trong khi
// 7/7 assert vẫn xanh (chúng đọc `textContent`, thứ có đủ chữ kể cả khi màn
// cắt), và dòng "đang làm gì" nằm TRÊN tên phiên. Cả hai chỉ lộ khi mở ảnh ra.
//
// Assert chỉ kiểm được thứ mình NGHĨ RA để kiểm; bức ảnh cho thấy thứ mình
// không nghĩ tới. Nên việc nhìn phải là một bước cơ học của deploy, không phải
// một thói quen tốt. `fe-shots` chỉ đọc, không gọi `claude`, chạy bao nhiêu lần
// cũng không tốn hạn mức — không có lý do gì để bỏ.
try {
  const { spawnSync } = await import("node:child_process");
  const tag = `after-${version}`;
  // Chụp bằng TÀI KHOẢN CHỦ (`alice_local`), không phải tài khoản bot dùng cho
  // console admin: màn của chủ máy mới là màn cần nhìn.
  const r = spawnSync(
    "node",
    [HERE + "fe-shots.mjs", APP_TID, "alice_local", env.HUB_TFL5_ALICE_PASSWORD || "", tag],
    {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    }
  );
  if (r.status === 0) {
    console.log(`\nẢnh 5 màn sau deploy: ui-shots/${tag}-*.png — MỞ RA NHÌN trước khi nói xong.`);
  } else {
    // Thử LẠI một lần: bundle vừa đổi xong, trang có nhịp chuyển và lượt chụp
    // đầu tiên có thể rơi đúng vào đó (gặp thật ở v141, chạy tay ngay sau đó thì
    // exit 0). Một lần thử lại rẻ hơn hẳn việc bỏ luôn thói quen nhìn.
    const again = spawnSync(
      "node",
      [HERE + "fe-shots.mjs", APP_TID, "alice_local", env.HUB_TFL5_ALICE_PASSWORD || "", tag],
      { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }
    );
    if (again.status === 0) {
      console.log(`\nẢnh 5 màn sau deploy (lượt thử lại): ui-shots/${tag}-*.png — MỞ RA NHÌN.`);
    } else {
      // In LÝ DO, không chỉ mã thoát. Một dòng "exit 1" trần trụi là đúng thứ
      // dự án này gọi là lỗi im lặng — nó không nói được vì sao để mà sửa.
      const why = `${again.stdout || ""}${again.stderr || ""}`.trim().split("\n").slice(-6).join("\n  ");
      console.log(`\n⚠ không chụp được ảnh sau deploy (exit ${again.status}) — hãy tự chạy fe-shots.mjs.\n  ${why}`);
    }
  }
} catch (e) {
  console.log(`\n⚠ không chụp được ảnh sau deploy: ${e.message}`);
}
