# Tasks

- [ ] Task 1: 修复后端 SSE 响应编码
  - [ ] 1.1: 在 bridge/mod.rs 的 chat_handler 中为 SSE 响应添加 charset=utf-8 Content-Type
  - [ ] 1.2: 在 research_events_handler 中同样添加 charset=utf-8
  - [ ] 1.3: 在 tool_loop.rs 的 ToolUseStart 事件中添加 textBefore 字段

- [ ] Task 2: 修复前端 SSE 解析和工具事件
  - [ ] 2.1: 统一 api.ts 中所有 SSE data 前缀解析为兼容 `data:` 和 `data: ` 格式
  - [ ] 2.2: 修复 tool_use_done 事件中 output → content 字段映射
  - [ ] 2.3: 在 sendMessage 中添加 tool_use_start/tool_use_done/tool_arg_delta 事件处理
  - [ ] 2.4: 在 sendMessageNative 中添加 tool_use_start/tool_use_done/tool_arg_delta 事件处理

- [ ] Task 3: 修复模型选择器
  - [ ] 3.1: 在 MainContent.tsx 或 useChatStore 中调用 /api/providers/models 填充模型列表
  - [ ] 3.2: 确保自托管 provider 的模型出现在选择器中

- [ ] Task 4: 启动应用并端到端验证
  - [ ] 4.1: 编译后端 cargo check
  - [ ] 4.2: 启动应用验证中文流式对话
  - [ ] 4.3: 验证工具调用显示
  - [ ] 4.4: 验证模型选择器
  - [ ] 4.5: 检查后端日志确认请求处理

# Task Dependencies
- Task 2 depends on Task 1 (需要 textBefore 字段)
- Task 4 depends on Task 1, 2, 3
