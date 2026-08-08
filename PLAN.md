# PLAN — hub

> **2026-08-06 — hướng đã chốt: hai mặt bổ trợ, một xương sống.** hub không phải
> "công cụ riêng của owner" *hoặc* "kênh cho người dùng" — mặt tiền cho người
> ngoài là **nguồn vào thứ 5 + kênh ra thứ 5** trên đúng pipeline đang chạy.
> Phương án rộng + lộ trình 6 phase: **`PLAN-portal.md`**. Thứ tự ưu tiên ở mục
> "Việc tiếp theo" bên dưới **đã bị đảo**: mailler lên đầu (là hạ tầng
> magic-link của portal), Telegram lùi xuống sau P4.

## Mục tiêu

Một kênh duy nhất để quản lý tiến trình: email, GitHub, tiến độ dự án và phản hồi
người dùng đổ vào; Claude CLI xử lý; kết quả là trả lời người gửi, brief cho chủ
dự án, hoặc thay đổi code trên branch.

## Trạng thái 2026-07-27 (tối) — Phase 3.1: VÁ SAU REVIEW ĐỐI KHÁNG

Chạy 1 workflow review 4 chiều (web auth / ghi config từ UI / secrets / tiền + vòng
lặp), mỗi phát hiện bị 1 agent khác **cố bác bỏ**: 37 agent, ~2.8M token, 13 lỗi sống
sót. 7 mục xếp hạng "chặn trước khi bật auto-start" đã vá hết:

| # | Lỗi thật | Vá |
|---|---|---|
| 1 | **Config sửa trên UI không tới vòng lặp** → mọi công tắc an toàn là giả | `hubd` theo dõi mtime, `config::load` lại mỗi vòng; giữ config cũ + log nếu file hỏng. **Chạy thật:** `config_reloaded` sau 15s với `autonomy L1` + `budget 0.25` |
| 2 | **Không kiểm `Host` → DNS rebinding** chiếm trọn console (token nằm trong trang) | allowlist Host cho mọi route + `web.allowed_hosts`; 5 test đi qua socket thật (`Host: evil.tld` → 403) |
| 3 | Triage lỗi **vẫn tốn tiền** nhưng không ghi `decisions` → trần ngày mù | ghi row `status='failed'` kèm `cost_usd` |
| 4 | `claim_new_messages` chỉ SELECT → 2 vòng (daemon + `hub once`/UI) **triage đôi, gửi đôi** | `UPDATE … RETURNING` nguyên tử + cột `claimed_at`; `reset_triaging` chỉ thu hồi row quá hạn |
| 5 | **Token Telegram nằm trong URL** → reqwest ghi vào log/`runs.err`/`dead_letter`/`/api/doctor` | `.without_url()` |
| 6 | `validate()` chạy trước khi nạp `hub.env` → bind off-loopback + mật khẩu trong hub.env = **crash-loop exit 70** | nạp `hub.env` **bên trong** `config::load` trước `validate` |
| 7 | Trần ngày kiểm 1 lần/vòng → batch 6 call vượt 100% trần; `hub say` không kiểm | chuyển kiểm tra vào `process_message` (mọi đường đều đi qua) |

Thêm (không chặn nhưng đã vá): 1 transaction cho decision+outbox+status (kill giữa
chừng không còn gửi 2 lần), pid-lock `O_EXCL` + `pid_alive` fail-closed, handler config
ra khỏi async worker (console không treo khi đang chạy cycle), UI: bỏ ghi đè
`trust.telegram_chat_ids`, `leak_patterns` tách theo dòng (regex có dấu phẩy), "Dùng
JSON thô" áp cho **toàn bộ** form, header chống clickjack/cache, log mọi lần từ chối.

**Test:** `cargo test` **72/72**, 0 warning · Playwright **14/14**, 0 console error.

## Trạng thái 2026-07-27 (chiều) — Phase 3: CHẠY KHÔNG NGƯỜI TRÔNG

| Hạng mục | Bằng chứng |
|---|---|
| 1 agent làm 2 việc | `hubd` chạy vòng poll **và** web console (`web.enabled`), đã chạy thật: log `hubd_started` + `web_ui_started`, `curl /` = 200, poll ra 6 notification mới |
| launchd | `deploy/com.dipgle.hubd.plist` trỏ binary Rust, `ProcessType=Background`, `LowPriorityIO`, hướng dẫn `bootstrap`/`bootout` |
| secret cho daemon | `hub.env` (chmod 600) nạp lúc khởi động, **chỉ log tên biến**; env thật luôn thắng; file `.gitignore` |
| trần tiền/ngày | `daily_budget_usd` (đang để $3): chạm trần → dừng triage, cảnh báo **1 lần/ngày**, hàng đợi giữ nguyên. Test nhánh chặn có thật (`autostart.rs`) |
| phục hồi sau kill | chạy thật: `stale_lock_removed` + `recovered_stuck_triaging rows=1`, spend không tăng |
| hàng rào VPS | bind ngoài loopback **bắt buộc** có `HUB_WEB_PASSWORD` → nếu thiếu, `validate()` từ chối **lúc load** và `serve()` bail với hướng dẫn SSH tunnel; có mật khẩu thì mọi request (kể cả `/`) qua HTTP Basic |
| test | `cargo test` **65/65**, 0 warning; Playwright **14/14**, 0 console error |

Sai lầm của tôi trong lúc nghiệm thu (ghi lại để khỏi lặp): tôi hạ trần xuống $0.5 để
"chứng minh nó chặn", nhưng spend hôm nay = $0 (toàn bộ $1.20 là của 26-07) nên đúng
ra không chặn — guard không sai, phép thử sai. Nhánh chặn giờ được phủ bằng test thật
với DB tạm.

## Trạng thái 2026-07-27 — Phase 2: MẶT GIAO TIẾP (web UI + nút Telegram)

| Hạng mục | Bằng chứng |
|---|---|
| build | `cargo build --release --offline` exit 0, **0 warning**, binary 9.2M (nhúng cả `ui.html` + echarts) |
| test | `cargo test --offline` exit 0 → **56/56**, 0 warning (thêm suite `channel_commands`) |
| UI test | `node ui-smoke.mjs` → **14/14**, **0 console error**, ảnh ở `ui-shots/` |
| web chạy thật | `hub web --port 9247`: `/` = 200, `/api/inbox` không token = **401**, có token = dữ liệu thật |
| config qua UI | đổi `coalesce_hours` trên form → Lưu → **đọc lại từ đĩa đúng giá trị** (test tự đảo lại) |
| nút Telegram | code + unit test (parse callback, chặn chat lạ, data hỏng báo lỗi); **chưa bấm thật** vì chưa có token |

Bug do chạy thật lộ ra và đã sửa: `std::fs::read("/dev/urandom")` đọc stream vô
hạn → server treo trước cả khi in địa chỉ (giờ dùng `read_exact` 16 byte).

## Trạng thái 2026-07-26 (chiều) — Phase 1: PORT SANG RUST xong, parity đạt

Bản Rust là canonical (`rust/`, binary `hub` + `hubd`); bản Node đã archive vào
`legacy-node/` sau khi qua parity gate.

| Hạng mục | Bằng chứng |
|---|---|
| build | `cargo build --release --offline` exit 0, **0 warning**, `hub` 7.2M + `hubd` 6.7M |
| test | `cargo test --offline` exit 0 → **50/50** (6 test binary), 0 warning |
| schema | binary Rust mở đúng `data/hub.sqlite` do Node ghi: `doctor` đọc lại 32 message, spend $1.0789 |
| ingest thật | `hub ingest` poll GitHub thật qua `gh` (0 new vì cursor đã ở hiện tại — đúng) |
| coalesce thật | message #21 gắn vào decision #7 cùng thread, 0 chi phí |
| triage thật | `hub say -p tfl5` → `claude -p` thật → decision #11 (`status_update`, evidence có thật) → policy → outbox |
| an toàn | `unsafe`: 0 dòng; secret chỉ qua env; leak-scan chạy trước mọi auto-reply ra ngoài |

Lý do port (ghi lại để khỏi tranh luận lại): `node:sqlite` vẫn **experimental**
(cảnh báo mỗi lần chạy, API có thể đổi giữa các bản Node) mà đây là service 24/7
giữ sổ sách; mọi service sống lâu trong workspace đều Rust; cùng bộ dep với
mailler (`rusqlite` bundled + `reqwest` rustls) nên deploy/musl cross-compile
dùng lại được nguyên pattern có sẵn.

## Trạng thái 2026-07-26 (sáng) — Phase 0 walking skeleton: XONG và đã chạy thật

Đã chạy end-to-end trên dữ liệu thật, không mock:

| Khâu | Bằng chứng thật |
|---|---|
| ingest GitHub | 30 notification thật vào DB; poll lần 2 → 0 new (dedupe đúng) |
| ingest devlog | 18 project được phát hiện, baseline tự set, `sdvi`/`tfl5` (devlog rỗng, chưa có bảng `events`) được ghi nhận là "not initialized" chứ không phải lỗi |
| triage | 8 decision thật qua `claude -p`, kind/severity/evidence có thật (tự tương quan 3 CI run của tfl5 + `git status` + commit) |
| policy | tất cả → `await_human` đúng vì tier L0; 1 case ghi rõ `tier L0 (trust=untrusted)` |
| coalesce | 6 message → 1 triage, 5 gắn vào decision cũ (tiết kiệm ~$0.5) |
| outbox | 6 brief gửi qua kênh `notify` (file log + banner macOS), 0 fail |
| auto-reply (L1) | `hub say` ở tier L1 → `auto_reply → notify:local`, câu trả lời có nội dung dùng được |
| test | 47/47 xanh, `node --test`, exit 0 |

Chưa verify (thiếu credential, **không** claim là chạy được):

- **email (mailler)**: cần `HUB_MAILLER_API_KEY`. Endpoint + Bearer đã đọc từ
  source mailler (`crates/server/src/main.rs:660`, auth Bearer ở 490-503), nhưng
  chưa gọi thật lần nào.
- **telegram**: cần `HUB_TELEGRAM_TOKEN`. `getUpdates`/`sendMessage` chưa gọi thật.
- **act stage** (`hub act`): code + hàng rào tool đã có, `act.enabled=false`,
  **chưa chạy lần nào** trên project thật.

## Việc tiếp theo (theo thứ tự giá trị)

1. **Bật một kênh người-thật.** Telegram (nhanh nhất, không cần public endpoint)
   hoặc email qua mailler. Sau khi có token: `hub doctor` → lấy chat id → bật
   `enabled` + `allowed_chat_ids` → chạy `hub once` → xác nhận nhận/gửi thật.
2. **Nghiệm thu act stage một lần** trên một item nhỏ, thật: `act.enabled=true`,
   `hub approve` → `hub act` → đọc diff → tự quyết push/PR. Chưa chạy thì chưa
   được nói là hoạt động.
3. **Nâng tier có kiểm soát.** Cho 1 project lên L1, quan sát 1 tuần bằng
   `hub status` + `logs/notify.log`, xem có auto-reply nào lệch không.
4. **GitHub Issues làm kênh feedback người dùng.** Thêm repo vào
   `adapters.github.repos` → issue + comment vào hub, trả lời bằng
   `hub approve` (đã có đường comment qua `gh api`).
5. **Chi phí:** nếu lượng vào tăng, thêm bộ lọc tất định trước triage (bỏ
   `ci_activity` trùng branch, gộp theo repo+ngày) — coalesce hiện chỉ gộp theo
   thread.
6. **Đóng sổ định kỳ:** hub tự ghi `log_event` vào devlog của chính nó
   (`AI/hub/logs/devlog.sqlite`) để mọi vòng chạy có sổ sách.

## Không làm (quyết định có chủ ý)

- **Không webhook** ở Phase 0: poll `gh` + IMAP/REST đủ, không mở cổng vào máy.
- **Không dependency npm**: `node:sqlite` + `fetch` built-in là đủ; cài đặt bằng
  `git clone` là xong.
- **Không cho triage tool nào**: mọi dữ kiện do host gom rồi chèn vào prompt.
  Muốn agent có tay thì phải qua act stage + người duyệt.
- **Không tự deploy/merge/push**: dù tier nào.
