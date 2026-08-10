# hub — các phiên Claude CLI trên máy này, xem và điều khiển từ điện thoại

Trên Mac lúc nào cũng có mươi phiên `claude` đang chạy trong các terminal. `hub`
đưa chúng lên **một trang trên tfl5**: xem phiên nào đang làm gì, mở phiên mới
cho một dự án, hỏi chen ngang một phiên mà không phá việc nó đang làm, dừng nó,
hoặc đóng sổ lấy bản bàn giao để làm tiếp trên máy.

Đường đi hai chiều, cả hai đều do máy này **gọi ra ngoài** — không mở cổng nào
vào máy:

```
  điện thoại ──► phòng chat tfl5 ──► hubd đọc lệnh ──► claude CLI trên Mac
       ▲                                    │
       └──────── ảnh chụp read-only ◄───────┘   (hubd đẩy mỗi vòng)
```

> **Trước đây hub là một hộp thư** (GitHub · devlog · email · Telegram → triage
> bằng `claude -p` → duyệt/gửi). Nhánh đó đã **xoá hẳn ngày 2026-08-08**: 65%
> hộp việc là thông báo CI, ngốn $5.89/$9.12 tổng chi, trong khi việc thật sự
> cần — điều khiển phiên từ xa — mới chạy đúng một lần. Chi tiết trong
> `CLAUDE.md`. Hệ quả thẳng thắn: **hub không còn tự tiêu hạn mức**; chỉ khi bạn
> bấm nút mới có một lần gọi `claude`, đúng giá như gõ ở terminal.

## Chạy trong 3 lệnh

```bash
cd ~/Documents/projects/AI/hub
./hub doctor          # kiểm tra thật: claude CLI, phòng chat, thư mục dự án
./hub sessions        # mọi phiên claude đang sống, mọi tài khoản
./hub once            # một vòng: đọc phòng → chạy lệnh trong đó → đẩy ảnh chụp
```

Bí mật (tài khoản tfl5) nằm trong `hub.env` (chmod 600), **chỉ tên biến** nằm
trong `hub.config.json`. Xem `hub.env.example`.

## Tự chạy cùng máy (launchd)

```bash
deploy/install.sh                                   # build → ký → cài → khởi động lại
cp deploy/com.dipgle.hubd.plist ~/Library/LaunchAgents/
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.dipgle.hubd.plist
launchctl list | grep hubd
tail -f ~/Library/Logs/hubd.err
```

launchd chạy **bản đã cài** `~/Library/Application Support/hub/bin/hubd`, không
phải bản `cargo` trong `target/`. Lý do nằm ở `CLAUDE.md` §12, tóm tắt: macOS
neo quyền theo chữ ký, `cargo` ký ad-hoc lại sau **mỗi** lệnh build/test, nên
bản trong `target/` đổi danh tính liên tục và mất quyền ngay lần bật máy sau.
Sửa mã xong mà quên `deploy/install.sh` thì daemon vẫn chạy mã cũ — tab **Sức
khoẻ** có một hàng nói thẳng điều đó.

`hubd` giữ một pid-lock (`data/hubd.lock`) nên hai daemon không cùng chạy; lỗi
vòng chạy được đếm và backoff luỹ thừa (tối đa 10 phút), sau 5 lần liên tiếp thì
ghi `logs/notify.log` + hiện thông báo macOS.

## Lệnh trong phòng chat

Chỉ chủ máy (`trust.tfl5_user_tids`) ra lệnh được; người khác gõ `/new` thì đó
chỉ là chữ.

| Lệnh | Việc |
|---|---|
| `/session <id>` | theo một phiên — ảnh chụp kế tiếp mang cả luồng của nó (`/session -` để bỏ theo) |
| `/new <dự án> <việc>` | mở phiên nền làm việc đó trong thư mục dự án |
| `/ask <câu hỏi>` | hỏi bên lề phiên đang theo, **trên bản fork** — phiên gốc không thêm lượt nào |
| `/tell <nội dung>` | nói tiếp vào phiên nền (phải `/stop` nó trước) |
| `/stop [id]` | dừng phiên nền, hội thoại vẫn giữ |
| `/handover [id]` | đóng sổ: bản bàn giao + phiên mới giữ nguyên ngữ cảnh + lệnh `--resume` |
| `/project [tên]` | xem/ghim dự án cho phòng |
| `/ingest` · `/run` · `/doctor` | đọc phòng ngay · chạy một vòng · kiểm tra thật |
| `/set <khoá> <giá trị>` | sửa một trường cấu hình (validate + backup + ghi nguyên tử) |
| `/help` | bảng này |

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
  khẩu trong lượt cuối.
- **Trang chỉ đọc**: mọi thứ thay đổi trạng thái đều đi qua phòng chat dưới dạng
  lệnh, nên luôn có dấu vết ở nơi người đọc được.

## Cấu hình

`hub.config.json` — không có bí mật, chỉ có TÊN biến môi trường:

```jsonc
{
  "poll_interval_sec": 120,
  "call": { "max_budget_usd": 0.5, "timeout_sec": 240 },  // trần MỘT lần gọi
  "adapters": { "tfl5": { "enabled": true, "app_tid": "a-…", "room": "hub",
                          "user_env": "HUB_TFL5_USER",
                          "password_env": "HUB_TFL5_PASSWORD", "live": true } },
  "trust": { "tfl5_user_tids": ["u-…"] },                  // ai ra lệnh được
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
| `runs` | mỗi lượt đọc phòng: ok/lỗi/skip, để bảng Sức khoẻ không phải đoán |
| `cursors` | mốc đọc phòng, phiên đang theo, bản bàn giao/hỏi-bên-lề gần nhất |
| `spend` | mỗi lần gọi `claude` do bạn bấm, kèm thước đo độ lớn — ghi để trả lời được, không để trưng |

Tệp cũ vẫn còn 4 bảng của hộp thư và dữ liệu trong đó. Không có lệnh nào xoá
chúng: dọn schema không phải lý do để xoá dữ liệu của người khác.

## Trang trên điện thoại

`fe/index.html` được đóng gói thành một bundle của app tfl5:

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
cd rust && cargo test --offline     # 67 test, 0 warning
cargo clippy --offline --all-targets
```

Test xanh là điều kiện cần, không phải đủ: đường thật phải chạy ít nhất một lần
(`./hub once` + một kịch bản `fe-*.mjs` trên bundle đã deploy).

## Bản Node cũ

`legacy-node/` là bản mẫu đầu tiên, giữ làm đối chiếu. Không chạy nữa.
