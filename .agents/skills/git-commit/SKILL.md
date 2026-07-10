---
name: git-commit
description: >-
  Use when writing or amending a git commit in this repo. Enforces commit
  identity (yevpt / vpt940417@gmail.com), Conventional Commits with a Chinese
  subject, and forbids author overrides. Trigger whenever you are about to run
  `git commit`, generate a commit message, amend a commit, or are asked to
  commit changes.
---

# Git Commit 规范

由 `.githooks` 强制校验（`pre-commit` 查身份，`commit-msg` 查信息），不合规会被拒。请第一次就写对。

## 身份（强制）

- Author / Committer **必须**是：`yevpt <vpt940417@gmail.com>`
- **只**使用已配置的 `user.name` / `user.email` 直接提交
- **禁止**任何身份覆盖：
  - `git -c user.name=...` / `git -c user.email=...`
  - `git commit --author=...`
  - 设置 `GIT_AUTHOR_NAME` / `GIT_AUTHOR_EMAIL` / `GIT_COMMITTER_NAME` / `GIT_COMMITTER_EMAIL`
- **禁止** `Co-authored-by:`、`Generated with`、🤖、Claude Code、Cursor Agent 等 AI 署名/生成标记

## 提交信息

格式：`<type>(<scope>): <中文主题>`，可选正文（中文 bullet）与 footer。

- **type**（必填，小写）：`feat` `fix` `refactor` `perf` `test` `docs` `style` `build` `chore` `ci`
- **scope**（可选）：英文小写技术词，可含数字/连字符，如 `api`、`scanner`、`transcode`
- **主题**：冒号后留一空格；用中文、动词开头、≤50 字、结尾不加句号
- **破坏性变更**：footer 写 `BREAKING CHANGE: <描述>`（全大写+冒号）

示例：

```
feat(api): 新增 OpenSubsonic ping 端点
fix(scanner): 修复重复扫描漏删曲目
docs: 补充服务端实现计划交接提示词
```

## 操作步骤

1. 先读本 skill，再写 message
2. `git status` / `git diff` / `git log` 确认改动与风格
3. `git add` 相关文件后执行普通 `git commit`（可用 HEREDOC 传 message）
4. **不要**加 `-c user.*`、`--author`、`--no-verify`

常见错误：用 `-c` 覆盖作者、缺 type、主题非中文、冒号后无空格、超 50 字、结尾带句号、加 AI 署名。
