// UC-S02 — "xem một phiên như đang ngồi máy", nghiệm thu trên UI THẬT, 390px.
//
// Đi trọn đường người dùng: đăng nhập → chạm vào một phiên trong danh sách →
// hub nhận lệnh /session qua phòng chat → ảnh chụp kế tiếp mang luồng về →
// màn hiện lời nói, từng lệnh và kết quả từng lệnh.
//
// Không goto, không gọi API dựng trạng thái. Số liệu đối chiếu lấy từ
// `hub portal-push --dry-run` chạy độc lập, để màn phải khớp sự thật của máy
// chứ không khớp với chính nó.
//
// Usage: node fe-stream-uc.mjs <app_tid> <username> <password>

import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { execFileSync } from "node:child_process";
import { mkdirSync, statSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

/// Megabytes of transcript for a session — what a `--resume` call has to pay to
/// load. Returns Infinity when the file cannot be found, so an unmeasurable
/// session sorts LAST rather than looking like the cheapest one.
function transcriptMB(sessionId) {
  const root = join(homedir(), ".claude", "projects");
  for (const dir of readdirSync(root)) {
    try {
      return statSync(join(root, dir, `${sessionId}.jsonl`)).size / 1e6;
    } catch {
      /* not in this project folder */
    }
  }
  return Infinity;
}

const [appTid, username, password] = process.argv.slice(2);
if (!appTid || !username || !password) {
  console.error("usage: node fe-stream-uc.mjs <app_tid> <username> <password>");
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

// ── CỔNG HẠN MỨC ────────────────────────────────────────────────────────────
// Bước dưới đây gọi `claude` THẬT trên bản fork, và lượng dùng tỉ lệ độ dài
// nhật ký (đo thật: 0.986 MB → 1.72 đơn vị).
//
// ⚠ ĐƠN VỊ: con số `$` ở đây là `total_cost_usd` do chính CLI trả về — GIÁ QUY
// ĐỔI theo bảng giá API cho lượng token của lần gọi. Máy này chạy gói **Max**
// (`claude auth status` → authMethod claude.ai, subscriptionType max), nên
// KHÔNG có đồng nào bị trừ mỗi lần gọi; cái bị tiêu là **hạn mức của gói**
// (cửa sổ 5 giờ / tuần). Dùng số này làm thước đo lượng dùng, đừng đọc nó là
// hoá đơn. `--max-budget-usd` của CLI cũng gác trên đúng thang này.
//
// Vì sao vẫn cần gác: đêm 2026-08-08 kịch bản này chạy 5 lượt cho CÙNG một
// bằng chứng, đốt ~5.65 đơn vị hạn mức của chủ máy — hết hạn mức thì chính Hà
// bị chặn làm việc. Nên: ước tính TRƯỚC, vượt trần thì KHÔNG gọi; bỏ qua phải
// ghi rõ ra màn — một kịch bản lặng lẽ nhảy cóc rồi báo xanh còn tệ hơn.
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

const hub = (args) =>
  JSON.parse(
    execFileSync(HERE + "rust/target/release/hub", args, {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    })
  );

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

  // Pick a session that is NOT moving. A live one keeps appending, and the
  // 256 KB window slides as it grows, so "count on screen == count in a later
  // read" is false by construction — the first run of this script compared 84
  // against 81 and blamed the product for a race in the measurement.
  const truth = hub(["sessions", "--json"]);
  const idleFor = (s) =>
    s.last_activity ? (Date.now() - Date.parse(s.last_activity)) / 60000 : Infinity;
  // Trong các phiên đứng yên, chọn phiên có NHẬT KÝ NGẮN NHẤT: bước /handover
  // bên dưới nạp cả hội thoại, nên phiên càng dài thì lượt gọi càng nặng và
  // càng lâu. Chạy trên phiên 20 MB không kiểm thêm được gì.
  const idle = truth.sessions.filter((s) => s.last_text && !s.note && idleFor(s) > 30);
  const pool = (idle.length ? idle : truth.sessions.filter((s) => s.last_text && !s.note));
  const target =
    pool.map((s) => ({ s, mb: transcriptMB(s.session_id) }))
        .sort((a, b) => a.mb - b.mb)
        .map((x) => x.s)[0] || truth.sessions[0];
  const targetMB = transcriptMB(target.session_id);
  console.log(
    `chạm vào phiên: ${target.name} (${target.account}, ` +
    `${targetMB.toFixed(2)} MB nhật ký ≈ ${(targetMB * USD_PER_MB).toFixed(2)} đơn vị hạn mức cho bước bàn giao)\n`
  );

  await page.locator(`.sess[data-session="${target.session_id}"]`).click();
  check("chạm vào phiên thì mở màn chi tiết", await page.locator("#sessDetail").isVisible());
  check("danh sách nhường chỗ cho chi tiết", await page.locator("#sessListPanel").isHidden());

  // hub has to receive /session through the room, follow it, and push. Give it
  // a real cycle, then read what actually arrived.
  // Playwright's second argument is the ARG passed to the function, not
  // options — passing `{timeout}` there silently falls back to the 30s default
  // (đã đạp 2026-08-07, ghi trong active-context). `null` then options.
  await page.waitForFunction(
    () => document.querySelectorAll("#sessStream .ev").length > 0,
    null,
    { timeout: 180000, polling: 1000 }
  );
  const shown = await page.evaluate(() => {
    const kinds = {};
    document.querySelectorAll("#sessStream .ev").forEach((el) => {
      const k = [...el.classList].find((c) => c.startsWith("ev-") && c !== "ev-hidden");
      kinds[k] = (kinds[k] || 0) + 1;
    });
    return {
      total: document.querySelectorAll("#sessStream .ev").length,
      kinds,
      status: document.getElementById("sessStatus").textContent,
      firstTool: document.querySelector("#sessStream .ev-tool .ev-body")?.textContent || "",
      firstResult: document.querySelector("#sessStream .ev-result .ev-body")?.textContent || "",
    };
  });

  // The machine's own answer, read independently.
  const snap = hub(["portal-push", "--dry-run"]);
  const focus = snap.sessions.focus;
  check("hub đang theo đúng phiên vừa chạm", focus && focus.session_id === target.session_id);
  check(
    "số sự kiện trên màn khớp ảnh chụp",
    focus && shown.total === focus.events.length,
    `màn ${shown.total} / ảnh chụp ${focus ? focus.events.length : "?"}`
  );

  // ĐỌC DỞ THÌ PHẢI Ở YÊN. Hà 2026-08-09: *"vào một phiên… view bị nhảy khi đã
  // cuộn xuống dưới cùng, có vẻ đang tải lại và render toàn bộ danh sách"*.
  // Đúng: cứ 4 giây `renderStream` xoá sạch rồi dựng lại cả trăm sự kiện.
  //
  // Phép đo bám vào thứ MẮT thấy, không bám `scrollTop`: chọn dòng đầu tiên
  // còn trong khung nhìn, nhớ CHỮ của nó và toạ độ y, chờ qua ít nhất một nhịp
  // làm mới, rồi đòi vẫn đúng dòng ấy ở đúng chỗ ấy. Bám `scrollTop` sẽ xanh
  // giả khi cửa sổ luồng trượt (sự kiện cũ rụng khỏi đầu, trang ngắn lại).
  const readAnchor = () =>
    page.evaluate(() => {
      const el = [...document.querySelectorAll("#sessStream .ev")]
        .find((n) => n.getBoundingClientRect().bottom > 0);
      const m = document.querySelector("main");
      return {
        text: (el?.textContent || "").replace(/\s+/g, " ").trim().slice(0, 40),
        y: el ? Math.round(el.getBoundingClientRect().top) : null,
        toBottom: Math.round(m.scrollHeight - m.scrollTop - m.clientHeight),
      };
    });

  await page.evaluate(() => {
    const m = document.querySelector("main");
    m.scrollTop = Math.round(m.scrollHeight * 0.55);
  });
  await page.waitForTimeout(400);
  const beforeMid = await readAnchor();
  await page.waitForTimeout(9500);            // qua ít nhất hai nhịp 4s
  const afterMid = await readAnchor();
  check(
    "đang đọc dở thì làm mới KHÔNG đẩy chỗ đọc đi",
    beforeMid.text.length > 0 &&
      afterMid.text === beforeMid.text &&
      Math.abs(afterMid.y - beforeMid.y) <= 4,
    `"${beforeMid.text}" y=${beforeMid.y} → "${afterMid.text}" y=${afterMid.y}`
  );

  await page.evaluate(() => {
    const m = document.querySelector("main");
    m.scrollTop = m.scrollHeight;
  });
  await page.waitForTimeout(9500);
  const atBottom = await readAnchor();
  check("đang ở đáy thì làm mới vẫn giữ ở đáy", atBottom.toBottom <= 4, `cách đáy ${atBottom.toBottom}px`);

  // Không hộp cuộn nào bên trong màn này: một trang, một thanh cuộn.
  const innerScrolls = await page.evaluate(() =>
    [...document.querySelectorAll("#sessDetail *")]
      .filter((e) => !/^(TEXTAREA|SELECT|INPUT)$/.test(e.tagName))
      .filter((e) => {
        const cs = getComputedStyle(e);
        if (cs.display === "none") return false;
        const cy = /(auto|scroll)/.test(cs.overflowY) && e.scrollHeight > e.clientHeight + 2;
        const cx = /(auto|scroll)/.test(cs.overflowX) && e.scrollWidth > e.clientWidth + 2;
        return cy || cx;
      })
      .map((e) => `${e.tagName.toLowerCase()}.${(e.className || "").toString().split(" ")[0]}`)
  );
  check("không hộp cuộn nào trong luồng phiên", innerScrolls.length === 0, innerScrolls.join(" · "));

  // "Giống như ngồi máy" nghĩa là thấy LỆNH và KẾT QUẢ, không chỉ lời nói.
  check("có hiện lệnh đã chạy", (shown.kinds["ev-tool"] || 0) > 0, `${shown.kinds["ev-tool"] || 0} lệnh`);
  check("có hiện kết quả của lệnh", (shown.kinds["ev-result"] || 0) > 0, `${shown.kinds["ev-result"] || 0} kết quả`);
  check("lệnh hiện kèm tham số", shown.firstTool.length > 0, shown.firstTool.slice(0, 50));
  check("kết quả có nội dung", shown.firstResult.length > 0, shown.firstResult.slice(0, 50));

  // UC-S03, nửa phía trang: mở phiên rồi thì trang phải tự hỏi lại liên tục.
  // Bản trước dừng ngay sau lần đầu nhận được luồng, nên màn đóng băng ở trạng
  // thái của một phút trước mà không có dấu hiệu gì.
  let reads = 0;
  const countReads = (req) => {
    if (/\/app\/(doc|resource)/.test(req.url())) reads += 1;
  };
  page.on("request", countReads);
  await page.waitForTimeout(15000);
  page.off("request", countReads);
  check(
    "đang mở phiên thì trang tự làm mới (≥3 lần/15 giây)",
    reads >= 3,
    `${reads} lần trong 15 giây`
  );

  const over = await page.evaluate(() => ({
    w: document.documentElement.scrollWidth,
    inner: window.innerWidth,
  }));
  check("trang không tràn ngang", over.w <= over.inner + 1, `${over.w}/${over.inner}`);
  await page.screenshot({ path: `${SHOTS}stream-01-phone.png` });

  // UC-S07 — đóng sổ để mở phiên mới làm tiếp. Nó gọi claude thật mỗi lần chạy,
  // nên nhắm phiên có nhật ký ngắn nhất: phiên dài hơn không chứng minh thêm
  // điều gì, chỉ chậm hơn.
  // Bản bàn giao CŨ vẫn nằm trong cursor từ lần chạy trước, nên "có tồn tại
  // không" là phép đo sai — phải hỏi "có bản MỚI trong lượt này không".
  const handoverBefore = snap.sessions.handover?.ts || "";
  // Không còn trần chặn thao tác của chủ máy (đã gỡ 2026-08-08) — chỉ còn SỔ.
  // Bản trước đọc `snap.budget` (trần robot) rồi tự suy lại luật, và xanh chỉ
  // vì hai trần tình cờ cùng kết luận: một phép đo mù.
  // Ảnh chụp KHÔNG được mang số tiền nào nữa (gỡ 2026-08-08). Đòi vắng mặt,
  // vì đây là thứ đã mọc lại một lần rồi: trần → giá → và sẽ lại là gì đó.
  check(
    "ảnh chụp KHÔNG mang số tiền nào",
    snap.owner_spend === undefined && snap.owner_budget === undefined
  );
  const willPay = affordable(targetMB, "UC-S07 /handover");
  if (willPay) {
  await page.click("#sessHandover");
  await page.waitForFunction(
    () => {
      const t = document.getElementById("sessHandoverBox").textContent || "";
      return t.length > 0 && !t.startsWith("Đang xin");
    },
    null,
    { timeout: 180000, polling: 1000 }
  ).catch(() => {});
  const hv = await page.evaluate(() => ({
    box: document.getElementById("sessHandoverBox").textContent || "",
    // `cmdNote` là HÀM, không phải id — nó ghi vào `#cmdStatus`.
    note: document.getElementById("cmdStatus")?.textContent || "",
  }));

  {
    // Chờ ảnh chụp mang bản bàn giao MỚI. Từ 08-08 màn nhận lời đáp thẳng từ
    // phòng chat, tức nó hiện xong TRƯỚC khi hubd kịp đẩy ảnh chụp — nên "hộp
    // đã đổi chữ" không còn đồng nghĩa "trạng thái đã tới". Bản trước đọc ngay
    // và báo đỏ trong khi hub vừa làm xong đúng việc của nó.
    let after = hub(["portal-push", "--dry-run"]);
    for (let i = 0; i < 40 && (after.sessions.handover?.ts || "") === handoverBefore; i++) {
      await page.waitForTimeout(3000);
      after = hub(["portal-push", "--dry-run"]);
    }
    const h = after.sessions.handover;
    check("có bản bàn giao MỚI", !!h && h.ts !== handoverBefore && h.source_id === target.session_id);
    check("phiên mới KHÁC phiên cũ", h && h.new_session_id !== h.source_id, h ? h.new_session_id.slice(0, 8) : "");
    check("có lệnh để tiếp tục trên máy", !!(h && h.resume_command.includes("--resume")), h ? h.resume_command : "");
    check("màn hiện bản bàn giao", hv.box.includes("Đã đóng sổ"), hv.box.slice(0, 60));
  }
  }

  // Quay lại phải thôi theo, không để hub gánh luồng không ai đọc.
  await page.click("#sessBack");
  check("quay lại thì hiện danh sách", await page.locator("#sessListPanel").isVisible());
  // Cùng lý do: chờ ảnh chụp bắt kịp thay vì đếm 9 giây rồi kết luận.
  let after = hub(["portal-push", "--dry-run"]);
  for (let i = 0; i < 20 && after.sessions.focus; i++) {
    await page.waitForTimeout(3000);
    after = hub(["portal-push", "--dry-run"]);
  }
  check(
    "quay lại thì hub thôi theo phiên",
    !after.sessions.focus,
    after.sessions.focus ? `vẫn theo ${after.sessions.focus.session_id.slice(0, 8)}` : ""
  );
} catch (e) {
  problems.push(`ngoại lệ: ${e.message}`);
} finally {
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(
  `\n${passed}/${checks.length} kiểm tra qua · ${problems.length} vấn đề` +
  (skipped.length ? ` · ${skipped.length} BỎ QUA vì tốn hạn mức` : "")
);
// Một lần bỏ qua mà không nói ra thì bản tóm tắt đang nói dối: "xanh trọn" và
// "xanh phần không mất tiền" là hai kết luận khác nhau.
if (skipped.length) {
  console.log("\nCHƯA NGHIỆM THU (không gọi claude lượt này):");
  skipped.forEach((s) => console.log(`  ⏭ ${s}`));
  console.log("  → chạy lại với HUB_UC_PAY=1 khi cần bằng chứng đường tốn hạn mức.");
}
if (problems.length) {
  console.log("\nVẤN ĐỀ:");
  problems.forEach((p) => console.log(`  - ${p}`));
  process.exit(1);
}
console.log("ảnh: ui-shots/stream-01-phone.png");
