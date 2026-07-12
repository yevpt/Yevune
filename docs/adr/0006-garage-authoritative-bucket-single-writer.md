# ADR-0006：Garage 权威 bucket 采用单写者与 `library/` 正式键

**状态**：已接受（2026-07-11）

**背景**：T8 的 `moveTrack`、`deleteTrack` 与失败补偿需要删除 Garage 对象。Garage v2.3 的 DeleteObject 是无条件删除；实测 `If-Match` 与旧 `versionId` 都不能阻止删除当前版本，同时 `object_store 0.14` 没有条件删除 API。因此 `head(ETag) → delete` 不能保护来自其他 writer 的并发覆盖。

**决策**：

- 权威 `yevune` bucket 只向**单个服务端实例**授予写/删凭据，该实例是唯一 writer。
- 正式原始音频键统一使用非空 `library/...` 前缀；`uploadTrack` 与 `moveTrack` 拒绝 `inbox/...`、其它前缀和空相对路径。
- 服务端实例内以共享逐键锁串行化 track、源键、目标键，并用旧 `object_key + etag` 做 SQLite CAS。单写者前提下，补偿可在持锁且当前 ETag 匹配本次 put 后执行普通删除。
- 外部直传使用独立 inbox bucket 与独立凭据。inbox 仅是非权威暂存，不能直接进入权威 bucket；本阶段不新增 inbox 消费接口。
- 多实例部署前必须先引入跨实例协调与可证明的对象所有权协议，不得直接扩展当前写路径。

**理由**：

- 不伪装 Garage 已提供原子条件删除；把并发边界收紧到当前产品明确不做的水平扩展之外。
- 权威 bucket 凭据隔离后，进程内逐键锁覆盖全部合法 writer，普通删除不会与另一个合法 writer 竞争。
- inbox 与权威数据分桶，既保留未来外部导入能力，也避免绕过服务端索引、授权和补偿状态机。

**后果**：

- 部署文档和凭据配置必须保证权威 bucket 单写者；第二服务端实例不得复用写凭据。
- 管理 API 的 key 兼容性收紧为 `library/...`，旧的非正式前缀需先迁移后才能由管理 API 操作。
- 若未来要求多实例或外部直接改权威 bucket，必须先更新本 ADR，并实现跨实例锁/租约或支持真实条件删除的存储能力。
