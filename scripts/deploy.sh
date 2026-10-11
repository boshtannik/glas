#!/bin/sh
# Deploy the glas binary to a Sailfish OS phone.
#
# Environment variables (or defaults):
#   SFOS_HOST      — phone host (default 172.28.172.1)
#   SFOS_USER      — user (defaultuser)
#   SFOS_PASS      — SSH password
#
# Usage:
#   scripts/deploy.sh                           # debug aarch64 binary
#   scripts/deploy.sh -r                        # release aarch64 binary
#   SFOS_PASS=<your-ssh-password> scripts/deploy.sh -r
set -e

HOST="${SFOS_HOST:-172.28.172.1}"
USER="${SFOS_USER:-defaultuser}"
PASS="${SFOS_PASS:-}"
SSH_OPTS="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"

RELEASE=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        -r|--release) RELEASE="--release" ;;
        *) echo "unknown flag: $1"; exit 1 ;;
    esac
    shift
done

PKG_DIR="$(dirname "$0")/../glas"
cd "$PKG_DIR"

cargo build $RELEASE --target aarch64-unknown-linux-gnu

BIN="target/aarch64-unknown-linux-gnu/${RELEASE:-debug}/harbour-glas"
if [ ! -f "$BIN" ]; then
    echo "binary not found: $BIN"; exit 1
fi

echo "==> copying $BIN to $USER@$HOST"
if [ -n "$PASS" ]; then
    sshpass -p "$PASS" scp $SSH_OPTS "$BIN" "$USER@$HOST:~"
else
    scp $SSH_OPTS "$BIN" "$USER@$HOST:~"
fi

echo
echo "OK: binary delivered to ~/$USER (harbour-glas)"
