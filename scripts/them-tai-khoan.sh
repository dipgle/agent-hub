#!/usr/bin/env bash
# Thêm một tài khoản Claude cho huba — trọn gói, chạy một lệnh.
#
#   bash scripts/them-tai-khoan.sh acc4
#
# Vì sao phải là script chứ không phải mấy dòng gõ tay: ba bước dưới đây có THỨ
# TỰ, và cả ba lỗi khi làm sai thứ tự đều IM LẶNG.
#
# ① `projects` phải là LIÊN KẾT MỀM về `~/.claude/projects` TRƯỚC lượt chạy đầu
#    tiên. huba đọc nhật ký phiên từ MỘT gốc duy nhất (`claude_transcript_root`
#    rỗng = `~/.claude`, xem `config.rs`), nên nếu CLI tự tạo một thư mục
#    `projects` THẬT cho tài khoản mới thì mọi phiên của nó vô hình với huba:
#    `/sessions` không liệt kê, `/ask` không hỏi được, bàn giao không đọc được
#    nhật ký. Không có dòng lỗi nào — chỉ là một tài khoản "không có phiên nào".
#    acc2 và acc3 đều đang là liên kết mềm (đo 2026-09-12).
#
# ② ĐĂNG NHẬP XONG RỒI MỚI KHAI VÀO `huba.config.json`. Đây là cái bẫy đắt nhất:
#    một tài khoản có trong config mà chưa đăng nhập thì `quota::read` không
#    thấy `.claude.json` ⟹ hạng `Rank::Unknown` — mà `Unknown` đứng **TRƯỚC**
#    `Full` trong thứ tự chọn (`quota.rs`: *"một ẩn số vẫn hơn một cánh cửa đã
#    đóng"*). Nên đúng lúc mọi tài khoản khác kịch trần — tức đúng lúc người ta
#    cần chuyển nhất — `watch::suggest_account` sẽ chọn nó, mở một cửa sổ, và
#    cửa sổ ấy chết ngay ở màn đăng nhập. Đúng hình dạng con bug acc1 ngày
#    02/09 đã phải dựng cả một cuốn sổ tài khoản chết để chữa.
#
# ③ KHÔNG đặt `launch` (kiểu `claude4`) trừ khi alias ấy có thật trong `~/.zshrc`.
#    Bỏ trống thì `sessions::account_launch` tự dựng
#    `CLAUDE_CONFIG_DIR=<dir> claude` — chạy được ở mọi shell, không phụ thuộc
#    một tệp ngoài tầm của repo này. Alias chỉ để CHỦ MÁY gõ cho ngắn.
#
# Không cần khởi động lại daemon: `hubad` tự nạp lại khi `huba.config.json` đổi
# mtime (`rust/src/bin/hubad.rs`, khối `config_reloaded`).
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CFG="$HERE/huba.config.json"

TEN="${1:-}"
if [[ -z "$TEN" ]]; then
  echo "dùng: bash scripts/them-tai-khoan.sh <tên>   (ví dụ: acc4)" >&2
  exit 2
fi
# Tên đi thẳng vào đường dẫn và vào dòng lệnh mở cửa sổ ⟹ chỉ nhận một từ sạch.
if [[ ! "$TEN" =~ ^[A-Za-z0-9_-]+$ ]]; then
  echo "tên chỉ được gồm chữ, số, '-' và '_' — '$TEN' không hợp lệ" >&2
  exit 2
fi

DIR="$HOME/.claude-$TEN"
GOC_PROJECTS="$HOME/.claude/projects"

echo "── ① thư mục cấu hình của $TEN ──"
mkdir -p "$DIR" || exit 1

# Liên kết `projects`. Ba trạng thái, và trạng thái giữa KHÔNG được tự sửa:
#   · chưa có         ⟹ tạo
#   · đã là liên kết  ⟹ trỏ lại cho chắc (idempotent)
#   · là thư mục THẬT ⟹ DỪNG. Trong đó có thể đã có nhật ký phiên thật; gộp hai
#     kho nhật ký là việc phải nhìn tận mắt, không phải việc của một script.
if [[ -L "$DIR/projects" ]]; then
  ln -sfn "$GOC_PROJECTS" "$DIR/projects"
  echo "   projects → $(readlink "$DIR/projects")  (đã trỏ lại)"
elif [[ -e "$DIR/projects" ]]; then
  echo "   ⛔ $DIR/projects đang là THƯ MỤC THẬT, không phải liên kết." >&2
  echo "      Trong đó có $(ls -1 "$DIR/projects" 2>/dev/null | wc -l | tr -d ' ') mục." >&2
  echo "      Gộp tay rồi chạy lại — script không tự xoá nhật ký của ai." >&2
  exit 1
else
  ln -s "$GOC_PROJECTS" "$DIR/projects"
  echo "   projects → $GOC_PROJECTS  (vừa tạo)"
fi

echo "── ② đăng nhập ──"

# 🔴 `claude auth status` THOÁT 0 Ở CẢ HAI CHIỀU — đo 2026-09-12:
#     acc2 (đã đăng nhập)  → {"loggedIn": true,  "email": …, "subscriptionType": "max"}  exit 0
#     thư mục trắng        → {"loggedIn": false, "authMethod": "none"}                   exit 0
# Nên mã thoát ở đây KHÔNG phải phép đo, và bắt chữ trong văn xuôi ("not logged
# in"…) cũng không: câu trả lời là JSON, phán quyết nằm trong THÂN. Bản đầu của
# script này chấm bằng `&& ! grep -qiE "not logged"` — với thân JSON thì cả hai
# vế đều đúng ⟹ một tài khoản CHƯA đăng nhập đọc ra là "đã đăng nhập sẵn" ⟹ đi
# thẳng xuống bước ③ và gài đúng cái bẫy mà cả tệp này viết ra để tránh.
da_dang_nhap() {
  CLAUDE_CONFIG_DIR="$DIR" claude auth status 2>/dev/null | python3 -c '
import json,sys
try:
    d = json.load(sys.stdin)
except Exception:
    sys.exit(2)          # không đọc ra JSON là TRẠNG THÁI RIÊNG, không phải "chưa đăng nhập"
sys.exit(0 if d.get("loggedIn") is True else 1)
'
}

khai_ra() {
  CLAUDE_CONFIG_DIR="$DIR" claude auth status 2>/dev/null \
    | python3 -c '
import json,sys
try:
    d = json.load(sys.stdin)
except Exception:
    print("   | (không đọc ra JSON)"); raise SystemExit
for k in ("loggedIn","email","orgName","subscriptionType"):
    if k in d:
        print(f"   | {k}: {d[k]}")
'
}

da_dang_nhap
TRANG_THAI=$?
case $TRANG_THAI in
  0) echo "   đã đăng nhập sẵn:"; khai_ra ;;
  2) echo '   ⛔ "claude auth status" không trả ra JSON — chưa đo được, DỪNG.' >&2
     echo "      (khác hẳn 'chưa đăng nhập': ở đây hỏng cái THƯỚC, không phải cái được đo)" >&2
     exit 1 ;;
  *)
     echo "   chưa đăng nhập — mở trình duyệt ngay bây giờ."
     echo "   (đây là bước DUY NHẤT máy không làm thay được: OAuth của tài khoản Hà)"
     CLAUDE_CONFIG_DIR="$DIR" claude auth login
     echo "   ─ kiểm lại bằng chính phép đo trên ─"
     da_dang_nhap
     LAI=$?
     khai_ra
     if [[ $LAI -ne 0 ]]; then
       echo "   ⛔ vẫn chưa đăng nhập (loggedIn ≠ true) — DỪNG, chưa khai vào config." >&2
       echo "      Khai bây giờ là gài một cái bẫy: hạng Unknown đứng TRƯỚC Full nên huba" >&2
       echo "      sẽ chọn đúng nó lúc mọi tài khoản khác kịch trần. Xem đầu tệp này." >&2
       exit 1
     fi
     ;;
esac

echo "── ③ khai vào huba.config.json ──"
python3 - "$CFG" "$TEN" "$DIR" <<'PY'
import json, os, shutil, sys

cfg_path, ten, dir_that = sys.argv[1], sys.argv[2], sys.argv[3]
cfg = json.load(open(cfg_path, encoding="utf-8"))
accs = cfg.setdefault("claude_accounts", [])

if any(a.get("name") == ten for a in accs):
    print(f"   '{ten}' đã có trong config — không đụng.")
    raise SystemExit(0)

# Ghi `~/...` chứ không ghi đường tuyệt đối: `config::expand_home` hiểu nó, và
# cấu hình thì không nên gõ cứng tên người dùng.
moi = {"name": ten, "config_dir": dir_that.replace(os.path.expanduser("~"), "~", 1)}
accs.append(moi)

shutil.copy(cfg_path, cfg_path + ".bak-them-" + ten)
tmp = cfg_path + ".tmp"
with open(tmp, "w", encoding="utf-8") as f:
    json.dump(cfg, f, ensure_ascii=False, indent=2)
    f.write("\n")
os.replace(tmp, cfg_path)   # đổi tên là thao tác nguyên tử — daemon không bao giờ đọc phải tệp ghi dở
print(f"   đã thêm: {json.dumps(moi, ensure_ascii=False)}")
print(f"   bản lưu: {os.path.basename(cfg_path)}.bak-them-{ten}")
PY
CFG_EXIT=$?
[[ $CFG_EXIT -ne 0 ]] && { echo "   ⛔ ghi config hỏng (mã $CFG_EXIT)" >&2; exit 1; }

echo "── ④ daemon nhận chưa ──"
echo "   (hubad tự nạp lại khi config đổi mtime — không cần khởi động lại)"
LOG="$HERE/logs/huba.log"
# Chờ NGẮN và có trần. Bản đầu chờ 40×5s = 200 giây, và một bộ thử chạy 4 ca đã
# treo đúng vì nó — một vòng chờ dài trong bước "báo cáo" thì cái giá nằm ở mọi
# lượt chạy, kể cả lượt không có gì để chờ.
for _ in 1 2 3 4 5 6; do
  if tail -c 200000 "$LOG" 2>/dev/null | grep -q "\"account\":\"$TEN\""; then
    tail -c 200000 "$LOG" | grep "\"account\":\"$TEN\"" | tail -1 | cut -c1-220
    break
  fi
  sleep 5
done

echo
echo "Xong phần máy làm được. Kiểm bằng mắt:  huba doctor   ·   /accounts trên Telegram"
echo "Muốn gõ tắt ở terminal thì thêm dòng này vào ~/.zshrc (huba KHÔNG cần nó):"
echo "  alias claude${TEN#acc}='CLAUDE_CONFIG_DIR=\$HOME/.claude-$TEN claude'"
