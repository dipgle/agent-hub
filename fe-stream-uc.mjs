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
import { mkdirSync } from "node:fs";

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
  const target =
    truth.sessions.find((s) => s.last_text && !s.note && idleFor(s) > 30) ||
    truth.sessions.find((s) => s.last_text && !s.note) ||
    truth.sessions[0];
  console.log(`chạm vào phiên: ${target.name} (${target.account})\n`);

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

  // UC-S07 — đóng sổ để mở phiên mới làm tiếp. Đây là đường CHI TIỀN, nên
  // kiểm cả hai nhánh: còn tiền thì phải ra bản bàn giao + id mới; hết tiền thì
  // phải từ chối và NÓI RÕ, không được im lặng tiêu tiếp.
  // Bản bàn giao CŨ vẫn nằm trong cursor từ lần chạy trước, nên "có tồn tại
  // không" là phép đo sai — phải hỏi "có bản MỚI trong lượt này không".
  const handoverBefore = snap.sessions.handover?.ts || "";
  // Trần của CHỦ MÁY, không phải trần robot. Bản trước đọc `snap.budget`
  // (daily_budget_usd — thứ ghìm con robot chạy không ai trông) trong khi sản
  // phẩm gác bằng `owner_daily_budget_usd`; nó xanh chỉ vì hai trần tình cờ
  // cùng kết luận từ chối, tức là một phép đo mù.
  //
  // Và không tự suy lại luật nữa: đọc thẳng `blocks_owner_action` — chính
  // quyết định sản phẩm dùng. Phép đo tự tính lại quy tắc là phép đo có thể
  // gật gù cùng một sản phẩm đã hỏng.
  const owner = snap.owner_budget || {};
  const blocked = owner.blocks_owner_action === true;
  console.log(
    `\ntrần chủ máy: đã dùng $${Number(owner.spent_usd ?? 0).toFixed(3)}/$${Number(owner.cap_usd ?? 0).toFixed(2)}` +
    ` · mỗi lần gọi tối đa $${Number(owner.per_call_usd ?? 0).toFixed(2)} · chặn: ${blocked}`
  );
  check(
    "ảnh chụp có công bố trần của chủ máy",
    typeof owner.blocks_owner_action === "boolean" && owner.cap_usd > 0,
    JSON.stringify(owner)
  );
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
    note: document.getElementById("cmdNote")?.textContent || "",
  }));

  if (blocked) {
    // Không đủ tiền: hub phải từ chối ở phòng chat, và màn không được vờ như xong.
    const after = hub(["portal-push", "--dry-run"]);
    const now = after.sessions.handover?.ts || "";
    check(
      "hết ngân sách thì KHÔNG bàn giao lén",
      now === handoverBefore,
      now === handoverBefore ? "không sinh bản mới — đúng" : `sinh bản mới lúc ${now}`
    );
    console.log("  · ngân sách không đủ để kiểm đường thành công của UC-S07 hôm nay");
  } else {
    const after = hub(["portal-push", "--dry-run"]);
    const h = after.sessions.handover;
    check("có bản bàn giao MỚI", !!h && h.ts !== handoverBefore && h.source_id === target.session_id);
    check("phiên mới KHÁC phiên cũ", h && h.new_session_id !== h.source_id, h ? h.new_session_id.slice(0, 8) : "");
    check("có lệnh để tiếp tục trên máy", !!(h && h.resume_command.includes("--resume")), h ? h.resume_command : "");
    check("tiền vào sổ chi", !!(h && h.cost_usd > 0), h ? `$${h.cost_usd}` : "");
    check("màn hiện bản bàn giao", hv.box.includes("Đã đóng sổ"), hv.box.slice(0, 60));
  }

  // Quay lại phải thôi theo, không để hub gánh luồng không ai đọc.
  await page.click("#sessBack");
  check("quay lại thì hiện danh sách", await page.locator("#sessListPanel").isVisible());
  await page.waitForTimeout(9000);
  const after = hub(["portal-push", "--dry-run"]);
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
console.log(`\n${passed}/${checks.length} kiểm tra qua · ${problems.length} vấn đề`);
if (problems.length) {
  console.log("\nVẤN ĐỀ:");
  problems.forEach((p) => console.log(`  - ${p}`));
  process.exit(1);
}
console.log("ảnh: ui-shots/stream-01-phone.png");
