#!/usr/bin/env bash
# 验证线性历史校验器允许普通提交、拒绝正在进行的合并提交。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
VALIDATOR="$ROOT/scripts/validate-no-merge-commit.sh"
TMPDIR_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_ROOT"' EXIT

if [[ ! -x "$VALIDATOR" ]]; then
  echo "缺少线性历史校验器：$VALIDATOR" >&2
  exit 1
fi

repo="$TMPDIR_ROOT/repo"
git init -q "$repo"
git -C "$repo" config user.name tester
git -C "$repo" config user.email tester@example.com

printf 'base\n' >"$repo/base.txt"
git -C "$repo" add base.txt
git -C "$repo" commit -qm 'test: 建立校验基线'
initial_branch="$(git -C "$repo" branch --show-current)"

"$VALIDATOR" -C "$repo"

git -C "$repo" switch -q -c topic
printf 'topic\n' >"$repo/topic.txt"
git -C "$repo" add topic.txt
git -C "$repo" commit -qm 'test: 添加分支改动'

git -C "$repo" switch -q "$initial_branch"
printf 'main\n' >"$repo/main.txt"
git -C "$repo" add main.txt
git -C "$repo" commit -qm 'test: 添加主线改动'
git -C "$repo" merge --no-commit topic >/dev/null

if "$VALIDATOR" -C "$repo"; then
  echo '合并提交未被拒绝' >&2
  exit 1
fi
