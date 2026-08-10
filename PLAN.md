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

**UC-S11, bằng chứng chạy thật (2026-08-10, cả hai đường):**

| | hỏi lúc | Hà bấm | kết cục | phiên sau đó |
|---|---|---|---|---|
| đường thuận | 04:56:58 | ✅ Xác nhận (38s) | `Confirmed` → `session_stopped` | biến khỏi danh sách |
| đường chặn | 04:59:26 | ✖ Huỷ (48s) | `Declined` | **CÒN SỐNG · working** |

Phòng chat nói đúng cả chuỗi: `🔒 Đã gửi yêu cầu xác nhận sang Telegram… Chưa dừng
gì cho tới khi bấm nút.` → `✋ Đã huỷ trên Telegram — không dừng phiên nào.`

Mặt bằng: 4 tab (Phiên · Trao đổi · Sức khoẻ · Cấu hình), nghiệm thu ở **390×844**.

## Còn nợ, có sổ

1. **UC-S02b (phiên có subagent)** — đường hiển thị mới chỉ ghim bằng unit test
   (13 test trong `tests/sessions.rs`); lúc nghiệm thu không phiên nào đang chạy
   subagent nên chưa thấy nó vẽ ra trên màn thật.
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
6. **`fe-newsession-uc` không còn tự chạy trọn vẹn được.** Bước `/stop` nay cần
   một ngón tay thật bấm Telegram; không ai bấm thì kịch bản in "BỎ QUA 2 kiểm
   tra" thay vì báo đỏ — sản phẩm lúc ấy đang cư xử đúng. Đây là **cái giá của
   chốt chặn**, không phải hỏng, nhưng nghĩa là UC-S06 từ nay là bán tự động.
7. **Bí mật đã từng vào git.** `2b6ea80` commit `.env` kèm `HUB_TFL5_USER` +
   `HUB_TFL5_PASSWORD`; repo chưa từng có remote nên nó chưa rời máy này. Đã
   `git rm --cached`, đã `chmod 600`, đã thêm vào `.gitignore`. **Còn nợ: đổi
   mật khẩu tfl5**, vì giá trị cũ vẫn nằm trong lịch sử `.git`.

## Đã trả xong (giữ lại vì sổ từng ghi là nợ)

- ~~`/tell` + `/stop` chưa nghiệm thu qua UI~~ → `fe-newsession` **22/22**
  (2026-08-10): mở phiên `58f37f0c` → dừng → nói tiếp, nhật ký dài ra
  32509→38438 byte.
- ~~`fe-phone-uc` còn đỏ~~ → **31/31**.

## Nguyên tắc còn giữ

- Mọi thứ đổi trạng thái đi qua **lệnh trong phòng chat** → luôn có dấu vết.
- Phiên nền chạy sau `DENIED_TOOLS`; hỏi/bàn giao chạy trên **fork** read-only.
- hub **không tự tiêu hạn mức**; chỉ nút bấm của chủ máy mới gọi `claude`.
- Không con số `$` nào trên màn hình (sổ `spend` vẫn ghi, im lặng).
