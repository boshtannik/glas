#!/bin/sh
# Build harbour-glas (RPM) via mb2 in the sailo-rs Docker image (Rust + SFOS SDK).
#
# Usage:
#   sg docker -c "./mbuild.sh"            # debug — fast, no optimizations
#   sg docker -c "./mbuild.sh --release"  # release — optimized
#
# The cargo-target cache lives on the host (CACHE) and survives container runs:
# rebuilds touch only the changed crates.
set -e

PROJ="$(cd "$(dirname "$0")" && pwd)"
OUT="$HOME/.cache/glas-sailfish/out"
CACHE="$HOME/.cache/glas-sailfish/cache"
IMAGE=registry.gitlab.com/whisperfish/sailo-rs/rust-aarch64-5.1.0.11:sfos-5-1

MODE=debug
for arg in "$@"; do
    case "$arg" in
        --release) MODE=release ;;
        -h|--help)
            echo "Build harbour-glas (RPM)."
            echo "  ./mbuild.sh            — debug (default, fast)."
            echo "  ./mbuild.sh --release  — fully optimized."
            exit 0 ;;
        *) echo "unknown flag: $arg (only --release is allowed)"; exit 1 ;;
    esac
done

mkdir -p "$OUT"
mkdir -p "$CACHE/cargo"
chmod 777 "$CACHE" "$CACHE/cargo"
chmod 777 "$OUT"
rm -f "$OUT"/*.rpm

docker run --rm \
  -e MBUILD_MODE="$MODE" \
  -e CARGO_NET_RETRY=10 \
  -e CARGO_HTTP_MULTIPLEXING=false \
  -e CARGO_HTTP_TIMEOUT=120 \
  -e CARGO_HTTP_LOW_SPEED_LIMIT=1 \
  -v "$PROJ:/src:ro" \
  -v "$OUT:/out" \
  -v "$CACHE:/home/mersdk/cargo-cache" \
  "$IMAGE" bash -lc '
  set -e
  echo "MBUILD_MODE=$MBUILD_MODE"
  rm -rf ~/glas-build
  mkdir -p ~/glas-build
  cd /src
  tar -cf - \
    --exclude=./.git \
    --exclude=./vault \
    --exclude=./glas/target \
    --exclude=./third_party \
    --exclude=./sfos-sysroot \
    . | tar -xf - -C ~/glas-build
  cd ~/glas-build
  # whisper.cpp (STT) — in $HOME/cargo-cache (a persistent volume).
  if [ ! -d "$HOME/cargo-cache/whisper.cpp" ]; then
    curl -fsSL -o /tmp/whisper.cpp.tar.gz \
      "https://github.com/ggml-org/whisper.cpp/archive/refs/tags/v1.5.4.tar.gz"
    tar -xzf /tmp/whisper.cpp.tar.gz -C "$HOME/cargo-cache"
    mv "$HOME/cargo-cache"/whisper.cpp-1.5.4 "$HOME/cargo-cache/whisper.cpp"
  fi
  mkdir -p third_party
  ln -sfn "$HOME/cargo-cache/whisper.cpp" third_party/whisper.cpp
  # whisper-cli patch: adds --audio-ctx (speeds up recognition).
  if ! grep -qF -- "--audio-ctx" third_party/whisper.cpp/examples/main/main.cpp; then
    patch -p1 -d third_party/whisper.cpp < /src/packaging/whisper-cli-audio-ctx.patch
    echo "whisper-cli: --audio-ctx patch applied"
  else
    echo "whisper-cli: --audio-ctx patch already applied"
  fi
  ls -d third_party/whisper.cpp
  # Piper (TTS): a ready-made static aarch64 build.
  if [ ! -e "$HOME/cargo-cache/piper-aarch64/piper/piper" ]; then
    curl -fsSL -o /tmp/piper-aarch64.tar.gz \
      "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_aarch64.tar.gz"
    rm -rf "$HOME/cargo-cache/piper-aarch64"
    mkdir -p "$HOME/cargo-cache/piper-aarch64"
    tar -xzf /tmp/piper-aarch64.tar.gz -C "$HOME/cargo-cache/piper-aarch64"
  fi
  find "$HOME/cargo-cache/piper-aarch64" -maxdepth 2 -name piper -type f
  ls -l "$HOME/cargo-cache/piper-aarch64/piper"
  mb2 -n -t SailfishOS-5.1.0.11-aarch64 --no-snapshot=force build
  echo "=== RPM ($MBUILD_MODE) ==="
  ls -l RPMS/*.rpm
  cp -v RPMS/*.rpm /out/
'

cp -v "$OUT"/*.rpm "$PROJ"
