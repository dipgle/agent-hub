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
| S05b | Hỏi bên lề — **hai đường, hai lời hứa**: phiên có cửa sổ thì `/btw` hỏi thẳng (nhật ký không dài thêm, ăn ngữ cảnh đang chạy); phiên không gõ vào được thì **fork** (y nguyên byte) | `/btw` **21/21** trên `projects-ff`/`ttys001` · fork **10/10** trên `projects-71` (bước gọi `claude` bỏ qua: 0.26 > trần 0.25) — 2026-08-11 |
| S06 | Mở phiên cho một dự án — **cửa sổ Terminal thật**, tắt hẳn được từ điện thoại | `fe-newsession-uc` **26/26** trên màn thật, CÓ bấm xác nhận Telegram (2026-08-11 17:04-17:08): mở cửa sổ `ttys005` → nói tiếp (nhật ký 76797→78673 byte) → tắt hẳn (cửa sổ đóng, phiên rời danh sách, nhật ký còn 82267 byte) |
| S07 | Đóng sổ → bản bàn giao + phiên mới + lệnh `--resume` | `fe-stream-uc` |
| S08 | Bí mật không rò ra trang (quét trước khi đẩy) | `redaction` tests |
| S09 | Ảnh chụp cũ thì nói là cũ | `fe-board-uc` |
| S10 | Dừng / đóng sổ **ngay từ danh sách**, không phải mở phiên ra | `fe-sessions-uc` 25/25 |
| S11 | Lệnh dừng phải **xác nhận qua Telegram** mới chạy | chạy thật 2026-08-10 (dưới) |
| S02b | Phiên **đang chạy subagent** thì màn nói ra | `fe-subagent-uc` 12/12 trên HAI phiên, hai con số (3 và 4) |
| S12 | Danh sách biết **phiên nào đang chạy** — mọi phiên, không chỉ phiên nền | `fe-sessions-uc`, đối chiếu hai chiều với máy |
| S13 | Phiên **vừa xong / kẹt hỏi / tắt hẳn** thì báo vào phòng chat + Telegram | chạy thật, 0 lỗi gửi; câu nói dựa trên **đọc màn**, không đoán |
| S09b | **Ảnh chụp cũ thì nói là cũ** | `fe-stale-uc` 8/8 trên ảnh chụp 6.3 phút tuổi (tắt `hubd` thật) |
| S14 | Làm việc **hoàn toàn qua Telegram** — gõ lệnh, `/sessions` ra danh sách **bấm được**, bấm một phiên là vào thẳng và **thấy màn** | chạy thật 2026-08-11 22:53–22:58: `telegram_buttons_sent count=5` → bấm nút → `👁 Đang theo phiên projects-ff` + `📷 Màn của…` 14 dòng thật; lần bấm thứ hai **0 giây** |
| S14b | **Chữ thường gõ trên Telegram = gõ vào phiên đang theo**, kèm Enter rời khi TUI nuốt mất dấu xuống dòng | chạy thật 2026-08-12 08:28:34 → `keys_enter_sent` 08:28:36 → câu ấy tới đúng phiên; lượt 08:29 phiên đang bận thì đi đường hàng chờ, không cần Enter |
| S15 | Danh sách nói được **dự án** mỗi phiên đang làm (không lấy từ `cwd` — mọi phiên cùng `cwd`) | đo thật 2026-08-12: 4/4 phiên ra đúng `dwork · AI/hub · games · AI/tfl5`; bundle **v149**, ảnh 390px không cắt chữ |
| S16 | **Cái loa thôi kêu oan**: phiên sống chớp nhoáng (phép dò hạn mức của chính hub) chết đi thì im | chạy thật: **26** dòng `session_end_muted` (15s · 27s · 114s) và **0** tin "đã tắt" kể từ 11:03, so với **20 tin trong 4 tiếng** trước đó |
| S17 | Phiên **dừng lại HỎI** thì câu hỏi + từng lựa chọn lên điện thoại (đọc từ nhật ký, không rình trên màn) | ⚠ mới **chạy thử trên dữ liệu thật** của `projects-11` (dựng lại lúc câu hỏi còn treo → ra đúng tin + 3 lựa chọn); **chưa** có lượt gửi Telegram thật |
| S21 | **Mọi phiên terminal dừng chờ đều báo**, và tin của phiên khác phiên đang theo mang **nút vào phiên** | luật im 08-10 gỡ theo chỉ đạo 2026-08-12 (*"mọi phiên terminal đều báo"*); 3 test nút, 2 test đã kiểm là **đỏ được**; cài lúc 18:14, daemon pid 23685 `cert` |
| S25 | **`/cmd <dòng lệnh>`** — cổng chạy lệnh thứ ba, chạy một lệnh rồi thôi | route mới, đi chung `parse_command`/sổ với hai cổng cũ; kết quả qua cổng quét rò trước khi rời máy; 4 test hình dạng câu trả lời |
| S26 | **Lệnh thấy trên màn thành NÚT gửi nhanh** — bấm là gõ `!<lệnh>` vào chính phiên | `/shot` nay giữ **40 dòng** (trước 14 — đúng lý do lệnh của Hà không hiện); 6 test, trong đó 1 test ghim đúng dòng THẬT làm lộ bug (lệnh nằm trong câu văn) |
| S27 | **Bấm nút trên Telegram thôi đợi** | đo từng khúc: ảnh chụp phiên ~10s là thủ phạm (không phải hàng chờ). Đệm 20 giây ⟹ `/session` **11,6s → 1,5s** (`command_done ms=1496`, `sessions_snapshot_reused age_ms=4470`) |
| S24 | **Hỏi được một phiên VỪA TẮT** — `/ask` · `/handover` rơi về sổ phiên tắt trong 24 giờ | chạy thật 20:43:41: mở phiên qua phòng → gõ `/exit` qua `/type` → hub ghi sổ đủ ba thứ `--resume` cần (`acc3` · `cwd` · id). Ngõ cụt cũ thay bằng câu kèm danh sách phiên đang sống (đo, không tốn hạn mức) |
| — | **Token sai thôi chết câm**: mọi câu từ chối của `getUpdates` đều log rồi lùi 30s, và hub khai đang cầm bot nào lúc mở kênh | chạy thật 20:23:29: `telegram_bot_identity {"username":"ai_angles_bot"}` |
| S22 | Lệnh Telegram **chạy ngay khi bấm** (không đợi vòng), `/session` thôi chụp lại màn | đo trước khi vá: bấm 18:17:45 → chạy 18:18:11 → trả lời 18:18:27 (**42s**: 26s chờ vòng + 16s chụp màn); nay chạy ở luồng riêng, xếp hàng bằng `CMD_LOCK`; cài 18:29 |
| S23 | **Tự xoá tin Telegram cũ hơn 36 giờ** | cài 19:35 (pid 62301). 6 test, 2 đã kiểm là **đỏ được**. ⏳ chưa có lượt xoá THẬT: sổ `telegram:sent` mới bắt đầu ghi, tin đầu tiên tới hạn sau ~36h |
| S19 | **Xem ba tài khoản** từ phòng chat, và biết `/new` rơi vào tài khoản nào | `fe-accounts-uc` **12/12** trên bundle đã deploy (2026-08-12 17:28): `acc1 ⭐ mặc định · acc2 tuần 100% · acc3 tuần 5%`, 0 `$`, không tràn ngang ở 390px |
| S20 | Lệnh đi bằng **cờ** (`/new -a acc3 -s hub`), đề bài để trống vẫn mở được, mở xong **theo luôn** phiên mới | `fe-newflags-uc` **8/8** chạy thật 17:33 + 17:35; đối chiếu ngoài màn: `new_window_opened tty=ttys003 task=""`, `focus:session` = đúng phiên vừa mở |
| — | **Cái loa thôi đọc phép đo hỏng thành cái chết**: `claude agents` hỏng cho một tài khoản thì phiên của nó KHÔNG bị coi là đã tắt, và sổ giữ nguyên | 8 test mới, 3 test lõi đã kiểm là **đỏ được**; ⏳ chưa có lượt THẬT (xem "Còn nợ") |
| — | **Cái loa thôi nói về phiên của CHÍNH hub, và cái nút thôi dẫn vào phiên đã chết** | Hà đọc tin thật: *"tại sao 1 phiên đã tắt mà vẫn gắn nút vào phiên"* · *"quá vô lý"*. Đo log: `⏹ hub-e6 … cửa sổ ấy nay đang chạy hub-36` (16:11:51) rồi `⏹ hub-36 … hub-f5` (16:16:05) — **5 phút một tin**, cả ba đều là phép dò `/usage` của hub, `tty="??"` (không phiên nào có cửa sổ). Ba vá: `is_real_tty` (một chỗ thay bốn bản chép), `is_hub_own_probe` (hai cửa: đang sống + trong sổ), `enter_button` (nút phải có phiên SỐNG để vào). 5 test mới, **cả 5 đỏ được** |
| — | **hub theo gốc workspace mới `~/projects`** — không còn đường dẫn nào gõ cứng vào `~/Documents/projects` | đo thật 2026-08-12 22:5x sau khi cài lại: `hub doctor` `workspace /Users/hanguyen/projects`, snapshot liệt kê **32 dự án**, `folder` của 3 phiên sống ra đúng `dwork · AI/hub · AI/tcc`; phép đo "daemon cũ hơn mã" đã **sống lại và đo đúng**: `stale=false` → chạm một `.rs` → `true` → `install.sh` → `false`; 2 test mới, cả hai **đỏ được** khi trả đường cứng về chỗ cũ |
| S18 | Tin báo mang **thông tin chốt** của lượt cuối, không mang câu dẫn nhập | chạy thật 2026-08-12 16:26 trên **4 phiên đang sống**: phiên trước đây mang `[dùng Read]`/`[dùng Bash]` nay mang một câu có nghĩa; `projects-71` (báo cáo 3151 byte) ra đủ *kết luận → bằng chứng → ⋯ → đề xuất → câu chốt* + `… (còn N dòng)`. ⚠ **chưa** có lượt gửi Telegram thật (từ lúc cài chưa phiên nào chuyển trạng thái) |

**UC-S11, bằng chứng chạy thật (2026-08-10, cả hai đường):**

| | hỏi lúc | Hà bấm | kết cục | phiên sau đó |
|---|---|---|---|---|
| đường thuận | 04:56:58 | ✅ Xác nhận (38s) | `Confirmed` → `session_stopped` | biến khỏi danh sách |
| đường chặn | 04:59:26 | ✖ Huỷ (48s) | `Declined` | **CÒN SỐNG · working** |

Phòng chat nói đúng cả chuỗi: `🔒 Đã gửi yêu cầu xác nhận sang Telegram… Chưa dừng
gì cho tới khi bấm nút.` → `✋ Đã huỷ trên Telegram — không dừng phiên nào.`

Mặt bằng: 4 tab (Phiên · Trao đổi · Sức khoẻ · Cấu hình), nghiệm thu ở **390×844**.

## Còn nợ, có sổ

- 🔴 **`claude -p "/usage"` TREO khi hubd gọi — chưa có thủ phạm** (mở
  2026-08-12). Hình dạng đã đo chắc: `timed_out: true · ms: 60952 ·
  stdout_bytes: 0 · stderr rỗng` — treo tới trần 60s, không ra byte nào. Hậu quả
  nhìn thấy được: hàng tài khoản trên tab Sức khoẻ **trống số hạn mức**
  (`fe-board-uc` 29/31). Đã loại bằng đo: stdin (đã đóng, `exec.rs:132`), sai
  binary (cùng `~/.npm-global/bin/claude`), môi trường launchd (chạy lại y hệt
  bằng `env -i` + cwd của hubd → **3,58s ra đủ số**), và **không phải do dời gốc
  workspace** — đếm log: 60 lần, lần đầu **10/08 05:51**, đi theo đợt. Nghi can
  còn lại chưa kiểm: chồng lấn với `claude agents` trong cùng một vòng. Phép đo
  kế tiếp: ghi kèm "lúc ấy còn lời gọi `claude` nào đang chạy không".

- ~~Chưa quan sát được một tin Telegram THẬT mang thông tin chốt (S18)~~ →
  **đã có, 18:17:13**: `⏸ projects-7c dừng, đang chờ bạn — sau 16 phút chạy` kèm
  nguyên khối thông tin chốt (mở bằng kết luận, có dấu đứt `⋯`, và **ba dòng
  cuối** — đúng thứ `key_points` đặt chỗ trước). Hà nhận được và bấm nút trên
  chính tin ấy (`telegram_command_queued /session e27806c2` lúc 18:17:45).
- **Còn nợ của S17**: tin THẬT của một phiên *dừng lại HỎI* vẫn chưa quan sát
  được — cần đúng lúc một phiên đang treo câu hỏi mà hub nhìn vào (khe mù ~139s).
- ~~**Phiên `projects-71` (pid 5001) tự cập nhật `claude` mỗi 30 phút**~~ →
  **đã đóng 2026-08-12 16:59** (Hà chốt). `kill 5001` xong: pid biến mất, không
  còn `npm install @anthropic-ai/claude-code` nào chạy, và hub báo đúng
  `⏹ projects-71 · games (296972d4) đã tắt hẳn` (phiên nằm trong terminal tích
  hợp VS Code, không phải cửa sổ Terminal.app ⟹ "tắt hẳn" là câu đúng).
  Nó là thủ phạm của **cả hai** chuyện: mất quyền `~/Documents` từng lượt ~2
  phút, và lỗi A của cái loa (danh sách phiên hỏng ⟹ báo tắt nhầm).
- **`/ask` trên một phiên ĐÃ TẮT chưa chạy thật** — nửa ghi sổ đã đo (20:43:41),
  nửa còn lại tiêu hạn mức của chủ máy nên để chính anh bấm. Cơ chế đã cài.
- **Cơ chế xoá tin Telegram chưa xoá được tin nào** — sổ `telegram:sent` chỉ ghi
  từ 19:35 trở đi, nên tin cũ hơn thời điểm ấy **vĩnh viễn không xoá được**
  (không có `message_id` để gọi, và Telegram chỉ cho bot xoá trong 48 giờ). Lượt
  xoá thật đầu tiên rơi vào khoảng 36 giờ sau tin đầu tiên được ghi.
- **Token Telegram mới chưa tới chỗ hub đọc** (đo 2026-08-12 19:30): hub nạp bí
  mật từ HAI tệp (`config.rs:594`), mà tệp đầu sửa lần cuối **06/08**, tệp thứ
  hai **10/08 11:43**, và không tệp môi trường nào dưới `~/Documents/projects`
  đổi sau 15:30 hôm nay. Bot mới còn phải được bấm `/start` một lần thì mới nhắn
  cho chủ máy được.
- **Hai bản vá của cái loa chưa có lượt chạy THẬT** (xem UC "Hai lỗi của cái
  loa"): lỗi A cần một lần `claude agents` hỏng nữa — mà thủ phạm vừa bị đóng,
  nên có thể không tái diễn; lỗi B cần một phiên tắt trong lúc phiên khác giữ
  đúng tty của nó. Cả hai đã có test đỏ-được; đừng đọc thành "đã chứng minh trên
  máy".

Trước đó **rỗng** (2026-08-10). Mục cuối — bốn bảng hộp thư chết — đã dọn bằng bước nâng
cấp lược đồ 4; xem "Đã trả xong". Món nào mới phát sinh thì ghi vào đây, đừng để
danh sách này có sẵn vài dòng thường trực: một sổ nợ không bao giờ rỗng thì thôi
là sổ việc, thành cái nền để biện minh.

## Đã thử và ĐO RA LÀ SAI (đừng thử lại mà không đo)

- **Song song hoá ba lời gọi `claude agents`** (2026-08-12): tưởng chia được 10
  giây thành 3,5 — đo lại thì **trung vị 10,1s → 13,0s**, chậm hơn 30%. Ba tiến
  trình `claude` (279 MB) dựng cùng lúc giẫm chân nhau ở CPU và đĩa; cái giá ấy
  không chia được. Đã trả lại bản nối đuôi, giữ nguyên phép đo trong
  `sessions.rs` để lần sau không ai thử lại bằng trực giác.

## Theo thiết kế, KHÔNG phải nợ

- **`fe-newsession-uc` bán tự động.** Bước tắt phiên đi qua chốt xác nhận
  Telegram nên cần một ngón tay thật. Không ai bấm thì kịch bản in **"BỎ QUA 4
  kiểm tra"** kèm tên từng kiểm tra chưa nghiệm thu, và vẫn thoát 0 — sản phẩm
  lúc ấy đang cư xử ĐÚNG. Đây là cái giá của chốt chặn, không phải một thứ để
  sửa. Muốn đóng trọn thì bấm nút Telegram trong lúc kịch bản chạy.
  **Đừng soi trang bằng `fe-probe` trong lúc kịch bản đang chạy**: mỗi lượt soi
  gửi thêm một `/session` vào phòng, chen mất lượt trả lời của bước tắt — đo
  2026-08-11, một lượt chạy sạch thì tin về đúng hạn, lượt bị soi song song thì
  nằm mãi ở tin giữa chừng `🔒`.

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
