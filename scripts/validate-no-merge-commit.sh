#!/usr/bin/env bash
# 线性历史硬校验：拒绝创建 merge commit。
set -euo pipefail

git_cmd=(git)
if [[ "${1:-}" == "-C" ]]; then
  if [[ -z "${2:-}" ]]; then
    echo "[linear-history] -C 缺少仓库路径" >&2
    exit 1
  fi
  git_cmd=(git -C "$2")
  shift 2
fi

if ((${#})); then
  echo "[linear-history] 不支持的参数：$*" >&2
  exit 1
fi

if "${git_cmd[@]}" rev-parse -q --verify MERGE_HEAD >/dev/null 2>&1; then
  echo "" >&2
  echo "✗ 禁止创建 merge commit：仓库历史必须保持线性。" >&2
  echo "" >&2
  echo "  请中止本次合并并改用 rebase + fast-forward：" >&2
  echo "  git merge --abort" >&2
  echo "  git switch <feature-branch>" >&2
  echo "  git rebase <target-branch>" >&2
  echo "  git switch <target-branch>" >&2
  echo "  git merge --ff-only <feature-branch>" >&2
  echo "" >&2
  echo "  规范详见 .agents/skills/git-commit/SKILL.md" >&2
  echo "" >&2
  exit 1
fi
