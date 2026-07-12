#!/usr/bin/env bash
# 启用本仓库版本化 git hooks（克隆后执行一次即可）。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

git config core.hooksPath .githooks
chmod +x .githooks/pre-commit .githooks/pre-merge-commit .githooks/commit-msg \
  scripts/validate-commit-author.sh scripts/validate-commit-msg.sh \
  scripts/validate-no-merge-commit.sh

echo "已设置 core.hooksPath=.githooks"
echo "Author/Committer 须为：yevpt <vpt940417@gmail.com>"
echo "分支整合须使用 rebase + fast-forward，禁止 merge commit"
echo "规范见 .agents/skills/git-commit/SKILL.md"
