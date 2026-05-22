# Tasks

- [ ] Task 1: 连接多智能体编排 Bridge 端点
  - [ ] 1.1: 修改 research_start_handler 实例化 ResearchOrchestrator，使用 ProviderManager 解析模型
  - [ ] 1.2: 添加 /api/research/{id}/events SSE 端点，推送 OrchestratorEvent 到前端
  - [ ] 1.3: 修改 research_stop_handler 取消正在运行的研究任务
  - [ ] 1.4: 修改 research_status_handler 返回真实状态
  - [ ] 1.5: 在 BridgeServer 中添加 HashMap<String, JoinHandle> 跟踪活跃研究任务
  - [ ] 1.6: 添加 /api/multiagent/research 路由到 router

- [ ] Task 2: 修复任务执行系统
  - [ ] 2.1: 修改 TaskExecutor 使用 ProviderManager 而非环境变量
  - [ ] 2.2: 在 BridgeServer::new() 中初始化 TaskExecutor
  - [ ] 2.3: 修改 create_task_handler 使用持久 TaskExecutor 实例
  - [ ] 2.4: 添加任务流式事件推送（通过 SSE 或 WebSocket）
  - [ ] 2.5: 修复 task_status_handler 和 cancel_task_handler

- [ ] Task 3: 实现真实 Computer Use
  - [ ] 3.1: 添加 enigo 和 screenshots crate 依赖到 Cargo.toml
  - [ ] 3.2: 实现 execute_action_impl 使用 enigo 执行真实鼠标/键盘操作
  - [ ] 3.3: 实现 take_screenshot 使用 screenshots crate 捕获屏幕
  - [ ] 3.4: 在 tools/mod.rs 中注册 computer_use 工具定义
  - [ ] 3.5: 添加 /api/computer-use/* Bridge API 端点
  - [ ] 3.6: 在 ToolLoopExecutor 中集成 computer_use 工具

- [ ] Task 4: 补全工具系统
  - [ ] 4.1: 添加 Git 工具定义（git_status, git_diff, git_log, git_commit, git_add）
  - [ ] 4.2: 实现 Git 工具执行逻辑
  - [ ] 4.3: 修复 AskUserQuestion 暂停/恢复机制
  - [ ] 4.4: 在 ToolLoopExecutor 中实现 ask_user 暂停逻辑
  - [ ] 4.5: 添加 Browser 工具（基于 Puppeteer MCP 或简单 HTTP fetch）

- [ ] Task 5: 修复前端对话显示
  - [ ] 5.1: 确保 sendMessage 正确调用 Bridge API /api/chat
  - [ ] 5.2: 验证 SSE 事件流正确解析和渲染
  - [ ] 5.3: 修复模型选择器与后端 provider 的同步
  - [ ] 5.4: 确保研究模式 UI 连接到 Bridge API

- [ ] Task 6: 编译验证和端到端测试
  - [ ] 6.1: cargo check 确保编译通过
  - [ ] 6.2: 启动应用验证对话功能
  - [ ] 6.3: 验证多智能体研究功能
  - [ ] 6.4: 验证 Computer Use 功能
  - [ ] 6.5: 验证工具系统功能

# Task Dependencies
- Task 2 depends on Task 1 (TaskExecutor 需要 ProviderManager)
- Task 3 depends on Task 4 (Computer Use 需要注册为工具)
- Task 5 depends on Task 1, Task 2 (前端需要后端端点就绪)
- Task 6 depends on all previous tasks
