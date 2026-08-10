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
| S02b | Phiên **đang chạy subagent** thì màn nói ra | `fe-subagent-uc` 6/6 trên subagent THẬT (3 đường thông báo, đo trên 384 tệp) |

**UC-S11, bằng chứng chạy thật (2026-08-10, cả hai đường):**

| | hỏi lúc | Hà bấm | kết cục | phiên sau đó |
|---|---|---|---|---|
| đường thuận | 04:56:58 | ✅ Xác nhận (38s) | `Confirmed` → `session_stopped` | biến khỏi danh sách |
| đường chặn | 04:59:26 | ✖ Huỷ (48s) | `Declined` | **CÒN SỐNG · working** |

Phòng chat nói đúng cả chuỗi: `🔒 Đã gửi yêu cầu xác nhận sang Telegram… Chưa dừng
gì cho tới khi bấm nút.` → `✋ Đã huỷ trên Telegram — không dừng phiên nào.`

Mặt bằng: 4 tab (Phiên · Trao đổi · Sức khoẻ · Cấu hình), nghiệm thu ở **390×844**.

## Còn nợ, có sổ

1. **Sáu chỗ "lỗi im lặng" đã được xác minh bằng file:line, chưa vá.** Nặng nhất:
   `keys::screen_of` (`keys.rs:378-393`) trả `None` cho **ba** chuyện khác hẳn
   nhau — không có cửa sổ, `osascript` hỏng (`.ok()?` hai lần), **và màn hình có
   dấu hiệu lộ bí mật** (`keys.rs:381`) — còn `pipeline.rs:924` đọc cả ba thành
   một: `None.is_some_and(..)` = `false` ⟹ chốt **hỏng về phía GỬI**, tức gửi
   phím mũi tên đúng lúc đang có hộp thoại chọn, thứ mà chú thích ngay trên nó
   gọi là "không lùi lại được". Đường thứ ba là tệ nhất: đúng lúc màn đang hiện
   một mật khẩu thì chốt mở toang. Kế đó:
**12 chỗ** đọc `db.get_cursor` làm "SQLite hỏng" và "chưa chọn phiên nào" nói
   cùng một câu — tự đếm lại: **9** chỗ `.ok()` (`portal.rs:153,162` ·
   `pipeline.rs:104,575,735,785,844,1014` · `bin/hubd.rs:457`) và **3** chỗ
   `match … _ =>` (`portal.rs:90` · `pipeline.rs:1076` · `bin/hubd.rs:496`);
   hai chỗ `match` còn lại (`pipeline.rs:285,373`) thì CÓ log, không tính.
   Rồi: `bin/hubd.rs:233` chết bằng `eprintln!` nên lý do không vào sổ nào;
   `sessions.rs:1758` bail! khẳng định "đã dừng lại" cả khi lệnh dừng chưa từng
   chạy được; `config.rs:602` coi "không đọc được `hub.env`" y như "không có
   file"; `adapters/tfl5.rs:405,527` bỏ rơi kết quả `set_read_timeout` (và chỉ
   đặt cho nhánh **không TLS**).
2. **UC-S09 nửa "ảnh chụp đã cũ"** — phải tắt `hubd` rồi chờ qua 5 phút mới thấy;
   chưa chạy.
3. **Bảng cũ trong `data/hub.sqlite`** (`messages`, `decisions`, `outbox`,
   `dead_letter`) vẫn còn dữ liệu. Không có mã nào đọc chúng. Muốn dọn thì phải
   là một quyết định có chủ ý, không phải tác dụng phụ của việc đổi schema.
4. **Chữ ký ổn định mới nghiệm thu tới bước "bản cài mang DR cố định"**
   (2026-08-10): đã đo hai build khác byte cùng một designated requirement, và
   `hubd` tự khai `kind: cert` khi chạy. Chưa đo được vế cuối — **bật lại máy thì
   hub có tự lên không** — vì việc đó phải reboot thật.
5. ~~Hai hàng mới ở tab Sức khoẻ chưa nhìn thấy trên UI thật~~ → đã thấy:
   `fe-board` 27/27 trên daemon do launchd sở hữu.
6. **`fe-sessions-uc` đỏ giả khi tập phiên vừa đổi.** Nó đọc sự thật từ
   `hub sessions --json` một lần rồi so với màn, mà ảnh chụp trên trang trễ tới
   ~25 giây ⟹ dừng/mở một phiên ngay trước lúc chạy là ra `màn 6 / máy 5`. Đo
   2026-08-10: đỏ 3 dòng, chờ 45 giây chạy lại thì **xanh, exit 0**. Cách chữa đã
   có sẵn khuôn trong `fe-subagent-uc.mjs`: đọc sự thật — đọc màn — đọc lại sự
   thật, chỉ so khi hai đầu kẹp bằng nhau.
7. **`fe-newsession-uc` là kịch bản bán tự động.** Bước `/stop` cần một ngón tay
   thật bấm Telegram; không ai bấm thì kịch bản in **"BỎ QUA 2 + 3 kiểm tra"** kèm
   tên từng kiểm tra chưa nghiệm thu, và vẫn thoát 0 — sản phẩm lúc ấy đang cư xử
   đúng. (Câu này trước đây là **ý định chứ không phải hành vi**: đo 2026-08-10
   chiều thì nó báo đỏ 3 dòng, vì hai lỗi đã vá cùng ngày — xem "Đã trả xong".)
8. **Một quan sát trên màn, chưa quyết.** Hàng phụ của thẻ phiên cố ý **một dòng,
   cắt đuôi** (`fe/index.html:425`). Khi có subagent, hàng thành `acc2 · tự duyệt ·
   1 subagent đang chạy · ngữ cản…` — tức **mất con số ngữ cảnh**, thứ dùng để
   quyết định "có nên đóng sổ chưa". Ba đường: cho hàng xuống hai dòng (tốn ~18px
   mỗi thẻ), rút gọn chữ subagent, hoặc để nguyên vì màn chi tiết đã có đủ. Chưa
   tự chọn — đây là đánh đổi bố cục, không phải lỗi.
9. **Bí mật đã từng vào git.** `2b6ea80` commit `.env` kèm `HUB_TFL5_USER` +
   `HUB_TFL5_PASSWORD`; repo chưa từng có remote nên nó chưa rời máy này. Đã
   `git rm --cached`, đã `chmod 600`, đã thêm vào `.gitignore`. **Còn nợ: đổi
   mật khẩu tfl5**, vì giá trị cũ vẫn nằm trong lịch sử `.git`.

## Đã trả xong (giữ lại vì sổ từng ghi là nợ)

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
