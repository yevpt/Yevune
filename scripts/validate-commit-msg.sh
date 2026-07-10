#!/usr/bin/env bash
# Commit message 格式校验（与工具/AI 无关，git 层强制）。
# 规范：<type>(<scope>): <中文主题>
# 由 .githooks/commit-msg 调用，参数为 commit message 文件路径。
set -euo pipefail

TYPES='feat|fix|refactor|test|chore|perf|docs|ci|style|build'
MAX_SUBJECT=50

msg_file="${1:-}"
if [[ -z "$msg_file" ]]; then
  echo "[commit-msg] 缺少 message 文件参数" >&2
  exit 1
fi

# 取首个非空、非注释行作为主题
subject_line="$(
  awk '
    /^[[:space:]]*#/ { next }
    /^[[:space:]]*$/ { next }
    { print; exit }
  ' "$msg_file"
)"

# 放行 merge / revert
if [[ -z "$subject_line" ]] || [[ "$subject_line" =~ ^(Merge|Revert)\b ]]; then
  exit 0
fi

errors=()

if [[ ! "$subject_line" =~ ^([a-z]+)(\(([a-z0-9-]+)\))?:\ (.+)$ ]]; then
  errors+=("格式必须为 \`<type>(<scope>): <中文主题>\`（scope 可选，冒号后留一个空格）")
else
  type="${BASH_REMATCH[1]}"
  scope="${BASH_REMATCH[3]}"
  subject="${BASH_REMATCH[4]}"

  if [[ ! "$type" =~ ^($TYPES)$ ]]; then
    errors+=("type 非法：「${type}」。仅允许 feat/fix/refactor/test/chore/perf/docs/ci/style/build")
  fi
  if [[ -n "$scope" && ! "$scope" =~ ^[a-z0-9-]+$ ]]; then
    errors+=("scope「${scope}」须为英文小写技术词（可含数字与连字符）")
  fi

  # 按字符计长度（兼容中文）
  subject_len="$(printf '%s' "$subject" | awk '{print length}')"
  if ((subject_len > MAX_SUBJECT)); then
    errors+=("主题超长：${subject_len} 字，需 ≤ ${MAX_SUBJECT} 字")
  fi
  if ! printf '%s' "$subject" | grep -q '[一-龥]'; then
    errors+=("主题需使用中文描述")
  fi
  # 用 glob 按「字符」判断结尾，避免 =~ 在非 UTF-8 locale 下把全角「。」
  # 按字节拆入字符类，从而误伤末字节相同的中文字（如「层」E5 B1 82）。
  if [[ "$subject" == *. || "$subject" == *"。" ]]; then
    errors+=("主题结尾不要加句号")
  fi
fi

raw="$(cat "$msg_file")"
if printf '%s\n' "$raw" | grep -qi 'breaking change' && ! printf '%s\n' "$raw" | grep -q 'BREAKING CHANGE:'; then
  errors+=("破坏性变更标记须为 \`BREAKING CHANGE: <描述>\`（全大写 + 冒号）")
fi
if printf '%s\n' "$raw" | grep -qiE '^Co-authored-by:'; then
  errors+=("禁止添加 \`Co-authored-by:\` 署名")
fi
if printf '%s\n' "$raw" | grep -qiE 'generated (with|by)\b|🤖|noreply@|Claude Code|Cursor Agent'; then
  errors+=("禁止添加 AI 生成标记 / 工具署名（如 Generated with、🤖、Claude Code 等）")
fi

if ((${#errors[@]})); then
  echo "" >&2
  echo "✗ Commit message 不符合规范：" >&2
  echo "" >&2
  for e in "${errors[@]}"; do
    echo "  - $e" >&2
  done
  echo "" >&2
  echo "  示例：feat(api): 新增 OpenSubsonic ping 端点" >&2
  echo "  规范详见 .agents/skills/git-commit/SKILL.md" >&2
  echo "" >&2
  exit 1
fi

exit 0
