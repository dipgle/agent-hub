// UC-S05b — "chen ngang hỏi mà không phá việc đang làm", nghiệm thu trên UI
// THẬT ở 390px.
//
// Đi trọn đường người dùng: đăng nhập → chạm một phiên trong danh sách → gõ câu
// hỏi vào ô trên màn phiên → bấm Hỏi → huba trả lời về màn.
// Không goto, không gọi API dựng trạng thái.
//
// ⚠ HAI ĐƯỜNG, HAI LỜI HỨA NGƯỢC NHAU (từ 2026-08-11):
//   · phiên CÓ cửa sổ terminal → huba gõ `/btw` thẳng vào phiên đang sống
//     (mức 1). Lời hứa (ĐO 2026-08-11): câu trả lời tới từ CHÍNH phiên ấy, hiện
//     trong một BẢNG BÊN của TUI; nhật ký KHÔNG dài thêm — cái bị ăn là ngữ
//     cảnh đang chạy, và màn phải nói đúng chừng ấy.
//   · phiên không gõ vào được → fork (mức 2). Lời hứa: phiên gốc y nguyên byte.
//
// Nên phép đo phải CHỌN LỜI HỨA TRƯỚC KHI BẤM, từ chính điều kiện quyết định
// đường đi (`tty` + `host`), rồi mới đo. Đo cả hai bằng một khuôn — hoặc tệ hơn,
// đo lời hứa của fork trên đường `/btw` — là phép đo đòi sản phẩm làm sai thiết
// kế, thứ không bao giờ xanh được và cũng chẳng dạy được gì.
//
// Nhắm vào một phiên cụ thể: HUB_UC_ASIDE_TARGET=<session_id>. Mặc định chọn
// phiên ĐỨNG YÊN có nhật ký ngắn nhất — đứng yên là điều kiện của phép đo, vì
// nhật ký của phiên đang chạy tự dài ra và lúc ấy cả hai lời hứa đều mù.
//
// Usage: node fe-aside-uc.mjs <app_tid> <username> <password>

import { chromium } from "/Users/hanguyen/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
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
/// Đường nào đã được nghiệm thu lượt này — in ra ở cuối, vì "17/17 đạt" mà
/// không nói đo đường nào thì lần sau có người đọc thành "cả hai đều xong".
let branchTaken = "";
const check = (name, ok, detail = "") => {
  checks.push({ name, ok });
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) problems.push(`${name}${detail ? `: ${detail}` : ""}`);
};

// ── CỔNG HẠN MỨC ────────────────────────────────────────────────────────────
// Bước dưới đây gọi `claude` THẬT trên bản fork, lượng dùng tỉ lệ độ dài nhật ký.
//
// ⚠ ĐƠN VỊ: `$` ở đây là `total_cost_usd` của CLI — giá QUY ĐỔI theo bảng giá
// API cho lượng token, KHÔNG phải tiền bị trừ: máy này chạy gói Max, cái bị
// tiêu là hạn mức của gói. Xem chú thích đầy đủ trong fe-stream-uc.mjs.
const USD_PER_MB = 1.75;                      // mốc đo thật 2026-08-08
const MAX_USD = Number(process.env.HUB_UC_MAX_USD || 0.25); // đơn vị quy đổi, không phải tiền
const PAY = process.env.HUB_UC_PAY === "1";   // chấp nhận trả tiền lượt này
const skipped = [];
const affordable = (mb, what) => {
  const est = mb * USD_PER_MB;
  if (PAY || est <= MAX_USD) return true;
  const msg = `${what}: ước tính ${est.toFixed(2)} > trần ${MAX_USD.toFixed(2)} đơn vị hạn mức`;
  skipped.push(msg);
  console.log(`\n⏭  BỎ QUA (không gọi claude) — ${msg}`);
  console.log(`   Muốn nghiệm thu lại đường tốn hạn mức: HUB_UC_PAY=1 node ${process.argv[1].split("/").pop()} …\n`);
  return false;
};

const huba = (args) =>
  JSON.parse(
    execFileSync(HERE + "rust/target/release/huba", args, {
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

/// Nhật ký CHƯA tồn tại cũng là một câu trả lời hợp lệ: một phiên vừa mở, chưa
/// nói lượt nào, thì tệp chỉ sinh ra ở lượt đầu tiên. Với đường `/btw` đó lại là
/// mục tiêu SẠCH nhất — không có việc dở nào để làm nhiễu phép đo.
const fingerprint = (path) => {
  if (!path) return { bytes: 0, mtimeMs: 0, lines: 0 };
  try {
    const st = statSync(path);
    return {
      bytes: st.size,
      mtimeMs: st.mtimeMs,
      lines: readFileSync(path, "utf8").split("\n").filter(Boolean).length,
    };
  } catch {
    return { bytes: 0, mtimeMs: 0, lines: 0 };
  }
};

/// Đúng ĐIỀU KIỆN huba dùng để chọn đường, và nay là con số huba TỰ ĐO
/// (`sessions::mark_can_type` hỏi Terminal.app đang giữ những tty nào) chứ
/// không phải một bản sao của luật nằm trong kịch bản.
///
/// Bản trước chép luật: `tty && host === "terminal"`. Nó SAI đúng ca đã tốn
/// tiền thật ngày 2026-08-11 — phiên trong terminal tích hợp của VS Code thoả
/// cả hai vế mà Terminal.app không biết cái tty ấy, nên kịch bản chờ `/btw`
/// trong khi huba đi đường fork.
const canTypeInto = (s) => s.can_type === true;

/// Những dòng nhật ký MỚI so với lúc trước khi hỏi.
///
/// "Nhật ký dài ra" một mình không chứng minh gì trên đường `/btw`: một phiên
/// vừa tỉnh dậy cũng dài ra. Cái chứng minh là câu hỏi VỪA GÕ nằm trong đúng
/// tệp của phiên gốc.
const linesSince = (path, beforeLines) => {
  try {
    return readFileSync(path, "utf8").split("\n").filter(Boolean).slice(beforeLines).join("\n");
  } catch {
    return "";
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
  const truth = huba(["sessions", "--json"]);
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
  const want = (process.env.HUB_UC_ASIDE_TARGET || "").trim();
  // Nhắm tay thì được nhắm vào cả phiên CHƯA có nhật ký: một phiên vừa mở, đứng
  // ở dấu nhắc, là mục tiêu sạch nhất cho đường `/btw` (không việc dở nào làm
  // nhiễu). Bộ lọc mặc định vẫn đòi `last_text` vì câu hỏi mặc định hỏi về việc
  // phiên đang làm — hỏi câu ấy trên một phiên trắng thì "trả lời trôi chảy"
  // chẳng chứng minh gì.
  const aimedRaw = want ? truth.sessions.find((s) => s.session_id.startsWith(want)) : null;
  if (want && !aimedRaw) {
    throw new Error(`HUB_UC_ASIDE_TARGET=${want} không có trong danh sách phiên — dừng, không đổi mục tiêu ngầm`);
  }
  const aimed = aimedRaw
    ? {
        s: aimedRaw,
        file: transcriptOf(aimedRaw.session_id),
        mb: (statSync(transcriptOf(aimedRaw.session_id) || "/dev/null").size || 0) / 1e6,
      }
    : null;
  if (!aimed && !sized.length) {
    throw new Error("không có phiên đứng yên nào để hỏi — dừng, không đo bừa");
  }
  const { s: target, file, mb } = aimed || sized[0];
  // ĐƯỜNG NÀO — chốt TRƯỚC khi bấm. Chốt sau khi có câu trả lời thì phép đo chỉ
  // là cái gương: huba trả về gì nó cũng gật.
  const viaBtw = canTypeInto(target);
  branchTaken = viaBtw ? "btw" : "fork";
  console.log(`chạm vào phiên: ${target.name} (${target.account}, đứng yên ${Math.round(idleFor(target))} phút)`);
  console.log(`nhật ký: ${file}`);
  console.log(
    viaBtw
      ? `đường DỰ KIẾN: /btw — phiên có cửa sổ (${target.tty}). Lời hứa phải đo: câu trả lời tới từ chính phiên ấy, nhật ký KHÔNG dài thêm, và câu trả lời là câu trả lời chứ không phải ảnh màn hình.`
      : `đường DỰ KIẾN: fork — phiên không gõ vào được (host ${target.host}, tty "${target.tty || ""}"). Lời hứa phải đo: phiên gốc y nguyên byte.`
  );
  console.log(`kích thước: ${mb.toFixed(2)} MB ≈ ${(mb * USD_PER_MB).toFixed(2)} đơn vị hạn mức cho một câu hỏi (chỉ đúng với đường fork)\n`);

  const before = fingerprint(file);
  const activityBefore = target.last_activity;

  await page.locator(`.sess[data-session="${target.session_id}"]`).click();
  check("chạm vào phiên thì mở màn chi tiết", await page.locator("#sessDetail").isVisible());
  // Từ v85 hai ô nhập gộp làm một: mặc định là "nói tiếp", tích ô để chuyển
  // sang "hỏi bên lề". Với phiên KHÔNG gõ vào được thì ô tích đã bật sẵn và bị
  // khoá — nên phép đo phải bật nó khi cần, chứ không giả định.
  check("màn phiên có ô nhập", await page.locator("#sessSayInput").isVisible());
  const asideBox = page.locator("#sessAside");
  if (!(await asideBox.isChecked())) await asideBox.check();
  check("đã ở chế độ hỏi bên lề",
    await asideBox.isChecked(),
    await page.locator("#sessSayInput").getAttribute("placeholder"));
  // Chữ hứa hẹn phải khớp ĐƯỜNG SẮP ĐI — đây là chỗ trang từng nói hộ một điều
  // sản phẩm thôi làm: phiên có cửa sổ vẫn được hứa "phiên gốc không bị đụng"
  // trong khi huba sắp gõ thẳng vào nó.
  const hintText = await page.locator("#sessAskHint").textContent();
  check(
    viaBtw
      ? "màn báo trước sẽ hỏi thẳng bằng /btw và nói đúng cái nó ăn"
      : "màn nói rõ phiên gốc không bị đụng",
    viaBtw
      ? hintText.includes("/btw") && hintText.includes("ngữ cảnh đang chạy")
      : hintText.includes("không bị đụng"),
    hintText.slice(0, 90)
  );

  // Chờ luồng về trước đã, để chắc huba đã theo đúng phiên này.
  //
  // Phiên CHƯA nói lượt nào thì luồng RỖNG mãi mãi — không có sự kiện nào để
  // chờ, và đó là trạng thái đúng, không phải hỏng. Chờ cứng ở đây làm kịch bản
  // chết sau 3 phút trên đúng mục tiêu sạch nhất của đường `/btw`. Với phiên
  // trắng, thứ chứng minh "huba đang theo đúng phiên" là **màn sống** của nó.
  const fresh = !file;
  await page.waitForFunction(
    (empty) =>
      empty
        ? (document.getElementById("sessScreen")?.textContent || "").trim().length > 0
        : document.querySelectorAll("#sessStream .ev").length > 0,
    fresh,
    { timeout: 180000, polling: 1000 }
  );

  // Ảnh chụp KHÔNG mang số tiền nào (gỡ 2026-08-08). Đòi vắng mặt, vì thứ này
  // đã mọc lại một lần: trần → giá.
  const snap = huba(["portal-push", "--dry-run"]);
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
  // Cổng giá chỉ có nghĩa với đường FORK: fork nạp lại toàn bộ nhật ký nên giá
  // tỉ lệ độ dài, còn `/btw` hỏi phiên đang sống bằng ngữ cảnh đã nằm sẵn trong
  // đầu nó — đúng một lượt, y như chủ máy tự gõ. Chặn `/btw` bằng số MB của
  // nhật ký là chặn nhầm thứ, và nó sẽ chặn đúng những phiên lâu đời nhất.
  const willPay = viaBtw || affordable(mb, "UC-S05b /ask (đường fork)");
  // Khai báo NGOÀI khối: các phép đo bên dưới ("phiên gốc y nguyên") vẫn chạy
  // khi bỏ qua bước hỏi, và chúng đọc mấy biến này.
  let a = null;
  let shown = { box: "" };
  if (willPay) {
  await page.fill("#sessSayInput", question);
  await page.click("#sessSay");
  check("ô hỏi được dọn sau khi gửi", (await page.inputValue("#sessSayInput")) === "");

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
  // đúng bẫy đó và tưởng đã đo xong trong khi huba còn đang nghĩ.
  let after = null;
  for (let i = 0; i < 60; i++) {
    after = huba(["portal-push", "--dry-run"]);
    if ((after.sessions.aside?.ts || "") !== asideBefore) break;
    await page.waitForTimeout(3000);
  }
  a = after.sessions.aside;
  // Rồi mới chờ MÀN bắt kịp đúng câu trả lời ấy. Dấu nhận biết khác nhau theo
  // đường: fork thì màn in id bản sao; `/btw` không có id nào để in, dấu của nó
  // là chính câu nói ra cái giá.
  // Chờ đúng cái màn NGƯỜI TA RỐT CUỘC NHÌN THẤY, tức tấm thẻ do ảnh chụp dựng
  // (`renderAside`), chứ không phải lời đáp thô từ phòng chat tới trước nó.
  //
  // Dấu phải là dấu CHỈ tấm thẻ mới có: `Bạn hỏi:`. Bản trước chờ chữ `/btw` —
  // mà lời đáp của phòng chat cũng chứa `/btw` (nó khai đã đi đường nào), nên
  // phép chờ khớp ngay ở trạng thái giữa chừng và mọi assert phía sau đo một
  // màn đang trên đường đi. Đúng cái bẫy ghi ngay trong tệp này từ 2026-08-08,
  // chỉ đổi mặt nạ.
  await page.waitForFunction(
    () => (document.getElementById("sessAskBox").textContent || "").includes("Bạn hỏi:"),
    null,
    { timeout: 60000, polling: 1000 }
  ).catch(() => {});
  // Chỉ đọc `#sessAskBox`. Bản trước còn đọc `#cmdNote` — mà `cmdNote` là tên
  // một HÀM trong `fe/index.html`, không phải id phần tử (id thật là
  // `#cmdStatus`), nên trường ấy luôn rỗng và không assert nào đọc tới. Mã chết
  // trong một kịch bản đo là thứ khiến người đọc tưởng có gì đó đang được đo.
  shown = await page.evaluate(() => ({
    box: document.getElementById("sessAskBox").textContent || "",
  }));

  }

  // ——— LỜI HỨA CỦA UC, đo trên tệp thật ———
  const now = fingerprint(file || transcriptOf(target.session_id));
  const liveNow = huba(["sessions", "--json"]).sessions.find((s) => s.session_id === target.session_id);
  if (viaBtw) {
    // Đường `/btw` — LỜI HỨA ĐÃ ĐO LẠI 2026-08-11, và nó ngược với lời hứa mà
    // chính kịch bản này viết ra buổi sáng cùng ngày.
    //
    // Giả thiết cũ: "hỏi thẳng vào phiên sống ⟹ phiên gốc dài ra một lượt".
    // Chạy thật trên `projects-ff` (Terminal.app, ttys001): `/btw` mở một BẢNG
    // BÊN trong TUI, trả lời đầy đủ, rồi đóng lại bằng Esc — và **không một byte
    // nào vào nhật ký**, phiên ấy tới giờ vẫn chưa có tệp `.jsonl`. Cái nó ăn là
    // NGỮ CẢNH đang chạy, thứ không nhìn thấy trên đĩa.
    //
    // Nên phép đo ở đây đúng bằng cái sản phẩm đang hứa trên màn: nhật ký KHÔNG
    // dài thêm, và câu trả lời tới từ chính phiên ấy.
    // ⚠ Đo được đúng MỘT ca (phiên trắng). Phiên đã có nhật ký sẵn thì chưa
    // biết — nếu ca ấy làm phép đo này đỏ thì đó là phát hiện, không phải hỏng.
    if (willPay) {
      check(
        "nhật ký phiên gốc KHÔNG dài thêm (đúng câu màn vừa hứa)",
        now.bytes === before.bytes && now.lines === before.lines,
        `${before.bytes}→${now.bytes} byte · ${before.lines}→${now.lines} dòng`
      );
      check(
        "phiên gốc không nhảy lên 'vừa động'",
        !liveNow || liveNow.last_activity === activityBefore,
        `${activityBefore} → ${liveNow ? liveNow.last_activity : "(không còn trong danh sách)"}`
      );
      // Câu trả lời phải là CÂU TRẢ LỜI, không phải ảnh chụp màn hình. Bản đầu
      // trả cả logo khởi động + dòng `/btw …` vừa gõ + chân bảng hướng dẫn
      // phím, và tệ hơn: trả lúc bảng còn đang viết (`· Answering…`), vì nó chờ
      // "màn đổi và phiên thôi bận" — mà bảng `/btw` không có đồng hồ để `bận`.
      const ans = (a && a.answer) || "";
      check("câu trả lời không kèm logo khởi động", !ans.includes("Claude Code v"), ans.slice(0, 60));
      check("câu trả lời không kèm chân bảng phím", !ans.includes("Esc to close"));
      check("câu trả lời không phải bảng đang viết dở", !ans.includes("Answering…"));
      check("câu trả lời không lặp lại chính câu vừa gõ", !ans.includes(`/btw ${question.slice(0, 20)}`));
    }
  } else {
    // Đường fork: chạy cả khi cổng giá chặn bước hỏi — "phiên gốc y nguyên" vẫn
    // là điều phải đúng, và đo nó không tốn gì.
    check(
      "phiên gốc không thêm một byte nào",
      now.bytes === before.bytes && now.lines === before.lines,
      `${before.bytes}→${now.bytes} byte · ${before.lines}→${now.lines} dòng`
    );
    check("phiên gốc không bị ghi lại (mtime y nguyên)", now.mtimeMs === before.mtimeMs);
    check(
      "phiên gốc không nhảy lên 'vừa động'",
      !liveNow || liveNow.last_activity === activityBefore,
      `${activityBefore} → ${liveNow ? liveNow.last_activity : "(không còn trong danh sách)"}`
    );
  }

  // Các phép đo dưới đây chỉ có nghĩa khi thật sự đã hỏi. Khi cổng giá chặn,
  // chúng KHÔNG được tính là đạt — bỏ qua thì phải hiện ra là bỏ qua.
  if (willPay) {
  check("có câu hỏi bên lề MỚI trong ảnh chụp", !!a && a.ts !== asideBefore && a.source_id === target.session_id);
  check("câu hỏi được giữ nguyên văn", a && a.question === question, a ? a.question : "");
  check("có câu trả lời", !!(a && a.answer && a.answer.length > 0), a ? a.answer.slice(0, 80) : "");
  check(
    viaBtw
      ? "trả lời tới từ CHÍNH phiên đó, không có bản sao nào"
      : "trả lời nằm ở phiên KHÁC (fork), không phải phiên gốc",
    viaBtw
      ? !!(a && a.new_session_id && a.new_session_id === a.source_id)
      : !!(a && a.new_session_id && a.new_session_id !== a.source_id),
    a ? `${a.source_id.slice(0, 8)} → ${a.new_session_id.slice(0, 8)}` : ""
  );
  // Câu dưới thẻ trả lời phải nói ĐÚNG chuyện vừa xảy ra. Hai vế, và vế phủ
  // định mới là vế cứu người đọc: một màn `/btw` mà vẫn in "phiên gốc không
  // thêm lượt nào" là trang khẳng định sai về nhật ký của chính chủ máy.
  check(
    viaBtw ? "màn nói ĐÚNG: hỏi thẳng /btw, và nói đúng cái nó ăn" : "màn hiện câu trả lời",
    viaBtw
      ? shown.box.includes("/btw") &&
        shown.box.includes("ngữ cảnh đang chạy") &&
        !shown.box.includes("bản sao")
      : shown.box.includes("phiên gốc không thêm lượt nào"),
    shown.box.slice(0, 80)
  );
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
  (skipped.length ? ` · ${skipped.length} BỎ QUA vì tốn hạn mức` : "")
);
// Nói rõ lượt này đo đường nào và CHƯA đo đường nào. UC-S05b có hai đường hứa
// ngược nhau; một lượt chạy chỉ mua được bằng chứng cho một đường.
if (branchTaken === "btw") {
  console.log("\nĐƯỜNG ĐÃ NGHIỆM THU: /btw (hỏi thẳng phiên có cửa sổ).");
  console.log("  CHƯA đo lượt này: đường fork (phiên không gõ vào được, y nguyên byte).");
} else if (branchTaken === "fork") {
  console.log("\nĐƯỜNG ĐÃ NGHIỆM THU: fork (phiên không gõ vào được).");
  console.log("  CHƯA đo lượt này: đường /btw — cần nhắm vào một phiên có cửa sổ terminal:");
  console.log("  HUB_UC_ASIDE_TARGET=<session_id> node fe-aside-uc.mjs …");
}
if (skipped.length) {
  console.log("\nCHƯA NGHIỆM THU (không gọi claude lượt này):");
  skipped.forEach((s) => console.log(`  ⏭ ${s}`));
  console.log("  → chạy lại với HUB_UC_PAY=1 khi cần bằng chứng đường tốn hạn mức.");
}
if (problems.length) {
  console.log("\nVẤN ĐỀ:");
  problems.forEach((p) => console.log(`  · ${p}`));
}
process.exit(problems.length ? 1 : 0);
