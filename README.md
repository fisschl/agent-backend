# agent-backend

一个基于 Rust + Axum 构建的高性能后端服务。

## 开发环境要求

- Rust 1.70+
- Cargo

## 构建与运行

安装依赖并运行：

```bash
cargo run
```

构建生产版本：

```bash
cargo build --release
```

## Docker 构建

使用 PowerShell 脚本构建：

```powershell
.\build.ps1
```

或直接使用 Docker：

```bash
docker build -t agent-backend .
docker run -p 3000:3000 agent-backend
```
