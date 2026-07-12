---
name: git-commit
description: Use when preparing or amending commits or integrating Git branches in this repo.
---

# Git Commit 规范

`.githooks` 强制校验身份、提交信息和线性历史；不要绕过它。

## 线性历史（强制）

所有分支整合必须线性：先 rebase，再 fast-forward；禁止产生或保留 merge commit。

```bash
git switch <feature-branch>
git rebase <target-branch>
# 有冲突：解决后 git add <files> && git rebase --continue
git switch <target-branch>
git merge --ff-only <feature-branch>
```

- rebase 冲突必须解决并继续；放弃时用 `git rebase --abort`，不得改用普通 `git merge`、`--no-ff` 或 merge 冲突后的 `git commit`。
- 推送前执行 `test -z "$(git rev-list --merges HEAD)"`；禁止使用 `--no-verify`。

## 身份（强制）

- Author / Committer 必须是 `yevpt <vpt940417@gmail.com>`；直接使用已配置的身份。
- 禁止 `-c user.*`、`--author`、`GIT_*_NAME/EMAIL`、`Co-authored-by:` 及任何 AI 署名/生成标记。

## 提交信息

格式：`<type>(<scope>): <中文主题>`，例如 `feat(api): 新增 OpenSubsonic ping 端点`。

- type：`feat`、`fix`、`refactor`、`perf`、`test`、`docs`、`style`、`build`、`chore`、`ci`；scope 可选且须为小写技术词。
- 主题须中文、动词开头、≤50 字、冒号后一个空格、结尾无句号；破坏性变更写 `BREAKING CHANGE: <描述>`。

## 操作步骤

1. 用 `git status`、`git diff`、`git log` 确认改动。
2. `git add` 后执行普通 `git commit`；不得加身份覆盖参数或 `--no-verify`。
