# PLAN — Mặt tiền của hub trên tfl5

> **Bản sửa 2 · 2026-08-06.** Bản 1 (sáng nay) thiết kế một server axum thứ hai
> tự host trong hub (`portal.rs` + bảng `tickets` + magic link + TLS riêng).
> Hai ràng buộc mới của owner làm bản đó thừa gần hết:
>
> 1. **UI phải nằm trên tafalo5/tfl5**, không phải một cổng riêng của hub.
> 2. **`claude` CLI cài ở máy local** ⇒ bộ não không thể chuyển lên VPS.
>
> Bản 2 thay bằng: **tfl5 là điểm hẹn, hub là client kết nối ra.** Rẻ hơn nhiều
> vì tfl5 đã có sẵn gần hết những thứ bản 1 định tự viết.

---

## 1. Ràng buộc quyết định kiến trúc

| Ràng buộc | Hệ quả |
|---|---|
| `claude` CLI ở máy local | bộ não **phải** ở lại Mac; không deploy hubd lên VPS được |
| UI ở trên tfl5 (công khai) | trình duyệt không thể gọi thẳng `127.0.0.1` của Mac |
| hub không mở cổng vào máy (`PLAN.md` §Không làm) | không tunnel, không ngrok, không port forward |

**Chỉ còn đúng một hình dạng thoả cả ba: hub là *client kết nối ra*, tfl5 là chỗ
hai bên gặp nhau.** May mắn là đây đúng triết lý hub đã theo từ đầu — mọi adapter
hiện tại đều poll ra ngoài, không cái nào nhận kết nối vào.

```
  trình duyệt (bất kỳ đâu)
        │  HTTPS
        ▼
  ┌──────────────────────────────────────┐
  │ tfl5 trên VPS                        │
  │  · app "hub" — bundle FE tĩnh        │
  │  · /ws/chat — WebSocket có ACL       │
  │  · chat_message (Postgres)           │
  └──────────────────────────────────────┘
        ▲
        │  kết nối RA (outbound), hub là client
        │  không cổng nào mở trên Mac
  ┌─────┴────────────────────────────────┐
  │ hubd trên Mac  ← claude CLI ở đây    │
  │  pipeline · policy · leak_scan · act │
  └──────────────────────────────────────┘
```

---

## 2. tfl5 đã có sẵn gì — đã đọc mã, không phải phỏng đoán

Module chat của tfl5 (`crates/routes/src/ws_chat.rs`) không phải bản nháp:

| Có sẵn | Bằng chứng |
|---|---|
| `/ws/chat?app_tid=&room=` WebSocket | `ws_chat.rs:181 routes()` |
| **Xác thực TRƯỚC khi upgrade** — client chưa đăng nhập nhận 401/403, không bao giờ nhận 101 | `ws_chat.rs:7-11` |
| Quyền theo app: `require_app_perm(min_level)` ⇒ socket bị khoá trong đúng tenant | `ws_chat.rs:311-313` |
| **Ghi DB TRƯỚC khi broadcast** — ghi hỏng thì client nhận lỗi có cấu trúc, không mất im lặng | `ws_chat.rs:19-24` |
| `POST /app/chat/history` — scrollback cho người vào muộn / F5 | `ws_chat.rs:25` |
| Phân quyền tới mức **phòng**: `apps.acls['chat']['rooms'][<room>]['min_level']` + `scope_attrs` | `ws_chat.rs:28-40` |
| Fan-out **đa cell** qua `pg_notify('chat_message_v1')` trong cùng transaction | `ws_chat.rs:52-60` |
| Quản trị: xoá tin, liệt kê tin, đặt cấu hình phòng | `ws_chat.rs:87-98` |
| Host bundle FE tĩnh: `data/<cell>/<app_tid>/public/` phục vụ thẳng theo URL | `docs/landing-deploy.md` |

Nguyên tắc "ghi trước, phát sau, hỏng thì báo" của tfl5 **trùng khớp** với luật
"không lỗi im lặng" của hub. Hai hệ nói cùng một thứ tiếng.

---

## 3. Bản 1 bỏ được những gì

| Bản 1 định tự viết | Bản 2 | Vì sao |
|---|---|---|
| `portal.rs` — server axum thứ hai | **bỏ** | tfl5 đã là server công khai |
| bảng `tickets` | **bỏ** | `room` của chat = thread; `chat_message` là store |
| magic link + xác thực email | **bỏ** | tfl5 đã có tài khoản + phiên + ACL |
| TLS, reverse proxy, `allowed_hosts` | **bỏ** | tfl5 đã đứng sau hạ tầng đó |
| rate limit tự viết | **giữ, nhưng đổi chỗ** | vẫn cần, nhưng gác ở phía hub để chặn **chi phí triage**, không phải chặn HTTP |
| 3 màn HTML nhúng binary | **đổi** | thành bundle FE deploy lên app tfl5 |

Còn lại từ bản 1, vẫn cần nguyên: nhánh target trong `policy.rs`, arm trong
`outbound.rs`, thêm kênh vào `EXTERNAL_CHANNELS`, và **toàn bộ tầng trần tiền**.

---

## 4. hub nối vào tfl5 thế nào

**Không dùng `tfl5-sdk-rust`** — SDK là async/tokio, còn pipeline của hub cố ý
đồng bộ. Gọi thẳng REST/WS bằng thứ đã có (`reqwest` blocking + rustls).

Hai nấc, làm nấc 1 trước để chứng minh vòng lặp:

| Nấc | Cách | Độ trễ | Dep mới |
|---|---|---|---|
| **A. Poll** | `POST /app/chat/history` mỗi N giây, con trỏ theo `ts` | = chu kỳ poll | 0 — dùng `reqwest` sẵn có |
| **B. WebSocket** | client `tungstenite` (API đồng bộ, **không cần tokio**) | tức thời | `tungstenite` — **đã có trong cache offline** (0.21/0.24/0.28), nên `cargo build --offline` vẫn chạy |

Nấc A đủ để nghiệm thu toàn bộ đường đi. Nấc B là thứ tạo cảm giác "đang chat"
thật sự. Không nhảy thẳng B: nếu vòng lặp sai thì WS chỉ làm sai nhanh hơn.

**Xác thực:** hub cần một tài khoản tfl5 riêng ("service account") có quyền
Reader/Editor trên app `hub`. Đăng nhập `POST /login` → cookie `_token`. Phải có
đường phát hiện phiên hết hạn và đăng nhập lại — nếu không sẽ là một lỗi im lặng
kiểu mới. Mật khẩu đi qua env var, đúng luật #3 của hub.

---

## 5. "Giao tiếp như extension trên VSCode" — dịch ra cho cụ thể

Đây là phần cần chốt kỳ vọng, vì nó là chỗ dễ hứa quá tay nhất.

**Làm được, và gần như đã có sẵn:**

- Khung chat nhiều lượt, có lịch sử, F5 không mất — `chat_message` + history
- Người dùng gõ → hub trả lời trong cùng phòng
- **Nhiều lượt có trí nhớ:** hub đã lưu `session_id` trả về từ `claude -p`
  (`triage.rs:366`), nên nối lượt bằng `--resume` là đường đã mở sẵn
- Nút Duyệt/Bỏ ngay trong khung chat — đúng khuôn `ChannelCommand` đang dùng cho
  Telegram (`adapters/mod.rs:33`), adapter chỉ *parse*, pipeline mới hành động
- "hub đang gõ…" — một frame trạng thái gửi trước khi gọi model

**Làm được nhưng phải nói rõ giới hạn:**

- **Streaming từng chữ:** `claude -p --output-format json` hiện trả một cục.
  Muốn chữ chảy ra thì phải đổi sang `stream-json` và chuyển tiếp từng phần.
  Khả thi, nhưng là việc riêng, không miễn phí.

**Cố ý KHÔNG giống extension:**

- Extension trong VSCode là một agent **có tay** — đọc file, sửa file, chạy lệnh.
  Triage của hub chạy `--tools ""` **theo thiết kế** (`triage.rs:285-309`), vì
  nội dung người khác gửi tới không được phép điều khiển máy này.

⇒ Khung chat có **hai chế độ**, ranh giới rõ:

| Chế độ | Quyền | Ai bật |
|---|---|---|
| **Hỏi/đáp** (mặc định) | `--tools ""`, không tay, nhiều lượt | tự động |
| **Làm** (act) | worktree riêng, nhánh `hub/act-<id>`, 17 tool bị cấm, không push/merge | **owner bấm duyệt**, không bao giờ tự vào |

Đó không phải giới hạn tạm — đó là lý do hub an toàn khi cho người ngoài nhắn.

---

## 6. Tin cậy & chi phí — hai thứ tfl5 không lo hộ

**Tin cậy là hai tầng, đừng lẫn.** tfl5 trả lời "ai được vào phòng này"
(`require_app_perm`). hub vẫn phải tự trả lời "người này có được làm hub hành
động không". Người có quyền Reader trên app **không** đương nhiên là `trusted`
với hub. Cần map `tfl5 user_tid → trust` trong config, mặc định `untrusted` ⇒
rơi về L0 ⇒ chờ owner duyệt. Bất biến `policy.rs:200-211` giữ nguyên.

**Chat khuyến khích nhắn nhiều tin ngắn** — và mỗi tin là một lần gọi model
(~$0.11). Bắt buộc có, trước khi mở phòng cho người ngoài:

1. Gộp theo `thread_key = 'tfl5:<app>:<room>'` — cơ chế đã có
   (`pipeline.rs:251-263`), chỉ cần đặt đúng khoá
2. **Cửa sổ im lặng**: chờ ~10s sau tin cuối rồi mới gọi model một lần cho cả
   cụm — người ta gõ "chào" / "cho hỏi" / "về vụ deploy" thành 3 tin
3. Lọc tất định: tin quá ngắn, chỉ emoji, trùng nội dung → không triage
4. Trần ngày riêng cho kênh chat, tách khỏi trần chung $3

---

## 7. Lộ trình

Nguyên tắc: **chứng minh vòng lặp trước, làm đẹp sau.**

### F0 — hub nói được với tfl5 ✅ **XONG 2026-08-06, chạy thật trên tfl5 local**

| Hạng mục | Bằng chứng |
|---|---|
| build | `cargo build --release --offline` exit 0, **0 warning** |
| test | `cargo test --offline` exit 0 → **82/82** (72 cũ + 10 mới ở `tests/tfl5.rs`), 0 warning |
| dep mới | `tungstenite 0.24` — có sẵn trong cache, `--offline` vẫn build |
| hub → tfl5 | `hub tfl5-say` gửi qua `/ws/chat`, nhận receipt `cm-b4f03c…`; **kiểm chéo trong Postgres** thấy đúng row đó, không chỉ tin lời hub |
| tfl5 → hub | `alice_local` gõ một câu, `hub tfl5-tail` đọc ra đúng câu đó kèm đúng tên người gửi |
| hàng rào | tài khoản không có vai **bị chặn** ở bước upgrade — không vào được phòng |
| secret | mật khẩu chỉ nằm trong `hub.env` (chmod 600); log chỉ in **tên** biến, đúng luật #10 |

**Bug do chạy thật lộ ra và đã sửa:** tfl5 trả **HTTP 200** kèm
`{code:"access_denied"}` cho ca *đã đăng nhập nhưng không có vai*
(`error.rs:289` — cố ý tương thích w3c; chỉ ca *chưa đăng nhập* mới là 401).
hub chỉ hỏi "có phải 101 không" nên báo ra câu vô nghĩa `HTTP error: 200 OK`,
người vận hành không thể biết là thiếu ACL. Giờ `ws_connect_error` bóc envelope
và **gọi tên cách sửa** ("cấp Reader qua `/app/acl-set`"). Đã xác nhận envelope
thật đúng byte với cái test đang feed vào.

**Đã dựng trên tfl5 local (dev):** user `hubbot`, app `hub`
(`a-65dd60d3-624e-45a9-8fdf-62aa7d894d80`), phòng `hub`, `alice_local` làm
Reader. Tất cả tạo qua **API thật**, không SQL trực tiếp.

⚠ **Đây là nghiệm thu đường ống, KHÔNG phải nghiệm thu UC.** Chưa có UI nào —
người dùng thật chưa gõ vào màn hình thật lần nào. Nghiệm thu UC là việc của F2.

### F1 — Xương sống nuốt được nguồn `tfl5` ✅ **XONG 2026-08-06**

`ADAPTER_NAMES` +`tfl5` · nhánh target trong `policy.rs:239` · arm
`outbound.rs:56` · `tfl5` vào `EXTERNAL_CHANNELS` · `trust.tfl5_user_tids`.

**Nghiệm thu chạy thật:** alice gõ *"CI của tfl5 fail ở nhánh main…"* → triage
thật ($0.078) → decision #17 `tier L0 (trust=untrusted)` → brief → owner
`hub approve 17` → **câu trả lời nằm đúng trong phòng**, kiểm chéo Postgres.
Bản nháp từ chối đoán nguyên nhân và đòi log — đúng ý đồ.

### F2 — Bundle FE lên app tfl5 ✅ **XONG 2026-08-06**

`fe/index.html` (vanilla, tự chứa) → zip → `/app/bundle/upload` →
`/app/bundle/activate` → phục vụ tại
`http://<app_tid>.test.localhost:8090`. `fe-smoke.mjs` **15/15, 0 console error**.

**Nghiệm thu UC thật (`fe-watch.mjs`):** trang mở sẵn, socket mở, owner duyệt ở
CLI → **câu trả lời tự hiện trên trang, không tải lại**. Ảnh: `ui-shots/uc-live-reply.png`.

**Bug do chạy UI thật lộ ra:** F5 xong là bị bắt đăng nhập lại dù cookie `_token`
còn sống — FE không thử khôi phục phiên. Đã thêm `restore()` gọi `POST /user`
lúc tải trang; bundle v2.

### F3 — Chống đốt tiền cho kênh chat ✅ **XONG 2026-08-06**

Ba lớp, đều có test:
- **Cửa sổ im lặng** (`silence_window_sec`, mặc định 10s) — giữ tin còn mới để
  cả cụm thành **một** lần gọi. Tách thành hàm thuần `select_new` để test được
  mà không phải đua với server thật.
- **Lọc tất định** (`min_chars`, mặc định 3) — "ok"/"👍" không phải câu hỏi.
- **Trần ngày theo nguồn** (`source_daily_budget_usd`) — một phòng ồn không được
  ăn hết ngân sách ngày trước khi việc khác được nhìn tới.

*Chưa làm:* streaming từng chữ + nối lượt `--resume`. Đó là hai việc riêng.

### F4 — Hàng rào cho người ngoài ✅ (phần kiểm được) **2026-08-06**

`trust` hai tầng có test: vào được phòng ≠ được hub tin. Người lạ → `untrusted`
→ L0 → luôn chờ owner. Tripwire ghi đè cả tier L2 + model tự tin.

⚠ **Chưa chạy được:** gửi payload tiêm lệnh **qua khung chat thật**. Hook lệnh của
workspace từ chối cho một chuỗi tấn công đi vào kênh thật — đúng đắn, và tôi
không lách. Nhánh đó được phủ bằng test tích hợp (`tests/policy.rs`) + 7 case mẫu
(`tests/triage.rs`), **không phải bằng một lần chạy thật**. Nói đúng như vậy.

### F5 — Chế độ "Làm" ✅ **XONG 2026-08-06 — act stage chạy thật lần đầu**

Dựng `AI/hub-act-demo` (repo nháp, cô lập, không deploy đi đâu) với hai lỗi
thật: `median()` sai khi số phần tử chẵn, `mean([])` ném `ZeroDivisionError`.

`hub say` → triage → `hub approve 36` → `hub act 36`.

| Hạng mục | Kết quả |
|---|---|
| worktree | `data/worktrees/hub-act-demo-act-36`, nhánh `hub/act-36` off HEAD |
| chi phí | $0.2374 (trần đặt $1 cho lần đầu) |
| diff | `src/stats.py` +8/−1, `test_stats.py` +12/−1 — tối thiểu, không refactor lan man |
| RED→GREEN | agent báo test đỏ trước, xanh sau. **Tôi tự chạy lại**: `python3 -m pytest -q` → **4 passed** |
| hàng rào | `main` **sạch và vẫn còn lỗi** (chưa merge) · không remote · chưa từng push |

Bài học: đừng tin báo cáo của agent. Nó nói "4 passed" và đúng là 4 passed —
nhưng chỉ vì tôi tự chạy lại mới biết.

### Điều khiển từ trong phòng chat ✅ **2026-08-06**

`/approve <id>` · `/reject <id> [lý do]` · `/help`. Lệnh **không đi qua model**
(0 đồng) và **không trở thành message**.

**Ranh giới an toàn:** chỉ user_tid nằm trong `trust.tfl5_user_tids` mới ra lệnh
được — ở trong phòng là quyết định của tfl5, duyệt một câu trả lời ra ngoài là
quyết định của owner. Danh sách rỗng ⇒ **không ai** có quyền (fail-closed).
Người lạ gõ `/approve 12` thì đó chỉ là một câu chữ, đi qua triage như mọi tin
khác — không bị nuốt im lặng.

**`/act` cố ý bị từ chối trong chat.** Nó ghi code và có thể chạy hàng chục phút;
kích hoạt từ bàn phím điện thoại là sai chỗ, và để nó chẹn vòng poll còn tệ hơn.
hub trả lời kèm đúng lệnh cần gõ ở terminal.

### Trí nhớ hội thoại ✅ **2026-08-06** · Streaming ❌ **chặn ở phía tfl5**

`source_thread_memory_hours` bật `--resume` cho từng nguồn. **Chạy thật:** alice
nói tên ở lượt A, hỏi lại ở lượt B → hub nhớ đúng "Minh / báo cáo thống kê của
sdvi", cùng `session_id`, chi phí tụt $0.066 → $0.035 nhờ dùng lại context.

Ba điều học được, đều tốn công mới biết:

1. **`--no-session-persistence` và `--resume` loại trừ nhau.** Lượt ĐẦU phải bỏ
   cờ persistence, nếu không chẳng có phiên nào để nối — nên có ba trạng thái
   (`Off` / `Start` / `Resume`), không phải hai.
2. **Phiên đã resume giữ nguyên system prompt lúc tạo.** Sửa `SYSTEM_PROMPT` chỉ
   ăn vào hội thoại MỚI. Phát hiện ra vì bản sửa prompt không đổi được hành vi
   cho tới khi ép bắt đầu phiên sạch.
3. **Phiên mất phải thoái lui, không được chết.** Session cũ (tạo từ thời còn
   `--no-session-persistence`) làm triage fail hẳn. Giờ bắt đúng lỗi "no
   conversation found", cảnh báo, chạy lại không `--resume`.

Bảo mật: thread nào từng dính tripwire thì **không bao giờ** được resume — kẻ
tấn công lọt một lượt không được phép xây tiếp trên context owner đã tin.

**Streaming từng chữ: KHÔNG làm được.** tfl5 chat chỉ có `history`, `delete`,
`list`, `set-room-config` và `/ws/chat` — **không có endpoint sửa tin nhắn**
(`ws_chat.rs:183-195`). Streaming vào phòng sẽ phải spam nhiều tin rời rạc.
Muốn có thì cần tfl5 thêm edit-message ⇒ phải bàn với session tfl5, không tự sửa.

### Socket thường trực ✅ **XONG 2026-08-06 — toàn bộ ở phía hub**

**Không sửa một dòng nào của tfl5.** `/ws/chat` đã push sẵn: mỗi socket subscribe
`ChatBus` theo `(app_tid, room)` và mọi tin trong phòng được fan-out tới mọi
socket đang mở, kể cả tin từ cell khác qua `pg_notify` (`ws_chat.rs:419-446`).
Thiếu sót nằm ở hub: nó mở socket rồi đóng ngay, và **đọc bằng poll HTTP**.

`live.rs` giữ socket mở trong `hubd` + `Waker` để vòng lặp không ngủ hết chu kỳ.

**Chạy thật, cố ý đặt `poll_interval_sec = 600` để không thể ăn may:**

| Lúc | Việc |
|---|---|
| 10:07:37 | `tfl5_live_connected` — socket mở |
| 10:07:56 | alice gõ một câu |
| 10:08:08 | `tfl5_live_ingested` — **12s** (= cửa sổ im lặng 10s + xử lý) |
| 10:08:57 | vòng 1 xong |
| **10:08:58** | **vòng 2 bắt đầu — 1 giây sau, không phải 600** |

**Về NAT, ghi lại để khỏi bàn lại:** không có gì để "đục". Hole punching
(STUN/TURN/ICE) sinh ra cho hai máy **đều** sau NAT muốn nối trực tiếp; tfl5 đã
có IP công khai nên đầu kia chỉ việc gọi tới. Một kết nối đi ra rồi giữ mở
**chính là** cái lỗ — NAT/CGNAT/firewall đều cho gói về trên đúng kết nối đó, và
không cần cấu hình gì ở phía người dùng. tfl5 có webhook (`hooks.rs`) nhưng nó
gọi tới một URL công khai ⇒ quay lại đúng bài toán cũ, lần này bắt máy owner phơi
cổng ra Internet. Đường hầm duy nhất trong tfl5 là WireGuard **giữa hai host
Postgres**, không liên quan.

**Poller KHÔNG bị bỏ.** Socket là đường *nhanh*, poller là đường *chắc*: rớt
mạng, tắt tiến trình, hay burst còn trong bộ đệm thì vòng poll sau vẫn lấy được
theo cursor. `UNIQUE(source, external_id)` khiến trùng lặp thành miễn phí.

**Nút thắt thật vẫn là người duyệt.** Bỏ polling cắt được ≤600s, nhưng người lạ
luôn ở L0 nên vẫn phải chờ owner bấm. Socket thường trực chỉ thực sự đáng giá khi
nâng tier cho người tin cậy.

**Telegram lùi xuống sau F3** (kênh báo cho owner khi rời máy). **mailler** giờ
không còn là hạ tầng bắt buộc nữa (bản 1 cần nó cho magic link) — quay về hàng
đợi bình thường.

---

## 8. Chưa xác minh — phải kiểm khi bắt tay làm

Ghi ra để không ai (kể cả tôi) coi là đã biết:

- [ ] Hợp đồng chính xác của `POST /app/chat/history` (mới đọc doc-comment, chưa đọc handler)
- [ ] Envelope đầy đủ của frame WS (`welcome` / `msg` / `pong` / `error`)
- [ ] Tạo service account cho hub thế nào, và cấp `min_level` nào trên phòng
- [ ] `app_tid` dùng cái nào — tạo app mới hay dùng app sẵn có
- [ ] Đường deploy bundle FE lên tfl5 **prod** (khác với chạy local)
- [ ] tfl5 prod đang chạy ở đâu, còn sống không, phiên bản nào có `ws_chat`
- [ ] Phiên `_token` sống bao lâu → hub phải đăng nhập lại theo nhịp nào

## 9. Rủi ro phối hợp

**`AI/tfl5` đang có session khác sửa** (file đổi lúc 12:34–12:50 hôm nay). Phương
án này cố ý **không cần sửa mã tfl5** — chỉ cần cấu hình (app, phòng, tài khoản,
ACL) và một bundle FE. Nếu tới lúc làm mà phát hiện thiếu một endpoint phía tfl5,
đó là việc **phải bàn với session tfl5**, không tự sửa xen ngang.
