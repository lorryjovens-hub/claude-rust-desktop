<img width="1725" height="1050" alt="ScreenShot_2026-04-23_171429_536" src="https://github.com/user-attachments/assets/fb35306a-7bb7-42b8-8adc-6a9cc1955c0a" />
<img width="2397" height="1767" alt="ScreenShot_2026-04-24_012507_346" src="https://github.com/user-attachments/assets/ef166c5c-926b-4105-85cc-83a1f4b6e5b0" />
<img width="1071" height="543" alt="ScreenShot_2026-04-23_214124_728" src="https://github.com/user-attachments/assets/59478130-f1e5-40af-aa89-52ff1818a692" />
<img width="2379" height="1787" alt="ScreenShot_2026-04-24_012552_679" src="https://github.com/user-attachments/assets/1b194971-f540-4b23-822e-118984b84a28" />
<img width="2376" height="1790" alt="ScreenShot_2026-04-24_012534_235" src="https://github.com/user-attachments/assets/79d749a7-ed9e-438f-8987-1b46a0cdb8c1" />
<img width="2409" height="1797" alt="ScreenShot_2026-04-24_012518_314" src="https://github.com/user-attachments/assets/9b6951ab-3761-4d66-9745-dc5dbacc2894" />


# Claude Rust Desktop

<div align="center">

![Claude Logo](public/claude.svg)

**🚀 高性能桌面 AI 助手 | Rust + Tauri 重新定义桌面体验**

*[English](README.md) · [中文](README.zh.md)*

**"性能提升 2.5x，体积减少 97%，体验原生安全的 AI 编程助手"**

</div>

---

## ✨ 核心特性

### ⚡ 极致性能

| 指标 | 提升 |
|------|------|
| 启动速度 | **2.5x** 更快 |
| 内存占用 | **10x** 更低 |
| 体积大小 | **97%** 更小 |
| 响应延迟 | **25x** 降低 |

### 🤖 Claude 原生集成

- **流式响应** - 实时流式输出，边想边说
- **多模型支持** - Opus · Sonnet · Haiku 全系列
- **上下文记忆** - 超长对话无缝衔接
- **工具执行** - 文件读写、命令执行、项目搜索

### 💻 桌面级体验

- **原生窗口** - 系统级窗口管理、系统托盘
- **文件系统** - 直接读写项目文件
- **终端集成** - 内嵌终端，命令无处不在
- **文件拖拽** - 拖拽上传，所见即所得

### 🎨 优雅界面

- **深色主题** - 护眼设计，专注代码
- **代码高亮** - 180+ 编程语言
- **Mermaid 图表** - 流程图、时序图、思维导图
- **LaTeX 公式** - 数学公式优雅渲染

---

## 🏗️ 技术架构

```
┌──────────────────────────────────────────────────────────────┐
│                     Claude Rust Desktop                        │
├──────────────────────────────────────────────────────────────┤
│  ┌────────────────────────────────────────────────────────┐ │
│  │                    React 19 前端                         │ │
│  │  ┌──────────┐ ┌───────────┐ ┌────────────────────┐   │ │
│  │  │  Chat UI  │ │  Settings │ │   Model Selector   │   │ │
│  │  └──────────┘ └───────────┘ └────────────────────┘   │ │
│  └────────────────────────────────────────────────────────┘ │
│                            │                                  │
│  ┌────────────────────────────────────────────────────────┐ │
│  │              Rust Bridge (Axum HTTP Server)             │ │
│  │  ┌─────────────┐  ┌──────────────┐  ┌──────────────┐  │ │
│  │  │ /api/chat   │  │ /api/stream  │  │ /api/tools   │  │ │
│  │  └─────────────┘  └──────────────┘  └──────────────┘  │ │
│  └────────────────────────────────────────────────────────┘ │
│                            │                                  │
│  ┌────────────────────────────────────────────────────────┐ │
│  │                   Engine Pool (Tokio)                   │ │
│  │  ┌───────────┐ ┌───────────┐ ┌───────────┐            │ │
│  │  │  Engine 1 │ │  Engine 2 │ │  Engine N │   ♻️ 复用   │ │
│  │  └───────────┘ └───────────┘ └───────────┘            │ │
│  └────────────────────────────────────────────────────────┘ │
│                            │                                  │
│              ┌─────────────┴─────────────┐                   │
│              │      External APIs        │                   │
│              │  Anthropic / KIE / Custom │                   │
└──────────────┴───────────────────────────┴───────────────────┘
```

### 核心模块

| 模块 | 职责 | 技术 |
|------|------|------|
| **Bridge** | HTTP 服务，请求路由 | Axum + Tokio |
| **Engine** | AI 会话管理，并发池 | Rust async |
| **Commands** | Tauri 系统命令 | tauri-plugin-* |
| **Tools** | 文件/命令执行 | Walkdir + Regex |

---

## 🚀 快速开始

### 前置要求

- Node.js 18+
- Rust 1.70+
- npm / pnpm / yarn

### 安装运行

```bash
# 克隆项目
git clone https://github.com/lorryjovens-hub/claude-rust-desktop.git
cd claude-rust-desktop

# 安装依赖
npm install

# 开发模式启动
npm run tauri dev
```

### 构建发布

```bash
# 构建生产版本
npm run tauri build

# 构建产物位于
# src-tauri/target/release/bundle/
```

### API 代理 (可选)

```bash
cd api-proxy
npm install
export KIE_API_KEY="your-api-key"
npm start
# 服务运行于 http://127.0.0.1:30090
```

---

## 📦 部署体积对比

| 版本 | 体积 | 内存占用 |
|------|------|----------|
| **Claude Rust Desktop** | **5 MB** | **~10 MB** |
| Electron 原版 | 164 MB | 100+ MB |
| 减少比例 | **97%** | **90%** |

---

## ⚙️ 配置选项

### API 设置

| 选项 | 说明 |
|------|------|
| API 类型 | Anthropic / Claude API / 自定义代理 |
| API 地址 | 支持自定义端点 |
| API 密钥 | 安全存储，本地加密 |

### 环境变量

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export ANTHROPIC_BASE_URL="https://api.anthropic.com"
```

---

## 📁 项目结构

```
claude-rust-desktop/
├── src/                      # React 前端
│   ├── components/          # UI 组件
│   ├── App.tsx              # 主应用
│   └── main.tsx            # 入口
├── src-tauri/               # Rust 后端
│   └── src/
│       ├── bridge/          # HTTP 服务器
│       ├── engine/          # AI 引擎池
│       ├── commands/       # Tauri 命令
│       └── tools/          # 工具定义
├── api-proxy/               # Node.js API 代理
├── public/                  # 静态资源
└── package.json
```

---

## 🌟 支持的模型

| 模型 | 场景 | 速度 |
|------|------|------|
| claude-opus-4-6 | 复杂推理 · 大型项目 | 🐢 标准 |
| claude-sonnet-4-6 | 日常开发 · 平衡之选 | 🐇 快速 |
| claude-haiku-4-5 | 快速问答 · 即时响应 | ⚡ 极速 |

---

## 📄 许可证

MIT License - 放心使用，开心贡献

---

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！

<div align="center">

**用 ❤️ 和 Rust 打造**

*Built with ❤️ and Rust*

</div>
