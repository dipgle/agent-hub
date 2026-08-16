# Từ màn phiên tới cái nút bấm — đường đi thật, từng chặng

Hà 2026-08-16: *"vẽ cho tôi flow bạn đang chạy để tôi xem vấn đề nằm ở đâu"*,
sau ảnh chụp `/shot` của `[dwork]` có **7 dòng lệnh mà chỉ 3 icon**.

Mỗi ô dưới đây là một chỗ có thể LÀM RƠI một dòng lệnh. Cột phải ghi rõ nó rơi
vì cái gì — vì đó là chỗ anh cần nhìn khi thiếu nút.

```
   ┌─────────────────────────────────────────────────────────────┐
   │  A. NGUỒN CHỮ                                               │
   │  sessions::commands_of(cfg, sid, max)                       │
   │  → đọc NHẬT KÝ của phiên (không đọc màn!)                   │
   └─────────────────────────────────────────────────────────────┘
        │  ⚠ Đọc lượt CUỐI của nhật ký. Lệnh nằm ở lượt trước đó
        │    thì không tới được đây.
        ▼
   ┌─────────────────────────────────────────────────────────────┐
   │  B. HÀNG RÀO NHẬN DẠNG   keys::commands_in_report            │
   │  giữ lại dòng nào TRÔNG NHƯ một lệnh                         │
   └─────────────────────────────────────────────────────────────┘
        │  RƠI ở đây nếu:
        │   · từ đầu dòng không nằm trong KNOWN (git, bash, cargo,
        │     npm, docker, …) — `cd` KHÔNG có trong danh sách
        │   · `cd X && lệnh` thì lấy phần sau `&&`; nhưng `cd` đứng
        │     RIÊNG MỘT DÒNG thì không cứu được dòng nào cả
        │   · dài quá 200 ký tự (một KHỐI, không phải một lệnh)
        │   · trông như văn xuôi (`looks_like_prose`)
        │   · câu đang CẤM lệnh ấy (`forbids`)
        │   · lệnh phá hoại: rm, git reset --hard, kill… → CỐ Ý bỏ,
        │     nay có log `cmd_no_button_destructive` + một dòng
        │     trong tin nói ra
        ▼
   ┌─────────────────────────────────────────────────────────────┐
   │  C. TRẦN SỐ LƯỢNG   `max`                                    │
   │  giữ phần CUỐI, cắt phần ĐẦU                                 │
   └─────────────────────────────────────────────────────────────┘
        │  ⚠⚠ ĐÂY LÀ CHỖ ĐÃ LÀM RƠI CẢ HAI LẦN ANH BẮT ĐƯỢC.
        │  Trước 16/08: max = 4, và cắt IM LẶNG.
        │  Nay: max = 12, và mỗi lần cắt có `cmds_truncated` trong log.
        ▼
   ┌─────────────────────────────────────────────────────────────┐
   │  D. QUÉT BÍ MẬT TRÊN DÒNG LỆNH   redaction::file_risk         │
   │  dòng mang GIÁ TRỊ bí mật (PGPASSWORD=…, sk-…) → bỏ          │
   └─────────────────────────────────────────────────────────────┘
        │  Có log `cmd_withheld`. Cân HẸP, không phải cân của phần
        │  xem trước — nếu không thì mọi lệnh có `/Users/…` đều rơi.
        ▼
   ┌─────────────────────────────────────────────────────────────┐
   │  E. CẤT SỔ + SINH MÃ   pipeline::remember_quick / quick_token │
   │  mỗi lệnh một mã hex 8 ký tự, gắn với ĐÚNG phiên ấy           │
   └─────────────────────────────────────────────────────────────┘
        │  Mã riêng để nút của một tin CŨ không chạy việc của tin MỚI.
        ▼
   ┌─────────────────────────────────────────────────────────────┐
   │  F. DÁN VÀO CHỮ   pipeline::html_with_links                   │
   │  tìm dòng trong TIN có chứa lệnh ấy → bọc <code> → dán icon   │
   └─────────────────────────────────────────────────────────────┘
        │  RƠI ở đây nếu dòng lệnh trong TIN không khớp chuỗi trong
        │  SỔ — hai nguồn khác nhau (nhật ký vs màn) nên chữ có thể
        │  lệch. Không khớp ⟹ tụt xuống một cái NÚT ở đáy tin.
        │  Khớp theo phần đầu chỉ được tính khi lệnh BẮT ĐẦU dòng
        │  (nếu không thì icon rơi vào giữa một câu văn — 16/08).
        ▼
   ┌─────────────────────────────────────────────────────────────┐
   │  G. HAI ĐÍCH CHẠM cho mỗi dòng                                │
   │  ▶️ run_<mã>   → hub chạy /bin/zsh -lc, chờ xong, dán tóm tắt │
   │                  vào phiên                                     │
   │  🖥 term_<mã>  → mở cửa sổ Terminal, gõ lệnh vào, chuyển con   │
   │                  trỏ sang cửa sổ ấy                            │
   └─────────────────────────────────────────────────────────────┘
```

## Ảnh `[dwork]` đọc theo bảng trên

```
cd ~/projects/dwork/scripts               ← rơi ở B (`cd` đứng riêng một dòng)
bash ./dci-probe-rcat04j-state.sh         ← rơi ở C (trần 4, cắt từ đầu)
bash ./dci-uc-m6-rcat04j-web.sh           ← rơi ở C
bash ./dci-uc-m6-rcat04j-mobile.sh        ▶️
bash ./dci-uc-w03-so-phep-tru.sh          ▶️
bash ./dci-uc-nghi53-inactive-filter.sh   ▶️
bash ./dci-smoke-man-web.sh               ← rơi ở C
rm ~/…/__tu_kiem_no_undef.js              ← rơi ở B, CỐ Ý (lệnh xoá)
```

## Còn một cái bẫy chưa vá, và nó nguy hiểm hơn thiếu nút

Sáu dòng `bash ./dci-*.sh` là đường **tương đối**, chúng chỉ đúng sau dòng
`cd ~/projects/dwork/scripts`. Nhưng chặng E cất kèm `cwd` = **thư mục của
phiên**, không phải thư mục của dòng `cd` ngay trên nó. Nên bấm ▶️ có thể chạy
`bash ./dci-…sh` ở sai thư mục — và kết cục là "không tìm thấy tệp", chứ không
phải chạy nhầm thứ khác.

Chưa vá vì nó cần đọc `cd` như một thứ ĐỔI TRẠNG THÁI cho những dòng sau nó, mà
đó là một phép đọc mới chứ không phải nới một hàng rào. Ghi ra đây để nó không
biến mất.
