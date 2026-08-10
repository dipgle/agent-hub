// UC-S02b — "một phiên đang chạy subagent thì màn hình phải NÓI RA", nghiệm thu
// trên UI THẬT ở cỡ điện thoại.
//
// Vì sao UC này tồn tại (Hà 2026-08-09: *"một phiên đang chạy subagents thì cũng
// hiển thị được"*): một phiên giao hết việc cho subagent trông ĐỨNG IM từ bên
// ngoài — lượt hội thoại cuối đã vài phút trước và không có gì mới rơi vào nhật
// ký cho tới khi subagent quay về. Đứng từ điện thoại mà không có con số này thì
// "đang chạy" và "treo" nhìn giống hệt nhau.
//
// Vì sao nó nợ tới hôm nay: đường hiển thị mới chỉ được ghim bằng 13 unit test
// trong `rust/tests/sessions.rs`; lúc nghiệm thu không phiên nào trên máy đang
// chạy subagent, nên chưa ai THẤY nó vẽ ra.
//
// NGUỒN SỰ THẬT PHẢI ĐỘC LẬP VỚI HUB. So màn hình với `hub sessions --json` chỉ
// chứng minh trang vẽ trung thành cái hub tính — nó không chứng minh hub đếm
// đúng, vì cả hai vế cùng một mã. Nên kịch bản này tự đọc nhật ký phiên
// (`~/.claude*/projects/*/<session_id>.jsonl`), cùng cửa sổ 256KB mà
// `sessions.rs:41` dùng, và tự đếm lại bằng JS. `hub sessions --json` chỉ được
// dùng cho DANH SÁCH phiên (thứ không nằm trong phạm vi UC này).
//
// PHÉP ĐO ĐI HAI CHIỀU, để nó có thể ĐỎ:
//   - phiên ĐANG chạy subagent  → thẻ + màn chi tiết phải hiện đúng CON SỐ;
//   - phiên KHÔNG chạy subagent → hai chỗ đó phải KHÔNG có gì.
// Nếu trang luôn vẽ huy hiệu, chiều âm đỏ; nếu không bao giờ vẽ, chiều dương đỏ.
// Và nếu lúc chạy không phiên nào có subagent thì kịch bản THOÁT MÃ 2 và nói
// "chưa dựng được trạng thái cần đo" — không đếm là đạt.
//
// Usage: node fe-subagent-uc.mjs
import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { readFileSync, mkdirSync, readdirSync, existsSync, openSync, fstatSync, readSync, closeSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { homedir } from "node:os";

const HERE = new URL("./", import.meta.url).pathname;
const env = Object.fromEntries(
  readFileSync(HERE + "hub.env", "utf8")
    .split("\n").filter((l) => l.includes("=") && !l.trim().startsWith("#"))
    .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim().replace(/^["']|["']$/g, "")])
);
const APP_TID = env.HUB_TFL5_APP_TID || "a-65dd60d3-624e-45a9-8fdf-62aa7d894d80";
const SHOTS = HERE + "ui-shots/";
mkdirSync(SHOTS, { recursive: true });
const PHONE = { width: 390, height: 844 };
const TAIL_BYTES = 256 * 1024; // giống hệt sessions.rs:41

const problems = [];
const checks = [];
const check = (name, ok, detail = "") => {
  checks.push({ name, ok });
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) problems.push(`${name}${detail ? `: ${detail}` : ""}`);
};

// ——— nguồn sự thật: đọc thẳng nhật ký, không hỏi hub ———

/// Nhật ký của một phiên nằm dưới thư mục cấu hình của TÀI KHOẢN chạy nó, mà
/// máy này có nhiều tài khoản (`~/.claude`, `~/.claude-acc2`, …). Quét thay vì
/// dựng lại đường dẫn: dựng lại là chép logic của hub, tức lại hỏi hub lần nữa.
function findTranscript(sessionId) {
  const home = homedir();
  for (const dir of readdirSync(home)) {
    if (!dir.startsWith(".claude")) continue;
    const root = `${home}/${dir}/projects`;
    if (!existsSync(root)) continue;
    for (const proj of readdirSync(root)) {
      const p = `${root}/${proj}/${sessionId}.jsonl`;
      if (existsSync(p)) return p;
    }
  }
  return null;
}

function readTail(path) {
  const fd = openSync(path, "r");
  try {
    const len = fstatSync(fd).size;
    const start = len > TAIL_BYTES ? len - TAIL_BYTES : 0;
    const buf = Buffer.alloc(Number(len - start));
    readSync(fd, buf, 0, buf.length, start);
    return buf.toString("utf8");
  } finally {
    closeSync(fd);
  }
}

/// `toolUseId` của những lệnh gọi đã đẻ ra subagent CHẠY NỀN.
///
/// Chạy nền nhận `tool_result` ngay lập tức ("đã tung agent"), nên với chúng
/// `tool_result` KHÔNG phải dấu kết thúc. Hỏi đĩa để biết lệnh gọi nào là nền:
/// `<thư mục>/<session_id>/subagents/agent-<id>.meta.json` mang `toolUseId`.
function backgroundCalls(transcriptPath) {
  const dir = transcriptPath.replace(/\.jsonl$/, "") + "/subagents";
  const out = new Set();
  if (!existsSync(dir)) return out;
  for (const f of readdirSync(dir)) {
    if (!(f.startsWith("agent-") && f.endsWith(".meta.json"))) continue;
    try {
      const id = JSON.parse(readFileSync(`${dir}/${f}`, "utf8"))?.toolUseId;
      if (id) out.add(id);
    } catch { /* nói ra ở dưới nếu số liệu lệch; ở đây bỏ qua một file hỏng */ }
  }
  return out;
}

/// Đếm subagent chưa quay về, khớp theo `tool_use_id` — KHÔNG đếm theo tên.
/// Tung 5 nhận về 3 mà báo "5 đang chạy" là sai đúng lúc con số ấy cần đúng
/// nhất. Kết quả rơi ngoài cửa sổ 256KB coi như đã xong: thà thiếu còn hơn để
/// một subagent ma chạy mãi trên màn.
///
/// Hai chế độ, hai dấu kết thúc: agent CHẶN xong khi có `tool_result`; agent
/// NỀN xong khi nhật ký cha ghi `<task-notification>` mang cùng tool-use-id.
function pendingSubagents(path) {
  const background = backgroundCalls(path);
  const started = [];
  const finished = new Set();
  const stopped = new Set();
  for (const line of readTail(path).split("\n")) {
    let rec;
    try { rec = JSON.parse(line); } catch { continue; } // dòng đầu bị cắt giữa chừng là bình thường
    const blocks = rec?.message?.content;
    if (!Array.isArray(blocks)) continue;
    for (const b of blocks) {
      if (b?.type === "tool_use" && (b.name === "Agent" || b.name === "Task") && b.id) started.push(b.id);
      else if (b?.type === "tool_result" && b.tool_use_id) finished.add(b.tool_use_id);
      else if (typeof b?.text === "string" && b.text.includes("<task-notification>")) {
        // Chỉ đọc BÊN TRONG từng khối thông báo. Quét cả đoạn thì một phiên
        // đang bàn về chính tính năng này (lời văn có nhắc hai thứ thẻ) sẽ
        // đóng nhầm một lệnh gọi đang chạy — và bản đếm "độc lập" này sẽ đồng
        // ý với con bug thay vì bắt nó.
        // ĐÒI thẻ đóng, y như bản Rust: khối mở mà không đóng là lời văn, không
        // phải thông báo — mỗi bản ghi là một dòng JSON trọn vẹn, dòng bị cắt
        // thì trượt JSON.parse và bị bỏ cả dòng.
        for (const blk of b.text.matchAll(/<task-notification>([\s\S]*?)<\/task-notification>/g)) {
          for (const m of blk[1].matchAll(/<tool-use-id>([^<]+)<\/tool-use-id>/g)) stopped.add(m[1]);
        }
      }
    }
  }
  return started.filter((id) => (background.has(id) ? !stopped.has(id) : !finished.has(id))).length;
}

/// Tiến trình còn sống không? Hỏi HỆ ĐIỀU HÀNH, không hỏi lại hub — nếu hub bịa
/// trạng thái sống/chết thì `ps` sẽ cãi. Phiên đã chết thì không thể có gì
/// "đang chạy", dù nhật ký của nó dừng lại giữa chừng ở trạng thái vừa tung.
function alive(pid) {
  if (!pid || pid <= 0) return false;
  try {
    execFileSync("/bin/ps", ["-p", String(pid)], { stdio: ["ignore", "ignore", "ignore"] });
    return true;
  } catch { return false; }
}

/// Ảnh chụp sự thật cho MỌI phiên đang sống. `hub sessions --json` chỉ cho danh
/// sách id — con số subagent thì tự đếm lấy.
function truthNow(roster) {
  const out = new Map();
  for (const s of roster) {
    const p = findTranscript(s.session_id);
    if (!p) { out.set(s.session_id, null); continue; }
    out.set(s.session_id, alive(s.pid) ? pendingSubagents(p) : 0);
  }
  return out;
}

const roster = JSON.parse(
  execFileSync(HERE + "rust/target/release/hub", ["sessions", "--json"], {
    encoding: "utf8", stdio: ["ignore", "pipe", "ignore"],
  })
).sessions;

const t0 = truthNow(roster);
const targets = roster.filter((s) => t0.get(s.session_id) > 0);
console.log(`máy đang có ${roster.length} phiên; đếm từ nhật ký:`);
for (const s of roster) {
  const n = t0.get(s.session_id);
  console.log(`  ${s.name.padEnd(28)} ${n === null ? "(không thấy nhật ký)" : `${n} subagent chưa về`}`);
}
console.log();

if (targets.length === 0) {
  console.error("✗ CHƯA DỰNG ĐƯỢC TRẠNG THÁI CẦN ĐO: không phiên nào đang chạy subagent.");
  console.error("  UC-S02b KHÔNG được tính là đạt. Tung một subagent thật rồi chạy lại.");
  process.exit(2);
}
const zeros = roster.filter((s) => t0.get(s.session_id) === 0);
// Không thấy nhật ký = KHÔNG KIỂM ĐƯỢC, không phải "0 subagent". Coi nó là 0 là
// tự bịa ra một chiều âm rồi tự chấm đạt — đúng cái bẫy phép-đo-mù mà UC này
// tồn tại để tránh. Nói thẳng ra và loại khỏi cả hai chiều.
const unknown = roster.filter((s) => t0.get(s.session_id) === null);
console.log(`đích: ${targets.map((s) => `${s.name}=${t0.get(s.session_id)}`).join(", ")}`);
console.log(`chiều âm: ${zeros.length} phiên không có subagent`);
if (unknown.length) {
  console.log(`không kiểm được (không thấy nhật ký): ${unknown.map((s) => s.name).join(", ")}`);
}
console.log();

/// Mở lý lịch phiên bằng đúng cú chạm người dùng làm — nhưng chỉ khi nó đang
/// gấp. `<details id="sessInfoBox">` là phần tử TĨNH của trang: mở ở phiên này
/// thì sang phiên khác nó vẫn mở, nên chạm vô điều kiện là tự tay ĐÓNG nó lại
/// (đỏ lượt đầu 2026-08-10, và đỏ vì phép đo chứ không vì sản phẩm).
async function openInfo(page) {
  const open = await page.evaluate(() => document.getElementById("sessInfoBox")?.open === true);
  if (!open) await page.click("#sessInfoBox summary");
  await page.waitForSelector("#sessInfo .rt-row", { timeout: 20000 });
}

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: PHONE, deviceScaleFactor: 3, isMobile: true, hasTouch: true });
page.on("console", (m) => { if (m.type() === "error") problems.push(`console.error: ${m.text()}`); });
page.on("pageerror", (e) => problems.push(`uncaught: ${e.message}`));

try {
  await page.goto(`http://${APP_TID}.test.localhost:8090`, { waitUntil: "domcontentloaded" });
  await page.fill("#u", "alice_local");
  await page.fill("#p", env.HUB_TFL5_ALICE_PASSWORD);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 20000 });
  await page.waitForFunction(() => document.querySelectorAll("#sessList .sess").length > 0, { timeout: 25000 });

  // Ảnh chụp trên trang trễ tối đa một nhịp đẩy (~10s) cộng một nhịp đọc (15s).
  // Trong lúc ấy một subagent có thể quay về, và màn hình sẽ khác sự thật MÀ
  // KHÔNG AI SAI. Nên: đọc sự thật — đọc màn — đọc lại sự thật; chỉ so khi hai
  // đầu kẹp bằng nhau. Lệch thì làm mới trang (F5, đúng thứ người dùng làm) và
  // đo lại. Không có bước này thì phép đo chập chờn, và phép đo chập chờn dạy
  // người ta bỏ qua màu đỏ.
  let screen = null, truth = null;
  for (let attempt = 1; attempt <= 4; attempt++) {
    const before = truthNow(roster);
    screen = await page.evaluate(() =>
      [...document.querySelectorAll("#sessList .sess")].map((c) => ({
        session: c.dataset.session,
        badge: c.dataset.subagents ?? null,
        meta: (c.querySelector(".sess-meta")?.textContent || "").trim(),
      }))
    );
    const after = truthNow(roster);
    const stable = roster.every((s) => before.get(s.session_id) === after.get(s.session_id));
    if (stable) { truth = before; break; }
    console.log(`  … sự thật đổi giữa hai lần đọc (lượt ${attempt}), làm mới trang rồi đo lại`);
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForFunction(() => document.querySelectorAll("#sessList .sess").length > 0, { timeout: 25000 });
  }
  if (!truth) {
    check("sự thật đứng yên đủ lâu để đo", false, "subagent về/đi liên tục qua 4 lượt");
    throw new Error("không kẹp được phép đo");
  }

  // Ảnh chụp có thể trễ hơn sự thật một nhịp. Chờ tới khi trang bắt kịp thay vì
  // báo đỏ cho một độ trễ đã biết — nhưng có TRẦN, hết trần thì đỏ thật.
  const wanted = (sid) => truth.get(sid);
  const matches = () =>
    screen.every((c) => wanted(c.session) === null || Number(c.badge ?? 0) === wanted(c.session));
  for (let wait = 0; wait < 6 && !matches(); wait++) {
    await page.waitForTimeout(6000);
    screen = await page.evaluate(() =>
      [...document.querySelectorAll("#sessList .sess")].map((c) => ({
        session: c.dataset.session,
        badge: c.dataset.subagents ?? null,
        meta: (c.querySelector(".sess-meta")?.textContent || "").trim(),
      }))
    );
    const t = truthNow(roster);
    if (!roster.every((s) => t.get(s.session_id) === truth.get(s.session_id))) {
      console.log("  … sự thật đổi trong lúc chờ ảnh chụp bắt kịp — dừng chờ, so bằng số mới");
      truth = t;
    }
  }

  const byId = new Map(screen.map((c) => [c.session, c]));

  // ——— chiều DƯƠNG: phiên đang chạy subagent ———
  for (const s of targets) {
    const n = truth.get(s.session_id);
    const card = byId.get(s.session_id);
    if (!card) { check(`thẻ «${s.name}» có trên màn`, false, "không tìm thấy"); continue; }
    check(
      `«${s.name}» — thẻ khai đúng số subagent`,
      Number(card.badge) === n,
      `màn ${card.badge ?? "(không có)"} / nhật ký ${n}`
    );
    check(
      `«${s.name}» — chữ trên thẻ nói ra bằng tiếng người`,
      card.meta.includes(`${n} subagent đang chạy`),
      card.meta.slice(0, 90)
    );
  }

  // ——— chiều ÂM: phiên không có subagent thì không được có gì ———
  const falsePositives = zeros
    .map((s) => byId.get(s.session_id))
    .filter((c) => c && (c.badge !== null || /subagent/.test(c.meta)));
  check(
    `${zeros.length} phiên không chạy subagent thì thẻ KHÔNG nhắc tới subagent`,
    falsePositives.length === 0,
    falsePositives.map((c) => c.meta.slice(0, 50)).join(" | ")
  );

  await page.screenshot({ path: SHOTS + "subagent-list.png" });

  // ——— màn chi tiết, cũng hai chiều ———
  const target = targets[0];
  const nTarget = truth.get(target.session_id);
  await page.click(`#sessList .sess[data-session="${target.session_id}"]`);
  // Lý lịch phiên gấp sẵn trong `<details>` để không đẩy luồng xuống — nên mở
  // nó bằng đúng cú chạm người dùng làm, không `evaluate` cạy `open` ra.
  await openInfo(page);
  const rows = await page.evaluate(() =>
    [...document.querySelectorAll("#sessInfo .rt-row")].map((r) => [
      r.querySelector(".rt-k")?.textContent?.trim(),
      r.querySelector(".rt-v")?.textContent?.trim(),
    ])
  );
  const row = rows.find(([k]) => k === "subagent");
  check(
    "màn chi tiết có hàng «subagent»",
    !!row,
    rows.map(([k]) => k).join(", ")
  );
  check(
    "hàng «subagent» nói đúng con số",
    row?.[1] === `${nTarget} đang chạy`,
    `màn "${row?.[1] ?? "(không có)"}" / nhật ký ${nTarget}`
  );
  await page.screenshot({ path: SHOTS + "subagent-detail.png" });

  // Quay ra danh sách bằng nút thật, rồi mở một phiên KHÔNG có subagent: hàng
  // ấy phải vắng mặt. Không có vế này thì "có hàng subagent" có thể xanh vì
  // trang luôn vẽ nó.
  if (zeros.length) {
    await page.click("#sessBack");
    await page.waitForSelector("#sessList .sess", { timeout: 20000 });
    const z = zeros[0];
    await page.click(`#sessList .sess[data-session="${z.session_id}"]`);
    await openInfo(page);
    const zrows = await page.evaluate(() =>
      [...document.querySelectorAll("#sessInfo .rt-row")].map((r) => r.querySelector(".rt-k")?.textContent?.trim())
    );
    check(
      `phiên «${z.name}» không có subagent thì màn chi tiết KHÔNG có hàng ấy`,
      !zrows.includes("subagent"),
      zrows.join(", ")
    );
  }
} catch (e) {
  problems.push(`ngoại lệ: ${e.message}`);
} finally {
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra đạt`);
if (problems.length) {
  console.log("\nVẤN ĐỀ:");
  problems.forEach((p) => console.log(`  - ${p}`));
  process.exit(1);
}
