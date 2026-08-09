// UC-S06 (mở phiên nền) + UC-S05 (nói tiếp vào chính phiên đó) + dừng phiên,
// nghiệm thu trên UI THẬT ở 390px. Kèm UC-S09 nửa "ảnh chụp còn tươi".
//
// Phép đo phân biệt hai UC dễ lẫn nhau — và phải NGƯỢC nhau:
//   · UC-S05b "hỏi bên lề" → nhật ký phiên gốc KHÔNG đổi một byte (fork).
//   · UC-S05 "nói tiếp"    → nhật ký phiên gốc DÀI RA, và session_id KHÔNG đổi.
// Nếu cả hai cùng cho một kết quả thì một trong hai đang làm sai việc của nó.
//
// Usage: node fe-newsession-uc.mjs <app_tid> <username> <password> [project]

import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { execFileSync } from "node:child_process";
import { mkdirSync, statSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const [appTid, username, password, project = "hub-act-demo"] = process.argv.slice(2);
if (!appTid || !username || !password) {
  console.error("usage: node fe-newsession-uc.mjs <app_tid> <username> <password> [project]");
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

function transcriptOf(sessionId) {
  const root = join(homedir(), ".claude", "projects");
  for (const dir of readdirSync(root)) {
    const p = join(root, dir, `${sessionId}.jsonl`);
    try {
      statSync(p);
      return p;
    } catch {
      /* not here */
    }
  }
  return null;
}
const sizeOf = (p) => {
  try {
    return statSync(p).size;
  } catch {
    return 0;
  }
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

let started = null;
try {
  await page.goto(BASE, { waitUntil: "domcontentloaded" });
  await page.fill("#u", username);
  await page.fill("#p", password);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 20000 });
  await page.waitForFunction(() => document.querySelectorAll("#sessList .sess").length > 0, {
    timeout: 25000,
  });

  // UC-S09, nửa "còn tươi": trang phải nói ảnh chụp CŨ BAO LÂU, không chỉ mấy
  // giờ mấy phút — một mốc giờ trần trụi đọc như "bây giờ" với người không ngồi
  // trừ nhẩm, và đó đúng là lúc hub chết thì màn vẫn trông như thật.
  const stamp0 = await page.textContent("#boardStamp");
  check("trang nói ảnh chụp cũ bao lâu", /trước\)/.test(stamp0), stamp0.trim().slice(0, 70));
  check(
    "ảnh chụp đang tươi thì KHÔNG kêu cảnh báo",
    !(await page.locator("#boardStamp").evaluate((el) => el.classList.contains("stale"))),
    stamp0.trim().slice(0, 40)
  );

  const before = new Set(hub(["sessions", "--json"]).sessions.map((s) => s.session_id));

  // ——— UC-S06: mở phiên làm việc mới ———
  await page.click("#sessNewBox summary");
  const options = await page.$$eval("#sessNewProject option", (os) => os.map((o) => o.value));
  check("ô chọn dự án có danh sách thật", options.length > 0, `${options.length} dự án`);
  check(`danh sách có '${project}'`, options.includes(project), options.slice(0, 6).join(", "));

  await page.selectOption("#sessNewProject", project);
  await page.fill("#sessNewTask", "Đọc README.md và tóm tắt trong đúng một câu. Không sửa file.");
  await page.click("#sessNew");
  check("ô việc được dọn sau khi gửi", (await page.inputValue("#sessNewTask")) === "");

  // Phiên MỚI phải hiện ra mà không cần đụng vào máy tính — HOẶC hub phải nói
  // thẳng là nó kẹt. Trên máy này mọi dự án đều nằm dưới một `.mcp.json`, nên
  // phiên nền dựng lên sẽ dừng ở hộp thoại duyệt MCP; điều KHÔNG được phép là
  // hub báo "đã mở phiên" cho một phiên chẳng bao giờ làm gì.
  // Chờ CÂU TRẢ LỜI CỦA HUB trên màn, không rình danh sách phiên. Rình danh
  // sách là đua với chính hub: nó mất ~14s để xem phiên có kẹt không rồi mới
  // dừng, nên có cửa sổ mà phiên vừa hiện vừa sắp bị giết — bản trước báo
  // "phiên mới xuất hiện ✓" cho một phiên vài giây sau không còn.
  //
  // Đọc bằng evaluate + optional chaining: `page.textContent` CHỜ phần tử xuất
  // hiện, nên hỏi một id không tồn tại là treo 30 giây rồi ném. (`cmdNote` là
  // HÀM; nó ghi vào `#cmdStatus`.)
  let verdict = "";
  for (let i = 0; i < 60; i++) {
    verdict = await page.evaluate(
      () => document.getElementById("sessNewNote")?.textContent || ""
    );
    if (verdict && !/^Đang mở/.test(verdict)) break;
    await page.waitForTimeout(3000);
  }
  console.log(`\nhub trả lời: ${verdict.slice(0, 160)}\n`);
  const openedOk = /^🚀/.test(verdict);
  let fresh = openedOk
    ? hub(["sessions", "--json"]).sessions.find((s) => !before.has(s.session_id))
    : null;
  const blocked = openedOk ? "" : verdict;

  if (!fresh) {
    // Đường KẸT: nghiệm thu ở đây là "hub nói đúng sự thật", không phải "có phiên".
    check("hub KHÔNG báo mở thành công khi phiên kẹt", !/🚀/.test(blocked), blocked.slice(0, 80));
    check("hub nói rõ kẹt vì đâu", /MCP/.test(blocked), blocked.slice(0, 160));
    check("hub chỉ ra cách gỡ một lần trên máy", /claude/.test(blocked) && /Enter|Esc/.test(blocked));
    check(
      "không để lại phiên kẹt lơ lửng",
      !hub(["sessions", "--json"]).sessions.some((s) => !before.has(s.session_id))
    );
    console.log("\n  · máy này chưa duyệt MCP cho dự án đó ⇒ chỉ nghiệm thu được ĐƯỜNG KẸT");
    console.log("  · duyệt một lần rồi chạy lại để kiểm đường thành công:");
    console.log(`      cd ~/Documents/projects/AI/${project} && claude   → Esc → thoát`);
    throw new Error("__blocked_path_done__");
  }
  check("phiên mới xuất hiện mà không cần chạm vào máy", true, fresh.session_id.slice(0, 8));
  started = fresh;
  check("phiên mới là phiên NỀN", fresh.kind === "background", fresh.kind);
  check(
    "phiên mới nằm đúng thư mục dự án",
    (fresh.cwd || "").endsWith(project),
    fresh.cwd
  );

  // Màn phải thấy nó, không chỉ máy thấy.
  await page.waitForFunction(
    (id) => !!document.querySelector(`.sess[data-session="${id}"]`),
    fresh.session_id,
    { timeout: 120000, polling: 2000 }
  );
  check("phiên mới hiện trên màn danh sách", true, fresh.name || "");

  // Phiên vừa mở phải TỰ NÓI nó do hub mở — đó là điểm khác biệt duy nhất giữa
  // nó và một `claude --bg` gõ tay, và là thứ quyết định người dùng có nói tiếp
  // / dừng được từ điện thoại hay không.
  const newGroup = await page.evaluate((id) => {
    // Đi ngược lên từ thẻ tới tiêu đề nhóm gần nhất — đúng thứ người dùng thấy
    // phía trên nó.
    let el = document.querySelector(`.sess[data-session="${id}"]`);
    while (el && !(el.classList && el.classList.contains("sess-group"))) el = el.previousElementSibling;
    return el ? { g: el.dataset.g, text: el.textContent.replace(/\s+/g, " ").trim() } : null;
  }, fresh.session_id);
  check("phiên mới nằm dưới tiêu đề 'hub mở từ điện thoại'",
    !!newGroup && newGroup.g === "hub" && /hub mở/.test(newGroup.text),
    newGroup ? newGroup.text : "(không thấy tiêu đề nhóm)");

  await page.locator(`.sess[data-session="${fresh.session_id}"]`).click();
  check("mở được màn chi tiết của phiên mới", await page.locator("#sessDetail").isVisible());
  check("phiên do hub mở thì CÓ nút Dừng", await page.locator("#sessStop").isVisible());
  check("phiên do hub mở thì CÓ ô nói tiếp", await page.locator("#sessTellRow").isVisible());

  const file = transcriptOf(fresh.session_id);
  check("phiên mới có nhật ký trên đĩa", !!file, file || "");

  // ——— Dừng phiên: claude không cho nối vào phiên đang chạy ———
  await page.click("#sessStop");
  await page.waitForFunction(
    () => {
      const t = document.getElementById("sessTellBox").textContent || "";
      return t.length > 0 && !t.startsWith("Đang dừng");
    },
    null,
    { timeout: 180000, polling: 1000 }
  ).catch(() => {});
  const stopMsg = await page.textContent("#sessTellBox");
  check("màn báo đã dừng phiên", /Đã dừng phiên/.test(stopMsg), stopMsg.trim().slice(0, 70));
  check("dừng rồi thì hội thoại vẫn còn", sizeOf(file) > 0, `${sizeOf(file)} byte`);

  // ——— UC-S05: nói tiếp vào CHÍNH phiên đó ———
  const sizeBefore = sizeOf(file);
  await page.fill("#sessTellInput", "Nội dung README bạn vừa đọc nói về cái gì? Trả lời một dòng.");
  await page.click("#sessTell");
  await page.waitForFunction(
    () => {
      const t = document.getElementById("sessTellBox").textContent || "";
      return t.length > 0 && !t.startsWith("Đang nói tiếp");
    },
    null,
    { timeout: 240000, polling: 1000 }
  ).catch(() => {});
  const tellMsg = await page.textContent("#sessTellBox");
  const sizeAfter = sizeOf(file);

  check("màn báo đã nói tiếp", /Đã nói tiếp vào phiên/.test(tellMsg), tellMsg.trim().slice(0, 70));
  // ĐÂY là chỗ UC-S05 khác hẳn UC-S05b: lượt này ở LẠI trên phiên cũ.
  check(
    "nhật ký phiên DÀI RA (khác hẳn hỏi bên lề)",
    sizeAfter > sizeBefore,
    `${sizeBefore} → ${sizeAfter} byte`
  );
  const stillThere = hub(["sessions", "--json"]).sessions.find(
    (s) => s.session_id === fresh.session_id
  );
  check(
    "KHÔNG đẻ ra phiên mới ngoài ý muốn",
    !!stillThere || sizeAfter > sizeBefore,
    stillThere ? "phiên cũ vẫn là phiên cũ" : "(đã rời danh sách sống, nhật ký vẫn là của nó)"
  );

  const over = await page.evaluate(() => ({
    w: document.documentElement.scrollWidth,
    inner: window.innerWidth,
  }));
  check("trang không tràn ngang", over.w <= over.inner + 1, `${over.w}/${over.inner}`);
  await page.screenshot({ path: `${SHOTS}newsession-01-phone.png`, fullPage: true });
} catch (e) {
  // Đường kẹt đã kiểm xong bằng các assert riêng của nó — không tính là lỗi.
  if (e.message !== "__blocked_path_done__") problems.push(`ngoại lệ: ${e.message}`);
} finally {
  await browser.close();
  // Đừng để lại một agent chạy hoang sau khi kiểm xong.
  if (started) {
    try {
      execFileSync("claude", ["stop", started.session_id.slice(0, 8)], { stdio: "ignore" });
      console.log(`\n(dọn: đã dừng ${started.session_id.slice(0, 8)})`);
    } catch {
      console.log(`\n⚠ dọn không xong — tự dừng bằng: claude stop ${started.session_id.slice(0, 8)}`);
    }
  }
}

const failed = checks.filter((c) => !c.ok).length;
console.log(`\n${checks.length - failed}/${checks.length} đạt`);
if (problems.length) {
  console.log("\nVẤN ĐỀ:");
  problems.forEach((p) => console.log(`  · ${p}`));
}
process.exit(problems.length ? 1 : 0);
