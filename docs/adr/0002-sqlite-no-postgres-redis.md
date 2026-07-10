# ADR-0002：用 SQLite，不引入 Postgres / Redis

**状态**：已接受（2026-07-10）

**背景**：个人/家庭（少量用户）自托管，硬约束是省内存 + 部署对小白友好。曾考虑既然用 docker-compose，是否换 Postgres、加 Redis。

**决策**：元数据索引用 **SQLite**（经 `sqlx`，开 WAL），放服务器本地磁盘。**不引入 Postgres、不引入 Redis**。docker-compose 只含 `服务端 + Garage + FFmpeg` 三件。

**理由**：
- SQLite 进程内嵌入式，无网络/IPC 往返，读密集场景常快于 Postgres（Navidrome 扛 10 万+ 曲库为证）。
- 短板是并发写，但本场景写负载极小（偶尔扫描/改标签/scrobble），WAL 足矣。
- Postgres/Redis 各拉起独立容器、常驻内存，顶撞"省内存 + 小白友好"，对少量用户零收益。
- Redis 能干的活儿被 OS 页缓存 / SQLite / 进程内 tokio 任务覆盖。

**后果**：并发写能力有限，但符合场景。用 `sqlx` 抽象保留未来迁 Postgres 的后路（若扩为多用户公开产品）。
