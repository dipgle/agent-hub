#!/usr/bin/env bash
# Create the self-signed code-signing identity that sign.sh uses.
#
# 🔴 Moved to the huba root 2026-08-16 together with `install_update.sh` — see the
# header there for why the old folder name had to go.
#
# RUN THIS ONCE, EVER. The certificate IS the identity macOS remembers: every
# TCC grant given to hubad (Full Disk Access, Automation) is pinned to
#     identifier "com.dipgle.hubd" and certificate root = H"<this cert>"
# Make a second certificate and every one of those grants points at a program
# that no longer exists — hubad goes back to hanging at boot with no log. That is
# why this script refuses to overwrite an existing key.
#
# The key never leaves this machine and is not in git. Back up
#   ~/Library/Application Support/hub/signing/hub-codesign.p12
# if you care about surviving a disk wipe without re-granting Full Disk Access.
set -euo pipefail

STORE="$HOME/Library/Application Support/hub/signing"
CERT_CN="Hub Local Signing"
P12_PASS="hublocal"   # not a secret: it protects a local, self-signed signing key

die() { echo "make-signing-cert.sh: $*" >&2; exit 1; }

mkdir -p "$STORE"; chmod 700 "$STORE"

if [[ -f "$STORE/hub-codesign.p12" ]]; then
  die "$STORE/hub-codesign.p12 already exists — that is the identity in use.
  To re-import it into a fresh keychain, just run sign.sh.
  To deliberately start over, move the old files aside first and be ready to
  re-add hubad to Full Disk Access afterwards."
fi

openssl req -x509 -newkey rsa:2048 \
  -keyout "$STORE/hub-codesign-priv" -out "$STORE/hub-codesign.crt" \
  -days 3650 -nodes \
  -subj "/CN=$CERT_CN/O=huba/C=VN" \
  -addext "basicConstraints=critical,CA:false" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning"
chmod 600 "$STORE/hub-codesign-priv"

openssl pkcs12 -export \
  -inkey "$STORE/hub-codesign-priv" -in "$STORE/hub-codesign.crt" \
  -name "$CERT_CN" -out "$STORE/hub-codesign.p12" -passout "pass:$P12_PASS"
chmod 600 "$STORE/hub-codesign.p12"

security import "$STORE/hub-codesign.p12" \
  -k "$HOME/Library/Keychains/login.keychain-db" -P "$P12_PASS" \
  -T /usr/bin/codesign -A

# The cert is NOT added to the trust store on purpose: codesign signs happily
# with an untrusted identity (it only reports CSSMERR_TP_NOT_TRUSTED when asked
# to *validate a trust chain*), TCC matches on the requirement, and trusting a
# root needs an admin password for no gain.
security find-identity -p codesigning | grep "$CERT_CN" \
  || die "imported, but the identity is not visible to codesign"

echo "make-signing-cert.sh: identity ready — now run sign.sh"
