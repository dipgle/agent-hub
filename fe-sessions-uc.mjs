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
const truth = JSON.parse(
  execFileSync(HERE + "rust/target/release/hub", ["sessions", "--json"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  })
);
const liveCount = truth.sessions.length;
const liveAccounts = [...new Set(truth.sessions.map((s) => s.account))].sort();
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
  const cards = await page.locator("#sessList .sess").count();
  check("số phiên trên màn khớp với máy", cards === liveCount, `màn ${cards} / máy ${liveCount}`);

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
  await page.evaluate(() => {
    const m = document.querySelector("main");
    m.scrollTop = Math.round(m.scrollHeight * 0.6);
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
  await page.waitForTimeout(17000);            // qua ít nhất một nhịp làm mới
  const afterList = await readCard();
  check(
    "làm mới danh sách KHÔNG đẩy chỗ đang đọc đi",
    beforeList.id.length > 0 && afterList.id === beforeList.id &&
      Math.abs(afterList.y - beforeList.y) <= 4,
    `${beforeList.name} y=${beforeList.y} → ${afterList.name} y=${afterList.y}`
  );
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
      else if (cur) cur.marks.push(el.dataset.activity || "");
    }
    return out;
  });
  const unsorted = perGroup.filter((grp) => {
    const m = grp.marks.filter(Boolean);
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
  const expect = {
    hub: truth.sessions.filter((x) => x.started_by_hub && x.host !== "dead").length,
    terminal: truth.sessions.filter((x) => x.host === "terminal" && !(x.started_by_hub)).length,
    background: truth.sessions.filter((x) => x.host === "background" && !(x.started_by_hub)).length,
    detached: truth.sessions.filter((x) => x.host === "detached" && !(x.started_by_hub)).length,
    dead: truth.sessions.filter((x) => x.host === "dead").length,
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
