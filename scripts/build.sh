#!/bin/sh
# Build glas (cross-compiled for aarch64 / Sailfish OS).
#
# Usage:
#   scripts/build.sh          # debug, host (check)
#   scripts/build.sh -r       # release, host
#   scripts/build.sh -a       # debug, aarch64 (phone)
#   scripts/build.sh -ar      # release, aarch64
set -e

PKG_DIR="$(dirname "$0")/../glas"
cd "$PKG_DIR"

TARGET=""
RELEASE=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        -a|--aarch64) TARGET="--target aarch64-unknown-linux-gnu" ;;
        -r|--release) RELEASE="--release" ;;
        *) echo "unknown flag: $1"; exit 1 ;;
    esac
    shift
done

if [ -n "$TARGET" ]; then
    echo "==> cargo build $RELEASE $TARGET"
    cargo build $RELEASE $TARGET
    BIN="target/aarch64-unknown-linux-gnu/${RELEASE:-debug}/harbour-glas"
else
    echo "==> cargo build $RELEASE"
    cargo build $RELEASE
    BIN="target/${RELEASE:-debug}/harbour-glas"
fi

echo
echo "OK: $BIN"
ls -lh "$BIN"
