# Claude Desktop Tauri — 全面架构升级设计方案 v2.0

> 日期：2026-05-29
> 版本：v2.0（整合版）
> 状态：草案
> 前置文档：
> - `docs/superpowers/specs/2026-05-29-architecture-upgrade-design.md`
> - `D:\user\Documents\claude rust升级计划.txt`

---

## 核心哲学

> **"AI 客户端不是聊天窗口，是编译器级别的智能扩展。"**

目标是把 Claude Desktop 从一个功能原型级的聊天客户端，升级为**专业级 AI 编码 Agent 平台** — 像编译器一样常驻内存、增量更新、本地优先、快速反馈。云端大模型是"最后的手段"而非"默认选项"。

### 五维目标

```
        智能（好）
           △
          / \
         /   \
        /     \
    速度（快）——— 成本（省）
        |         |
        |         |
    多（场景覆盖）  稳（生产级质量）
```

| 维度 | 含义 | 核心指标 | 当前 | 目标 |
|------|------|---------|------|------|
| **快** | 响应延迟低、索引速度快 | 首 Token 延迟 (TTFB) | >2s | <200ms |
| **好** | 代码质量高、上下文理解准 | 用户撤销率 | >15% | <5% |
| **省** | Token 消耗少、资源占用低 | Token 成本 / 千次操作 | baseline | -80% |
| **多** | 多语言/多框架/多场景 | 支持语言数 | 1 (对话) | 20+ (编码) |
| **稳** | 生产级安全、可观测、可回滚 | 无警告构建、测试覆盖率 | 0% | >80% 端点 |

---

## 一、架构总览：六级加速引擎

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Claude Desktop (Tauri Edition)                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  Layer 0: 本地代码索引层（常驻内存）                          │   │
│  │  ┌──────────────┐ ┌────────────┐ ┌───────────────────────┐   │   │
│  │  │ HNSW 语义搜索 │ │ 符号表 AST  │ │ Tantivy 倒排索引      │   │   │
│  │  └──────────────┘ └────────────┘ └───────────────────────┘   │   │
│  │  ┌──────────────────────────────────────────────────────┐    │   │
│  │  │ 增量更新队列 <── File Watcher <── Git Diff <── 手动保存 │    │   │
│  │  └──────────────────────────────────────────────────────┘    │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  Layer 1: 边缘缓存层（本地 SQLite/RocksDB）                    │   │
│  │  ┌──────────────┐ ┌──────────────┐ ┌───────────────────────┐   │   │
│  │  │ 对话语义缓存   │ │ 工具调用缓存  │ │ 代码生成模板缓存      │   │   │
│  │  │ (99% 命中)    │ │ (LRU + TTL) │ │ (高频模式预编译)      │   │   │
│  │  └──────────────┘ └────────────┘ └───────────────────────┘   │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  Layer 2: 轻量本地模型层（端侧推理，candle/llama.cpp）         │   │
│  │  ┌──────────────┐ ┌──────────────┐ ┌───────────────────────┐   │   │
│  │  │ Qwen3-4B     │ │ Qwen3-8B     │ │ BGE-M3 Embedding      │   │   │
│  │  │ (60% 任务)   │ │ (30% 任务)   │ │ (代码向量化)           │   │   │
│  │  └──────────────┘ └────────────┘ └───────────────────────┘   │   │
│  │  投机解码草稿模型 + 意图分类器 + 代码质量验证器                 │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  Layer 3: API 网关层（Axum HTTP 桥 — 专业级加固）              │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────────────┐   │   │
│  │  │ Auth MW  │ │ Rate Limit│ │Req ID    │ │ Audit Log      │   │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └────────────────┘   │   │
│  │  ┌────────────────────────────────────────────────────────┐   │   │
│  │  │ 路由: Chat │ Config │ FS │ MCP │ Tools │ Terminal ...   │   │   │
│  │  └────────────────────────────────────────────────────────┘   │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  Layer 4: 领域服务层                                            │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────────────┐   │   │
│  │  │ 多智能体  │ │ MCP 协议  │ │ 技能系统  │ │ 即时通讯 (IM)  │   │   │
│  │  │ 编排      │ │ 服务器   │ │ Skills   │ │ Feishu/Wx     │   │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └────────────────┘   │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────────────┐   │   │
│  │  │ 知识库   │ │Computer  │ │ Remotion │ │ Worktree       │   │   │
│  │  │Knowledge │ │ Use      │ │ 视频渲染 │ │ 管理           │   │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └────────────────┘   │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  Layer 5: 基础设施层                                            │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────────────┐   │   │
│  │  │ SQLite    │ │ 安全 FS  │ │ 进程管理  │ │ LSP 客户端     │   │   │
│  │  │ + r2d2    │ │ 路径校验  │ │ 命令白名单 │ │ tower-lsp      │   │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └────────────────┘   │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────────────┐   │   │
│  │  │ PTY 终端  │ │ 文件监控  │ │ 配置管理  │ │ OpenTelemetry  │   │   │
│  │  │ xterm.js  │ │ Watcher  │ │ Config   │ │ 可观测性       │   │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └────────────────┘   │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  Layer 6: 云端大模型层（按需调用）                              │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────────────┐   │   │
│  │  │Anthropic │ │ OpenAI   │ │DeepSeek  │ │ 任意 OpenAI     │   │   │
│  │  │ Claude   │ │ GPT      │ │           │ │ 兼容端点       │   │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └────────────────┘   │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 二、"快"：毫秒级响应架构

### 2.1 本地代码索引（Layer 0）

问题：每次用户提问，Agent 都要重新扫描整个代码库找相关文件，延迟 2-5s。

方案：常驻内存的增量语义索引，Rust 实现、内存高效。

```rust
// 核心数据结构
pub struct CodeIndex {
    // 文件级 Embedding：快速定位相关文件（384 维，本地模型生成）
    file_embeddings: HNSWIndex<f32, 384>,
    
    // 符号表：类、函数、变量定义（tree-sitter 解析）
    symbol_table: DashMap<String, Vec<SymbolLocation>>,
    
    // 倒排索引：关键词 → 文件位置
    inverted_index: TantivyIndex,
    
    // 变更队列：Git diff / File watcher 增量更新，不阻塞查询
    pending_updates: mpsc::UnboundedSender<FileChange>,
}
```

| 操作 | 当前 | 优化后 | 关键手段 |
|------|------|--------|---------|
| 首次全量索引 | 无 | 30s（后台，不阻塞） | tree-sitter 增量解析 |
| 增量更新 | 无 | <50ms | File Watcher + Git diff hook |
| 查询相关文件 | 2-5s（重新扫描） | <10ms | HNSW 内存搜索 |
| 符号跳转 | 无 | <1ms | DashMap 直接查找 |

### 2.2 投机解码（Layer 2 → Layer 6 链路优化）

本地草稿模型（Qwen3-0.6B，4bit 量化，~400MB）预测接下来 N 个 token，云端并行验证：

```rust
struct DraftModel {
    model: LlamaModel,  // candle 或 llama.cpp 后端
    cache: KVCache,
}

struct CloudVerifier {
    client: LlmClient,
    // 一次验证 5-8 个 Token，延迟与验证 1 个相同
}

impl CloudVerifier {
    pub async fn verify_batch(&self, draft: Vec<Token>) -> VerificationResult { .. }
}
```

| 场景 | 当前 TTFB | 优化后 TTFB | 加速比 |
|------|----------|------------|-------|
| 简单代码补全 | 800ms | 150ms | 5.3x |
| 代码生成 | 3s | 1.2s | 2.5x |
| 跨文件重构 | 5s | 2.0s | 2.5x |

### 2.3 预加载工作区

启动时一次性分析项目结构，后续对话零延迟：

```rust
pub struct WorkspaceProfile {
    pub project_type: ProjectType,      // Rust? React? Python?
    pub dependency_graph: DepGraph,     // package.json / Cargo.toml 依赖
    pub style_profile: StyleProfile,    // 缩进、命名规范、常用模式
    pub system_prompt: PrecompiledPrompt, // 预编译提示词模板
}
```

---

## 三、"省"：Token 与资源的最优消耗

### 3.1 上下文压缩（三层裁剪）

```
原始上下文（15K Token）
    │
    ▼
[语法树裁剪] ──► 只保留相关函数定义 + 调用链（5K Token）
    │
    ▼
[语义重排序] ──► 按与当前任务相关性排序，尾部截断（3K Token）
    │
    ▼
[差异编码] ──► 只传"变更部分"的 diff，而非完整文件（1.5K Token）
```

```rust
pub struct ContextCompressor {
    pub fn compress(&self, files: Vec<FileContext>, task: &str) -> CompressedContext {
        let pruned = files.iter().map(|f| self.ast_prune(f, task)).collect();
        let ranked = self.semantic_rerank(pruned, task);
        let diff_encoded = self.to_diff_format(ranked);
        CompressedContext { tokens: diff_encoded.estimate_tokens(), .. }
    }
}
```

效果：Token 消耗降低 **60-80%**，质量不下降。

### 3.2 工具调用缓存

Agent 频繁执行 `git status`、`grep`、`find` 等命令：

| 命令 | TTL | 命中率 | 每次节省 |
|------|-----|--------|---------|
| `git status` | 2s | 95% | 200ms + API 成本 |
| `git log --oneline -10` | 30s | 90% | 500ms |
| `npm list --depth=0` | 60s | 85% | 3s |
| `find . -name "*.rs"` | 300s | 98% | 1s |
| `grep -r "struct Config"` | 10s | 80% | 2s |

实现：
```rust
pub struct ToolCache {
    command_cache: LruCache<CommandSignature, ToolOutput>,
    fs_snapshot: DashMap<PathBuf, (Metadata, Vec<u8>)>,
}
```

### 3.3 本地模型路由

```
                   ┌──────────────┐
                   │  意图分类器   │
                   │ (本地 4B，1ms) │
                   └──────┬───────┘
                          │
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
   ┌────────────┐  ┌────────────┐  ┌────────────┐
   │ 简单任务    │  │ 中等任务    │  │ 复杂任务    │
   │ 60% 任务   │  │ 30% 任务   │  │ 10% 任务   │
   │ Qwen3-4B   │  │ Qwen3-8B   │  │ Claude/GPT │
   │ 成本: 0    │  │ 成本: ~0   │  │ 成本: 100% │
   └────────────┘  └────────────┘  └────────────┘
```

**成本结构**：云端调用降至原成本的 **10%**，总 Token 成本降低 **80%**。

---

## 四、"好"：代码质量保障体系

### 4.1 多模型验证（Quality Gate）

对关键操作（删除文件、修改配置），用本地验证模型交叉检查：

```rust
pub struct QualityGate {
    primary: Box<dyn LlmClient>,     // 主模型生成
    verifier: Box<dyn LlmClient>,    // 验证模型检查
}

impl QualityGate {
    pub async fn generate_with_verify(&self, task: &str) -> VerifiedCode {
        let draft = self.primary.generate(task).await;
        let verification = self.verifier.check(&draft, &[
            "是否有语法错误？",
            "是否引入未定义变量？",
            "是否符合项目编码规范？",
        ]).await;
        // confidence > 0.9 → Accepted，否则 NeedsReview
    }
}
```

### 4.2 自动回滚与快照

每个 Agent 操作自动创建 Git 工作区快照，用户随时可撤销：

```rust
pub struct OperationLog {
    pub operations: Vec<Operation>,
    pub snapshots: Vec<GitSnapshot>,
}

impl OperationLog {
    pub async fn apply(&mut self, op: Operation) -> Result<()> {
        let snapshot = self.git.stash().await?;    // 操作前快照
        match op.execute().await {
            Ok(_) => { self.operations.push(op); self.snapshots.push(snapshot); Ok(()) }
            Err(e) => { self.git.apply_snapshot(snapshot).await?; Err(e) } // 自动回滚
        }
    }

    pub async fn undo(&mut self) -> Result<()> {  // 用户手动撤销
        if let Some(snapshot) = self.snapshots.pop() { self.git.apply_snapshot(snapshot).await }
    }
}
```

---

## 五、"多"：多语言/多框架/多场景

### 5.1 LSP 统一接入

利用 tree-sitter 解析（50+ 语言）+ LSP 协议：

```rust
pub struct LspManager {
    clients: DashMap<Language, LspClient>, // rust-analyzer, typescript-language-server, ...
}

impl LspManager {
    pub async fn get_symbols(&self, file: &Path) -> Vec<Symbol> {
        let lang = detect_language(file);
        let client = self.clients.entry(lang).or_insert_with(|| spawn_lsp_server(lang));
        client.document_symbols(file).await
    }
}
```

### 5.2 模板市场：社区驱动的能力扩展

```rust
pub struct AgentTemplate {
    pub name: String,                // "React Component with Tests"
    pub trigger: Regex,              // 匹配用户输入模式
    pub prompt_template: String,     // 预编译提示词
    pub post_actions: Vec<ToolCommand>, // 生成后自动执行操作
}
```

用户自定义流程示例：
```
输入："创建一个带权限控制的 API 路由"
    │
    ▼
匹配模板："protected-api-route"
    │
    ▼
执行：
1. 生成路由文件
2. 插入 auth middleware
3. 生成 Zod schema
4. 创建 Prisma migration
5. 生成单元测试
```

---

## 六、"稳"：生产级安全与质量

### 6.1 安全加固

| 措施 | 实现方式 | 优先级 |
|------|---------|--------|
| **CSP 强策略** | 移除 `unsafe-inline`/`unsafe-eval`，使用 nonce | P0 |
| **FS 路径校验** | canonicalize 后必须在允许根目录下 | P0 |
| **进程白名单** | 只允许 `git`、`node`、`npm`、`npx`、`python`、`cargo` 等 | P0 |
| **API Key 迁移** | 移除 localStorage，使用 Tauri secure-storage | P0 |
| **移除全局 allow** | 删除 `#![allow(...)]`，修复所有警告 | P0 |
| **请求体限制** | 限制 JSON 深度和大小 | P1 |
| **命令注入防护** | 所有 shell 命令使用参数数组而非字符串拼接 | P1 |

### 6.2 架构重构

| 问题 | 方案 |
|------|------|
| 24 元组 AppState | 命名 struct（`state.db_manager` 替代 `state.6`） |
| 2935 行 bridge/mod.rs | ~35 个路由文件，每文件 <200 行 |
| 全部 50+ 模块保留 | 重组成 `api/` → `core/` → `domain/` → `infra/` |
| 递归目录树 | 加 `max_depth` 和 `visited` 集合检测循环 |
| tokio::Mutex 滥用 | `std::sync::Mutex` / `RwLock` / `Atomic` 按场景选优 |

### 6.3 模块重组

```
src-tauri/src/
├── api/                         # API 层（路由 + 中间件）
│   ├── mod.rs                   # Router 组装
│   ├── state.rs                 # 结构化 AppState（命名而非元组）
│   ├── error.rs                 # 统一 `ApiError` + `IntoResponse`
│   └── middleware/
│       ├── auth.rs              # API Key 验证 + Bearer 解析
│       ├── rate_limit.rs        # token bucket
│       ├── request_id.rs        # 分布式追踪 ID
│       └── audit.rs             # 审计日志
│
├── routes/                      # 按领域分路由，每文件 < 200 行
│   ├── mod.rs                   # 统一注册
│   ├── chat.rs                  # SSE 流式 + H5 接入
│   ├── config.rs                # 配置读写
│   ├── filesystem.rs            # 路径校验安全版
│   ├── git.rs / mcp.rs/ memory.rs / tools.rs / ...
│   └── (35+ 文件)
│
├── core/                        # 核心引擎
│   ├── engine/                  # LLM 引擎（原 native_engine）
│   ├── memory/                  # 记忆系统 + CavemanRTK
│   ├── mcp/                     # MCP 服务器管理
│   ├── tools/                   # Tool 定义 + 执行
│   ├── permissions/             # 权限管理
│   ├── code_index/              # 🆕 Layer 0: 本地代码索引
│   │   ├── mod.rs
│   │   ├── hnsw_index.rs        # HNSW 语义搜索
│   │   ├── symbol_table.rs      # tree-sitter 符号表
│   │   ├── inverted_index.rs    # Tantivy 倒排索引
│   │   └── incremental.rs       # 增量更新
│   ├── model_router/            # 🆕 Layer 2: 本地模型路由
│   │   ├── mod.rs               # 意图分类 + 模型选择
│   │   ├── draft_model.rs       # 投机解码草稿模型
│   │   ├── local_inference.rs   # candle/llama.cpp 推理
│   │   └── embedding.rs         # BGE-M3 代码 Embedding
│   ├── cache/                   # 🆕 Layer 1: 缓存层
│   │   ├── mod.rs
│   │   ├── tool_cache.rs        # 工具调用缓存
│   │   ├── semantic_cache.rs    # 对话语义缓存
│   │   └── template_cache.rs    # 生成模板缓存
│   └── quality_gate/            # 🆕 质量门禁
│       ├── mod.rs
│       ├── verifier.rs          # 多模型验证
│       └── operation_log.rs     # 回滚与快照
│
├── domain/                      # 领域服务
│   ├── multiagent/              # 多智能体编排
│   ├── skills/                  # 技能系统 + 模板市场
│   ├── knowledge/               # 知识库
│   ├── computer_use.rs          # 屏幕控制（加固）
│   ├── remotion.rs              # 视频渲染（EXPERIMENTAL, cfg gate）
│   ├── im/                      # 即时通讯（Feishu 等）
│   ├── worktree.rs              # Git worktree 管理
│   └── ide.rs                   # IDE 桥接
│
├── infra/                       # 基础设施
│   ├── db/                      # SQLite + r2d2 + repos
│   ├── config/                  # 配置管理
│   ├── fs.rs                    # 安全 FS（路径校验 + 遍历保护）
│   ├── process.rs               # 带白名单的进程管理
│   ├── terminal.rs              # PTY + xterm.js
│   ├── streaming.rs             # SSE 流式管理
│   ├── lsp.rs                   # 🆕 LSP 客户端（tower-lsp）
│   ├── updater.rs               # 自动更新
│   └── telemetry.rs             # 🆕 性能监控
│
├── lib.rs                       # 仅声明 pub mod，无 #![allow]
└── main.rs                      # 仅应用启动，无 #![allow]
```

### 6.4 测试策略

| 层级 | 工具 | 范围 | 目标覆盖率 |
|------|------|------|-----------|
| Rust 单元测试 | `#[cfg(test)]` | core/ + infra/ | >60% |
| Rust 集成测试 | `axum-test` | api/routes/ | >80% 端点 |
| 属性测试 | `proptest` | FS 路径校验、序列化 | 安全函数 100% |
| 前端测试 | vitest + testing-library | components/ + stores/ | >50% |
| E2E | Tauri driver | 核心用户流程 | 3-5 流程 |

### 6.5 性能监控（可选关闭，尊重隐私）

| 指标 | 目标 | 优化手段 |
|------|------|---------|
| TTFB | <200ms | 本地索引 + 预加载 + 投机解码 |
| 端到端代码生成 | <3s | 上下文压缩 + 模型路由 |
| Token 成本/千次 | 降低 80% | 本地模型处理 90% 任务 |
| 本地内存占用 | <2GB | 4bit 量化 + 按需加载 |
| 索引更新延迟 | <100ms | 增量更新 + 异步持久化 |
| 用户撤销率 | <5% | 多模型验证 + 质量门禁 |

---

## 七、技术栈总表

| 模块 | 选型 | 理由 |
|------|------|------|
| 桌面框架 | **Tauri 2.0 + Axum 0.8** | 跨平台，内存安全，SSE 原生支持 |
| 通信 | Axum HTTP 桥（专业级加固） | 流式最佳实践，迁移 Tauri IPC 收益为零 |
| 数据库 | **SQLite + r2d2** | 嵌入式，零运维 |
| 缓存 | **SQLite + LRU DashMap** | 持久化 + 内存两级缓存 |
| 本地模型 | **candle (HuggingFace)** | Rust 原生，无需 Python 运行时 |
| 向量索引 | **hnsw-rs** | 内存高效，增量更新 |
| 代码解析 | **tree-sitter** | 50+ 语言，增量解析 |
| LSP 客户端 | **tower-lsp** | Rust 原生 LSP 实现 |
| 前端 UI | **React 19 + Tailwind** | 用户已熟悉 |
| 前端状态 | **Zustand 5** | 简洁，TS 友好 |
| 构建 | **Vite 6** | 极速 HMR |
| 可观测 | **OpenTelemetry + Prometheus** | 已有基础，完善 |
| 流式 | **Axum SSE** | 成熟稳定 |
| 遥测 | **可选关闭** | 尊重隐私 |

---

## 八、实施路线图（10~12 周）

### Phase 1（Week 1-2）：安全加固 + 现有代码修整
| ID | 任务 | 估算 |
|----|------|------|
| P1.1 | 修 CSP，移除 `unsafe-inline`/`unsafe-eval`，启用 nonce | 0.5d |
| P1.2 | 替换整个文件操作层：添加路径校验 + 遍历深度限制 | 1d |
| P1.3 | 进程管理添加白名单 | 0.5d |
| P1.4 | 删除全局 `#![allow(...)]`，修复所有编译器警告 | 1.5d |
| P1.5 | API Key 从 localStorage 迁移到 secure-storage | 1d |
| P1.6 | 添加请求体限制、JSON 深度校验 | 0.5d |
| P1.7 | 删除 `$null`、`-p/` 等垃圾文件，统一版本号 | 0.5d |
| | **小计** | **~5.5 天** |

### Phase 2（Week 2-3）：架构重构
| ID | 任务 | 估算 |
|----|------|------|
| P2.1 | 创建新目录结构 | 0.5d |
| P2.2 | 实现结构化 AppState（命名 struct） | 1d |
| P2.3 | 拆分 bridge/mod.rs 到 routes/ | 3d |
| P2.4 | 统一错误类型 `ApiError` | 1d |
| P2.5 | Mutex 审计：按场景替换为 RwLock / Atomic / std::Mutex | 1d |
| P2.6 | 模块重组（core/ → domain/ → infra/） | 2d |
| | **小计** | **~8.5 天** |

### Phase 3（Week 3-4）：前端重构
| ID | 任务 | 估算 |
|----|------|------|
| P3.1 | App.tsx 拆分为 Layout / ChatHeader / Announcement 等独立组件 | 1d |
| P3.2 | 30+ useState 迁移到 Zustand stores | 1.5d |
| P3.3 | 提取 hooks：useZoom / useAuth / useAnnouncements / useNavHistory | 1d |
| P3.4 | 统一 api.ts + 错误处理 + 重试逻辑 | 1d |
| P3.5 | API Key 前端适配 secure-storage IPC | 0.5d |
| | **小计** | **~5 天** |

### Phase 4（Week 4-6）：智能层引擎
| ID | 任务 | 估算 |
|----|------|------|
| P4.1 | **Code Index**: tree-sitter + HNSW + Tantivy 索引引擎 | 3d |
| P4.2 | **增量更新**: File Watcher + Git diff → 索引同步 | 1.5d |
| P4.3 | **模型路由**: 意图分类 + Qwen3-4B/8B 接入 (candle) | 3d |
| P4.4 | **上下文压缩**: AST 裁剪 + 语义重排序 + Diff 编码 | 2d |
| P4.5 | **工具缓存**: LRU + TTL + 自动失效 | 1d |
| P4.6 | **语义缓存**: 历史对话 99% 命中 | 1.5d |
| P4.7 | **质量门禁**: 多模型验证 + QualityGate | 2d |
| P4.8 | **投机解码**: DraftModel + CloudVerifier | 3d |
| P4.9 | **操作日志 + 自动回滚**: OperationLog + GitSnapshot | 1.5d |
| | **小计** | **~18.5 天** |

### Phase 5（Week 6-7）：扩展生态
| ID | 任务 | 估算 |
|----|------|------|
| P5.1 | **LSP 客户端**: tower-lsp 集成 rust-analyzer / ts-server | 2d |
| P5.2 | **模板市场**: prompt templates + 社区分享机制 | 1.5d |
| P5.3 | 多语言 tree-sitter 解析（Python/Go/Java/C++ 支持） | 1d |
| P5.4 | 性能遥测仪表盘（Prometheus + Grafana） | 1.5d |
| | **小计** | **~6 天** |

### Phase 6（Week 7-8）：测试覆盖
| ID | 任务 | 估算 |
|----|------|------|
| P6.1 | 安全边界测试（路径穿越 10+ 用例、注入攻击 5+ 用例） | 1d |
| P6.2 | core/ 模块单元测试 | 2d |
| P6.3 | api/routes/ 集成测试（axum-test） | 2d |
| P6.4 | 前端组件测试 | 1d |
| P6.5 | E2E 测试（3-5 个核心流程） | 1d |
| | **小计** | **~7 天** |

### Phase 7（Week 8+）：文档与优化
| ID | 任务 | 估算 |
|----|------|------|
| P7.1 | 重写 README（诚实版本），删除夸大内容 | 0.5d |
| P7.2 | Rust API docs + 架构决策记录 | 1d |
| P7.3 | 性能基准测试 + 持续优化 | 2d |
| P7.4 | 用户体验打磨（错误提示、加载态、过渡动画） | 2d |
| | **小计** | **~5.5 天** |

### 甘特图

```
Phase 1: 安全加固     ████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   5.5d
Phase 2: 架构重构     ░░░░██████████████████░░░░░░░░░░░░░░░░░░░░   8.5d
Phase 3: 前端重构     ░░░░░░░░░░░░████████░░░░░░░░░░░░░░░░░░░░░░   5d
Phase 4: 智能层引擎   ░░░░░░░░░░░░░░░░░░█████████████████████████  18.5d
Phase 5: 扩展生态     ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░█░░████████░░   6d  (部分并行)
Phase 6: 测试覆盖     ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░████████████░░   7d  (与 P4/P5 并行)
Phase 7: 文档优化     ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░████   5.5d

总计约 10-12 周（含并行）
```

---

## 九、现有问题修复清单

以下是基于最初审查发现的所有问题，以及对应到实施路线图中的任务编号：

| # | 问题 | 严重程度 | 所在 Phase | 任务 |
|---|------|---------|-----------|------|
| 1 | CSP 无效 (`unsafe-inline` + `unsafe-eval`) | 🔴 | P1 | P1.1 |
| 2 | 文件系统 API 无路径穿越防护 | 🔴 | P1 | P1.2 |
| 3 | 任意进程启动接口 | 🔴 | P1 | P1.3 |
| 4 | API Key 存 localStorage | 🔴 | P1 + P3 | P1.5 + P3.5 |
| 5 | 整个 crate 级 `#![allow(...)]` | 🔴 | P1 | P1.4 |
| 6 | 24 元素元组 AppState | 🟠 | P2 | P2.2 |
| 7 | 2935 行单文件 bridge/mod.rs | 🟠 | P2 | P2.3 |
| 8 | 50+ 模块扁平分不清层次 | 🟠 | P2 | P2.6 |
| 9 | 递归目录树无循环检测 | 🟠 | P1 | P1.2 |
| 10 | tokio::sync::Mutex 滥用 | 🟠 | P2 | P2.5 |
| 11 | App.tsx >700 行，30+ useState | 🟠 | P3 | P3.1 + P3.2 |
| 12 | 多处静默吞掉错误 | 🟠 | P2 | P2.4 |
| 13 | 全局仅 2 个单元测试 | 🔴 | P6 | P6.1-P6.5 |
| 14 | README 严重夸大 | 🟠 | P7 | P7.1 |
| 15 | tauri-plugin-barcode-scanner 等无关依赖 | ⚪ | P1 | 随 P1.7 清理 |
| 16 | connect-src CSP 过于宽松 | 🟠 | P1 | P1.1 |
| 17 | 版本号冲突（2.0 vs 3.0） | ⚪ | P1 | P1.7 |
| 18 | `$null` 和 `-p/` 垃圾文件 | ⚪ | P1 | P1.7 |
| 19 | 缺乏工作区预加载 | 🟠 | P4 | P4.1 |
| 20 | 无 LSP 集成 | 🟠 | P5 | P5.1 |

---

## 十、关键决策记录

| 决策 | 选择 | 备选 | 理由 |
|------|------|------|------|
| 通信方式 | **Axum HTTP 桥（加固）** | Tauri IPC 迁移 | LLM 调用是秒级瓶颈，IPC 收益为零；SSE 成熟 |
| 本地模型 | **candle (Rust 原生)** | llama.cpp bindings | 无需 Python 运行时，类型安全 |
| 向量索引 | **hnsw-rs** | pgvector, Milvus | 内存级，无需外置服务 |
| 代码解析 | **tree-sitter** | 手写解析器 | 50+ 语言，增量解析，社区活跃 |
| 缓存存储 | **SQLite + LRU 内存** | Redis, Memcached | 零运维，嵌入式 |
| 前端状态 | **Zustand 5** | Redux, Jotai | 简洁，TS 友好，已有使用 |
| 遥测 | **可选关闭** | 强制 | 尊重隐私优先 |
| 模块可见性 | **remotion/computer_use 标记 EXPERIMENTAL** | 删除 | 全保留用户要求，但使用条件编译隔离风险 |
| 测试框架 | **axum-test + proptest** | cargo-nextest | 集成测试 + 属性测试组合 |
