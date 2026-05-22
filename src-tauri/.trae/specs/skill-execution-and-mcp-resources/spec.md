# Skills 执行引擎与 MCP 资源功能完善 - PRD

## Overview
- **Summary**: 补全 Skills 系统的执行引擎，实现技能上下文注入和工具调用能力；完善 MCP 系统的资源读取和监控功能。
- **Purpose**: 解决当前技能执行仅返回格式化文本的问题，让技能能够真正接收对话上下文并调用工具执行；实现 MCP 协议的完整资源管理能力。
- **Target Users**: AI 助手用户，需要使用技能系统和 MCP 连接器的用户

## Goals
- 实现 Skills 系统的完整执行引擎，支持上下文注入和工具调用
- 实现 MCP 资源读取 (`resources/read`) 功能
- 实现 MCP 资源监控 (`resources/monitor`) 功能
- 提升模型响应速度，减少无效信息返回

## Non-Goals (Out of Scope)
- 技能发现和推荐系统
- 技能验证和评估框架
- 技能市场集成
- MCP 采样能力和提示模板（后续迭代）

## Background & Context
- 当前 SkillsManager 的 `execute_skill()` 方法仅返回格式化文本，不具备实际执行能力
- MCP 系统缺少资源相关端点的实现
- 用户反馈模型回复慢且返回无效信息

## Functional Requirements

### FR-1: Skills 执行引擎
- **FR-1.1**: 技能执行时能够接收对话上下文（messages、conversation_id、tool_list）
- **FR-1.2**: 技能能够调用可用工具执行具体操作
- **FR-1.3**: 技能执行结果能够正确返回给调用者

### FR-2: MCP 资源读取
- **FR-2.1**: 实现 `resources/read` 端点，支持读取 MCP 服务器提供的资源内容
- **FR-2.2**: 支持指定资源 URI 和读取选项（如 offset、limit）

### FR-3: MCP 资源监控
- **FR-3.1**: 实现 `resources/monitor` 端点，支持订阅资源变更事件
- **FR-3.2**: 支持资源变更通知机制

## Non-Functional Requirements
- **NFR-1**: 技能执行响应时间 < 100ms（不含工具执行时间）
- **NFR-2**: MCP 资源操作超时时间 < 30s
- **NFR-3**: 支持并发技能执行和资源操作

## Constraints
- **Technical**: Rust + Tauri 2 + Axum HTTP Server
- **Architecture**: Tauri 原生壳 + 内嵌 Axum HTTP bridge
- **Database**: SQLite (rusqlite)

## Assumptions
- MCP 服务器已正确实现 resources/read 和 resources/monitor 端点
- 技能内容包含有效的 YAML frontmatter 和执行指令

## Acceptance Criteria

### AC-1: 技能执行引擎接收上下文
- **Given**: 技能执行请求包含对话上下文
- **When**: 调用 `execute_skill()` 方法
- **Then**: 技能能够访问上下文信息（messages、conversation_id、工具列表）
- **Verification**: `programmatic`

### AC-2: 技能调用工具执行
- **Given**: 技能内容包含工具调用指令
- **When**: 执行技能
- **Then**: 技能能够调用指定工具并获取执行结果
- **Verification**: `programmatic`

### AC-3: MCP resources/read 端点
- **Given**: MCP 服务器提供可读取的资源
- **When**: 调用 `resources/read` 方法
- **Then**: 返回资源内容和元数据
- **Verification**: `programmatic`

### AC-4: MCP resources/monitor 端点
- **Given**: 订阅资源监控
- **When**: 资源发生变更
- **Then**: 客户端收到变更通知
- **Verification**: `programmatic`

## Open Questions
- [ ] 是否需要支持技能执行的异步模式？
- [ ] 资源监控是否需要持久化订阅状态？