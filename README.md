# free-model

一个基于 Rust + Axum 构建的高性能后端服务，提供 DeepSeek API 代理功能。采用异步架构设计，支持 HTTP 流式传输，适用于 AI 代理相关场景。

## 核心功能

### DeepSeek API 代理

- **HTTP 接口**：`POST /chat/completions`
- **功能特性**:
  - 透明代理转发到 DeepSeek API (`https://api.deepseek.com/chat/completions`)
  - 支持流式请求和响应
  - 自动处理请求/响应头过滤
  - 自动注入 API Key（如果请求未携带 Authorization 头）

## 技术栈

- **语言**：Rust (Edition 2021)
- **Web 框架**：Axum 0.8
- **异步运行时**：Tokio 1.48（完整特性）
- **HTTP 客户端**：Reqwest 0.12（支持流式传输）
- **中间件**：tower-http（CORS、Trace）
- **日志**：tracing + tracing-subscriber
- **其他**：dotenvy（环境变量加载）

## 开发环境要求

- Rust 1.70+
- Cargo

## 环境变量配置

在项目根目录创建 `.env` 文件，配置以下环境变量：

```env
DEEPSEEK_API_KEY=your_api_key_here
```

**说明**：

- `DEEPSEEK_API_KEY`：DeepSeek API 密钥（必填）
- 如果未配置，程序启动时会报错并退出

## 构建与运行

### 本地开发

安装依赖并运行：

```bash
cargo run
```

服务将启动在 `http://localhost:3000`

### 生产构建

构建优化版本：

```bash
cargo build --release
```

可执行文件位于 `target/release/free-model`

## Docker 构建

### 使用 PowerShell 脚本（Windows）

```powershell
.\build.ps1
```

### 使用 Docker 命令

```bash
# 构建镜像
docker build -t free-model .

# 运行容器
docker run -p 3000:3000 -e DEEPSEEK_API_KEY=your_api_key_here free-model
```

## API 接口文档

### DeepSeek Chat Completions 代理

**接口**：`POST /chat/completions`  
**说明**：代理转发到 DeepSeek API，支持所有 DeepSeek Chat Completions 的参数和功能。

**请求示例**：

```bash
curl -X POST http://localhost:3000/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "deepseek-chat",
    "messages": [
      {
        "role": "user",
        "content": "你好，请介绍一下 Rust 语言"
      }
    ],
    "stream": true
  }'
```

**特性说明**：

- 如果请求头中未包含 `Authorization`，会自动使用配置的 `DEEPSEEK_API_KEY`
- 完全支持 DeepSeek API 的所有参数和选项
- 支持流式响应（设置 `"stream": true`）
- 请求和响应头和体都会被透明转发

## 项目结构

```
.
├── src/
│   ├── main.rs                    # 程序入口，路由配置
│   └── handlers/
│       └── chat_completions.rs    # DeepSeek API 代理处理逻辑
├── Cargo.toml                     # 项目依赖配置
├── Cargo.lock                     # 依赖版本锁定
├── Dockerfile                     # Docker 镜像构建配置
├── build.ps1                      # Windows Docker 构建脚本
└── README.md                      # 项目说明文档
```

## 主要依赖说明

| 依赖库      | 版本 | 用途                  |
| ----------- | ---- | --------------------- |
| axum        | 0.8  | Web 框架              |
| tokio       | 1.48 | 异步运行时            |
| tower-http  | 0.6  | CORS、Trace 中间件    |
| reqwest     | 0.12 | HTTP 客户端，支持流式 |
| serde       | 1.0  | 序列化/反序列化       |
| tracing     | 0.1  | 结构化日志            |
| dotenvy     | 0.15 | 环境变量加载          |

## 日志

项目使用 `tracing` 进行日志记录，默认日志级别为 `DEBUG`。日志输出格式为 Pretty 格式，包含时间戳（RFC 3339）。

## 许可证

MIT
