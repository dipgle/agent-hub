// UC-S05b mức 2 — "chen ngang hỏi mà KHÔNG phá ngữ cảnh đang làm", nghiệm thu
// trên UI THẬT ở 390px.
//
// Đi trọn đường người dùng: đăng nhập → chạm một phiên trong danh sách → gõ câu
// hỏi vào ô trên màn phiên → bấm Hỏi → hub fork phiên đó và trả lời về màn.
// Không goto, không gọi API dựng trạng thái.
//
// Phép đo QUAN TRỌNG NHẤT ở đây không phải "có câu trả lời không" mà là "phiên
// gốc có bị đụng không" — đó mới là lời hứa của UC. Nên script đọc thẳng tệp
// nhật ký của phiên gốc trước và sau: byte, số dòng, mtime. Một câu trả lời
// đúng mà phiên gốc bị thêm lượt là UC HỎNG, không phải UC đạt.
//
// Usage: node fe-aside-uc.mjs <app_tid> <username> <password>

import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { execFileSync } from "node:child_process";
import { mkdirSync, statSync, readFileSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const [appTid, username, password] = process.argv.slice(2);
if (!appTid || !username || !password) {
  console.error("usage: node fe-aside-uc.mjs <app_tid> <username> <password>");
  process.exit(2);
}
const BASE = `http://${appTid}.test.localhost:8090`;
const HERE = new URL("./", import.meta.url).pathname;
const SHOTS = HERE + "ui-shots/";
mkdirSync(SHOTS, { recursive: true });

const problems = [];
const checks = [];
const check = (name, ok, detail = "") => {
  checks.push({ name, ok });
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) problems.push(`${name}${detail ? `: ${detail}` : ""}`);
};

// ── CỔNG GIÁ ────────────────────────────────────────────────────────────────
// Bước dưới đây gọi `claude` THẬT trên bản fork, và giá tỉ lệ độ dài nhật ký
// (đo thật: 0.986 MB → $1.72). Đêm 2026-08-08 kịch bản này chạy 5 lượt cho cùng
// một bằng chứng và tiêu $5.65 — tiền của kịch bản, không phải của hub: vòng
// chạy của hub từ hôm nay là $0.
//
// Nên: ước tính TRƯỚC, và nếu vượt trần thì KHÔNG gọi. Bỏ qua được ghi rõ ra
// màn và đếm vào ô "bỏ qua" — một kịch bản lặng lẽ nhảy cóc rồi báo xanh còn
// tệ hơn một kịch bản đắt.
const USD_PER_MB = 1.75;                      // mốc đo thật 2026-08-08
const MAX_USD = Number(process.env.HUB_UC_MAX_USD || 0.25);
const PAY = process.env.HUB_UC_PAY === "1";   // chấp nhận trả tiền lượt này
const skipped = [];
const affordable = (mb, what) => {
  const est = mb * USD_PER_MB;
  if (PAY || est <= MAX_USD) return true;
  const msg = `${what}: ước tính $${est.toFixed(2)} > trần $${MAX_USD.toFixed(2)}`;
  skipped.push(msg);
  console.log(`\n⏭  BỎ QUA (không gọi claude) — ${msg}`);
  console.log(`   Muốn nghiệm thu lại đường tốn tiền: HUB_UC_PAY=1 node ${process.argv[1].split("/").pop()} …\n`);
  return false;
};

const hub = (args) =>
  JSON.parse(
    execFileSync(HERE + "rust/target/release/hub", args, {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    })
  );

/// The transcript file for a session id. Searched rather than built from `cwd`:
/// a session can be opened from one folder and recorded under another slug, and
/// guessing wrong here would silently measure a file nobody is writing to —
/// which would make "phiên gốc không đổi" pass for the wrong reason.
function transcriptOf(sessionId) {
  const root = join(homedir(), ".claude", "projects");
  for (const dir of readdirSync(root)) {
    const p = join(root, dir, `${sessionId}.jsonl`);
    try {
      statSync(p);
      return p;
    } catch {
      /* not in this project folder */
    }
  }
  return null;
}

const fingerprint = (path) => {
  const st = statSync(path);
  return {
    bytes: st.size,
    mtimeMs: st.mtimeMs,
    lines: readFileSync(path, "utf8").split("\n").filter(Boolean).length,
  };
};

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({
  viewport: { width: 390, height: 844 },
  deviceScaleFactor: 3,
  isMobile: true,
  hasTouch: true,
});
page.on("console", (m) => {
  if (m.type() === "error") problems.push(`console.error: ${m.text()}`);
});
page.on("pageerror", (e) => problems.push(`uncaught: ${e.message}`));

try {
  await page.goto(BASE, { waitUntil: "domcontentloaded" });
  await page.fill("#u", username);
  await page.fill("#p", password);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 20000 });
  await page.waitForFunction(() => document.querySelectorAll("#sessList .sess").length > 0, {
    timeout: 25000,
  });

  // Một phiên ĐỨNG YÊN. Với phiên đang chạy thì "tệp không đổi" là phép đo mù:
  // nó tự đổi vì phiên vẫn làm việc, và script sẽ đổ lỗi cho tính năng.
  const truth = hub(["sessions", "--json"]);
  const idleFor = (s) =>
    s.last_activity ? (Date.now() - Date.parse(s.last_activity)) / 60000 : Infinity;
  // Trong các phiên đứng yên, chọn phiên NHẬT KÝ NGẮN NHẤT. `--resume` nạp cả
  // hội thoại nên giá tỉ lệ độ dài (đo thật: 0.986 MB → $1.72); hỏi trên phiên
  // 20 MB thì mỗi lần chạy kịch bản tự đốt ~$36 mà không kiểm thêm được gì.
  const pool = truth.sessions.filter((s) => s.last_text && !s.note && idleFor(s) > 30);
  const sized = (pool.length ? pool : truth.sessions.filter((s) => s.last_text && !s.note))
    .map((s) => ({ s, file: transcriptOf(s.session_id) }))
    .filter((x) => x.file)
    .map((x) => ({ ...x, mb: statSync(x.file).size / 1e6 }))
    .sort((a, b) => a.mb - b.mb);
  if (!sized.length) throw new Error("không có phiên đứng yên nào để hỏi — dừng, không đo bừa");
  const { s: target, file, mb } = sized[0];
  console.log(`chạm vào phiên: ${target.name} (${target.account}, đứng yên ${Math.round(idleFor(target))} phút)`);
  console.log(`nhật ký: ${file}`);
  console.log(`kích thước: ${mb.toFixed(2)} MB ≈ $${(mb * USD_PER_MB).toFixed(2)} cho một câu hỏi\n`);

  const before = fingerprint(file);
  const activityBefore = target.last_activity;

  await page.locator(`.sess[data-session="${target.session_id}"]`).click();
  check("chạm vào phiên thì mở màn chi tiết", await page.locator("#sessDetail").isVisible());
  check("màn phiên có ô hỏi bên lề", await page.locator("#sessAskInput").isVisible());
  check("màn nói rõ phiên gốc không bị đụng", (await page.locator("#sessAskHint").textContent()).includes("không bị đụng"));

  // Chờ luồng về trước đã, để chắc hub đã theo đúng phiên này.
  await page.waitForFunction(
    () => document.querySelectorAll("#sessStream .ev").length > 0,
    null,
    { timeout: 180000, polling: 1000 }
  );

  // Ảnh chụp KHÔNG mang số tiền nào (gỡ 2026-08-08). Đòi vắng mặt, vì thứ này
  // đã mọc lại một lần: trần → giá.
  const snap = hub(["portal-push", "--dry-run"]);
  check(
    "ảnh chụp KHÔNG mang số tiền nào",
    snap.owner_spend === undefined && snap.owner_budget === undefined
  );
  const asideBefore = snap.sessions.aside?.ts || "";

  // Câu hỏi chỉ trả lời được nếu CÓ ngữ cảnh phiên gốc — nếu không thì "trả lời
  // trôi chảy" chẳng chứng minh điều gì về fork cả.
  const question = "Tóm tắt trong 1 câu: phiên này đang làm việc gì?";
  // Cổng giá đứng TRƯỚC cú bấm, không phải sau: bản đầu chỉ in ước tính rồi vẫn
  // bấm, và lượt chạy đó tiêu $1.0969 trong lúc đang sửa đúng cái lỗ ấy.
  const willPay = affordable(mb, "UC-S05b /ask");
  // Khai báo NGOÀI khối: các phép đo bên dưới ("phiên gốc y nguyên") vẫn chạy
  // khi bỏ qua bước hỏi, và chúng đọc mấy biến này.
  let a = null;
  let shown = { box: "", note: "" };
  if (willPay) {
  await page.fill("#sessAskInput", question);
  await page.click("#sessAsk");
  check("ô hỏi được dọn sau khi gửi", (await page.inputValue("#sessAskInput")) === "");

  // Hai bước, vì màn có HAI trạng thái và chỉ trạng thái sau mới là cái người
  // dùng rốt cuộc nhìn thấy: lời đáp về qua phòng chat trước (nhanh, thô), rồi
  // ảnh chụp kế tiếp thay bằng thẻ đầy đủ (câu hỏi + bản fork + giá). Đọc ngay
  // sau bước 1 là đo một màn hình đang trên đường đi.
  await page.waitForFunction(
    () => {
      const t = document.getElementById("sessAskBox").textContent || "";
      return t.length > 0 && !t.startsWith("Đang hỏi");
    },
    null,
    { timeout: 180000, polling: 1000 }
  ).catch(() => {});
  // Chờ câu trả lời MỚI, không chờ "có chữ trên màn". Kịch bản hỏi lại đúng câu
  // cũ, nên đáp án của lần chạy trước cũng thoả mọi phép so chuỗi — bản đầu rơi
  // đúng bẫy đó và tưởng đã đo xong trong khi hub còn đang nghĩ.
  let after = null;
  for (let i = 0; i < 60; i++) {
    after = hub(["portal-push", "--dry-run"]);
    if ((after.sessions.aside?.ts || "") !== asideBefore) break;
    await page.waitForTimeout(3000);
  }
  a = after.sessions.aside;
  // Rồi mới chờ MÀN bắt kịp đúng câu trả lời ấy (nhận diện bằng bản fork).
  if (a && a.new_session_id) {
    await page.waitForFunction(
      (fork) => (document.getElementById("sessAskBox").textContent || "").includes(fork),
      a.new_session_id.slice(0, 8),
      { timeout: 60000, polling: 1000 }
    ).catch(() => {});
  }
  shown = await page.evaluate(() => ({
    box: document.getElementById("sessAskBox").textContent || "",
    note: document.getElementById("cmdNote")?.textContent || "",
  }));

  }

  // ——— LỜI HỨA CỦA UC, đo trên tệp thật ———
  // Chạy cả khi bỏ qua bước hỏi: "phiên gốc y nguyên" vẫn là điều phải đúng, và
  // đo nó không tốn đồng nào.
  const now = fingerprint(file);
  check(
    "phiên gốc không thêm một byte nào",
    now.bytes === before.bytes && now.lines === before.lines,
    `${before.bytes}→${now.bytes} byte · ${before.lines}→${now.lines} dòng`
  );
  check("phiên gốc không bị ghi lại (mtime y nguyên)", now.mtimeMs === before.mtimeMs);
  const liveNow = hub(["sessions", "--json"]).sessions.find((s) => s.session_id === target.session_id);
  check(
    "phiên gốc không nhảy lên 'vừa động'",
    !liveNow || liveNow.last_activity === activityBefore,
    `${activityBefore} → ${liveNow ? liveNow.last_activity : "(không còn trong danh sách)"}`
  );

  // Các phép đo dưới đây chỉ có nghĩa khi thật sự đã hỏi. Khi cổng giá chặn,
  // chúng KHÔNG được tính là đạt — bỏ qua thì phải hiện ra là bỏ qua.
  if (willPay) {
  check("có câu hỏi bên lề MỚI trong ảnh chụp", !!a && a.ts !== asideBefore && a.source_id === target.session_id);
  check("câu hỏi được giữ nguyên văn", a && a.question === question, a ? a.question : "");
  check("có câu trả lời", !!(a && a.answer && a.answer.length > 0), a ? a.answer.slice(0, 80) : "");
  check(
    "trả lời nằm ở phiên KHÁC (fork), không phải phiên gốc",
    !!(a && a.new_session_id && a.new_session_id !== a.source_id),
    a ? `${a.source_id.slice(0, 8)} → ${a.new_session_id.slice(0, 8)}` : ""
  );
  check("màn hiện câu trả lời", shown.box.includes("phiên gốc không thêm lượt nào"), shown.box.slice(0, 80));
  check("màn hiện câu mình đã hỏi", shown.box.includes(question.slice(0, 20)));
  check(
    "màn KHÔNG hiện con số tiền nào",
    !/\$\s?\d/.test(shown.box),
    shown.box.slice(-60)
  );
  }


  const over = await page.evaluate(() => ({
    w: document.documentElement.scrollWidth,
    inner: window.innerWidth,
  }));
  check("trang không tràn ngang", over.w <= over.inner + 1, `${over.w}/${over.inner}`);
  await page.screenshot({ path: `${SHOTS}aside-01-phone.png`, fullPage: true });

  // Quay lại phải dọn câu trả lời — câu trả lời của phiên này nằm trên màn của
  // phiên khác thì người đọc sẽ hiểu nhầm là của phiên đang mở.
  await page.click("#sessBack");
  await page.waitForTimeout(500);
  const other = truth.sessions.find((s) => s.session_id !== target.session_id);
  if (other) {
    await page.locator(`.sess[data-session="${other.session_id}"]`).click();
    check(
      "mở phiên khác thì câu trả lời cũ biến mất",
      await page.locator("#sessAskBox").isHidden()
    );
  }
} catch (e) {
  problems.push(`ngoại lệ: ${e.message}`);
} finally {
  await browser.close();
}

const failed = checks.filter((c) => !c.ok).length;
console.log(
  `\n${checks.length - failed}/${checks.length} đạt` +
  (skipped.length ? ` · ${skipped.length} BỎ QUA vì tốn tiền` : "")
);
if (skipped.length) {
  console.log("\nCHƯA NGHIỆM THU (không gọi claude lượt này):");
  skipped.forEach((s) => console.log(`  ⏭ ${s}`));
  console.log("  → chạy lại với HUB_UC_PAY=1 khi cần bằng chứng đường tốn tiền.");
}
if (problems.length) {
  console.log("\nVẤN ĐỀ:");
  problems.forEach((p) => console.log(`  · ${p}`));
}
process.exit(problems.length ? 1 : 0);
