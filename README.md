# Claude Rust Desktop

<div align="center">

![Claude Logo](public/claude.svg)

**A high-performance desktop AI assistant powered by Claude, built with Tauri and Rust**

[English](README.md) | [中文](README.zh.md)

</div>

---

## Features

### 🤖 AI对话

- **流式响应** - 实时流式输出，响应更快
- **多模型支持** - 支持 Claude Opus、Sonnet、Haiku 等多版本模型
- **对话管理** - 创建、编辑、删除对话，管理会话历史
- **上下文记忆** - 智能上下文管理，支持长对话

### 💻 系统集成

- **原生桌面应用** - 基于 Tauri + Rust，性能卓越
- **文件系统访问** - 直接读取、编辑项目文件
- **终端集成** - 内嵌终端，支持命令执行
- **窗口管理** - 窗口调整、系统托盘

### 🛠️ 开发工具

- **代码高亮** - 支持 180+ 编程语言
- **Mermaid 图表** - 原生支持流程图、时序图
- **LaTeX 公式** - 支持数学公式渲染
- **文件上传** - 支持附件上传和文档处理

### 🔒 安全与隐私

- **本地API代理** - 支持自建API代理服务
- **自定义端点** - 支持配置 Anthropic API 或兼容接口
- **数据本地存储** - 对话记录本地保存

## Tech Stack

| Layer | Technology |
|-------|------------|
| Desktop Framework | [Tauri 2.x](https://tauri.app/) |
| Backend | Rust + Axum |
| Frontend | React 19 + TypeScript |
| Styling | Tailwind CSS |
| Build Tool | Vite |
| State Management | React Context + Hooks |

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Desktop Window                        │
├─────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────┐   │
│  │              React Frontend (WebView)             │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────────────┐ │   │
│  │  │  Chat UI │ │ Settings │ │  Model Selector │ │   │
│  │  └──────────┘ └──────────┘ └──────────────────┘ │   │
│  └─────────────────────────────────────────────────┘   │
│                          │                               │
│  ┌─────────────────────────────────────────────────┐   │
│  │           Rust Bridge Server (Axum)              │   │
│  │  ┌──────────────┐  ┌────────────────────────┐  │   │
│  │  │ API Router    │  │ WebSocket Handler     │  │   │
│  │  │ /api/chat     │  │ /api/chat/stream      │  │   │
│  │  └──────────────┘  └────────────────────────┘  │   │
│  └─────────────────────────────────────────────────┘   │
│                          │                               │
│  ┌─────────────────────────────────────────────────┐   │
│  │              Engine Pool (Concurrent)            │   │
│  │  ┌────────────┐ ┌────────────┐ ┌────────────┐  │   │
│  │  │  Engine 1  │ │  Engine 2  │ │  Engine N   │  │   │
│  │  └────────────┘ └────────────┘ └────────────┘  │   │
│  └─────────────────────────────────────────────────┘   │
│                          │                               │
│                    External APIs                         │
│         (Anthropic / Claude API / Custom Proxy)         │
└─────────────────────────────────────────────────────────┘
```

## Getting Started

### Prerequisites

- Node.js 18+
- Rust 1.70+
- npm or pnpm

### Installation

```bash
# Clone the repository
git clone https://github.com/lorryjovens-hub/claude-rust-desktop.git
cd claude-rust-desktop

# Install dependencies
npm install

# Run in development mode
npm run tauri dev
```

### Build

```bash
# Build for production
npm run tauri build
```

The executable will be generated in `src-tauri/target/release/`.

## Configuration

### API Settings

在设置页面配置以下选项：

| Setting | Description |
|---------|-------------|
| API Type | Anthropic / Claude API / Custom Proxy |
| API URL | API 端点地址 |
| API Key | 你的 API 密钥 |

### Environment Variables

```bash
# Optional: Set API key before launch
export ANTHROPIC_API_KEY="your-api-key"

# Optional: Custom API base URL
export ANTHROPIC_BASE_URL="https://api.anthropic.com"
```

## API Proxy (Optional)

项目包含一个可选的 Node.js API 代理服务，支持：

- 用户认证与注册
- 订阅管理
- 用量统计
- 请求转发到后端 AI 服务

### Setup API Proxy

```bash
cd api-proxy
npm install

# Set KIE API key
export KIE_API_KEY="your-kie-api-key"

# Start server
npm start
```

代理服务默认运行在 `http://127.0.0.1:30090`

## Project Structure

```
claude-desktop-tauri/
├── src/                    # React frontend source
│   ├── components/         # React components
│   ├── api.ts             # API client
│   ├── App.tsx            # Main app component
│   └── main.tsx           # Entry point
├── src-tauri/             # Rust backend source
│   ├── src/
│   │   ├── bridge/        # HTTP bridge server
│   │   ├── commands/      # Tauri commands
│   │   ├── engine/        # AI engine pool
│   │   └── tools/         # Tool definitions
│   ├── Cargo.toml         # Rust dependencies
│   └── tauri.conf.json    # Tauri configuration
├── api-proxy/             # Optional Node.js API proxy
├── public/               # Static assets
└── package.json          # Node dependencies
```

## Key Modules

### Bridge Server (`src-tauri/src/bridge/`)

基于 Axum 的 HTTP 服务器，处理前端请求：

- `/api/chat` - 聊天消息处理
- `/api/chat/stream` - 流式聊天响应
- `/api/tools` - 工具执行
- `/api/conversations` - 对话管理

### Engine Pool (`src-tauri/src/engine/`)

并发引擎池，管理多个 AI 会话：

- 引擎复用，减少创建开销
- 并发处理多个请求
- 自动清理空闲引擎

### Tools (`src-tauri/src/tools/`)

可执行的工具定义：

- 文件读取 / 写入
- 命令执行
- 项目搜索
- 任务管理

## Supported Models

| Model | Description |
|-------|-------------|
| claude-opus-4-6 | 最强推理能力 |
| claude-sonnet-4-6 | 平衡性能与速度 |
| claude-haiku-4-5 | 快速响应 |
| claude-opus-4 | 标准版 Opus |
| claude-sonnet-4 | 标准版 Sonnet |

## License

MIT License

## Contributing

Issues and Pull Requests are welcome!

---

<div align="center">

**Made with ❤️ using Tauri + Rust**

</div>
