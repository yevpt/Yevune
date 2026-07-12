# 音乐服务端镜像：musl 静态编译单二进制 + 运行期附带 FFmpeg。
#
# 构建上下文为仓库根（server 依赖 ../contract 路径 crate，必须一并纳入）。
# 运行阶段极简 alpine，仅带 FFmpeg（转码子进程）与 CA 证书。

# ── 构建阶段：musl 静态链接 ─────────────────────────────────────────────
FROM rust:1.97-alpine AS builder
# musl-dev/build-base 提供 musl-gcc，用于编译 sqlx 内置的 SQLite C 源码。
RUN apk add --no-cache musl-dev build-base
WORKDIR /build
COPY contract ./contract
COPY server ./server
WORKDIR /build/server
# 完全静态链接（含 crt），产出可在任意发行版运行的单二进制。
# crt-static 仅对目标三元组生效（经 .cargo/config.toml），避免误施于 host 上的 proc-macro。
RUN TRIPLE="$(uname -m)-unknown-linux-musl" && \
    mkdir -p .cargo && \
    printf '[target.%s]\nrustflags = ["-C", "target-feature=+crt-static"]\n' "$TRIPLE" > .cargo/config.toml && \
    cargo build --release --locked --bin yevune-server --target "$TRIPLE" && \
    cp "target/$TRIPLE/release/yevune-server" /yevune-server

# ── 运行阶段：极简 + FFmpeg ─────────────────────────────────────────────
FROM alpine:3.20
# FFmpeg 为按需转码子进程（非常驻服务），随镜像附带；ca-certificates 供可选 HTTPS 反代场景。
RUN apk add --no-cache ffmpeg ca-certificates
COPY --from=builder /yevune-server /usr/local/bin/yevune-server
# 默认明文 HTTP 监听（局域网小白友好，TLS/反代为进阶可选）。
ENV YEVUNE__SERVER__HOST=0.0.0.0 \
    YEVUNE__SERVER__PORT=4533
EXPOSE 4533
ENTRYPOINT ["yevune-server"]
