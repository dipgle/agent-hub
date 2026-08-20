#!/usr/bin/env bash
# Re-sign hubad with a STABLE identity so macOS keeps its TCC grants across rebuilds.
#
# 🔴 Moved to the huba root 2026-08-16 together with `install_update.sh` — see the
# header there for why the old folder name had to go.
#
# WHY THIS EXISTS (measured 2026-08-10). `cargo` ad-hoc-signs every binary it
# links, and an ad-hoc signature's designated requirement is
#     designated => cdhash H"fea4ff94…"
# — a hash of the bytes. TCC pins a grant to that requirement, so EVERY rebuild
# makes hubad a different program to macOS and the launchd copy silently loses
# Full Disk Access and Automation. The failure mode has no log and no exit code:
# getcwd()/open() block forever on a dialog a background process cannot show.
#
# Signing with a certificate changes the requirement to
#     designated => identifier "com.dipgle.hubd" and certificate root = H"9de8ec03…"
# — identity, not bytes. Rebuild all you like; the grant survives. The cert is
# self-signed and deliberately NOT added to the trust store: codesign accepts an
# untrusted identity, and TCC matches on the requirement, so nothing here needs
# an admin password. Gatekeeper is not involved — this binary is never
# quarantined, it is built on the machine that runs it.
#
# Only hubad is signed. `huba` (the CLI) runs from a terminal and inherits that
# terminal's TCC responsibility; giving it its own identity would only add a
# second program for macOS to ask about.
#
# Usage: sign.sh [path-to-binary]   (default: rust/target/release/hubad)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="${1:-$HERE/rust/target/release/hubad}"
IDENTIFIER="com.dipgle.hubd"
CERT_CN="Huba Local Signing"
STORE="$HOME/Library/Application Support/hub/signing"
P12="$STORE/huba-codesign.p12"
P12_PASS="hublocal"   # not a secret: it protects a local, self-signed signing key
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

die() { echo "sign.sh: $*" >&2; exit 1; }

[[ -f "$BIN" ]] || die "no binary at $BIN — run cargo build --release first"

# 1. Make sure the signing identity is in the keychain. Never generate silently
#    a SECOND time: a new cert means a new requirement, which means every TCC
#    grant is lost again — the exact bug this script exists to kill.
# `find-identity` prints:  1) <40-hex> "Common Name" (CSSMERR_TP_NOT_TRUSTED)
# Match on the quoted name, not on a field index — the name has spaces in it,
# and awk $3 lands in the middle of it.
sha="$(security find-identity -p codesigning 2>/dev/null \
       | grep -F "\"$CERT_CN\"" | head -1 | awk '{ print $2 }')"

if [[ -z "$sha" ]]; then
  [[ -f "$P12" ]] || die "identity '$CERT_CN' is not in the keychain and $P12 is missing.
  Recreate both (this INVALIDATES existing TCC grants — you will have to re-add
  hubad to Full Disk Access):
    make-signing-cert.sh"
  echo "sign.sh: importing '$CERT_CN' from $P12" >&2
  security import "$P12" -k "$KEYCHAIN" -P "$P12_PASS" -T /usr/bin/codesign -A >&2
  sha="$(security find-identity -p codesigning 2>/dev/null \
         | awk -v cn="\"$CERT_CN\"" '$3 == cn { print $2; exit }')"
  [[ -n "$sha" ]] || die "imported $P12 but '$CERT_CN' still is not a code-signing identity"
fi

# 2. Sign. --force replaces cargo's ad-hoc signature. No hardened runtime: hubad
#    spawns osascript to talk to Terminal, and the runtime flag would demand an
#    apple-events entitlement to keep that working.
codesign --force --sign "$sha" --identifier "$IDENTIFIER" --timestamp=none "$BIN" 2>&1 | sed 's/^/sign.sh: /' >&2

# 3. Prove the requirement is the STABLE one. A signature that still says cdhash
#    means the grant will be lost on the next build — that is a failure, not a
#    warning, because nothing downstream would notice until hubad hangs at boot.
dr="$(codesign -d -r- "$BIN" 2>/dev/null | grep '^designated' || true)"
case "$dr" in
  *"certificate root"*) : ;;
  *) die "signed, but the designated requirement is not certificate-based:
  $dr" ;;
esac
codesign --verify "$BIN" || die "signature does not verify"

echo "sign.sh: $(basename "$BIN") <- $CERT_CN"
echo "  $dr"
