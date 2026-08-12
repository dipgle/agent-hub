# UC — quản lý phiên Claude CLI từ điện thoại

Sổ use case cho hướng đã chốt 2026-08-08: *hub là BE quản lý các phiên làm việc
`claude` CLI trên máy local; giao diện trên tfl5 để theo dõi và xử lý việc từ
điện thoại.*

**Chuẩn Hà đặt ra (08-08):** *"ui uc phải biết, phải nhìn thấy giống như đang
ngồi máy"*. Vậy vạch đích **không** phải "xem tóm tắt phiên" mà là **thấy đúng
thứ terminal đang hiện**: lời nói, suy nghĩ, từng lệnh chạy và kết quả của nó,
file bị sửa, lúc nào phiên xin quyền.

Kiến trúc và *vì sao* nó có hình dạng này: `fe/flow.html` (ảnh chụp 8/8, KHÔNG phải
bảng trạng thái). Hướng và lý do: `memory/active-context.md`.

> **Đọc mọi con số `$` trong sổ này thế nào (chốt 2026-08-09).** Máy này chạy
> gói **Max** (`claude auth status` → `subscriptionType: max`): **không có hoá
> đơn tính theo từng lần gọi**. Con số `total_cost_usd` mà CLI trả về được quy
> theo **giá API niêm yết**, nên mọi `$…` dưới đây là **thước đo độ lớn của một
> cú gọi**, không phải tiền bị trừ khỏi tài khoản Hà. Cái thật sự bị tiêu là
> **hạn mức của gói** — đúng như khi ngồi gõ ở terminal. (Trần mỗi-lần-gọi vẫn
> có tác dụng, vì CLI tự tính đúng con số ấy bất kể gói nào.)
>
> **Vì sao vẫn giữ các con số ấy.** Chúng là **bằng chứng đo thật** của những lần
> chạy đã diễn ra — giữ vì chúng trả lời được "cú này to bao nhiêu" khi cần. Nhưng **sản phẩm không còn
> hiện đồng nào**: không trần chặn tay chủ máy, không giá cạnh câu trả lời,
> không tab Chi phí, không dải tổng chi. Hà, hai nhịp trong một ngày: *"bỏ hết
> github rồi sao vẫn trần chuồng gì thế"* → gỡ trần; *"sao vẫn nhắc tới tiền
> vậy"* → gỡ nốt giá. Sổ `spend` vẫn ghi, im lặng. Chỗ nào trong sổ này còn tả
> "màn hiện giá" là mô tả một bản đã bị thay.

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

### UC-S02b · Phiên có subagent thì hiện thế nào  ✅ chạy thật 2026-08-10

**Thí nghiệm đã chạy** (nợ từ 08-09: *"lúc nghiệm thu không phiên nào đang chạy
subagent"*). Tung subagent thật trong một phiên đang sống rồi mở màn hình xem.

**Nhật ký ghi thế nào — quan sát, không còn suy luận:**

| Thứ | Sự thật đo được |
|---|---|
| Tệp của subagent | **tách riêng**: `<slug>/<session_id>/subagents/agent-<agentId>.jsonl`, mọi bản ghi `isSidechain: true` |
| Móc ngược về lệnh gọi | `agent-<agentId>.meta.json` mang `{agentType, description, **toolUseId**, spawnDepth, model}` |
| Trường `sessionId` trong tệp con | là id của **phiên CHA**, không phải id riêng |
| `claude agents` | **không** liệt kê subagent như một mục riêng (đúng như dự đoán cũ) |
| Tiến trình | subagent nền **không** là tiến trình riêng — `ps` không thấy con nào; chúng chạy trong chính tiến trình `claude` |

**Cái bẫy đã sập, và là lý do UC này đáng nợ tới hôm nay:** subagent **chạy nền**
nhận `tool_result` **ngay lập tức** (nội dung chỉ là "đã tung agent"), nên phép
khớp `tool_use ↔ tool_result` báo nó xong đúng lúc nó vừa bắt đầu. Đo lúc 14:22:
hai agent đang chạy thật mà `hub sessions` khai `pending 0`. Và chính chế độ nền
mới là chế độ con số này sinh ra để bắt — agent chặn thì phiên cha đang bận nhìn
là biết, agent nền thì phiên cha rảnh tay, từ điện thoại nhìn y như treo.

Dấu kết thúc đúng của lệnh gọi nền là khối `<task-notification>` mà CLI chèn vào
nhật ký phiên cha, mang `<tool-use-id>` **đúng bằng id của lệnh gọi ban đầu**
(`toolu_01C7bc…` khớp cả hai đầu) — nên đường khớp vẫn theo ID, không theo tên.

**Nghiệm thu trên màn thật** (`fe-subagent-uc.mjs`, 6/6, bundle v130): thẻ trong
danh sách ghi `1 subagent đang chạy`; màn chi tiết có hàng `subagent · 1 đang
chạy`; và **chiều âm**: 4 phiên không chạy subagent thì cả hai chỗ đều không nhắc
tới nó. Nguồn sự thật là **bản đếm lại bằng JS đọc thẳng tệp**, không hỏi
`hub sessions` — so màn với hub chỉ chứng minh trang vẽ trung thành, không chứng
minh hub đếm đúng. Không phiên nào đang chạy subagent thì kịch bản **thoát mã 2**
và nói "chưa dựng được trạng thái cần đo", không đếm là đạt.

**Nói chuyện trực tiếp với subagent: vẫn chưa có đường.** Mọi lối vào một phiên
(`-p --resume <sessionId>`, stdin) đều nói với **phiên cha**; không thấy primitive
nào trong `claude --help` / `claude agents --help` gửi thẳng vào một sub đang
chạy. Về thiết kế cũng nên vậy: sub là **công cụ của cha**, chen ngang sẽ phá
chính vòng điều phối đó.

**Còn nợ trong UC này:** con số chỉ đúng khi lệnh gọi còn nằm trong cửa sổ 256KB;
và một agent nền được **đánh thức lại** sau khi đã báo xong thì hub đọc là "không
còn chạy" — cố ý thiếu còn hơn để một subagent ma chạy mãi trên màn.

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
  pipeline, không gọi `claude`. Tệp đổi thì đẩy ngay, có sàn **4 giây** giữa hai lần
  để một phiên bận không biến thành cơn lũ đẩy. Tin chat tới thì trả quyền lại
  cho vòng chính chạy trọn chu kỳ.
- **Phía trang:** đang mở phiên thì tự hỏi lại mỗi **4 giây**, và **giữ chỗ cuộn** —
  chỉ bám đáy khi người đọc đang ở đáy, đúng như terminal.

**Đo thật (2026-08-08):** khi phiên hoạt động → **8 lần đẩy trong ~50 giây**, cách
nhau **4–17 giây** (trước đây: **120 giây**). Khi phiên đứng yên → **0 lần đẩy**
trong hai cửa sổ 42 giây, tức không đẩy rác. Trang tự làm mới **5 lần/15 giây**.

⚠ **Hai lần tôi đo sai trước khi đo đúng:** (1) ghi thẳng cursor `focus:session`
bằng SQL **không đánh thức** daemon, nên nó ngủ nốt 120 giây của chu kỳ cũ — đường
thật là route `/session` qua phòng chat, và route đó mới đánh thức. (2) Cửa sổ đo
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

### Đo thật 2026-08-08 — bức tường là của CLI, không phải của hub

Nối vào một phiên **đang sống** bị `claude` từ chối thẳng:

> `Error: Session … is currently running as a background agent (bg). Use
> claude agents to find and attach to it, or add --fork-session to branch off a copy.`

⟹ "gõ vào phiên đang chạy" **không có đường nào** từ điện thoại: `attach` cần
terminal trên máy, còn fork là việc khác (UC-S05b mức 2, đã dựng).

Thứ **có** chạy: **dừng phiên trước, rồi nối tiếp**. Đo: `claude stop` → `-p
--resume` ⇒ **cùng session_id**, nhật ký **8.434 → 11.529 byte (16 → 22 bản ghi)**
— một lượt thật trên chính thread cũ, ngược hẳn với "hỏi bên lề".

⚠ Ba bẫy đã ghi: `--no-session-persistence` loại trừ `--resume`; phiên đã resume
**giữ nguyên system prompt lúc tạo**; phiên mất phải **thoái lui**, không được làm
chết luồng.

**Cách hub làm:** route `/tell <nội dung>` + ô nhập trên màn phiên, **chỉ hiện với
phiên do hub mở**. Đang `busy` thì từ chối kèm lý do thật và chỉ sang hai lối
khác (Dừng, hoặc hỏi bên lề). Denylist giữ nguyên như lúc mở phiên — nối tiếp một
việc không được lén cấp thêm quyền mà lượt đầu không có.

**Cơ chế:** ✅ đo thật · **Sản phẩm:** ✅ (`/tell`, `/stop`) ·
**Kịch bản:** ⏳ chưa nghiệm thu được qua UI — nó cần một phiên nền **do hub mở
và chạy được**, mà UC-S06 đang kẹt ở hộp thoại MCP (xem trên).

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
thước đo     $0.0826
```

Ba điều cùng đúng một lúc: **có** ngữ cảnh phiên gốc · **không** thêm lượt nào vào
phiên gốc · sinh **session id mới**. Đó chính là "chen ngang hỏi mà không phá ngữ
cảnh đang làm".

**Trạng thái thật:**
- Mức **1 (hỏi thẳng phiên đang sống)** — ✅ **có cửa, và cửa KHÔNG nằm ở CLI.**
  Sổ này từng ghi *"đã đo, không có cửa… ngừng theo đuổi cho tới khi CLI có
  đường"*, và câu ấy đúng về CLI: mặt lệnh chỉ có `agents · attach · logs ·
  stop`, `--resume` bị từ chối thẳng khi phiên còn chạy, `attach` đòi TTY. Cái
  sai là **kết luận rộng hơn phép đo**: cửa là **cửa sổ Terminal** — cùng đường
  `keys::type_into` mà `/type` đã đi. Từ 2026-08-11 hub gõ **`/btw <câu>`** vào
  phiên có cửa sổ (chính `claude` gợi ý đường này trên màn), chờ màn đổi + phiên
  thôi bận (trần 60s), rồi đọc câu trả lời về.
  Vì sao nó là đường ĐI TRƯỚC, không phải đường phụ: fork nạp lại TOÀN BỘ nhật
  ký — đo thật 0.99 MB → **1.72 đơn vị hạn mức cho MỘT câu hỏi**, đắt tới mức
  `fe-aside-uc` phải có cổng chặn và mặc định không gọi, mà *một tính năng không
  ai dám bấm thì coi như không có*. `/btw` hỏi phiên đang sống bằng ngữ cảnh đã
  nằm sẵn trong đầu nó, và còn biết cả việc đang dở giữa chừng — thứ chưa kịp
  vào nhật ký.
  📌 **Cái giá — ĐO LẠI cuối ngày 2026-08-11, và nó KHÁC điều mục này viết buổi
  sáng.** Câu cũ: *"phiên gốc CÓ thêm một lượt"*. Chạy thật trên `projects-ff`
  (Terminal.app, `ttys001`): `/btw` mở một **bảng bên** trong TUI, trả lời đầy
  đủ, đóng bằng Esc — và **không một byte nào vào nhật ký**; phiên ấy tới cuối
  ngày vẫn chưa có tệp `.jsonl`. Lời hứa đúng: **nhật ký không dài thêm, cái bị
  ăn là NGỮ CẢNH đang chạy** — thứ không nhìn thấy trên đĩa. Màn nói đúng chừng
  ấy, cả trước lẫn sau khi hỏi.
  ⚠ **Chưa đo:** phiên ĐÃ có nhật ký sẵn thì `/btw` có ghi thêm không. Phép đo
  chỉ chạy một ca — phiên trắng. Đừng suy rộng lần nữa.
  Màn không đọc được thì **KHÔNG gõ gì cả**; hết trần chờ thì rơi về fork chứ
  không bịa.
  ⚠ Ba cái bẫy của đường này, cả ba đều đã cắn một lần rồi mới vá (mỗi cái một
  test khoá bằng ảnh chụp màn hình THẬT):
  1. **`Esc to close` KHÔNG phải dấu "xong"** — nó hiện ngay từ lúc bảng mới mở,
     nên hub từng gửi về một bảng còn đang chạy chữ `✳ Answering…`. Dấu đúng là
     chân bảng **và** không còn `Answering`.
  2. **Câu hỏi dài bị TUI ngắt dòng**, nên "tìm dòng chứa câu hỏi" trượt sạch và
     câu trả lời trả về còn nguyên dòng lệnh `/btw …` ở đầu. Neo vào chữ `/btw`
     mà `claude` tự vẽ lại.
  3. **Bảng của lượt trước còn mở thì nuốt câu hỏi lượt này** (gõ vào là bảng
     đóng, không mở bảng mới) ⟹ chờ hết trần rồi rơi về fork. Nay dọn bảng cũ
     trước khi hỏi.
- Mức **2 (fork)** — ✅ **đã chạy thật**, kết quả ở trên. Chỉ là một lần gọi
  `claude -p --resume <id> --fork-session`, không cần chen vào tiến trình nào.
  ⟹ đường cho phiên **không gõ vào được** (phiên nền, phiên trong editor, phiên
  không tìm ra cửa sổ), vì nó an toàn tuyệt đối với việc đang chạy.
  ⚠ Ba điều kèm theo: bản fork là **ảnh chụp đông cứng** (phiên gốc chạy tiếp thì
  nó lạc hậu) · điều fork biết thì **phiên gốc không biết** · fork **đẻ ra
  session id mới** nên UI phải gắn nhãn "hỏi bên lề" và gom dưới phiên cha,
  không thì danh sách rác dần.
- Mức **3** — không có primitive, và cũng không nên có.

**Hai đường ⟹ HAI lời hứa ngược nhau, và phép đo phải chọn trước khi bấm.**
Điều kiện quyết định đường đi là `tty` + `host == "terminal"` (hub: `window_of`;
trang: `canType`). `fe-aside-uc.mjs` chốt `viaBtw` **trước** cú bấm rồi mới đo:

| | `/btw` (có cửa sổ) | fork (không gõ vào được) |
|---|---|---|
| lời hứa | trả lời tới từ CHÍNH phiên ấy; nhật ký KHÔNG dài thêm, ăn ngữ cảnh đang chạy | phiên gốc y nguyên byte |
| bằng chứng | byte · dòng · `last_activity` không đổi, VÀ câu trả lời sạch (không logo, không chân bảng, không `Answering`, không lặp lại dòng lệnh) | byte · dòng · mtime · `last_activity` không đổi |
| ảnh chụp | `new_session_id == source_id` | `new_session_id != source_id` |
| cổng hạn mức | **không áp** (một lượt, như tự gõ) | áp theo cỡ nhật ký |
| đã chạy thật | **21/21**, `projects-ff` trên `ttys001` (2026-08-11) | **10/10**, `projects-71` trong VS Code — bước gọi `claude` BỎ QUA vì 0.26 > trần 0.25 |

Điều kiện chọn đường nay là `can_type` — con số **hub tự đo** (hỏi Terminal.app
đang giữ những tty nào), không phải `tty && host == "terminal"` như trước. Khác
nhau ở đúng ca đã tốn tiền thật: phiên trong **terminal tích hợp của VS Code** có
tty đàng hoàng mà Terminal.app không biết nó, nên `/btw` lặng lẽ rơi về fork.

Chốt đường **sau** khi có câu trả lời thì phép đo chỉ là cái gương — hub trả về
gì nó cũng gật.

**Nghiệm thu:**
- Bấm “hỏi bên lề” trên phiên **không gõ vào được** → trả lời đúng ngữ cảnh
  phiên gốc, và phiên gốc **không thêm lượt nào** (`last_activity` không đổi, số
  bản ghi không tăng).
- Bấm “hỏi bên lề” trên phiên **có cửa sổ** → trả lời tới từ chính phiên ấy
  (`new_session_id == source_id`), nhật ký **không dài thêm**, câu trả lời là câu
  trả lời chứ không phải ảnh chụp màn hình, và **màn nói đúng cái nó ăn** (“ăn
  vào ngữ cảnh đang chạy”) cả trước lẫn sau khi hỏi.
- Bấm “gửi vào phiên” → màn hiện *đang xếp hàng* → *đã nhận*, và việc đang chạy
  **không đứt quãng** (không có lượt nào bị bỏ dở).

### Đã dựng — mức 2 (bundle v43, 2026-08-08)

Ô hỏi + nút **💬 Hỏi** nằm ngay trên màn luồng phiên; gõ → trang gửi route
**`/ask <câu hỏi>`** vào phòng → hub fork phiên đang theo, trả lời về màn kèm
nhãn *"phiên gốc không thêm lượt nào"*. (Bản đầu hiện thêm **giá của lần hỏi**;
gỡ ngày 08-08 — xem 💰 bên dưới.)
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

💰 **Không có trần nào chặn nút này, và cũng không còn con số tiền nào trên màn.**
Hà chốt 08-08, hai nhịp: *"bỏ hết github rồi sao vẫn trần chuồng gì thế"* /
*"liên quan gì tới tiền"* (⟹ gỡ trần), rồi *"sao vẫn nhắc tới tiền vậy, đã bảo
xóa hết github rồi mà"* (⟹ gỡ nốt **giá** đã hiện cạnh câu trả lời, dải tổng chi
ở hộp việc, và cả **tab Chi phí + hai biểu đồ**). Bấm nút trên điện thoại là **Hà
đang làm việc** — cùng việc, cùng mức tiêu hạn mức như gõ ở terminal, nơi không
ai dán bảng giá lên từng câu. Sổ `spend` **vẫn ghi**, im lặng, để còn trả lời được nếu có ngày ai
hỏi; nó chỉ thôi tự hỏi trên mọi màn. Ảnh chụp không mang `owner_spend`,
`owner_budget` hay `cost_days` nữa — và `portal.rs` có assert **đòi vắng mặt**,
vì thứ này đã mọc lại một lần rồi (trần → giá).
Trần mỗi-lần-gọi (bảo vệ khỏi cú chạy vượt tầm, không phải để hỏi tiền người dùng)
vẫn còn và **tự đo theo độ dài phiên** (`fork_cost_estimate`) chứ không mượn con
số cứng $0.50 của triage — con số ấy nhỏ hơn giá nạp của 13/14 phiên, mà cú chết
vì trần **vẫn tiêu hạn mức** (đã nạp xong nhật ký rồi mới bị chặn).

**Nghiệm thu THẬT (2026-08-08, bundle v45):** phiên `projects-cd` 0.47 MB —
ước tính **$0.83**, thực tế **$0.8735** (lệch 5%); phiên gốc **474.525 byte ·
246 dòng · mtime y nguyên**, `last_activity` không đổi; trả lời đến từ bản fork
`801d9c9d → a8723989` và **đúng ngữ cảnh gốc** (*"dựng multi-account cho Claude
CLI: 3 config dir + alias + symlink skills/plugins…"*); sổ chi $1.7228 → $2.5963.

**Cơ chế:** ✅ fork · ✅ hàng đợi (quan sát) · **Sản phẩm:** ✅ mức 2 (bundle v45) ·
**Kịch bản:** ✅ `fe-aside-uc.mjs` — chạy thật cả đường thành công.
Mức 1 (xếp hàng) và mức 3: vẫn như trên.

---

## UC-S06 · Mở phiên mới cho một dự án

**Luồng chính:** chọn thư mục dự án + gõ yêu cầu → hub chạy `claude --bg` với
`cwd` đó → phiên mới xuất hiện ở UC-S01.

**Nghiệm thu:** phiên mới có `kind = background`, đúng `cwd`, hiện lên **mà không
cần** thao tác nào trên máy tính. Mỗi phiên nền là tiền — phải đếm như mọi đường chi.

### Đã đo cơ chế (2026-08-08) — và nó không như tài liệu gợi ý

| Đo | Kết quả |
|---|---|
| `claude --bg "<việc>"` | ✅ trả id sau **1 giây**, phiên lên `claude agents` với `kind: background`, tên tự đặt theo việc |
| `--bg` + `-p` | ❌ **xung khắc**: *"--print never starts the interactive session that `claude agents` attaches to"* ⇒ prompt là **tham số vị trí** |
| `--disallowedTools` + prompt đứng sau | ❌ option **nhận nhiều giá trị**, nuốt luôn câu việc ⇒ phiên mở ra **không có việc gì** (`idle — send a prompt to start`). Prompt phải đứng **trước**. |
| `claude stop <uuid đầy đủ>` | ❌ *"No job matching"* — chỉ nhận **id ngắn 8 ký tự** |
| chi phí phiên nền | ❌ **không đọc được**: `claude agents` không có, nhật ký không ghi `costUSD`/`usage` |
| `status`/`state` | ✅ `busy`/`idle`/`done` — **chỉ có ở phiên nền** |

⟹ Yêu cầu *"phiên nền là tiền, phải đếm"* **không thực hiện được**. Thay bằng thứ
trung thực hơn: hiện **trạng thái thật** + **nút Dừng**. Đưa một con số bịa ra còn
tệ hơn không có số.

📌 **Sửa lại lần nữa (08-08, tối):** bản đầu còn kèm câu *"nó tiêu tiền trong lúc
chạy — hub không đọc được chi phí của nó"* ở ô mở phiên và trong lời hub trả lời.
Hà: *"sao vẫn nhắc tới tiền vậy"* ⟹ bỏ nốt. Lời nhắc còn lại đúng thứ cần làm:
**nhớ dừng khi xong**. Nói về tiền mà không đo được tiền thì chỉ là gieo lo lắng,
không phải thông tin.

### 🔴 Rào chắn thật: phiên nền trong workspace này KẸT ngay khi mở

Mọi dự án đều nằm dưới `~/Documents/projects/.mcp.json` (project-agent, vault),
nên phiên nền mở lên là dừng ở **hộp thoại duyệt MCP** — *"2 new MCP servers found
in this project… Space to select · Enter to confirm · Esc to reject all"* — chờ
một phím bấm mà điện thoại không gõ được. `state: blocked`, **không có nhật ký,
không làm gì**. `--strict-mcp-config` **không** gỡ được; `--mcp-config
'{"mcpServers":{}}'` cũng không (đã thử cả hai).

**Cách hub xử lý:** sau khi mở, đợi tới 14s xem trạng thái; nếu `blocked` thì
**dừng phiên đó và báo hỏng kèm cách gỡ**, chứ không báo "🚀 đã mở phiên" cho một
phiên chẳng bao giờ chạy.

**Cách gỡ hoá ra KHÔNG phải bắt người dùng duyệt** (chốt 2026-08-10, Hà hỏi
*"duyệt thế nào?"* và câu trả lời đúng là *"không cần"*). Hộp thoại ấy chỉ nổi
lên khi mở phiên trong một **thư mục con chưa được duyệt**. Mọi phiên trên máy
này vốn chạy từ **gốc workspace**, và cả ba tài khoản đã duyệt gốc ấy từ lâu
(`hasTrustDialogAccepted: true`). Nên `start_background` mở ở **gốc workspace**,
nói việc thuộc dự án nào trong ĐỀ BÀI (`[dự án] việc…`) thay vì đổi thư mục —
`claude` vẫn đọc `CLAUDE.md` của cả cây từ gốc (`sessions.rs:1811`).

**Cơ chế:** ✅ đo thật · **Sản phẩm:** ✅ · **Kịch bản:** ✅ `fe-newsession-uc.mjs`.

✅ **Đường THÀNH CÔNG đã chạy thật 2026-08-10** — bốn phiên nền do hub tự mở,
mỗi phiên chạy lệnh thật rồi trả lời:

| Phiên | Số lệnh | Câu cuối |
|---|---|---|
| `5602abc4` | 3 | *"[hub] `hub` là công cụ đưa các phiên Claude CLI…"* |
| `2254aec2` | 4 | *"[hub-act-demo] …"* |
| `4bbd6d74` | 5 | *"[hub-act-demo] `hub-act-demo` là repo git dùng-một-lần…"* |
| `f7926139` | 3 | *"[hub] `hub` là công cụ Rust chạy trên Mac…"* |

📌 Mục này từng ghi *"đường thành công cần Hà duyệt MCP một lần rồi chạy lại"* và
câu ấy sống sót nhiều ngày sau khi nó hết đúng — đủ lâu để một đội soi sổ đọc lại
và báo cáo nó như việc còn nợ, rồi tôi chép lại cho Hà. *Sổ lạc hậu không nằm im:
nó quay lại thành một việc không có thật.*

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

**Cách làm:** route `/handover <id>` → hub chạy `claude -p --resume <id>
--fork-session` với prompt bàn giao (đang làm gì · đã xong tới đâu kèm `file:dòng`
· đang kẹt gì · việc kế). **Fork** nên phiên gốc **không bị sửa một byte nào** —
đóng sổ không được phép làm hỏng chính cái thread nó đang mô tả.

**Chạy thật (08-08)** trên `fix-deploy-verify-hash`: id mới `57dc5d73` ≠ nguồn ·
bản bàn giao có nội dung dùng được (nêu cả sha bundle và tên script kiểm) ·
`resume_command` = `cd … && claude --resume 57dc5d73…`.

### 💸 Nhưng nó lộ một lỗ tiền, đã vá trong cùng lượt

Lần chạy đầu tốn **$1.7228 trong MỘT lần gọi** (resume nạp cả phiên 986 KB), đẩy
chi hôm nay lên **$4.701 / trần $3.00 — vượt 57%**. Ba lỗ, đều đã bịt:

1. **Sổ không nhìn thấy đường chi này.** `cost_on_day` chỉ cộng `decisions`. Nay
   có bảng **`spend`** (`SCHEMA_VERSION 3`) — trước khi vá, một đường chi mới là
   **hoàn toàn vô hình**.
2. **Lệnh handover không truyền `--max-budget-usd`** ⇒ một lần gọi tiêu bao
   nhiêu cũng được. Nay có trần mỗi-lần-gọi.

### 🔁 Rồi chiều cùng ngày, hai trong ba "bản vá" ấy bị gỡ

Hà bác tiền đề: *"bỏ hết github rồi sao vẫn trần chuồng gì thế"*. Sổ cho thấy
**$2.24 trong $2.98 tiền triage hôm đó là của github+devlog đã bị xoá** — cái
trần chặn tay Hà phần lớn là bóng ma. ⟹ **trần NGÀY không còn gác thao tác của
chủ máy** (chỉ còn đếm + hiện giá), và **trần mỗi-lần-gọi tự đo theo độ dài
phiên** (`fork_cost_estimate`) thay vì mượn $0.50 của triage.

**Nghiệm thu THẬT (bundle v46):** `projects-cd` → bản bàn giao mới, phiên
`632cdba2`, **$0.8610** vào sổ, `resume_command` chạy được. Đây là **lần đầu**
đường thành công của UC-S07 đi trọn qua UI — trước đó chỉ chạy tay bằng CLI.

**Cơ chế:** ✅ · **Sản phẩm:** ✅ · **Kịch bản:** ✅ `fe-stream-uc.mjs` **18/18**.
⚠ Vì không còn trần chặn, **mỗi lần chạy kịch bản này là một lần trả tiền thật**
— nên nó tự nhắm phiên có nhật ký ngắn nhất.

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

**Cách làm:** đầu trang nói **ảnh chụp cũ bao lâu**, không chỉ mốc giờ — một mốc
giờ trần trụi đọc như "bây giờ" với người không ngồi trừ nhẩm, và đó đúng là lúc
`hubd` chết thì màn vẫn trông như thật. Quá **5 phút** (chu kỳ là ~120s) thì đổi
sang chữ đỏ và nói thẳng *"Số dưới đây là của lúc đó, không phải bây giờ"*. Mốc
không đọc được cũng tính là cũ. Tài khoản lỗi vẫn hiện riêng ở `notes[]`.

**Cơ chế:** ✅ · **Sản phẩm:** ✅ (bundle v50) · **Kịch bản:** ✅ nửa "còn tươi"
trong `fe-newsession-uc.mjs` (nói đúng tuổi, không kêu oan) · ⏳ nửa "đã cũ" chưa
chạy: phải tắt `hubd` rồi chờ qua 5 phút.

---

## UC-S14 · Làm việc **hoàn toàn qua Telegram**  ✅ chạy thật 2026-08-11

Hà hỏi trước, rồi chốt bằng cách gõ thử: *"nếu làm việc hoàn toàn qua kênh tele
thì có gửi được nội dung chát không"*. Câu trả lời cũ là **không** — Telegram chỉ
là cái loa có đúng hai cái nút (Xác nhận / Huỷ của `confirm.rs`), tin chữ bị bỏ
qua hoàn toàn, và một phiên **dừng lại hỏi** thì tin báo nói "có N lựa chọn" mà từ
đó không chọn được gì.

**Nghiệm thu:** gõ một lệnh trong Telegram thì nó CHẠY và trả lời ngay tại đó;
`/sessions` cho danh sách phiên **bấm được**; bấm một phiên là vào thẳng phiên ấy
và **thấy màn hình** của nó.

**Cách làm — một đường, một cuốn sổ.** Vòng `getUpdates` thường trực
(`telegram.rs`) đẩy tin chữ vào hàng đợi, `execute_telegram_commands` cho chúng đi
qua **đúng `parse_command` + `execute_commands` của phòng chat**. Cổng người là
`chat_id` (cùng vai với `trust.tfl5_user_tids`): tin từ người khác được LOG rồi bỏ.
Cái nút cũng không đẻ ra động từ mới — `callback_to_command` biến `sess:<id>` /
`key:<id>:<n>` thành đúng dòng lệnh mà ngón tay sẽ gõ.

**Bằng chứng chạy thật (2026-08-11, giờ máy):**

| Lúc | Việc | Kết quả |
|---|---|---|
| 21:31:34 → 21:33:50 | `/help` | chạy, ack về Telegram — **nhưng chờ 2 phút 16 giây** |
| 22:53:02 | `/sessions` | `telegram_buttons_sent count=5`, 5 phiên kèm hàng phụ + câu cuối |
| 22:54:05 → 22:54:11 | **bấm nút** một phiên | `👁 Đang theo phiên projects-ff (acc3)` + `📷 Màn của projects-ff:` kèm 14 dòng màn thật |
| 22:58:04 → 22:58:04 | bấm lần hai | **0 giây** — sau khi hòm thư cầm `waker` |

📌 **Độ trễ 2 phút ấy là một lỗ hổng thật, không phải "chậm chút".**
`execute_telegram_commands` đứng đầu `run_once` mà vòng ngủ 120 giây; phòng chat
tfl5 thoát được vì socket `/ws/chat` gọi `wake()`, kênh này thì lúc đầu không có
gì gọi. Một mệnh lệnh gõ tay đợi hai phút thì người ta **gõ lại lần nữa** — và
lần thứ hai là một hành động THẬT chạy hai lần. Vá: hòm thư giữ chính cái `waker`
ấy, `push_text` đánh thức ngay.

⚠ **Một luật phải giữ:** chỉ **MỘT** nơi đọc `getUpdates`. Telegram giao mỗi
update cho người hỏi trước và `offset` là con dấu dùng chung, nên hai vòng đọc
song song sẽ ăn mất update của nhau — một cú bấm ✅ rơi vào vòng đọc lệnh thì
`confirm::ask` ngồi tới hết giờ rồi kết luận "không ai bấm", một câu SAI gửi cho
đúng người vừa bấm. `confirm` mượn đường bằng `hold()`, và nhặt hộ tin chữ vào
hàng đợi thay vì bỏ rơi.

### Chọn phiên xong thì **chữ thường = gõ vào phiên**  ✅ chạy thật 2026-08-12

Hà: *"bấm vào mỗi phiên focus vào phiên đó luôn"*. Chọn xong coi như đang ngồi
trong phiên: gõ gì nó nhận nấy, không phải nhớ thêm động từ. Ranh giới là **dấu
gạch chéo đầu dòng** — `/sesion` gõ nhầm KHÔNG được bơm vào cửa sổ đang chạy.

🔴 **Và nó lôi ra một niềm tin sai nằm sẵn trong `CLAUDE.md`.** Sổ ghi *"`do
script` luôn kèm xuống dòng ⟹ gõ xong là gửi"*. Đúng với shell, **sai với ô nhập
của `claude`**: chữ và dấu xuống dòng đi trong CÙNG một lượt ghi nên TUI đọc cả
cụm như một cú **DÁN** — chữ vào ô, dấu xuống dòng bị nuốt vào nội dung. Hà bắt
được bằng mắt: *"nhận được text nhưng không tự gửi, có vẻ như thiếu enter?"*

Bốn lượt gõ thật trong 12 phút giải thích trọn cơ chế:

| Lúc | Dài | Phiên lúc ấy | Kết cục |
|---|---|---|---|
| 08:28:34 | 38 byte | **rảnh** | chữ nằm lại trong ô ⟹ `keys_enter_sent` 08:28:36 ⟹ **gửi được** |
| 08:29:20 | 8 byte | **đang chạy** | `claude` xếp vào hàng chờ ⟹ tự gửi, không cần Enter |

⟹ Nuốt-dấu-xuống-dòng chỉ xảy ra khi phiên **đang rảnh**; lúc bận thì đường hàng
chờ nhận cả cụm và gửi đúng. Nay hub **nhìn rồi mới gửi**: đọc lại màn, chữ còn
trong ô thì bắn một **Enter rời**. Ba cửa, đều là đo: chữ còn trong ô · phiên
không bận · màn không có hộp chọn (ở đó Enter là CHỐT). Dòng dưới 6 ký tự không
kích hoạt — "2"/"ok" có mặt trên gần như mọi màn.

⚠ **Phép đo suýt trỏ sai chỗ:** gửi đi rồi thì câu ấy vẫn còn trên màn — ở phần
hội thoại. Soi cả màn thì hub đọc "đã gửi" thành "còn trong ô", bắn Enter thừa và
báo sai. `still_in_box` chỉ soi **khối đóng khung cuối cùng**, có test riêng.

### Cái loa thôi kêu vì phiên của chính hub  ✅ vá 2026-08-12

Hà: *"tại sao cứ báo phiên đã tắt liên tục, không biết nó là phiên nào rất mơ
hồ"*. Log 4 tiếng: **20 tin, mỗi tin một id khác**, đều đặn 7–12 phút — không
phải một phiên báo lặp, mà là **phép dò hạn mức của chính hub** (`claude -p
"/usage"`, 5 phút/lượt) đẻ ra phiên thật rồi chết trong vài giây. Luật "rời khỏi
danh sách = đã kết thúc" thiếu vế **sống bao lâu**: nay có `MIN_LIFE_SEC` (120s,
dùng chung con số với `MIN_RUN_SEC`), sống chớp nhoáng thì chết lặng lẽ + ghi
`session_end_muted`; phiên do **hub mở** thì luôn báo, vì ở đó chết ≠ xong.
Và tin nay gọi được tên: sổ nhớ sẵn **tên + dự án** từ trước, vì lúc phiên rời
danh sách thì hàng của nó đi theo — `⏹ projects-71 · AI/hub (8db91183)`.

**Cơ chế:** ✅ · **Sản phẩm:** ✅ · **Kịch bản:** ⏳ không có E2E — cổng là
`chat_id` nên chỉ ngón tay của chủ máy bấm được; bù bằng test thuần cho phần
quyết định (`callback_to_command`, `session_list_text`, `still_in_box`,
`text_for_session`) + log đối chiếu từng mốc.

---

## Đọc bảng này thế nào

**Không UC nào có kịch bản tự động.** Xương sống là **UC-S02** — và đo theo chuẩn
"giống như ngồi máy" thì phần đã làm hôm nay (lấy **một** lượt cuối) mới là mảnh
nhỏ của nó. UC-S03 (độ trễ) đi kèm S02, vì thấy đủ mà chậm hai phút thì vẫn không
giống ngồi máy.

Thứ tự đề xuất: **S02 + S03** (thấy đủ, thấy ngay) → S01 hoàn thiện → S08 nâng lên
mức từng khối → thử `--bg` cho S06 → điều tra S04 → S07 sau cùng.
