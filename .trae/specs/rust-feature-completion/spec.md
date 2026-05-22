# Rust 重构功能补全 Spec

## Why
当前 Tauri/Rust 重构版本的核心功能存在严重缺失：多智能体编排系统代码已完成但 Bridge 端点是 stub、任务执行系统绕过 ProviderManager 且事件被丢弃、Computer Use 全部是模拟实现、工具系统缺少 Git/Browser 工具、AskUserQuestion 无法暂停等待用户输入。这些缺失导致应用无法执行实际任务。

## What Changes
- **连接多智能体编排 Bridge 端点**：将 ResearchOrchestrator 和 MultiAgentOrchestrator 接入 Bridge API，替换 stub 实现
- **修复任务执行系统**：TaskExecutor 使用 ProviderManager 而非环境变量，初始化持久实例，支持流式事件
- **实现真实 Computer Use**：使用 enigo crate 实现 OS 级输入模拟，使用 screenshots crate 实现屏幕截图，注册为工具
- **补全工具系统**：添加 Git 工具、Browser 工具（Puppeteer MCP），修复 AskUserQuestion 暂停/恢复机制
- **修复前端对话显示**：确保 sendMessage 路径正确调用 Bridge API，SSE 事件正确渲染

## Impact
- Affected code: bridge/mod.rs, multiagent/mod.rs, research/mod.rs, task/mod.rs, computer_use/mod.rs, tools/mod.rs, tool_loop.rs, api.ts, MainContent.tsx

## ADDED Requirements

### Requirement: 多智能体编排 Bridge 端点
系统 SHALL 通过 Bridge API 暴露多智能体研究功能。

#### Scenario: 启动研究任务
- **WHEN** 前端发送 POST /api/research/start 请求
- **THEN** Bridge 创建 ResearchOrchestrator 实例，使用 ProviderManager 解析模型，启动研究管道，返回 research_id 和 SSE 事件流

#### Scenario: 研究子智能体并行执行
- **WHEN** ResearchOrchestrator 执行子研究任务
- **THEN** 使用 Semaphore 控制并发数，每个子智能体独立调用 LLM API，事件通过 broadcast channel 推送到前端

#### Scenario: 停止研究任务
- **WHEN** 前端发送 POST /api/research/{id}/stop
- **THEN** 取消所有正在运行的子智能体任务，清理资源

### Requirement: 任务执行系统集成
系统 SHALL 提供完整的任务执行功能，集成 ProviderManager 和工具系统。

#### Scenario: 创建并执行任务
- **WHEN** 前端发送 POST /api/tasks 请求
- **THEN** TaskExecutor 使用 ProviderManager 解析模型，执行带工具支持的 LLM 调用，流式推送事件

#### Scenario: 任务状态查询
- **WHEN** 前端发送 GET /api/tasks/{id}/status
- **THEN** 返回任务当前状态（Pending/Running/Completed/Failed/Cancelled）

### Requirement: Computer Use 真实实现
系统 SHALL 提供真实的 OS 级计算机控制能力。

#### Scenario: 鼠标操作
- **WHEN** LLM 调用 computer_use 工具执行鼠标移动/点击
- **THEN** 使用 enigo crate 在真实操作系统上执行鼠标操作

#### Scenario: 键盘输入
- **WHEN** LLM 调用 computer_use 工具执行键盘输入
- **THEN** 使用 enigo crate 在真实操作系统上模拟键盘输入

#### Scenario: 屏幕截图
- **WHEN** LLM 调用 computer_use 工具请求截图
- **THEN** 使用 screenshots crate 捕获屏幕图像，返回 base64 编码

#### Scenario: 作为工具注册
- **WHEN** ToolLoopExecutor 初始化工具列表
- **THEN** computer_use 工具出现在可用工具列表中，LLM 可以自主调用

### Requirement: 工具系统补全
系统 SHALL 提供完整的工具集供 LLM 使用。

#### Scenario: Git 工具
- **WHEN** LLM 需要执行 Git 操作
- **THEN** 提供 git_status、git_diff、git_log、git_commit 等工具

#### Scenario: AskUserQuestion 暂停/恢复
- **WHEN** LLM 调用 AskUserQuestion 工具
- **THEN** 工具循环暂停，通过 SSE 发送 ask_user 事件，等待前端用户输入后恢复

### Requirement: 前端对话显示修复
系统 SHALL 确保流式对话正确显示。

#### Scenario: 发送消息并收到流式回复
- **WHEN** 用户在前端输入消息并发送
- **THEN** 前端调用 Bridge API，SSE 事件流正确解析，AI 回复逐字显示

## MODIFIED Requirements

### Requirement: Provider 配置同步
ConfigManager 和 NativeEngine 的 ProviderManager SHALL 保持同步。每次 chat 请求前从 ConfigManager 同步最新 provider 配置到 NativeEngine。

### Requirement: TaskExecutor 使用 ProviderManager
TaskExecutor SHALL 使用 BridgeServer 的 ProviderManager 而非环境变量来解析模型和 API 密钥。
