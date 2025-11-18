# agent-backend

一个基于 Rust + Axum 构建的高性能后端服务，主要用于支持 AI 代理相关功能。采用异步架构设计，支持 WebSocket 实时通信和 HTTP 代理转发，适用于低延迟、高并发场景。

## 核心功能

### 1. 实时 TTS（文本转语音）

- **WebSocket 接口**：`ws://localhost:3000/tts-realtime?voice={voice}`
- **功能特性**：
  - 支持实时文本转语音流式输出
  - 自动 Markdown 格式转换为纯文本
  - 支持长文本智能分片处理（超过 100 字符自动分片）
  - 输出格式：PCM，采样率 24kHz
  - 对接阿里云 DashScope TTS Realtime API

### 2. 兼容模式代理

- **HTTP 接口**：`POST /compatible-mode/v1/{path}`
- **功能特性**：
  - 透明代理转发到阿里云 DashScope 兼容模式 API
  - 自动处理请求/响应头黑名单过滤
  - 支持流式请求和响应
  - 自动注入 API Key（如果请求未携带）

## 技术栈

- **语言**：Rust (Edition 2024)
- **Web 框架**：Axum 0.8（支持 WebSocket）
- **异步运行时**：Tokio 1.48（完整特性）
- **HTTP 客户端**：Reqwest 0.12（支持流式传输）
- **WebSocket**：tokio-tungstenite 0.24
- **中间件**：tower-http（CORS、Trace）
- **日志**：tracing + tracing-subscriber
- **序列化**：serde + serde_json
- **其他**：uuid（v7）、base64、pulldown-cmark（Markdown 解析）

## 开发环境要求

- Rust 1.70+
- Cargo

## 环境变量配置

在项目根目录创建 `.env` 文件，配置以下环境变量：

```env
DASHSCOPE_API_KEY=your_api_key_here
```

**说明**：

- `DASHSCOPE_API_KEY`：阿里云 DashScope API 密钥（必填）
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

可执行文件位于 `target/release/agent-backend`

## Docker 构建

### 使用 PowerShell 脚本（Windows）

```powershell
.\build.ps1
```

### 使用 Docker 命令

```bash
# 构建镜像
docker build -t agent-backend .

# 运行容器
docker run -p 3000:3000 -e DASHSCOPE_API_KEY=your_api_key_here agent-backend
```

## API 接口文档

### 1. TTS 实时语音合成

**接口**：`GET /tts-realtime`  
**协议**：WebSocket  
**查询参数**：

- `voice`（必填）：音色参数，如 `Cherry`

**示例**：

```javascript
const ws = new WebSocket("ws://localhost:3000/tts-realtime?voice=Cherry");

// 发送文本（支持 Markdown 格式）
ws.send("你好，这是一段**测试**文本。");

// 接收音频数据（Binary 格式，PCM 编码）
ws.onmessage = (event) => {
  const audioData = event.data; // ArrayBuffer
  // 处理音频数据
};
```

### 2. 兼容模式代理

**接口**：`POST /compatible-mode/v1/{path}`  
**说明**：代理转发到 `https://dashscope.aliyuncs.com/compatible-mode/v1/{path}`

**示例**：

```bash
curl -X POST http://localhost:3000/compatible-mode/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen-plus",
    "messages": [{"role": "user", "content": "你好"}]
  }'
```

## 项目结构

```
.
├── src/
│   ├── main.rs                    # 程序入口，路由配置
│   ├── handlers.rs                # 处理器模块声明
│   └── handlers/
│       ├── tts_realtime.rs        # TTS 实时语音合成处理逻辑
│       └── compatible_mode.rs     # 兼容模式代理处理逻辑
├── Cargo.toml                     # 项目依赖配置
├── Cargo.lock                     # 依赖版本锁定
├── Dockerfile                     # Docker 镜像构建配置
├── build.ps1                      # Windows Docker 构建脚本
└── README.md                      # 项目说明文档
```

## 主要依赖说明

| 依赖库            | 版本 | 用途                      |
| ----------------- | ---- | ------------------------- |
| axum              | 0.8  | Web 框架，支持 WebSocket  |
| tokio             | 1.48 | 异步运行时                |
| tower-http        | 0.6  | CORS、Trace 中间件        |
| reqwest           | 0.12 | HTTP 客户端，支持流式传输 |
| tokio-tungstenite | 0.24 | WebSocket 客户端          |
| serde             | 1.0  | 序列化/反序列化           |
| tracing           | 0.1  | 结构化日志                |
| pulldown-cmark    | 0.12 | Markdown 解析器           |
| uuid              | 1.18 | UUID v7 生成              |
| base64            | 0.22 | Base64 编解码             |

## 日志

项目使用 `tracing` 进行日志记录，默认日志级别为 `DEBUG`。日志输出格式为 Pretty 格式，包含时间戳（RFC 3339）。

## 许可证

MIT
