# Claude Desktop Tauri — 全面架构升级设计方案

> 日期：2026-05-29
> 版本：v1.0
> 状态：草案

---

## 1. 概述

### 1.1 目标

将现有 Claude Desktop (Tauri Edition) 从功能原型级代码重构为**专业级生产环境桌面应用**，解决安全、架构、代码质量和可维护性的根本问题。

### 1.2 当前问题摘要

| 类别 | 具体问题 | 严重程度 |
|------|---------|---------|
| 安全 | CSP 无效 (`unsafe-inline` + `unsafe-eval`) | 🔴 |
| 安全 | 文件系统 API 无路径穿越防护 | 🔴 |
| 安全 | 任意进程启动接口 | 🔴 |
| 安全 | API Key 存 localStorage | 🔴 |
| 安全 | 整个 crate 级 `#![allow(...)]` 关闭所有 Rust 安全检查 | 🔴 |
| 架构 | 24 元素元组作为 AppState → 代码中到处 `state.6`、`state.21` | 🟠 |
| 架构 | 2935 行单文件 `bridge/mod.rs` | 🟠 |
| 架构 | 50+ 模块无层次结构，扁平分不清核心与外围 | 🟠 |
| 架构 | 递归目录树构建无循环检测 | 🟠 |
| 代码质量 | `tokio::sync::Mutex` 滥用 | 🟠 |
| 代码质量 | App.tsx >700 行，30+ `useState` + 散落 `useEffect` | 🟠 |
| 代码质量 | 多处静默吞掉错误（memory 模块等） | 🟠 |
| 测试 | 全局仅 2 个单元测试 | 🔴 |
| 文档 | README 严重夸大（不存在的 GitHub 仓库、Stars、下载量） | 🟠 |

---

## 2. 架构决策

### 2.1 通信层：保留 Axum HTTP 桥，改造为专业级实现

**决策**：不迁移到 Tauri IPC，继续使用 Axum 但完全重构。

**理由**：
- 桌面应用性能瓶颈是 LLM API 调用（秒级），IPC 机制差异（微秒级）无实际影响
- Axum SSE 是流式响应的最成熟方案，Tauri events 实现同等功能更复杂
- 50+ 模块全部从 Axum handler 迁移到 Tauri command 成本极高，无实际收益
- 通过安全加固（CSP、path validation、auth middleware）可达专业级安全水平

### 2.2 整体架构：分层领域驱动

```
┌─────────────────────────────────────────────┐
│                Frontend (React)              │
│  Components │ Stores │ Router │ API Client   │
└──────────────────┬──────────────────────────┘
                   │ HTTP / SSE
┌──────────────────▼──────────────────────────┐
│     API Layer (Axum) — 路由 + 中间件         │
│  Auth │ Rate Limit │ CSP │ Validation │ Log   │
├────────────────┬─────────────────────────────┤
│  Core Services │  Domain Services            │
│  ┌──────────┐  │  ┌──────────┐               │
│  │ Engine   │  │  │ Chat     │               │
│  │ Memory   │  │  │ MultiAgent│              │
│  │ MCP      │  │  │ Skills   │               │
│  │ Tools    │  │  │ Knowledge│               │
│  │ Perms    │  │  │ Computer  │              │
│  └──────────┘  │  │ Remotion  │              │
│                │  │ IM       │               │
│                │  └──────────┘               │
├────────────────┴─────────────────────────────┤
│  Infrastructure                              │
│  DB │ Config │ Cache │ FS │ Process │ Terminal│
└──────────────────────────────────────────────┘
```

### 2.3 新模块结构

```
src-tauri/src/
├── api/                        # API 层
│   ├── mod.rs                  # Router 组装
│   ├── state.rs                # 结构化 AppState
│   ├── error.rs                # 统一错误类型
│   ├── middleware/
│   │   ├── mod.rs
│   │   ├── auth.rs             # API Key 验证
│   │   ├── rate_limit.rs       # 限流
│   │   ├── request_id.rs       # 请求追踪
│   │   └── audit.rs            # 审计日志
│   └── routes/                 # 按领域分路由文件
│       ├── chat.rs
│       ├── config.rs
│       ├── conversations.rs
│       ├── filesystem.rs       # 带路径校验
│       ├── git.rs
│       ├── mcp.rs
│       ├── memory.rs
│       ├── multiagent.rs
│       ├── tools.rs
│       ├── terminal.rs
│       ├── process.rs          # 带命令白名单
│       ├── skills.rs
│       ├── analytics.rs
│       ├── updater.rs
│       ├── worktree.rs
│       ├── preview.rs
│       ├── knowledge.rs
│       ├── computer_use.rs
│       └── im.rs
├── core/                       # 核心引擎
│   ├── mod.rs
│   ├── engine/                 # LLM 引擎 (原 native_engine)
│   │   ├── mod.rs
│   │   ├── engine_core.rs
│   │   ├── tool_loop.rs
│   │   ├── provider_manager.rs
│   │   └── session_manager.rs
│   ├── memory/                 # 记忆系统
│   │   ├── mod.rs
│   │   ├── memex_client.rs
│   │   ├── caveman_rtk.rs
│   │   ├── tiered_compressor.rs
│   │   └── context_manager.rs
│   ├── mcp/                    # MCP 服务器
│   │   ├── mod.rs
│   │   ├── server_manager.rs
│   │   └── protocol.rs
│   ├── tools/                  # Tool 系统
│   │   ├── mod.rs
│   │   ├── definitions.rs
│   │   └── executor.rs
│   └── permissions/            # 权限系统
│       ├── mod.rs
│       ├── manager.rs
│       └── audit.rs
├── domain/                     # 领域服务
│   ├── mod.rs
│   ├── chat.rs
│   ├── knowledge.rs
│   ├── multiagent.rs
│   ├── skills.rs
│   ├── computer_use.rs
│   ├── remotion.rs
│   ├── im/                    # 即时通讯集成
│   │   ├── mod.rs
│   │   ├── feishu.rs
│   │   └── message_router.rs
│   └── worktree.rs
├── infra/                      # 基础设施
│   ├── mod.rs
│   ├── db/
│   │   ├── mod.rs
│   │   ├── pool.rs
│   │   ├── migration.rs
│   │   └── repos/             # 每个实体的 repository 
│   │       ├── conversation_repo.rs
│   │       ├── message_repo.rs
│   │       ├── project_repo.rs
│   │       └── ...
│   ├── config/
│   │   ├── mod.rs
│   │   ├── app_config.rs
│   │   └── endpoints.rs
│   ├── cache.rs
│   ├── fs.rs                   # 带路径校验的安全文件操作
│   ├── process.rs              # 带白名单的进程管理
│   ├── terminal.rs
│   ├── streaming.rs
│   └── updater.rs
├── lib.rs                      # 只声明 pub mod，无 #![allow]
└── main.rs                     # 仅应用启动，无 #![allow]
```

---

## 3. 各模块具体升级方案

### 3.1 安全加固（最高优先级）

| 措施 | 文件 | 实现方式 |
|------|------|---------|
| **修 CSP** | `tauri.conf.json` | 移除 `unsafe-inline`/`unsafe-eval`，改用 nonce 或 hash。严格限制 `connect-src` 只到必要的 API 域名 |
| **文件路径校验** | `infra/fs.rs` | 所有文件操作前校验：路径 canonicalize 后必须在允许的根目录下。拒绝 `..`、符号链接跳转、绝对路径滥用以 root 启动的场景 |
| **进程白名单** | `domain/process.rs` | 定义 `ALLOWED_COMMANDS: &[&str]`，只允许 `git`、`node`、`npm`、`npx`、`python`、`rustc`、`cargo` 等已知安全命令 |
| **API Key 迁移** | 前端 + `secure_storage.rs` | 所有 API Key 使用 Tauri secure-storage plugin 存储。前端只通过 IPC 读取。移除 localStorage 存储 |
| **移除全局 allow** | `main.rs` | 删除所有 `#![allow(...)]`，修复真实的编译器警告。使用 `#[allow(...)]` 在少数合理的局部作用域 |
| **输入验证** | `api/middleware/` | 添加请求体大小限制、JSON 深度限制、内容类型验证 |

### 3.2 架构重构

#### 3.2.1 AppState 结构化

将 24 元组替换为命名字段 struct：

```rust
// api/state.rs
pub struct AppState {
    pub engine_pool: EnginePool,
    pub mcp_manager: Arc<McpServerManager>,
    pub stream_manager: Arc<StreamManager>,
    pub db_manager: Arc<DbManager>,
    pub config_manager: Arc<ConfigManager>,
    pub skills_manager: Arc<SkillsManager>,
    pub memex_client: Arc<MemExClient>,
    pub cost_tracker: Arc<CostTracker>,
    pub preview_engine: Arc<PreviewEngine>,
    pub analytics_store: Arc<AnalyticsStore>,
    pub context_manager: Arc<ContextManager>,
    pub permission_manager: Arc<PermissionManager>,
    pub process_manager: Arc<ProcessManager>,
    pub terminal_manager: Arc<PtyManager>,
    pub git_integration: Arc<GitIntegration>,
    pub file_watcher: Arc<FileWatcher>,
    pub clipboard_manager: Arc<ClipboardManager>,
    pub notification_manager: Arc<NotificationManager>,
    pub logger: Arc<Logger>,
    pub orchestrator: Arc<Mutex<Option<MultiAgentOrchestrator>>>,
    pub rate_limiter: Arc<RateLimiter>,
    pub worktree_manager: Arc<Mutex<Option<WorktreeManager>>>,
    pub ide_bridge: Arc<Mutex<Option<IdeBridge>>>,
}
```

**效果**：`state.6` → `state.db_manager` ，代码可读性大幅提升。

#### 3.2.2 bridge/mod.rs 拆分

当前 2935 行 → 按领域拆分为 35+ 文件：

- 每个 `api/routes/*.rs` 文件包含对应的 handler 函数
- 每个文件 < 200 行（4-8 个 handler 每个约 20-40 行）
- 所有 handler 通过 `api/routes/mod.rs` 统一注册到 router

#### 3.2.3 目录树递归修复

```
infra/fs.rs:
fn build_tree(dir_path: &str, max_depth: u32) -> Result<Value, FsError>
```
- 添加 `max_depth` 参数（默认 10）
- 维护 `visited: HashSet<PathBuf>` 检测符号链接循环
- 超过深度限制直接返回 `[TRUNCATED]` 标记

### 3.3 Mutex 优化

| 场景 | 当前 | 优化后 |
|------|------|--------|
| 短时同步锁（配置读取、状态检查） | `tokio::sync::Mutex` | `std::sync::Mutex` 或 `parking_lot::Mutex` |
| 跨 await 点持有锁（流式写入） | `tokio::sync::Mutex` | 保留 `tokio::sync::Mutex` |
| 读多写少（配置、缓存） | `Mutex` | `tokio::sync::RwLock` 或 `std::sync::RwLock` |
| 无竞争原子操作（计数器、指标） | `Mutex` | `AtomicU64` / `AtomicBool` |

### 3.4 错误处理统一

当前问题：多种错误类型混用（`anyhow`、`String`、`StatusCode` 直接返回），部分模块静默吞错误。

统一方案：
```rust
// api/error.rs
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Not found: {0}")]        NotFound(String),
    #[error("Bad request: {0}")]      BadRequest(String),
    #[error("Unauthorized")]          Unauthorized,
    #[error("Internal: {0}")]         Internal(String),
    #[error("Rate limited")]          RateLimited,
}

impl IntoResponse for ApiError { ... }
```

所有 handler 使用 `Result<impl IntoResponse, ApiError>` 模式。

### 3.5 前端重构

| 问题 | 方案 |
|------|------|
| App.tsx >700 行 | 提取 Layout、ChatHeader、Tooltip、AnnouncementModal 为独立组件文件 |
| 30+ useState 混乱 | 用 Zustand store 拆分：`useChatStore`、`useUIStore`、`useSettingsStore` |
| 散落 useEffect | 提取自定义 hooks：`useZoom`、`useAnnouncements`、`useAuth`、`useNavigationHistory` |
| API Key 存 localStorage | 迁移到 Tauri secure-storage，通过 IPC 读取 |
| inline API 调用 | 统一 `api.ts`，添加请求/响应拦截器、错误处理、重试逻辑 |

### 3.6 测试策略

| 层级 | 工具 | 覆盖目标 | 最低覆盖率目标 |
|------|------|---------|-------------|
| Rust 单元测试 | `#[cfg(test)]` | core/ 和 infra/ 模块 | 核心模块 >60% |
| Rust 集成测试 | `axum-test` | api/routes/ 所有 endpoint | >80% 端点 |
| Rust 属性测试 | `proptest` | FS 路径校验、序列化边界 | 安全关键函数 |
| 前端单元测试 | vitest + testing-library | components/ 和 stores/ | 关键组件 >50% |
| E2E | Tauri 驱动测试 | 核心用户流程 | 3-5 个关键流程 |

关键测试点：
- 文件系统路径穿越防护（10+ 边界用例）
- Auth 中间件（缺 key、错 key、过期 key）
- SSE 流式超时和断线重连
- Mutex 死锁预防

---

## 4. 执行计划

### Phase 1: 安全加固（Day 1-2）
1. 修 CSP 配置
2. 添加 FS 路径校验层
3. 添加进程白名单
4. 移除全局 `#![allow]`，修复所有编译器警告
5. API Key 迁移到 secure-storage

### Phase 2: 架构重构（Day 3-5）
1. 创建新的模块目录结构
2. 实现结构化 AppState
3. 拆分 bridge/mod.rs → api/routes/
4. 统一错误类型
5. 优化 Mutex 使用

### Phase 3: 前端重构（Day 6-7）
1. 提取 Zustand stores
2. 拆分大型组件
3. 提取自定义 hooks
4. 统一 API 客户端

### Phase 4: 测试覆盖（Day 8-10）
1. 核心模块 Rust 单元测试
2. API 端点集成测试
3. 安全边界属性测试
4. 前端组件测试

### Phase 5: 清理与文档（Day 11-12）
1. 清理冗余模块和代码
2. 重写 README（诚实版本）
3. 删除无用文件（`$null`、`-p` 目录）
4. 统一版本号

---

## 5. 保留模块说明

用户要求全保留 50+ 模块，但以下模块需要降级处理：

| 模块 | 处理方式 |
|------|---------|
| `remotion` | 标记为 EXPERIMENTAL，条件编译 `#[cfg(feature = "remotion")]` |
| `computer_use` | 加固安全边界（限制屏幕操作范围、添加确认） |
| `multiagent` + `orchestration` | 整合到 `domain/multiagent/` |
| `agent_bus` | 并入 `domain/multiagent/` |
| `app_studio` | 整理到 `domain/` |
| `im_integration` | 整理到 `domain/im/` |
| `native_engine` | 核心模块，重整到 `core/engine/` |

---

## 6. 预期成果

| 指标 | 当前 | 升级后 |
|------|------|--------|
| AppState 可读性 | `state.6` 无意义索引 | `state.db_manager` 自描述 |
| bridge/mod.rs | 2935 行单文件 | ~35 文件，每文件 <200 行 |
| App.tsx | 720+ 行 | <200 行（拆为 8+ 文件） |
| Rust 编译器警告 | 全部静默 | 0 warnings |
| 测试数 | 2 | >50 |
| CSP 有效性 | 无效 | 严格策略 + nonce |
| 文件操作安全 | 无防护 | 路径校验 + 白名单 |
| Mutex 使用 | 90% tokio::Mutex | 按场景最优选择 |
| 版本号 | 冲突（2.0/3.0） | 统一为 3.0.0 |
