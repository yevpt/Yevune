#!/usr/bin/env bash
# 提交身份硬校验（与工具/AI 无关，git 层强制）。
# 禁止用 git -c user.name/email、--author、GIT_AUTHOR_* 覆盖成非本仓库身份。
set -euo pipefail

EXPECTED_NAME="yevpt"
EXPECTED_EMAIL="vpt940417@gmail.com"

author_ident="$(git var GIT_AUTHOR_IDENT)"
committer_ident="$(git var GIT_COMMITTER_IDENT)"

author_name="${author_ident%% <*}"
author_email="$(printf '%s\n' "$author_ident" | sed -n 's/.*<\([^>]*\)>.*/\1/p')"
committer_name="${committer_ident%% <*}"
committer_email="$(printf '%s\n' "$committer_ident" | sed -n 's/.*<\([^>]*\)>.*/\1/p')"

errors=()

if [[ "$author_name" != "$EXPECTED_NAME" || "$author_email" != "$EXPECTED_EMAIL" ]]; then
  errors+=("Author 必须是 ${EXPECTED_NAME} <${EXPECTED_EMAIL}>，当前为 ${author_name} <${author_email}>")
fi

if [[ "$committer_name" != "$EXPECTED_NAME" || "$committer_email" != "$EXPECTED_EMAIL" ]]; then
  errors+=("Committer 必须是 ${EXPECTED_NAME} <${EXPECTED_EMAIL}>，当前为 ${committer_name} <${committer_email}>")
fi

if ((${#errors[@]})); then
  echo "" >&2
  echo "✗ Commit 身份不符合规范：" >&2
  echo "" >&2
  for e in "${errors[@]}"; do
    echo "  - $e" >&2
  done
  echo "" >&2
  echo "  禁止：git -c user.name=... / -c user.email=... / --author=... / GIT_AUTHOR_* 覆盖身份" >&2
  echo "  请使用全局或本仓库已配置的 user.name / user.email 直接 git commit" >&2
  echo "  规范详见 .agents/skills/git-commit/SKILL.md" >&2
  echo "" >&2
  exit 1
fi

exit 0
