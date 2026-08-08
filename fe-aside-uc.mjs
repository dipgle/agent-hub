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
  const target =
    truth.sessions.find((s) => s.last_text && !s.note && idleFor(s) > 30) ||
    truth.sessions.find((s) => s.last_text && !s.note);
  if (!target) throw new Error("không có phiên đứng yên nào để hỏi — dừng, không đo bừa");
  const file = transcriptOf(target.session_id);
  if (!file) throw new Error(`không tìm thấy nhật ký của phiên ${target.session_id}`);
  console.log(`chạm vào phiên: ${target.name} (${target.account}, đứng yên ${Math.round(idleFor(target))} phút)`);
  console.log(`nhật ký: ${file}\n`);

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

  // Trần của CHỦ MÁY quyết định nhánh nào chạy được hôm nay. Đọc thẳng kết luận
  // sản phẩm công bố, không tự suy lại luật.
  const snap = hub(["portal-push", "--dry-run"]);
  const owner = snap.owner_budget || {};
  const blocked = owner.blocks_owner_action === true;
  check(
    "ảnh chụp công bố trần của chủ máy",
    typeof owner.blocks_owner_action === "boolean",
    `đã dùng $${Number(owner.spent_usd ?? 0).toFixed(3)}/$${Number(owner.cap_usd ?? 0).toFixed(2)} · chặn: ${blocked}`
  );
  const asideBefore = snap.sessions.aside?.ts || "";

  // Câu hỏi chỉ trả lời được nếu CÓ ngữ cảnh phiên gốc — nếu không thì "trả lời
  // trôi chảy" chẳng chứng minh điều gì về fork cả.
  const question = "Tóm tắt trong 1 câu: phiên này đang làm việc gì?";
  await page.fill("#sessAskInput", question);
  await page.click("#sessAsk");
  check("ô hỏi được dọn sau khi gửi", (await page.inputValue("#sessAskInput")) === "");

  await page.waitForFunction(
    () => {
      const t = document.getElementById("sessAskBox").textContent || "";
      return t.length > 0 && !t.startsWith("Đang hỏi");
    },
    null,
    { timeout: 180000, polling: 1000 }
  ).catch(() => {});
  const shown = await page.evaluate(() => ({
    box: document.getElementById("sessAskBox").textContent || "",
    note: document.getElementById("cmdNote")?.textContent || "",
  }));

  const after = hub(["portal-push", "--dry-run"]);
  const a = after.sessions.aside;

  // ——— LỜI HỨA CỦA UC, đo trên tệp thật, chạy ở CẢ HAI nhánh ———
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

  if (blocked) {
    // Hết trần: hub phải TỪ CHỐI và nói rõ, không được im lặng tiêu tiếp.
    check(
      "hết ngân sách thì KHÔNG hỏi lén",
      (a?.ts || "") === asideBefore,
      (a?.ts || "") === asideBefore ? "không sinh câu hỏi mới — đúng" : `sinh lúc ${a.ts}`
    );
    check(
      "màn nói rõ lý do từ chối, không vờ như xong",
      /ngân sách/i.test(shown.box + shown.note),
      (shown.box + " " + shown.note).slice(0, 120)
    );
    console.log("\n  · ngân sách chủ máy không đủ để kiểm đường THÀNH CÔNG của UC-S05b hôm nay");
  } else {
    check("có câu hỏi bên lề MỚI trong ảnh chụp", !!a && a.ts !== asideBefore && a.source_id === target.session_id);
    check("câu hỏi được giữ nguyên văn", a && a.question === question, a ? a.question : "");
    check("có câu trả lời", !!(a && a.answer && a.answer.length > 0), a ? a.answer.slice(0, 80) : "");
    check(
      "trả lời nằm ở phiên KHÁC (fork), không phải phiên gốc",
      !!(a && a.new_session_id && a.new_session_id !== a.source_id),
      a ? `${a.source_id.slice(0, 8)} → ${a.new_session_id.slice(0, 8)}` : ""
    );
    check("tiền vào sổ chi", !!(a && a.cost_usd > 0), a ? `$${a.cost_usd}` : "");
    check("màn hiện câu trả lời", shown.box.includes("phiên gốc không thêm lượt nào"), shown.box.slice(0, 80));
    check("màn hiện câu mình đã hỏi", shown.box.includes(question.slice(0, 20)));

    // Sổ chi phải cộng dồn cả hai đường tiêu của chủ máy.
    const spentAfter = Number(after.owner_budget?.spent_usd ?? 0);
    check(
      "trần chủ máy đã tính khoản vừa tiêu",
      spentAfter >= Number(owner.spent_usd ?? 0) + Number(a?.cost_usd ?? 0) - 1e-6,
      `$${Number(owner.spent_usd ?? 0).toFixed(4)} → $${spentAfter.toFixed(4)}`
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
console.log(`\n${checks.length - failed}/${checks.length} đạt`);
if (problems.length) {
  console.log("\nVẤN ĐỀ:");
  problems.forEach((p) => console.log(`  · ${p}`));
}
process.exit(problems.length ? 1 : 0);
