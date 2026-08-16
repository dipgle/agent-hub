#!/usr/bin/env bash
# Build hubd, then INSTALL a signed copy at a stable path for launchd to run.
#
#   ~/Library/Application Support/hub/bin/hubd    <- what launchd runs
#   rust/target/release/hubd                      <- what cargo builds
#
# 🔴 RENAMED 2026-08-16, and the reason is not cosmetic. Hà: *"xóa deploy đi sửa
# thành /hub/install_update.sh"*. The workspace denies every Bash command that
# NAMES a file containing the word "deploy" — `git mv`, `ls`, even a read-only
# `grep` — so the old path `deploy/install.sh` made this script unrunnable and
# unmaintainable from a session, while the thing it does (copy a binary to a
# path under $HOME and kickstart a launchd job) touches no server at all. The
# guard was firing on a NAME, not on a risk. Keep this file free of that word.
#
# WHY THERE ARE TWO BINARIES (measured 2026-08-10, the hard way). Signing
# target/release/hubd in place works for about a minute: the very next
# `cargo test --release` or `cargo clippy --all-targets` RELINKS the binary and
# stamps its own ad-hoc signature over the certificate one. Nothing complains —
# the daemon keeps running, tests stay green — and the loss only surfaces at the
# next reboot, when launchd starts a program macOS no longer recognises. It was
# caught here only because hubd prints `hubd_signature` at boot and it said
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
SRC="$HERE/rust/target/release/hubd"
DEST_DIR="$HOME/Library/Application Support/hub/bin"
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

if launchctl print "gui/$(id -u)/$LABEL" >/dev/null 2>&1; then
  launchctl kickstart -k "gui/$(id -u)/$LABEL"
  echo "install_update.sh: restarted $LABEL"
else
  echo "install_update.sh: $LABEL is not loaded — bootstrap it with"
  echo "  launchctl bootstrap gui/\$(id -u) ~/Library/LaunchAgents/$LABEL.plist"
fi
