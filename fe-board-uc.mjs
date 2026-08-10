// Acceptance for the board: the four tabs the phone actually has, and the two
// things that must NOT be there.
//
// The journey is a person's: log in on the deployed bundle, walk the tabs, read
// what is on them. Nothing is faked — the snapshot must already have been
// pushed by `hubd`, which is the point: if the push path is broken, this fails
// instead of quietly rendering an empty screen.
//
// It also pins the shape after 2026-08-08, when the inbox product was deleted:
// no "Hộp việc" tab, no "Chi phí" tab, and no number with a `$` in front of it
// anywhere on the surface. Those are ABSENCE assertions on purpose — each one
// grew back once already (a ceiling returned as a price tag), and only a test
// that demands absence can stop it happening a third time.
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
// Phone first: this page has one user and he reads it on a phone. A layout that
// only works at 1280px is a layout nobody here ever sees.
const page = await browser.newPage({
  viewport: { width: 390, height: 844 },
  deviceScaleFactor: 3,
  isMobile: true,
  hasTouch: true,
});
const errors = [];
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text().slice(0, 200)); });
page.on("pageerror", (e) => errors.push(`uncaught: ${e.message.slice(0, 200)}`));

// Anything aimed off this app's own origin would be a design break, so watch
// the wire rather than trusting the code to stay honest.
const offOrigin = [];
page.on("request", (r) => {
  const u = new URL(r.url());
  if (u.host !== `${APP_TID}.test.localhost:8090`) offOrigin.push(r.url());
});

try {
  await page.goto(`http://${APP_TID}.test.localhost:8090`, { waitUntil: "domcontentloaded" });
  // The owner account: the room only takes orders from a trusted tid, and
  // logging in as hub's own bot account would test a permission nobody uses.
  await page.fill("#u", "alice_local");
  await page.fill("#p", env.HUB_TFL5_ALICE_PASSWORD);
  await page.click("#loginBtn");
  await page.waitForSelector("#panelTabs", { timeout: 15000 });

  await page.waitForFunction(
    () => !/đang tải/.test(document.getElementById("boardStamp").textContent),
    null,
    { timeout: 20000 }
  );
  const stamp = (await page.locator("#boardStamp").innerText()).trim();
  check("có ảnh chụp, không phải thông báo lỗi", /Ảnh chụp lúc/.test(stamp), stamp.slice(0, 60));

  // ---- the tabs that exist, and the two that must not --------------------
  const tabs = await page.locator("#panelTabs button[data-panel]").evaluateAll(
    (els) => els.map((e) => e.dataset.panel)
  );
  check("đúng bốn tab: Phiên · Trao đổi · Sức khoẻ · Cấu hình",
    JSON.stringify(tabs) === JSON.stringify(["sessions", "chat", "health", "config"]),
    tabs.join(" · "));
  check("KHÔNG còn tab Hộp việc", !tabs.includes("inbox"));
  check("KHÔNG còn tab Chi phí", !tabs.includes("cost"));

  // ---- health ------------------------------------------------------------
  await page.click('#panelTabs button[data-panel="health"]');
  await page.waitForSelector("#panel-health:not(.hidden)", { timeout: 5000 });
  const probe = await page.locator("#healthProbe").innerText();
  check("tab Sức khoẻ có kết quả đo thật", /claude/.test(probe), probe.split("\n")[0].slice(0, 70));
  check("có đo kênh tfl5 — kênh duy nhất còn lại", /tfl5/.test(probe));
  check("không còn kênh đã gỡ",
    !/github|telegram|mailler|devlog/i.test(probe), probe.replace(/\s+/g, " ").slice(0, 90));
  check("nói rõ đo lúc nào (số liệu có thể cũ vài phút)", /Đo lúc/.test(probe));

  // Hai cách hub chết lặng mà mọi màn khác vẫn xanh (2026-08-10):
  //  · bản cài rơi về chữ ký ad-hoc ⇒ bật máy lại là macOS không nhận ra
  //    chương trình, hub không dậy — và không có lỗi nào để đọc;
  //  · bản cài cũ hơn mã đã build ⇒ daemon chạy mã hôm qua trong khi test xanh
  //    và trang đã deploy.
  // Cả hai chỉ tồn tại dưới dạng MỘT DÒNG CHỮ trên màn này, nên phải kiểm bằng
  // chữ trên màn chứ không bằng trường JSON — chính cái bẫy "hàng có class sạch
  // mà display:none" đã bắt được ở nút ← Danh sách.
  const runtimeText = (await page.locator("#runtimeBox").innerText()).trim();
  check(
    "Tình trạng nói bản cài mang chữ ký gì",
    /chữ ký bản cài/i.test(runtimeText),
    runtimeText.split("\n").find((l) => /chữ ký/i.test(l)) || "(không có hàng nào)"
  );
  check(
    "chữ ký là loại cố định, không phải ad-hoc",
    /chữ ký bản cài[\s\S]{0,40}cố định/i.test(runtimeText),
    runtimeText.split("\n").find((l) => /chữ ký/i.test(l)) || ""
  );
  // Hàng "bản đang chạy CŨ hơn" chỉ hiện khi có chuyện — vắng mặt là tin tốt.
  // Kiểm đúng chiều đó: nếu nó hiện thì phải kèm cách sửa, đừng chỉ báo động.
  const staleRow = runtimeText.split("\n").find((l) => /bản đang chạy/i.test(l));
  check(
    "không có cảnh báo daemon chạy mã cũ (hoặc nếu có thì kèm cách sửa)",
    !staleRow || /install\.sh/.test(runtimeText),
    staleRow || "(vắng mặt — daemon đang chạy đúng mã)"
  );
  // Bảng lượt chạy nay CHỈ kê lượt HỎNG (rà ba tab, 2026-08-09): 12 dòng
  // `ok · 0` chiếm hai phần ba màn để nói một điều duy nhất. Cái phải kiểm bây
  // giờ là dòng tóm tắt — nó luôn có, và nó là chỗ nói "tất cả ok" hay "N hỏng".
  const runsSummary = (await page.locator("#runsSummary").innerText()).trim();
  const runRows = await page.locator("#runsRows tr").count();
  check(
    "có lịch sử lượt chạy",
    // `innerText` trả về chữ đã HOA HOÁ vì `<h2>` có `text-transform:
    // uppercase` — đúng bẫy mà `fe-denied-uc` đã ghi chú, và tôi vừa đạp lại.
    /lượt gần nhất|chưa có lượt nào/i.test(runsSummary),
    `${runsSummary} · ${runRows} dòng hỏng được kê`
  );

  // ---- config ------------------------------------------------------------
  await page.click('#panelTabs button[data-panel="config"]');
  await page.waitForSelector("#panel-config:not(.hidden)", { timeout: 5000 });
  // JSON thô nay GẤP LẠI trong `<details>` — mở ra là hàng trăm dòng nuốt cả
  // màn. Phép đo phải MỞ nó ra như người dùng làm, chứ đọc `innerText` của một
  // khối đang đóng thì được chuỗi rỗng và báo "config trống".
  await page.locator("#configRaw summary").click();
  await page.waitForTimeout(300);
  const cfgText = await page.locator("#configBody").innerText();
  check("tab Cấu hình hiện config thật", cfgText.length > 200, `${cfgText.length} ký tự`);
  check("cấu hình chỉ chứa TÊN biến môi trường, không có giá trị bí mật",
    /_ENV|_env/.test(cfgText) && !/"password"\s*:\s*"[^"]+"/.test(cfgText));
  const fields = await page.locator("#panel-config [data-cfg-key]").count();
  check("cấu hình sửa được ngay tại đây", fields >= 3, `${fields} trường`);
  const cfgKeys = await page.locator("#panel-config [data-cfg-key]").evaluateAll(
    (els) => els.map((e) => e.dataset.cfgKey)
  );
  check("KHÔNG còn núm của hộp thư (triage/act/autonomy/trần ngày)",
    !cfgKeys.some((k) => /^(triage|act|autonomy|routing|daily_budget|max_triage|coalesce)/.test(k)),
    cfgKeys.join(" · "));

  // ---- no money anywhere on the surface ----------------------------------
  const moneyPerTab = {};
  for (const t of tabs) {
    await page.click(`#panelTabs button[data-panel="${t}"]`);
    await page.waitForSelector(`#panel-${t}:not(.hidden)`, { timeout: 5000 });
    // What the PRODUCT draws, not text it merely displays.
    //
    // Bỏ ra hai loại nội dung KHÔNG do hub viết: (1) lịch sử phòng chat — vẫn
    // còn nguyên các câu "$0.8735" hub trả lời hồi chưa gỡ giá, và sửa lịch sử
    // cho phép đo xanh mới là gian; (2) chữ lấy từ chính phiên Claude (xem
    // trước, luồng sự kiện, câu trả lời fork) — một phiên đang bàn "Chi phí
    // engine" thì màn phải hiện đúng như thế. Bắt được thật ngày 2026-08-09:
    // phép đo báo đỏ vì một dòng transcript, không phải vì hub hiện tiền.
    // Phần còn lại là chrome do hub tự vẽ — chỗ phải sạch bóng tiền.
    const txt = await page.evaluate(() => {
      const el = document.getElementById('board').cloneNode(true);
      el.querySelectorAll(
        '#thread, .msg, .sess-body, .sess-note, #sessStream, #sessAskBox, #sessHandoverBox'
      ).forEach((n) => n.remove());
      return el.innerText;
    });
    moneyPerTab[t] = /\$\s?\d|chi phí|trần tiền|đã tiêu/i.test(txt);
  }
  check("KHÔNG tab nào hiện con số tiền / chữ chi phí",
    Object.values(moneyPerTab).every((v) => !v),
    Object.entries(moneyPerTab).map(([k, v]) => `${k}:${v ? "CÓ" : "sạch"}`).join(" "));

  // ---- bốn tab dùng chung một khung cuộn ----------------------------------
  // Hà 2026-08-10: *"các ui theo menu đang bị ảnh hưởng nhau … luôn bị nhảy
  // cuộn xuống cuối trang rất khó chịu"*. Cơ chế: `main` là khung cuộn DUY NHẤT
  // cho cả bốn tab, tab Trao đổi dán đáy (đúng), nên mọi tab mở sau đó kế thừa
  // `scrollTop` ấy và mở ra đã ở đáy. Đo được: Sức khoẻ `68/68`, Cấu hình
  // `68/589`, Phiên `68/148`.
  //
  // Đo trên `main` chứ không phải `window`: `window.scrollY` đứng im ở 0 trên
  // mọi màn, nên phép đo cũ nhìn vào đó không thể đỏ được.
  const mainScroll = () => page.evaluate(() => {
    const m = document.querySelector("main");
    return { top: Math.round(m.scrollTop), max: Math.round(m.scrollHeight - m.clientHeight) };
  });
  const goTab = async (t) => {
    await page.click(`#panelTabs button[data-panel="${t}"]`);
    await page.waitForSelector(`#panel-${t}:not(.hidden)`, { timeout: 5000 });
    await page.waitForTimeout(700);
  };
  await goTab("chat");
  await page.waitForTimeout(400);
  const chatScroll = await mainScroll();
  check("tab Trao đổi vẫn dán đáy (tin mới nằm dưới cùng)",
    chatScroll.max === 0 || chatScroll.max - chatScroll.top < 8,
    `${chatScroll.top}/${chatScroll.max}`);

  // Điều PHẢI chứng minh là "tab này không mang vị trí của tab kia sang", KHÔNG
  // phải "mọi tab luôn mở ở đầu" — thiết kế là mỗi tab nhớ chỗ của chính nó, và
  // quay lại đúng chỗ mình rời đi là ĐÚNG. Bản đo đầu của tôi đòi `top === 0`
  // nên báo đỏ cho `config` khi nó trả người đọc về đúng 589 — một phép đo đòi
  // sản phẩm sai thiết kế thì không bao giờ xanh được.
  for (const t of ["health", "config", "sessions"]) {
    await goTab(t);
    await page.evaluate(() => { document.querySelector("main").scrollTop = 0; });
    await page.waitForTimeout(200);
    await goTab("chat");                       // dán đáy: 3000+ px
    await goTab(t);                            // quay lại
    const s = await mainScroll();
    check(`tab ${t} về đúng chỗ mình rời đi, không dính đáy của Trao đổi`,
      s.top === 0, `scrollTop ${s.top}/${s.max}`);
  }

  // ---- nút trong header chung phải TRẢ LỜI -------------------------------
  // Cùng lời báo trên: *"header chung dẫn đến hiện mà bấm không có tác dụng"*.
  // Gốc là `#cmdStatus` bị xoá cùng đợt gỡ nhánh hộp thư (393db8f) trong khi
  // `cmdNote()` vẫn gọi tới rồi `return` im lặng — lệnh vẫn đi, màn không nói gì
  // suốt hai ngày. Kiểm bằng CHỮ HIỆN RA, không bằng "hàm có được gọi không".
  await page.click('#panelTabs button[data-panel="health"]');
  await page.waitForSelector("#panel-health:not(.hidden)", { timeout: 5000 });
  await page.click("#btnCycle");
  await page.waitForTimeout(1200);
  const cmdSaid = await page.evaluate(() => {
    const el = document.getElementById("cmdStatus");
    if (!el) return "(không có #cmdStatus trên trang)";
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 ? el.textContent.trim() : "(có phần tử nhưng không hiện)";
  });
  check("bấm nút lệnh ở tab khác vẫn thấy hub trả lời", /Đã gửi|Chưa kết nối/.test(cmdSaid), cmdSaid.slice(0, 70));

  // ---- the room still takes orders ---------------------------------------
  await page.click('#panelTabs button[data-panel="chat"]');
  await page.waitForSelector("#foot:not(.hidden)", { timeout: 10000 });
  check("ô nhập tin ở tab Trao đổi", await page.locator("#text").isVisible());

  await page.click('#panelTabs button[data-panel="sessions"]');
  await page.waitForSelector("#panel-sessions:not(.hidden)", { timeout: 5000 });
  const over = await page.evaluate(() => ({
    w: document.documentElement.scrollWidth,
    inner: window.innerWidth,
  }));
  check("trang không tràn ngang trên điện thoại", over.w <= over.inner + 1, `${over.w}/${over.inner}`);

  // Trang không tràn KHÔNG có nghĩa là không có gì tràn: một khối con vẫn có
  // thể rộng hơn khung của nó và cuộn ngang bên trong, và người đọc chỉ thấy
  // phần đầu — tưởng đó là tất cả. Bắt thật ngày 2026-08-09: bảng "lần poll"
  // rộng 520px trong khung 300px vì còn `min-width` của bảng NĂM cột đã bỏ, nên
  // cột "mới" nằm ngoài màn suốt hai bản deploy mà mọi kiểm tra vẫn xanh.
  //
  // Bỏ qua input/textarea/select: `scrollWidth` của chúng phản ánh độ dài giá
  // trị đang gõ, không phải lỗi bố cục.
  const sideways = {};
  for (const t of tabs) {
    await page.click(`#panelTabs button[data-panel="${t}"]`);
    await page.waitForSelector(`#panel-${t}:not(.hidden)`, { timeout: 5000 });
    sideways[t] = await page.evaluate(() =>
      [...document.querySelectorAll("#board *")]
        .filter((e) => !/^(INPUT|TEXTAREA|SELECT)$/.test(e.tagName))
        .filter((e) => e.clientWidth > 0 && e.scrollWidth > e.clientWidth + 2)
        // Chữ CẮT ĐUÔI có chủ ý (`text-overflow: ellipsis` + `overflow:
        // hidden`) luôn có `scrollWidth` lớn hơn khung — đó là thiết kế, không
        // phải tràn: tên phiên dài và dòng trạng thái đều cố tình cắt. Đòi
        // chúng vừa khít là đòi bỏ dấu "…".
        .filter((e) => {
          const cs = getComputedStyle(e);
          return !(cs.textOverflow === "ellipsis" && cs.overflowX !== "visible");
        })
        .map((e) => `${e.tagName.toLowerCase()}${e.id ? "#" + e.id : ""} ${e.scrollWidth}/${e.clientWidth}`)
    );
  }
  // Màn CHI TIẾT phiên cũng phải đo — nó là màn dày nhất và có những hàng chỉ
  // xuất hiện khi phiên do hub mở (ô "nói tiếp"). Đúng chỗ ấy có lỗi tràn thật
  // 2026-08-09 (`div.row-flex 300 → 368px`) mà vòng quét 4 tab không thấy, vì
  // nó chỉ đứng ở danh sách.
  await page.click('#panelTabs button[data-panel="sessions"]');
  await page.waitForSelector("#panel-sessions:not(.hidden)", { timeout: 5000 });
  const liveId = await page.evaluate(
    () => document.querySelector('#sessList .sess:not([data-host="dead"])')?.dataset.session
  );
  if (!liveId) {
    console.log("  · không có phiên sống nào — bỏ qua đo màn chi tiết");
  } else {
    await page.locator(`.sess[data-session="${liveId}"]`).click();
    await page.waitForSelector("#sessDetail:not(.hidden)", { timeout: 10000 });
    await page.waitForTimeout(1500);
    sideways["chi-tiết-phiên"] = await page.evaluate(() =>
      [...document.querySelectorAll("#board *")]
        .filter((e) => !/^(INPUT|TEXTAREA|SELECT)$/.test(e.tagName))
        .filter((e) => e.clientWidth > 0 && e.scrollWidth > e.clientWidth + 2)
        // Cùng lý do với vòng quét bốn tab: chữ cắt đuôi có chủ ý không phải
        // tràn. Bản trước chỉ lọc ở MỘT trong hai chỗ quét, nên `#sessStatus`
        // (cố tình cắt) vẫn báo đỏ.
        .filter((e) => {
          const cs = getComputedStyle(e);
          return !(cs.textOverflow === "ellipsis" && cs.overflowX !== "visible");
        })
        .map((e) => `${e.tagName.toLowerCase()}${e.id ? "#" + e.id : ""} ${e.scrollWidth}/${e.clientWidth}`)
    );
    await page.click("#sessBack");
    await page.waitForSelector("#sessListPanel:not(.hidden)", { timeout: 10000 });
  }

  const overflowing = Object.entries(sideways).filter(([, v]) => v.length);
  check("không khối nào rộng hơn khung của nó (nội dung không trốn sang bên phải)",
    overflowing.length === 0,
    overflowing.map(([k, v]) => `${k}: ${v.join(", ")}`).join(" | "));
  await page.screenshot({ path: `${SHOTS}board-01-phone.png`, fullPage: true });

  check("trang không gọi ra ngoài origin của app", offOrigin.length === 0, offOrigin.slice(0, 2).join(" "));
  check("0 lỗi console", errors.length === 0, errors.join(" | "));
} catch (e) {
  check("kịch bản chạy trọn vẹn", false, e.message.split("\n").slice(0, 3).join(" | "));
} finally {
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra qua`);
process.exit(passed === checks.length ? 0 : 1);
