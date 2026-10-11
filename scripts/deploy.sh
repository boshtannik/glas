#!/bin/sh
# Деплой бинаря glas на телефон Sailfish OS.
#
# Переменные окружения (или дефолты):
#   SFOS_HOST      — хост телефона (по умолчанию 172.28.172.1)
#   SFOS_USER      — пользователь (defaultuser)
#   SFOS_PASS      — пароль SSH
#
# Использование:
#   scripts/deploy.sh                           # бинарь debug aarch64
#   scripts/deploy.sh -r                        # бинарь release aarch64
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
        *) echo "неизвестный флаг: $1"; exit 1 ;;
    esac
    shift
done

PKG_DIR="$(dirname "$0")/../glas"
cd "$PKG_DIR"

cargo build $RELEASE --target aarch64-unknown-linux-gnu

BIN="target/aarch64-unknown-linux-gnu/${RELEASE:-debug}/harbour-glas"
if [ ! -f "$BIN" ]; then
    echo "бинарь не найден: $BIN"; exit 1
fi

echo "==> копируем $BIN на $USER@$HOST"
if [ -n "$PASS" ]; then
    sshpass -p "$PASS" scp $SSH_OPTS "$BIN" "$USER@$HOST:~"
else
    scp $SSH_OPTS "$BIN" "$USER@$HOST:~"
fi

echo
echo "OK: бинарь доставлен в ~/$USER (harbour-glas)"
