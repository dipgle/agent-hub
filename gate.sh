#!/usr/bin/env bash
# Cổng gộp, và lần này CHẠY TEST BINARY TỪ MỘT THƯ MỤC SẠCH.
#
# Vì sao — đo 13/09, cùng một chuỗi byte đặt ở bốn chỗ, chạy lần đầu:
#   .tmp/verbs_copy                       1 giây   (ngoài target/)
#   target/debug/deps/verbs-7c82…       103 giây
#   target/debug/deps/zz_probe_dir      121 giây   (đổi tên, vẫn trong deps/)
#   target/debug/zz_thu/zz_probe_sach     0 giây   (trong target/debug/, thư mục TRỐNG)
# ⇒ thủ phạm là **chính thư mục `target/debug/deps/`**, đang có **1.375.586 mục**.
# Không phải nội dung, không phải kích thước, không phải chữ ký, không phải cargo.
#
# Nên: dựng bằng `--no-run`, CHÉP từng binary sang một thư mục trống, rồi chạy
# BẢN CHÉP. Cổng 13/09 chạy tại chỗ mất 5h18; phần lớn là ~125 giây/binary chờ.
#
# Ba chỗ dễ thành xanh giả, bịt:
#  ① MẪU SỐ in ra; `--no-run` đỏ ⇒ dừng, không chấm.
#  ② Doctest không nằm trong binary nào ⇒ chạy riêng `--doc`.
#  ③ Chép hỏng là TRẠNG THÁI RIÊNG (mã thoát 2), không lẫn vào nhóm xanh — một
#     binary không chép nổi thì lượt chạy sau đó không nói được gì về nó.
# Và ④: bộ chạy này MỚI, nên nó phải chứng minh ĐỎ ĐƯỢC trước khi ai tin màu xanh
#     của nó — có một ca đối chứng ngược chạy qua ĐÚNG đường dẫn ấy ở ngay đầu.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUST="$HERE/rust"
OUT="$HERE/.tmp"
SAN="$OUT/binrun"
cd "$RUST" || exit 2
moc() { date -u +%Y-%m-%dT%H:%M:%SZ; }

# ── ⓪ MỘT LƯỢT MỘT LÚC ────────────────────────────────────────────────────────
#
# 🔴 Vì sao có khoá, và nó đã trả giá BA lần trong hai ngày. Cả lượt chạy đều ghi
# chung `$OUT/cong-sach-{bin,dachep,ket}.txt`; mỗi lượt mở đầu bằng `: >` (cắt
# tệp) rồi nối tiếp vào. Hai lượt chồng nhau ⟹ mẫu số của lượt này cộng vào kết
# quả của lượt kia: đo 2026-09-16 sáng ra `MAU_SO=152` mà `DA_CHEP=277` ·
# `DA_CHAY=553`. Cổng xử ĐÚNG (fail-closed vì `DA_CHAY != MAU_SO`); cái sai là
# cách chạy nó. Lần thứ ba, cùng ngày 18:0x: một `gate.sh` MỒ CÔI (`ppid 1`, cha
# đã chết theo phiên) chạy từ 15:54:52 song song với một lượt mới — không ai
# thấy, vì bản bàn giao khai là nó "đã mất dấu".
#
# `mkdir` là thao tác NGUYÊN TỬ trên POSIX: hai tiến trình cùng gọi thì đúng một
# cái thành công. Đó là cả cơ chế — không cần `flock` (macOS không có sẵn) và
# không cần đọc-rồi-ghi (chỗ đua nằm đúng giữa hai bước ấy).
#
# Khoá MỒ CÔI phải cướp được, nếu không một lượt bị `kill -9` sẽ khoá cổng vĩnh
# viễn — và một cổng không bao giờ chạy được cũng vô dụng y như một cổng không
# bao giờ đỏ. Nên khoá mang PID: chủ còn sống ⟹ TỪ CHỐI; chủ đã chết ⟹ cướp và
# NÓI RA (im lặng cướp khoá là đúng thứ khoá này sinh ra để chặn).
KHOA="$OUT/gate.lock.d"
mkdir -p "$OUT"
if ! mkdir "$KHOA" 2>/dev/null; then
  CHU=$(cat "$KHOA/pid" 2>/dev/null || echo "")
  if [ -n "$CHU" ] && kill -0 "$CHU" 2>/dev/null; then
    echo "TEST_EXIT=2  # KHÔNG ĐO ĐƯỢC: đã có lượt gate.sh đang chạy (pid $CHU) — hai lượt chung tệp đếm thì mẫu số hỏng"
    echo "GATE_DONE=$(moc)"
    exit 2
  fi
  echo "KHOA_MO_COI: pid ${CHU:-?} không còn sống — cướp khoá"
  rm -f "$KHOA/pid" 2>/dev/null
fi
echo "$$" > "$KHOA/pid"
# Nhả khoá ở MỌI đường ra, kể cả `exit 2` ở các cửa "không đo được" bên dưới.
# `trap` chỉ chạy cho tiến trình này; `kill -9` thì mục "cướp khoá" ở trên lo.
nha_khoa() { rm -f "$KHOA/pid" 2>/dev/null; rmdir "$KHOA" 2>/dev/null; }
trap nha_khoa EXIT

echo "STARTED=$(moc)"
mkdir -p "$SAN"

# chạy một tệp thực thi, in "<mã> <giây> <tên>" — MỘT đường duy nhất, dùng cho cả
# ca đối chứng lẫn ca thật, để đối chứng nói đúng về thứ đang được dùng.
chay_mot() {
  local b="$1" t ma
  t=$(date +%s)
  "$b" > "$OUT/cong-sach-log.tmp" 2>&1
  ma=$?
  echo "$ma $(($(date +%s) - t)) $(basename "$b")"
}

# ── ⓪ ĐỐI CHỨNG NGƯỢC: bộ chạy này có đỏ được không ─────────────────────────
printf '#!/bin/sh\nexit 101\n' > "$SAN/dc_hong"
printf '#!/bin/sh\nexit 0\n' > "$SAN/dc_lanh"
chmod +x "$SAN/dc_hong" "$SAN/dc_lanh"
DC_HONG=$(chay_mot "$SAN/dc_hong" | awk '{print $1}')
DC_LANH=$(chay_mot "$SAN/dc_lanh" | awk '{print $1}')
echo "DOI_CHUNG: ca_hong=$DC_HONG (phải 101) · ca_lanh=$DC_LANH (phải 0)"
if [ "$DC_HONG" != "101" ] || [ "$DC_LANH" != "0" ]; then
  echo "TEST_EXIT=2  # KHÔNG ĐO ĐƯỢC: bộ chạy không phân biệt được đỏ với xanh"
  echo "GATE_DONE=$(moc)"
  exit 2
fi

# ── ① fmt ────────────────────────────────────────────────────────────────────
cargo fmt --all --check
FMT_EXIT=$?
echo "FMT_EXIT=$FMT_EXIT"

# ── ② clippy ─────────────────────────────────────────────────────────────────
t0=$(date +%s)
cargo clippy --offline --all-targets -- -D warnings
CLIPPY_EXIT=$?
echo "CLIPPY_EXIT=$CLIPPY_EXIT  # $(($(date +%s) - t0))s"
if [ "$CLIPPY_EXIT" -ne 0 ]; then
  echo "TEST_EXIT=2  # KHÔNG ĐO ĐƯỢC: clippy đỏ, chạy suite trên cây không ai ship là đo hư không"
  echo "GATE_DONE=$(moc)"
  exit "$CLIPPY_EXIT"
fi

# ── ③ dựng, lấy đường dẫn từ JSON (KHÔNG glob: deps/ có 1,37 triệu mục) ──────
t0=$(date +%s)
cargo test --offline --no-run --message-format=json > "$OUT/cong-sach-build.json" 2>"$OUT/cong-sach-build.err"
BUILD_EXIT=$?
echo "BUILD_EXIT=$BUILD_EXIT  # $(($(date +%s) - t0))s"
if [ "$BUILD_EXIT" -ne 0 ]; then
  tail -30 "$OUT/cong-sach-build.err"
  echo "TEST_EXIT=2  # KHÔNG ĐO ĐƯỢC: không dựng nổi test binary"
  echo "GATE_DONE=$(moc)"
  exit 2
fi
python3 - "$OUT/cong-sach-build.json" "$OUT/cong-sach-bin.txt" <<'PY'
import json, sys
d = set()
for l in open(sys.argv[1], encoding="utf-8", errors="replace"):
    try:
        m = json.loads(l)
    except json.JSONDecodeError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m.get("profile", {}).get("test"):
        d.add(m["executable"])
open(sys.argv[2], "w").write("\n".join(sorted(d)) + "\n")
PY
MAU_SO=$(grep -c . "$OUT/cong-sach-bin.txt")
echo "MAU_SO=$MAU_SO"
if [ "$MAU_SO" -eq 0 ]; then
  echo "TEST_EXIT=2  # KHÔNG ĐO ĐƯỢC: 0 binary — mẫu số rỗng không phải là xanh"
  echo "GATE_DONE=$(moc)"
  exit 2
fi

# ── ④ CHÉP ra thư mục sạch ───────────────────────────────────────────────────
t0=$(date +%s)
CHEP_HONG=0
: > "$OUT/cong-sach-dachep.txt"
while IFS= read -r b; do
  [ -n "$b" ] || continue
  if cp "$b" "$SAN/$(basename "$b")" 2>/dev/null; then
    echo "$SAN/$(basename "$b")" >> "$OUT/cong-sach-dachep.txt"
  else
    CHEP_HONG=$((CHEP_HONG + 1))
    echo "CHÉP HỎNG: $b"
  fi
done < "$OUT/cong-sach-bin.txt"
DA_CHEP=$(grep -c . "$OUT/cong-sach-dachep.txt")
echo "CHEP_WALL=$(($(date +%s) - t0))s · DA_CHEP=$DA_CHEP/$MAU_SO · CHEP_HONG=$CHEP_HONG"

# ── ⑤ chạy BẢN CHÉP, tuần tự, mã thoát trực tiếp ─────────────────────────────
t0=$(date +%s)
: > "$OUT/cong-sach-ket.txt"
FAILED=0
while IFS= read -r b; do
  [ -n "$b" ] || continue
  dong=$(chay_mot "$b")
  echo "$dong" >> "$OUT/cong-sach-ket.txt"
  ma=${dong%% *}
  if [ "$ma" -ne 0 ]; then
    FAILED=$((FAILED + 1))
    echo "❌ $dong"
    tail -25 "$OUT/cong-sach-log.tmp"
  fi
done < "$OUT/cong-sach-dachep.txt"
DA_CHAY=$(grep -c . "$OUT/cong-sach-ket.txt")
echo "SUITE_WALL=$(($(date +%s) - t0))s · DA_CHAY=$DA_CHAY/$MAU_SO"

# ── ⑥ doctest ────────────────────────────────────────────────────────────────
t0=$(date +%s)
cargo test --offline --doc
DOC_EXIT=$?
echo "DOC_EXIT=$DOC_EXIT  # $(($(date +%s) - t0))s"

echo "FAILED=$FAILED · CHEP_HONG=$CHEP_HONG · MAU_SO=$MAU_SO"
if [ "$FAILED" -eq 0 ] && [ "$FMT_EXIT" -eq 0 ] && [ "$DOC_EXIT" -eq 0 ] && [ "$CHEP_HONG" -eq 0 ] && [ "$DA_CHAY" -eq "$MAU_SO" ]; then
  GATE_EXIT=0
else
  GATE_EXIT=1
fi
echo "GATE_EXIT=$GATE_EXIT"
echo "GATE_DONE=$(moc)"
exit "$GATE_EXIT"
