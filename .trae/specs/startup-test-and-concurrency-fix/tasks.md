# Tasks

- [ ] Task 1: 修复前端 Store 并发安全问题
  - [ ] 1.1: useStreamingStore.ts — abort() 中清空 streamingIds，删除 streamingState.ts 双重状态源
  - [ ] 1.2: useAuthStore.ts — 删除所有 setter 内部的 persistAuth 调用，仅保留 subscribe 监听器作为唯一持久化入口
  - [ ] 1.3: useToolStore.ts — resetTool 使用工厂函数创建新实例，addTask 简化去重逻辑
  - [ ] 1.4: useChatStore.ts — 添加 appendDeltaThrottled 函数，使用 requestAnimationFrame 节流流式 delta 更新

- [ ] Task 2: 修复后端 SQLite 并发安全问题
  - [ ] 2.1: db/mod.rs — with_conn 返回 Result 而非 panic，Mutex lock 使用 map_err 处理 poison
  - [ ] 2.2: 搜索所有在 async 函数中直接调用 get_conn().lock().unwrap() 的位置，替换为 spawn_blocking + with_conn
  - [ ] 2.3: conversation_repo.rs — 在 bridge 层的复合写操作（conversation_update 等）中添加事务包装
  - [ ] 2.4: migration.rs — 迁移失败时不写 .migrated 标记文件，添加事务保护，修复 content 数组类型丢失

- [ ] Task 3: 修复流式调用和多智能体并发问题
  - [ ] 3.1: tool_loop.rs — 每次 iteration 开始时清理 streaming_tool_args，stream 错误时显式 drop，检查 event_tx.is_closed()
  - [ ] 3.2: multiagent/mod.rs — semaphore.acquire() 错误处理替代 unwrap()，join_all 处理 JoinError 而非静默丢弃，删除未使用的 event_rx 字段，统一使用 consume_sse_payloads

- [ ] Task 4: 增加关键路径日志
  - [ ] 4.1: Bridge 数据库初始化和迁移检测日志
  - [ ] 4.2: 前端 SSE 连接建立和断开日志
  - [ ] 4.3: 流式对话 delta 接收和工具执行日志

- [ ] Task 5: 启动应用并验证
  - [ ] 5.1: 编译 Rust 后端 (cargo check)
  - [ ] 5.2: 编译前端 (tsc --noEmit)
  - [ ] 5.3: 启动应用 (npm run tauri dev)
  - [ ] 5.4: 验证 Bridge 启动日志输出
  - [ ] 5.5: 验证流式对话功能
  - [ ] 5.6: 验证多智能体研究模式

# Task Dependencies
- [Task 5] depends on [Task 1, 2, 3, 4] — 测试前需先修复所有问题
- [Task 1, 2, 3, 4] 可并行执行
