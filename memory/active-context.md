# active context — hub

## 🧹 2026-08-08 — hub giờ CHỈ là kênh quản lý phiên Claude CLI

**Đã gỡ hẳn nhánh hộp thư** (commit `88398ca`, **−2.488 dòng**). Hà: *"rõ ràng loại
bỏ github ra khỏi flow rồi mà, chỉ đơn thuần tạo kênh quản lý thôi"*.

Bỏ: 4 adapter `github`/`telegram`/`devlog`/`email` + toàn bộ wiring
(`ADAPTER_NAMES`, `poll_adapter`, `adapter_enabled`, các `*Cfg`,
`Trust.{github_logins,emails,telegram_chat_ids}`, mục kênh trong doctor/web/portal),
nhánh gửi outbound của 3 kênh, nhánh callback Telegram, khối CI-context trong
`triage`, nhánh trust theo github/email/telegram trong `policy`, và test tương ứng.

**Trước khi xoá** — hub chưa từng có git: `git init` + commit **`525beeb`** chụp
nguyên trạng, nhánh **`backup/inbox-adapters`**, bản sao `/tmp/adapters-backup`.
Hook kiểm duyệt đòi đủ ba thứ đó; tôi làm theo chứ không lách.

**Còn lại:** một kênh `tfl5`, và ba việc — **danh sách phiên · xem luồng như ngồi
máy · đóng sổ bàn giao**. Hộp việc + `/approve` vẫn chạy trên xương sống
`messages/decisions/outbox` cũ (Hà chưa chốt có bỏ nữa không).

⚠ **Hai hồi quy do chính việc gỡ, bắt được trước khi commit:**
1. `known_projects` bỏ devlog adapter → chuyển sang quét thư mục, và bản đầu vơ
   cả `logs`, `memory`, `scripts`, `crates`. Một tên trong danh sách này là một
   tên **`/project` sẽ chấp nhận** — pin trỏ vào thư mục không có việc. Nay chỉ
   nhận thư mục có dấu hiệu dự án (`CLAUDE.md`/`.git`/`Cargo.toml`/`package.json`/
   `logs/devlog.sqlite`).
2. `fe-board-uc` đòi tab Sức khoẻ có **≥3 chip kênh** — assert của sản phẩm cũ.
   Nay đòi đúng 2 chip còn lại **và** đòi **không** còn github/telegram/devlog,
   để lần sau ai bật lại lén thì test đỏ.

**Nghiệm thu sau khi gỡ:** `cargo test` **161** · clippy **0** · build 0 warning ·
`fe-sessions-uc` 9/9 · `fe-stream-uc` 13/13 · `fe-smoke` 15/15 · `fe-board-uc`
43/43 · `hubd` pid 35849 · `doctor` chỉ còn kênh `tfl5`.


**Cập nhật:** 2026-08-08 · **Một mặt tiền duy nhất trên tfl5**: hộp việc +
trao đổi + sức khoẻ + chi phí + cấu hình, đọc-và-ghi.
Phương án + bằng chứng: `PLAN-portal.md`. Lịch sử cũ ở `PLAN.md`.

## 🎯 2026-08-08 — ĐỔI HƯỚNG: hub là BE quản lý phiên Claude CLI, không phải hộp thư

Hà chốt: *"hub là BE quản lý các phiên làm việc claude cli, ui cung cấp giao diện
quản lý giống như tôi đang làm trên máy qua terminal"* — **theo dõi và xử lý việc
từ ĐIỆN THOẠI**, claude chạy ở máy local. Trước đó Hà nhận xét *"có vẻ bạn đang
làm chưa đúng hoặc quá phức tạp"*, và số liệu cho thấy Hà đúng: 117/180 tin trong
hộp việc là CI của GitHub (65%), $5.89/$9.12 chi cho nhánh hộp-thư,
email+telegram 616 dòng **chưa gọi thật lần nào**, còn "Làm" (act) — thứ thật sự
là *xử lý việc* — mới chạy đúng **1** lần.

**Bước 1 (chỉ đọc) XONG, đã chạy thật:** `rust/src/sessions.rs` + lệnh
`hub sessions [--json]` → **14 phiên đang sống trên 3 tài khoản** (acc1 9 · acc2 2
· acc3 3), sắp theo vừa-động, kèm lượt hội thoại cuối của từng phiên.

Bốn điều đo được quyết định thiết kế (đừng phát hiện lại):
1. **`claude agents --json` không cần TTY** nhưng **phân mảnh theo tài khoản** ⇒
   gọi một lần mỗi tài khoản. Tài khoản chọn bằng `CLAUDE_CONFIG_DIR`, và tài
   khoản **mặc định chọn bằng biến VẮNG MẶT** — trỏ nó vào `~/.claude` sẽ báo
   "not logged in". Vì thế `exec::RunOpts` có `env: Vec<(String, Option<String>)>`
   với `None` = **xoá biến**.
2. **Transcript dùng chung**: `~/.claude/projects/<slug>/<id>.jsonl` (acc2/acc3
   symlink `projects` về acc1). Đọc **đuôi 256KB**, không đọc cả file (có file 12MB).
3. **Đọc dòng cuối là SAI**: 5/14 phiên kết thúc bằng bản ghi sổ sách
   (`pr-link`/`attachment`/`system`/`last-prompt`) ⇒ phải đi ngược tìm
   `assistant`/`user`. Lượt chỉ có tool → hiện `[dùng Bash]` chứ không để trống.
4. **`cwd` vô dụng làm nhãn dự án**: cả 14 phiên đều mở từ thư mục cha
   `~/Documents/projects`. Thứ có nghĩa là **tên phiên** ("Tiếp tục dwork",
   "fix-deploy-verify-hash"); `projects-87` là tên tự sinh.

### 🔒 Lớp quét rò rỉ chỉ biết tiếng Anh — phát hiện nhờ chạy thật

Lần chạy đầu của `hub sessions` in ra một phiên **nói thẳng chuỗi đăng nhập bằng
tiếng Việt**, mà `redaction::leak_scan` báo **0 rủi ro**: mẫu `credential_word`
chỉ có từ tiếng Anh, trong khi cả workspace này làm việc bằng tiếng Việt. Đã thêm
mẫu tiếng Việt (kể cả bản không dấu) vào `redaction.rs` — **vá này ăn cho mọi
đường ra ngoài**, không riêng sessions.

`sessions.rs` chặn phần xem trước **ngay tại nguồn** (mọi mặt tiền đọc chung
struct đó), và **chỉ dùng 4 nhãn nguy hiểm nhất** chứ không dùng cả `leak_scan`:
mẫu `/Users/…`, IP, "blocker" dính gần hết dòng dev ⇒ dùng cả bộ thì màn hình
trống trơn và Hà sẽ học cách phớt lờ nó. **Đo trên dữ liệu thật: ẩn 1/14, xem
trước được 12, còn sót 0.**

Kèm theo: **log mức info chuyển từ stdout sang stderr** — nó phá `--json` ngay
lần đầu đem đi pipe. Dữ liệu ra stdout, chẩn đoán ra stderr.

**Tài liệu luồng, đã lên bundle:** `fe/flow.html` → **bundle v29**, xem ở
`http://<app_tid>.test.localhost:8090/flow.html` (đường bundle là public, không
tính quota). 4 hình SVG tự vẽ: ràng buộc kiến trúc · thu thập 3 tài khoản ·
nhật ký → cổng quét → điện thoại · ranh giới điều khiển. Bản artifact:
`claude.ai/code/artifact/c8839c27-d993-4820-90d8-83070b564096`.
⚠ **Bẫy khi mang trang ra khỏi artifact**: bản artifact được hệ thống bọc `<head>`
hộ, bản tfl5 phục vụ **thô** thì không ⇒ v28 thiếu `<meta viewport>`, render
980px, điện thoại thu nhỏ. v29 tự mang head. Nghiệm thu từ URL thật: byte trùng
khít (shasum), 390/390px, 2 chế độ, 0 lỗi console. Hình để `min-width: 600px` +
cuộn ngang trong hộp riêng — co theo khung 390px làm chữ còn ~6px.

**Hà chốt 2026-08-08: "không cần nhánh VS Code".** Điều khiển **chỉ** nhắm phiên
do hub khởi động (`claude --bg`, `-p --resume <sessionId>`). Phiên mở sẵn trong
VS Code vẫn nằm trong danh sách để **theo dõi**, nhưng **không** là đích điều
khiển ⇒ bỏ hẳn nhánh "chen vào stdin", bỏ luôn nhu cầu tmux. Thiết kế còn **một
đường duy nhất**, dùng lại hạ tầng ảnh chụp sẵn có (bundle v30).

**UC-S01 XONG tới điện thoại (bundle v35).** `portal.rs` bump snapshot **schema
2→3** thêm `sessions{list,notes}`; FE có tab **"Phiên" làm mặc định**, render bằng
DOM chứ không `innerHTML` (tên/nội dung phiên là chữ do máy sinh). `hubd` restart
cùng lượt — trang cũ có guard `schema > 2` nên bump mà quên FE là **trang tự từ
chối hiển thị**. Nghiệm thu `fe-sessions-uc.mjs` **8/8** ở 390px, đối chiếu với
`hub sessions --json` chạy độc lập. Không hồi quy: smoke 15/15 · board 41/41 ·
`cargo test` 169 · clippy 0.
⚠ **Bẫy phép đo lần 3:** assert "phiên đầu nằm trong màn đầu" xanh ở **0px** vì
panel đang ẩn (bbox = 0). Sửa tab mặc định mới lộ số thật **570px** = 67% màn là
vỏ → ẩn vỏ hộp việc + bỏ app tid 38 ký tự khỏi header → **321px**. *Assert xanh
trên phần tử đang ẩn là assert mù.*

**UC-S02 XONG (bundle v38).** Chạm phiên → trang gửi verb mới **`/session <uuid>`**
vào phòng chat → hub ghi cursor `focus:session` → ảnh chụp mang luồng **của riêng
phiên đang theo** (đẩy hết mọi transcript mỗi cycle là megabyte cho một màn).
`sessions::parse_stream` tách từng khối thành **say · tool · result · think**, gác
bí mật **TỪNG KHỐI** — `tool_result` là đầu ra lệnh, nơi khoá hay lộ nhất; gác mỗi
lượt cuối như màn danh sách là vô dụng ở đây. Cửa sổ 120 sự kiện + `older_hidden`.
Nghiệm thu `fe-stream-uc.mjs` **11/11**: 90/90 sự kiện khớp, 36 lệnh + 37 kết quả,
lệnh kèm tham số, quay lại thì hub thôi theo. `cargo test` **172** · clippy 0.
⚠ **Ba bẫy:** (1) `waitForFunction(fn,{timeout})` — tham số 2 là **đối số**, không
phải options ⇒ 180s rơi về 30s (sổ đã ghi 08-07, vẫn đạp lại). (2) So bằng số sự
kiện trên phiên **đang chạy** sai theo cấu trúc (cửa sổ trượt: 84 vs 81) → chọn
phiên đứng yên >30 phút. (3) **Lỗi thật:** trang chỉ `loadBoard()` **một lần** sau
6s ⇒ phiên nào hub đẩy chậm hơn là treo vĩnh viễn → nay vòng chờ 5s, hạn 2 phút,
hết hạn nói rõ lý do.

**UC-S03 XONG (bundle v39, hubd pid 91053).** Hai nửa: `hubd::follow_sleep` cắt
giấc ngủ chu kỳ thành lát **2s**, mỗi lát chỉ `stat` **mtime** của đúng tệp phiên
đang theo — không gọi `claude`, không chạy pipeline, **không tốn tiền**; đổi thì
đẩy ngay, **sàn 4s** để phiên bận không thành lũ đẩy; tin chat tới thì trả quyền
cho vòng chính. `Waker::sleep` nay **trả về bool** để phân biệt hết-giờ với
có-tin. `sessions::find_transcript` tìm tệp theo id (readdir nông) nên vòng bám
không phải dựng path mỗi 2s. Phía trang: tự hỏi lại **4s/lần** khi đang mở phiên
và **giữ chỗ cuộn** (chỉ bám đáy nếu người đọc đang ở đáy).
**Đo thật:** phiên hoạt động → **8 lần đẩy/50s, cách nhau 4–17s** (trước: 120s);
phiên đứng yên → **0 lần đẩy** trong 2 cửa sổ 42s; trang làm mới 5 lần/15s.
⚠ **Hai lần đo sai trước khi đo đúng:** ghi thẳng cursor bằng SQL **không đánh
thức** daemon (đường thật là verb `/session` — chính nó đánh thức); và phiên bị
theo dõi **chính là phiên đang đo**, lúc đó chỉ `sleep` nên không ghi gì — số 0
ấy **đúng**, không phải hỏng. Hỏi "phép đo có tạo được điều kiện nó đang đo
không" trước khi kết luận sản phẩm hỏng.

**UC-S07 đổi nghĩa + XONG (bundle v40).** Hà: *"tắt terminal mà không mất luồng
đang xử lý dở, để mở phiên mới làm tiếp"* ⟹ **không phải giết tiến trình** — mạch
nằm ở nhật ký. Verb `/handover <id>` → `claude -p --resume <id> --fork-session`
với prompt bàn giao 4 mục; **fork** nên phiên gốc không bị sửa byte nào; trả id
mới + `cd … && claude --resume <newid>`. Chạy thật: `57dc5d73` ≠ nguồn, bàn giao
dùng được.
💸 **Lỗ tiền lộ ra ngay lần chạy đầu, đã vá cùng lượt:** tốn **$1.7228 một lần
gọi** (resume nạp cả phiên 986KB) → chi hôm nay **$4.701/$3.00, vượt 57%**. Ba lỗ:
(1) `cost_on_day` chỉ cộng `decisions` ⇒ đường chi mới **vô hình** với trần → thêm
bảng **`spend`**, trần cộng cả hai (`SCHEMA_VERSION 3`); (2) gác `spent >= cap`
chỉ chặn lần SAU → nay chặn theo **trường hợp xấu nhất** `spent + max_budget_usd
> cap`; (3) lệnh handover thiếu `--max-budget-usd` → nay có.
⚠ **`hubd` chết LẦN 2** cũng vì `claude` tự cập nhật (11:16 và 15:16): EPERM
thoáng qua làm đọc lock hỏng, mà code cũ `unwrap_or(true)` coi đó là **bị chiếm
lock** nên thoát. Nay `read_lock_pid` thử lại 3 lần/1.5s và phân biệt
Ours/Taken/**Unreadable** — chỉ nhường khi tệp đọc được và chứa pid khác; đọc
không được thì log error rồi **chạy tiếp**. Lỗi tạm thời ≠ bị chiếm quyền.

**UC-S04 đổi định nghĩa + XONG (bundle v42).** Câu hỏi cũ "thấy lúc phiên xin
quyền" **không có câu trả lời và cũng không có câu hỏi**: quét **~14.000 bản ghi**
→ thứ duy nhất dính quyền là `permission-mode` với đúng 2 giá trị `auto` **648** ·
`dontAsk` **76**; không có bản ghi lần-hỏi-đang-treo, và không thể có vì **mọi
phiên ở đây chạy chế độ tự duyệt — chúng không hỏi**. Đổi sang **hiện chế độ quyền
của từng phiên** (*tự duyệt · không hỏi · hỏi trước*): biết một phiên đang chạy
không-hỏi-ai còn quan trọng hơn. Đo: 5/14 đọc được `auto`, 9 phiên **"(chưa rõ)"**
vì cửa sổ 256KB không chứa bản ghi mode — hiện đúng là chưa rõ, **không đoán**.

**Hà hỏi "bỏ phương án cũ rồi sao vẫn dính tiền?" — đúng, và tôi để lộn xộn.**
Đo: trong $4.70 hôm nay, **github $1.818 + devlog $0.423 = $2.24 (48%)** là của
**nhánh hộp thư đã chốt bỏ mà tôi quên chưa tắt** — rồi chính nó làm cạn trần và
trần quay lại chặn tính năng mới. Đã sửa hai chỗ:
1. **Tắt `adapters.github` + `adapters.devlog`** (`doctor` xác nhận `off`; chỉ còn
   `tfl5`). Backup: `hub.config.json.bak-adapters`.
2. **Tách trần**: thêm **`owner_daily_budget_usd = 2.0`**, đếm riêng từ bảng
   `spend`. `daily_budget_usd` sinh ra để ghìm **robot chạy không ai trông**; một
   cú Hà bấm trên điện thoại là **chính chủ làm việc** — chặn nó bằng ngân sách
   của robot là vô lý.

⟹ Phần **theo dõi (S01–S04) vốn $0**: chỉ đọc `claude agents --json`, `stat` mtime
và đọc tệp. Chỉ phần **hành động** mới tốn, và tốn **đúng bằng** làm việc đó ở
terminal — không phải phí của hub.

⚠ **Nợ có sổ:** `fe-stream-uc.mjs` vẫn đọc `snap.budget` (trần robot) để chọn
nhánh kiểm UC-S07, trong khi sản phẩm nay dùng `owner_daily_budget_usd`. Lần chạy
vừa rồi xanh vì **hai trần tình cờ cùng kết luận từ chối**, không phải vì phép đo
đúng. Phải đưa owner budget vào ảnh chụp rồi sửa kịch bản.

**Chưa làm:**
tín hiệu "phiên nào đang chờ Hà" (bản ghi cuối không đủ để suy, chưa hứa); bước 2
= gõ vào phiên. Và **không gõ được vào 9 phiên đang mở**: chúng do VS Code
extension nuôi stdin bằng `--input-format stream-json`, máy **không có tmux**
(chỉ `screen`) — muốn điều khiển thì phiên phải do hub khởi động từ đầu.

## 2026-08-08 — CI đỏ: hub trả tiền để nói "chưa rõ nguyên nhân"

**Đo trước:** 38/38 quyết định nguồn github là `ci_failure`, ngốn **$4.99 / $9.12**
tổng chi cả đời hub (55%), và **cả 38 vẫn nằm nguyên trong hộp việc**. Nội dung
lặp đi lặp lại: *"chưa có log lỗi thực tế nên chưa rõ nguyên nhân"*.

**Gốc, hai chỗ:** thông báo CheckSuite **không có `subject.url`** ⇒ không có lần
gọi chi tiết nào ⇒ `body` = đúng cái tiêu đề (`github.rs:104-115`); và
`triage.rs` gom bối cảnh bằng `gh run list --limit 3` **cho cả repo, không lọc
nhánh** ⇒ model thấy 3 run của nhánh khác. Chính quyết định #66 đã tự tố:
*"gh run list (context host) lại không chứa run nào của branch này"*.

**Vá:** `github::parse_check_suite_title()` tách `workflow` + `branch` từ tiêu đề
(GitHub cố định dạng `"<wf> workflow run failed for <branch> branch"`), ghi vào
`raw.ci` ngay lúc ingest; `ci_failure_context()` lấy **run đúng nhánh** → job/step
đỏ → **check-run annotations** (gộp bản trùng thành `… [×N jobs]`), log tail chỉ
đụng tới khi không có gì giải thích được. Mọi lần hỏng thành `notes` nhìn thấy
trong prompt, không im. `is_safe_ref()` chặn ref dị dạng biến thành cờ `--repo`.
Khối CI được **`insert(0, …)`** vì `clip(6000)` cắt đuôi.

**Lỗ bảo mật do chính bản vá này mở ra — đã bịt trong cùng lượt.** Khối
`<<<CONTEXT` được prompt khai là *"trusted — collected by hub code, not by the
sender"*, mà `tripwire` chỉ quét subject+body (`triage.rs:435-440`). Bơm **log
bước CI hỏng** vào đó = cho code của người khác nói chuyện với model từ **nửa
được tin** của prompt. Nay `gather_context` trả `GatheredContext{text, tripwire}`,
quét `detect_injection` trên chính đoạn CI (nhãn `ci_log:*`), `triage` gộp vào
tripwire ⇒ `policy.rs:346` ép human + tắt trí nhớ hội thoại. Prompt tách **hai
loại phát hiện**: "untrusted body matched" (người gửi tấn công) vs "quoted CI
output" (log in ra chuỗi giống lệnh — là *dữ liệu*), vì gộp chung sẽ vu cho người
gửi mỗi khi build log có `sudo apt-get`. Nhãn khối context sửa lại cho đúng sự
thật: hub *thu thập*, nhưng nội dung *trích* là của bên thứ ba.
⚠ Còn nợ, có sổ: `git log`/`devlog tail` trong cùng khối **chưa** được quét —
hành vi cũ, không đổi trong lượt này.

**Chạy thật trên mục #243** (không mock): quyết định **#76** (conf 0.55, $0.1495,
*"chưa rõ… do CI infra hay nội dung PR"*) → **#79** (conf **0.90**, **$0.107**):
*"fail toàn bộ 6 job không do lỗi code — annotation ghi rõ recent account payments
have failed, job còn chưa kịp start bước nào"*. **Đúng hơn và rẻ hơn.**

**Chạy thật lần hai trên repo KHÁC** (dwork, nguyên nhân đỏ khác hẳn) để loại trừ
ăn may — mục #246: **#80** (bản cũ, `01:45:02Z`, trước lúc restart `01:45:26Z`)
*"chưa có log chi tiết để biết nguyên nhân"* → **#81** nêu **3 job lỗi cụ thể**
(`vitest workspace-mobile` fail thật, `vitest workspace-web` + `playwright smoke`
chết ngay ở bước `actions/setup-node@v4` kèm *"unable to cache dependencies"*).
Ở ca này **giá TĂNG** $0.107 → $0.166 và confidence hạ 0.82 → 0.78 — hạ đúng, vì
nó thôi đoán và nói thẳng phần còn chưa biết. Bài học: bản vá đổi *"rẻ hơn"* thành
*"trả tiền cho thứ dùng được"*, không phải luôn rẻ hơn.

`cargo test` **162 xanh** · fmt sạch · clippy không thêm cảnh báo mới · `hubd`
restart 2 lần trong lượt (cuối: pid 23972) để binary đang chạy khớp mã — đúng cái
bẫy "contract + consumer" đã đạp 2 lần ngày 08-07.

### 🔴 Việc của Hà, ngoài phạm vi hub: CI của tfl5 chết 5 ngày vì THANH TOÁN

`dipgle/tfl5`: **60/60 run FAIL, 0 thành công, từ 2026-08-03**. Job chết trong 2
giây, chưa từng chạy bước nào. Annotation nguyên văn: *"The job was not started
because recent account payments have failed or your spending limit needs to be
increased"*. ⟹ **mọi CI đỏ của tfl5 là giả**, không phải lỗi code — kể cả run
00:48 hôm nay của session tfl5. Mở **GitHub → Settings → Billing & plans** của
tài khoản `dipgle`. (API billing cần scope `user`/`admin:org`, hub không đọc được.)

`dwork-dev/dwork` cũng 50/50 fail từ 08-04 nhưng **lý do khác và là lỗi thật**:
`Run actions/setup-node@v4` fail (Node 20 bị ép sang Node 24) + `Test
mobile-app-dwork` fail.

### Vì sao hộp việc không tự xử lý (Hà hỏi 08-08) — đã đo, chưa sửa

Lý do hub tự ghi trong `decisions.raw.outcome.reason`, 62 mục treo:
`model set needs_human` **39** · `tier L0 (trust=trusted) drafts only` **19** ·
`tier L0 (trust=untrusted)` **4**. Bốn khoá chồng nhau:
1. `autonomy.default = "L0"` và **không dự án nào đặt `tier`** → mọi thứ L0
   (`policy.rs:254-275`, `:370`). ⚠ `active-context` từng chép ví dụ registry có
   `"tier":"L1"` — **config thật không có**, đừng tưởng đã bật.
2. `needs_human = 1` trên **78/78** quyết định (`policy.rs:355`).
3. `ci_failure` **không** nằm trong `AUTO_REPLY_KINDS` (`policy.rs:29`).
4. CheckSuite **không có địa chỉ trả lời**: `github_reply_target` cần số issue/PR,
   mà `detail=null` và url là repo → `target=None` (`policy.rs:394`).
Thêm: **không có cơ chế hết hạn** — `db.rs:527 reset_triaging` chỉ cứu hàng kẹt ở
`triaging`; `awaiting_human` không bao giờ tự già đi. Hộp việc chỉ có tăng.

⟹ Kết luận thiết kế: nâng tier **không** giải quyết, vì L1 chỉ mở đường *gửi trả
lời*. Thứ còn thiếu là nước đi thứ tư **`auto_resolve`** (tự kết luận + tự đóng,
có ghi lý do) bên cạnh `auto_reply/await_human/ignore` (`policy.rs:33-37`).

### Đã mất chỗ quản lý danh sách project (Hà hỏi 08-08) — đã đo, chưa sửa

Registry `config.projects` là nguồn sự thật trong mã (`policy.rs:151-161`) và
`hub doctor` vẫn đối chiếu đủ 8/8 với thư mục thật — nhưng **không màn nào sửa
được nó**: `grep CFG.projects rust/assets/ui.html` → **0**; board tfl5 chỉ có
`autonomy.default` (`fe/index.html:1027`); `/set` từ chối kiểu object và **không
thêm được trường chưa tồn tại** (kiểu suy từ giá trị đang có, `pipeline.rs:1383`)
nên `/set projects.tfl5.tier L1` báo "không có trường" — **đó là lý do không dự
án nào có tier**. Console vẫn đang quản lý đúng hai bảng **cũ** mà registry đã
thay: `autonomy.projects{}` + `routing[]` (`ui.html:468-503`).
Bug kèm theo: `routing` rỗng nên biến mất khỏi JSON (`skip_serializing_if`), mà
`ui.html:543` gọi `CFG.routing.push(...)` → **nút "+ thêm luật" ném TypeError**
(đọc từ mã, chưa bấm thử). Và `known_projects()` = `discover_projects()` = *thư
mục có devlog* (19 cái), khác hẳn 8 khoá registry — giao nhau đúng `sdvi`+`tfl5`.

## 🔚 CHỐT PHIÊN 2026-08-07/08 — đọc mục này trước

**Đang chạy:** bundle **v27** (prev v26) tại
`http://a-65dd60d3-624e-45a9-8fdf-62aa7d894d80.test.localhost:8090/` · `hubd`
binary mới nhất · console `hub web` :9200 vẫn sống (nay chỉ còn để sửa JSON thô
+ dự phòng khi tfl5 chết).

**Trang tfl5 giờ làm được mọi thứ console làm**, qua 5 tab: Hộp việc · Trao đổi
· Sức khoẻ · Chi phí · Cấu hình. Ghi = nút bấm gửi đúng lệnh gạch chéo vào phòng
(`/approve /reject /close /reply /ingest /run /doctor /set /project`), nên CLI,
console và trang web dùng **chung một đường** `pipeline::*`.

**Nghiệm thu đang xanh** (chạy thật, dữ liệu thật, 0 lỗi console):
board 41/41 · link 8/8 · reply 8/8 · context 7/7 · config 8/8 · command 6/6 ·
denied 10/10 · pending 4/4 · smoke 15/15 · `cargo test` 15 bộ · fmt sạch.

**Trạng thái vận hành cần biết:** hôm nay **đã chạm trần** `daily_budget_usd = 3`
($3.04) ⟹ câu hỏi mới vào hộp việc nhưng KHÔNG được triage; bảng hiện chip đỏ
và khung chat nói rõ lý do. `max_triage_per_cycle = 6`,
`trust.tfl5_user_tids` = alice_local + administrator.

**Việc chưa làm:** bug tfl5 `pattern="[A-Za-z0-9._-]{1,64}"` ở màn Releases
(Chrome parse bằng cờ `v` ⇒ pattern vô hiệu + bẩn console) — sửa bằng escape
`\-`, thuộc session tfl5.

## 2026-08-07 — "Mất kết nối, đang thử lại…" hoá ra là lỗi phân quyền

Hà mở `http://a-65dd60d3….test.localhost:8090/` bằng tài khoản `administrator`
và thấy báo mất kết nối lặp vô hạn. **Không có sự cố mạng nào**: `administrator`
không nằm trong ACL của app hub (`readers` chỉ có alice), nên `/ws/chat` từ chối
nâng cấp (Reader+, `crates/routes/src/ws_chat.rs`); WS `close` không mang mã HTTP
nên FE hiểu nhầm thành rớt mạng và thử lại mỗi 3s mãi mãi.

- **Cấp quyền qua UI console** (`node console-acl.mjs u-34d6a0c5-…`): đăng nhập
  hubbot → Open app → Manage access → "Can view". ⚠ Bẫy đã đạp: xpath theo nhãn
  bắt trúng dòng tóm tắt *ngoài* hộp thoại và ghi nhầm vào **Can delete**, mà
  script vẫn in "ĐẠT" vì nó đọc lại chính ô nó vừa gõ. Nay đọc (nhãn→giá trị)
  theo document order và **nghiệm bằng dòng tóm tắt "N can view"** của máy chủ.
- **FE `fe/index.html` phân biệt từ chối với mất mạng**: `api()` giữ `code` +
  `status`; `access_denied` → nói rõ tài khoản/phòng/cách sửa, ẩn ô nhập, có nút
  "Đăng nhập bằng tài khoản khác", **ngừng** thử lại; `unauthorized` → về màn
  đăng nhập; mất mạng thật → báo **một lần** + backoff 3s→30s.
- **Deploy `v3` qua UI Releases** bằng `fe-deploy.mjs` (thay các lệnh curl rời của
  v1/v2; idempotent, đọc HTTP của `/app/bundle/activate`, tự đọc trang công khai
  để xác nhận). Bẫy: Playwright **tự dismiss** `confirm()` ⇒ nút Activate bấm mà
  không gửi request nào; bảng Releases không bắt được bằng locator theo text,
  phải dùng `[data-activate="<v>"]`.
- **Nghiệm thu**: `fe-smoke.mjs` 15/15 · `fe-denied-uc.mjs` 10/10 (owner gỡ alice
  khỏi "Can view" bằng UI → alice thấy đúng lý do, 0 dòng "Mất kết nối" trong 13s
  → owner trả quyền lại; ACL đối chiếu lại đúng nguyên trạng).
- **Console 9200**: bỏ `max-width:1180px` (thừa 740px trên màn 1920) → dùng hết
  bề ngang, thêm mốc 1800px cho cột danh sách 760px. Đã build lại + khởi động lại
  `hub web` ⇒ **token đổi, tab đang mở phải F5**.
- **Lỗi của tfl5 phát hiện kèm** (chưa báo sang session tfl5): ô Version ở màn
  Releases có `pattern="[A-Za-z0-9._-]{1,64}"` → Chrome parse bằng cờ `v` và ném
  `SyntaxError: Invalid character in character class` ⇒ **pattern mất tác dụng**
  và console log lỗi. Sửa: escape `\-` (hoặc đặt `-` ở cuối lớp ký tự).
## 2026-08-07 — gộp console vào app hub: hubd đẩy, trang tfl5 đọc

Hà chốt hướng "hubd đẩy dữ liệu lên tfl5" (không port console sang bundle,
không mở CORS). Lý do đầy đủ nằm ở đầu `rust/src/portal.rs`.

- **Kênh: docs, KHÔNG phải files.** Bản đầu ghi `hub-status.json` qua
  `/app/file/save`. Files nằm dưới public-asset tree và
  `public.rs::row_acl_evaluate` coi **ACL rỗng = ai cũng tải được** — chỉ an
  toàn chừng nào còn bundle live (mọi path khác 404), một điều kiện operator gỡ
  được bằng một cú bấm. Điền ACL vào cũng không xong: `file_row_visible` dùng
  đúng rosters đó nên danh sách cứng sẽ chặn luôn thành viên thêm sau. ⟹ chuyển
  sang **doc** trong resource `hub_status` (chỉ vào được qua API, gác bởi
  Reader+ của app). File cũ đã xoá trong cùng lượt đẩy.
- Đường đi: `hub portal-push` (có `--dry-run`) + `hubd` tự đẩy sau mỗi cycle
  (lỗi được log, `Skip` khi kênh tắt, không bao giờ làm chết vòng lặp).
- FE bundle **v5**: hai tab "Trò chuyện | Bảng điều khiển"; bảng có dải số,
  biểu đồ chi phí (**ECharts** vendored trong bundle, CSP `script-src 'self'`),
  bảng hộp việc. **Chỉ đọc** — duyệt vẫn bằng `/approve` trong phòng chat, đúng
  nguyên tắc #7 "một đường duyệt duy nhất".
- Nghiệm thu: `fe-board-uc.mjs` **14/14** (kể cả "trang không gọi ra ngoài
  origin"), `fe-smoke.mjs` 15/15, `fe-denied-uc.mjs` 10/10, `cargo test` 14 bộ
  xanh, fmt sạch.
- ⚠ **Bẫy phép đo lặp lại lần 2 trong phiên**: check biểu đồ ban đầu chỉ hỏi
  "có `<canvas>` không" → xanh trong khi chart mới vẽ mỗi trục (ảnh chụp lúc
  animation chưa xong). Nay đếm **pixel màu series** + đọc `getOption().series`.
- Console 9200: nút "Tải lại" ở Hộp việc vỡ 2 dòng cạnh ô lọc → `white-space:
  nowrap` cho mọi `button`; đo lại 62×35px, 1 dòng, cùng hàng ở 1100/1440/1920.
- `cargo fmt` đã format lại vài file ngoài phạm vi (web.rs, db.rs…) vì cây
  trước đó chưa fmt-sạch; hub không có git nên không tách được diff.

`hubd` đã restart sang bản có portal-push và **tự đẩy ngay trong cycle đầu**.
Bản cũ không dừng bằng Ctrl-C vì nó chạy nền chứ không phải foreground job —
phải `kill <pid>` rồi khởi động lại (lock file từ chối tiến trình thứ hai, đúng
thiết kế).

### Gộp ĐỦ cả 4 mảng của console (Hà hỏi "sao chỉ gộp mỗi hộp việc?")

Bản đầu chỉ mang sang hộp việc + số tổng + biểu đồ chi phí — tự thu hẹp phạm vi
mà không nói ra. Nay snapshot **schema 2** mang đủ:

| mảng | nguồn | ghi chú |
|---|---|---|
| Hộp việc + **chi tiết từng mục** | `list_messages` + `latest_decision_for` | thêm `body` (cắt 1200 ký tự), `reply_draft` (2000), `actions`, `model`; `clip()` **nói rõ đã cắt** |
| Sức khoẻ | `last_runs(12)` mỗi cycle + probe kênh | probe (github/telegram/email/tfl5/claude) tốn mạng ⇒ cache `HEALTH_TTL_MS` 10 phút, snapshot mang `checked_at` để trang nói rõ số liệu cũ bao lâu |
| Chi phí | query theo ngày | biểu đồ ECharts + bảng |
| Cấu hình | `serde_json::to_value(cfg)` | **chỉ đọc**; an toàn vì config chỉ chứa TÊN biến môi trường (nguyên tắc #3) — có test `config_carries_no_secret_values_only_env_var_names` khoá lại |

Bundle **v6**. Nghiệm thu `fe-board-uc.mjs` **23/23** (thêm: bấm dòng ra đúng
mục đó, tab Sức khoẻ có 6 chip + 12 lượt chạy, tab Chi phí có bảng ngày, tab
Cấu hình 3143 ký tự + không lộ secret + chỉ đường sang 9200 để sửa).

⚠ **Bẫy "contract + consumer phải đi cùng nhau"**: đẩy tay snapshot schema 2
xong mà `hubd` vẫn chạy binary CŨ ⇒ cycle kế tiếp **ghi đè lại bằng schema 1**,
test đỏ đúng 4 mục health/config. Đổi định dạng snapshot ⇒ phải restart hubd
trong cùng một lượt.

### Dồn về MỘT nơi: bảng cũng ghi được (Hà chốt "dồn tất cả làm 1")

Nút trên bảng gửi **đúng lệnh gạch chéo vào phòng chat** qua socket đang mở —
không API thứ hai, không token, không CORS; mọi thao tác để lại dấu trong phòng
và vẫn đi qua `pipeline` như CLI. Thêm 2 verb: `/close <message-id>` và
`/reply <message-id> <text>`; `close_message` + `reply_to_message` chuyển từ
`main.rs` xuống `pipeline.rs` để CLI / console / chat dùng **chung một hàm**.

**Giao diện**: bảng dùng lại nguyên hệ của console (`rust/assets/ui.html`) —
token màu, `.panel` bo 12px, `.split` list-trái/chi-tiết-phải, `.scroll`. Trước
đó tôi tự chế palette của trang chat nên hai mặt trông như hai sản phẩm.

### "Hộp việc và Trao đổi khác nhau à" → tab riêng + nối hai chiều (v23–v27)

Đo trước khi trả lời. **Khác nhau thật**, và chỉ giao nhau một phần:

| | trong phòng chat | trong hộp việc |
|---|---|---|
| tổng | 146 dòng (83 alice · **61 hubbot** · 2 administrator) | 169 mục |
| chỉ có ở đây | 61 dòng hub tự nói + **46 lệnh gạch chéo** | **125 mục** github/devlog/cli |
| giao nhau | \=\=\=\= 44 câu hỏi gõ trong phòng \=\=\=\= | |

⟹ Hà chốt: **tab riêng** cạnh Hộp việc (gộp vào cột phải thì chật, đúng như Hà
nói). Nhưng phần giao nhau phải **nhìn thấy và đi lại được**:
- chọn mục nguồn `tfl5` → tin gốc trong Trao đổi được viền xanh + nút *"Xem tin
  gốc trong Trao đổi"* ở khung Chi tiết (đánh dấu chứ **không** tự nhảy tab —
  giật người đang đọc danh sách đi còn tệ hơn),
- mỗi tin đã thành việc có nút *"#\<id\> trong hộp việc"* → mở đúng dòng đó
  (tự bỏ bộ lọc đang che nó).
- Snapshot mang thêm `external_id` để nối `tfl5:<chat tid>` ↔ mục.

Bẫy: tin chat render **trước** khi ảnh chụp đầu tiên về, nên map tid→id còn
rỗng ⟹ nút không bao giờ mọc. Nay `decorateChatLinks()` chạy lại sau mỗi lần
`renderRows`. Và trang **nhớ tab đang mở qua F5** (localStorage) — trước đó F5
là văng về Hộp việc, mất luôn ô nhập.

Nghiệm thu: `fe-link-uc.mjs` **8/8** + board 41/41 · smoke 15/15 · reply 8/8 ·
context 7/7 · pending 4/4 · command 6/6 · denied 10/10.

### MỘT màn duy nhất: bỏ tab Trò chuyện, đưa trao đổi vào cột phải (v20–v22)

Hà: *"trong hộp việc có đủ rồi cần gì tab trò chuyện nữa?"*. Đo trước khi bỏ —
bảng **chưa** đủ: snapshot chỉ mới lại mỗi `poll_interval_sec` (**120s**) trong
khi socket chat là tức thì; **52 tin** hubbot trong phòng (ack `/close`,
`/project`, `/doctor`…) chỉ hiện ở luồng chat; và nút ↩ gắn-ngữ-cảnh sống trong
luồng đó. Nên **gộp** chứ không xoá: thanh tab trên cùng biến mất, khung
**Trao đổi** nằm ngay dưới Chi tiết ở cột phải của Hộp việc. Còn đúng 4 tab con
như console.

Ba lỗi lộ ra khi gộp (đều đã vá):
- `scrollDown()` vẫn cuộn `<main>` trong khi khung trao đổi có thanh cuộn
  riêng ⟹ tin mới nhất nằm khuất dưới đáy.
- Trạng thái tải nhét trong `<h2>` thành "TRAO ĐỔI — ĐẦU HỘI THOẠI —".
- Màn **không có quyền** ẩn luôn `#board`, mà khung trao đổi (chứa lời giải
  thích + nút "đăng nhập tài khoản khác") giờ nằm TRONG đó ⟹ mất lối thoát.
  Nay `#board.denied` chỉ giữ panel `.keep`.

Nghiệm thu lại toàn bộ sau khi đổi layout: board 41/41 · smoke 15/15 · denied
10/10 · reply 8/8 · context 7/7 · command 6/6 · pending 4/4.

### "Vẫn bất tiện, phải nhớ mới làm được" → trả lời một tin là gắn ngữ cảnh (v18–v19)

Ghim bằng `/project` vẫn bắt người dùng **nhớ lệnh**. Hà đề xuất đúng mô hình
quen thuộc: *"sao không phải trả lời tin nào thì gắn được"*.

- tfl5 chat **không có trường reply/parent** (`chat_message` chỉ có tid + text,
  kiểm bằng `\d chat_message`), nên reply được mã hoá trong nội dung:
  `↩[<chat tid>] …`. `split_reply_marker` tách ra ở **cả hai** đường ingest,
  tid đi vào `raw.reply_to`, và **model không bao giờ thấy phần kỹ thuật**.
  Trang chat cũng ẩn nó, hiển thị thành khối trích dẫn.
- Thứ tự suy ngữ cảnh: repo/tiền tố → **reply_to** (chủ ý, nhắm vào một tin) →
  ghim phòng → tin gần nhất trong 12h.
- Nút ↩ nằm trên **từng tin**; thanh "Đang trả lời: …" phía trên ô nhập, có nút
  bỏ. Không lệnh, không phải nhớ gì.

**Chạm trần ngân sách = im lặng, đã vá.** Bộ pending đỏ lộ ra: khi
`daily_budget_reached` ($3.04/$3.00), tin vào hộp việc rồi **đứng nguyên** và
trang vẫn quay 8 phút rồi tắt không lý do. Nay snapshot mang `budget
{spent_usd, cap_usd, stopped}`; khung chat nói thẳng lý do + chỉ chỗ nâng trần,
bảng hiện chip đỏ **CHẠM TRẦN NGÀY**.

Nghiệm thu: `fe-reply-uc.mjs` **8/8** (không gõ lệnh nào: bấm ↩ → câu hỏi không
nêu dự án vẫn nhận `project = tfl5`, trích dẫn hiện đúng, không lộ `↩[…]`),
`fe-pending-uc.mjs` 4/4 dưới đúng tình huống hết ngân sách, context 7/7, board
41/41, smoke 15/15, `cargo test` 15 bộ.

### Chat mất ngữ cảnh dự án (Hà báo 08-07 tối) — bundle v17

*"phải nhắc quá nhiều thông tin trong nội dung chat để biết đang nói về dự án
nào"* — đúng, và dữ liệu xác nhận: **mọi** dòng chat trong store đều
`project = NULL`.

Hai nguyên nhân, cái thứ hai mới là gốc:
1. `resolve_project` (`policy.rs:136-190`) chỉ nhận ra dự án qua repo / matcher
   / `routing` / tiền tố `[tfl5]`–`tfl5:` ở ĐẦU subject. Chat không có repo,
   `routing` rỗng, registry khớp theo repo ⟹ chỉ còn cách gõ tiền tố ở **mỗi**
   câu.
2. **`live.rs` chèn thẳng `NewMessage` vào store, bỏ qua toàn bộ khâu gắn
   project/trust của `ingest`.** Live gần như luôn thắng poller ở kênh chat,
   nên routing của poller là **mã chết** với source này. Nay tách
   `pipeline::enrich_message` và **cả hai đường đều gọi** — cùng loại lỗi
   "hai đường, một đường quên bước" đã gặp ở `parse_command`.

Thêm:
- **Ngữ cảnh dính theo phòng**: câu không nêu dự án kế thừa dự án nhắc gần nhất
  trong cùng `thread_key`, cửa sổ **12h** (`THREAD_PROJECT_HOURS`). Câu trống
  dự án KHÔNG xoá ngữ cảnh; phòng khác không rò sang nhau.
- **`/project <tên>`** ghim cố định (cursor `pin:project:<thread>`), `/project`
  xem, `/project -` bỏ. Tên không có thật bị từ chối kèm danh sách dự án đang
  biết — một ghim sai sẽ định tuyến mọi câu sau vào thư mục không tồn tại.
- **Header khung chat hiện ngữ cảnh**: `📌 tfl5` (ghim) · `· tfl5` (suy ra) ·
  `· chưa rõ dự án`, lấy từ `snapshot.chat`.

Nghiệm thu: `rust/tests/chat_context.rs` **6/6** (kế thừa, cửa sổ 12h, mới nhất
thắng, câu trống không xoá, không rò giữa phòng, pin theo thread) +
`fe-context-uc.mjs` **7/7** chạy thật: ghim → hỏi câu **không nhắc tên dự án**
→ tin vào hộp việc mang `project = tfl5` → bỏ ghim.

### Spinner "đang xử lý" treo vĩnh viễn (Hà báo 08-07 tối) — bundle v16

Gửi câu hỏi ở tab Trò chuyện xong là dòng *"đang xử lý — câu trả lời cần người
duyệt trước khi gửi"* đứng mãi. Không phải treo kỹ thuật: ở tier **L0 hub soạn
NHÁP rồi dừng chờ owner**, nên **không bao giờ** có tin trả lời tự đến để tắt
spinner. Câu chữ đúng trong ~1 phút đầu và sai từ đó trở đi.

Nay trang **bám theo chính câu hỏi đó trong snapshot** (`watchPending`) và kết
thúc ở một trong ba trạng thái thật: đang phân loại · đã gộp vào quyết định
khác · **hub soạn xong, chờ duyệt** — kèm nút **Duyệt & gửi #<id>** ngay trong
khung chat và nút mở Bảng điều khiển. Có hạn 8 phút; hết hạn thì tắt spinner
kèm lý do, không im lặng.

⚠ Hai bẫy trong lúc vá:
- `messages.coalesced_into` chứa **DECISION id** (`pipeline.rs:507` ghi
  `open.id`), KHÔNG phải message id. Tra nhầm nên mục cha không bao giờ tìm
  thấy → spinner vẫn treo ở nhánh coalesced.
- `page.waitForFunction(fn, {timeout})` — tham số thứ hai của Playwright là
  **đối số truyền vào hàm**, không phải options; timeout 300s âm thầm rơi về
  mặc định 30s và test đỏ vì phép đo, không vì sản phẩm.

Nghiệm thu: `fe-pending-uc.mjs` **6/6** (gửi thật → không còn quay vô hạn → có
nút Duyệt); board 41/41 · smoke 15/15 · denied 10/10.

### Đối chiếu TỪNG màn với console (Hà: "màn nào chả thiếu")

Sau khi bị bắt lỗi tab Chi phí, đọc lại toàn bộ `ui.html` và lập bảng. Thiếu 5
nhóm, nay đã bù đủ (bundle **v14**):

| console | trạng thái |
|---|---|
| Header: **Poll kênh**, **Chạy 1 vòng** | ✅ verb mới `/ingest`, `/run` |
| Hộp việc: panel **Hỏi hub**, **bộ lọc trạng thái** | ✅ (hỏi = tin nhắn thường; lọc phía client trên snapshot) |
| Chi tiết: **Đề xuất**, **Bằng chứng**, dòng **policy**, link, **nháp SỬA ĐƯỢC** | ✅ snapshot mang thêm `evidence`+`outcome`; `/approve <id> <nội dung>` giờ dùng nội dung đã sửa (trước đó `cmd.arg` **bị nuốt** — bấm Duyệt là gửi bản của model) |
| Sức khoẻ: nút **Kiểm tra** | ✅ verb `/doctor` → `portal::probe_now` (bỏ qua cache 10 phút) |
| Cấu hình: **form + Lưu** | ✅ verb `/set <trường> <giá trị>`: kiểu được suy từ giá trị đang có, round-trip qua `Config` + `config::validate` + `config::save` (backup + temp-rename). Form gửi **một lệnh cho mỗi trường thực sự đổi** |

`/ingest` và `/run` chỉ **trả lời** chứ không tự gọi lại pipeline: code này chạy
BÊN TRONG một cycle (`run_once → ingest → execute_commands`), gọi lại là đệ quy.

Nghiệm thu: `fe-board-uc.mjs` **41/41**, `fe-config-uc.mjs` **8/8** (đổi
`max_triage_per_cycle`, hub xác nhận, **đọc lại tệp trên đĩa**, rồi trả về giá
trị cũ), `fe-command-uc.mjs` 6/6, `fe-smoke` 15/15, `fe-denied` 10/10.

⚠ Ba lần liên tiếp test đỏ vì **phép đo**, không phải sản phẩm: (1) `td.num` đã
đổi class khi port CSS; (2) tiêu đề mục/nhãn nhóm bị CSS `text-transform:
uppercase` nên regex có dấu-thường không khớp `innerText`; (3) chờ ack theo
NỘI DUNG trong phòng chat → khớp ngay tin y hệt của lần chạy trước, đọc file
trước khi ghi kịp. Ack phải chờ **tin mới** (đếm trước/sau), không phải tin khớp.

**Tab Chi phí cũng phải khớp** (Hà bắt lỗi lần 2): console có **hai** panel —
"Chi phí triage theo ngày" (bar + line *smooth*, palette nhà
`['#2f6f4e','#74c69d','#9a6b1f','#a33a2c','#4a6fa5']`, có legend, trục phải tên
`decisions`) và **"Message theo trạng thái" (donut 45–70%)**. Bản đầu của tôi
chỉ có 1 biểu đồ màu mặc định ECharts + một cái bảng console không hề có. Nay
port field-for-field từ `drawCharts`; test khoá cả loại biểu đồ lẫn mã màu
`#2f6f4e`. **Bài học: "gộp" nghĩa là mang nguyên thiết kế sang, không phải vẽ
lại theo trí nhớ** — mỗi lần tự chế là một lần Hà phải chỉ ra.

**Ba lỗi thật lộ ra khi chạy bằng UI (đều đã vá + có test khoá):**
1. `live.rs` ingest MỌI frame thành message, **không hề gọi `parse_command`** —
   chỉ đường poll mới tách lệnh. Kết quả: `/close` vừa chạy vừa bị đem đi
   triage. **Tốn $0.18 để phân loại chữ "close"** (message #157).
2. Cửa sổ im lặng 10s + chu kỳ poll giữ luôn cả **mệnh lệnh** ⇒ bấm nút xong
   ~2 phút mới có gì xảy ra. Nay lệnh đi đường riêng, lấy thẳng từ trang
   history, có **cursor riêng** `…:last_cmd_ts` (dùng chung cursor tin nhắn thì
   hoặc nuốt tin đang trong cửa sổ, hoặc chạy lệnh hai lần — `/reply` sẽ gửi
   hai lần).
3. Lệnh đã thực thi ở cycle trước **rơi ngược thành tin nhắn thường** ở cycle
   sau (cursor lệnh đã vượt ⇒ không khớp nhánh command ⇒ lọt vào `plain`).
   Nay lệnh luôn bị loại khỏi luồng tin nhắn, cursor chỉ quyết định *có thực
   thi hay không*.
4. Bảng tải lại thì **xoá trắng pane chi tiết** — nay giữ nguyên mục đang chọn.

**Vai trò**: lệnh phải do tài khoản trong `trust.tfl5_user_tids` gửi, và **không
thể là chính `hubbot`** (hub lọc tin của chính mình trong `select_new`, nếu
không sẽ tự triage lời mình). Đã thêm `administrator`
(`u-34d6a0c5-…`) vào trust list — gỡ bằng cách xoá khỏi `hub.config.json`
(daemon tự nạp lại theo mtime, không cần restart).

**Nghiệm thu**: `fe-command-uc.mjs` **6/6** — bấm nút → hub trả lời trong phòng
→ ảnh chụp mới cho thấy mục sang `closed`, và **số mục chứa "/close" không
tăng** (bằng chứng không còn tốn tiền triage). Cùng lượt: fe-board 31/31,
fe-smoke 15/15, fe-denied 10/10, `cargo test` 14 bộ xanh, fmt sạch.
Rác thí nghiệm (#157/#160/#161/#163 — lệnh bị ingest nhầm) đã đóng bằng `hub close`.
Bundle **v9**.

## Đang ở đâu (2026-08-06)

hub giờ có **kênh thứ 5: `tfl5`** — một phòng chat trên tfl5, hub là *client kết
nối ra*, không mở cổng nào trên máy. Người dùng gõ trên giao diện web, câu hỏi
chạy hết pipeline, owner duyệt, câu trả lời **tự hiện lại trên trang đang mở**.

- Local: tfl5 `:8090`, user `hubbot`, app `a-65dd60d3-624e-45a9-8fdf-62aa7d894d80`,
  phòng `hub`, FE ở `http://<app_tid>.test.localhost:8090` (bundle v2).
- Test: `cargo test --offline` → **112/112**, 0 warning. FE: `node fe-smoke.mjs
  <app_tid> <user> <pass>` → 15/15, 0 console error.
- Secret trong `hub.env` (chmod 600, gitignored). `HUB_TFL5_ALICE_PASSWORD` là
  tài khoản **test** — xoá được bất cứ lúc nào.

**Đã chạy thật thêm (2026-08-06 chiều):** act stage lần đầu trên `AI/hub-act-demo`
(repo nháp cô lập) · lệnh `/approve` `/reject` `/help` trong phòng chat, chỉ owner
mới ra lệnh được, `/act` cố ý bị từ chối trong chat · trí nhớ hội thoại qua
`--resume` (`source_thread_memory_hours`).

**Socket thường trực:** `src/live.rs` giữ `/ws/chat` mở trong `hubd` + `Waker`
cắt ngắn giấc ngủ của vòng lặp. Toàn bộ ở phía hub, **không sửa gì của tfl5** —
server đã push sẵn. Poller giữ nguyên làm đường chắc (rớt mạng vẫn không mất tin;
`UNIQUE(source,external_id)` lo trùng lặp).

**NAT — đừng bàn lại:** không có gì để "đục". Hole punching dành cho hai máy đều
sau NAT; tfl5 đã có IP công khai. Một kết nối đi ra giữ mở **chính là** cái lỗ.
Webhook của tfl5 (`hooks.rs`) gọi tới URL công khai ⇒ quay lại đúng bài toán cũ.

**Không làm được:** streaming từng chữ — tfl5 chat **không có endpoint sửa tin
nhắn** (`ws_chat.rs:183-195`), cần tfl5 thêm mới; phải bàn với session tfl5.

**Ba cái bẫy của `--resume` (đừng đâm lại):** `--no-session-persistence` loại trừ
`--resume` nên lượt ĐẦU phải bỏ cờ đó (3 trạng thái Off/Start/Resume, không phải
2) · phiên đã resume **giữ nguyên system prompt lúc tạo**, sửa `SYSTEM_PROMPT`
chỉ ăn vào hội thoại mới · phiên mất phải **thoái lui** chứ không được làm triage
chết.

**Chưa nghiệm thu được:** gửi payload tiêm lệnh qua khung chat thật — hook lệnh
của workspace chặn (đúng), nên nhánh đó chỉ có test, không có lần chạy thật.

## Một registry duy nhất cho project (Hà chốt 2026-08-06)

`config.projects` — khoá là **tên thư mục**, repo GitHub là **option của dự án**:

```json
"projects": { "tfl5": { "repos": ["dipgle/tfl5"], "tier": "L1" } }
```

Thay cho `routing[]` + `autonomy.projects{}`; **cả hai bảng cũ vẫn được đọc**
(có test khoá) nên config cũ không vỡ, registry thắng khi cả hai cùng khai.
`hub doctor` in mục `projects:` đối chiếu từng tên với thư mục thật và báo **SAI**
nếu không thấy. `hub say -p` và `/api/say` từ chối tên không có thư mục ngay tại
chỗ — trước đó `../../tmp` vẫn được **lưu vào DB** dù phân giải đã từ chối.

## Tên project phân giải thế nào (Hà chốt 2026-08-06)

**Gốc là `~/Documents/projects`, KHÔNG phải `AI/`.** `config.project_roots`
(mặc định `["", "AI"]`) là danh sách thư mục tìm theo thứ tự — gốc trước, `AI/`
sau. Dùng chung cho cả `project_dir` lẫn `devlog::discover_projects`; trước đó
hai hàm có thứ tự riêng nên "thư mục nào chứa project" có hai câu trả lời.
`hub doctor` in dòng `project dirs` để danh sách này hết vô hình.

**`project` là TÊN, không phải đường dẫn** — `is_project_name()` chỉ nhận một
segment. Lý do: giá trị này đến từ output của model (`decision.project`) và
`act.rs` dùng nó làm target cho `git worktree add`; `../../elsewhere` sẽ trỏ act
stage ra ngoài workspace.

## Bài học đắt nhất phiên này (đừng phát hiện lại)

`thread_key` của chat là **cái phòng**, không phải chủ đề. Coalescing 12h đúng
cho một issue GitHub nhưng làm chat nuốt mất câu hỏi. Và cửa sổ gộp phải neo vào
**lúc người ta gõ** (`messages.received_at`), không phải lúc hub xử lý — nếu neo
vào `decisions.ts` thì hễ hub xử lý dồn là mọi câu hỏi lại dính vào nhau.

## Đang ở đâu

**Canonical = Rust** (`rust/`, binary `hub` + `hubd`, wrapper `./hub` tự build lần
đầu). Bản Node archive ở `legacy-node/` làm oracle — xoá lúc nào cũng được, nó
không còn trỏ vào DB thật. Test: `cd rust && cargo test --offline` → 50/50, 0
warning, exit 0.

Hub chạy được với 3 kênh không cần credential: **GitHub** (`gh`), **devlog dự
án**, **CLI** (`hub say`). Email (mailler) + Telegram đã code ở **cả hai bản**
nhưng **chưa gọi thật lần nào** vì thiếu token. Act stage (sửa code trên branch)
đã code, cờ tắt, **chưa chạy lần nào**.

Trạng thái DB (2026-07-26 ~14:30, sau khi bản Rust chạy thật): 34 message,
11 decision, ~$1.20 chi phí triage, 12 outbox đã gửi, 1 dead_letter (lần đầu
adapter devlog fail vì `sdvi`/`tfl5` có file devlog rỗng — đã sửa thành "not
initialized", không còn tính là lỗi).

## Mặt giao tiếp (Phase 2, 2026-07-27)

- `./hub web` → bảng điều khiển ở `127.0.0.1:9200`: Hộp việc (duyệt/bỏ/sửa nháp),
  Cấu hình (form đầy đủ + JSON thô, lưu có validate + backup), Sức khoẻ, Chi phí
  (ECharts). Auth bằng header `x-hub-token` sinh mỗi lần khởi động.
- Telegram: brief chờ duyệt kèm nút ✅/🚫; bấm nút chạy đúng đường approve/reject
  của CLI rồi sửa lại chính tin nhắn đó. `./hub telegram-link` tự ghi chat id.
- Test UI: `node ui-smoke.mjs http://127.0.0.1:9200` (Playwright headless, 14 case).

## Việc tiếp theo khi mở lại

1. `./hub doctor` — xem kênh nào bật/tắt.
2. Nếu muốn kênh người-thật: xuất `HUB_TELEGRAM_TOKEN` (BotFather) → `doctor` in
   chat id → bật `adapters.telegram.enabled` + `allowed_chat_ids` +
   `trust.telegram_chat_ids` → `hub once`.
3. Nghiệm thu act stage 1 lần (mục 2 trong `PLAN.md`) trước khi tin nó.

## Điều đã học trong lúc dựng (đừng phát hiện lại)

- `claude -p --json-schema` trả `structured_output` + `total_cost_usd` +
  `session_id` trong JSON kết quả → dùng làm hợp đồng cho triage. `--tools ""`
  chạy được (0 tool).
- **Subprocess `claude -p` vẫn nạp auto-memory của workspace** — một decision
  thật đã trích `MEMORY.md` làm evidence. Vì vậy có `src/redaction.mjs`: mọi
  auto-reply ra kênh ngoài bị quét rò trước khi gửi.
- Giá thật/1 item: sonnet $0.11, haiku $0.051 (chỉ ~2× vì token phần lớn là
  input/cache). Haiku có case suy diễn không dựa trên context → giữ sonnet.
- `devlog.sqlite` của `sdvi` và `tfl5` **rỗng, không có bảng `events`** (project
  ghi log ở nơi khác) → adapter coi là "chưa init", không phải lỗi.
- Một số project nằm **ở gốc workspace** chứ không dưới `AI/` (dwork, social,
  sso-user, uiux, video) → luôn dùng `projectDir()` trong `src/config.mjs`.
- `notifications` của GitHub với `type=CheckSuite` không có `subject.url` → không
  có body, không có author (`sender = github:<owner>/<repo>`); trust dựa vào
  owner của repo.
- Hook `bash-guardrail` của workspace chặn lệnh nào trông như "dò sandbox" (tôi
  bị chặn 1 lần khi tạo file thử tên `SECRET_MARKER`). Đừng đặt tên kiểu đó.
