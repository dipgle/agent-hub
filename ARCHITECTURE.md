# Kiến trúc & cách dựng chỗ làm việc

Tài liệu này trả lời đúng một câu: **kéo repo về rồi đặt mọi thứ ở đâu để nó
chạy được** — cho cả người không rành kỹ thuật.

`README.md` nói huba *làm gì*. `CLAUDE.md` nói *luật khi sửa huba*. Còn đây là bố
cục thư mục, cách cấu hình, và ba quyết định mà người mới hay hỏi ngược.

---

## 1. Một gốc, mỗi ứng dụng một thư mục

huba không quản lý mã của bạn. Nó **nhìn vào một gốc** rồi coi mỗi thư mục con là
một ứng dụng — đúng như bạn đã ngồi làm ở terminal.

```
<gốc workspace>/            ← huba.config.json trỏ vào đây (workspace_root)
├── AI/
│   ├── huba/                ← chính huba
│   └── <app khác>/
├── <app ở gốc>/
└── scripts/                ← tiện ích dùng chung, không phải app
```

Mỗi app tự mang sổ sách của nó — huba chỉ ĐỌC, không bao giờ ghi vào:

| tệp | để làm gì |
|---|---|
| `CLAUDE.md` | luật riêng của app ấy: cách chạy, cách deploy, cái gì cấm |
| `PLAN.md` | đang xây gì, còn nợ gì |
| `memory/active-context.md` | phiên trước dừng ở đâu, vì sao |
| `logs/devlog.sqlite` | nhật ký sự kiện có cấu trúc |

**Vì sao mỗi app một thư mục, không phải một repo lớn:** một phiên `claude` mở ở
đúng thư mục của app thì mọi đường dẫn tương đối nó gõ ra đều đúng, và cái tên
thư mục trở thành nhãn phân biệt phiên trên điện thoại (`[huba]`,
`[dwork]`). Mở tất cả ở gốc thì mọi phiên mang cùng một cái tên tự sinh
`projects-xx` — đúng cái tên **không phân biệt được gì**.

### Đặt gốc ở đâu — và một chỗ ĐỪNG đặt

Trên macOS, **đừng đặt gốc trong `~/Documents`, `~/Desktop` hay `~/Downloads`**.

Ba thư mục ấy bị TCC gác, và quyền đọc là một quyết định **cache theo tiến
trình** — nó chớp tắt giữa chừng ngay cả khi đã cấp Full Disk Access: `stat` qua,
`open` trả `EPERM`, vài chục giây sau lại bình thường. Đo trên chính máy này
2026-08-12: mỗi lần chớp mất 8–64 giây ngồi chờ, nhiều ngày liền. Thư mục nằm
ngoài vùng ấy thì không có quyền nào để mà mất.

Đi vòng qua symlink **không cứu được**: vẫn phải đọc một mục nằm trong
`~/Documents`. Dùng `~/projects` (hoặc bất cứ đâu ngoài ba thư mục kia).

---

## 2. Đường dẫn: động hết, không cứng chỗ nào

Đây là ứng dụng công khai, nên **không dòng mã nào được mang đường dẫn máy của
người viết**. Nhưng lý do sâu hơn chuyện gọn gàng: **gốc workspace chính là ranh
giới QUYỀN**.

Mọi thứ phải nằm trong cùng cái cây đã được cấp quyền thì mới chạy: macOS cấp
Automation/đĩa theo đường dẫn, `.claude/settings*.json` cấp thư mục làm việc theo
đường dẫn, và file allowlist của daemon cũng viết theo đường dẫn. Một tệp nằm
ngoài cây ấy không phải "hơi bất tiện" — nó **không có quyền**, và cái hỏng
thường im lặng: đọc ra rỗng, hàm trả `None`, tính năng tắt mà không ai báo.

Nên đường dẫn không được gõ cứng, mà phải **suy ra từ gốc**: đổi gốc là cả cây
quyền đi theo, không sót chỗ nào.

Mọi thứ bắt nguồn từ đúng một điểm và suy ra:

```
HUB_CONFIG (biến môi trường)
   └─► cfg.hub_home          = thư mục chứa huba.config.json
         └─► cfg.workspace_root = <hub_home>/../..   ← gốc ở mục 1
               └─► danh sách app · cwd của mọi /new · cây mã bảng sức khoẻ đem so
```

Kịch bản `.mjs` tự định vị bằng `import.meta.url`, không hỏi `$HOME`.

**Một ngoại lệ bắt buộc, và cách xử lý nó:** `com.dipgle.hubd.plist`.
launchd không hiểu `~` cũng không hiểu `$HOME` — plist **phải** mang đường tuyệt
đối. Nên tệp trong repo là **bản mẫu có dấu chỗ** (`__HOME__`,
`__INSTALL_DIR__`, `__HUB_CONFIG__`), và bạn sinh bản thật lúc cài bằng dòng
`sed` ghi ngay trong đầu tệp ấy.

📌 Cái giá đã trả cho việc gõ cứng một đường dẫn (2026-08-12): `runtime.rs` so
bản cài với một đường dẫn cố định. Đổi gốc ⟹ hàm trả `None` ⟹ bảng sức khoẻ
**thôi cảnh báo "daemon đang chạy mã hôm qua"** — mất đúng thứ duy nhất phát hiện
ra việc quên cài lại. Nó không kêu một tiếng nào.

---

## 3. Cấu hình: bí mật ở một chỗ, mọi thứ khác ở chỗ kia

| | ở đâu | vào git? |
|---|---|---|
| **giá trị bí mật** (mật khẩu, token) | `huba.env`, chmod 600 | ❌ `.gitignore` chặn |
| **tên biến** + hành vi | `huba.config.json` | ✅ |

Cấu hình bị đọc, bị chụp vào ảnh trạng thái, bị commit — nên nó chỉ mang **tên**
biến. huba cũng chỉ ghi TÊN khoá vào log, không bao giờ ghi giá trị.

Cách dễ nhất để điền:

```bash
./huba setup      # mở một trang ở 127.0.0.1, điền form, tự ghi huba.env chmod 600
./huba doctor     # kiểm tra THẬT: hỏi Telegram, tìm claude CLI, đọc thư mục app
```

Trang `setup` chạy trên chính máy cài huba, có vé một lần trong URL, **không bao
giờ đọc ngược giá trị đã lưu ra HTTP** (chỉ nói khoá ấy *đã có* hay *chưa*), và
tự đóng sau khi lưu. Ô để trống = giữ nguyên giá trị cũ.

---

## 4. Một bot Telegram cho **mỗi người** — đừng dùng chung

Đây là câu hỏi hay gặp nhất, và câu trả lời dứt khoát: **tự xin bot của bạn ở
@BotFather, khai vào `huba.env`.** Đừng nối vào bot của người khác.

Ba lý do, đều nằm trong chính thiết kế:

1. **Tin huba gửi đi mang chữ đang hiện trên màn phiên của bạn** — kể cả những
   dòng chưa kịp vào nhật ký. Bot dùng chung = màn hình của nhiều người đi qua
   một con bot.
2. **huba chỉ nhận lệnh từ một buồng chat** (`HUB_TELEGRAM_CHAT_ID`), và từ
   2026-08-14 đó là **cổng người duy nhất**. Bot dùng chung biến đúng một phép
   so ấy thành thứ duy nhất ngăn người lạ **điều khiển máy của bạn** — `/new`,
   `/type`, `/cmd` đều chạy bằng shell của chính bạn.
3. **Hạn mức và quyền sở hữu**: phiên `claude` tiêu hạn mức tài khoản của bạn.

---

## 5. Đường đi của một mệnh lệnh

```
điện thoại ──► Telegram ──► hubad (long-poll getUpdates) ──► claude CLI trên máy bạn
     ▲                              │
     └──── tin báo + nút bấm ◄──────┘
```

Cả hai chiều đều do **máy bạn gọi ra ngoài** — không mở cổng nào vào máy. Một
kênh, một cái mồm: phòng chat tfl5 chạy song song tới 2026-08-14 thì đóng.

Mọi cú bấm đều là phím tắt của một **route** đã có (`/session`, `/new`, `/ask`,
`/type`, `/key`, `/shot`, `/cmd`…), đi cùng hàng đợi và để lại cùng một vết trong
sổ. Không có nhánh xử lý riêng cho Telegram.

---

## 6. Nếu bạn dùng codetrail

huba đọc `logs/devlog.sqlite` của từng app. Nếu bạn dùng **codetrail** để dựng và
quản lý app, cấu trúc trên là thứ nó sinh ra sẵn — MCP router định địa chỉ app
**theo tên**, và mọi công cụ nhận thêm tham số `project`:

```jsonc
// .mcp.json ở gốc workspace
{ "mcpServers": { "project-agent": {
    "env": { "PROJECTS_ROOT": "<gốc workspace của bạn>" } } } }
```

Không dùng codetrail cũng chạy được: huba chỉ cần thư mục có `CLAUDE.md`; devlog
thiếu thì phần lịch sử sự kiện trống, không có gì hỏng.

---

## 7. Cần gì trên máy

- **macOS** — huba lái Terminal bằng AppleScript (`do script`); phần này không có
  bản Linux.
- **Quyền Automation** cho tiến trình chạy huba (macOS tự hỏi lần đầu). Không cần
  Accessibility: `System Events keystroke` bị từ chối thẳng, huba không dùng.
- **Claude CLI** (`claude`) trong `PATH`.
- **Rust** để build (`cargo build --release`).
- Một **bot Telegram** (@BotFather) + `chat_id` của buồng chat riêng với nó.
  Không còn "không bắt buộc": từ 2026-08-14 đây là kênh duy nhất.
