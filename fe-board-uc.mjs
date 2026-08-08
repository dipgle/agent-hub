// Acceptance for the board tab: the console's read-only view, now inside the
// chat page on tfl5.
//
// The journey is a person's: log in on the deployed bundle, click the tab,
// read the numbers. Nothing is faked — the snapshot must already have been
// pushed by `hub portal-push` / `hubd`, which is the point: if the push path
// is broken, this test fails instead of quietly rendering an empty screen.
//
// It also pins the security shape: the board must offer NO control that
// changes state (approve/reject live in the chat room and the console), and
// the page must never reach the loopback console.
//
// Usage: node fe-board-uc.mjs
import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { readFileSync, mkdirSync } from "node:fs";

const HERE = new URL("./", import.meta.url).pathname;
const env = Object.fromEntries(
  readFileSync(HERE + "hub.env", "utf8")
    .split("\n").filter((l) => l.includes("=") && !l.trim().startsWith("#"))
    .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim().replace(/^["']|["']$/g, "")])
);
const APP_TID = env.HUB_TFL5_APP_TID || "a-65dd60d3-624e-45a9-8fdf-62aa7d894d80";
const SHOTS = HERE + "ui-shots/";
mkdirSync(SHOTS, { recursive: true });

const checks = [];
const check = (name, ok, detail = "") => {
  checks.push({ name, ok });
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? ` — ${detail}` : ""}`);
};

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
const errors = [];
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text().slice(0, 200)); });
page.on("pageerror", (e) => errors.push(`uncaught: ${e.message.slice(0, 200)}`));

// Anything aimed at the loopback console would be a design break, so watch
// the wire rather than trusting the code to stay honest.
const offOrigin = [];
page.on("request", (r) => {
  const u = new URL(r.url());
  if (u.host !== `${APP_TID}.test.localhost:8090`) offOrigin.push(r.url());
});

try {
  await page.goto(`http://${APP_TID}.test.localhost:8090`, { waitUntil: "domcontentloaded" });
  await page.fill("#u", env.HUB_TFL5_USER);
  await page.fill("#p", env.HUB_TFL5_PASSWORD);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 15000 });
  // One screen: the board and the conversation are both up after login, no
  // tab to switch to.
  check("đăng nhập xong là vào thẳng bảng, có tab Trao đổi bên cạnh",
    (await page.locator("#board").isVisible()) &&
    (await page.locator('#panelTabs button[data-panel="chat"]').count()) === 1);

  await page.waitForFunction(
    () => !/đang tải/.test(document.getElementById("boardStamp").textContent),
    { timeout: 20000 }
  );
  const stamp = (await page.locator("#boardStamp").innerText()).trim();
  check("có ảnh chụp, không phải thông báo lỗi", /Ảnh chụp lúc/.test(stamp), stamp);

  const rows = await page.locator("#boardRows tr").count();
  check("bảng hộp việc có dữ liệu thật", rows > 1, `${rows} dòng`);
  const chips = await page.locator("#boardCounts .chip").count();
  check("có dải số tổng hợp", chips >= 3, `${chips} chip`);
  const spend = await page.locator("#boardCounts .chip", { hasText: "đã tiêu" }).innerText();
  check("hiện tổng chi phí", /\$\d/.test(spend), spend.replace(/\s+/g, " "));

  // The chart is ECharts (house rule). "A canvas exists" proves nothing —
  // the first version of this check passed against a chart that had drawn
  // only its axes. So: wait for the series to carry data, then COUNT PAINTED
  // PIXELS in the series colour.
  await page.click('#panelTabs button[data-panel="cost"]');
  await page.waitForSelector("#panel-cost:not(.hidden)", { timeout: 5000 });
  await page.waitForFunction(
    () => {
      const el = document.getElementById("boardCost");
      const inst = window.echarts && echarts.getInstanceByDom(el);
      const opt = inst && inst.getOption();
      return !!opt && opt.series.some((s) => (s.data || []).length > 0);
    },
    { timeout: 20000 }
  );
  await page.waitForTimeout(1500); // let the entry animation finish
  const paint = await page.evaluate(() => {
    const el = document.getElementById("boardCost");
    const inst = echarts.getInstanceByDom(el);
    const opt = inst.getOption();
    const cv = el.querySelector("canvas");
    const { width, height } = cv;
    const px = cv.getContext("2d").getImageData(0, 0, width, height).data;
    let coloured = 0;
    for (let i = 0; i < px.length; i += 4) {
      const [r, g, b, a] = [px[i], px[i + 1], px[i + 2], px[i + 3]];
      // Anything opaque that is not near-white/near-grey is series ink.
      if (a > 200 && Math.max(Math.abs(r - g), Math.abs(g - b), Math.abs(r - b)) > 40) coloured++;
    }
    return { series: opt.series.map((s) => (s.data || []).length), coloured, total: width * height };
  });
  check("biểu đồ có dữ liệu trong series", paint.series.every((n) => n > 0), paint.series.join(" + ") + " điểm");
  check("biểu đồ thực sự vẽ ra pixel (không chỉ trục)", paint.coloured > 2000,
    `${paint.coloured} pixel màu / ${paint.total}`);
  check("dùng đúng thư viện ECharts", await page.evaluate(() => !!window.echarts));

  // ---- detail pane: clicking a row must show that row's own data ---------
  await page.click('#panelTabs button[data-panel="inbox"]');
  await page.waitForSelector("#panel-inbox:not(.hidden)", { timeout: 5000 });
  const firstId = (await page.locator("#boardRows tr td").first().textContent()).trim();
  await page.locator("#boardRows tr").first().click();
  const detail = await page.locator("#boardDetail").innerText();
  check("bấm một dòng thì hiện chi tiết đúng mục đó", detail.startsWith(firstId), detail.split("\n")[0]);
  check("chi tiết có các trường của quyết định",
    /nguồn/.test(detail) && /trạng thái/.test(detail) && /chi phí/.test(detail));

  // ---- parity with the console: the controls it has, this must have -------
  // These live on a TRIAGED item — the newest row is often still untriaged,
  // and checking it would prove nothing either way.
  await page.locator("#boardRows tr", { hasText: "awaiting_human" }).first().click();
  const triaged = await page.locator("#boardDetail").innerText();
  // Section headings are uppercased by CSS and innerText reports the
  // transformed text — this exact trap already cost two red runs today.
  check("chi tiết có Đề xuất / Bằng chứng như console",
    /đề xuất/i.test(triaged) || /bằng chứng/i.test(triaged), triaged.slice(0, 50).replace(/\n/g, " "));
  check("có dòng policy (vì sao hub dừng ở đây)", /policy:/.test(triaged));
  check("nháp trả lời SỬA ĐƯỢC (textarea, không phải chữ chết)",
    await page.locator("#boardDetail textarea#draft").count() > 0);
  check("header có nút Poll kênh + Chạy 1 vòng như console",
    (await page.locator("#btnIngest").isVisible()) && (await page.locator("#btnCycle").isVisible()));
  check('có ô "Hỏi hub" ngay trên bảng', await page.locator("#askText").isVisible());

  // Status filter, client-side over the snapshot.
  const allRows = await page.locator("#boardRows tr").count();
  await page.selectOption("#fStatus", "awaiting_human");
  await page.waitForTimeout(400);
  const filtered = await page.locator("#boardRows tr").count();
  check("bộ lọc trạng thái hoạt động", filtered > 0 && filtered < allRows, `${allRows} → ${filtered}`);
  // The panel heading is `text-transform: uppercase`, and innerText returns
  // the transformed text — match case-insensitively or this can never pass.
  check("nói rõ đang lọc bao nhiêu", /\d+\/\d+ mục/i.test(await page.locator("#filterNote").innerText()));
  await page.selectOption("#fStatus", "");
  await page.waitForTimeout(400);

  // ---- health tab --------------------------------------------------------
  await page.click('#panelTabs button[data-panel="health"]');
  await page.waitForSelector("#panel-health:not(.hidden)", { timeout: 5000 });
  check('tab Sức khoẻ có nút "Kiểm tra" như console', await page.locator("#btnDoctor").isVisible());
  // Hai chip: `claude` và kênh `tfl5`. Trước 2026-08-08 có sáu vì còn github /
  // devlog / email / telegram — chúng đi cùng nhánh hộp thư, nên assert cũ
  // (>= 3) giờ là assert của một sản phẩm không còn tồn tại.
  const chipText = await page.locator("#healthProbe").innerText();
  const probeChips = await page.locator("#healthProbe .chip").count();
  check("tab Sức khoẻ có kết quả đo kênh", probeChips >= 2, `${probeChips} chip`);
  check("có đo kênh tfl5 — kênh duy nhất còn lại", /tfl5/.test(chipText));
  check("không còn kênh đã gỡ", !/github|telegram|devlog/i.test(chipText), chipText.slice(0, 60));
  check("nói rõ đo lúc nào (số liệu có thể cũ vài phút)",
    /Đo lúc/.test(await page.locator("#healthProbe .boardnote").innerText()));
  const runRows = await page.locator("#runsRows tr").count();
  check("có lịch sử lượt chạy của adapter", runRows > 0, `${runRows} dòng`);

  // ---- cost tab ----------------------------------------------------------
  await page.click('#panelTabs button[data-panel="cost"]');
  await page.waitForSelector("#panel-cost:not(.hidden)", { timeout: 5000 });
  // Two charts, exactly like the console: spend-per-day and the status donut.
  await page.waitForFunction(
    () => !!document.querySelector("#boardStatus canvas"),
    { timeout: 20000 }
  );
  const donut = await page.evaluate(() => {
    const inst = echarts.getInstanceByDom(document.getElementById("boardStatus"));
    const s = inst && inst.getOption().series[0];
    return s ? { type: s.type, n: (s.data || []).length, colors: (s.data || []).map((d) => d.itemStyle.color) } : null;
  });
  check("tab Chi phí có biểu đồ tròn trạng thái như console", donut && donut.type === "pie" && donut.n > 0,
    donut ? `${donut.n} phần` : "không có");
  check("dùng đúng bảng màu của console", (donut?.colors || []).includes("#2f6f4e"),
    (donut?.colors || []).join(" "));

  // ---- config tab --------------------------------------------------------
  await page.click('#panelTabs button[data-panel="config"]');
  await page.waitForSelector("#panel-config:not(.hidden)", { timeout: 5000 });
  const cfgText = await page.locator("#configBody").innerText();
  check("tab Cấu hình hiện config thật", cfgText.length > 200 && cfgText.trim().startsWith("{"),
    `${cfgText.length} ký tự`);
  check("cấu hình chỉ chứa TÊN biến môi trường, không có giá trị bí mật",
    !/"[A-Z_]*(TOKEN|PASSWORD|KEY)"\s*:\s*"[^"]{16,}"/.test(cfgText) && !/sk-[A-Za-z0-9]/.test(cfgText));
  // The config tab is now a real form, so this check moved from "it tells you
  // where to edit" to "it lets you edit, and says how the write travels".
  check("cấu hình sửa được ngay tại đây", (await page.locator("#cfgSave").isVisible()) &&
    (await page.locator("[data-cfg-key]").count()) > 10,
    `${await page.locator("[data-cfg-key]").count()} trường`);
  check("nói rõ thay đổi đi qua lệnh /set trong phòng chat",
    /\/set/.test(await page.locator("#panel-config").innerText()));

  await page.click('#panelTabs button[data-panel="inbox"]');
  await page.waitForSelector("#panel-inbox:not(.hidden)", { timeout: 5000 });

  // ---- the buttons: they must emit the SAME slash command a person types --
  //
  // Spy on the socket instead of letting the click through: this half checks
  // the wire format (verb + which id) without touching real data. The real
  // end-to-end run is the block after it.
  await page.evaluate(() => {
    window.__sent = [];
    const orig = WebSocket.prototype.send;
    WebSocket.prototype.send = function (payload) {
      try {
        const f = JSON.parse(payload);
        if (f && f.type === "msg") {
          window.__sent.push(f.text);
          return; // swallow: this is a dry run
        }
      } catch (_) { /* not ours — pass through */ }
      return orig.call(this, payload);
    };
  });

  const pendingRow = page.locator("#boardRows tr", { hasText: "awaiting_human" }).first();
  // textContent, not innerText: the row lives inside a scrolling panel and we
  // only need the string, not a visibility guarantee.
  const msgId = (await pendingRow.locator("td").first().textContent()).trim().replace("#", "");
  await pendingRow.click();
  await page.locator("#boardDetail .reason").fill("lý do thử");
  const labels = await page.evaluate(() =>
    [...document.querySelectorAll("#boardDetail .actions button")].map((b) => b.textContent.trim())
  );
  check("mục đang chờ có đủ nút Duyệt / Bỏ / Đóng / Trả lời",
    ["Duyệt", "Bỏ", "Đóng", "Trả lời"].every((w) => labels.some((l) => l.startsWith(w))),
    labels.join(" · "));

  for (const [label, re] of [["Duyệt", /^\/approve \d+$/], ["Bỏ", /^\/reject \d+ lý do thử$/],
                             ["Đóng", /^\/close \d+ lý do thử$/], ["Trả lời", /^\/reply \d+ lý do thử$/]]) {
    await page.locator("#boardDetail .actions button", { hasText: label }).first().click();
    const last = await page.evaluate(() => window.__sent.at(-1));
    check(`nút "${label}" gửi đúng lệnh`, re.test(last || ""), last);
  }
  // /close and /reply take the MESSAGE id (the # column); /approve and
  // /reject take the decision id. Getting these two crossed would close the
  // wrong item, so pin it.
  const sentIds = (await page.evaluate(() => window.__sent)).map((t) => t.split(" ")[1]);
  check("/close dùng đúng id tin nhắn của dòng đang chọn", sentIds[2] === msgId,
    `gửi ${sentIds[2]}, dòng là #${msgId}`);
  check("/reply cũng dùng id tin nhắn", sentIds[3] === msgId, sentIds.join(" · "));

  await page.evaluate(() => { window.__sent = []; });
  check("không gửi gì khi ô nội dung trống mà bấm Trả lời", await (async () => {
    await page.locator("#boardDetail .reason").fill("");
    await page.locator("#boardDetail .actions button", { hasText: "Trả lời" }).first().click();
    return (await page.evaluate(() => window.__sent.length)) === 0;
  })());
  check("và nói rõ vì sao không gửi",
    /Cần nội dung/.test(await page.locator("#cmdStatus").innerText()));

  check("ghi rõ nút bấm đi qua phòng chat",
    /phòng chat/.test(await page.locator("#board > .boardnote").innerText()));

  await page.screenshot({ path: `${SHOTS}board-01.png`, fullPage: true });

  // Back to chat: the socket must have survived the detour.
  check("socket vẫn mở sau khi thao tác trên bảng",
    (await page.locator("#status").getAttribute("data-state")) === "on");
  // The composer is on the Trao đổi tab now — check it is reachable, not that
  // it is on screen while the inbox is.
  await page.click('#panelTabs button[data-panel="chat"]');
  check("ô nhập tin ở tab Trao đổi", await page.locator("#foot").isVisible());
  await page.click('#panelTabs button[data-panel="inbox"]');

  check("trang không gọi ra ngoài origin của app", offOrigin.length === 0, offOrigin.slice(0, 3).join(" "));
  check("0 lỗi console", errors.length === 0, errors.join(" | "));
} catch (e) {
  // Keep the call log: "Timeout exceeded" on its own never says WHICH
  // locator, and that is the only part worth knowing.
  check("kịch bản chạy trọn vẹn", false, e.message.split("\n").slice(0, 8).join(" | "));
} finally {
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra qua`);
process.exit(passed === checks.length ? 0 : 1);
