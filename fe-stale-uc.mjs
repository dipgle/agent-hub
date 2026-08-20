// UC-S09, nửa "đã cũ" — "ảnh chụp cũ thì phải NÓI là cũ".
//
// UC-S09 (UC.md:526-540) có hai nửa. Nửa "còn tươi" (ảnh chụp mới thì KHÔNG
// kêu cảnh báo oan) đã chạy trong `fe-newsession-uc.mjs:86-95`. Nửa còn lại —
// ảnh chụp ĐÃ CŨ thì trang phải tự nói ra, chứ không lặng lẽ hiện số liệu cũ
// như thể vừa mới — chưa từng chạy (UC.md:538-540: "⏳ nửa 'đã cũ' chưa chạy:
// phải tắt hubad rồi chờ qua 5 phút"). Kịch bản này là nửa còn thiếu đó.
//
// Vì sao nó nợ tới hôm nay: đây là UC DUY NHẤT trong repo đòi TẮT chính cái nó
// đang kiểm — trong lúc `hubad` đứng im, huba MÙ với mọi phiên trên máy (không
// đẩy ảnh chụp mới, không trả lời `/session`, `/ask`...) suốt quãng chờ. Đó là
// một cái giá thật, không phải chỉ tốn thời gian máy chạy test — nên kịch bản
// phải KIỂM CHO ĐÁNG cái outage ấy: đo đúng ngưỡng sản phẩm dùng (không đoán
// số), đo hai chiều (marker phải CÓ, chữ "còn tươi" phải VẮNG), và đòi danh
// sách phiên vẫn hiện ra chứ không trắng trơn — chứ không chỉ liếc một dòng
// rồi coi là xong.
//
// NGƯỠNG CŨ ĐỌC TỪ SẢN PHẨM, KHÔNG ĐOÁN: `fe/index.html:1675-1676`
//   const ageMin = (Date.now() - Date.parse(snap.generated_at)) / 60000;
//   const stale = !(ageMin < 5);   // NaN (unparseable stamp) counts as stale
// và lý do chọn 5 phút, ngay phía trên nó (`fe/index.html:1673-1674`): "hubad
// cycles every ~120s (4s while following a session), so anything past 5
// minutes means it is not running." Chữ hiện ra khi cũ (`fe/index.html:1683`):
//   stamp.classList.toggle('stale', stale);
//   stamp.textContent = stale
//     ? `⚠ huba chưa đẩy dữ liệu mới — ${when}. Số dưới đây là của lúc đó, không phải bây giờ.`
//     : when;
// `when` (khi đứng ở danh sách, không mở chi tiết phiên) là `stampFull` =
// `Ảnh chụp lúc ... (Np trước)` (`fe/index.html:1635,1637`) — nên chữ CŨ vẫn
// CHỨA "Ảnh chụp lúc" ở giữa câu, chỉ là nó không còn đứng ĐẦU câu (câu cũ mở
// đầu bằng "⚠"). Đây chính là cái bẫy: đo "có chứa 'Ảnh chụp lúc'" sẽ xanh ở
// CẢ HAI trạng thái và không phân biệt được gì — phải đo đầu câu.
//
// Cách vận hành (người thao tác làm tay — kịch bản này KHÔNG đụng launchctl):
//   1. Dừng hubad:
//        launchctl bootout gui/$(id -u)/com.dipgle.hubd
//      (nhãn + lệnh y hệt deploy/com.dipgle.hubd.plist:17)
//   2. Đứng im QUÁ 5 phút — không portal-push, không chạy `huba` tay. Trong lúc
//      này huba mù với mọi phiên; đó là cái giá của phép đo này.
//   3. node fe-stale-uc.mjs
//   4. Bật lại hubad:
//        launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.dipgle.hubd.plist
//      (y hệt deploy/com.dipgle.hubd.plist:12)
//
// Nếu chạy khi ảnh chụp vẫn còn tươi (hubad chưa kịp tắt quá 5 phút), kịch bản
// THOÁT MÃ 2 và nói rõ chưa dựng được trạng thái — không tự ý coi "chưa đo
// được" là "đạt", đúng khuôn `fe-subagent-uc.mjs` đã dùng cho UC-S02b.
//
// Usage: node fe-stale-uc.mjs
import { chromium } from "/Users/hanguyen/projects/AI/sdvi/web-v2/node_modules/playwright-core/index.mjs";
import { readFileSync, mkdirSync } from "node:fs";

const HERE = new URL("./", import.meta.url).pathname;
const env = Object.fromEntries(
  readFileSync(HERE + "huba.env", "utf8")
    .split("\n").filter((l) => l.includes("=") && !l.trim().startsWith("#"))
    .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim().replace(/^["']|["']$/g, "")])
);
const APP_TID = env.HUB_TFL5_APP_TID || "a-65dd60d3-624e-45a9-8fdf-62aa7d894d80";
const SHOTS = HERE + "ui-shots/";
mkdirSync(SHOTS, { recursive: true });

// Ngưỡng đọc từ fe/index.html:1631 — xem trích dẫn trong header. Không hardcode
// mù: nếu ai đổi ngưỡng trong sản phẩm mà quên đổi ở đây, comment trên vẫn
// trỏ đúng chỗ để phát hiện lệch.
const STALE_THRESHOLD_MIN = 5;

const checks = [];
const check = (name, ok, detail = "") => {
  checks.push({ name, ok });
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? ` — ${detail}` : ""}`);
};

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({
  viewport: { width: 390, height: 844 },
  deviceScaleFactor: 3,
  isMobile: true,
  hasTouch: true,
});
const errors = [];
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text().slice(0, 200)); });
page.on("pageerror", (e) => errors.push(`uncaught: ${e.message.slice(0, 200)}`));

try {
  await page.goto(`http://${APP_TID}.test.localhost:8090`, { waitUntil: "domcontentloaded" });
  await page.fill("#u", "alice_local");
  await page.fill("#p", env.HUB_TFL5_ALICE_PASSWORD);

  // NGUỒN SỰ THẬT VỀ TUỔI ẢNH CHỤP PHẢI ĐỘC LẬP VỚI CÁCH TRANG VẼ CHỮ — cùng lý
  // do `fe-subagent-uc.mjs` tự đọc transcript thay vì tin `huba sessions --json`.
  // Ở đây "độc lập" nghĩa là: bắt đúng response mạng mà `loadBoard()` gọi
  // (`fe/index.html:1603`, POST `/app/doc/list`) và tự tính `ageMin` bằng đúng
  // công thức đã trích ở header — KHÔNG đọc lại text trên màn rồi tự tin theo.
  // Đây là THĂM DÒ (đọc response đã có sẵn, không tự tạo dữ liệu), không phải
  // hành vi "bơm API thay UI" — trạng thái cũ/mới của ảnh chụp là do `hubad` có
  // chạy hay không quyết định, kịch bản chỉ ĐỌC nó qua đúng lệnh gọi mà trang
  // thật đã tự bắn ra khi người dùng đăng nhập.
  const [snapRes] = await Promise.all([
    page.waitForResponse(
      (r) => r.request().method() === "POST" && r.url().includes("/app/doc/list"),
      { timeout: 20000 }
    ),
    page.click("#loginBtn"),
  ]);
  const snapBody = await snapRes.json();
  const snapRow = (snapBody.data || [])[0];
  const snap = snapRow && snapRow.data && snapRow.data.snapshot;

  await page.waitForSelector("#panelTabs", { timeout: 15000 });
  await page.waitForSelector("#panel-sessions:not(.hidden)", { timeout: 5000 }); // tab Phiên = mặc định (fe/index.html:772)
  await page.waitForFunction(
    () => !/đang tải/.test(document.getElementById("boardStamp").textContent),
    null,
    { timeout: 20000 }
  );

  if (!snap || !snap.generated_at) {
    console.error("✗ CHƯA DỰNG ĐƯỢC TRẠNG THÁI CẦN ĐO: response /app/doc/list không mang snapshot (huba có thể CHƯA TỪNG portal-push).");
    console.error("  UC-S09 (nửa 'đã cũ') KHÔNG được tính là đạt bằng lần chạy này.");
    console.error("  Chạy `huba portal-push` một lần cho có ảnh chụp, rồi mới làm theo quy trình ở đầu file này.");
    await browser.close();
    process.exit(2);
  }

  const ageMin = (Date.now() - Date.parse(snap.generated_at)) / 60000;
  if (Number.isNaN(ageMin)) {
    console.error(`✗ CHƯA DỰNG ĐƯỢC TRẠNG THÁI CẦN ĐO: generated_at "${snap.generated_at}" không đọc được thành ngày giờ.`);
    console.error("  Sản phẩm coi mốc không đọc được là 'cũ', nhưng kịch bản này cần đo được TUỔI thật để kiểm chữ trên màn — không đoán được thì không kiểm được.");
    await browser.close();
    process.exit(2);
  }

  console.log(`ảnh chụp generated_at=${snap.generated_at} → tuổi hiện tại ${ageMin.toFixed(1)} phút (ngưỡng cũ = ${STALE_THRESHOLD_MIN} phút, fe/index.html:1631)`);

  if (ageMin < STALE_THRESHOLD_MIN) {
    console.error(`\n✗ CHƯA DỰNG ĐƯỢC TRẠNG THÁI CẦN ĐO: ảnh chụp còn TƯƠI (${ageMin.toFixed(1)} phút < ${STALE_THRESHOLD_MIN} phút) — hubad vẫn đang đẩy dữ liệu, hoặc mới tắt chưa đủ lâu.`);
    console.error("  UC-S09 (nửa 'đã cũ') KHÔNG được tính là đạt bằng ảnh chụp này — nửa 'còn tươi' đã có ở fe-newsession-uc.mjs rồi, lặp lại nó không kiểm thêm gì.");
    console.error("  Người thao tác cần, theo đúng thứ tự:");
    console.error("    1) launchctl bootout gui/$(id -u)/com.dipgle.hubd     # dừng daemon — deploy/com.dipgle.hubd.plist:17");
    console.error(`    2) đứng im QUÁ ${STALE_THRESHOLD_MIN} phút — không portal-push, không chạy \`huba\` tay (huba sẽ mù trong lúc này, đó là giá của phép đo)`);
    console.error("    3) node fe-stale-uc.mjs");
    console.error("    4) launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.dipgle.hubd.plist   # bật lại — deploy/com.dipgle.hubd.plist:12");
    await browser.close();
    process.exit(2);
  }

  // ---- từ đây, ảnh chụp CHẮC CHẮN cũ hơn ngưỡng — kiểm trang có nói ra không ----
  const stampEl = page.locator("#boardStamp");
  const stampText = (await stampEl.innerText()).trim();
  const isStaleClass = await stampEl.evaluate((el) => el.classList.contains("stale"));

  // Đảo NGƯỢC đúng phép đo `fe-newsession-uc.mjs:91-95` dùng cho chiều "còn
  // tươi" (`!classList.contains('stale')`) — cặp hai chiều đối xứng cho UC-S09.
  check("có marker/class 'stale' trên #boardStamp", isStaleClass, `class="${await stampEl.getAttribute("class")}"`);

  // Chữ cảnh báo đúng nguyên văn sản phẩm (fe/index.html:1640), không chỉ suy
  // luận từ class — class có thể đúng mà chữ bị đổi/thiếu ở một đợt sửa sau.
  check("chữ nói thẳng 'huba chưa đẩy dữ liệu mới'", /⚠ huba chưa đẩy dữ liệu mới/.test(stampText), stampText.slice(0, 90));
  check("chữ nói rõ số liệu là CỦA LÚC ĐÓ, không phải bây giờ", /không phải bây giờ/.test(stampText), stampText.slice(0, 90));

  // Chữ phải NÊU TUỔI, không chỉ nói chung chung "cũ" — cùng định dạng `agoOf`
  // dùng cho CLI (fe/index.html:2827-2836): "Np trước" / "Ng trước" / "Nng trước".
  const namesAge = /\d+(?:p|g|ng) trước/.test(stampText);
  check("chữ nêu đúng TUỔI ảnh chụp (định dạng Np trước/Ng trước/Nng trước)", namesAge, stampText.slice(0, 90));

  // HAI CHIỀU: chữ "còn tươi" (mở đầu bằng "Ảnh chụp lúc", không có "⚠") phải
  // VẮNG MẶT. Không đủ nếu chỉ kiểm "chuỗi con 'Ảnh chụp lúc' có mặt" — nó vẫn
  // có mặt Ở GIỮA câu cũ vì `when` bị nhúng vào (xem giải thích trong header).
  // Phải kiểm đầu câu: câu cũ luôn mở bằng "⚠", câu tươi luôn mở bằng "Ảnh chụp
  // lúc" — đây là điểm phân biệt thật giữa hai trạng thái.
  const looksLikeFreshOnly = stampText.startsWith("Ảnh chụp lúc");
  check(
    "chữ 'còn tươi' (mở đầu bằng 'Ảnh chụp lúc', không cảnh báo) VẮNG MẶT — cặp hai chiều với fe-newsession-uc.mjs:91-95",
    !looksLikeFreshOnly,
    stampText.slice(0, 30)
  );

  // Trang vẫn phải hiện DANH SÁCH PHIÊN, không trắng trơn — ảnh chụp cũ vẫn là
  // dữ liệu có thật của một thời điểm trước, `renderSessions()` chạy vô điều
  // kiện sau đoạn set chữ cảnh báo (fe/index.html:1642), nên nếu màn trắng thì
  // đó là một lỗi khác đang núp sau lớp cảnh báo.
  const sessCount = await page.locator("#sessList .sess").count();
  check("danh sách phiên vẫn hiện ra, không trắng trơn", sessCount > 0, `${sessCount} thẻ phiên`);

  // ĐỌC HẾT ĐƯỢC không, chứ không chỉ CÓ MẶT.
  //
  // Lượt chạy đầu của kịch bản này xanh 7/7 trong khi màn hình chỉ hiện tới
  // `⚠ huba chưa đẩy dữ liệu mới — Ảnh chụp lúc 19:50:59 1…` — nửa sau bị cắt,
  // mà nửa sau mới là phần đổi hành vi người đọc ("số dưới đây là của lúc đó").
  // Bảy assert đều đọc `textContent`, thứ có đủ chữ kể cả khi màn cắt cụt; chỉ
  // MỞ ẢNH RA NHÌN mới thấy. Nay hỏi trình duyệt bằng `scrollWidth`.
  const fit = await stampEl.evaluate((el) => ({
    cut: el.scrollWidth > el.clientWidth + 1,
    shown: el.textContent.trim(),
  }));
  check(
    "cảnh báo ĐỌC HẾT ĐƯỢC trên màn 390px, không bị cắt đuôi",
    !fit.cut,
    fit.cut ? `BỊ CẮT: ${fit.shown.slice(0, 70)}…` : "hiện trọn câu"
  );

  await page.screenshot({ path: `${SHOTS}stale-snapshot.png`, fullPage: true });

  check("0 lỗi console", errors.length === 0, errors.join(" | "));
} catch (e) {
  check("kịch bản chạy trọn vẹn", false, e.message.split("\n").slice(0, 3).join(" | "));
} finally {
  await browser.close();
}

const passed = checks.filter((c) => c.ok).length;
console.log(`\n${passed}/${checks.length} kiểm tra đạt`);
process.exit(passed === checks.length ? 0 : 1);
