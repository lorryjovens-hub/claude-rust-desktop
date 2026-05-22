# 生产级 Zustand + SQLite + 流式修复 + 多智能体 Spec

## Why
当前 Tauri 版本存在四大核心缺陷：(1) 前端 40+ useState 上帝组件导致流式对话时大量无效重渲染；(2) JSON 全量覆写存储在对话增长后 I/O 线性恶化且阻塞异步运行时；(3) ToolLoopExecutor 仅调用非流式 API，用户看不到逐字输出；(4) 多智能体编排器全部返回硬编码数据未接入 LLM。这些问题使得应用无法达到生产级可用标准。

## What Changes
- 引入 Zustand 替换 MainContent.tsx 的 40+ useState，按领域拆分为 6 个独立 Store
- 引入 rusqlite 替换双套 JSON 存储（ConversationStore + SessionManager），统一为单库多表结构
- 将 ToolLoopExecutor 从非流式切换为流式调用，集成 SSE 解析器
- 实现多智能体编排器的 LLM 接入和并行执行
- Bridge 启动和前端初始化增加详细日志
- **BREAKING**: 移除 SessionManager（Tauri commands 路径），统一使用 SQLite + Bridge HTTP API
- **BREAKING**: 移除 commands/mod.rs 中的 chat_send 独立调用路径

## Impact
- Affected specs: 前端状态管理、数据持久化、模型调用链路、多智能体编排
- Affected code: MainContent.tsx, App.tsx, api.ts, streamingState.ts, config/mod.rs, session_manager.rs, tool_loop.rs, anthropic_client.rs, openai_client.rs, multiagent/mod.rs, bridge/mod.rs, commands/mod.rs, engine_core.rs

## ADDED Requirements

### Requirement: Zustand 状态管理
系统 SHALL 使用 Zustand 作为全局状态管理方案，将 MainContent.tsx 的 40+ useState 拆分为 6 个领域 Store。

#### Scenario: 流式对话时仅更新消息 Store
- **WHEN** AI 模型返回流式 delta
- **THEN** 仅 useMessageStore 的订阅组件重渲染，其他 Store 的订阅组件不受影响

#### Scenario: 跨组件状态共享
- **WHEN** Sidebar 需要获取流式状态
- **THEN** 通过 useStreamingStore 直接读取，无需 window.CustomEvent

### Requirement: SQLite 数据持久化
系统 SHALL 使用 rusqlite 替换 JSON 文件存储，统一 ConversationStore 和 SessionManager 为单一数据库。

#### Scenario: 保存单条消息
- **WHEN** 流式对话完成一条助手消息
- **THEN** 仅执行 INSERT 语句，不触发全量读写

#### Scenario: 列出会话元数据
- **WHEN** 用户打开侧边栏
- **THEN** 通过索引查询 conversations 表，O(log n) 复杂度，不读取消息内容

#### Scenario: 数据迁移
- **WHEN** 首次启动新版本检测到旧 JSON 文件
- **THEN** 自动将 JSON 数据迁移到 SQLite，迁移完成后标记旧文件

### Requirement: 流式模型调用
系统 SHALL 在 ToolLoopExecutor 中使用流式 API 调用，实现逐字输出。

#### Scenario: Anthropic 流式对话
- **WHEN** 用户发送消息且 Provider 为 Anthropic 格式
- **THEN** ToolLoopExecutor 调用 send_message_stream()，逐 delta 发送 EngineEvent::Text

#### Scenario: OpenAI 流式对话
- **WHEN** 用户发送消息且 Provider 为 OpenAI 格式
- **THEN** ToolLoopExecutor 调用 send_message_stream()，逐 delta 发送 EngineEvent::Text

#### Scenario: 流式中断恢复
- **WHEN** 流式传输中断后重连
- **THEN** 通过历史事件回放补齐缺失内容

### Requirement: 多智能体编排 LLM 接入
系统 SHALL 将多智能体编排器的三个阶段接入 LLM，支持并行执行。

#### Scenario: 研究规划阶段
- **WHEN** 用户发起研究任务
- **THEN** Planner Agent 调用 LLM 生成 ResearchPlan（含子问题列表）

#### Scenario: 并行子研究
- **WHEN** ResearchPlan 生成完成
- **THEN** 最多 N 个 Researcher Agent 并行调用 LLM 执行子研究

#### Scenario: 报告综合
- **WHEN** 所有子研究完成
- **THEN** Writer Agent 调用 LLM 综合生成最终报告

#### Scenario: 前端触发
- **WHEN** 用户在 UI 中选择研究模式并发送查询
- **THEN** 前端通过 Bridge API 触发多智能体编排

### Requirement: 详细启动日志
系统 SHALL 在 Bridge 启动和前端初始化时输出详细日志。

#### Scenario: Bridge 端口绑定
- **WHEN** Bridge 尝试绑定端口
- **THEN** 输出尝试的端口号、绑定结果、最终使用的端口

#### Scenario: 前端端口检测
- **WHEN** 前端启动时检测 Bridge 端口
- **THEN** 输出检测过程、最终连接的端口、连接状态

## MODIFIED Requirements

### Requirement: 模型调用链路统一
移除 commands/mod.rs 中的 chat_send 独立路径，所有模型调用统一走 NativeEngine → ToolLoopExecutor 链路。chat_stream 从空壳改为调用 ToolLoopExecutor 的流式接口。

### Requirement: 会话存储统一
移除 SessionManager（Tauri commands 路径），所有会话操作统一通过 Bridge HTTP API 访问 SQLite 数据库。

## REMOVED Requirements

### Requirement: SessionManager JSON 存储
**Reason**: 被 SQLite 统一存储替代，双系统并存导致数据不一致
**Migration**: 首次启动时自动将 claude-desktop.json 迁移到 SQLite

### Requirement: window.CustomEvent 跨组件通信
**Reason**: 被 Zustand Store 的订阅机制替代，提供类型安全和精确更新
**Migration**: 所有 CustomEvent 替换为对应的 Zustand Store 订阅
