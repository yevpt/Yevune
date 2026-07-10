# ADR-0005：storage 层选用 `object_store` 而非 `aws-sdk-s3`

**状态**：已接受（2026-07-10）

**背景**：T3 storage 模块需要一个 S3 兼容客户端读写 Garage（[ADR-0004] 定 Garage 为唯一源）。
AGENTS.md 技术栈表将对象存储库定为 `object_store` 或 `aws-sdk-s3` **二选一，选定后不再更换**。

**决策**：采用 **`object_store`**（crate `object_store`，启用 `aws` feature，用 `AmazonS3Builder` 指向 Garage/MinIO）。

**理由**：
- **更轻**：`object_store` 是聚焦对象存储的窄库；`aws-sdk-s3` 会拉进 `aws-config`/`aws-smithy-*` 整套 AWS SDK，编译体积与依赖树都更大，直接顶撞"服务端省内存 + 能不加就不加（YAGNI）"两条约束。
- **接口贴合**：其内建 `ObjectStore` trait 已提供 `put`/`get`/`get_range(Range<u64>)`/`head`（返回 `size`+`e_tag`）/`delete`/`list(_with_offset)`，与本任务要抽象的窄接口几乎一一对应，实现 Garage 后端只是薄封装。
- **S3 兼容 + 明文 HTTP**：`AmazonS3Builder` 支持自定义 `endpoint`、path-style、`with_allow_http(true)`，契合 Garage/MinIO 与"不强制 HTTPS"（[ADR-0004] / spec §10）。
- **多后端后路**：同一 trait 亦覆盖本地文件系统/内存后端，便于本地开发与未来迁移，无需换库。

**为何不选 `aws-sdk-s3`**：功能更全但我们只用到最基础的 GET/PUT/HEAD/LIST/DELETE，其余全是本场景用不上的重量级能力；引入它属过早优化。

**后果**：
- storage 层对外只暴露**自研的窄 `ObjectStore` trait**（见 `server/src/storage/mod.rs`），不泄漏 `object_store` 类型给 scanner/transcode，保留将来换实现的自由。
- 分页用 `list_with_offset`（start-after 语义），对外以不透明 `token`（= 上一页末尾 key）表达。
- 该选择**锁定**，后续不再更换（AGENTS.md 技术栈红线）。

[ADR-0004]: 0004-garage-sole-source.md
