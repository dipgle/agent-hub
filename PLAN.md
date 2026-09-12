# PLAN — huba

**Mục tiêu duy nhất:** từ điện thoại, xem và điều khiển các phiên `claude` đang
chạy trên Mac này. Không hộp thư, không triage, không tự tiêu hạn mức.

Sổ UC đầy đủ (kèm bằng chứng chạy thật): `UC.md`. Vì sao nhánh hộp thư bị xoá:
`CLAUDE.md` §"What huba is NOT".

## Đã xong, đã chạy thật

🔴 **Đọc trước bảng này (2026-08-14).** Phần lớn cột "Bằng chứng" trỏ vào các
kịch bản `fe-*-uc.mjs` chạy Playwright trên bundle tfl5. **Những kịch bản ấy đã
bị xoá** cùng trang điện thoại — bằng chứng là chuyện ĐÃ XẢY RA nên giữ nguyên
văn, nhưng **không chạy lại được**, và một dòng ở đây không còn tự chứng minh
mình nữa. Việc gì cần khẳng định lại thì phải khẳng định bằng đường Telegram
thật. Mã cũ nằm trong git trước `cf20874`.

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
| S09b | **Ảnh chụp cũ thì nói là cũ** | `fe-stale-uc` 8/8 trên ảnh chụp 6.3 phút tuổi (tắt `hubad` thật) |
| S14 | Làm việc **hoàn toàn qua Telegram** — gõ lệnh, `/sessions` ra danh sách **bấm được**, bấm một phiên là vào thẳng và **thấy màn** | chạy thật 2026-08-11 22:53–22:58: `telegram_buttons_sent count=5` → bấm nút → `👁 Đang theo phiên projects-ff` + `📷 Màn của…` 14 dòng thật; lần bấm thứ hai **0 giây** |
| S14b | **Chữ thường gõ trên Telegram = gõ vào phiên đang theo**, kèm Enter rời khi TUI nuốt mất dấu xuống dòng | chạy thật 2026-08-12 08:28:34 → `keys_enter_sent` 08:28:36 → câu ấy tới đúng phiên; lượt 08:29 phiên đang bận thì đi đường hàng chờ, không cần Enter |
| S15 | Danh sách nói được **dự án** mỗi phiên đang làm (không lấy từ `cwd` — mọi phiên cùng `cwd`) | đo thật 2026-08-12: 4/4 phiên ra đúng `dwork · AI/huba · games · AI/tfl5`; bundle **v149**, ảnh 390px không cắt chữ |
| S16 | **Cái loa thôi kêu oan**: phiên sống chớp nhoáng (phép dò hạn mức của chính huba) chết đi thì im | chạy thật: **26** dòng `session_end_muted` (15s · 27s · 114s) và **0** tin "đã tắt" kể từ 11:03, so với **20 tin trong 4 tiếng** trước đó |
| S17 | Phiên **dừng lại HỎI** thì câu hỏi + từng lựa chọn lên điện thoại (đọc từ nhật ký, không rình trên màn) | ⚠ mới **chạy thử trên dữ liệu thật** của `projects-11` (dựng lại lúc câu hỏi còn treo → ra đúng tin + 3 lựa chọn); **chưa** có lượt gửi Telegram thật |
| S21 | **Mọi phiên terminal dừng chờ đều báo**, và tin của phiên khác phiên đang theo mang **nút vào phiên** | luật im 08-10 gỡ theo chỉ đạo 2026-08-12 (*"mọi phiên terminal đều báo"*); 3 test nút, 2 test đã kiểm là **đỏ được**; cài lúc 18:14, daemon pid 23685 `cert` |
| S25 | **`/cmd <dòng lệnh>`** — cổng chạy lệnh thứ ba, chạy một lệnh rồi thôi | route mới, đi chung `parse_command`/sổ với hai cổng cũ; kết quả qua cổng quét rò trước khi rời máy; 4 test hình dạng câu trả lời |
| S26 | **Lệnh thấy trên màn thành NÚT gửi nhanh** — bấm là gõ `!<lệnh>` vào chính phiên | `/shot` nay giữ **40 dòng** (trước 14 — đúng lý do lệnh của Hà không hiện); 6 test, trong đó 1 test ghim đúng dòng THẬT làm lộ bug (lệnh nằm trong câu văn) |
| S27 | **Bấm nút trên Telegram thôi đợi** | đo từng khúc: ảnh chụp phiên ~10s là thủ phạm (không phải hàng chờ). Đệm 20 giây ⟹ `/session` **11,6s → 1,5s** (`command_done ms=1496`, `sessions_snapshot_reused age_ms=4470`) |
| S24 | **Hỏi được một phiên VỪA TẮT** — `/ask` · `/handover` rơi về sổ phiên tắt trong 24 giờ | chạy thật 20:43:41: mở phiên qua phòng → gõ `/exit` qua `/type` → huba ghi sổ đủ ba thứ `--resume` cần (`acc3` · `cwd` · id). Ngõ cụt cũ thay bằng câu kèm danh sách phiên đang sống (đo, không tốn hạn mức) |
| — | **Token sai thôi chết câm**: mọi câu từ chối của `getUpdates` đều log rồi lùi 30s, và huba khai đang cầm bot nào lúc mở kênh | chạy thật 20:23:29: `telegram_bot_identity {"username":"ai_angles_bot"}` |
| S22 | Lệnh Telegram **chạy ngay khi bấm** (không đợi vòng), `/session` thôi chụp lại màn | đo trước khi vá: bấm 18:17:45 → chạy 18:18:11 → trả lời 18:18:27 (**42s**: 26s chờ vòng + 16s chụp màn); nay chạy ở luồng riêng, xếp hàng bằng `CMD_LOCK`; cài 18:29 |
| S23 | **Tự xoá tin Telegram cũ hơn 36 giờ** | cài 19:35 (pid 62301). 6 test, 2 đã kiểm là **đỏ được**. ⏳ chưa có lượt xoá THẬT: sổ `telegram:sent` mới bắt đầu ghi, tin đầu tiên tới hạn sau ~36h |
| S19 | **Xem ba tài khoản** từ phòng chat, và biết `/new` rơi vào tài khoản nào | `fe-accounts-uc` **12/12** trên bundle đã deploy (2026-08-12 17:28): `acc1 ⭐ mặc định · acc2 tuần 100% · acc3 tuần 5%`, 0 `$`, không tràn ngang ở 390px |
| S20 | Lệnh đi bằng **cờ** (`/new -a acc3 -s huba`), đề bài để trống vẫn mở được, mở xong **theo luôn** phiên mới | `fe-newflags-uc` **8/8** chạy thật 17:33 + 17:35; đối chiếu ngoài màn: `new_window_opened tty=ttys003 task=""`, `focus:session` = đúng phiên vừa mở |
| — | **Cái loa thôi đọc phép đo hỏng thành cái chết**: `claude agents` hỏng cho một tài khoản thì phiên của nó KHÔNG bị coi là đã tắt, và sổ giữ nguyên | 8 test mới, 3 test lõi đã kiểm là **đỏ được**; ⏳ chưa có lượt THẬT (xem "Còn nợ") |
| S28 | **Bấm "vào phiên" trả lời NGAY** — tên + tài khoản đọc từ sổ, không dựng lại ảnh chụp | đo trên chính ngón tay Hà, cùng điều kiện (ảnh chụp quanh đó vẫn 15–28s): `command_done kind=Session` **48 407ms** (16:49) · **29 295ms** (16:53) → **1 106ms** (17:00, sau vá). 4 test mới |
| — | **Cái loa thôi nói về phiên của CHÍNH huba, và cái nút thôi dẫn vào phiên đã chết** | Hà đọc tin thật: *"tại sao 1 phiên đã tắt mà vẫn gắn nút vào phiên"* · *"quá vô lý"*. Đo log: `⏹ huba-e6 … cửa sổ ấy nay đang chạy huba-36` (16:11:51) rồi `⏹ huba-36 … huba-f5` (16:16:05) — **5 phút một tin**, cả ba đều là phép dò `/usage` của huba, `tty="??"` (không phiên nào có cửa sổ). Ba vá: `is_real_tty` (một chỗ thay bốn bản chép), `is_hub_own_probe` (hai cửa: đang sống + trong sổ), `enter_button` (nút phải có phiên SỐNG để vào). 5 test mới, **cả 5 đỏ được** |
| — | **huba theo gốc workspace mới `~/projects`** — không còn đường dẫn nào gõ cứng vào `~/Documents/projects` | đo thật 2026-08-12 22:5x sau khi cài lại: `huba doctor` `workspace /Users/hanguyen/projects`, snapshot liệt kê **32 dự án**, `folder` của 3 phiên sống ra đúng `dwork · AI/huba · AI/tcc`; phép đo "daemon cũ hơn mã" đã **sống lại và đo đúng**: `stale=false` → chạm một `.rs` → `true` → `install.sh` → `false`; 2 test mới, cả hai **đỏ được** khi trả đường cứng về chỗ cũ |
| S18 | Tin báo mang **thông tin chốt** của lượt cuối, không mang câu dẫn nhập | chạy thật 2026-08-12 16:26 trên **4 phiên đang sống**: phiên trước đây mang `[dùng Read]`/`[dùng Bash]` nay mang một câu có nghĩa; `projects-71` (báo cáo 3151 byte) ra đủ *kết luận → bằng chứng → ⋯ → đề xuất → câu chốt* + `… (còn N dòng)`. ⚠ **chưa** có lượt gửi Telegram thật (từ lúc cài chưa phiên nào chuyển trạng thái) |
| S29 | **Hết hạn mức thì TỰ chuyển tài khoản** — không chờ chủ máy gõ `/handover -a`; cờ tắt được `auto_handover.on_limit` | Hà 2026-09-01: *"Tại sao acc bị limit không tự chuyển mà bắt tôi gõ lệnh"*. Trước vá: `Change::Limited` chỉ **soạn một câu gợi ý**, còn `auto_handover` vào bằng cửa **% ngữ cảnh** và chưa bao giờ đọc `LiveSession.limited` — thiếu một cái cò, không thiếu cơ chế. **Chạy thật 19:48:00Z**: `[dwork/A-DDRIVE]` (acc2, đứng chặn từ 17:23:34) → `auto_limit_firing` → cửa sổ mới `ttys006` bằng acc1 lúc 19:48:04 (`doi_acc:true`), phiên cũ biến khỏi danh sách, tin + nút `👁` sang Telegram; bàn giao dựng từ NHẬT KÝ ⟹ **0 đồng**. 14 test, **đối chứng ngược đã chạy**: bỏ cửa *"phiên có bị chặn không"* ⟹ 3 test đỏ, exit 101 |
| — | ⚠ **Chính lượt chạy thật ấy lôi ra một lỗi, và nó đã được vá cùng ngày** | `resets 1:40am` đọc lúc **02:48 giờ máy** ra **1372 phút** — đồng hồ 12 giờ không phân biệt được "còn 23 tiếng" với "đã qua 70 phút" — nên huba đổi một phiên **67% ngữ cảnh** lấy một bản tóm thô trong khi hạn mức của nó đã tự mở. Vá bằng `LIMIT_WINDOW_MAX_MIN`: cửa sổ hạn mức phiên là **5 giờ** ⟹ mốc dạng đồng hồ xa hơn 6 giờ là mốc **đã qua** ⟹ `ResetAlreadyPassed`, KHÔNG chuyển. Kèm một ghi nhận ngược: mutant "bỏ lượt cắt `(` trong `minutes_until_reset`" hoá ra **TƯƠNG ĐƯƠNG** (bài kiểm không đỏ) nên lượt cắt ấy đã bị **gỡ khỏi mã** thay vì giữ lại kèm một câu chuyện sai |
| — | ⚠ **Và lượt vá ấy lôi ra lỗi thứ hai: DÂY CHUYỀN** — `auto_limit_why` nay có phanh TUỔI (`AUTO_LIMIT_MIN_AGE_SEC = 600`, `pipeline.rs:2705`) | Đo trên log thật: `19:50:21` mở phiên `34f57a63` mang bản bàn giao dựng từ nhật ký ⟹ `19:52:39` **chính nó** bị chuyển tiếp (`auto_limit_firing session=34f57a63`) ⟹ `19:52:42` mở tiếp `3360dcc9`. Sổ `auto_limit:done` không đỡ được, và đó là phần đáng nhớ: mỗi vòng là một **id phiên MỚI** nên cuốn sổ trả lời đúng câu của nó mà dây chuyền vẫn chạy. **Đối chứng ngược đã chạy**: gỡ đúng cái phanh ⟹ 1 bài đỏ, `left: Do / right: TooYoung(138)`, exit 101 — 138 giây đúng bằng khoảng cách 19:50:21→19:52:39 |
| — | ⚠ **Lỗi thứ ba, và nó giết đúng cái phiên đang vá lỗi thứ hai**: `session_limit_on_screen` nhận theo HÌNH DẠNG, không theo trần độ dài (`keys.rs:4518`) | `22:05:13Z` `auto_limit_firing session=5dac6ac5 khi="…"` — dòng khớp là một **chú thích trong mã nguồn của chính bản vá** (`rust/tests/auto_limit_switch.rs:182`, 76 ký tự) nên **lọt trần 120**: câu TRÍCH bao giờ cũng ngắn hơn câu gốc, nên vặn trần xuống chỉ vừa đúng một mẫu. Hai cửa thay cho trần: dòng phải **mở đầu** bằng câu CLI in ra, và dòng có **dấu nháy ngược** là trích dẫn chứ không phải trạng thái. **Mỗi cửa một đối chứng ngược RIÊNG** — và lượt đo đầu bắt được một cửa chưa đo được: nới cửa "mở đầu dòng" ra thì **không bài nào đỏ** (`RED_B_EXIT=0`) vì cả ba mẫu đều mang dấu nháy nên cửa kia bắt trọn; thêm mẫu KHÔNG nháy rồi cấy lại ⟹ `RED_A_EXIT=101` · `RED_B2_EXIT=101`, `clippy --all-targets -- -D warnings` = 0. ⚠ **Full suite trên cây gộp CHƯA chạy được** — phiên `[huba] 6c9cf354` (acc3) đang sửa cùng cây, Hà chốt 2026-09-02 để phiên ấy giữ cây và chạy MỘT cổng cho cả hai |
| — | ✅ **Cổng gộp ấy ĐÃ chạy** — đóng dấu hỏi ở dòng trên | `2026-09-02 22:54`: `clippy --all-targets -- -D warnings` = **0** · full suite **127 binary, 0 fail**, `TEST_EXIT=0` · cài + `--verify` **KHỚP** (`bc92b2fa…`), daemon `pid 33406`, chữ ký `cert`, `schema_meta.version=5` |
| S30 | **Sổ TÀI KHOẢN CHẾT** — cái chết của một tài khoản sống LÂU HƠN cửa sổ báo nó | Hà 2026-09-02: `/new` lại nhảy vào acc1 đã bị tổ chức khoá. Cổng 31/08 chỉ loại một tài khoản khi ĐANG NHÌN THẤY một phiên mang dấu khoá — mà phiên bị khoá chết ngay dòng đầu, cửa sổ đóng, và trí nhớ duy nhất đi theo nó; vòng sau `quota` đọc `.claude.json` ra `92%` già ba ngày ⟹ `Unknown` đứng TRƯỚC `Full` ⟹ chọn lại đúng nó. Nay cái chết nằm trong SỔ: khoá `acc_dead:<tên>` trong `cursors`, hạng mới `Rank::Dead` (`quota::apply_dead_book`), `watch::reconcile_dead_book` đối chiếu mỗi vòng, `/accounts` in `⛔ TỔ CHỨC ĐÃ KHOÁ` để luật còn soi được. Gỡ khỏi sổ bằng **BẰNG CHỨNG SỐNG** (một phiên của nó đang CHẠY), KHÔNG bằng hạn dùng — một TTL sẽ âm thầm hồi sinh tài khoản vẫn đang chết và trả nguyên con bug về vào ngày thứ hai. **3 đối chứng ngược, cả 3 đỏ**: bỏ cửa `Dead` · bỏ cửa bằng-chứng-sống · `apply_dead_book` chạy không ⟹ `101/101/101`; phục hồi ⟹ `0` |
| S31 | **Menu ☰ xếp bằng ĐẾM 200 lượt gần nhất**, đếm theo TÊN lệnh chứ không theo `CommandKind` | Hà 2026-09-02: *"Thứ tự menu bạn đang sắp xếp theo quy tắc nào thế mà tôi thấy vô lý quá?"*. Đo trên sổ thật: `/enter`, `/right`, `/ctrlc` cùng `CommandKind::Key` nên dùng CHUNG một bộ đếm — **280.19 y hệt nhau** — nên thứ tự giữa ba dòng menu ấy là ngẫu nhiên vĩnh viễn, bấm bao nhiêu cũng không tách được. Ba tầng cũ (điểm theo `kind` · suy giảm nửa đời 7 ngày · hãm 25%) rút còn một, theo đúng luật Hà đặt: bảng `cmd_log(ts,name)` + đếm `MENU_WINDOW=200` + xếp giảm dần, hoà thì giữ thứ tự `ROUTES`. ⚠ **Cửa hãm 25% mất theo** (nó dựng 19/08 sau câu *"menu cứ nhảy loạn"*) — hệ quả không giấu mà ghi thành bài kiểm `without_the_brake_one_press_flips_the_top_pair`: `session` 60 / `shot` 61 là lật. **4 đối chứng ngược, cả 4 đỏ** (`101×4`) |
| — | ⚠ **Một bài kiểm tự ĐỎ theo ĐỒNG HỒ, không ai đụng vào mã** | `quota_from_the_cli_book_reaches_the_line` dựng bản đọc `week_resets_at=2026-09-02T10:59:59Z` rồi chấm *"đã dùng 22%"*, trong khi `accounts_text` tự hỏi đồng hồ THẬT ⟹ `cua_so` coi mốc ấy là ĐÃ QUA kể từ **10:59:59Z sáng 02/09** ⟹ `Rank::Unknown` ⟹ đỏ. Lượt full suite `07:26` cùng ngày xanh chỉ vì nó chạy TRƯỚC cái mốc — một lượt xanh **đi mượn đồng hồ**. Vá GỐC chứ không sửa mẫu: `now_ms` thành THAM SỐ của `accounts_text`. Ranh giới *"dựng chữ ≠ đi đo"* mà chính tệp ấy tự đặt đã kể tên `usage` và `quotas`, nhưng **bỏ sót THỜI GIAN** — thời gian cũng là một phép đo đi ra ngoài. **Đối chứng ngược**: trả về `quota::now_ms()` ⟹ `RED_T_EXIT=101`; phục hồi ⟹ `0` |
| S32 | **`huba handover` — đổi tài khoản NGAY TẠI phiên đang kẹt**, không cần điện thoại | Hà 2026-09-10: *"Giờ phiên đang bị kẹt vì limit làm thế nào, tôi đã bảo làm lệnh để chuyển tài khoản khi đang đứng ở phiên đó"*. Cửa vào mới, **không phải lõi mới**: arm `CommandKind::Handover` bị tách 171 dòng ra thành `pipeline::handover_now`, Telegram và CLI cùng gọi nó (chính arm ấy đã trả giá một lần cho luật "hai cửa một câu trả lời": `/new` gác tên tài khoản sai, `/handover -a` thì không). "Phiên nào" thì ĐO chứ không hỏi — lệnh chạy như CON của tiến trình `claude` ⟹ `sessions::session_of_this_terminal` leo dây tổ tiên (`ps -o ppid=`, trần 12 bậc), đường lùi là cùng `tty` và **chỉ nhận khi đúng một phiên** mang tty ấy (tty là số được dùng lại). **Đo thật 19:39**: `huba handover --dry-run` trong chính phiên này ra `54bd8153 · projects-4d · acc1 (ttys009)` → chọn `acc2`, đúng phiên đang gõ. `--dry-run` dừng TRƯỚC lượt gọi `claude` nên phép đo ấy tốn 0đ. ⚠ **CHƯA đo**: một lượt chuyển THẬT (tốn hạn mức + mở/đóng cửa sổ) |
| — | 🔴 **Và chính lượt ấy lôi ra vì sao 10 phiên kẹt vẫn KHÔNG được tự chuyển**: `suggest_account` hỏi *"có dòng hạn mức không"* thay vì *"mốc mở lại đã qua chưa"* | Đo 19:11 ngày 10/09: acc1 100% (tới 13/09) · acc3 100% (tới 15/09) · **acc2 còn `week 37% · 5h 32%`** — nhưng hai phiên acc2 còn mang banner `resets 3pm`, **mốc đã qua 4 tiếng**, dòng chữ nằm lại vì phiên bị chặn thì không in thêm gì. `watch.rs:963` đọc cái VẾT ấy thành "acc2 đang chặn" ⟹ loại acc2 ⟹ `NoAccount` cho **cả 8 phiên acc1** ⟹ `auto_limit_held` lặp mỗi vòng, không ai được chuyển. Đây đúng là tầng mà bản vá 09/09 (`80598be`) **cố ý chừa lại** với lời biện hộ *"chỉ làm huba dè dặt lâu hơn vài phút"* — câu ấy đo được là sai: cái vết không tự mất, nên nó khoá vĩnh viễn. Nay `suggest_account` nhận `now_min` và hỏi `limit_still_biting`; `account_dead` giữ nguyên `is_some()` (khoá tổ chức không có đồng hồ). **Đối chứng ngược đã chạy** (`.tmp/doi-chung-moc-da-qua.sh`): cấy lại `is_some()` ⟹ `RED_EXIT=101`, đỏ ĐÚNG bài mới; phục hồi + `touch` ⟹ `GREEN_EXIT=0`, 11/11. Bài mới tự nó **hai chiều**: 19:11 ⟹ chọn acc2; 14:30 (mốc còn 30 phút) ⟹ `None` |
| S33 | 🔴 **123/157 lượt cài bị macOS GIẾT lượt khởi chạy đầu** — và cái thước đo nó từng mang HAI NGHĨA, nay tách được | Báo cáo `.ips` nói thẳng: `termination {namespace:"CODESIGNING", code:4, indicator:"Launch Constraint Violation"}` · `EXC_CRASH · SIGKILL (Code Signature Invalid)`; `procLaunch 10:03:00.3921` → `captureTime 10:03:00.9291` ⟹ chết **0,54 giây** sau exec, `reportNotes:["dyld_process_snapshot_get_shared_cache failed"]` ⟹ chết TRƯỚC khi dyld kịp map shared cache, tức trước dòng đầu của `main`. Thước: `do-khe-cai-dat.py` (đưa ra khỏi `.tmp/` vì thư mục ấy bị `.gitignore`, tức mọi phép đo để trong đó là mất) đọc `hubd.err`, so mốc ký của binary (`hubd@<UTC>Z`) với mốc `hubd_boot_announced`. **MẪU SỐ: 177 dòng · 160 mốc ký · 157 lượt cài · 3 lượt khởi động máy KHÔNG xếp loại** ⟹ **123 bị giết · 34 lên thẳng**. ⚠ Thước ấy suýt là lời đồn: vòng 5 ra 6/6 "chậm 30s", vòng 6 đổi ĐÚNG MỘT biến (`ThrottleInterval` 30→1) ra **0/6** ⟹ khe 30 giây cũng là hình dạng của một job bị `kickstart` lại khi lần spawn trước còn mới, **không ai bị giết cả**. Vế còn thiếu nay được ĐO chứ không giả định — tuổi tiến trình cũ lúc ký: **123/123 lượt có tuổi 76s…86.786s > `ThrottleInterval`** ⟹ cửa throttle đã đóng ⟹ chỉ còn một cách giải thích. **0 hai nghĩa · 0 không đo được**. Đối chiếu chéo nguồn độc lập: 22/123 có `.ips` trong 180s (thư mục chỉ giữ 22 tệp và macOS bóp bản trùng ⟹ phần không khớp là *không được ghi*, không phải *không bị giết*) |
| — | **8 vòng dò, 0 lượt tái lập được** — loại được từng biến một, và một vòng "xanh" hoá ra là KHÔNG ĐO ĐƯỢC | Job thử nghiệm mang label riêng `com.dipgle.probe.cs`, không chạm daemon thật. Đã loại: **ký bằng cert** · **đè cùng đường dẫn** (30/30) · **launchd spawn** (10/10) · **binary `hubd` 8,7 MB THẬT** và **identifier `com.dipgle.hubd` đã được cấp TCC** (vòng 4, bản sao hubd thật, `HUB_CONFIG` trỏ tệp không tồn tại nên nó thoát ngay sau dòng đầu `main`) · **tiến trình cũ đang sống khi tệp bị thay dưới chân** (vòng 7: **0/6**, mỗi lượt để tiến trình sống 35s > throttle rồi mới `mv` + `kickstart -k`, đủ `KeepAlive` + `ThrottleInterval 30`). Một vòng 16 lượt bị vứt đúng cách: nhồi byte bằng `printf >>` phá cấu trúc Mach-O nên `codesign` từ chối ⟹ **KHÔNG ĐO ĐƯỢC**, không được đọc thành xanh. Khác biệt cấu trúc CUỐI CÙNG còn lại: plist job thật nằm trong `~/Library/LaunchAgents` từ 14/08 (⟹ là *background item* của macOS 13+, có sổ BTM kèm chữ ký tệp chạy), còn mọi vòng 3-7 đều bootstrap ad-hoc từ `.tmp`. **Vòng 8 đã chạy 20:13-20:16 ngày 10/09 với plist đặt đúng trong `~/Library/LaunchAgents`: 0/4, cả 4 lượt đều đo được** (dọn dẹp tự kiểm: plist đã xoá khỏi `LaunchAgents`, không còn tiến trình probe). Nhưng đúng như điều khai TRƯỚC khi bấm chạy ở dòng dưới, con số ấy **KHÔNG loại được biến** — nó rơi trọn trong cửa sổ mà chính production cũng sạch, và kịch bản in kèm đối chứng production cùng cửa sổ (`2s · 6s · 6s`) để người đọc thấy ngay điều đó |
| — | ⛔ **CỬA SỔ ĐO ĐANG ĐÓNG — và đó là lý do phải DỪNG bắn probe, không phải nghĩ thêm giả thuyết** | Từ `04:15:56Z` ngày 10/09 (sau lần khởi động máy `10:14` giờ máy) **4/4 lượt cài THẬT lên thẳng 2-6s**, trong khi tỉ lệ nền là 34/157 = 22% ⟹ P(4 sạch liên tiếp) ≈ **0,2%**. Chính production hiện KHÔNG ở trạng thái hỏng. Hệ quả đọc phán quyết: vòng 5-8 đều chạy trong cửa sổ này, nên **XANH của chúng KHÔNG loại được biến nào — chỉ ĐỎ mới kết luận được**. Cùng lý do, đối chứng "thứ tự cài" chạy 11:17 **không kết luận được gì**: vế cấy thứ tự CŨ và vế thứ tự MỚI đều ra 6s, và phép đo phụ của nó (đếm `.ips`) cho `22→22` ở CẢ HAI vế — một phép đo không bao giờ đỏ. (Kịch bản ấy còn chết ở dòng 59 vì lỗi bash 3.2 nuốt `$VAR` đứng cạnh ký tự nhiều byte ⟹ `SCRIPT_EXIT=1`, verdict không bao giờ được in ra.) **CHƯA đo, nói thẳng: nguyên nhân, và bản vá.** Việc đúng tiếp theo không phải vòng dò thứ 9 mà là **bắt trạng thái lúc nó quay lại** — thước đã nằm sẵn trong repo |
| — | 📐 **Full suite trên máy này tốn ~3,3 GIỜ, và không phải vì test chậm** | Lần chạy ĐẦU của mỗi binary vừa link xong bị chặn **~95 giây** với `user 0.00 sys 0.00` (đo `close_button-*`: `real 94.87`), lần thứ hai còn `0.01s` — đó là lượt quét lần-đầu của macOS (`syspolicyd`), không phải tính toán. `rust/tests/` có **127 tệp ⇒ 127 binary**, và **mọi thay đổi trong `src/` relink cả 127**, nên cái giá ấy trả lại từ đầu sau mỗi lượt sửa. Hâm song song KHÔNG cứu được: `xargs -P 24` cho cả 127 mất `WARM_SEC=12021` (3h20) — đúng bằng xếp hàng nối đuôi, tức `syspolicyd` phục vụ tuần tự. Chữa gốc (CHƯA làm, cần Hà bật tay): Privacy & Security → Developer Tools → Terminal |
| S34 | 🔴 **Một tài khoản CHƯA DÙNG ĐƯỢC không bao giờ được gợi ý** — hạng mới `Rank::NotReady`, và nó đứng cạnh `Dead` chứ không cạnh `Unknown` | Ca thật 2026-09-12, và nó tốn một lượt bàn giao: acc4 vừa khai vào `huba.config.json` lúc 17:33. Đăng nhập xong thì CLI ghi ngay `cachedUsageUtilization` = **0%** ⟹ `rank` đọc ra `Free(0)`, tức **rộng cửa nhất trong bốn tài khoản**; acc2 kịch trần lúc 17:3x ⟹ `suggest_account` chọn acc4 ⟹ cửa sổ `ttys007` mở ra và đứng ở **hộp chọn giao diện lần chạy đầu**, vì thư mục cấu hình mới chưa ai onboard. huba báo trung thực *"phiên CHƯA chào đời sau 12 giây"* — nhưng phiên cũ thì đã bị bỏ lại, đang bị chặn. Hai câu nghe giống nhau mà đi hai đường: *"chưa đo được hạn mức"* → `Unknown`, một cái đồng hồ chữa được; *"chưa có ai ngồi vào máy này"* → `NotReady`, **chỉ NGƯỜI chữa được**. Hai mốc đọc từ chính sổ của CLI, đo hai chiều trên bốn tài khoản thật: `oauthAccount` (có credential chưa) + `hasCompletedOnboarding` (lượt chạy đầu xong chưa) — và hàng đắt nhất là hàng GIỮA: acc4 sau khi đăng nhập có `oauthAccount` ✓ mà `hasCompletedOnboarding` ✗, tức **có credential chưa đủ** (`claude -p` trên acc4 trả lời bình thường trong khi cửa sổ tương tác vẫn đứng). Ba cửa: `quota::account_not_ready` (`quota.rs:396`) · `rank` chặn TRƯỚC mọi phép tính phần trăm (`quota.rs:412`) · `watch::suggest_account` loại `NotReady` cùng cửa với `Full`/`Dead` (`watch.rs:1007`). Kèm một tách nhỏ trong `quota::read`: **không có tệp** ⟹ chắc chắn chưa chạy `claude` lần nào ⟹ fail-closed; **đọc hỏng** (quyền/đĩa) ⟹ một trục trặc có thể tự qua, không được đóng vĩnh viễn một tài khoản đang tốt. 5 bài `rust/tests/tai_khoan_chua_dung_duoc.rs`, **3 đối chứng ngược, cả 3 đỏ** (`.tmp/doi-chung-12-09.sh`): bỏ cửa trong `rank` ⟹ `RED_A1=101`; `suggest_account` thôi loại `NotReady` ⟹ `RED_A2=101`; `account_not_ready` luôn trả `None` ⟹ `RED_C=101`; phục hồi ⟹ `0`. Bài tự nó **hai chiều**: cùng `0%` ấy, tài khoản dùng được vẫn phải ra `Free(0)`. Và thứ tự ba bước thêm tài khoản — cả ba lỗi khi làm sai đều IM LẶNG — nay là `scripts/them-tai-khoan.sh` chứ không phải mấy dòng gõ tay |
| — | 🔴 **Cửa sổ vừa mở nhận bừa phiên đang chạy ở cửa sổ KHÁC — ca tệ nhất trong họ này vì nó IM LẶNG** | Log 2026-09-12 18:36: `new_window_opened tty=ttys017` rồi `new_session_matched_by_transcript session=54bd8153… tty=ttys017` — mà `54bd8153` là phiên `[huba]` đang chạy ở **ttys009**, acc1, không liên quan gì tới cửa sổ acc4 vừa mở. huba nhắn *"nay đang theo phiên này"* ⟹ mọi câu chủ máy gõ tiếp đi thẳng vào một phiên KHÁC đang làm việc dở, còn phiên vừa sinh thì không ai biết id — đúng câu Hà hỏi: *"danh sách phiên cũng không thấy phiên mới đâu"*. Gốc: lối đoán *"tệp nhật ký nào vừa được ghi"* (`newest_transcript_since`), trong khi `projects` của acc2·acc3·acc4 đều là **liên kết mềm về `~/.claude/projects`** ⟹ một gốc chung ⟹ phiên đang gõ liên tục luôn thắng cuộc đua, còn phiên mới thì 15–60 giây nữa mới có tệp. Chú thích 2026-08-15 đã kể **đúng** con bug này (ba lượt ghép nhầm liên tiếp) và đã vá bằng cách hỏi tty TRƯỚC — nhưng lối đoán cũ ở lại làm đường lui, nên nó quay về nguyên vẹn đúng vào ngày phép hỏi-theo-tty chậm một nhịp: **vá một tầng không nói gì về tầng kia**. Nay `id_bound_elsewhere` (`sessions.rs:6190`) đối chiếu `sessionId` trong sổ CLI với tty của tiến trình đang SỐNG và fail-CLOSED khi `ps` không đọc được (giá của từ chối là một cái tên tạm `win-ttysNNN`; giá của nhận bừa là gõ vào việc của người khác); phần thuần `id_bound_elsewhere_in` (`sessions.rs:6160`) tách ra để kiểm được, vì cái sai ở đây không kêu. Kèm một **cái tên sai** bị sửa cùng lượt: dòng `new_session_matched_by_transcript` in cho CẢ HAI đường, nên đọc log tưởng lối đoán luôn chạy — nay là `new_session_matched`, đứng cạnh `new_session_matched_by_tty`. 4 bài `rust/tests/khong_cuop_phien_dang_chay.rs`, **2 đối chứng ngược hai CHIỀU, cả 2 đỏ**: luôn `false` (cú cướp quay lại) ⟹ `RED_B1=101`; luôn `true` (không phiên mới nào được nhận nữa) ⟹ `RED_B2=101` |
| — | ⚠ **Chính cái cửa mới ấy CHƯA được production chạm tới** — nói thẳng, vì ba lượt chạy thật đều đi đường khác | Đo trên log 2026-09-12 19:10–19:13, ba lượt `auto_limit_firing` acc2→acc4 liên tiếp (`hang:["acc1=ĐÃ KỊCH TRẦN","acc2=đã dùng 93%","acc3=ĐÃ KỊCH TRẦN","acc4=đã dùng 2%"]`): cả ba ghép đúng bằng tty — `1b1ed558`·`ttys019` · `e1728a42`·`ttys011` · `974cf68d`·`ttys008` — **0 dòng `transcript_guess_rejected`**. Nên bằng chứng chạy thật ở đây chứng minh *đường tty trả lời được*, `Rank::NotReady` không loại oan acc4 (đã onboard xong ⟹ `đã dùng 2%`), và cái tên `new_session_matched` in đúng chỗ; nó **không** chứng minh cửa chặn-cướp-phiên — cửa ấy chỉ có bằng chứng ở bàn kiểm. Ba lượt ấy cũng là bằng chứng cho S32/S29 đi trọn vòng: phiên `6fc47e02` (phiên vừa bàn giao lại cho phiên đang viết dòng này) bị chuyển, `focus_kept_on_auto_limit_switch` giữ tiêu điểm ở phiên Hà đang xem |
| — | **`/esc` — một cái TÊN ngắn, không phải một cơ chế** | Hà 2026-09-12: *"Tạo thêm lệnh esc đi đỡ phải gõ dài"*, sau một buổi gõ `/key esc` liên tục vì mọi cửa sổ acc4 vừa mở đều đón bằng một hộp hỏi (tin thư mục · tiện ích Chrome · giao diện) — `esc` là phím hay phải bấm nhất trên một phiên vừa sinh. Đi ĐÚNG route `/key` như `enter`/`right`: cùng `CommandKind::Key`, `Arg::Fixed("esc")`, cùng phép đọc màn và cùng cổng an toàn, **không đẻ nhánh xử lý mới** (`commands.rs:243`) |
| — | **Một tệp gửi lên mà không nhận được thì câu từ chối phải NÓI VÌ SAO** | Hà 2026-09-11: *"Sao gửi ảnh qua tele tự nhiên lại lỗi này: ⚠ không hỏi được Telegram đường dẫn của tệp ấy"* — rồi *"Chả nhẽ do trong ảnh có chuỗi gì đặc biệt?"*. Không phải nội dung tấm ảnh, và câu hỏi ấy chỉ nảy ra được vì câu trả lời **đã nằm trong tay huba mà bị vứt đi**: chuỗi `.ok().and_then(…)` cũ gộp **bốn** kết cục khác hẳn nhau — mạng hỏng · Telegram từ chối (HTTP 400 kèm `description` nói rõ) · thân không phải JSON · JSON không có `result.file_path` — thành một dòng duy nhất, nên ba dòng log của cả đời huba (16/08 · 04/09 · 11/09) không dòng nào chẩn đoán được, và người đọc phải đi đoán về NỘI DUNG tấm ảnh, thứ mà tới bước ấy huba **chưa tải về một byte nào**. `reqwest` coi HTTP 400 là một PHẢN HỒI chứ không phải `Err`, nên lời giải thích về tới tận nơi rồi mới bị `and_then` bỏ. Đo bằng chính token đang chạy 11/09: `getMe` → HTTP 200 `@ai_angles_bot`; `getFile` với `file_id` sai → **HTTP 400 · `Bad Request: invalid file_id`**. Nay `telegram::getfile_verdict` (`telegram.rs:3341`) trả `Result` và câu của Telegram đi **nguyên văn** ra tin nhắn kèm mã HTTP + một câu đóng đúng cái cửa đoán sai: *"chưa tải về byte nào, nên nội dung tệp KHÔNG phải nguyên nhân"*. Bước tải về tách ba kết cục theo cùng luật, và **bỏ câu đoán hộ** *"chắc quá 20 MB"* — trần ấy thực ra bị chặn ở bước `getFile` (`file is too big`), nên nó chỉ gợi ý sai chỗ đúng lúc người ta cần đi đúng chỗ. 4 bài `rust/tests/getfile_says_why.rs`, mọi mẫu là hình dạng THẬT của Telegram |
| — | ⚠ **Cây đã CÀI lên production lúc 18:57 trong khi `cargo check --all-targets` thì ĐỎ** — vì đường phát hành không bao giờ biên dịch tầng kiểm | Đo 2026-09-12 19:2x (phiên kế tiếp, acc4): thêm một trường vào `pub struct Quota` phá **ba** chỗ dựng struct nằm ngoài đường chạy — `quota.rs:488` (bài kiểm trong chính tệp ấy) và `rust/tests/config.rs:334`·`:386` — `error[E0063]: missing field 'chua_dung_duoc'`. `hubd` cài lúc 18:57 vẫn chạy đúng và ba lượt bàn giao 19:1x vẫn sạch, vì `cargo build --release` không biên dịch `#[cfg(test)]`. Đúng hình dạng §13: **"cài được" không phải một phép đo về cây kiểm** — hai thứ ấy đọc hai tập tệp khác nhau, nên một cái xanh không nói gì về cái kia. Vá ở cả ba chỗ, và cố ý **KHÔNG** dùng `..Default::default()`: để lần thêm trường sau, compiler còn bắt được từng chỗ dựng mà hỏi lại |

**UC-S11, bằng chứng chạy thật (2026-08-10, cả hai đường):**

| | hỏi lúc | Hà bấm | kết cục | phiên sau đó |
|---|---|---|---|---|
| đường thuận | 04:56:58 | ✅ Xác nhận (38s) | `Confirmed` → `session_stopped` | biến khỏi danh sách |
| đường chặn | 04:59:26 | ✖ Huỷ (48s) | `Declined` | **CÒN SỐNG · working** |

Phòng chat nói đúng cả chuỗi: `🔒 Đã gửi yêu cầu xác nhận sang Telegram… Chưa dừng
gì cho tới khi bấm nút.` → `✋ Đã huỷ trên Telegram — không dừng phiên nào.`

Mặt bằng: 4 tab (Phiên · Trao đổi · Sức khoẻ · Cấu hình), nghiệm thu ở **390×844**.

## Còn nợ, có sổ

- **Bản bàn giao từ nhật ký (`sessions::handover_from_journal`) mang thêm CÂY
  LÀM VIỆC THẬT — VÁ XONG 2026-09-05, ĐÃ ĐO BẰNG DÒ THẬT, CHƯA QUA MỘT LƯỢT
  HẾT HẠN MỨC THẬT.** Hà: *"Kiểm lại cách chuyển phiên khi bị limit, cần nhiều
  thông tin hơn để phiên mới không bị đi lạc hay thiếu ngữ cảnh cũ, hiện tại
  chỉ lấy log là chưa đủ"* — đúng sau một lượt bàn giao thật của chính huba:
  phiên nhận chỉ có **3 lượt nói** để đọc, vì cửa sổ 40 sự kiện cuối gần hết là
  `Edit`/`Bash`, và phải tự chạy `git status`/`git diff --stat` mới thấy ra 16
  tệp sửa dở trải trên ba việc khác nhau. Hai vá: (1) cửa sổ đọc thô 40 → 160
  sự kiện (cùng 256 KB tail, không đọc thêm byte nào, chỉ GIỮ nhiều hơn trước
  khi lọc `kind == "say"`); (2) thêm `working_tree_summary`, chạy `git status
  --short` + `git diff --stat HEAD` rồi nối vào bản bàn giao. Cái phải sửa
  ngay khi viết: hàm ĐẦU TIÊN chạy trên `session.cwd` thẳng, và đo trên chính
  workspace này (`huba sessions --json`) thì **mọi** phiên báo `cwd:
  "/Users/hanguyen/projects"` — cwd của tiến trình `claude` lúc khởi động,
  không đổi theo `cd` trong Bash — nên `git status` ở đó luôn trả "không phải
  repo". Đổi sang `config::project_dir(cfg, &session.folder)`, đúng tầng suy
  dự án đã có sẵn cho nhãn (`declared_parts`), chỉ rơi về `cwd` khi `folder`
  rỗng — cùng lắm bằng hành vi cũ. **Đã đo:** `cargo build --offline --lib`
  xanh 0 warning; bài DÒ thật `handover_from_journal_live` (`--ignored`) chạy
  trên đúng phiên `[huba] 2d091ed0…` đang viết dòng này — bản bàn giao ra 4123
  ký tự, phần cây làm việc khớp NGUYÊN VĂN `git status --short` +
  `git diff --stat HEAD` chạy tay cùng lúc (16 tệp, đúng `PLAN.md`,
  `sessions.rs`, `browser.rs`…). **CHƯA đo:** chưa thấy nó đi qua một lượt
  `auto_switch_on_limit` THẬT — một tài khoản thật hết hạn mức, cửa sổ mới mở
  ra, và bản bàn giao giàu hơn này thật sự giúp phiên mới không lạc — vì lượt
  ấy tốn thật (đổi tài khoản, mở/đóng cửa sổ) nên không tự tạo ra để kiểm.

- **`huba handover` — VIẾT XONG 2026-09-10, LƯỢT CHUYỂN THẬT CHƯA CHẠY.** Phần
  đo được đã đo: `--dry-run` nhận ra đúng phiên đang gõ (`54bd8153`, dây tổ
  tiên) và chọn đúng `acc2`. Phần CHƯA đo là phần tốn: một lượt bàn giao thật
  (gọi `claude` trên bản fork, mở cửa sổ mới, đóng cửa sổ cũ) — cố ý không tự
  tạo ra để kiểm, cùng lý do với `huba ask`. Ba nhánh chưa ai đi qua: bàn giao
  hỏng vì tài khoản cũ đã chết ⟹ rơi về `handover_from_journal`; cửa sổ mở được
  mà chưa ghép được id; cửa sổ cũ đóng hụt ⟹ `defer_close_to_book`. Cả ba đã có
  người dùng ở đường Telegram, nhưng "đường ấy chạy rồi" không phải bằng chứng
  cho lượt gọi từ CLI.

- **`huba ask <id> "<câu hỏi>"` — VIẾT XONG 2026-09-05, CHƯA CHẠY THẬT LẦN
  NÀO.** Bọc `sessions::ask_aside` (đúng cơ chế Telegram `/ask`, fork đọc-thật)
  thành CLI, gọi được từ Bash bởi bất kỳ phiên nào trên máy — không cần
  Telegram/chat_id. Dựng sau lượt phiên `A-dsign` hỏi thẳng qua `SendMessage`,
  đẩy tin vào giữa dòng hội thoại chính của `hub`; Hà: *"các phiên hỏi nhau
  phải hỏi riêng chứ không phải đẩy thẳng câu hỏi vào phiên"*. Tài liệu:
  `hub/docs/lien-phien.md`. **Đã đo:** `cargo build --release --offline --bin
  huba` xanh, `huba ask --help` in đúng. **CHƯA đo:** chưa gọi thật lên một
  phiên đang sống (cố ý — mỗi lượt fork tốn hạn mức thật của Hà, không tiêu
  thay khi chưa ai cần).

- **Debounce "đã tắt hẳn" cho phiên NỀN — VÁ XONG 2026-09-04, CHƯA THẤY LẠI CA
  THẬT ĐỂ ĐỐI CHỨNG.** Đo thật: `[fbot]·167252e2` báo "đã tắt hẳn" lặp lại
  hàng chục lần trong một ngày (11:01 · 14:02 · 15:00 · 17:20), trong khi
  `session_busy_by_shell` CÙNG GIÂY xác nhận tiến trình gốc (`pid 68743`) vẫn
  sống, đang bận. `claude agents` thỉnh thoảng không liệt kê ĐÚNG một phiên
  nền dù nó còn sống, trong khi các phiên khác CÙNG tài khoản vẫn liệt kê bình
  thường — cửa `blind` (cả tài khoản mù, luật 11b) không bắt được ca này vì nó
  chỉ soi theo TÀI KHOẢN, không soi theo TỪNG PHIÊN. Thêm `Mark::g` +
  `watch::BG_MISS_DEBOUNCE_SEC` (60s, gấp đôi nhịp quét ~20-30s đo được): phiên
  NỀN phải vắng mặt LIÊN TỤC đủ 60s mới bị kết luận "đã tắt", thay vì kết luận
  ngay ở vòng đầu vắng mặt. Chỉ áp cho phiên nền (`o == "background"`) — phiên
  có cửa sổ đã có phép thử riêng (`keys::window_of`) không cần debounce này.
  **Đã đo:** test mới `a_background_session_missing_once_is_not_reported_as_ended`
  (36/36 `watch.rs` xanh, dựng tay 3 vòng gọi `changes()` liên tiếp mô phỏng
  đúng chuỗi sự kiện thật). Build lại + `self-install` đã chạy — daemon đang
  chạy bản vá. **CHƯA đo:** chưa có thêm một đợt chớp-tắt thật nào của
  `claude agents` xảy ra kể từ khi vá để xác nhận nó không còn spam nữa —
  chỉ có thể verify khi ca thật lặp lại (không dựng lại được theo yêu cầu).

- **Click/fill/chụp ảnh Chrome thật + MCP `browser-mcp` — VIẾT XONG 2026-09-04,
  CHƯA CHẠM CHROME THẬT LẦN NÀO.** Hà: *"không dùng Playwright vì bị hạn chế
  dịch vụ"* + *"nó đóng vai trò như 1 mcp mới, giống như claude extension"*.
  Thêm vào `browser.rs`: `mang_ra_truoc`/`chup_anh` (chụp ảnh Chrome thật, tái
  dùng `keys::frame_is_blank`/`screen_locked`/`blank_frame_reason`),
  `bam`/`dien` (click/fill qua `execute javascript`, script CỐ ĐỊNH — selector
  + value nhúng qua `serde_json::to_string` RỒI `as_string`, hai tầng thoát,
  công thức đã kiểm tay khớp test). Thêm binary mới `browser-mcp` (tự viết tay
  JSON-RPC stdio, không SDK — không có crate MCP nào trong cargo cache
  offline), 7 tool 1-đối-1 với `browser::*`, không tool "chạy JS tuỳ ý". `/web
  anh` (Telegram) gọi `chup_anh` + `send_photo`, cùng khuôn `/anh`/`/web an`.
  **Đã đo:** `cargo test --offline` xanh, 0 warning (cả bộ, gồm test escape
  hai tầng + 6 test giao thức MCP nói chuyện thật với binary đã build qua
  stdin/stdout). **CHƯA đo — cần Hà tự chạy, sandbox không chạm được Chrome
  thật:** quyền Tự động hoá `hubd → Google Chrome` (dòng trống lúc viết
  `browser.rs` 23/08, chưa rõ đã cấp chưa) + `cargo test --offline --test
  browser_live -- --ignored --nocapture` cho `bam`/`dien`/`chup_anh` +
  `/web anh` thật qua Telegram + gọi tool MCP thật sau khi đăng ký
  `hub/.mcp.json`. Đừng đọc "cargo test xanh" thành "đã chạy được trên Chrome
  thật" — đúng luật honest-reporting của sổ này.
- 💣 **Bẫy đang nằm chờ: huba gõ vào `selected tab`, không phải tab của phiên.**
  `keys::do_script` và `keys::screen_text` đều nhắm `selected tab of window id
  W`, trong khi `window_of` chỉ tìm ra CỬA SỔ chứa tty ấy. Đo 2026-08-12: cả 4
  cửa sổ Terminal đều đúng 1 tab nên hiện chưa sai — nhưng mở tab thứ hai trong
  một cửa sổ là huba **gõ vào việc của người khác**. Vá đúng: `window_of` trả
  `(cửa sổ, tab)` rồi `do script … in tab i of window id W` (~15 chỗ gọi).
- ~~🐢 **`/shot` 22,2s · `/type` 10,9s · `/key` 7,5s**~~ → **đã vá 2026-08-12**
  (sổ + `ps` thay ảnh chụp). Mục dưới giữ lại vì nó ghi cái GỐC: (đo 2026-08-12 16:49–17:00).
  Cùng bệnh với `/session` vừa vá: hỏi `snapshot_cached` để tìm phiên. **Nhưng
  không được vá theo cùng một cách** — thứ chúng cần là `tty`, mà tty là con số
  ĐƯỢC DÙNG LẠI, nên một tty cũ trong sổ có thể đang là cửa sổ của phiên khác ⟹
  gõ nhầm cửa sổ (chính cái bẫy đã trả giá hôm nay). Đường đúng: hỏi **một tài
  khoản** mà sổ đã biết (`Mark::a`) thay vì cả ba, hoặc trị gốc cái chậm của
  `claude agents` — xem mục dưới.
- 🔴 **`claude -p "/usage"` TREO khi hubad gọi — chưa có thủ phạm** (mở
  2026-08-12). Cùng họ với việc `sessions_snapshot_ms` đo được **15–92 giây**
  mỗi vòng trong khi `claude agents --json` chạy tay chỉ **0,3 giây**. Hình dạng đã đo chắc: `timed_out: true · ms: 60952 ·
  stdout_bytes: 0 · stderr rỗng` — treo tới trần 60s, không ra byte nào. Hậu quả
  nhìn thấy được: hàng tài khoản trên tab Sức khoẻ **trống số hạn mức**
  (`fe-board-uc` 29/31). Đã loại bằng đo: stdin (đã đóng, `exec.rs:132`), sai
  binary (cùng `~/.npm-global/bin/claude`), môi trường launchd (chạy lại y hệt
  bằng `env -i` + cwd của hubad → **3,58s ra đủ số**), và **không phải do dời gốc
  workspace** — đếm log: 60 lần, lần đầu **10/08 05:51**, đi theo đợt. Nghi can
  còn lại chưa kiểm: chồng lấn với `claude agents` trong cùng một vòng. Phép đo
  kế tiếp: ghi kèm "lúc ấy còn lời gọi `claude` nào đang chạy không".

  ⚠ **Món nợ này thu hẹp ngày 2026-08-30, KHÔNG phải đã trả.** Số hạn mức nay
  đọc từ `<config_dir>/.claude.json`, khoá `cachedUsageUtilization` — chính CLI
  ghi, không cần dò (`rust/src/quota.rs`). Nên hai thứ từng phụ thuộc phép dò
  nay có số: hàng `/accounts` và luật chọn tài khoản
  (`watch::suggest_account`). Cái phép dò vẫn là đường DUY NHẤT làm được, và
  vẫn treo: **ép số tươi lại theo yêu cầu**. Tệp chỉ đổi khi chính CLI của tài
  khoản ấy chạy — đo được ngay 30/08: bản đọc của acc1 già hai ngày.

- ~~Chưa quan sát được một tin Telegram THẬT mang thông tin chốt (S18)~~ →
  **đã có, 18:17:13**: `⏸ projects-7c dừng, đang chờ bạn — sau 16 phút chạy` kèm
  nguyên khối thông tin chốt (mở bằng kết luận, có dấu đứt `⋯`, và **ba dòng
  cuối** — đúng thứ `key_points` đặt chỗ trước). Hà nhận được và bấm nút trên
  chính tin ấy (`telegram_command_queued /session e27806c2` lúc 18:17:45).
- **Còn nợ của S17**: tin THẬT của một phiên *dừng lại HỎI* vẫn chưa quan sát
  được — cần đúng lúc một phiên đang treo câu hỏi mà huba nhìn vào (khe mù ~139s).
- ~~**Phiên `projects-71` (pid 5001) tự cập nhật `claude` mỗi 30 phút**~~ →
  **đã đóng 2026-08-12 16:59** (Hà chốt). `kill 5001` xong: pid biến mất, không
  còn `npm install @anthropic-ai/claude-code` nào chạy, và huba báo đúng
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
- **Token Telegram mới chưa tới chỗ huba đọc** (đo 2026-08-12 19:30): huba nạp bí
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

- ~~Bốn bảng hộp thư chết trong `data/huba.sqlite`~~ → **đã dọn 2026-08-10** bằng
  **bước nâng cấp lược đồ 4**, không phải một lệnh gõ tay: nằm trong mã, có test,
  có log, chạy đúng một lần trên mọi máy. Chạy thật trên DB sống:
  `messages 200 · outbox 90 · decisions 87 · dead_letter 2`, mỗi bảng một dòng
  log kèm số dòng; còn lại đúng `cursors runs schema_meta spend`, lược đồ lên 4.
  ⚠ Bản đầu **giết daemon ngay lúc dựng lên**: bốn bảng ấy tham chiếu lẫn nhau
  mà `open()` bật `foreign_keys = ON` ngay trên đó — `FOREIGN KEY constraint
  failed` (787), `last exit code = 70`. Vá bằng cách TẮT kiểm khoá ngoại trong
  lúc dọn rồi bật lại kể cả khi hỏng, chứ không xếp thứ tự xoá: thứ tự đúng hôm
  nay là thứ tự sai vào ngày ai đó thêm một tham chiếu.
  📌 Lỗi ấy lộ ra trong 20 giây **nhờ đúng bản vá cùng ngày** cho `bin/hubad.rs`
  — trước đó nó chết bằng `eprintln!`, lý do chỉ nằm ở stderr của launchd.

- ~~"Bật lại máy thì huba có tự lên không" chưa nghiệm thu~~ → **mọi điều kiện
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
  `fe-smoke` exit 0 và daemon vẫn đẩy được ảnh chụp, tức huba còn đăng nhập tfl5.

- ~~`fe-sessions-uc` đỏ giả khi tập phiên vừa đổi~~ → đã cắm kẹp cùng khuôn với
  `fe-subagent-uc` (đọc sự thật — chờ màn bắt kịp — đọc lại; chỉ so khi hai đầu
  kẹp bằng nhau), và dấu vân tay của tập phiên nay gồm cả `working` nên phép đo
  chấm màu cũng được kẹp theo.

- ~~UC-S09 nửa "ảnh chụp đã cũ" chưa chạy~~ → **8/8** trên ảnh chụp **6.3 phút
  tuổi**, dựng bằng cách `bootout` `hubad` thật (huba mù ~6 phút, hai lượt). Và
  lượt đầu dạy đúng bài của dự án này: **7/7 xanh trong khi màn hình cắt cụt**
  câu cảnh báo — bảy assert đều đọc `textContent`, thứ có đủ chữ kể cả khi màn
  chỉ hiện tới `…Ảnh chụp lúc 19:50:59 1…`. Chỉ MỞ ẢNH RA NHÌN mới thấy nửa
  quan trọng nhất (*"Số dưới đây là của lúc đó, không phải bây giờ"*) không bao
  giờ tới mắt. Nay `.stale` được xuống dòng, và có phép đo hỏi `scrollWidth`.

- ~~Năm chỗ "lỗi im lặng"~~ → vá hết 2026-08-10 (Hà: *"làm nốt đi"*).
  **12 chỗ** đọc `db.get_cursor` nay đi qua `Db::cursor_or_log` — đặt chốt ở MỘT
  nơi vì mười hai chỗ gọi thì chỗ thứ mười ba sẽ quên; `bin/hubad.rs` chết bằng
  `logging::error` (ra cả stderr LẪN tệp log) thay vì `eprintln!` chỉ ra stderr
  của launchd; `sessions.rs` thôi khẳng định "đã dừng lại" khi lệnh dừng chưa
  chạy được — nay nói thẳng phiên còn sống kèm lệnh để tự dừng, và cả hai đường
  hỏng đều log; `config.rs` phân biệt "không có `huba.env`" (im, chuyện thường)
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
  thành "không có hộp chọn" rồi GỬI, tức hỏng về phía nguy hiểm đúng lúc huba mù
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
  `✋`/`⌛` (kết cục). Đo thật: huba trả lời `⌛ Hết hạn…` lúc 07:54:00 mà màn cũ
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
- huba **không tự tiêu hạn mức**; chỉ nút bấm của chủ máy mới gọi `claude`.
- Không con số `$` nào trên màn hình (sổ `spend` vẫn ghi, im lặng).
