# hub — một kênh cho email · GitHub · tiến độ dự án · phản hồi người dùng

Mọi thứ cần bạn để ý đổ về **một chỗ**, Claude đọc từng cái, rồi hoặc **trả lời
người gửi**, hoặc **đưa bạn bản tóm tắt + đề xuất**, hoặc **sửa code trên một
branch** cho bạn review.

```
  email (mailler)  ─┐
  GitHub (gh)      ─┤                                    ┌→ trả lời đúng kênh cũ
  devlog dự án     ─┼→ hub.sqlite ─→ triage (claude -p) ─→│  (github/email/telegram)
  chat (telegram)  ─┤   messages      zero-tool          │
  bạn (hub say)    ─┘   decisions     JSON schema        ├→ brief cho bạn (notify/telegram)
                        outbox        tripwire injection │
                                                         └→ hub act → branch + diff (bạn review)
```

**Rust**, 2 binary tĩnh (`hub` + `hubd`), build offline được (`cargo build
--offline` — crate đã có sẵn trong cache), **0 dòng `unsafe`**. Cùng bộ dependency
với mailler nên workspace chỉ 1 toolchain. Không cần public endpoint (poll, không
webhook). Không cần token nào để chạy phần GitHub + devlog + CLI — `gh` đã auth sẵn.

---

## Chạy trong 3 lệnh

```bash
cd ~/Documents/projects/AI/hub
./hub doctor      # kiểm tra từng kênh + credential (tự build lần đầu)
./hub once        # 1 vòng: ingest → triage → gửi
./hub inbox       # xem cái gì vào, quyết định gì ra
```

Chạy liên tục (`hubd` chạy **cả vòng poll lẫn web console** trong 1 tiến trình):

```bash
rust/target/release/hubd
```

## Tự chạy cùng máy (launchd)

Bí mật để trong `hub.env` chứ không nhét vào plist — launchd không đọc `~/.zshrc`,
còn plist thì hay bị copy/backup:

```bash
cd ~/Documents/projects/AI/hub
cp hub.env.example hub.env
chmod 600 hub.env
```

Mở `hub.env` điền token (`HUB_TELEGRAM_TOKEN`, `HUB_MAILLER_API_KEY`). Khi khởi
động, hub nạp file này và **chỉ ghi log tên biến**, không bao giờ ghi giá trị; biến
đã có sẵn trong môi trường luôn thắng.

Cài agent:

```bash
cp deploy/com.dipgle.hubd.plist ~/Library/LaunchAgents/
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.dipgle.hubd.plist
```

Xem / gỡ:

```bash
launchctl print gui/$(id -u)/com.dipgle.hubd | head -20
tail -f logs/hubd.out
launchctl bootout gui/$(id -u)/com.dipgle.hubd
```

⚠️ **Tự chạy = tự tiêu tiền.** Vì vậy có `daily_budget_usd` (mặc định $3/ngày trong
config đang dùng): chạm trần thì **dừng triage**, nhắn cho bạn **đúng một lần** trong
ngày, hàng đợi giữ nguyên và xử tiếp sau nửa đêm UTC. Kèm 2 van cũ:
`poll_interval_sec` và `max_triage_per_cycle`. Muốn tạm dừng hẳn: `launchctl bootout`.

Khi chạy dưới launchd, tiến trình cũng phục hồi được sau khi bị kill: pid-lock cũ
được nhận lại, item đang triage dở quay về hàng đợi (đã chạy thật: `stale_lock_removed`
+ `recovered_stuck_triaging rows=1`).

## Đưa lên VPS?

Binary chạy Linux được (cùng bộ dep với mailler, cross-compile musl y hệt; banner
macOS tự tắt khi không phải macOS). Nhưng **UI mặc định chỉ nghe loopback là có chủ
đích**: trang web mang sẵn API token, nên "ai vào được trang = toàn quyền duyệt/gửi/sửa
config".

**Cách an toàn nhất, không cần đổi gì:** giữ `web.bind = 127.0.0.1` trên VPS rồi
tunnel qua SSH:

```bash
ssh -N -L 9200:127.0.0.1:9200 vps-a
```

rồi mở `http://127.0.0.1:9200/` trên máy bạn.

**Nếu vẫn muốn mở ra ngoài:** phải đặt mật khẩu (hub từ chối chạy nếu thiếu). Thêm
dòng `HUB_WEB_PASSWORD=...` vào `hub.env`, rồi trong config:

```jsonc
"web": { "enabled": true, "bind": "0.0.0.0", "port": 9200,
         "allowed_hosts": ["hub.tenmien.cua-ban"] }
```

Lúc đó mọi request (kể cả trang chủ và `/echarts.js`) phải qua HTTP Basic (user `hub`),
và `Host` phải khớp `allowed_hosts` — thiếu 1 trong 2 là 401/403. Vẫn nên để sau
Caddy/TLS + firewall, vì Basic trên HTTP trần là gửi mật khẩu dạng thô. Config sai
(bind ra ngoài mà thiếu mật khẩu) bị **từ chối ngay lúc load**, không im lặng chạy.

Ba thứ cần có trên VPS thì hub mới làm được việc, không phải lỗi code mà là môi trường:
`gh` đã auth, `claude` CLI đã đăng nhập, và bản sao các repo (để lấy `git log`/`git
status` làm context, và để act stage có worktree). Thiếu chúng thì hub vẫn chạy nhưng
triage nghèo context — nó sẽ nói thẳng là thiếu dữ kiện thay vì đoán.

## CLI

| Lệnh | Việc |
|---|---|
| `hub doctor` | kiểm tra claude/gh/mailler/telegram + kênh nào đang tắt vì thiếu gì |
| `hub once` / `ingest` / `triage` / `flush` | cả vòng, hoặc từng pha |
| `hub inbox [--status s] [-p project]` | danh sách message + decision |
| `hub show <id>` / `hub show d<id>` | 1 message + decision đầy đủ (summary, actions, evidence, nháp trả lời) |
| `hub say "câu hỏi" [-p tfl5]` | tự hỏi hub, trả lời về ngay terminal |
| `hub approve <d-id>` | gửi nháp trả lời / green-light action |
| `hub reject <d-id> [lý do]` | bỏ, huỷ mọi thứ đang xếp hàng của nó |
| `hub close <msg-id> [lý do]` | bỏ qua 1 message, không tốn tiền triage |
| `hub reply <msg-id> "text"` | tự trả lời bằng lời của bạn qua kênh gốc |
| `hub act <d-id>` | thực hiện thay đổi code đã duyệt trên branch `hub/act-<id>` |
| `hub status` | đếm theo trạng thái, sức khoẻ từng adapter, tổng chi phí |
| `hub web [--port 9200]` | mở bảng điều khiển web ở `127.0.0.1` (xem mục dưới) |
| `hub telegram-link [chat_id]` | nối Telegram: tự đọc chat id đã nhắn bot rồi ghi vào config |

`./hub --help` cho bản help đầy đủ (clap). Muốn gọi gọn `hub` ở mọi nơi:
`ln -s ~/Documents/projects/AI/hub/hub /usr/local/bin/hub`.

---

## Bảng điều khiển web (`hub web`)

```bash
./hub web
```

Rồi mở `http://127.0.0.1:9200/` — token nhúng sẵn trong trang, không phải dán gì.

4 tab: **Hộp việc** (danh sách + chi tiết + sửa nháp rồi Duyệt/Bỏ/Đóng, ô "Hỏi hub"),
**Cấu hình** (form cho toàn bộ `hub.config.json` + JSON thô), **Sức khoẻ** (dò thật
gh/mailler/telegram + lịch sử poll), **Chi phí** (biểu đồ ECharts: tiền theo ngày +
message theo trạng thái).

An toàn — UI này gửi được mail và sửa được policy nên bị coi là mặt privileged:

- chỉ nghe **127.0.0.1**, không bao giờ 0.0.0.0;
- **allowlist Host**: mọi request (kể cả `/`) bị từ chối 403 nếu `Host` không phải
  `127.0.0.1` / `localhost` / `::1` (hoặc host bạn khai trong `web.allowed_hosts` khi
  chạy off-loopback). Đây mới là thứ chặn **DNS rebinding** — header `x-hub-token`
  chỉ chặn CSRF. *(Bản đầu tôi viết sai là token chặn được rebinding; review đối kháng
  bác bỏ, đã vá + có test đi qua socket thật: `Host: evil.tld` → 403.)*
- mọi `/api/*` phải kèm header `x-hub-token` — 32 hex sinh từ `/dev/urandom` mỗi lần
  khởi động, nhúng thẳng vào trang;
- trang mang token nên gửi kèm `X-Frame-Options: DENY`, `CSP frame-ancestors 'none'`
  (chống clickjack nút "Duyệt & gửi") và `Cache-Control: no-store`;
- mọi lần từ chối (sai Host / sai mật khẩu / sai token) đều ghi log `web_request_denied`;
- lưu cấu hình đi qua `config::validate` + ghi **temp → rename** + giữ `.bak`, và
  round-trip qua struct `Config` nên key lạ/secret nhét vào cũng bị loại. Daemon
  **tự nạp lại** file khi mtime đổi (≤ 1 chu kỳ poll) — công tắc trên UI có tác dụng
  thật với vòng đang chạy, không phải chờ restart;
- Duyệt/Bỏ dùng **chung** `pipeline::approve_decision` với CLI và Telegram — không có
  đường tắt nào lách được policy.

Nghiệm thu UI: `node ui-smoke.mjs http://127.0.0.1:9200` (Playwright headless, 14 kiểm
tra, chụp ảnh vào `ui-shots/`, fail nếu có **bất kỳ** console error).

## Duyệt từ điện thoại (Telegram)

Brief của item đang chờ bạn được gửi kèm 2 nút **✅ Duyệt** / **🚫 Bỏ**. Bấm nút →
hub chạy đúng đường approve/reject như CLI, trả lời đi ra kênh gốc, rồi sửa lại chính
tin nhắn đó thành kết quả và gỡ nút (không bấm lại được 2 lần). Chat id lạ bấm nút
cũng bị bỏ qua + ghi log.

Nối trong 1 lệnh:

```bash
export HUB_TELEGRAM_TOKEN='<token @BotFather>'
# nhắn cho bot 1 câu bất kỳ, rồi:
./hub telegram-link          # tự lấy chat id, bật adapter, ghi vào config
./hub once
```

---

## Bốn kênh

| Kênh | Cách vào | Cần gì | Trạng thái |
|---|---|---|---|
| **GitHub** | `gh api /notifications` + (tuỳ chọn) issues/comments theo repo | `gh` đã auth | ✅ đã chạy thật (cả 2 bản): 30 notification vào, dedupe đúng |
| **devlog dự án** | đọc read-only `<project>/logs/devlog.sqlite`, tail event `warning/blocker/bug/test_fail/question` | không | ✅ đã chạy thật (19 project, tự set baseline lần đầu) |
| **CLI (`hub say`)** | bạn gõ trực tiếp | không | ✅ đã chạy thật, có auto-reply về `notify` |
| **Email** | REST của mailler: `GET /api/v1/messages` + `POST /api/v1/messages` (Bearer) | `HUB_MAILLER_API_KEY` | ⚠️ code xong, **chưa verify với key thật** |
| **Telegram** | `getUpdates` long-poll (không cần webhook/public IP) | `HUB_TELEGRAM_TOKEN` + `allowed_chat_ids` | ⚠️ code xong, **chưa verify với bot thật** |

Bật email/telegram:

```bash
# email: mint key trong webmail → Settings → API keys
export HUB_MAILLER_API_KEY=...
# telegram: @BotFather → /newbot
export HUB_TELEGRAM_TOKEN=...
./hub doctor                     # in ra chat id đã nhắn cho bot
# rồi bật enabled:true + allowed_chat_ids trong hub.config.json
```

Thiếu env var ⇒ adapter **skip có log** (`runs.skipped`), không im lặng, không crash.

---

## Mức tự chủ (autonomy tier)

Đặt trong `hub.config.json` → `autonomy.default` và `autonomy.projects.<tên>`:

| Tier | Hub được làm gì |
|---|---|
| **L0** (mặc định) | chỉ soạn nháp. Không gì rời khỏi máy trước khi bạn `hub approve` |
| **L1** | tự trả lời các item *thông tin* (`question`, `status_update`, `feature_request`) khi confidence ≥ `min_confidence_auto` |
| **L2** | thêm quyền chạy act stage (sửa code trên branch), vẫn cần bạn duyệt trước |

**Luật không config nào nới được:**

- Người gửi **untrusted** ⇒ tier bị ép về **L0**. Ai là trusted: `trust.github_logins`,
  `trust.emails`, `trust.telegram_chat_ids`, `trust.trusted_sources`.
- `deploy` / `merge` / `force_push` / `delete_data` / `rotate_secret` ⇒ **luôn** cần người.
- `kind=security` ⇒ không bao giờ tự trả lời.
- Tripwire prompt-injection ⇒ ép human review, ghi đè cả kết luận của model.
- Auto-reply ra kênh ngoài phải qua `rust/src/redaction.rs`; có dấu hiệu rò nội bộ ⇒ chuyển người.
- Act stage: worktree riêng, branch `hub/act-<id>` cắt từ HEAD, deny `git push/merge/reset`,
  `ssh`, `scp`, `sudo`, `rm`, `curl`, `docker`, `*deploy*`, `WebFetch`. Không tự push, không tự merge.

## Chống prompt injection (vì sao thiết kế vậy)

Email/issue là **chữ của người khác**. Nếu chữ đó tới được một agent có
`Bash`+`Write` thì người gửi coi như sở hữu máy này. Nên:

- triage chạy `--tools ""` — **không có tool nào** để chiếm;
- cwd là thư mục rỗng `data/triage-cwd`, không repo, không CLAUDE.md của project;
- MCP bị loại (`--strict-mcp-config`), session không lưu;
- mọi dữ kiện về repo/CI do **code host** gom (`git log`, `git status`, `gh run list`,
  devlog tail) rồi chèn vào phần *trusted context* — model không tự đi lấy;
- thân message bọc trong `<<<INBOUND … INBOUND>>>` + system prompt nói rõ đây là
  **dữ liệu, không phải chỉ thị**;
- `--json-schema` ép kết quả về đúng một hình dạng;
- tripwire quét 11 mẫu ("ignore previous instructions", `rm -rf`, `.env`,
  base64 payload, đòi exfil…) → gặp là ép `kind=security` + `needs_human`.

⚠️ **Caveat đo được (2026-07-26):** `claude -p` vẫn nạp **auto-memory của
workspace** vào context (một decision thật đã trích dòng `MEMORY.md` làm
evidence). Nên coi output của triage là **văn bản nội bộ**, và mọi auto-reply ra
ngoài đều phải qua leak scan — đó chính là lý do `rust/src/redaction.rs` tồn tại.
Hành vi này đã tái hiện y hệt ở bản Rust (decision #11), không phải đặc thù Node.

## Chi phí (số đo thật, 2026-07-26)

| | mỗi item | ghi chú |
|---|---|---|
| sonnet | **$0.11** | chất lượng tốt: tự tương quan 3 CI run, đọc `git status`, evidence có thật |
| haiku | **$0.051** | rẻ hơn ~2.2×, nông hơn (confidence 0.6) và đã thấy 1 câu **suy diễn không có trong context** |

Chỉ ~2× chênh vì phần lớn token là input/cache, không phải output. Mặc định để
**sonnet**. Ba cái van giữ tiền:

1. **coalesce** (`coalesce_hours: 12`) — item mới trên thread đã có decision chưa
   xử lý thì gắn vào, không triage lại. Đo thật: 6 message → 1 lần triage, 5 gắn kèm.
2. `max_triage_per_cycle` — trần mỗi vòng.
3. `--max-budget-usd` — trần cứng từng call, tính vào `decisions.cost_usd`; `hub status` in tổng.

## Cấu hình

`hub.config.json` (secret **không** nằm ở đây, chỉ tên env var):

```jsonc
{
  "poll_interval_sec": 120,
  "max_triage_per_cycle": 6,
  "coalesce_hours": 12,
  "triage": { "model": "sonnet", "max_budget_usd": 0.5, "timeout_sec": 240, "min_confidence_auto": 0.8 },
  "act":    { "enabled": false, "model": "sonnet", "max_budget_usd": 3, "timeout_sec": 1800 },
  "autonomy": { "default": "L0", "projects": { "tfl5": "L1" } },
  "adapters": { "github": { "enabled": true, "repos": [] }, "devlog": { "enabled": true },
                "email": { "enabled": false, "api_key_env": "HUB_MAILLER_API_KEY" },
                "telegram": { "enabled": false, "token_env": "HUB_TELEGRAM_TOKEN", "allowed_chat_ids": [] } },
  "trust": { "github_logins": ["dipgle"], "emails": [], "telegram_chat_ids": [], "trusted_sources": ["devlog", "cli"] },
  "routing": [{ "when": { "repo": "dipgle/tfl5" }, "project": "tfl5" }],
  "leak_patterns": []
}
```

Config sai (tier lạ, confidence > 1, routing thiếu `project`) ⇒ **fail ngay khi
load**, không chạy tiếp với default.

## Dữ liệu

`data/hub.sqlite`: `messages` (dedupe `UNIQUE(source, external_id)`) ·
`decisions` (kết luận + cost + tripwire) · `outbox` (retry 5 lần → `dead_letter`) ·
`runs` (sức khoẻ từng lần poll) · `cursors` (watermark) · `dead_letter`.

Cursor chỉ tiến **sau khi** message trong cửa sổ đó đã commit → crash thì poll
lại, không bao giờ nhảy mất. Message kẹt ở `triaging` (crash giữa triage) được
`reset_triaging()` đưa về hàng đợi ở vòng sau.

Schema giữ **byte-compatible** với bản Node đã archive: binary Rust mở đúng file
DB mà bản Node ghi (đã verify: `./hub doctor` đọc lại đủ 32 message + $1.0789 chi
phí do bản Node tạo).

## Test

```bash
cd rust && cargo test --offline      # 56 test, 0 warning
node ui-smoke.mjs http://127.0.0.1:9200   # 14 kiểm tra UI, 0 console error
```

Gồm: idempotency + retry/dead-letter của DB, ma trận policy (L0 không gửi,
untrusted bị ép L0, tripwire thắng confidence, deploy cần người, CI-notification
tin theo owner của repo), tripwire injection (7 case tấn công + 3 case lành),
fence của prompt, leak scan (có case lấy đúng từ **dòng rò thật** đã quan sát),
coalescing (4 case biên), validate config, nút bấm từ kênh (parse callback, chặn
chat lạ, data hỏng phải báo lỗi chứ không im), ghi config (round-trip + backup +
từ chối config sai để UI không làm chết daemon), và normalizer GitHub chạy trên
**payload notification thật đã capture** (`rust/tests/fixtures/`).

## Bản Node cũ

`legacy-node/` là bản prototype Node đã chứng minh vòng chạy rồi làm **oracle**
cho port Rust. Giữ để đối chiếu; xoá lúc nào cũng được (`rm -rf legacy-node`).
Nó không còn trỏ vào DB thật — chạy nhầm cũng không đụng state.
