# Tasks

- [x] Task 1: 引入 Zustand 并创建 6 个领域 Store
  - [x] 1.1: 安装 zustand 依赖 (`npm install zustand`)
  - [x] 1.2: 创建 `src/stores/useChatStore.ts` — 对话核心状态
  - [x] 1.3: 创建 `src/stores/useStreamingStore.ts` — 流式状态
  - [x] 1.4: 创建 `src/stores/useUIStore.ts` — UI 交互状态
  - [x] 1.5: 创建 `src/stores/useAuthStore.ts` — 认证状态
  - [x] 1.6: 创建 `src/stores/useProjectStore.ts` — 项目和技能状态
  - [x] 1.7: 创建 `src/stores/useToolStore.ts` — 工具和任务状态

- [x] Task 2: 重构 MainContent.tsx 使用 Zustand Store
  - [x] 2.1: 将 40+ useState 替换为对应的 Zustand Store hook 调用
  - [x] 2.2: 移除 window.CustomEvent 事件总线，替换为 Zustand Store 订阅
  - [x] 2.3: 移除 streamingState.ts 模块级单例，替换为 useStreamingStore
  - [x] 2.4: 优化流式 delta 更新：使用 Zustand 的 `subscribeWithSelector` 中间件
  - [x] 2.5: 验证流式对话时组件重渲染次数减少 80%+

- [x] Task 3: 引入 rusqlite 并创建 SQLite 数据库层
  - [x] 3.1: 在 Cargo.toml 添加 `rusqlite = { version = "0.31", features = ["bundled"] }`
  - [x] 3.2: 创建 `src-tauri/src/db/mod.rs` — 数据库连接管理
  - [x] 3.3: 创建 `src-tauri/src/db/schema.rs` — 表结构定义
  - [x] 3.4: 创建 `src-tauri/src/db/conversation_repo.rs` — 会话 CRUD
  - [x] 3.5: 创建 `src-tauri/src/db/message_repo.rs` — 消息 CRUD
  - [x] 3.6: 创建 `src-tauri/src/db/project_repo.rs` — 项目和文件 CRUD
  - [x] 3.7: 创建 `src-tauri/src/db/migration.rs` — JSON → SQLite 自动迁移

- [x] Task 4: 统一存储层，替换 ConversationStore 和 SessionManager
  - [x] 4.1: 修改 bridge/mod.rs 中 18 处 ConversationStore 调用，替换为 SQLite repo 调用
  - [x] 4.2: 移除 SessionManager 及其在 engine_core.rs 和 commands/mod.rs 中的引用
  - [x] 4.3: 修改 commands/mod.rs 中的会话相关 Tauri 命令，改为调用 Bridge HTTP API
  - [x] 4.4: 将同步 std::fs 调用替换为 tokio::task::spawn_blocking 中的 rusqlite 操作
  - [x] 4.5: 验证 list_conversations 性能

- [x] Task 5: 修复流式模型调用 — ToolLoopExecutor 集成流式 API
  - [x] 5.1: 修改 execute_anthropic_loop 调用 send_message_stream() 替代 send_message()
  - [x] 5.2: 修改 execute_openai_loop 调用 send_message_stream() 替代 send_message()
  - [x] 5.3: 集成 sse_parser.rs 解析流式 SSE 事件
  - [x] 5.4: 实现 Anthropic 流式事件分发
  - [x] 5.5: 实现 OpenAI 流式事件分发
  - [x] 5.6: 连接 handle_streaming_tool_arg_delta 和 finalize_streaming_tool_args
  - [x] 5.7: 验证流式对话逐字输出正常工作

- [x] Task 6: 清理冗余调用路径
  - [x] 6.1: 移除 commands/mod.rs 中的 chat_send 独立路径
  - [x] 6.2: 将 chat_stream 从空壳改为调用 ToolLoopExecutor 流式接口
  - [x] 6.3: 提取 execute_anthropic_loop 和 execute_openai_loop 中重复的工具执行逻辑为公共方法

- [x] Task 7: 实现多智能体编排 LLM 接入
  - [x] 7.1: 实现 generate_research_plan 调用 LLM
  - [x] 7.2: 实现 execute_sub_researchers 并行执行
  - [x] 7.3: 实现 synthesize_report 调用 LLM 综合生成报告
  - [x] 7.4: 在 bridge/mod.rs 添加 `/api/multiagent/research` 路由
  - [x] 7.5: 在前端 api.ts 添加 multiagentResearch() 函数
  - [x] 7.6: 验证研究模式端到端工作流

- [x] Task 8: 增加详细启动日志
  - [x] 8.1: Bridge 启动时输出详细日志
  - [x] 8.2: 前端初始化时输出详细日志
  - [x] 8.3: 流式对话时输出详细日志

# Task Dependencies
- [Task 2] depends on [Task 1] — 重构组件需要先创建 Store
- [Task 4] depends on [Task 3] — 替换存储层需要先创建 SQLite 层
- [Task 5] has no dependencies — 可与 Task 1-4 并行
- [Task 6] depends on [Task 5] — 清理路径需要先确保流式路径可用
- [Task 7] depends on [Task 5] — 多智能体需要流式 LLM 调用
- [Task 8] has no dependencies — 可与所有任务并行
