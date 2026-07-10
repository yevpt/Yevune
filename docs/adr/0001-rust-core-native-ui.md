# ADR-0001：Rust 共享核心 + 各平台原生 UI（UniFFI）

**状态**：已接受（2026-07-10）

**背景**：需覆盖 iOS/macOS/Web/Android/Win/Linux 多平台，硬约束是原生流畅 + 省内存 + 接口类型全平台复用。

**决策**：客户端逻辑（API 客户端、认证、离线缓存、播放队列/状态机、同步）写在 Rust `core` crate，用 UniFFI 生成 Swift/Kotlin 绑定；共享类型在 `contract` crate。UI 各平台原生（SwiftUI/Compose/…）。Web 先用 `contract` 生成的 TS 类型 + REST 直连。音频输出留各平台原生层。

**理由**：
- 逻辑写一次，新增平台只写 UI，核心零改动。
- 类型从同一份 Rust 定义生成，接口不可能漂移。
- 原生 UI 满足省内存高流畅；排除 Electron/Flutter/RN。
- 生产验证充分：Matrix/Element X、Firefox、Signal、1Password、Bitwarden。

**后果**：初次需搭建 Rust→Xcode/Gradle 构建管线（一次性成本，有 cargo-swift/cargo-ndk 辅助）。跨 FFI 边界需谨慎设计接口。
