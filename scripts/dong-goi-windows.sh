#!/bin/bash
# Đóng gói bản Windows: dựng .exe (x86_64-pc-windows-gnu, linker mingw của Homebrew) → chép →
# zip → in sha256. Chạy từ đâu cũng được; gói ra ở .tmp/huba-windows-<sha>.zip.
#
#   bash scripts/dong-goi-windows.sh
#
# Thiếu target/linker thì DỪNG và nói thiếu gì (mã 2), không dựng nửa chừng.
set -u
H="$(cd "$(dirname "$0")/.." && pwd)"
R="$H/rust"
TARGET=x86_64-pc-windows-gnu

rustup target list --installed 2>/dev/null | grep -qx "$TARGET" \
  || { echo "KHÔNG ĐO ĐƯỢC: thiếu target $TARGET (rustup target add $TARGET)"; exit 2; }
command -v x86_64-w64-mingw32-gcc >/dev/null \
  || { echo "KHÔNG ĐO ĐƯỢC: thiếu linker x86_64-w64-mingw32-gcc (brew install mingw-w64)"; exit 2; }

if [ -n "$(git -C "$H" status --porcelain -- rust install_update.ps1 scripts/do-windows.ps1 DOC-TRUOC-WINDOWS.txt)" ]; then
  echo "⚠ cây có thay đổi CHƯA commit trong phần đóng gói — tên gói mang sha HEAD nhưng nội dung là cây hiện tại"
fi
SHA=$(git -C "$H" rev-parse --short HEAD)

( cd "$R" && cargo build --offline --release --target "$TARGET" --bin huba --bin hubad )
BUILD_EXIT=$?
echo "BUILD_EXIT=$BUILD_EXIT"
[ $BUILD_EXIT -eq 0 ] || exit 1

D="$H/.tmp/huba-windows-$SHA"
rm -rf "$D" "$D.zip"
mkdir -p "$D/scripts"
cp "$R/target/$TARGET/release/huba.exe" "$R/target/$TARGET/release/hubad.exe" "$H/install_update.ps1" "$D/"
cp "$H/scripts/do-windows.ps1" "$D/scripts/"
cp "$H/DOC-TRUOC-WINDOWS.txt" "$D/DOC-TRUOC.txt"
( cd "$H/.tmp" && zip -qr "huba-windows-$SHA.zip" "huba-windows-$SHA" )
ZIP_EXIT=$?
echo "ZIP_EXIT=$ZIP_EXIT"
[ $ZIP_EXIT -eq 0 ] || exit 1
unzip -l "$D.zip" | tail -n +4 | sed '$d' | sed '$d'
shasum -a 256 "$D.zip"
