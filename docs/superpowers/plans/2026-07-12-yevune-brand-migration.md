# Yevune Brand Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Use Yevune for the repository and every deployable project identifier, without retaining data compatibility.

**Architecture:** The migration changes package/module identifiers first, then deployment and storage defaults, then the Apple FFI boundary and documentation. OpenSubsonic routes and response schema remain intact; only the server `type` value changes to `yevune-server`. The filesystem root moves only after all tracked changes are committed and verified.

**Tech Stack:** Rust/Cargo, UniFFI, SwiftPM/SwiftUI, Docker Compose, Garage S3, Bash.

## Global Constraints

- Do not add dependencies or change the Rust, SQLite, Garage, OpenSubsonic, native-UI, streaming, or authorization architecture.
- Use `yevune-server`/`yevune_server`, `yevune-core`/`yevune_core`, and `YevuneCoreFFI` consistently.
- Use `YEVUNE` for environment variable prefixes, `yevune` for the Garage bucket, and `library/` for formal audio object keys.
- Old deployments and data are intentionally incompatible; do not add a migration or compatibility layer.
- Keep ordinary domain-language uses of “music” intact; remove only project, build, module, storage, and configuration identifiers.

---

## File structure

- `server/`: service package/binary, config prefix/defaults, response brand, tests, Docker build and deployment configuration.
- `core/`: client-core Cargo package/library and UniFFI-generated module naming.
- `clients/apple/`: Yevune SwiftPM executable, FFI wrapper and all Swift imports/tests.
- `scripts/`: one-click launcher and its black-box shell test.
- `README.md`, `AGENTS.md`, `CLAUDE.md`, `docs/`: user/developer instructions and branded paths.

### Task 1: Rename Rust and deployment identifiers

**Files:**
- Modify: `server/Cargo.toml`, `server/Cargo.lock`, `server/src/**/*.rs`, `server/tests/**/*.rs`, `core/Cargo.toml`, `core/Cargo.lock`, `core/src/**/*.rs`, `core/tests/**/*.rs`, `core/uniffi.toml`, `contract/tests/response_test.rs`, `Dockerfile`, `docker-compose.yml`, `.env.example`, `README.md`, `deploy/garage.toml`.

**Interfaces:**
- Produces: Cargo packages `yevune-server` and `yevune-core`; Rust libraries `yevune_server` and `yevune_core`; `YEVUNE__*` configuration; `YEVUNE_APP_SECRET`; `yevune` bucket; `library/` object keys; OpenSubsonic `type=yevune-server`.

- [ ] **Step 1: Write failing expectations for the new public identifiers.**

Update `server/src/config.rs` tests to assert `bucket == "yevune"`, `path == "./data/yevune.sqlite"`, and `YEVUNE__SERVER__PORT`; update `contract/tests/response_test.rs` to expect `"yevune-server"`; update `server/tests/deploy_test.rs` to require `--bin yevune-server` and `YEVUNE__GARAGE__BUCKET: "yevune"`.

- [ ] **Step 2: Run the focused tests and verify they fail against legacy names.**

Run: `cargo test --manifest-path server/Cargo.toml config::tests::默认值合理 --lib && cargo test --manifest-path contract/Cargo.toml --test response_test`

Expected: target assertions fail until the current identifiers are applied.

- [ ] **Step 3: Apply the exact identifier mapping.**

Set Cargo package names to `yevune-server` and `yevune-core`, Rust imports to `yevune_server` and `yevune_core`, the UniFFI module and filename to `YevuneCoreFFI`, and `SERVER_TYPE` to `"yevune-server"`. Use `YEVUNE` configuration, the `yevune` Garage bucket, `yevune.sqlite`, and `library/` formal object keys in deployment files and fixtures.

- [ ] **Step 4: Verify Rust and deployment behavior.**

Run: `cargo test --manifest-path contract/Cargo.toml && cargo test --manifest-path core/Cargo.toml && cargo test --manifest-path server/Cargo.toml && cargo clippy --manifest-path contract/Cargo.toml -- -D warnings && cargo clippy --manifest-path core/Cargo.toml -- -D warnings && cargo clippy --manifest-path server/Cargo.toml -- -D warnings && cargo fmt --manifest-path server/Cargo.toml --check && docker compose config`

Expected: all tests and Clippy checks pass; rendered Compose config has `yevune` and `YEVUNE_*` identifiers.

- [ ] **Step 5: Commit the Rust/deployment migration.**

Run: `git add server core contract Dockerfile docker-compose.yml .env.example README.md deploy/garage.toml && git commit -m 'refactor(rename): 迁移 Rust 与部署标识为 Yevune'`

### Task 2: Rename Apple and UniFFI integration

**Files:**
- Apple FFI package: `clients/apple/Packages/YevuneCoreFFI`.
- Apple application sources: `clients/apple/Sources/Yevune`.
- Apple tests: `clients/apple/Tests/YevuneTests`.
- Modify: `clients/apple/Package.swift`, all moved Swift source/tests, `clients/apple/Packages/YevuneCoreFFI/scripts/build-core.sh`, `.gitignore`, `scripts/run-mac-client.sh`, `scripts/tests/run-mac-client-test.sh`.

**Interfaces:**
- Consumes: `YevuneCoreFFI` bindings generated from Task 1.
- Produces: SwiftPM executable `Yevune`, application type `YevuneApp`, test target `YevuneTests`.

- [ ] **Step 1: Update the shell test to expect the new executable and FFI path.**

Change `scripts/tests/run-mac-client-test.sh` assertions to require `YevuneCoreFFI` paths and `swift run --package-path clients/apple Yevune`.

- [ ] **Step 2: Run the shell test and verify it fails against legacy paths.**

Run: `scripts/tests/run-mac-client-test.sh`

Expected: the test rejects a launcher that does not emit the `Yevune` command.

- [ ] **Step 3: Apply target paths and module identifiers.**

Use the `YevuneCoreFFI`, `Sources/Yevune`, and `Tests/YevuneTests` directories. Set SwiftPM package/product/executable/test target names to `Yevune`/`YevuneTests`; use `import YevuneCoreFFI` and `@testable import Yevune`; name the entry struct `YevuneApp`; and use `libyevune_core` in the binding generator.

- [ ] **Step 4: Build bindings and verify the Apple client.**

Run: `clients/apple/Packages/YevuneCoreFFI/scripts/build-core.sh && swift test --package-path clients/apple && scripts/tests/run-mac-client-test.sh`

Expected: bindings exist under `Packages/YevuneCoreFFI`, all Swift tests pass, and the launcher test observes `Yevune`.

- [ ] **Step 5: Commit the Apple migration.**

Run: `git add clients/apple .gitignore scripts/run-mac-client.sh scripts/tests/run-mac-client-test.sh && git commit -m 'refactor(rename): 迁移 Apple 客户端为 Yevune'`

### Task 3: Update documentation and repository references

**Files:**
- Modify: `AGENTS.md`, `CLAUDE.md`, `README.md`, `docs/adr/*.md`, `docs/superpowers/specs/*.md`, `docs/superpowers/plans/*.md`, `openapi.yaml`.

**Interfaces:**
- Consumes: identifiers from Tasks 1–2.
- Produces: instructions and historical plans that name current paths, commands and deployment variables correctly.

- [ ] **Step 1: Add a failing repository-brand check.**

Add `scripts/tests/test-yevune-brand.sh` to search tracked configuration, source, and documentation files for prohibited legacy project identifiers. Exclude the check script itself and exit nonzero when a match is found.

- [ ] **Step 2: Run the check and verify it fails on legacy references.**

Run: `scripts/tests/test-yevune-brand.sh`

Expected: it reports the legacy branding still present in documentation and generated OpenAPI metadata.

- [ ] **Step 3: Update only project identifiers.**

Update titles, links, directory trees, command examples, crate/module names, Docker/Compose names, environment variables, S3 setup and API `type` examples. Do not replace prose uses such as “音乐服务” or protocol endpoint names.

- [ ] **Step 4: Verify the brand check and project checks.**

Run: `scripts/tests/test-yevune-brand.sh && cargo fmt --manifest-path server/Cargo.toml --check && swift test --package-path clients/apple`

Expected: the brand check finds no legacy project identifier and the builds remain green.

- [ ] **Step 5: Commit the documentation migration.**

Run: `git add AGENTS.md CLAUDE.md README.md docs openapi.yaml scripts/tests/test-yevune-brand.sh && git commit -m 'docs(rename): 更新 Yevune 项目说明'`

### Task 4: Confirm the repository directory and restore hooks

**Files:**
- Modify: Git worktree metadata and local hooks configuration after the repository is located at `/Users/vpt/Documents/Codes/Yevune`.

**Interfaces:**
- Consumes: clean `main` after Tasks 1–3.
- Produces: a repository opened from `/Users/vpt/Documents/Codes/Yevune` with `core.hooksPath=.githooks`.

- [ ] **Step 1: Verify a clean, linear repository.**

Run: `git status --short && test -z "$(git rev-list --merges HEAD)" && git worktree list`

Expected: no tracked or untracked changes and no merge commits; any linked worktree is already re-homed or removed.

- [ ] **Step 2: Restore hooks from the Yevune root.**

Run: `cd /Users/vpt/Documents/Codes/Yevune && git config core.hooksPath .githooks && ./scripts/setup-git-hooks.sh`

Expected: `pwd` is `/Users/vpt/Documents/Codes/Yevune` and Git resolves hooks from `Yevune/.githooks`.

- [ ] **Step 3: Verify from the new root.**

Run: `git status --short && git log -1 --oneline && scripts/tests/test-validate-no-merge-commit.sh && cargo test --manifest-path server/Cargo.toml && swift test --package-path clients/apple`

Expected: a clean repository, working hooks, and passing server/Apple tests from the renamed directory.
