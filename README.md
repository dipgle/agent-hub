# hub — các phiên Claude CLI trên máy này, xem và điều khiển từ điện thoại

Trên Mac lúc nào cũng có mươi phiên `claude` đang chạy trong các cửa sổ Terminal.
`hub` đưa chúng vào **một buồng chat Telegram**: xem phiên nào đang làm gì, mở
phiên mới, gõ thẳng vào một phiên đang chạy, hỏi chen ngang mà không phá việc nó
đang làm, dừng nó, hoặc đóng sổ lấy bản bàn giao để làm tiếp trên máy.

Đường đi hai chiều, và **máy này luôn là bên gọi ra ngoài** — không mở cổng nào
vào máy, không có địa chỉ nào trỏ về đây:

```
  điện thoại ──► Telegram ──► hubd (long-poll getUpdates) ──► claude CLI trên Mac
       ▲                              │
       └──── tin báo + nút bấm ◄──────┘   (chỉ khi có THAY ĐỔI, không báo trạng thái)
```

## Telegram là kênh chính

Từ 2026-08-11 Telegram không còn là cái loa hai nút mà là **kênh ra lệnh đầy
đủ**: mọi động từ ở bảng dưới gõ được từ đó, và phần lớn việc thường ngày không
phải gõ gì cả — hub gắn sẵn nút.

- **Chữ thường (không có dấu `/`) đi thẳng vào phiên đang theo.** Bấm một phiên
  trong danh sách là coi như đang ngồi trong phiên ấy; sau đó gõ gì nó nhận nấy.
  Chưa theo phiên nào thì hub **không đoán** một phiên để gõ vào — gõ nhầm cửa
  sổ là thứ không lùi lại được.
- **Nút thay cho việc nhớ cú pháp:** danh sách phiên (mỗi phiên một nút), từng
  lựa chọn của một phiên đang dừng lại hỏi, `📄 Xem đầy đủ` (mở luôn phiên ấy),
  `📎` lấy một tệp phiên vừa nhắc tới, `👁 Vào phiên`, `⏎ Gửi`.
- **Cổng người:** biến `HUB_TELEGRAM_CHAT_ID` trong `hub.env`. Tin từ chat khác
  bị **ghi log rồi bỏ**, không im lặng. Cùng luật với `trust.tfl5_user_tids` của
  phòng chat: ở trong buồng là chuyện của Telegram, lái cái máy này là chuyện
  của chủ máy.
- **Chỉ nói khi có thay đổi.** `watch.rs` so hai lượt ảnh chụp rồi báo đúng một
  lần: phiên vừa xong, phiên dừng lại hỏi, phiên dừng vì lỗi, phiên đã tắt. Báo
  theo *trạng thái* trên một vòng lặp 10 giây là cái điện thoại rung mãi không
  thôi — và cái loa như thế thì người ta tắt, mất luôn những lần đáng nghe.
- **Phiên chạy trong VS Code / Cursor không lên danh sách** (2026-08-13). Gõ vào
  chúng phải qua Terminal.app, địa chỉ bằng tty, mà phiên editor không có cửa sổ
  Terminal nào — một dòng chỉ xem được mà không lái được thì nó không phải cây
  cầu, nó là một cái tên nữa để nhầm. Số phiên bị bỏ vẫn được đếm và nói ra.

**Phòng chat tfl5 + trang điện thoại vẫn chạy song song** (`adapters.tfl5`),
dùng chung đúng một bộ lệnh và một bộ sổ. Trang tfl5 là chỗ xem *rộng* — luồng
hội thoại của phiên, bảng sức khoẻ, form cấu hình; Telegram là chỗ dùng *nhanh*.

> **Trước đây hub là một hộp thư** (GitHub · devlog · email · Telegram → triage
> bằng `claude -p` → duyệt/gửi). Nhánh đó đã **xoá hẳn ngày 2026-08-08**: 65%
> hộp việc là thông báo CI, ngốn $5.89/$9.12 tổng chi, trong khi việc thật sự
> cần — điều khiển phiên từ xa — mới chạy đúng một lần. Chi tiết trong
> `CLAUDE.md`. Hệ quả thẳng thắn: **hub không còn tự tiêu hạn mức**; chỉ khi bạn
> bấm nút mới có một lần gọi `claude`, đúng giá như gõ ở terminal.

## Chạy trong 4 lệnh

```bash
cd ~/projects/hub
./hub setup           # trang cấu hình ở 127.0.0.1 → ghi hub.env (chmod 600)
./hub doctor          # kiểm tra thật: claude CLI, Telegram, phòng chat, thư mục dự án
./hub sessions        # mọi phiên claude đang sống, mọi tài khoản
./hub once            # một vòng: đọc kênh → chạy lệnh trong đó → đẩy ảnh chụp
```

Bí mật (bot token Telegram, tài khoản tfl5) nằm trong `hub.env` (chmod 600),
**chỉ tên biến** nằm trong `hub.config.json`. Xem `hub.env.example`.

`hub setup` chỉ nghe ở `127.0.0.1`, vào bằng vé một lần trong URL, và **không
bao giờ hiện lại giá trị đã lưu** — nó chỉ nói khoá ấy *đã có* hay *chưa có*.
Đọc một file 600 rồi bơm ngược ra HTTP là tự tay dựng đúng cái đường rò mà file
600 sinh ra để chặn.

Dựng chỗ làm việc từ đầu (thư mục, gốc workspace, ba câu hay hỏi ngược):
`ARCHITECTURE.md`.

## Tự chạy cùng máy (launchd)

```bash
./hub self-install                                  # build → ký → cài → khởi động lại

# plist là BẢN MẪU: ba dấu chỗ phải thay bằng đường tuyệt đối của máy này
# (launchd không hiểu `~` hay `$HOME`, mà repo công khai thì không mang đường
#  dẫn của máy ai cả)
sed -e "s|__HOME__|$HOME|g" \
    -e "s|__INSTALL_DIR__|$HOME/Library/Application Support/hub|g" \
    -e "s|__HUB_CONFIG__|$PWD/hub.config.json|g" \
    deploy/com.dipgle.hubd.plist > ~/Library/LaunchAgents/com.dipgle.hubd.plist

launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.dipgle.hubd.plist
launchctl list | grep hubd
tail -f ~/Library/Logs/hubd.err
```

`./hub self-install` làm đúng ba bước của `deploy/install.sh` (build → ký bằng
chứng chỉ → cài ra đường riêng), **viết lại trong Rust** chứ không gọi script:
hub phải tự cài được chính nó, không thì mỗi bản vá lại cần một người ngồi ở
máy gõ một dòng — đúng thứ dự án này sinh ra để bỏ đi. Từ điện thoại: `/upgrade`.
Hỏng ở bất kỳ bước nào thì **không đụng vào bản đang cài**.

launchd chạy **bản đã cài** `~/Library/Application Support/hub/bin/hubd`, không
phải bản `cargo` trong `target/`. Lý do nằm ở `CLAUDE.md` §12, tóm tắt: macOS
neo quyền theo chữ ký, `cargo` ký ad-hoc lại sau **mỗi** lệnh build/test, nên
bản trong `target/` đổi danh tính liên tục và mất quyền ngay lần bật máy sau.
Sửa mã xong mà quên `deploy/install.sh` thì daemon vẫn chạy mã cũ — tab **Sức
khoẻ** có một hàng nói thẳng điều đó.

`hubd` giữ một pid-lock (`data/hubd.lock`) nên hai daemon không cùng chạy; lỗi
vòng chạy được đếm và backoff luỹ thừa (tối đa 10 phút), sau 5 lần liên tiếp thì
ghi `logs/notify.log` + hiện thông báo macOS.

## Lệnh (giống hệt nhau ở Telegram và phòng chat tfl5)

Chỉ chủ máy ra lệnh được — Telegram gác bằng `chat_id`, phòng chat gác bằng
`trust.tfl5_user_tids`; người khác gõ `/new` thì đó chỉ là chữ. Hai kênh đi qua
**cùng một bộ phân tích** (`tfl5::parse_command`), nên không có động từ nào chỉ
chạy được ở một bên.

**Phiên Claude**

| Lệnh | Việc |
|---|---|
| `/sessions` (hay `/session` trống) | danh sách phiên đang sống, mỗi phiên một nút — bấm để theo (`/session -` để thôi theo) |
| *(chữ thường, không dấu `/`)* | gõ thẳng vào phiên đang theo |
| `/new [-a acc] [-s dự án] <việc>` | mở một **cửa sổ Terminal thật** chạy `claude` rồi gõ việc ấy vào; việc rỗng cũng được (mở cửa sổ rồi nói sau) |
| `/ask <câu hỏi>` | hỏi bên lề phiên đang theo, **trên bản fork** — phiên gốc không thêm lượt nào |
| `/tell <nội dung>` | nói tiếp vào phiên nền (phải `/stop` nó trước) |
| `/stop [id]` | phiên nền: dừng, hội thoại vẫn giữ · phiên cửa sổ hub mở: **tắt hẳn** — `/exit` rồi đóng cửa sổ |
| `/handover [id]` | đóng sổ: bản bàn giao + phiên mới giữ nguyên ngữ cảnh + lệnh `--resume` |

**Gõ thẳng vào cửa sổ phiên** (chỉ phiên Terminal — xem mục Telegram ở trên)

| Lệnh | Việc |
|---|---|
| `/type <chữ>` | gõ chữ vào phiên đang theo, kèm Enter khi đo được là chữ còn nằm trong ô |
| `/key <up\|down\|left\|right\|enter\|esc\|tab\|space\|1-9>` | bấm một phím |
| `/shot` | đọc chữ đang hiện trên màn của phiên |

**Vận hành**

| Lệnh | Việc |
|---|---|
| `/accounts` | ba tài khoản: phiên nào của ai, còn bao nhiêu hạn mức, `/new` mặc định vào tài khoản nào |
| `/cmd <dòng lệnh>` | chạy một lệnh trên máy rồi trả kết quả (chạy xong là hết) |
| `/upgrade` | hub tự dựng lại chính nó từ mã hiện tại rồi khởi động lại |
| `/project [tên]` | xem/ghim dự án cho kênh (`/project -` để bỏ ghim) |
| `/ingest` · `/run` · `/doctor` | đọc kênh ngay · chạy một vòng · kiểm tra thật |
| `/set <khoá> <giá trị>` | sửa một trường cấu hình (validate + backup + ghi nguyên tử) |
| `/help` | bảng này, đọc thẳng từ mã |

`/ask`, `/handover`, `/new`, `/tell` gọi `claude` thật nên **ăn vào hạn mức
của gói** — đúng như khi bạn tự gõ ở terminal, không hơn.

Máy này chạy gói **Max** (`claude auth status` → `subscriptionType: max`): không
có hoá đơn tính theo từng lần gọi. Con số `total_cost_usd` mà CLI trả về được
quy theo **giá API niêm yết**, nên nó là **thước đo một cú gọi TO cỡ nào**, không
phải tiền bị trừ khỏi tài khoản. Sổ `spend` ghi thước đo ấy, im lặng; màn hình
không hiện.

## Hàng rào (vì sao thiết kế vậy)

- **Phiên nền chạy sau một danh sách cấm** (`sessions::DENIED_TOOLS`): không
  push/merge/reset, không ssh/scp/sudo/rm/docker/launchctl/`*deploy*`, không
  WebFetch/WebSearch. Ghi file trong cây làm việc thì được — đó là công việc.
- **Hỏi/bàn giao chạy trên bản fork** với allowlist `Read,Grep,Glob`: một câu
  hỏi gõ trên điện thoại **không có tay để ghi**. Nghiệm thu của UC này là tệp
  nhật ký GỐC: y nguyên số byte, y nguyên mtime.
- **Xem trước nội dung phiên bị quét rò rỉ** trước khi lên ảnh chụp
  (`redaction::leak_scan`) — lần chạy thật đầu tiên đã in ra một phiên có mật
  khẩu trong lượt cuối. Cổng đặt ở NGUỒN (`sessions::snapshot`), nên cả Telegram
  lẫn trang tfl5 đều đi qua nó; không kênh nào tự bịa thêm nội dung.
- **Bấm phím thì phải NHÌN trước.** Một mũi tên trong `claude` vừa di chuyển vừa
  xác nhận, nên hub chỉ gửi khi **chứng minh được** màn không có hộp chọn —
  `keys::look` trả `Saw`/`Withheld`/`Blind` chứ không gộp cả ba vào `None`, vì
  gộp là fail OPEN đúng lúc hub mù nhất (kể cả lúc trên màn đang có mật khẩu).
- **Trang chỉ đọc**: mọi thứ thay đổi trạng thái đều đi qua Telegram hoặc phòng
  chat dưới dạng LỆNH, nên luôn có dấu vết ở nơi người đọc được.

## Cấu hình

`hub.config.json` — không có bí mật, chỉ có TÊN biến môi trường:

```jsonc
{
  "poll_interval_sec": 120,
  "call": { "max_budget_usd": 0.5, "timeout_sec": 240 },  // trần MỘT lần gọi
  // Kênh Telegram. Tên khoá là `confirm` vì thời nó mới sinh ra chỉ có hai nút
  // Xác nhận/Huỷ; nay đây là kênh ra lệnh chính. Chỉ TÊN biến, không có giá trị.
  "confirm": { "enabled": true,
               "bot_token_env": "HUB_TELEGRAM_BOT_TOKEN",
               "chat_id_env": "HUB_TELEGRAM_CHAT_ID",   // đây là cổng người
               "timeout_sec": 90 },
  "adapters": { "tfl5": { "enabled": true, "app_tid": "a-…", "room": "hub",
                          "user_env": "HUB_TFL5_USER",
                          "password_env": "HUB_TFL5_PASSWORD", "live": true } },
  "trust": { "tfl5_user_tids": ["u-…"] },                  // ai ra lệnh được (phòng chat)
  "auto_handover": { "enabled": true, "at_percent": 60, "idle_sec": 120 },
  "projects": { "tfl5": {}, "sdvi": {} },                  // tên thư mục
  "claude_accounts": [{ "name": "acc1" }, { "name": "acc2", "config_dir": "~/.claude-acc2" }]
}
```

Khoá cũ của thời hộp thư (`triage`, `act`, `autonomy`, `routing`,
`daily_budget_usd`, `max_triage_per_cycle`, `web`, `leak_patterns`) nay là khoá
lạ: tệp cũ vẫn nạp được, lần ghi kế tiếp thì rụng.

## Dữ liệu

`data/hub.sqlite` (WAL) — ba bảng:

| bảng | giữ gì |
|---|---|
| `runs` | mỗi lượt đọc kênh: ok/lỗi/skip, để bảng Sức khoẻ không phải đoán |
| `cursors` | mốc đọc kênh, phiên đang theo, sổ `watch:sessions`, bản bàn giao/hỏi-bên-lề gần nhất |
| `spend` | mỗi lần gọi `claude` do bạn bấm, kèm thước đo độ lớn — ghi để trả lời được, không để trưng |

(cộng `schema_meta`.) Bốn bảng của thời hộp thư đã đi hẳn ở bước schema 4
(2026-08-10) — chúng sống thừa hai ngày sau khi sản phẩm ấy bị xoá và giữ 379
dòng không truy vấn nào với tới. Migration ấy chạy với `foreign_keys` TẮT (bốn
bảng tham chiếu lẫn nhau; bản đầu chết ngay lúc khởi động, exit 70) và **log
từng bảng kèm số dòng** chứ không "đã dọn xong" một câu.

## Trang trên điện thoại (tfl5)

Kênh chính là Telegram; trang này là chỗ **xem rộng** — luồng hội thoại đầy đủ
của một phiên, bảng sức khoẻ, form cấu hình. `fe/index.html` được đóng gói thành
một bundle của app tfl5:

```bash
node fe-deploy.mjs v56 "đổi gì đó"     # zip → Releases → Activate → đối chiếu byte
```

Bốn tab: **Phiên** (danh sách + luồng + các nút Hỏi/Bàn giao/Dừng/Mở),
**Trao đổi** (phòng chat), **Sức khoẻ** (dò thật + lịch sử lượt chạy),
**Cấu hình** (form cho từng trường, mỗi thay đổi đi qua `/set`).

Nghiệm thu chạy trên **bundle đã deploy**, ở cỡ điện thoại 390×844, đăng nhập
bằng tài khoản chủ:

```bash
node fe-board-uc.mjs                 # 4 tab · sức khoẻ · cấu hình · KHÔNG có hộp việc/số tiền
node fe-sessions-uc.mjs  <app> <user> <pass>
node fe-stream-uc.mjs    <app> <user> <pass>   # bước /handover: có cổng giá
node fe-aside-uc.mjs     <app> <user> <pass>   # bước /ask: có cổng giá
node fe-newsession-uc.mjs <app> <user> <pass>
node fe-subagent-uc.mjs                # UC-S02b: phiên đang chạy subagent (cần có subagent THẬT)
node fe-config-uc.mjs · fe-denied-uc.mjs · fe-smoke.mjs · fe-phone-uc.mjs
```

**Cổng hạn mức.** Hai kịch bản trên có một bước gọi `claude` thật, độ lớn tỉ lệ
độ dài nhật ký (mốc đo: 0.99 MB → thước đo $1.72). Chúng **ước lượng trước và
mặc định KHÔNG gọi**: quá `HUB_UC_MAX_USD` (mặc định $0.25 quy theo giá API) thì
bỏ qua bước đó, không tính là đạt, và in rõ *"N BỎ QUA vì tốn hạn mức"* kèm thứ
chưa nghiệm thu. Muốn có bằng chứng ấy:

```bash
HUB_UC_PAY=1 node fe-stream-uc.mjs <app> <user> <pass>
```

## Test

```bash
cd rust && cargo test --offline     # 250 test, 0 warning
cargo clippy --all-targets -- -D warnings
```

Test xanh là điều kiện cần, không phải đủ: đường thật phải chạy ít nhất một lần
(`./hub once` + một kịch bản `fe-*.mjs` trên bundle đã deploy).

## Bản Node cũ

`legacy-node/` là bản mẫu đầu tiên, giữ làm đối chiếu. Không chạy nữa.
