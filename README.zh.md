# Claude Rust Desktop

<div align="center">

![Claude Logo](public/claude.svg)

**基于 Claude 的高性能桌面 AI 助手，采用 Tauri 和 Rust 构建**

[English](README.md) | [中文](README.zh.md)

</div>

---

## 功能特性

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

## 技术栈

| 层级 | 技术 |
|------|------|
| 桌面框架 | [Tauri 2.x](https://tauri.app/) |
| 后端 | Rust + Axum |
| 前端 | React 19 + TypeScript |
| 样式 | Tailwind CSS |
| 构建工具 | Vite |
| 状态管理 | React Context + Hooks |

## 系统架构

```
┌─────────────────────────────────────────────────────────┐
│                    桌面窗口                               │
├─────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────┐   │
│  │              React 前端 (WebView)                 │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────────────┐ │   │
│  │  │  聊天 UI  │ │   设置   │ │   模型选择器     │ │   │
│  │  └──────────┘ └──────────┘ └──────────────────┘ │   │
│  └─────────────────────────────────────────────────┘   │
│                          │                              │
│  ┌─────────────────────────────────────────────────┐   │
│  │           Rust 桥接服务器 (Axum)                   │   │
│  │  ┌──────────────┐  ┌────────────────────────┐  │   │
│  │  │ API 路由      │  │ WebSocket 处理器       │  │   │
│  │  │ /api/chat    │  │ /api/chat/stream       │  │   │
│  │  └──────────────┘  └────────────────────────┘  │   │
│  └─────────────────────────────────────────────────┘   │
│                          │                              │
│  ┌─────────────────────────────────────────────────┐   │
│  │              引擎池 (并发处理)                     │   │
│  │  ┌────────────┐ ┌────────────┐ ┌────────────┐  │   │
│  │  │  引擎 1    │ │  引擎 2    │ │  引擎 N    │  │   │
│  │  └────────────┘ └────────────┘ └────────────┘  │   │
│  └─────────────────────────────────────────────────┘   │
│                          │                              │
│                    外部 API                              │
│         (Anthropic / Claude API / 自定义代理)            │
└─────────────────────────────────────────────────────────┘
```

## 快速开始

### 前置要求

- Node.js 18+
- Rust 1.70+
- npm 或 pnpm

### 安装

```bash
# 克隆仓库
git clone https://github.com/lorryjovens-hub/claude-rust-desktop.git
cd claude-rust-desktop

# 安装依赖
npm install

# 开发模式运行
npm run tauri dev
```

### 构建

```bash
# 生产环境构建
npm run tauri build
```

可执行文件将生成在 `src-tauri/target/release/`。

## 配置

### API 设置

在设置页面配置以下选项：

| 设置项 | 说明 |
|--------|------|
| API 类型 | Anthropic / Claude API / 自定义代理 |
| API 地址 | API 端点 |
| API 密钥 | 你的 API 密钥 |

### 环境变量

```bash
# 可选：启动前设置 API 密钥
export ANTHROPIC_API_KEY="your-api-key"

# 可选：自定义 API 地址
export ANTHROPIC_BASE_URL="https://api.anthropic.com"
```

## API 代理 (可选)

项目包含一个可选的 Node.js API 代理服务，支持：

- 用户认证与注册
- 订阅管理
- 用量统计
- 请求转发到后端 AI 服务

### 配置 API 代理

```bash
cd api-proxy
npm install

# 设置 KIE API 密钥
export KIE_API_KEY="your-kie-api-key"

# 启动服务
npm start
```

代理服务默认运行在 `http://127.0.0.1:30090`

## 项目结构

```
claude-desktop-tauri/
├── src/                    # React 前端源码
│   ├── components/         # React 组件
│   ├── api.ts             # API 客户端
│   ├── App.tsx            # 主应用组件
│   └── main.tsx           # 入口文件
├── src-tauri/             # Rust 后端源码
│   ├── src/
│   │   ├── bridge/        # HTTP 桥接服务器
│   │   ├── commands/      # Tauri 命令
│   │   ├── engine/        # AI 引擎池
│   │   └── tools/         # 工具定义
│   ├── Cargo.toml         # Rust 依赖
│   └── tauri.conf.json    # Tauri 配置
├── api-proxy/             # 可选 Node.js API 代理
├── public/               # 静态资源
└── package.json          # Node 依赖
```

## 核心模块

### 桥接服务器 (`src-tauri/src/bridge/`)

基于 Axum 的 HTTP 服务器，处理前端请求：

- `/api/chat` - 聊天消息处理
- `/api/chat/stream` - 流式聊天响应
- `/api/tools` - 工具执行
- `/api/conversations` - 对话管理

### 引擎池 (`src-tauri/src/engine/`)

并发引擎池，管理多个 AI 会话：

- 引擎复用，减少创建开销
- 并发处理多个请求
- 自动清理空闲引擎

### 工具 (`src-tauri/src/tools/`)

可执行的工具定义：

- 文件读取 / 写入
- 命令执行
- 项目搜索
- 任务管理

## 支持的模型

| 模型 | 说明 |
|------|------|
| claude-opus-4-6 | 最强推理能力 |
| claude-sonnet-4-6 | 平衡性能与速度 |
| claude-haiku-4-5 | 快速响应 |
| claude-opus-4 | 标准版 Opus |
| claude-sonnet-4 | 标准版 Sonnet |

## 许可证

MIT License

## 贡献

欢迎提交 Issue 和 Pull Request！

---

<div align="center">

**使用 ❤️ 基于 Tauri + Rust 构建**

</div>
