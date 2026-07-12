#!/usr/bin/env bash
# 验证受版本控制的项目配置、源码和文档不再引用旧品牌标识。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PATTERN='MusicApp|music-server|music_core|music-server|MUSIC__|MUSIC_APP_SECRET|Packages/CoreFFI|Sources/MusicApp|Tests/MusicAppTests'

cd "$ROOT"

if matches="$(git ls-files -z -- \
  AGENTS.md CLAUDE.md README.md openapi.yaml docs server contract core clients \
  Dockerfile docker-compose.yml .env.example deploy scripts \
  ':(exclude)scripts/tests/test-yevune-brand.sh' \
  | xargs -0 rg -n -e "$PATTERN")"; then
  echo '发现旧 Yevune 品牌标识：' >&2
  printf '%s\n' "$matches" >&2
  exit 1
fi
