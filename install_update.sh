#!/usr/bin/env bash
# Build hubad, then INSTALL a signed copy at a stable path for launchd to run.
#
#   ~/Library/Application Support/hub/bin/hubd    <- what launchd runs
#   rust/target/release/hubad                      <- what cargo builds
#
# 🔴 RENAMED 2026-08-16, and the reason is not cosmetic. Hà: *"xóa deploy đi sửa
# thành /huba/install_update.sh"*. The workspace denies every Bash command that
# NAMES a file containing the word "deploy" — `git mv`, `ls`, even a read-only
# `grep` — so the old path `deploy/install.sh` made this script unrunnable and
# unmaintainable from a session, while the thing it does (copy a binary to a
# path under $HOME and kickstart a launchd job) touches no server at all. The
# guard was firing on a NAME, not on a risk. Keep this file free of that word.
#
# WHY THERE ARE TWO BINARIES (measured 2026-08-10, the hard way). Signing
# target/release/hubad in place works for about a minute: the very next
# `cargo test --release` or `cargo clippy --all-targets` RELINKS the binary and
# stamps its own ad-hoc signature over the certificate one. Nothing complains —
# the daemon keeps running, tests stay green — and the loss only surfaces at the
# next reboot, when launchd starts a program macOS no longer recognises. It was
# caught here only because hubad prints `hubd_signature` at boot and it said
# `adhoc` twenty minutes after being signed `cert`.
#
# A separate installed copy makes that impossible by construction: cargo may
# relink target/ all day, the installed program is untouched until someone runs
# this script on purpose.
#
# Usage:
#   install_update.sh            build, install, sign, restart the launchd job
#   install_update.sh --no-build use the release binary already built
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$HERE/rust/target/release/hubad"
DEST_DIR="$HOME/Library/Application Support/hub/bin"
# 🔴 TÊN TỆP ĐÍCH VẪN LÀ `hubd`, cố ý — Hà chốt 2026-08-20 khi đổi tên hub→huba:
# "giữ định danh hệ thống". `ProgramArguments` của plist trỏ đúng đường này, và
# quyền Trợ năng ghim theo `identifier "com.dipgle.hubd"` + chứng chỉ. Đổi tên
# tệp ⟹ phải sửa plist ⟹ bootout/bootstrap, và một lượt cấp quyền lại bằng tay.
# Bản cargo build tên `hubad`; chỗ này chép nó sang cái tên hệ thống đang giữ.
DEST="$DEST_DIR/hubd"
LABEL="com.dipgle.hubd"

die() { echo "install_update.sh: $*" >&2; exit 1; }

if [[ "${1:-}" != "--no-build" ]]; then
  ( cd "$HERE/rust" && cargo build --release --offline )
fi
[[ -f "$SRC" ]] || die "no binary at $SRC"

mkdir -p "$DEST_DIR"

# Copy to a temp name and move into place: a daemon that is mid-restart must
# never see a half-written file, and macOS refuses to overwrite a running image.
tmp="$DEST.new"
cp "$SRC" "$tmp"
chmod 755 "$tmp"
"$HERE/sign.sh" "$tmp"
mv -f "$tmp" "$DEST"
# The health panel reads this file's mtime to answer "is the daemon running
# today's code?", by comparing it against the newest .rs in rust/src. Nothing
# else records the install time, so the move above IS the record.

# Signing survives the move (the signature lives inside the file), but say so
# out loud rather than assume it — this is the one fact the whole script exists
# to deliver.
dr="$(codesign -d -r- "$DEST" 2>&1 | grep '^designated' || true)"
case "$dr" in
  *"certificate root"*) : ;;
  *) die "installed copy is not certificate-signed: $dr" ;;
esac

echo "install_update.sh: $DEST"
echo "  $dr"

# Which database the config NAMES. Read it before the restart so a failure
# below can say what was expected, not just what happened.
CFG="${HUB_CONFIG:-$HERE/huba.config.json}"
want_db=""
if [[ -f "$CFG" ]]; then
  want_db="$(python3 - "$CFG" "$HERE" <<'PY' 2>/dev/null || true
import json, os, sys
cfg, here = sys.argv[1], sys.argv[2]
db = json.load(open(cfg)).get("db", "")
print(os.path.realpath(os.path.join(here, db)) if db else "")
PY
)"
fi

if launchctl print "gui/$(id -u)/$LABEL" >/dev/null 2>&1; then
  launchctl kickstart -k "gui/$(id -u)/$LABEL"
  echo "install_update.sh: restarted $LABEL"

  # 🔴 Đã chứng minh đúng MÃ (chữ ký), giờ phải chứng minh đúng TRẠNG THÁI —
  # hai câu hỏi khác nhau, và cái thứ hai từng trượt suốt một đêm. Ngày
  # 2026-08-20, `data/hub.sqlite` bị `mv` sang tên mới trong lúc daemon đang
  # giữ nó: handle đã mở bám theo inode nên vẫn ghi đúng tệp cũ, còn mỗi lần
  # mở-mới lại theo đường dẫn cũ và SQLite DỰNG một DB rỗng ở đó. Hai tệp cùng
  # có mtime của phút này, cùng trông như đang sống; chỉ một tệp có 12 380 dòng
  # `runs`. Con trỏ phiên ghi một bên, đọc một bên ⟹ chữ gõ đi vào phiên khác.
  # Không có phép đo nào bắt được, vì không có bước nào ĐI HỎI tiến trình.
  if [[ -n "$want_db" ]]; then
    lock="$HERE/data/hubd.lock"
    pid=""
    for _ in $(seq 1 30); do
      sleep 1
      [[ -f "$lock" ]] || continue
      pid="$(tr -d '[:space:]' < "$lock")"
      [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null && break
      pid=""
    done
    if [[ -z "$pid" ]]; then
      die "restarted, but no live pid in $lock after 30s — check ~/Library/Logs/hubd.err"
    fi
    # `lsof` in trọn đường dẫn ĐÃ GIẢI của mọi tệp tiến trình đang mở, nên nó
    # trả lời được cả ca đổi tên dưới chân: hỏi inode, không hỏi cái tên.
    if lsof -p "$pid" 2>/dev/null | grep -qF "$want_db"; then
      echo "install_update.sh: pid $pid đang mở $want_db"
    else
      other="$(lsof -p "$pid" 2>/dev/null | grep -oE '[^ ]*\.sqlite' | sort -u | tr '\n' ' ')"
      die "pid $pid KHÔNG mở DB mà cấu hình gọi tên.
  cấu hình: $want_db
  đang mở : ${other:-<không có .sqlite nào>}
  Trạng thái tách đôi — đừng dùng cho tới khi gộp xong."
    fi

    # `lsof` mới trả lời được nửa câu hỏi: nó chỉ thấy handle GIỮ LÂU. Cú tách
    # đôi 08-20 sống ở nửa còn lại — mở theo đường dẫn, ghi, đóng. Nhìn lúc nào
    # cũng không ai cầm tệp lạc, thế mà nó lớn suốt đêm. Thứ bắt được kiểu ghi
    # đó là DẤU VẾT trên đĩa, nên hỏi thêm: còn .sqlite nào vừa bị ghi không?
    # Cảnh báo chứ không chặn — bản cài đã đúng, chỗ này là dọn nốt phần thừa.
    #
    # So sánh bằng đường ĐÃ GIẢI cả hai phía. Chạy script qua symlink `hub/`
    # thì `find` in đường logic còn `want_db` là đường thật; so thẳng chuỗi là
    # chính DB đúng bị gọi là tệp lạc — một phép đo kêu oan dạy người ta đọc
    # lướt qua nó, tệ hơn là không có.
    rows=""
    while IFS= read -r f; do
      [[ -n "$f" ]] || continue
      [[ "$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$f")" == "$want_db" ]] && continue
      rows+="    $f  (runs=$(sqlite3 "$f" "SELECT count(*) FROM runs;" 2>/dev/null || echo '?'))"$'\n'
    done < <(find "$HERE/data" -maxdepth 1 -name '*.sqlite' -mmin -5 2>/dev/null)

    if [[ -n "$rows" ]]; then
      {
        echo "install_update.sh: ⚠ CÒN DB KHÁC VỪA BỊ GHI trong 5 phút qua —"
        printf '%s' "$rows"
        echo "    cấu hình gọi tên: $want_db  (runs=$(sqlite3 "$want_db" "SELECT count(*) FROM runs;" 2>/dev/null || echo '?'))"
        echo "    Đối chiếu rồi bỏ bản thừa; hai DB cùng sống thì con trỏ phiên"
        echo "    ghi một bên đọc một bên, và chữ gõ đi vào phiên khác."
      } >&2
    fi
  fi
else
  echo "install_update.sh: $LABEL is not loaded — bootstrap it with"
  echo "  launchctl bootstrap gui/\$(id -u) ~/Library/LaunchAgents/$LABEL.plist"
fi
