// UC-S01 — "xem mọi việc đang chạy", nghiệm thu trên UI THẬT ở cỡ điện thoại.
//
// Mọi thứ đi qua giao diện như người dùng: gõ vào form đăng nhập, chạm tab.
// Không goto thẳng, không gọi API để dựng trạng thái.
//
// Phép đo không tự bịa chuẩn: số liệu đối chiếu lấy từ `hub sessions --json`
// chạy độc lập, nên màn hình phải khớp với sự thật của máy chứ không khớp với
// chính nó (bẫy đã đạp 08-07: script đọc lại đúng ô nó vừa gõ rồi báo ĐẠT).
//
// Usage: node fe-sessions-uc.mjs <app_tid> <username> <password>

import { chromium } from "/Users/hanguyen/Documents/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { execFileSync } from "node:child_process";
import { mkdirSync } from "node:fs";

const [appTid, username, password] = process.argv.slice(2);
if (!appTid || !username || !password) {
  console.error("usage: node fe-sessions-uc.mjs <app_tid> <username> <password>");
  process.exit(2);
}
const BASE = `http://${appTid}.test.localhost:8090`;
const HERE = new URL("./", import.meta.url).pathname;
const SHOTS = HERE + "ui-shots/";
mkdirSync(SHOTS, { recursive: true });
const PHONE = { width: 390, height: 844 };

const problems = [];
const checks = [];
const check = (name, ok, detail = "") => {
  checks.push({ name, ok });
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) problems.push(`${name}${detail ? `: ${detail}` : ""}`);
};

// Ground truth, read straight from the machine, not from the page.
const readTruth = () =>
  JSON.parse(
    execFileSync(HERE + "rust/target/release/hub", ["sessions", "--json"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    })
  );
/// Dấu vân tay của tập phiên — đổi khi có phiên mở/dừng/đổi nơi chạy.
const fingerprint = (t) =>
  t.sessions
    .map((s) => `${s.session_id}:${s.host}:${s.working}`)
    .sort()
    .join("|");

let truth = readTruth();
let liveCount = truth.sessions.length;
let liveAccounts = [...new Set(truth.sessions.map((s) => s.account))].sort();
console.log(`máy đang có ${liveCount} phiên · tài khoản: ${liveAccounts.join(", ")}\n`);

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({
  viewport: PHONE,
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

  // The screen the owner opens the phone for must be the one he lands on.
  check(
    "tab Phiên là màn mặc định sau khi đăng nhập",
    await page.locator('#panelTabs button[data-panel="sessions"].on').count() === 1
  );

  await page.waitForFunction(() => document.querySelectorAll("#sessList .sess").length > 0, {
    timeout: 25000,
  });

  // ——— KẸP phép đo trước khi so ———
  //
  // Ảnh chụp trên trang trễ tới ~25 giây (một nhịp đẩy của daemon + một nhịp đọc
  // của trang). Trong khoảng ấy một phiên có thể mở ra hoặc dừng hẳn, và lúc đó
  // màn KHÁC máy mà **không bên nào sai**. Đo 2026-08-10: chạy cả bộ liên tiếp
  // thì kịch bản này đỏ 3 dòng (`màn 6 / máy 5`), chờ 45 giây chạy lại thì xanh
  // — tức nó đang tố cáo sản phẩm cho một chuyện của phép đo, và đỏ giả thì
  // người ta sẽ bỏ qua cả những lần đỏ thật.
  //
  // Khuôn lấy từ `fe-subagent-uc.mjs`: đọc sự thật — chờ màn bắt kịp — đọc lại
  // sự thật; chỉ so khi hai đầu kẹp bằng nhau. Sự thật đổi giữa chừng thì lấy
  // bản mới và chờ tiếp. Hết trần vẫn lệch thì ĐỎ THẬT, không nới nữa.
  // Đọc CẢ số thẻ lẫn chấm màu trong cùng một nhịp: chỉ chờ số thẻ khớp là
  // chưa đủ. Đo 2026-08-10: một phiên vừa bắt đầu chạy thì SỐ thẻ không đổi
  // (nó đã có trong danh sách từ trước) nhưng ảnh chụp trên trang vẫn mang
  // `working` của nhịp cũ ⟹ máy nói "đang chạy", thẻ nói "đang rảnh", và phép
  // đo đỏ cho một độ trễ chứ không cho một lỗi.
  const readScreen = () =>
    page.evaluate(() => ({
      cards: document.querySelectorAll("#sessList .sess").length,
      dots: Object.fromEntries(
        [...document.querySelectorAll("#sessList .sess")].map((c) => [
          c.dataset.session,
          c.dataset.work || "",
        ])
      ),
    }));
  const dotsAgree = (screen, t) =>
    t.sessions.every((x) => {
      const d = screen.dots[x.session_id];
      if (d === undefined) return false;
      return x.working ? d === "đang chạy" : d !== "đang chạy";
    });

  let cards = 0;
  let screen = { cards: 0, dots: {} };
  let settled = false;
  for (let round = 0; round < 12 && !settled; round++) {
    const before = fingerprint(truth);
    screen = await readScreen();
    cards = screen.cards;
    const after = readTruth();
    if (fingerprint(after) !== before) {
      truth = after;
      liveCount = truth.sessions.length;
      liveAccounts = [...new Set(truth.sessions.map((s) => s.account))].sort();
      console.log(`  … tập phiên vừa đổi (${liveCount} phiên) — đọc lại rồi chờ màn bắt kịp`);
    } else if (cards === liveCount && dotsAgree(screen, truth)) {
      settled = true;
    }
    if (!settled) await page.waitForTimeout(6000);
  }
  check("số phiên trên màn khớp với máy", cards === liveCount, `màn ${cards} / máy ${liveCount}`);

  // ——— CHẤM MÀU phải nói đúng phiên nào đang chạy ———
  //
  // Trước v134 chấm màu đọc `status`/`state`, mà hai trường ấy chỉ có ở phiên
  // nền — nên mọi thẻ phiên terminal mang chấm xám VĨNH VIỄN và không phép đo
  // nào bắt được, vì không ai đối chiếu nó với máy. Nay `working` trả lời cho
  // mọi phiên (mtime nhật ký + số subagent + `status`), và phép đo đi HAI CHIỀU
  // trên dữ liệu thật: đang chạy phải xanh, không chạy thì không được xanh.
  const dots = screen.dots;
  const shouldRun = truth.sessions.filter((s) => s.working);
  const shouldNot = truth.sessions.filter((s) => !s.working && s.host !== "dead");
  const missRun = shouldRun.filter((s) => dots[s.session_id] !== "đang chạy");
  const falseRun = shouldNot.filter((s) => dots[s.session_id] === "đang chạy");
  check(
    `phiên ĐANG CHẠY thì thẻ nói vậy (${shouldRun.length} phiên)`,
    shouldRun.length === 0 || missRun.length === 0,
    missRun.map((s) => `${s.name}=${dots[s.session_id] || "(trống)"}`).join(", ")
  );
  check(
    `phiên KHÔNG chạy thì thẻ không nói đang chạy (${shouldNot.length} phiên)`,
    falseRun.length === 0,
    falseRun.map((s) => s.name).join(", ")
  );

  // ——— NƠI phiên sống, không chỉ SỐ phiên ———
  // Hỏi thật 2026-08-09: "máy chỉ mở 3 terminal sao màn hiện 13 phiên?" Cả hai
  // đều đúng — 3 terminal, 8 phiên trong VS Code/Cursor, 2 dòng đã dừng mà
  // `claude agents` vẫn liệt kê. Con số tổng không sai, nó chỉ không nói ra.
  // Đối chiếu với SỰ THẬT trên máy (hub sessions --json), không tự suy từ màn.
  const byHost = {};
  truth.sessions.forEach((x) => { byHost[x.host] = (byHost[x.host] || 0) + 1; });
  const summary = (await page.locator("#sessSummary").innerText()).trim();
  console.log(`nơi phiên sống: ${JSON.stringify(byHost)}`);

  // MÀN THEO DÕI PHẢI TỰ LÀM MỚI — và phải ĐO thấy nó chạy.
  //
  // v70 thêm vòng 15 giây nhưng móc vào `showTab()`, một hàm KHÔNG AI GỌI; nó
  // chưa từng chạy suốt 14 bản bundle trong khi tôi đã báo là "danh sách tự làm
  // mới". Không phép đo nào bắt được, vì mọi kịch bản đều tự bấm "Tải lại" hoặc
  // đổi tab. Nên đếm thẳng số lần trang đi hỏi ảnh chụp.
  const hits = [];
  const onReq = (r) => { if (/\/app\/doc\/list/.test(r.url())) hits.push(Date.now()); };
  page.on("request", onReq);
  await page.waitForTimeout(34000);            // đủ cho 2 nhịp 15s
  page.off("request", onReq);
  check(
    "danh sách tự làm mới, không cần bấm gì",
    hits.length >= 2,
    `${hits.length} lần đọc ảnh chụp trong 34s`
  );

  // ...và tự làm mới KHÔNG được đẩy chỗ đang đọc đi. `renderSessions` dựng lại
  // toàn bộ thẻ, nên phải neo theo THẺ đang nhìn (nhận diện bằng session_id,
  // không bằng vị trí — thứ tự đổi khi một phiên vừa động).
  // Đứng GIỮA danh sách, không dính đáy. `scrollHeight * 0.6` bị trình duyệt
  // kẹp về đúng trần cuộn khi danh sách ngắn, và đứng ở trần thì mọi lệch đều
  // "giải thích được" — phép đo hết đỏ nổi. Nửa trần thì còn chỗ phía dưới để
  // giữ neo, tức phép đo mới đo được thứ nó định đo.
  await page.evaluate(() => {
    const m = document.querySelector("main");
    m.scrollTop = Math.round((m.scrollHeight - m.clientHeight) * 0.5);
  });
  await page.waitForTimeout(400);
  const readCard = () =>
    page.evaluate(() => {
      const el = [...document.querySelectorAll("#sessList .sess")]
        .find((n) => n.getBoundingClientRect().bottom > 0);
      return {
        id: el?.dataset.session || "",
        name: (el?.querySelector("strong")?.textContent || "").trim().slice(0, 24),
        y: el ? Math.round(el.getBoundingClientRect().top) : null,
      };
    });
  const beforeList = await readCard();
  const heightBefore = await page.evaluate(() => document.querySelector("main").scrollHeight);
  await page.waitForTimeout(17000);            // qua ít nhất một nhịp làm mới
  // Đo ĐÚNG cái thẻ đã ghi, không phải "thẻ đầu tiên còn thấy": danh sách sắp
  // theo vừa-động-trước, nên một phiên khác vừa động là thứ tự đổi và thẻ đầu
  // đổi theo — đó là dữ liệu đổi, không phải màn nhảy. Cái phải giữ nguyên là
  // vị trí của thẻ NGƯỜI TA ĐANG NHÌN.
  const after = await page.evaluate((id) => {
    const m = document.querySelector("main");
    const el = document.querySelector(`.sess[data-session="${id}"]`);
    return {
      y: el ? Math.round(el.getBoundingClientRect().top) : null,
      scrollTop: m.scrollTop,
      max: m.scrollHeight - m.clientHeight,
      height: m.scrollHeight,
    };
  }, beforeList.id);
  const afterY = after.y;
  // Danh sách NGẮN LẠI đúng lúc đang ở gần đáy thì không giữ nổi toạ độ cũ, và
  // đó không phải lỗi: `renderSessions` bù bằng `sc.scrollTop += delta`
  // (`fe/index.html`), nhưng trần cuộn `scrollHeight - clientHeight` vừa tụt
  // đúng một chiều cao thẻ, nên trình duyệt kẹp lệnh ghi ngược về trần mới —
  // không có cách nào tạo ra chỗ cuộn không tồn tại. Dựng lại được đúng số đo
  // 2026-08-10: y=-98 → y=53, lệch 151px = một thẻ, và khung cuộn nằm đúng
  // trần mới. Cú xê dịch ấy có trần (≤ một thẻ), tự lành ở nhịp sau, và chỉ xảy
  // ra đúng lúc một phiên biến mất.
  //
  // Nới ĐÚNG một ca ấy, nhận diện bằng bằng chứng chứ không bằng lòng tin:
  // khung cuộn phải đang nằm ngay tại trần mới của chính nó. Mọi lệch khác vẫn
  // đỏ.
  //
  // ⚠ Và phải đòi CẢ HAI vế. Bản nới đầu tiên của tôi chỉ hỏi "có đang ở trần
  // không" — mà test khi ấy cuộn tới `scrollHeight*0.6`, con số bị kẹp về đúng
  // trần, nên vế ấy LUÔN đúng và phép đo hết đỏ nổi (đo 2026-08-10: xanh với
  // `y=-98 → y=-98 (đã ở đáy mới)`, tức nới cho một ca không hề xảy ra). Nay
  // đòi thêm bằng chứng danh sách THẬT SỰ ngắn đi.
  const shrank = after.height < heightBefore - 8;
  const atNewCeiling = shrank && Math.abs(after.scrollTop - after.max) <= 1;
  if (afterY === null) {
    console.log("  · thẻ đang nhìn đã rời danh sách — bỏ qua 1 kiểm tra");
  } else {
    check(
      "làm mới danh sách KHÔNG đẩy chỗ đang đọc đi (hoặc đã hết chỗ để giữ)",
      beforeList.id.length > 0 && (Math.abs(afterY - beforeList.y) <= 4 || atNewCeiling),
      `${beforeList.name} y=${beforeList.y} → y=${afterY}` +
        (atNewCeiling ? " (danh sách ngắn lại, khung cuộn đã ở đáy mới)" : "")
    );
  }
  await page.evaluate(() => { document.querySelector("main").scrollTop = 0; });

  // Từ v78 việc chia-theo-loại nằm ở TIÊU ĐỀ TỪNG NHÓM, không còn ở dòng tóm
  // tắt — nhắc hai lần thì trên 390px nó ngốn 3 dòng và đẩy thẻ đầu tiên xuống
  // 336px. Phép đo phải theo sản phẩm về chỗ mới, chứ đòi dòng tóm tắt kể lại
  // là đòi lại đúng thứ vừa bỏ đi (lần thứ năm dính bẫy "phép đo lạc hậu").
  const headers = await page.evaluate(() =>
    [...document.querySelectorAll("#sessList .sess-group")]
      .map((e) => `${e.dataset.g}=${e.dataset.count}`)
      .join(" ")
  );
  check(
    "màn nói rõ phiên sống Ở ĐÂU, không gộp hết làm một",
    Object.keys(byHost).filter((h) => byHost[h]).length < 2 || headers.length > 0,
    headers
  );
  if (byHost.terminal) {
    // Nhóm 'terminal' chỉ chứa phiên KHÔNG do hub mở; phiên hub mở nằm nhóm
    // riêng dù nó cũng chạy trong terminal.
    const expectTerm = truth.sessions.filter((x) => x.host === "terminal" && !x.started_by_hub).length;
    check(
      "số phiên terminal trên màn khớp máy",
      headers.includes(`terminal=${expectTerm}`),
      `máy có ${expectTerm} terminal · tiêu đề nhóm: ${headers}`
    );
  }
  if (byHost.editor) {
    check(
      "phiên chạy trong editor được gọi đúng tên",
      summary.includes(`${byHost.editor} trong editor`),
      `máy có ${byHost.editor} phiên trong editor`
    );
  }
  // Dòng đã dừng: phải còn trên màn (để dọn) nhưng KHÔNG được tính là đang sống
  // và KHÔNG được trông như việc đang chạy.
  if (byHost.dead) {
    const dead = await page.evaluate(() => {
      const els = [...document.querySelectorAll('#sessList .sess[data-host="dead"]')];
      return {
        n: els.length,
        marked: els.every((e) => e.classList.contains("sess-dead")),
        explained: els.every((e) => /không còn tiến trình/.test(e.innerText)),
      };
    });
    check("phiên đã dừng vẫn hiện để còn dọn", dead.n === byHost.dead, `${dead.n}/${byHost.dead}`);
    check("phiên đã dừng KHÔNG trông như đang chạy", dead.marked && dead.explained);
    check(
      "số 'đang sống' đã trừ phiên đã dừng",
      summary.startsWith(`${truth.sessions.length - byHost.dead} phiên đang sống`),
      summary
    );
  }

  const shown = await page.evaluate(() =>
    [...document.querySelectorAll("#sessList .sess")].map((el) => ({
      name: el.querySelector("strong")?.textContent || "",
      meta: el.querySelector(".sess-meta")?.textContent || "",
      id: el.dataset.session,
      top: Math.round(el.getBoundingClientRect().top),
    }))
  );

  const shownAccounts = [...new Set(shown.map((s) => s.meta.split(" · ")[0]))].sort();
  check(
    "phiên của cả 3 tài khoản đều có mặt",
    JSON.stringify(shownAccounts) === JSON.stringify(liveAccounts),
    `màn ${shownAccounts.join(",")} / máy ${liveAccounts.join(",")}`
  );

  // Order carries meaning: half the sessions have not moved in 40 hours.
  //
  // Kiểm TÍNH CHẤT chứ không so danh tính với một lần đọc sau: hai phiên đang
  // chạy song song thì "cái nào mới nhất" đổi giữa hai lần đọc, nên so
  // shown[0] với truth[0] là phép đo đua thời gian (đã đỏ oan 08-08).
  const order = await page.evaluate(() =>
    [...document.querySelectorAll("#sessList .sess")].map((el) => el.dataset.activity || "")
  );
  // Mảng rỗng làm every() xanh vô nghĩa — bắt buộc phải có đủ dữ liệu để đo.
  // Phiên CHƯA CÓ nhật ký thì không có mốc — đó là đúng, không phải thiếu.
  const expectMarks = truth.sessions.filter((s) => s.last_activity).length;
  check(
    "mọi phiên có nhật ký đều đọc được mốc hoạt động",
    order.length === cards && order.filter(Boolean).length === expectMarks && expectMarks > 1,
    `${order.filter(Boolean).length}/${expectMarks} dòng có mốc, ${cards} dòng`
  );
  // Từ 2026-08-09 danh sách GỘP THEO NHÓM, nên "vừa-động-trước" là tính chất
  // TRONG mỗi nhóm — so toàn cục sẽ đỏ vì lý do sai (nhóm "đã dừng" luôn đứng
  // cuối dù mốc của nó có thể mới hơn).
  const perGroup = await page.evaluate(() => {
    const out = [];
    let cur = null;
    for (const el of document.querySelectorAll("#sessList > *")) {
      if (el.classList.contains("sess-group")) { cur = { g: el.dataset.g, marks: [] }; out.push(cur); }
      else if (cur) cur.marks.push({ mark: el.dataset.activity || "", kid: el.classList.contains("sess-kid") });
    }
    // Bỏ thẻ con khỏi phép so thứ tự.
    out.forEach((g) => { g.marks = g.marks.filter((x) => !x.kid); });
    return out;
  });
  // Từ v97 phiên CON vẽ ngay dưới phiên cha (như một lượt trả lời), nên thứ
  // tự "vừa động trước" chỉ đúng giữa các thẻ CHA — con đi theo chỗ của cha
  // chứ không theo mốc hoạt động của chính nó.
  const unsorted = perGroup.filter((grp) => {
    const m = grp.marks.filter((x) => x && x.mark).map((x) => x.mark);
    return !m.every((v, i) => i === 0 || m[i - 1] >= v);
  });
  check(
    "trong mỗi nhóm, phiên vừa động đứng trước",
    unsorted.length === 0,
    perGroup.map((g) => `${g.g}:${g.marks.length}`).join(" · ")
  );

  // The point of the screen: work visible without scrolling past chrome.
  const fold = await page.evaluate(() => {
    const el = document.querySelector("#sessList .sess");
    return { top: Math.round(el.getBoundingClientRect().top), h: window.innerHeight };
  });
  check(
    "phiên đầu tiên nằm trong nửa trên của màn đầu",
    fold.top < fold.h * 0.5,
    `${fold.top}px / màn ${fold.h}px`
  );

  const over = await page.evaluate(() => ({
    w: document.documentElement.scrollWidth,
    inner: window.innerWidth,
  }));
  check("trang không tràn ngang", over.w <= over.inner + 1, `${over.w}/${over.inner}`);

  // A withheld preview must say so; silence would read as "phiên im lặng".
  // Nhìn qua một cái là biết loại nào, ai mở — Hà 2026-08-09. Bản đầu làm bằng
  // huy hiệu trên từng thẻ; Hà xem xong bảo *"dễ nhìn dễ hiểu hơn được không?"*,
  // nên đổi sang GỘP NHÓM: tiêu đề nói một lần cho cả cụm.
  //
  // Phép đo đối chiếu với MÁY, không hỏi lại trang: số phiên mỗi nhóm phải khớp
  // `hub sessions --json`, và nhãn "hub mở" chỉ được xuất hiện đúng bằng sổ.
  const groups = await page.evaluate(() =>
    [...document.querySelectorAll("#sessList .sess-group")].map((e) => ({
      g: e.dataset.g,
      n: Number(e.dataset.count),
      text: e.textContent.replace(/\s+/g, " ").trim(),
    }))
  );
  check("danh sách được gộp thành nhóm có tiêu đề", groups.length > 0,
    groups.map((g) => `${g.g}:${g.n}`).join(" · "));
  // Phiên CÓ CHA đang hiện trên màn thì được vẽ LỒNG dưới cha, không đếm vào
  // nhóm nào (v97). Máy và màn phải cùng luật ấy, nếu không phép đo sẽ đòi lại
  // đúng cái thiết kế vừa bỏ.
  const onScreenIds = new Set(truth.sessions.map((x) => x.session_id));
  const nested = (x) =>
    !!(x.parent_session_id && onScreenIds.has(x.parent_session_id) && x.parent_session_id !== x.session_id);
  const top = truth.sessions.filter((x) => !nested(x));
  const expect = {
    hub: top.filter((x) => x.started_by_hub && x.host !== "dead").length,
    terminal: top.filter((x) => x.host === "terminal" && !(x.started_by_hub)).length,
    background: top.filter((x) => x.host === "background" && !(x.started_by_hub)).length,
    detached: top.filter((x) => x.host === "detached" && !(x.started_by_hub)).length,
    dead: top.filter((x) => x.host === "dead").length,
  };
  const mismatch = Object.entries(expect)
    .filter(([g, n]) => n > 0 && (groups.find((x) => x.g === g)?.n ?? 0) !== n)
    .map(([g, n]) => `${g}: máy ${n} / màn ${groups.find((x) => x.g === g)?.n ?? 0}`);
  check("số phiên mỗi nhóm khớp với máy", mismatch.length === 0, mismatch.join(" · "));
  const hubGroup = groups.find((g) => g.g === "hub");
  check(
    "nhóm 'hub mở' chỉ xuất hiện khi sổ CÓ phiên hub mở",
    (expect.hub > 0) === !!hubGroup,
    `sổ ${expect.hub} · màn ${hubGroup ? hubGroup.n : "không có nhóm"}`
  );

  // Phiên nền không có cửa sổ nào, nên "ở đâu ra?" phải được trả lời ngay trên
  // thẻ. Đo 2026-08-09: cả hai đều truy được về một phiên trong danh sách.
  const bg = truth.sessions.filter((x) => x.host === "background" && x.parent_name);
  if (bg.length) {
    const fromLines = await page.evaluate((ids) =>
      ids.map((id) => (document.querySelector(`.sess[data-session="${id}"] .sess-from`)?.textContent || "").trim()),
      bg.map((x) => x.session_id)
    );
    const wrong = bg.filter((x, i) => !fromLines[i].includes(x.parent_name));
    check("phiên nền ghi rõ do phiên nào mở", wrong.length === 0,
      fromLines.join(" | ").slice(0, 90));
  } else {
    console.log("  · không phiên nền nào truy được cha — bỏ qua 1 kiểm tra");
  }

  // "terminal" phải THẬT SỰ là terminal — Hà hỏi thẳng 2026-08-09: *"danh sách
  // phiên thực sự đang liệt kê terminal hay chỉ claude?"*. Nhãn ấy từng được
  // suy bằng loại trừ (không phải editor ⇒ terminal), nên một `claude` do
  // script hay cron chạy vẫn đọc là "terminal". Phép đo này hỏi HỆ ĐIỀU HÀNH,
  // không hỏi lại chính hub: mỗi dòng gắn nhãn terminal phải có tty điều khiển.
  const ttyOf = (pid) => {
    try {
      return execFileSync("ps", ["-p", String(pid), "-o", "tty="], { encoding: "utf8" }).trim();
    } catch {
      return "";
    }
  };
  const claimTerminal = truth.sessions.filter((s) => s.host === "terminal");
  const noTty = claimTerminal.filter((s) => {
    const t = ttyOf(s.pid);
    return !t || t === "??" || t === "-";
  });
  check(
    "mọi dòng gắn nhãn 'terminal' đều có tty thật",
    noTty.length === 0,
    `${claimTerminal.length} dòng · ${noTty.length} không tty${noTty.length ? ": " + noTty.map((s) => s.name).join(", ") : ""}`
  );
  const ttys = new Set(claimTerminal.map((s) => ttyOf(s.pid)).filter(Boolean));
  check(
    "mỗi phiên terminal ngồi ở một cửa sổ riêng",
    ttys.size === claimTerminal.length,
    `${claimTerminal.length} phiên / ${ttys.size} tty: ${[...ttys].join(" ")}`
  );

  // Phiên chạy trong editor bị bỏ khỏi danh sách — màn phải NÓI RA, và nói
  // đúng số. Đo 2026-08-09: hub ẩn 8 phiên mỗi vòng suốt 601 lần ghi log, mà
  // màn hình không có một chữ nào về chúng; câu hỏi gốc của Hà ("3 terminal sao
  // hiện 13 phiên?") khi ấy chỉ đổi chiều chứ chưa được trả lời.
  const hiddenEditor = truth.hidden_editor || 0;
  const summaryTxt = (await page.locator("#sessSummary").innerText()).trim();
  const onScreenEditors = await page.evaluate(() =>
    [...document.querySelectorAll("#sessList .sess")].filter((e) => e.dataset.host === "editor").length
  );
  check("không dòng phiên editor nào lọt lên màn", onScreenEditors === 0, `${onScreenEditors} dòng`);
  if (hiddenEditor > 0) {
    check(
      "màn nói đúng số phiên editor bị ẩn — khớp với máy",
      new RegExp(`${hiddenEditor} phiên trong editor không hiện`).test(summaryTxt),
      `máy ẩn ${hiddenEditor} · màn: ${summaryTxt}`
    );
  } else {
    check("máy không có phiên editor thì màn không bịa ra con số",
      !/không hiện ở đây/.test(summaryTxt), summaryTxt);
  }

  const hidden = truth.sessions.filter((s) => (s.note || "").startsWith("ẩn phần xem trước"));
  if (hidden.length) {
    const noteShown = await page.evaluate(
      (id) => {
        const el = document.querySelector(`.sess[data-session="${id}"]`);
        return { note: el?.querySelector(".sess-note")?.textContent || "", body: el?.querySelector(".sess-body")?.textContent || "" };
      },
      hidden[0].session_id
    );
    check("phiên bị ẩn nói rõ lý do", noteShown.note.includes("ẩn phần xem trước"), noteShown.note.slice(0, 60));
    check("phiên bị ẩn KHÔNG hiện nội dung", noteShown.body === "");
  } else {
    console.log("  · lần chạy này không có phiên nào bị ẩn — bỏ qua 2 kiểm tra");
  }

  // ── UC-S10 · làm được việc NGAY TỪ DANH SÁCH ────────────────────────────
  // Hà 2026-08-10: *"thêm uc cho danh sách các phiên, hiện tại bắt buộc mở nó
  // mới có hơi bất tiện"*. Hai việc, đúng hai việc Hà chọn: Dừng và Đóng sổ.
  const liveRow = truth.sessions.find((s) => s.host !== "dead");
  if (!liveRow) {
    console.log("  · không có phiên sống nào — bỏ qua 4 kiểm tra của UC-S10");
  } else {
    const sel = `.sess[data-session="${liveRow.session_id}"]`;
    const acts = await page.evaluate((q) => {
      const el = document.querySelector(`${q} .sess-acts`);
      return el ? [...el.querySelectorAll("button")].map((b) => b.textContent.trim()) : [];
    }, sel);
    check("mỗi phiên sống có hàng thao tác ngay trên danh sách", acts.length >= 1, acts.join(" · "));
    check("phiên nào cũng đóng sổ được (chạy trên bản fork)", /Đóng sổ/.test(acts.join(" ")), acts.join(" · "));

    // ⏹ Dừng chỉ được có mặt ở nơi nó CHẠY ĐƯỢC. `stop_background` từ chối
    // phiên terminal, nên một nút Dừng trên dòng terminal là nút chỉ biết báo
    // lỗi. Kiểm cả hai chiều, vì "không bao giờ hiện" cũng sai như "hiện khắp nơi".
    const stopWhere = await page.evaluate(() =>
      [...document.querySelectorAll("#sessList .sess")]
        .filter((e) => e.dataset.host !== "dead")
        .map((e) => ({
          kind: e.dataset.host,
          hasStop: [...e.querySelectorAll(".sess-acts button")].some((b) => /Dừng/.test(b.textContent)),
        }))
    );
    const wrong = stopWhere.filter((r) => (r.kind === "background") !== r.hasStop);
    check(
      "nút Dừng chỉ nằm trên phiên hub mở được, và nằm trên MỌI phiên như vậy",
      wrong.length === 0,
      stopWhere.map((r) => `${r.kind}:${r.hasStop ? "có" : "không"}`).join(" · ")
    );

    // Bấm tới được THẬT, không phải "có mặt trong DOM": nút nằm bên trong một
    // thẻ vốn là vùng chạm lớn, nên đây đúng là chỗ dễ bị đè nhất.
    let reachable = true;
    try {
      await page.locator(`${sel} .sess-acts button`).first().click({ trial: true, timeout: 3000 });
    } catch (e) {
      reachable = false;
    }
    check("nút trên danh sách bấm tới được (không bị thẻ phiên đè)", reachable);

    // Bấm nút KHÔNG được kéo theo cú chạm "mở phiên" của cả thẻ — quên
    // `stopPropagation` thì mỗi lần dừng là một lần nhảy sang màn chi tiết.
    //
    // ⚠ BƯỚC NÀY GỌI THẬT NÊN MẶC ĐỊNH KHÔNG CHẠY. Bản đầu của tôi bấm thẳng
    // "Đóng sổ", và đó là `/handover` = một lời gọi `claude` trên fork: ba lượt
    // chạy kịch bản đã tiêu 3.19 + 4.44 (thước đo) và đẻ ra hai phiên mới, làm
    // chính các phép đếm phía trên lệch ở lượt sau. Đúng cái bẫy `CLAUDE.md` đã
    // ghi bằng máu ($6.75 một tối). Nút "Dừng" còn tệ hơn: nó phá trạng thái.
    if (process.env.HUB_UC_ACT === "1") {
      await page.locator(`${sel} .sess-acts button`).nth(1).click();   // Đóng sổ
      await page.waitForTimeout(900);
      const stillOnList = await page.evaluate(() =>
        !document.getElementById("sessDetail") ||
        document.getElementById("sessDetail").classList.contains("hidden")
      );
      check("bấm nút trên dòng KHÔNG nhảy sang màn chi tiết", stillOnList);
    } else {
      console.log(
        "  · BỎ QUA 1 kiểm tra vì nó gọi thật (tốn hạn mức + đẻ phiên mới): " +
        "'bấm nút trên dòng KHÔNG nhảy sang màn chi tiết'. Muốn chạy: HUB_UC_ACT=1"
      );
    }
  }

  await page.screenshot({ path: `${SHOTS}sessions-01-phone.png` });
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
console.log(`ảnh: ui-shots/sessions-01-phone.png`);
