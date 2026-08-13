// UC-S06 (mở phiên nền) + UC-S05 (nói tiếp vào chính phiên đó) + dừng phiên,
// nghiệm thu trên UI THẬT ở 390px. Kèm UC-S09 nửa "ảnh chụp còn tươi".
//
// Phép đo phân biệt hai UC dễ lẫn nhau — và phải NGƯỢC nhau:
//   · UC-S05b "hỏi bên lề" → nhật ký phiên gốc KHÔNG đổi một byte (fork).
//   · UC-S05 "nói tiếp"    → nhật ký phiên gốc DÀI RA, và session_id KHÔNG đổi.
// Nếu cả hai cùng cho một kết quả thì một trong hai đang làm sai việc của nó.
//
// Usage: node fe-newsession-uc.mjs <app_tid> <username> <password> [project]

import { chromium } from "/Users/hanguyen/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { execFileSync } from "node:child_process";
import { mkdirSync, statSync, readdirSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

const [appTid, username, password, project = "hub-act-demo"] = process.argv.slice(2);
if (!appTid || !username || !password) {
  console.error("usage: node fe-newsession-uc.mjs <app_tid> <username> <password> [project]");
  process.exit(2);
}
const BASE = `http://${appTid}.test.localhost:8090`;
const HERE = new URL("./", import.meta.url).pathname;
// Gốc workspace TỰ ĐỊNH VỊ: `<workspace>/hub/` → `<workspace>`. Gõ cứng
// `~/Documents/projects` ở đây thì phép đo "phiên mở ở gốc workspace" hoá đỏ
// ngay hôm gốc dời sang `~/projects` (2026-08-12) — đỏ vì phép đo, không vì sản
// phẩm, và đó là loại đỏ dạy người ta bỏ qua màu đỏ.
const WORKSPACE = resolve(HERE, "..");
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
/// Lượt NGƯỜI DÙNG đầu tiên trong nhật ký — tức đề bài đã tới phiên hay chưa.
///
/// Đọc thẳng tệp thay vì hỏi `hub sessions`: hub là thứ đang được nghiệm thu,
/// nên nó không được vừa làm vừa chấm bài của chính mình.
function firstUserTurn(path) {
  if (!path) return "";
  for (const line of readFileSync(path, "utf8").split("\n")) {
    if (!line.trim()) continue;
    let rec;
    try {
      rec = JSON.parse(line);
    } catch {
      continue;
    }
    if (rec.type !== "user") continue;
    const c = rec.message?.content;
    const text = typeof c === "string" ? c : (c || []).map((b) => b.text || "").join(" ");
    if (text.trim()) return text;
  }
  return "";
}

/// Còn cửa sổ Terminal nào đang chạy cái tty này không — hỏi CHÍNH Terminal.
///
/// Không hỏi hub: hub là thứ đang bị chấm bài. Không hỏi `ps`: `ps` trả lời
/// "còn tiến trình nào không", mà đó mới là NỬA định nghĩa "tắt hẳn" — nửa kia
/// là cửa sổ, và ca hay gặp nhất chính là CLI thoát rồi mà cửa sổ vẫn mở.
function ttyHasWindow(tty) {
  if (!tty) return false;
  const dev = tty.startsWith("/dev/") ? tty : `/dev/${tty}`;
  const out = execFileSync("osascript", ["-e", `tell application "Terminal"
  repeat with w in windows
    repeat with t in tabs of w
      if (tty of t) is "${dev}" then return "yes"
    end repeat
  end repeat
  return "no"
end tell`], { encoding: "utf8" });
  return out.trim() === "yes";
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
// Đã tắt trong lúc nghiệm thu chưa — khối `finally` cần biết, mà `stopped` thì
// nằm trong `try` nên nó không với tới.
let cleaned = false;
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
    console.log(`      cd ${WORKSPACE}/AI/${project} && claude   → Esc → thoát`);
    throw new Error("__blocked_path_done__");
  }
  check("phiên mới xuất hiện mà không cần chạm vào máy", true, fresh.session_id.slice(0, 8));
  started = fresh;
  // Từ 2026-08-11 `/new` mở CỬA SỔ THẬT, không phải phiên nền — Hà: *"cli
  // claude cài trên máy tôi, hub là cầu kết nối ra ui"*. Phép đo đổi theo, và
  // đổi sang thứ ĐÁNG đo: có cửa sổ thì mới có màn sống, mới `/btw` được, mới
  // hiện được "đang làm gì". `kind` một mình không đủ — một hàng `interactive`
  // không tty là phiên hub không với tới.
  check(
    "phiên mới là phiên CÓ CỬA SỔ",
    fresh.kind === "interactive" && !!fresh.tty && fresh.host === "terminal",
    `${fresh.kind} · tty=${fresh.tty || "(không)"} · host=${fresh.host}`
  );
  // Phiên mở ở GỐC WORKSPACE, không phải trong thư mục dự án — Hà 2026-08-10:
  // *"ngay từ đầu tôi bảo mọi phiên đều bắt đầu từ thư mục projects rồi mà"*.
  // Mở trong thư mục con là mở vào chỗ chưa tài khoản nào duyệt, nên phiên kẹt
  // ngay ở hộp thoại MCP rồi chết. Dự án được nói trong ĐỀ BÀI thay vì bằng
  // thư mục, nên phép đo cũng chuyển sang đo đúng chỗ ấy.
  check(
    "phiên mới mở ở gốc workspace",
    (fresh.cwd || "").replace(/\/+$/, "") === WORKSPACE,
    `${fresh.cwd} (mong đợi ${WORKSPACE})`
  );
  // Đề bài có tới nơi không — đo ở NHẬT KÝ, không ở tên phiên.
  //
  // Phiên nền được `claude` đặt tên theo lời nhắc, nên tên từng là bằng chứng
  // tốt. Phiên cửa sổ thì `claude` tự đặt tên (`projects-53`), nên đọc tên là
  // đo nhầm chỗ. Nhật ký thì không nói dối: lượt đầu tiên CHÍNH LÀ đề bài, và
  // đây đúng là chỗ bẫy `--disallowedTools` variadic sẽ lộ ra — đề bài bị nuốt
  // thì lượt ấy trống, phiên dựng lên mà không có việc.
  const firstTurn = firstUserTurn(transcriptOf(fresh.session_id));
  check(
    "đề bài tới được phiên, mang tên dự án",
    firstTurn.includes(`[${project}]`),
    firstTurn.slice(0, 60) || "(nhật ký chưa có lượt nào của người dùng)"
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
  // v85 gộp hai ô làm một. Với phiên hub mở, mặc định PHẢI là "nói tiếp":
  // ô tích "hỏi bên lề" bỏ trống và mở cho người dùng tự chọn.
  check("phiên do hub mở thì CÓ ô nhập", await page.locator("#sessSayInput").isVisible());
  check("mặc định là NÓI TIẾP, không phải hỏi bên lề",
    (await page.locator("#sessAside").isChecked()) === false &&
      (await page.locator("#sessAside").isDisabled()) === false,
    await page.locator("#sessSayInput").getAttribute("placeholder"));

  // MÀN SỐNG — thứ phân biệt "hub mở được cửa sổ" với "hub nhìn được vào cửa sổ".
  //
  // Đây là lý do `/new` bỏ `--bg`: phiên nền không có màn để nhìn. Nếu mở cửa
  // sổ rồi mà màn vẫn trống thì việc đổi đường coi như chưa tới đích, nên nó
  // phải là một mục nghiệm thu, không phải một lần soi tay.
  //
  // Chờ có mốc: màn chỉ tới trang sau lượt đẩy ảnh chụp KẾ TIẾP lượt `/session`
  // (hub đọc màn hình rồi mới đẩy), nên đo ngay là đo độ trễ chứ không đo sản
  // phẩm. Chờ tối đa 2 phút.
  let liveScreen = false;
  for (let i = 0; i < 60 && !liveScreen; i++) {
    liveScreen = await page.evaluate(() => {
      const st = document.getElementById("sessStream");
      return !!st && st.children.length > 0;
    });
    if (!liveScreen) await page.waitForTimeout(2000);
  }
  check(
    "màn chi tiết có nội dung sống, không phải hộp trắng",
    liveScreen,
    liveScreen ? "có" : "trống sau 2 phút — hub chưa đẩy được luồng/màn của phiên"
  );

  const file = transcriptOf(fresh.session_id);
  check("phiên mới có nhật ký trên đĩa", !!file, file || "");

  // ——— UC-S05: nói tiếp vào CHÍNH phiên đó, LÚC NÓ CÒN SỐNG ———
  //
  // Thứ tự đổi từ 2026-08-11, và đổi vì sản phẩm đổi. Phiên nền phải DỪNG rồi
  // mới nối vào được (`claude` từ chối nối vào job nền đang chạy), nên bản cũ
  // buộc phải dừng trước — kéo theo cả khúc sau treo vào một cú bấm Telegram.
  // Phiên CÓ CỬA SỔ thì ngược hẳn: gõ thẳng vào cửa sổ đang chạy (`/type`) mới
  // là đường thật, và tắt là việc CUỐI CÙNG. Đúng thứ tự người ta làm khi ngồi
  // trước máy.
  let stopped = false;
  const sizeBefore = sizeOf(file);
  const idsBefore = new Set(
    hub(["sessions", "--json"]).sessions.map((s) => s.session_id)
  );
  await page.fill("#sessSayInput", "Nội dung README bạn vừa đọc nói về cái gì? Trả lời một dòng.");
  await page.click("#sessSay");
  await page.waitForFunction(
    () => {
      const t = document.getElementById("sessTellBox").textContent || "";
      return t.length > 0 && !/^Đang (nói tiếp|gõ)/.test(t);
    },
    null,
    { timeout: 240000, polling: 1000 }
  ).catch(() => {});
  const tellMsg = await page.textContent("#sessTellBox");

  check(
    "màn báo chữ đã tới cửa sổ phiên",
    // Hai đường, hai câu: `/tell` báo "Đã nói tiếp vào phiên", `/type` báo
    // "⌨ đã gõ N ký tự vào …". Đo Ý NGHĨA (chữ đã tới phiên), không đo hoa
    // thường — bản trước bắt `Đã` viết hoa nên báo đỏ một lượt gõ THÀNH CÔNG,
    // trong khi nhật ký ngay dưới đó dài ra 3650 byte.
    /(đã nói tiếp vào phiên|đã gõ \d+ ký tự)/i.test(tellMsg),
    tellMsg.trim().slice(0, 70)
  );
  // ĐÂY là chỗ UC-S05 khác hẳn UC-S05b: lượt này ở LẠI trên phiên cũ.
  // Nhật ký chỉ dài ra sau khi phiên xử lý xong lượt, nên CHỜ có mốc — đo ngay
  // rồi kết luận "không dài ra" là đo cái độ trễ, không đo sản phẩm.
  let sizeAfter = sizeBefore;
  for (let i = 0; i < 60 && sizeAfter <= sizeBefore; i++) {
    await page.waitForTimeout(2000);
    sizeAfter = sizeOf(file);
  }
  check(
    "nhật ký phiên DÀI RA (khác hẳn hỏi bên lề)",
    sizeAfter > sizeBefore,
    `${sizeBefore} → ${sizeAfter} byte`
  );
  // Hỏi DANH SÁCH, đừng hỏi lại thứ vừa hỏi.
  //
  // Bản trước là `!!stillThere || sizeAfter > sizeBefore`, mà vế sau CHÍNH LÀ
  // điều kiện đã assert hai dòng trên ("nhật ký phiên DÀI RA"). Nên hễ phép đo
  // kia xanh thì phép đo này xanh theo, bất kể có đẻ ra phiên lạ hay không.
  // Nay so tập id trước/sau: chữ phải ở LẠI trên phiên cũ.
  const idsAfter = new Set(
    hub(["sessions", "--json"]).sessions.map((s) => s.session_id)
  );
  const moi = hub(["sessions", "--json"]).sessions.filter(
    (x) => !idsBefore.has(x.session_id) && x.session_id !== fresh.session_id
  );
  // Phiên lạ nào là DO BƯỚC NÀY đẻ ra?
  //
  // "Có phiên mới trên máy" là một phép đo quá rộng — 2026-08-11 nó báo đỏ vì
  // chủ máy tự mở một `claude` khác để gõ `/usage` trong lúc kịch bản chạy.
  // Cái cần khẳng định là "`/type` không đẻ phiên", nên bằng chứng phải buộc
  // vào chính lượt gõ: một phiên do bước này sinh ra thì hoặc mang dấu của hub,
  // hoặc mang CHÍNH câu vừa gõ ở lượt đầu nhật ký.
  //
  // Không thu hẹp về mỗi `started_by_hub`: một phiên hub đẻ ra NGOÀI Ý MUỐN sẽ
  // không có trong sổ của hub, nên đóng khung theo dấu ấy là bịt mắt đúng chỗ
  // phép đo sinh ra để nhìn.
  const doBuocNay = moi.filter(
    (x) =>
      x.started_by_hub ||
      firstUserTurn(transcriptOf(x.session_id)).includes("Nội dung README bạn vừa đọc")
  );
  if (moi.length && !doBuocNay.length) {
    console.log(
      `  · ghi nhận: có ${moi.length} phiên mới trên máy trong lúc chạy ` +
      `(${moi.map((x) => `${x.session_id.slice(0, 8)} @ ${x.cwd}`).join(", ")}) — ` +
      `không mang dấu hub và không mang câu vừa gõ, nên không phải do bước này.`
    );
  }
  check(
    "KHÔNG đẻ ra phiên mới ngoài ý muốn",
    doBuocNay.length === 0,
    doBuocNay.length
      ? `phiên do bước này đẻ: ${doBuocNay.map((x) => x.session_id.slice(0, 8)).join(", ")}`
      : "không có"
  );

  // ——— TẮT HẲN: thoát CLI và đóng cửa sổ ———
  //
  // Định nghĩa là của Hà (2026-08-11): *"tắt hẳn là thoát cli và đóng
  // terminal"*. Nghiệm thu vì thế phải đo ĐÚNG HAI thứ ấy trên máy thật: phiên
  // rời danh sách, VÀ không còn cửa sổ nào chạy cái tty đó. Đo một thứ thôi là
  // bỏ lọt đúng ca hay gặp nhất — CLI thoát rồi mà cửa sổ vẫn nằm đấy.
  //
  // Bước này CẦN MỘT NGÓN TAY THẬT: `/stop` đi qua chốt xác nhận Telegram
  // (`confirm.rs`), nên hub đứng chờ tới 90 giây một cú bấm. Không ai bấm thì
  // kịch bản nói "BỎ QUA vì chưa ai xác nhận", KHÔNG tính là hỏng — sản phẩm
  // lúc ấy đang cư xử đúng.
  const nutTat = (await page.textContent("#sessStop")).trim();
  check("nút tắt nói đúng việc nó làm", nutTat.includes("Tắt hẳn"), nutTat);
  await page.click("#sessStop");
  await page.waitForFunction(
    () => {
      const t = document.getElementById("sessTellBox").textContent || "";
      // `🔒 Đã gửi yêu cầu xác nhận…` là tin GIỮA CHỪNG — dừng ở đó là đo cái
      // bấm nút, không phải đo kết cục. Chờ tới nhịp thứ hai (⏹ / ✋ / ⌛).
      return t.length > 0 && !/^Đang (dừng|thoát)/.test(t) && !t.startsWith("🔒");
    },
    null,
    { timeout: 180000, polling: 1000 }
  ).catch(() => {});
  const stopMsg = await page.textContent("#sessTellBox");
  if (/Đã tắt hẳn phiên/.test(stopMsg)) {
    stopped = true;
    cleaned = true;
    check("màn báo đã tắt hẳn", true, stopMsg.trim().slice(0, 70));
    check("nhật ký vẫn còn sau khi tắt", sizeOf(file) > 0, `${sizeOf(file)} byte`);
    // Hai bằng chứng trên MÁY, không phải trên màn.
    check("cửa sổ terminal đã đóng", !ttyHasWindow(fresh.tty), fresh.tty);
    let left = false;
    for (let i = 0; i < 15 && !left; i++) {
      left = !hub(["sessions", "--json"]).sessions.some((x) => x.session_id === fresh.session_id);
      if (!left) await page.waitForTimeout(2000);
    }
    check("phiên rời khỏi danh sách đang chạy", left, fresh.session_id.slice(0, 8));
  } else if (/xác nhận|Telegram|Hết hạn|huỷ/i.test(stopMsg)) {
    console.log(
      `  · BỎ QUA 4 kiểm tra: /stop đang chờ xác nhận trên Telegram và không ai bấm ` +
      `("${stopMsg.trim().slice(0, 60)}"). Chưa nghiệm thu: "màn báo đã tắt hẳn", ` +
      `"nhật ký vẫn còn sau khi tắt", "cửa sổ terminal đã đóng", "phiên rời khỏi danh sách".`
    );
  } else {
    check("màn báo đã tắt hẳn", false, stopMsg.trim().slice(0, 70));
  }

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
  // Dọn: phiên CỬA SỔ không tắt được bằng `claude stop` (lệnh ấy chỉ biết job
  // nền) — phải thoát CLI rồi đóng cửa sổ, đúng đường sản phẩm đi. Bản cũ gọi
  // `claude stop` cho mọi thứ, nên mỗi lần chạy để lại một cửa sổ còn sống.
  if (started && !cleaned) {
    const short = started.session_id.slice(0, 8);
    try {
      if (started.kind === "background") {
        execFileSync("claude", ["stop", short], { stdio: "ignore" });
      } else if (ttyHasWindow(started.tty)) {
        const dev = started.tty.startsWith("/dev/") ? started.tty : `/dev/${started.tty}`;
        execFileSync("osascript", ["-e", `tell application "Terminal"
  repeat with w in windows
    repeat with t in tabs of w
      if (tty of t) is "${dev}" then
        do script "/exit" in t
        delay 6
        close w
        return "closed"
      end if
    end repeat
  end repeat
  return "gone"
end tell`], { stdio: "ignore" });
      }
      console.log(`\n(dọn: đã tắt ${short})`);
    } catch (e) {
      console.log(`\n⚠ dọn không xong (${e.message.slice(0, 60)}) — tự tắt cửa sổ ${started.tty}`);
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
