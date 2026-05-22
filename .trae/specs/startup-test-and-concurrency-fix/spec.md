# 启动测试与并发安全修复 Spec

## Why
上一轮生产级重构已完成 Zustand/SQLite/流式/多智能体的实现，但存在 5 个高严重度并发/数据安全问题需要修复，且需要实际启动应用验证流式对话和多智能体是否正常工作。

## What Changes
- 修复 useStreamingStore abort() 未清理 streamingIds 的状态不一致
- 修复 useAuthStore 双重 persistAuth 竞态写入
- 修复 db/mod.rs async 函数中直接锁 std::sync::Mutex 阻塞 tokio 运行时
- 修复 migration.rs 迁移失败仍写标记文件 + 缺少事务保护 + content 数组丢失
- 修复 multiagent/mod.rs semaphore.unwrap() panic 风险 + join_all 静默丢弃错误
- 修复 tool_loop.rs streaming_tool_args 迭代间未清理
- 在 Bridge 启动和前端连接关键路径增加更详细的日志
- 启动应用并验证流式对话和多智能体并行执行

## Impact
- Affected specs: 前端状态管理、数据持久化、流式调用、多智能体编排
- Affected code: useStreamingStore.ts, useAuthStore.ts, db/mod.rs, migration.rs, tool_loop.rs, multiagent/mod.rs, bridge/mod.rs, api.ts

## ADDED Requirements

### Requirement: 并发安全修复
系统 SHALL 修复所有已识别的高严重度并发和数据安全问题。

#### Scenario: abort 清理流式状态
- **WHEN** 用户取消正在进行的流式对话
- **THEN** streamingIds 被正确清空，UI 不再显示流式动画

#### Scenario: 认证状态持久化无竞态
- **WHEN** 快速连续调用 setUser 和 setToken
- **THEN** localStorage 中写入的是最终一致的状态，无中间状态泄漏

#### Scenario: SQLite 操作不阻塞异步运行时
- **WHEN** Bridge 处理并发请求时执行 SQLite 查询
- **THEN** 所有 SQLite 操作通过 spawn_blocking 执行，不阻塞 tokio 工作线程

#### Scenario: 迁移失败可重试
- **WHEN** JSON→SQLite 迁移过程中发生错误
- **THEN** 不写入 .migrated 标记文件，下次启动可重试

#### Scenario: 多智能体并行无 panic
- **WHEN** Semaphore 被关闭或 task panic
- **THEN** 返回错误结果而非 panic

### Requirement: 启动测试验证
系统 SHALL 能正常启动并验证核心功能。

#### Scenario: 流式对话验证
- **WHEN** 用户发送消息
- **THEN** AI 回复逐字显示，工具调用正常执行

#### Scenario: 多智能体验证
- **WHEN** 用户发起研究模式查询
- **THEN** 多个 researcher 并行执行，最终生成综合报告

## MODIFIED Requirements

### Requirement: 启动日志增强
在 Bridge 启动的数据库初始化、迁移检测、前端 SSE 连接建立等关键节点增加详细日志。

## REMOVED Requirements
无
