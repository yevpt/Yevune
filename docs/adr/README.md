# 架构决策记录 (ADR)

每条记录一个关键决策 + 理由，防止未来重新讨论已定问题。格式：背景 → 决策 → 理由 → 后果。

| 编号 | 决策 |
|---|---|
| [0001](0001-rust-core-native-ui.md) | Rust 共享核心 + 各平台原生 UI（UniFFI） |
| [0002](0002-sqlite-no-postgres-redis.md) | 用 SQLite，不引入 Postgres / Redis |
| [0003](0003-opensubsonic-plus-extensions.md) | OpenSubsonic 兼容 + 自研扩展 |
| [0004](0004-garage-sole-source.md) | Garage 为唯一源，SQLite 本地 + 转码缓存入 Garage |
| [0005](0005-object-store-over-aws-sdk.md) | storage 层选用 `object_store` 而非 `aws-sdk-s3` |
| [0006](0006-garage-authoritative-bucket-single-writer.md) | Garage 权威 bucket 采用单写者与 `library/` 正式键 |
