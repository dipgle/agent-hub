# UC — quản lý phiên Claude CLI từ điện thoại

Sổ use case cho hướng đã chốt 2026-08-08: *hub là BE quản lý các phiên làm việc
`claude` CLI trên máy local; giao diện trên tfl5 để theo dõi và xử lý việc từ
điện thoại.*

**Chuẩn Hà đặt ra (08-08):** *"ui uc phải biết, phải nhìn thấy giống như đang
ngồi máy"*. Vậy vạch đích **không** phải "xem tóm tắt phiên" mà là **thấy đúng
thứ terminal đang hiện**: lời nói, suy nghĩ, từng lệnh chạy và kết quả của nó,
file bị sửa, lúc nào phiên xin quyền.

Luồng kỹ thuật: `fe/flow.html` (bundle v30). Hướng và lý do: `memory/active-context.md`.

## Nguyên liệu đã có — đo trên một phiên thật (08-08)

Tệp `~/.claude/projects/<slug>/<sessionId>.jsonl` chứa sẵn mọi thứ cần để dựng lại
màn hình terminal:

| Khối | Số lượng trong MỘT phiên | Dùng để hiện |
|---|---|---|
| `tool_use` | **198** | từng lệnh + tham số |
| `tool_result` | **198** | kết quả từng lệnh |
| `thinking` | **115** | suy nghĩ |
| `text` | 45 | lời nói |
| `permission-mode` | 48 | chế độ quyền đang bật |
| `file-history-snapshot` / `-delta` | 10 / 10 | file bị sửa |
| `queue-operation` | 10 | tin đang xếp hàng |

⟹ **Không thiếu dữ liệu, chỉ thiếu phần vẽ.** Công cụ hay chạy nhất trong phiên
đó: `Bash` 153 · `Edit` 27 · `Write` 8 · `Read` 8.

**Quy ước trạng thái** — ba cột khác nhau, đừng gộp:

| Cột | Nghĩa |
|---|---|
| **Cơ chế** | ✅ đã chạy thật · 📄 có tài liệu nhưng CHƯA chạy lần nào · ❓ chưa có đường |
| **Sản phẩm** | hub đã làm phần này chưa |
| **Kịch bản** | có `*-uc.mjs` chạy trên UI thật chưa |

Luật workspace áp nguyên: **nghiệm thu phải đi qua UI thật**. Gọi tay
`claude agents` chỉ là *thăm dò*.

---

## UC-S01 · Xem mọi việc đang chạy

**Ai:** chủ máy, đang ở ngoài, mở điện thoại.

**Luồng chính:** mở app hub → đăng nhập → màn đầu là **danh sách phiên**, vừa động
nằm trên; mỗi dòng có tên phiên · tài khoản · loại · động cách đây bao lâu · dự án.

**Nghiệm thu:**
- Số dòng **bằng** số phiên trong `hub sessions --json` cùng lúc.
- Phiên của **cả 3 tài khoản** đều có mặt, phân biệt được.
- Ở 390 px: 0 lỗi console và **dòng đầu tiên nằm trong màn đầu** (bài học 08-08: bảng cũ đẩy việc xuống 90% chiều cao màn).

**Cơ chế:** ✅ 14 phiên, 3 tài khoản · **Sản phẩm:** ✅ **XONG** — màn "Phiên" là
tab mặc định, bundle **v35** · **Kịch bản:** ✅ `fe-sessions-uc.mjs` **8/8**.

Nghiệm thu thật ở 390 px (2026-08-08): 14/14 phiên khớp `hub sessions --json` ·
đủ 3 tài khoản · phiên vừa động nằm trên · phiên đầu ở **321 px** (nửa trên màn) ·
không tràn ngang · phiên bị ẩn nói rõ lý do và **không** hiện nội dung · 0 lỗi
console. Không hồi quy: `fe-smoke` 15/15, `fe-board-uc` 41/41.

⚠ **Bẫy phép đo, lần thứ ba trong dự án này:** lần chạy đầu assert "phiên đầu nằm
trong màn đầu" **xanh ở 0 px** — vì panel đang ẩn nên bbox bằng 0. Chỉ khi sửa tab
mặc định thành "Phiên" nó mới đo được thật: **570 px**, tức 67% màn là vỏ. Ẩn vỏ
hộp việc (nút quản trị, đoạn giải thích lệnh, chip thống kê) + bỏ app tid 38 ký tự
trên header ⇒ còn 321 px. *Assert xanh trên một phần tử đang ẩn là assert mù.*

---

## UC-S02 · Xem một phiên **như đang ngồi máy**  ★ UC xương sống

**Muốn:** mở một phiên ra và thấy **y như màn terminal**, không phải bản tóm tắt.

**Luồng chính:**
1. Từ UC-S01 chạm vào một phiên.
2. Hiện **toàn bộ luồng** theo thứ tự thời gian:
   - lời nói của mình và của Claude,
   - **từng lệnh** (`Bash`, `Edit`, `Read`…) kèm tham số,
   - **kết quả** của từng lệnh, cắt gọn nhưng mở rộng được,
   - suy nghĩ (thu lại mặc định, bung ra khi cần),
   - file đã sửa.
3. Cuộn ngược đọc lại được; mở ở đáy như terminal.

**Nghiệm thu (đo, không phải "nhìn ổn"):**
- Với một phiên chọn sẵn: **số khối `tool_use` trên màn == số trong tệp**; tương tự `tool_result`.
- Một lệnh bất kỳ: tham số và kết quả trên màn **khớp từng chữ** với tệp.
- Suy nghĩ có mặt nhưng không chiếm màn khi chưa bung.
- Phiên 12 MB vẫn mở được trên điện thoại (phải cửa sổ hoá, không nạp cả tệp).

**Cơ chế:** ✅ · **Sản phẩm:** ✅ **XONG** (bundle **v38**) · **Kịch bản:** ✅
`fe-stream-uc.mjs` **11/11**.

Đường đi: chạm phiên → trang gửi **`/session <uuid>`** vào phòng chat → hub ghi
cursor `focus:session` → ảnh chụp kế tiếp mang luồng **của riêng phiên đó**
(đẩy transcript mọi phiên mỗi cycle là megabyte cho một màn duy nhất ai đó đang
đọc). `parse_stream` tách từng khối thành sự kiện **say · tool · result · think**;
cửa sổ **120 sự kiện**, nói rõ `older_hidden`. Quay lại thì gửi `/session -` để
hub thôi gánh luồng không ai đọc.

Nghiệm thu thật ở 390 px: 90/90 sự kiện khớp ảnh chụp đọc độc lập · **36 lệnh +
37 kết quả** hiện đủ, lệnh kèm tham số (`{"command":"rm -f workspace-web/…"}`),
kết quả có nội dung thật · không tràn ngang · quay lại thì hub **thôi theo**.

⚠ **Ba bẫy, hai là phép đo và một là lỗi thật:**
1. `waitForFunction(fn, {timeout})` — tham số thứ hai của Playwright là **đối số
   truyền vào hàm**, không phải options ⇒ 180 s âm thầm rơi về 30 s. Sổ đã ghi
   bẫy này từ 08-07 mà vẫn đạp lại; nay có comment ngay tại chỗ.
2. So bằng số sự kiện trên phiên **đang chạy** là sai theo cấu trúc: nó ghi tiếp
   và cửa sổ 256 KB trượt ⇒ hai lần đọc lệch nhau (84 vs 81). Kịch bản nay chọn
   phiên **đứng yên > 30 phút**.
3. **Lỗi thật:** trang chỉ `loadBoard()` **một lần** sau 6 s — phiên nào hub đẩy
   chậm hơn thế thì màn treo vĩnh viễn. Nay có vòng chờ 5 s, **hạn 2 phút**, hết
   hạn thì nói rõ lý do thay vì quay mãi.

### UC-S02b · Phiên có subagent thì hiện thế nào  ⚠️ chưa có mẫu thật

**Đã kiểm được:**
- Mọi bản ghi đều mang cờ **`isSidechain`** (678/678 đang `false`) cùng `uuid`/`parentUuid` nối thành cây ⟹ định dạng **đã chừa chỗ** cho lượt của subagent, nhiều khả năng ghi vào **cùng tệp** của phiên cha.
- **Chưa phiên nào trên máy này từng chạy subagent**: quét toàn bộ nhật ký → **0** tệp có `"isSidechain":true`, **0** tệp gọi `Task`/`Agent`. Nên đây là suy luận từ định dạng, **không phải quan sát**.
- `claude agents --json` trả về `{pid, cwd, kind, sessionId, name}` — **không có trường cha/con** ⟹ subagent sẽ không hiện ra như một mục riêng để nhắm tới.

**Nói chuyện trực tiếp với subagent: chưa có đường.** Mọi lối vào một phiên
(`-p --resume <sessionId>`, stdin) đều nói với **phiên cha**; không thấy primitive
nào trong `claude --help` / `claude agents --help` gửi thẳng vào một sub đang
chạy. Về thiết kế cũng nên vậy: sub là **công cụ của cha**, chen ngang sẽ phá
chính vòng điều phối đó.

**Quyết định tạm cho UI:** hiện **cây** — phiên cha, dưới là nhánh subagent, **đọc
được hết** (ngồi máy thì cũng thấy sub chạy); **gõ thì gõ vào cha**, và màn phải
nói rõ đang nói với cha, không giả vờ đang nói với sub.

**Thí nghiệm để chốt:** chạy một phiên có subagent thật rồi xem tệp ghi gì — cờ
`isSidechain` có bật không, có tách tệp riêng không, `claude agents` có thấy
không. Chưa chạy.

---

## UC-S03 · Thấy gần như tức thì

**Muốn:** đang nhìn thì thấy nó chạy tiếp, không phải chờ.

**Vì sao thành UC riêng:** "giống như ngồi máy" là yêu cầu **về độ trễ** chứ không
chỉ về nội dung. Ảnh chụp hiện đẩy mỗi `poll_interval_sec = 120` — chậm hơn hai
bậc so với cảm giác ngồi trước terminal.

**Nghiệm thu:** gõ một lệnh trên máy → màn điện thoại đổi trong **vài giây**, không phải vài phút.

**Đường đi có sẵn:** tệp nhật ký được **ghi nối liên tục**; hub đã có socket
thường trực với phòng chat (`src/live.rs`) để đẩy ngay thay vì chờ chu kỳ.

**Cơ chế:** ✅ · **Sản phẩm:** ✅ **XONG** (bundle **v39**) · **Kịch bản:** ✅
trong `fe-stream-uc.mjs` **12/12**.

Hai nửa, phải có cả hai:
- **Phía hub:** `follow_sleep` cắt giấc ngủ chu kỳ thành lát **2 giây**; mỗi lát chỉ
  đọc **mtime** của đúng tệp phiên đang theo — không gọi `claude`, không chạy
  pipeline, không tốn tiền. Tệp đổi thì đẩy ngay, có sàn **4 giây** giữa hai lần
  để một phiên bận không biến thành cơn lũ đẩy. Tin chat tới thì trả quyền lại
  cho vòng chính chạy trọn chu kỳ.
- **Phía trang:** đang mở phiên thì tự hỏi lại mỗi **4 giây**, và **giữ chỗ cuộn** —
  chỉ bám đáy khi người đọc đang ở đáy, đúng như terminal.

**Đo thật (2026-08-08):** khi phiên hoạt động → **8 lần đẩy trong ~50 giây**, cách
nhau **4–17 giây** (trước đây: **120 giây**). Khi phiên đứng yên → **0 lần đẩy**
trong hai cửa sổ 42 giây, tức không đẩy rác. Trang tự làm mới **5 lần/15 giây**.

⚠ **Hai lần tôi đo sai trước khi đo đúng:** (1) ghi thẳng cursor `focus:session`
bằng SQL **không đánh thức** daemon, nên nó ngủ nốt 120 giây của chu kỳ cũ — đường
thật là verb `/session` qua phòng chat, và verb đó mới đánh thức. (2) Cửa sổ đo
đầu tiên có 0 lần đẩy vì **phiên bị theo dõi chính là phiên đang đo**, mà lúc đó
nó chỉ ngồi `sleep` nên chẳng ghi gì. Số 0 đó là **đúng**, không phải hỏng.

---

## UC-S04 · Biết phiên đang chạy dưới chế độ quyền nào  ✅ **đổi định nghĩa 08-08**

**Câu hỏi cũ ("thấy lúc phiên xin quyền") không có câu trả lời, và cũng không có
câu hỏi.** Quét **~14.000 bản ghi** trong 12 nhật ký: thứ duy nhất dính quyền là
`permission-mode`, với đúng **hai** giá trị — `auto` **648** lần, `dontAsk` **76**
lần. Không có bản ghi nào cho một lần hỏi đang treo, và **không thể có**: mọi
phiên trên máy này chạy chế độ tự duyệt, tức **chúng không hỏi**.

⟹ Đổi sang thứ có thật và quan trọng hơn: **hiện chế độ quyền của từng phiên**.
Đứng ngoài đường mà không biết một phiên đang chạy **không-hỏi-ai** thì nguy hiểm
hơn hẳn việc không nhìn thấy gì.

**Đã làm (bundle v42):** `LiveSession.permission_mode` đọc từ đuôi nhật ký; thẻ
phiên hiện *tự duyệt · không hỏi · hỏi trước*. Đo thật: **5/14** phiên đọc được
`auto`, **9 phiên "(chưa rõ)"** vì cửa sổ 256 KB không chứa bản ghi mode — hiện
đúng là *chưa rõ*, **không đoán**.

**Cơ chế:** ✅ · **Sản phẩm:** ✅ · **Kịch bản:** ✅ trong `fe-sessions-uc.mjs` 9/9.

<details>
<summary>Câu hỏi cũ, giữ lại để không ai đi lại đường này</summary>

## (cũ) Thấy lúc phiên xin quyền / đang chờ mình  ⚠️ chưa đủ dữ kiện

**Muốn:** phiên dừng lại hỏi "có chạy lệnh này không?" thì trên điện thoại phải
thấy — vì ngồi máy sẽ thấy.

**Chỗ chưa biết:** nhật ký có `permission-mode` (**chế độ** đang bật) nhưng chưa
xác định được nó có ghi **lần hỏi quyền đang treo** hay không. Và bản ghi cuối của
14 phiên thật gồm đủ loại (`assistant`, `pr-link`, `attachment`, `last-prompt`,
`system`) nên **không suy ra trạng thái chờ** từ đó.

**Việc phải làm trước khi hứa:** dựng một phiên chạm đúng lần hỏi quyền, xem tệp
ghi gì. Chưa có kết quả thì **không vẽ chỉ báo "đang chờ"** lên màn — sai một lần
là mất tin cả bảng.

**Cơ chế:** ❓ · **Sản phẩm:** chưa · **Kịch bản:** chưa.
</details>

---

## UC-S05 · Nói tiếp vào một phiên đang có

**Luồng chính:** từ UC-S02 gõ vào ô nhập của phiên đó → hub chạy
`claude -p --resume <sessionId>` trên máy → kết quả chảy về màn.

**Nghiệm thu:**
- Phiên nhận đúng ngữ cảnh cũ (hỏi câu chỉ trả lời được nếu nhớ lượt trước).
- `last_activity` nhảy lên mới nhất; **không** đẻ phiên mới ngoài ý muốn.

**Cơ chế:** ✅ `--resume` (hub đã dùng cho trí nhớ triage).
⚠ Ba bẫy đã ghi: `--no-session-persistence` loại trừ `--resume`; phiên đã resume
**giữ nguyên system prompt lúc tạo**; phiên mất phải **thoái lui**, không được làm
chết luồng. `--fork-session` chỉ dùng khi cố ý tách nhánh.
**Sản phẩm:** chưa · **Kịch bản:** chưa.

---

## UC-S05b · Chen ngang hỏi mà **không phá ngữ cảnh đang làm**  ★ Hà yêu cầu 08-08

Ngồi máy thì gõ chen vào lúc nào cũng được. Trên điện thoại phải có đúng lựa chọn
đó, và phải nói rõ nó ảnh hưởng tới phiên đang chạy như thế nào.

**Ba mức, khác nhau hẳn — màn hình phải cho chọn, không được tự quyết hộ:**

| Mức | Cơ chế | Việc đang chạy | Ngữ cảnh |
|---|---|---|---|
| **1. Xếp hàng** | gõ khi phiên bận → `queue-operation: enqueue` → tới ranh giới lượt thì nhận dưới dạng `attachment { type: "queued_command" }` | **không cắt** | câu hỏi thành một lượt của **chính phiên đó** |
| **2. Hỏi bên lề** | `-p --resume <id> --fork-session` | **không đụng** | có **toàn bộ** ngữ cảnh tới thời điểm đó, nhưng trả lời sang **session id mới** ⇒ phiên gốc nguyên vẹn |
| **3. Cắt ngang thật** | chưa thấy primitive | cắt | chính là cái **phá** ngữ cảnh — không làm |

**Bằng chứng cho mức 1** (dựng lại từ phiên tfl5, 2026-08-08):

```
00:38:09  queue-operation enqueue  "kiểm tra: bug pattern=… ở màn Releases"
00:38:12  assistant <nghĩ>          ← phiên KHÔNG bị cắt
00:38:13  assistant <Bash>          ← vẫn chạy tiếp
00:38:15  queue-operation remove    ← lấy ra ở ranh giới lượt
00:38:09  attachment { type:"queued_command", prompt:"kiểm tra: bug pattern=…",
                       commandMode:"prompt", origin:{kind:"human"} }
```

Mốc thời gian của `attachment` **giữ nguyên lúc gõ** (00:38:09), không phải lúc
nhận ⟹ dựng lại được đúng thứ tự trên màn: hiện *“đang xếp hàng”* ngay khi gõ,
đổi thành *“đã nhận”* khi `remove`.

**Bằng chứng cho mức 2** — chạy thật 2026-08-08, phiên `0172a51b-…`:

```
TRƯỚC        15222 byte · 11 dòng
lệnh         claude -p --resume 0172a51b-… --fork-session
session_id   5750d578-78fc-4c12-a0e1-d9c135fd4a16   ← KHÁC id truyền vào
trả lời      "…chưa có chủ đề — bạn mới chỉ chào 'alo'…"  ← CÓ ngữ cảnh gốc
SAU          15222 byte · 11 dòng                   ← phiên gốc KHÔNG đổi
chi phí      $0.0826
```

Ba điều cùng đúng một lúc: **có** ngữ cảnh phiên gốc · **không** thêm lượt nào vào
phiên gốc · sinh **session id mới**. Đó chính là "chen ngang hỏi mà không phá ngữ
cảnh đang làm".

**Trạng thái thật:**
- Mức **2 (fork)** — ✅ **đã chạy thật**, kết quả ở trên. Chỉ là một lần gọi
  `claude -p --resume <id> --fork-session`, không cần chen vào tiến trình nào.
  ⟹ **đây nên là nút mặc định**, vì nó an toàn tuyệt đối với việc đang chạy.
  ⚠ Ba điều kèm theo: bản fork là **ảnh chụp đông cứng** (phiên gốc chạy tiếp thì
  nó lạc hậu) · điều fork biết thì **phiên gốc không biết** (muốn đưa về phải dùng
  mức 1) · fork **đẻ ra session id mới** nên UI phải gắn nhãn "hỏi bên lề" và gom
  dưới phiên cha, không thì danh sách rác dần.
- Mức **1 (xếp hàng)** — cơ chế ✅ có thật và ghi lại đầy đủ, **nhưng** hàng đợi là
  của tiến trình đang chạy: hub chưa có đường gửi vào một phiên `interactive`
  (đúng cái nhánh đã bỏ 08-08). Với phiên **do hub nuôi** (`--bg`) thì **chưa thử**.
- Mức **3** — không có primitive, và cũng không nên có.

**Nghiệm thu:**
- Bấm “hỏi bên lề” → trả lời đúng ngữ cảnh phiên gốc, và phiên gốc **không thêm
  lượt nào** (`last_activity` không đổi, số bản ghi không tăng).
- Bấm “gửi vào phiên” → màn hiện *đang xếp hàng* → *đã nhận*, và việc đang chạy
  **không đứt quãng** (không có lượt nào bị bỏ dở).

### Đã dựng — mức 2 (bundle v43, 2026-08-08)

Ô hỏi + nút **💬 Hỏi** nằm ngay trên màn luồng phiên; gõ → trang gửi verb
**`/ask <câu hỏi>`** vào phòng → hub fork phiên đang theo, trả lời về màn kèm
nhãn *"phiên gốc không thêm lượt nào"* và số tiền của chính lần hỏi đó.
Đích là **phiên đang theo**, không phải một uuid gõ tay — hỏi người dùng chép
uuid trên điện thoại là hỏi họ đừng dùng tính năng.

🔒 **Hàng rào là cấu trúc, không phải lời dặn.** Bản fork chạy với allowlist
`Read,Grep,Glob` (`sessions::FORK_TOOLS`) — hỏi chính nó thì nó liệt kê đúng
`Glob · Grep · Read`, tức không có tay để ghi. Ba phép đo 08-08 loại hai phương
án nghe hợp lý: `--tools ""` **hỏng** trên phiên đầy vết dùng công cụ
(*"tool call could not be parsed"*), còn `--disallowedTools` mà không kèm
allowlist thì **nạp cả schema công cụ**, một câu hỏi tốn **$0.2185** và vỡ trần
$0.20. Allowlist vừa là hàng rào vừa là đòn bẩy giá (cùng câu hỏi: **$0.0356**).
⚠ `handover` trước đây chạy **không có** bộ khoá này — nay đi chung `fork_call`.

💰 Cùng cổng ngân sách với `/handover`: `owner_daily_budget_usd`, từ chối theo
**trường hợp xấu nhất**. Ảnh chụp nay công bố `owner_budget.blocks_owner_action`
để kịch bản đọc **kết luận của sản phẩm** thay vì tự suy lại luật.

**Cơ chế:** ✅ fork · ✅ hàng đợi (quan sát) · **Sản phẩm:** ✅ mức 2 (bundle v43) ·
**Kịch bản:** ✅ `fe-aside-uc.mjs` — nhánh từ-chối 10/10 · ⏳ nhánh thành công
**chưa chạy được**: trần chủ máy đã cạn ($1.723 + $0.50 > $2.00) đúng hôm dựng.
Mức 1 (xếp hàng) và mức 3: vẫn như trên.

---

## UC-S06 · Mở phiên mới cho một dự án

**Luồng chính:** chọn thư mục dự án + gõ yêu cầu → hub chạy `claude --bg` với
`cwd` đó → phiên mới xuất hiện ở UC-S01.

**Nghiệm thu:** phiên mới có `kind = background`, đúng `cwd`, hiện lên **mà không
cần** thao tác nào trên máy tính. Mỗi phiên nền là tiền — phải đếm như mọi đường chi.

**Cơ chế:** 📄 `claude --bg` có tài liệu, **chưa chạy lần nào** — phải thử một lần trước khi thiết kế.
**Sản phẩm:** chưa · **Kịch bản:** chưa.

---

## UC-S07 · Đóng sổ để mở phiên mới làm tiếp  ★ Hà định nghĩa lại 08-08

*"Tắt terminal hiện tại mà không mất luồng công việc đang xử lý dở, để mở phiên
mới làm tiếp."* ⟹ **không phải bài toán giết tiến trình.** Mạch việc nằm ở nhật
ký trên đĩa, nên tắt terminal chẳng mất gì; cái cần là **bản bàn giao** và **id
để nối tiếp**.

| bước | việc | trạng thái |
|---|---|---|
| 1 | **Đóng sổ** — phiên tự viết bàn giao 4 mục | ✅ **XONG** (bundle v40) |
| 2 | **Nối tiếp** — id mới giữ nguyên ngữ cảnh | ✅ **XONG** |
| 3 | Tắt terminal | tuỳ chọn — người dùng tự đóng |

**Cách làm:** verb `/handover <id>` → hub chạy `claude -p --resume <id>
--fork-session` với prompt bàn giao (đang làm gì · đã xong tới đâu kèm `file:dòng`
· đang kẹt gì · việc kế). **Fork** nên phiên gốc **không bị sửa một byte nào** —
đóng sổ không được phép làm hỏng chính cái thread nó đang mô tả.

**Chạy thật (08-08)** trên `fix-deploy-verify-hash`: id mới `57dc5d73` ≠ nguồn ·
bản bàn giao có nội dung dùng được (nêu cả sha bundle và tên script kiểm) ·
`resume_command` = `cd … && claude --resume 57dc5d73…`.

### 💸 Nhưng nó lộ một lỗ tiền, đã vá trong cùng lượt

Lần chạy đầu tốn **$1.7228 trong MỘT lần gọi** (resume nạp cả phiên 986 KB), đẩy
chi hôm nay lên **$4.701 / trần $3.00 — vượt 57%**. Ba lỗ, đều đã bịt:

1. **Trần ngày không nhìn thấy đường chi này.** `cost_on_day` chỉ cộng
   `decisions`. Nay có bảng **`spend`** và trần cộng cả hai (`SCHEMA_VERSION 3`).
   Trước khi vá, một đường chi mới là **hoàn toàn vô hình** với chính cái trần
   sinh ra để chặn nó.
2. **Gác kiểu `spent >= cap` chỉ từ chối lần SAU** — một lần gọi không chặn thì
   vượt bao nhiêu cũng được. Nay từ chối khi `spent + max_budget_usd > cap`, tức
   theo **trường hợp xấu nhất**.
3. **Lệnh handover không truyền `--max-budget-usd`.** Nay có, chặn ngay trong
   một lần gọi.

**Cơ chế:** ✅ · **Sản phẩm:** ✅ · **Kịch bản:** ✅ trong `fe-stream-uc.mjs`
**13/13** — kiểm **cả hai nhánh**: đủ tiền thì phải ra bản bàn giao + id mới +
tiền vào sổ; hết tiền thì **không được sinh bản mới** và phải nói rõ.
⚠ Hôm nay chỉ nghiệm thu được nhánh **từ chối** (đã vượt trần); nhánh thành công
đã chạy thật một lần trước khi vá, số liệu ở trên.

---

## UC-S08 · Bí mật không rò ra trang

**Luồng chính:** phiên có lượt chứa chuỗi đăng nhập → màn hiện **lý do bị ẩn**,
không hiện nội dung; muốn xem thì mở phiên trên máy.

⚠ UC-S02 làm UC này **khó hơn hẳn**: hiện cả `tool_result` nghĩa là hiện đầu ra
lệnh — nơi khoá và biến môi trường hay xuất hiện nhất. Cổng quét phải chạy trên
**từng khối**, không chỉ trên lượt cuối.

**Nghiệm thu:**
- Chuỗi đăng nhập tiếng Việt **và** tiếng Anh đều bị ẩn.
- Dòng lập trình bình thường (`/Users/…`, IP, "blocker") **không** bị ẩn — gate quá tay thì màn trống và người dùng bỏ qua nó.

**Cơ chế:** ✅ đo trên 14 phiên: ẩn 1 · hiện 12 · sót 0 (mới ở mức lượt cuối) ·
**Sản phẩm:** chặn tại nguồn · **Kịch bản:** chưa (mới có unit test).

---

## UC-S09 · Khi hub chết hoặc mất mạng

**Nghiệm thu:** dừng `hubd` → trang nói rõ ảnh chụp **cũ bao lâu**, không hiện số
liệu cũ như thật. Một tài khoản lỗi → hiện đúng "tài khoản này không trả lời",
không im lặng bỏ qua phiên của nó.

**Cơ chế:** ✅ `notes[]` + mốc thời gian trên ảnh chụp · **Sản phẩm:** dữ liệu xong, giao diện chưa · **Kịch bản:** chưa.

---

## Đọc bảng này thế nào

**Không UC nào có kịch bản tự động.** Xương sống là **UC-S02** — và đo theo chuẩn
"giống như ngồi máy" thì phần đã làm hôm nay (lấy **một** lượt cuối) mới là mảnh
nhỏ của nó. UC-S03 (độ trễ) đi kèm S02, vì thấy đủ mà chậm hai phút thì vẫn không
giống ngồi máy.

Thứ tự đề xuất: **S02 + S03** (thấy đủ, thấy ngay) → S01 hoàn thiện → S08 nâng lên
mức từng khối → thử `--bg` cho S06 → điều tra S04 → S07 sau cùng.
