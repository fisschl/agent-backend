# Chef 阶段：安装 cargo-chef
FROM rust:1 AS chef
WORKDIR /root
RUN cargo install cargo-chef

# Planner 阶段：分析项目依赖
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Builder 阶段：构建依赖和项目
FROM chef AS builder
WORKDIR /root
COPY --from=planner /root/recipe.json recipe.json
# 构建依赖（这将被缓存）
RUN cargo chef cook --release --recipe-path recipe.json
# 复制源代码
COPY . .
# 构建项目
RUN cargo build --release

# 最终阶段：创建包含构建产物的镜像
FROM rust:1
WORKDIR /root
COPY --from=builder /root/target/release/agent-backend ./agent-backend

EXPOSE 3000

CMD ["./agent-backend"]
