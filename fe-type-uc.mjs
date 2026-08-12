// UC-S08 — gõ thẳng vào cửa sổ phiên, qua đúng UI của điện thoại.
//
// Vì sao có kịch bản này: 2026-08-10 hub báo "⌨ đã bấm" trong khi Hà **không
// thấy hiện tượng gì**. Mã trả về của `osascript` bằng 0 chỉ chứng minh byte đã
// vào tab — nó không nói `claude` bên trong làm gì với byte ấy. Nên phép đo ở
// đây KHÔNG hỏi "hub có báo thành công không", mà hỏi **"hub có nói đúng chỗ
// chữ đã tới không"**: hàng chờ, đang chạy, hay dấu nhắc.
//
// ⚠ Kịch bản này gõ THẬT vào một phiên THẬT của chủ máy — không có đường giả.
// Nội dung gửi đi tự giới thiệu nó là phép kiểm để phiên nhận không hiểu nhầm.
import { chromium } from "/Users/hanguyen/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";

const [app, u, p, want] = process.argv.slice(2);
const PROBE = "(hub tự kiểm đường gõ — bỏ qua tin này)";

const br = await chromium.launch();
const pg = await br.newPage({ viewport: { width: 390, height: 844 } });
const errs = [];
pg.on("console", (m) => m.type() === "error" && errs.push(m.text()));
pg.on("pageerror", (e) => errs.push(String(e)));

let pass = 0;
const notes = [];
const ok = (c, msg) => {
  if (c) { pass++; console.log("  ✓", msg); }
  else { notes.push(msg); console.log("  ✗", msg); }
  return c;
};

await pg.goto(`http://${app}.test.localhost:8090`, { waitUntil: "domcontentloaded" });
await pg.fill("#u", u); await pg.fill("#p", p); await pg.click("#loginBtn");
await pg.waitForSelector("#panelTabs", { timeout: 20000 });
await pg.waitForFunction(
  () => document.querySelectorAll("#sessList .sess").length > 0,
  { timeout: 25000 },
);

// Phiên đích: phiên terminal (có cửa sổ để gõ vào). Cho phép chỉ định qua argv
// để chạy lại đúng phiên đang cần soi.
const sel = want
  ? `.sess[data-session^="${want}"]`
  : `.sess[data-host="terminal"]`;
const row = pg.locator(sel).first();
if (!(await row.count())) {
  console.log("\n⏭ BỎ QUA: không có phiên terminal nào đang chạy để gõ vào.");
  await br.close(); process.exit(0);
}
const sid = await row.getAttribute("data-session");
const name = (await row.locator(".sess-name").first().innerText().catch(() => "")).trim();
await row.click();
await pg.waitForSelector("#sessDetail:not(.hidden)", { timeout: 10000 });
console.log(`\nPhiên đích: ${name || sid?.slice(0, 8)}\n`);

// ── 1. Mặc định của ô nhập ────────────────────────────────────────────────
// Hà 2026-08-09: "mặc định là hỏi phiên nếu đó là phiên có thể gõ vào được".
const btn = (await pg.innerText("#sessSay")).trim();
ok(btn.includes("Gõ"), `ô nhập mặc định là GÕ THẲNG vào phiên (nút: "${btn}")`);
ok(
  !(await pg.isChecked("#sessAside")),
  '"Hỏi bên lề" mặc định KHÔNG tích — hỏi ngoài là lựa chọn, không phải mặc định',
);

// ── 1b. Mọi nút phải CÓ TAY BẤM ───────────────────────────────────────────
// Rào cấu trúc, không phải rào thẩm mỹ. Lỗi Hà gặp hôm nay là `#sessSay` được
// vẽ ra, có nhãn "⌨ Gõ", nằm đúng chỗ — mà không ai nối `click`; và cùng lần
// cắt ấy `#sessNew` với `#sessStop` cũng mất tay bấm. Kịch bản nào chỉ nhìn
// nhãn và vị trí đều xanh, vì cái sai không nằm ở hình mà ở chỗ nối. Nên hỏi
// thẳng trình duyệt: nút này có người nghe không?
const cdp = await pg.context().newCDPSession(pg);
await cdp.send("DOM.enable");
const { root } = await cdp.send("DOM.getDocument");
for (const q of ["#sessSay", "#sessNew", "#sessStop", "#sessHandover", "#sessBack"]) {
  const { nodeId } = await cdp.send("DOM.querySelector", { nodeId: root.nodeId, selector: q });
  if (!nodeId) { ok(false, `${q} có mặt trên trang`); continue; }
  const { object } = await cdp.send("DOM.resolveNode", { nodeId });
  const { listeners } = await cdp.send("DOMDebugger.getEventListeners", { objectId: object.objectId });
  ok(
    listeners.some((l) => l.type === "click"),
    `${q} có tay bấm click (nút vẽ ra mà không nối là nút chết)`,
  );
}

// ── 2. Gõ thật ────────────────────────────────────────────────────────────
await pg.fill("#sessSayInput", PROBE);
await pg.click("#sessSay");

// ── 3. Hub phải nói ĐÚNG CHỖ chữ đã tới ───────────────────────────────────
const LANDINGS = ["HÀNG CHỜ", "bắt đầu chạy", "dấu nhắc"];
await pg.click('#panelTabs button[data-panel="chat"]');
// Trước khi tin vào một phép đo, kiểm xem nó có NHÌN THẤY gì không: selector
// không khớp cái gì thì mọi kết luận sau đó là kết luận về hư vô.
await pg.waitForSelector(".msg .body", { timeout: 20000 });
ok(
  (await pg.locator(".msg .body").count()) > 0,
  "phép đo nhìn thấy được bong bóng chat (không phải selector rỗng)",
);
let reply = "";
try {
  // Selector phải là thứ trang THẬT dựng ra: `.msg .body` (giống fe-smoke).
  // Bản đầu của kịch bản này hỏi `#log .msg` — một id không tồn tại — nên nó
  // báo "hub không trả lời" trong khi hub đã trả lời đúng sau 7 giây. Phép đo
  // luôn-rỗng là phép đo mù, và nó tố cáo mã sai chứ không tố cáo chính nó.
  reply = await pg.waitForFunction(() => {
    const t = [...document.querySelectorAll(".msg .body")]
      .map((e) => e.textContent || "")
      .filter((s) => s.includes("⌨") || s.includes("không gõ được"));
    return t.length ? t[t.length - 1] : null;
  }, { timeout: 45000 }).then((h) => h.jsonValue());
} catch { /* để phép đo tự báo đỏ bên dưới */ }

console.log(`\n  hub trả lời: ${reply ? reply.replace(/\s+/g, " ").slice(0, 160) : "(không có)"}\n`);
ok(!!reply, "hub có trả lời cho lệnh gõ");
ok(!/không gõ được/.test(reply), "không dính lỗi quyền (System Events 1002)");
const landed = LANDINGS.find((l) => reply.includes(l));
ok(
  !!landed,
  `hub nói RÕ chữ đã tới đâu (một trong: ${LANDINGS.join(" · ")})`,
);
if (landed) console.log(`     → chữ nằm ở: ${landed}`);

// ── 4. Không được im lặng nuốt lỗi ────────────────────────────────────────
ok(errs.length === 0, `0 lỗi console (thấy ${errs.length})`);

const total = pass + notes.length;
console.log(`\n${pass}/${total} kiểm tra qua · ${notes.length} vấn đề`);
notes.forEach((n) => console.log("  •", n));
await br.close();
process.exit(notes.length ? 1 : 0);
