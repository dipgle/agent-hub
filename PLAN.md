# PLAN — hub

**Mục tiêu duy nhất:** từ điện thoại, xem và điều khiển các phiên `claude` đang
chạy trên Mac này. Không hộp thư, không triage, không tự tiêu tiền.

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

Mặt bằng: 4 tab (Phiên · Trao đổi · Sức khoẻ · Cấu hình), nghiệm thu ở **390×844**.

## Còn nợ, có sổ

1. **`/tell` + `/stop` chưa nghiệm thu qua UI.** Cơ chế có test thật, nhưng cần
   một phiên nền chạy được — mà mọi phiên nền mở trong workspace này đều kẹt ở
   hộp thoại duyệt MCP. **Việc của Hà, một lần cho mỗi dự án:**
   `cd ~/Documents/projects/AI/<dự án> && claude` → **Esc** → thoát.
2. **UC-S02b (phiên có subagent)** — chưa có mẫu thật để biết nên hiện thế nào.
3. **UC-S09 nửa "ảnh chụp đã cũ"** — phải tắt `hubd` rồi chờ qua 5 phút mới thấy;
   chưa chạy.
4. **`fe-phone-uc` (kiểm ergonomics) còn đỏ**, và đỏ từ trước đợt dọn: chữ 11.2px
   (`span.room`, trạng thái kết nối) và các nút tab cao 35px < chuẩn chạm 44px.
   Đây là nợ thiết kế thật, không phải hỏng do đợt xoá.
5. **Bảng cũ trong `data/hub.sqlite`** (`messages`, `decisions`, `outbox`,
   `dead_letter`) vẫn còn dữ liệu. Không có mã nào đọc chúng. Muốn dọn thì phải
   là một quyết định có chủ ý, không phải tác dụng phụ của việc đổi schema.

## Nguyên tắc còn giữ

- Mọi thứ đổi trạng thái đi qua **lệnh trong phòng chat** → luôn có dấu vết.
- Phiên nền chạy sau `DENIED_TOOLS`; hỏi/bàn giao chạy trên **fork** read-only.
- hub **không tự tiêu tiền**; chỉ nút bấm của chủ máy mới gọi `claude`.
- Không con số tiền nào trên màn hình (sổ `spend` vẫn ghi, im lặng).
