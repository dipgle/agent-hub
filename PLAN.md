# PLAN — hub

**Mục tiêu duy nhất:** từ điện thoại, xem và điều khiển các phiên `claude` đang
chạy trên Mac này. Không hộp thư, không triage, không tự tiêu hạn mức.

Sổ UC đầy đủ (kèm bằng chứng chạy thật): `UC.md`. Vì sao nhánh hộp thư bị xoá:
`CLAUDE.md` §"What hub is NOT".

## Đã xong, đã chạy thật

| UC | Việc | Bằng chứng |
|---|---|---|
| S01 | Danh sách mọi phiên đang sống, mọi tài khoản | `fe-sessions-uc` 9/9 |
| S02 | Xem một phiên như đang ngồi máy (lệnh + kết quả) | `fe-stream-uc` 17/17 |
| S03 | Trang tự làm mới khi đang theo một phiên | `fe-stream-uc` (≥3 lần/15s) |
| S04 | Biết phiên chạy dưới chế độ quyền nào | `fe-sessions-uc` |
| S05b | Hỏi bên lề trên **bản fork** — phiên gốc y nguyên byte | `fe-aside-uc` 17/17 |
| S06 | Mở phiên nền cho một dự án | `fe-newsession-uc` 9/9 (đường KẸT) |
| S07 | Đóng sổ → bản bàn giao + phiên mới + lệnh `--resume` | `fe-stream-uc` |
| S08 | Bí mật không rò ra trang (quét trước khi đẩy) | `redaction` tests |
| S09 | Ảnh chụp cũ thì nói là cũ | `fe-board-uc` |
| S10 | Dừng / đóng sổ **ngay từ danh sách**, không phải mở phiên ra | `fe-sessions-uc` 25/25 |
| S11 | Lệnh dừng phải **xác nhận qua Telegram** mới chạy | chạy thật 2026-08-10 (dưới) |
| S02b | Phiên **đang chạy subagent** thì màn nói ra | `fe-subagent-uc` 12/12 trên HAI phiên, hai con số (3 và 4) |
| S12 | Danh sách biết **phiên nào đang chạy** — mọi phiên, không chỉ phiên nền | `fe-sessions-uc`, đối chiếu hai chiều với máy |
| S13 | Phiên **vừa xong / kẹt hỏi / tắt hẳn** thì báo vào phòng chat + Telegram | chạy thật, 0 lỗi gửi; câu nói dựa trên **đọc màn**, không đoán |
| S09b | **Ảnh chụp cũ thì nói là cũ** | `fe-stale-uc` 8/8 trên ảnh chụp 6.3 phút tuổi (tắt `hubd` thật) |

**UC-S11, bằng chứng chạy thật (2026-08-10, cả hai đường):**

| | hỏi lúc | Hà bấm | kết cục | phiên sau đó |
|---|---|---|---|---|
| đường thuận | 04:56:58 | ✅ Xác nhận (38s) | `Confirmed` → `session_stopped` | biến khỏi danh sách |
| đường chặn | 04:59:26 | ✖ Huỷ (48s) | `Declined` | **CÒN SỐNG · working** |

Phòng chat nói đúng cả chuỗi: `🔒 Đã gửi yêu cầu xác nhận sang Telegram… Chưa dừng
gì cho tới khi bấm nút.` → `✋ Đã huỷ trên Telegram — không dừng phiên nào.`

Mặt bằng: 4 tab (Phiên · Trao đổi · Sức khoẻ · Cấu hình), nghiệm thu ở **390×844**.

## Còn nợ, có sổ

**Rỗng** (2026-08-10). Mục cuối — bốn bảng hộp thư chết — đã dọn bằng bước nâng
cấp lược đồ 4; xem "Đã trả xong". Món nào mới phát sinh thì ghi vào đây, đừng để
danh sách này có sẵn vài dòng thường trực: một sổ nợ không bao giờ rỗng thì thôi
là sổ việc, thành cái nền để biện minh.

## Theo thiết kế, KHÔNG phải nợ

- **`fe-newsession-uc` bán tự động.** Bước `/stop` đi qua chốt xác nhận Telegram
  nên cần một ngón tay thật. Không ai bấm thì kịch bản in **"BỎ QUA 2 + 3 kiểm
  tra"** kèm tên từng kiểm tra chưa nghiệm thu, và vẫn thoát 0 — sản phẩm lúc ấy
  đang cư xử ĐÚNG. Đây là cái giá của chốt chặn, không phải một thứ để sửa. Muốn
  đóng trọn thì bấm nút Telegram trong lúc kịch bản chạy.

## Đã trả xong (giữ lại vì sổ từng ghi là nợ)

- ~~Bốn bảng hộp thư chết trong `data/hub.sqlite`~~ → **đã dọn 2026-08-10** bằng
  **bước nâng cấp lược đồ 4**, không phải một lệnh gõ tay: nằm trong mã, có test,
  có log, chạy đúng một lần trên mọi máy. Chạy thật trên DB sống:
  `messages 200 · outbox 90 · decisions 87 · dead_letter 2`, mỗi bảng một dòng
  log kèm số dòng; còn lại đúng `cursors runs schema_meta spend`, lược đồ lên 4.
  ⚠ Bản đầu **giết daemon ngay lúc dựng lên**: bốn bảng ấy tham chiếu lẫn nhau
  mà `open()` bật `foreign_keys = ON` ngay trên đó — `FOREIGN KEY constraint
  failed` (787), `last exit code = 70`. Vá bằng cách TẮT kiểm khoá ngoại trong
  lúc dọn rồi bật lại kể cả khi hỏng, chứ không xếp thứ tự xoá: thứ tự đúng hôm
  nay là thứ tự sai vào ngày ai đó thêm một tham chiếu.
  📌 Lỗi ấy lộ ra trong 20 giây **nhờ đúng bản vá cùng ngày** cho `bin/hubd.rs`
  — trước đó nó chết bằng `eprintln!`, lý do chỉ nằm ở stderr của launchd.

- ~~"Bật lại máy thì hub có tự lên không" chưa nghiệm thu~~ → **mọi điều kiện
  một lần reboot sẽ kiểm đều đã đo xong 2026-08-10**, chỉ còn đúng sự kiện
  reboot (việc của Hà, và nó không đổi được kết quả nào ở dưới):
  plist nằm trong `~/Library/LaunchAgents` (5182 byte) · `launchctl print` khai
  `properties = keepalive | runatload`, `state = running`, `program` trỏ đúng
  **bản cài đã ký chứng chỉ** · job **không** nằm trong `print-disabled` (chỉ
  `com.dipgle.aw-daemon` bị tắt) · Background Task Management của macOS:
  `Disposition: [enabled, allowed, notified]` · và `bootout` + `bootstrap` đã
  chạy thật **hai lần trong ngày** — đó chính là thao tác launchd làm lúc đăng
  nhập. Vế "grant TCC sống qua rebuild" đã có bằng chứng riêng: DR neo theo
  danh tính chứ không theo byte.

- ~~Bí mật cũ nằm trong lịch sử `.git`~~ → **đã gỡ hẳn 2026-08-10**. Hà chốt
  *"mật khẩu tfl5 đã rời máy đâu mà đổi, bỏ commit liên quan đi"* — repo chưa
  từng có remote nên giá trị ấy chưa rời máy này, và đó là lý do xoay khoá không
  cần thiết. Hà tự chạy `filter-branch` + `gc` (Claude bị guard chặn ba lần và
  không lách). **Đo lại sau khi xong:** 0 commit còn mang tệp ấy · 0 tệp nào
  trong TOÀN BỘ lịch sử còn dòng gán mật khẩu · `.git` **8.8M → 1.3M** (object
  cũ bị vứt thật, không chỉ bị bỏ tham chiếu) · `git fsck` sạch · commit
  `2b6ea80` giữ nguyên **9 tệp việc thật**, chỉ mất đúng tệp bí mật · tệp thật
  vẫn nằm trong cây làm việc `chmod 600` và git chỉ theo dõi bản `*.example` ·
  `fe-smoke` exit 0 và daemon vẫn đẩy được ảnh chụp, tức hub còn đăng nhập tfl5.

- ~~`fe-sessions-uc` đỏ giả khi tập phiên vừa đổi~~ → đã cắm kẹp cùng khuôn với
  `fe-subagent-uc` (đọc sự thật — chờ màn bắt kịp — đọc lại; chỉ so khi hai đầu
  kẹp bằng nhau), và dấu vân tay của tập phiên nay gồm cả `working` nên phép đo
  chấm màu cũng được kẹp theo.

- ~~UC-S09 nửa "ảnh chụp đã cũ" chưa chạy~~ → **8/8** trên ảnh chụp **6.3 phút
  tuổi**, dựng bằng cách `bootout` `hubd` thật (hub mù ~6 phút, hai lượt). Và
  lượt đầu dạy đúng bài của dự án này: **7/7 xanh trong khi màn hình cắt cụt**
  câu cảnh báo — bảy assert đều đọc `textContent`, thứ có đủ chữ kể cả khi màn
  chỉ hiện tới `…Ảnh chụp lúc 19:50:59 1…`. Chỉ MỞ ẢNH RA NHÌN mới thấy nửa
  quan trọng nhất (*"Số dưới đây là của lúc đó, không phải bây giờ"*) không bao
  giờ tới mắt. Nay `.stale` được xuống dòng, và có phép đo hỏi `scrollWidth`.

- ~~Năm chỗ "lỗi im lặng"~~ → vá hết 2026-08-10 (Hà: *"làm nốt đi"*).
  **12 chỗ** đọc `db.get_cursor` nay đi qua `Db::cursor_or_log` — đặt chốt ở MỘT
  nơi vì mười hai chỗ gọi thì chỗ thứ mười ba sẽ quên; `bin/hubd.rs` chết bằng
  `logging::error` (ra cả stderr LẪN tệp log) thay vì `eprintln!` chỉ ra stderr
  của launchd; `sessions.rs` thôi khẳng định "đã dừng lại" khi lệnh dừng chưa
  chạy được — nay nói thẳng phiên còn sống kèm lệnh để tự dừng, và cả hai đường
  hỏng đều log; `config.rs` phân biệt "không có `hub.env`" (im, chuyện thường)
  với "có mà đọc không được" (log — sai quyền sẽ hiện ra dưới dạng "chưa đặt
  biến môi trường" ở tận cuối đường); `adapters/tfl5.rs` log khi mất trần đọc.
  *Ghi lại một điều đo được:* bản dựng này **không bật TLS** cho `tungstenite`
  (`Cargo.toml:30`), nên `MaybeTlsStream` chỉ có biến thể `Plain` — cái gọi là
  "bỏ sót nhánh TLS" không đúng với bản dựng này.
- ~~Hàng phụ thẻ phiên mất chữ `ngữ cảnh N%` khi có subagent~~ → giữ luật MỘT
  DÒNG (quyết định cũ, có ghi), rút chữ còn `N subagent`. Đo ở 390px với
  subagent thật: *"acc2 · tự duyệt · 1 subagent · ngữ cảnh 46%"* — **vừa khít**,
  không cắt. `fe-subagent-uc` **8/8** (thêm 2 phép đo hỏi `scrollWidth` chứ
  không đếm ký tự).

- ~~Chốt phím mũi tên hỏng về phía GỬI~~ → vá 2026-08-10 (Hà chỉ đạo *"vá chốt
  phím mũi tên đi"*). `screen_of` gộp **ba** kết cục vào `None` — không có cửa
  sổ · `osascript` hỏng · **màn có dấu hiệu lộ bí mật** — và chốt đọc `None`
  thành "không có hộp chọn" rồi GỬI, tức hỏng về phía nguy hiểm đúng lúc hub mù
  nhất. Nay `keys::look` trả `Saw`/`Withheld`/`Blind`, và mũi tên chỉ đi khi
  **chứng minh được không có hộp chọn**. `Withheld` vẫn quyết đúng: số lựa chọn
  là một CON SỐ, không mang chữ nào ra khỏi máy. Hai lỗi im lặng cùng họ vá kèm:
  `window_of`/`screen_text` hỏng nay có log, và câu trả lời sau khi gõ thôi khai
  "phiên đang đứng ở dấu nhắc" khi thực ra không đọc lại được màn.

- ~~UC-S02b chỉ có unit test~~ → `fe-subagent-uc` **6/6** trên subagent THẬT
  (2026-08-10). Trên đường đi lòi ra **bộ đếm sai với agent chạy nền** (nhận
  `tool_result` ngay lúc tung nên bị coi là đã xong) và **phiên chết khai 3
  subagent ma**; cả hai đã vá, có test RED-trước-GREEN-sau.
- ~~Màn phiên đứng nguyên "Đang dừng phiên…" mãi mãi khi không ai bấm Telegram~~
  → `fe/index.html:2498` nay nhận `🔒` (giữa chừng, KHÔNG đóng lượt chờ) và
  `✋`/`⌛` (kết cục). Đo thật: hub trả lời `⌛ Hết hạn…` lúc 07:54:00 mà màn cũ
  không hề đổi; bundle **v130** thì màn tự nói ra.

- ~~`/tell` + `/stop` chưa nghiệm thu qua UI~~ → `fe-newsession` **22/22**
  (2026-08-10): mở phiên `58f37f0c` → dừng → nói tiếp, nhật ký dài ra
  32509→38438 byte.
- ~~`fe-phone-uc` còn đỏ~~ → **31/31**.
- ~~Danh sách tự đi mất chỗ sau khi daemon khởi động lại~~ → neo giữ thẻ đang
  nhìn nay **nhường khi người ta đang ở đỉnh** (`fe/index.html:1760`): phiên mới
  chèn lên trên không đẩy trang nữa. Đo trong đúng điều kiện tái hiện:
  **151px → 0px**, `fe-board` 31/31.

## Nguyên tắc còn giữ

- Mọi thứ đổi trạng thái đi qua **lệnh trong phòng chat** → luôn có dấu vết.
- Phiên nền chạy sau `DENIED_TOOLS`; hỏi/bàn giao chạy trên **fork** read-only.
- hub **không tự tiêu hạn mức**; chỉ nút bấm của chủ máy mới gọi `claude`.
- Không con số `$` nào trên màn hình (sổ `spend` vẫn ghi, im lặng).
