// Bộ lái trình duyệt của huba — MỘT lệnh, một dòng JSON ra.
//
// 🔴 Hà 2026-08-23: *"Sao khong dùng playwright"*, hỏi ngay sau *"Tôi ko ngồi
// máy"*. Hai câu ấy là một câu, và nó lật lựa chọn đầu của tôi:
//
// * Đường AppleScript (`rust/src/browser.rs`) lái đúng cái Chrome đang đăng
//   nhập của chủ máy — nghe hợp phép thử cầu nối hơn. Nhưng nó cần quyền Tự
//   động hoá `hubd → Google Chrome`, mà **cấp quyền ấy phải ngồi trước máy**.
//   Một tính năng cho người ở xa mà cánh cửa đầu tiên của nó chỉ mở được khi
//   ngồi ở nhà thì chưa phải là cầu.
// * CDP không đi qua Apple Events, cũng không cần Screen Recording — **không
//   quyền macOS nào**. Nên đường này chạy được ngay bây giờ, từ điện thoại, và
//   chạy cả khi màn hình đang khoá.
//
// ## Ba số đo quyết định hình dạng tệp này, đo trên chính máy này 2026-08-23
//
// 1. **Đường "không cần Node" đã thử rồi mới bỏ.** `Google Chrome --headless
//    --dump-dom https://example.com` in ra đúng 561 byte DOM thật — nhưng tiến
//    trình KHÔNG thoát, chạy 3 phút vẫn treo, phải giết. Muốn dùng thì phải tự
//    đọc stdout rồi tự giết: đúng thứ `CLAUDE.md` gọi là đọc mã thoát của một
//    thứ chỉ khởi chạy. Playwright sinh ra để nuốt đúng lớp đó.
// 2. **Binary của Playwright KHÔNG ra được mạng trên máy này.** Cùng một đường
//    CDP, cùng một đoạn mã: `Chrome for Testing` (chromium-1228 trong cache
//    ms-playwright) treo hết 20 giây ở `page.goto`; `/Applications/Google
//    Chrome.app` xong trong **169 ms**. Biến số duy nhất là cái binary — nên
//    huba lái **Chrome thật**, và Playwright chỉ làm bộ lái.
// 3. **Hồ sơ mặc định không mở CDP được** (Chrome cấm), nên phải có hồ sơ
//    riêng. Đó vừa là giới hạn vừa là tính năng: không thừa hưởng phiên đăng
//    nhập nào của Chrome thật, nhưng GIỮ được phiên nó tự tạo — đăng nhập một
//    lần từ điện thoại thì lần sau còn nguyên.

import { chromium } from 'playwright'
import { spawn } from 'node:child_process'
import { mkdirSync, existsSync, readFileSync, writeFileSync, openSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const PORT = Number(process.env.HUB_WEB_PORT ?? 9334)
const PROFILE = join(HERE, 'data', 'browser-profile')
const LOCK = join(HERE, 'data', 'browser.pid')
// 🔴 Hà 2026-08-23: *"Làm sao giữ được cache, vì tôi thấy mỗi lần mở lại mất hết
// trạng thái cũ"*. Đo rồi mới vá, và hoá ra hai vế khác nhau:
//
// * **Cache thì KHÔNG mất.** Đặt `localStorage` + cookie, tắt hẳn trình duyệt,
//   mở lại — cả hai còn nguyên (hồ sơ đang giữ 112 MB thật). Đăng nhập một lần
//   là còn mãi; đó là công của `--user-data-dir`.
// * Thứ mất là **đang đứng ở trang nào**: Chrome khởi động về `about:blank`.
//   Với người cầm điện thoại thì đó đúng là "mất trạng thái" — mở lên thấy
//   trắng trơn, phải gõ lại địa chỉ.
//
// Nên nhớ lấy địa chỉ cuối, và lượt dựng NGUỘI thì tự quay về đó.
const LAST = join(HERE, 'data', 'browser-last.json')
const LOG = join(HERE, 'logs', 'browser.log')
const CONNECT_TIMEOUT_MS = 20_000

/// Chrome THẬT, không phải bản Playwright tải về — xem số đo 2 ở đầu tệp.
const BINS = [
  process.env.HUB_BROWSER_BIN,
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  '/Applications/Brave Browser.app/Contents/MacOS/Brave Browser',
].filter(Boolean)

function say(o) {
  process.stdout.write(JSON.stringify(o) + '\n')
}
function die(msg) {
  say({ error: String(msg) })
  process.exit(1)
}

async function noiVao(ms = 3000) {
  try {
    return await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`, { timeout: ms })
  } catch {
    return null
  }
}

function conDangSong() {
  try {
    const pid = Number(readFileSync(LOCK, 'utf8').trim())
    if (!pid) return 0
    process.kill(pid, 0) // chỉ HỎI, không giết
    return pid
  } catch {
    return 0
  }
}

async function dungTrinhDuyet() {
  // 🔴 MỘT BẢN, KHÔNG HAI. Bản đầu của tệp này dựng thêm một chromium mỗi khi
  // nối hụt, và ba tiến trình cùng giành một hồ sơ thì **cả ba treo** — đo
  // 23/08: cổng vẫn LISTEN mà `connectOverCDP` hết giờ ở 10 giây. Nên hỏi cái
  // khoá trước: có bản đang sống thì CHỜ nó, đừng dựng bản nữa.
  const cu = conDangSong()
  if (cu) {
    const b = await noiVao(CONNECT_TIMEOUT_MS)
    if (b) return b
    die(`pid ${cu} đang giữ hồ sơ nhưng cổng ${PORT} không trả lời — xem ${LOG}, hoặc {"do":"close"} rồi gọi lại`)
  }
  const bin = BINS.find((b) => existsSync(b))
  if (!bin) die(`không thấy trình duyệt nào ở: ${BINS.join(' · ')}`)
  mkdirSync(PROFILE, { recursive: true })
  mkdirSync(dirname(LOG), { recursive: true })
  // 🔴 KHÔNG `stdio:'ignore'`. Bản đầu vứt hết lời than của chromium, nên lượt
  // hỏng đầu tiên chỉ còn lại một câu "không nối được" — không nói được vì sao.
  // Đúng luật 3 của repo: không đường lỗi nào được đi qua trong im lặng.
  const nhatky = openSync(LOG, 'a')
  const con = spawn(
    bin,
    [
      '--headless=new',
      `--remote-debugging-port=${PORT}`,
      // Chỉ nghe ở máy này: thiếu dòng này thì bất kỳ ai cùng mạng cũng lái
      // được trình duyệt mang phiên đăng nhập của chủ máy.
      '--remote-debugging-address=127.0.0.1',
      `--user-data-dir=${PROFILE}`,
      '--no-first-run',
      '--no-default-browser-check',
    ],
    { detached: true, stdio: ['ignore', nhatky, nhatky] },
  )
  con.unref()
  writeFileSync(LOCK, String(con.pid))
  // Chờ bằng chính phép nối, không bằng một con số `sleep` đoán bừa.
  const han = Date.now() + CONNECT_TIMEOUT_MS
  while (Date.now() < han) {
    const b = await noiVao(1500)
    if (b) return b
    await new Promise((r) => setTimeout(r, 250))
  }
  die(`dựng được trình duyệt (pid ${con.pid}) nhưng cổng ${PORT} không mở sau ${CONNECT_TIMEOUT_MS / 1000}s — xem ${LOG}`)
}

function nhoTrang(url) {
  if (!/^https?:\/\//i.test(url ?? '')) return
  try {
    writeFileSync(LAST, JSON.stringify({ url, at: new Date().toISOString() }))
  } catch {}
}

function trangCuoi() {
  try {
    const u = JSON.parse(readFileSync(LAST, 'utf8')).url
    return /^https?:\/\//i.test(u) ? u : null
  } catch {
    return null
  }
}

async function trang(b) {
  const ctx = b.contexts()[0] ?? (await b.newContext())
  const pages = ctx.pages().filter((p) => !p.isClosed())
  return pages[pages.length - 1] ?? (await ctx.newPage())
}

const lenh = JSON.parse(process.argv[2] ?? '{}')

// `close` chạy được KỂ CẢ khi không nối được: "tắt cái vốn không chạy" là thành
// công, không phải lỗi.
if (lenh.do === 'close') {
  const b = await noiVao(3000)
  if (b) await b.close().catch(() => {})
  const pid = conDangSong()
  if (pid) {
    try {
      process.kill(pid, 'SIGTERM')
    } catch {}
  }
  try {
    writeFileSync(LOCK, '')
  } catch {}
  say({ ok: true, da: b || pid ? 'tat' : 'von khong chay' })
  process.exit(0)
}

let moiDung = false // lượt này có phải tự dựng trình duyệt không
let b = await noiVao(3000)
if (!b) {
  b = await dungTrinhDuyet()
  moiDung = true
}
const p = await trang(b)

// Dựng NGUỘI + trang trắng ⟹ quay về chỗ đang đứng lúc tắt. Không làm với
// `goto` (nó tự đi đâu đó rồi) và không làm khi trang đang có nội dung: người
// ta không muốn một cú nhảy bất ngờ dưới chân mình.
//
// 🔴 "Trang trắng" KHÔNG chỉ là `about:blank` — đo 23/08: hồ sơ đã có lịch sử
// thì Chrome mở thẳng `chrome://new-tab-page/`, nên điều kiện cũ trượt và lượt
// mở lại đứng ở Tab Mới. Bài kiểm chạy thật bắt được ngay; bản đầu đúng chỉ vì
// hồ sơ lúc ấy còn mới tinh.
const trangTrong = (u) => !u || u === 'about:blank' || /^chrome:\/\//i.test(u)
if (moiDung && lenh.do !== 'goto' && trangTrong(p.url())) {
  const cu = trangCuoi()
  if (cu) {
    await p.goto(cu, { waitUntil: 'domcontentloaded', timeout: 30_000 }).catch(() => {})
  }
}

try {
  switch (lenh.do) {
    case 'goto': {
      if (!/^https?:\/\//i.test(lenh.url ?? '')) die('chỉ mở http/https')
      // `domcontentloaded` chứ không `networkidle`: nhiều trang giữ một kết nối
      // mở mãi mãi (websocket, long-poll), nên `networkidle` là chờ tới hết giờ
      // trên đúng những trang đáng xem nhất.
      await p.goto(lenh.url, { waitUntil: 'domcontentloaded', timeout: 30_000 })
      nhoTrang(p.url())
      say({ url: p.url(), title: await p.title() })
      break
    }
    case 'where':
      say({ url: p.url(), title: await p.title() })
      break
    case 'text': {
      const chu = await p.evaluate(() => document.body?.innerText ?? '')
      say({ url: p.url(), title: await p.title(), text: chu })
      break
    }
    case 'shot': {
      if (!lenh.path) die('thiếu `path`')
      await p.screenshot({ path: lenh.path, fullPage: false })
      say({ url: p.url(), title: await p.title(), path: lenh.path })
      break
    }
    // Ba lệnh dưới đây mở ra việc ĐĂNG NHẬP — thứ trước 23/08 không làm được
    // từ xa, và là lý do "đăng nhập một lần còn mãi" mới chỉ chứng minh được
    // tới mức cookie chứ chưa tới mức một phiên thật.
    //
    // ⚠ Chữ gõ ở đây đi QUA TELEGRAM. Với mật khẩu thì đó là một quyết định của
    // chủ máy, không phải của huba — nhưng nó phải được nói ra chứ không lặng
    // lẽ nhận.
    case 'field': {
      if (!lenh.what) die('thiếu `what` — tên ô cần chọn')
      // Tìm theo thứ người đọc NHÌN THẤY, cùng luật với `click`: nhãn, chữ mờ
      // trong ô, rồi mới tới vai trò.
      const o = p
        .getByPlaceholder(lenh.what, { exact: false })
        .or(p.getByLabel(lenh.what, { exact: false }))
        .or(p.getByRole('textbox', { name: lenh.what }))
        .first()
      await o.click({ timeout: 10_000 })
      say({ url: p.url(), title: await p.title(), o: lenh.what })
      break
    }
    case 'type': {
      if (typeof lenh.text !== 'string' || lenh.text === '') die('thiếu `text`')
      // Chưa chọn ô nào thì rơi vào ô nhập ĐẦU TIÊN nhìn thấy được — ngồi ở máy
      // thì người ta bấm vào ô rồi mới gõ, và trang đăng nhập nào cũng để con
      // trỏ sẵn ở ô đầu.
      const dangO = await p.evaluate(() => {
        const e = document.activeElement
        return !!e && (e.tagName === 'INPUT' || e.tagName === 'TEXTAREA' || e.isContentEditable)
      })
      if (!dangO) {
        await p.getByRole('textbox').first().click({ timeout: 10_000 })
      }
      // `delay` để trang nào nghe từng phím (gợi ý, kiểm tra tức thời) kịp chạy.
      await p.keyboard.type(lenh.text, { delay: 20 })
      say({ url: p.url(), title: await p.title(), da_go: lenh.text.length })
      break
    }
    case 'press': {
      const phim = lenh.key || 'Enter'
      const truoc = p.url()
      await p.keyboard.press(phim)
      await p.waitForLoadState('domcontentloaded', { timeout: 15_000 }).catch(() => {})
      // 🔴 `domcontentloaded` KHÔNG bắt được điều hướng của một trang SPA — đo
      // 23/08 trên ô tìm kiếm DuckDuckGo: lệnh trả về ngay, `url` còn là trang
      // cũ và `title` RỖNG, rồi lượt đọc sau mới thấy trang kết quả. Trả một
      // câu như thế là nói rằng cú Enter không làm gì. Nên chờ thêm bằng chính
      // thứ đo được: địa chỉ có đổi không, hoặc tiêu đề đã kịp có chưa.
      for (let i = 0; i < 12 && p.url() === truoc; i++) {
        await new Promise((r) => setTimeout(r, 250))
      }
      for (let i = 0; i < 8 && !(await p.title()); i++) {
        await new Promise((r) => setTimeout(r, 250))
      }
      nhoTrang(p.url())
      say({ url: p.url(), title: await p.title() })
      break
    }
    case 'click': {
      if (!lenh.what) die('thiếu `what`')
      // Tìm theo CHỮ NGƯỜI ĐỌC THẤY, không theo selector: câu lệnh đến từ một
      // người đang nhìn ảnh chụp trang, và thứ họ nhìn thấy là chữ.
      await p.getByText(lenh.what, { exact: false }).first().click({ timeout: 10_000 })
      await p.waitForLoadState('domcontentloaded', { timeout: 15_000 }).catch(() => {})
      nhoTrang(p.url())
      say({ url: p.url(), title: await p.title() })
      break
    }
    default:
      die(`không hiểu lệnh: ${JSON.stringify(lenh.do)}`)
  }
} catch (e) {
  die(String(e?.message ?? e).split('\n')[0])
}
process.exit(0)
