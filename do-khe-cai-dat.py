#!/usr/bin/env python3
"""Lượt khởi chạy ĐẦU sau mỗi lần cài có bị kernel giết không — đo bằng KHE THỜI GIAN.

Vì sao không đếm tệp `.ips`: macOS bóp báo cáo trùng (`throttleTimeout: 30` ghi
ngay trong tệp) và ghi trễ, nên "không có tệp mới" đọc ra được cả hai nghĩa —
không có cú giết, HOẶC có mà không được ghi. Đo 2026-09-10: cấy lại đúng thứ tự
cài cũ vẫn cho 22→22, tức phép đếm ấy không đỏ nổi.

Thước thẳng hơn nằm sẵn trong `hubd.err`: `hubd_boot_announced` mang CẢ mốc ký
của binary (`hubd@<UTC>Z`) lẫn mốc nó lên được (`ts`). Lượt đầu bị giết ⟹ lượt
lên được phải đợi hết `ThrottleInterval` 30 giây của plist ⟹ khe ≥ ~30s.

🔴 CÁI BẪY CỦA CHÍNH THƯỚC NÀY, đo được 10/09 bằng đối chứng ngược: khe 30 giây
KHÔNG chỉ có một nghĩa. Một job bị `kickstart` lại khi lần spawn TRƯỚC còn mới
hơn `ThrottleInterval` cũng lên chậm đúng 30 giây — mà chẳng ai bị giết cả (vòng
5 ra 6/6 "chậm 30s"; vòng 6 đổi đúng một biến `ThrottleInterval` 30→1 thì ra
0/6). Nên "khe ≥ 30s" một mình là mệnh đề HAI NGHĨA.

Vế còn thiếu ấy đo được, và kịch bản này đo: **tuổi của tiến trình cũ lúc cài**
= mốc ký trừ đi lần `hubd_boot_announced` gần nhất TRƯỚC đó. Tuổi > 30s ⟹ cửa
throttle đã đóng từ lâu ⟹ khe 30s chỉ còn một cách giải thích là lượt spawn đầu
đã chết. Tuổi ≤ 30s ⟹ KHÔNG PHÂN BIỆT ĐƯỢC, và phải đếm riêng chứ không được
gộp vào phe nào.

Khai MẪU SỐ, và khai cả phần không xếp loại được.
"""
import re
import datetime
import os
import glob

ERR = os.path.expanduser("~/Library/Logs/hubd.err")
REPORTS = os.path.expanduser("~/Library/Logs/DiagnosticReports")
THROTTLE = 30          # ThrottleInterval của com.dipgle.hubd.plist
KHE_TOI_DA = 600       # khe lớn hơn ⟹ đây là lượt khởi động MÁY, không phải lượt cài

pat = re.compile(
    r'"binary":"[^"]*hubd@([0-9T:\-]+)Z"[^\n]*?"msg":"hubd_boot_announced"[^\n]*?"ts":"([0-9T:.\-]+)Z"'
)

dau = {}       # mốc ký -> khe nhỏ nhất (giây)
moi_boot = []  # mọi mốc lên được, để tính tuổi tiến trình cũ
tong_dong = 0
for line in open(ERR, errors="replace"):
    if "hubd_boot_announced" not in line:
        continue
    tong_dong += 1
    m = pat.search(line)
    if not m:
        continue
    ky = datetime.datetime.strptime(m.group(1), "%Y-%m-%dT%H:%M:%S")
    boot = datetime.datetime.strptime(m.group(2)[:19], "%Y-%m-%dT%H:%M:%S")
    moi_boot.append(boot)
    khe = (boot - ky).total_seconds()
    if khe < 0:
        continue
    if ky not in dau or khe < dau[ky]:
        dau[ky] = khe

moi_boot.sort()
trong_cua = {k: v for k, v in dau.items() if v <= KHE_TOI_DA}
ngoai_cua = len(dau) - len(trong_cua)


def tuoi_tien_trinh_cu(ky, khe):
    """Tiến trình cũ đã sống bao lâu tính tới lúc binary mới được ký.

    Trả None khi không có lần lên nào trước đó trong sổ — 'không đo được' là
    trạng thái riêng, không được đọc thành 'còn mới' hay 'đã cũ'.
    """
    boot_nay = ky + datetime.timedelta(seconds=khe)
    truoc = [b for b in moi_boot if b < boot_nay - datetime.timedelta(seconds=1)]
    if not truoc:
        return None
    return (ky - truoc[-1]).total_seconds()


cham = sorted((v, k) for k, v in trong_cua.items())
bi_giet = [(v, k) for v, k in cham if v >= THROTTLE]
lien_mach = [(v, k) for v, k in cham if v < THROTTLE]

print(f"MẪU SỐ: {tong_dong} dòng hubd_boot_announced · {len(dau)} mốc ký đọc được")
print(f"        {len(trong_cua)} mốc có khe ≤ {KHE_TOI_DA}s (lượt CÀI) · "
      f"{ngoai_cua} mốc khe lớn hơn (khởi động máy — KHÔNG xếp loại)")
print()
print(f"KHE ≥ {THROTTLE}s  ⟹ lượt đầu KHÔNG lên thẳng : {len(bi_giet)}")
print(f"KHE <  {THROTTLE}s ⟹ lượt đầu lên thẳng        : {len(lien_mach)}")
print()

# ── Tách hai nghĩa của "khe ≥ 30s" ──────────────────────────────────────────
that = ambigu = khong_do = 0
for khe, ky in bi_giet:
    tuoi = tuoi_tien_trinh_cu(ky, khe)
    if tuoi is None:
        khong_do += 1
    elif tuoi > THROTTLE:
        that += 1
    else:
        ambigu += 1

print(f"Trong {len(bi_giet)} lượt 'khe ≥ {THROTTLE}s', tách bằng TUỔI tiến trình cũ:")
print(f"  tuổi >  {THROTTLE}s ⟹ throttle đã hết hạn ⟹ BỊ GIẾT thật     : {that}")
print(f"  tuổi ≤  {THROTTLE}s ⟹ throttle cũng giải thích được ⟹ HAI NGHĨA : {ambigu}")
print(f"  không có lần lên nào trước đó trong sổ ⟹ KHÔNG ĐO ĐƯỢC        : {khong_do}")
print()

print("Mười lượt cài GẦN NHẤT, theo thời gian:")
for ky, khe in sorted(trong_cua.items())[-10:]:
    tuoi = tuoi_tien_trinh_cu(ky, khe)
    if khe < THROTTLE:
        nhan = "lên thẳng"
    elif tuoi is None:
        nhan = "KHÔNG ĐO ĐƯỢC (không có lần lên trước)"
    elif tuoi > THROTTLE:
        nhan = "BỊ GIẾT"
    else:
        nhan = "HAI NGHĨA (throttle cũng giải thích được)"
    t = "?" if tuoi is None else f"{tuoi:.0f}s"
    print(f"  ký {ky}Z  →  lên sau {khe:6.0f}s   [tiến trình cũ sống {t:>8}]   {nhan}")

# ── Đối chiếu chéo với một nguồn ĐỘC LẬP ────────────────────────────────────
crashes = []
for f in glob.glob(os.path.join(REPORTS, "hubd-*.ips")):
    m = re.match(r"hubd-(\d{4})-(\d{2})-(\d{2})-(\d{2})(\d{2})(\d{2})\.ips", os.path.basename(f))
    if m:
        crashes.append(datetime.datetime(*map(int, m.groups())) - datetime.timedelta(hours=7))
print()
print("ĐỐI CHIẾU CHÉO với báo cáo .ips (hai nguồn độc lập):")
khop = 0
for khe, ky in bi_giet:
    if any(0 <= (c - ky).total_seconds() <= 180 for c in crashes):
        khop += 1
print(f"  {khop}/{len(bi_giet)} lượt 'khe ≥ {THROTTLE}s' có một báo cáo .ips trong 180s sau lúc ký.")
print(f"  (thư mục báo cáo chỉ giữ {len(crashes)} tệp và macOS bóp bản trùng, nên")
print("   phần không khớp KHÔNG có nghĩa là không bị giết — chỉ là không được ghi.)")
