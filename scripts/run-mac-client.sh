#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT"

usage() {
  cat <<'EOF'
用法：./scripts/run-mac-client.sh [--with-server]

  --with-server  先启动 Docker Compose 服务端
  --help         显示帮助
EOF
}

with_server=false
case "${1-}" in
  "") ;;
  --with-server) with_server=true ;;
  --help|-h) usage; exit 0 ;;
  *) echo "未知参数：$1" >&2; usage >&2; exit 2 ;;
esac
if [ "$#" -gt 1 ]; then
  echo "参数过多" >&2
  usage >&2
  exit 2
fi

if [ "$(uname -s)" != Darwin ]; then
  echo "Mac 客户端只能在 macOS 上启动" >&2
  exit 1
fi
for command in cargo swift; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "缺少命令：$command" >&2
    exit 1
  fi
done

if $with_server; then
  if ! command -v docker >/dev/null 2>&1; then
    echo "缺少命令：docker" >&2
    exit 1
  fi
  docker compose up -d
fi

framework=clients/apple/Packages/CoreFFI/MusicCoreFFI.xcframework/Info.plist
needs_build=false
if [ ! -f "$framework" ]; then
  needs_build=true
elif find core/src core/Cargo.toml core/build.rs core/uniffi.toml contract/src contract/Cargo.toml \
  -type f -newer "$framework" -print -quit 2>/dev/null | grep -q .; then
  needs_build=true
fi

if $needs_build; then
  echo "Rust core 或绑定已更新，正在生成 xcframework…"
  ./clients/apple/Packages/CoreFFI/scripts/build-core.sh
fi

exec swift run --package-path clients/apple MusicApp
