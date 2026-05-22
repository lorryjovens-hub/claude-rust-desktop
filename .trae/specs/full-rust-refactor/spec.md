# Rust 完整重构 - 产品需求文档

## Overview
- **Summary**：当前 Tauri/Rust 版本已有基础架构，但核心引擎、权限管理、MCP、Skills 等功能不完整。本项目参考 `claude-code-rust-local` 的完整实现，进行全面的功能补全和架构优化。
- **Purpose**：解决前端无法正确接收流式回复、工具调用不可见、多智能体编排不完整、Computer Use 未集成到工具系统等问题。
- **Target Users**：需要完整 AI 助手功能的桌面应用用户

## Goals
1. **完善核心引擎**：实现类似 QueryEngine 的完整查询生命周期管理
2. **建立权限系统**：工具使用权限控制、用户确认机制、审计日志
3. **完善 MCP 系统**：完整支持 MCP 协议所有特性
4. **实现 Skills 引擎**：使技能系统可实际执行和集成
5. **工具系统增强**：添加验证、安全检查、进度报告等
6. **费用追踪完善**：完整的预算管理和使用量统计
7. **集成 Computer Use**：使计算机控制功能成为可调用的工具
8. **前端集成完善**：确保所有后端功能在前端正确显示和响应

## Non-Goals (Out of Scope)
- 不重写整个项目为全新架构（在现有基础上完善）
- 不引入新的技术栈依赖（保持 Rust + Tauri + SQLite）
- 不实现 CLI 模式（保持桌面应用 GUI）
- 不完整复制参考项目的所有功能（保持当前架构特色）
- 不实现 LSP 集成（IDE 集成暂不考虑）

## Background & Context
- **现有架构**：Rust 后端 + Tauri 桌面应用 + SQLite 数据库 + React 前端
- **参考项目**：`claude-code-rust-local` 有完整的功能实现，但为 CLI/Ink 架构
- **技术约束**：保持现有 Tauri 框架不变，充分利用已有模块
- **用户反馈**：前端返回成功但看不到回复内容、模型配置后无法使用、工具调用无反馈

## Functional Requirements
- **FR-1**：完整的查询引擎，支持多轮对话、状态管理和恢复
- **FR-2**：工具权限管理系统，支持危险操作警告、用户确认、审计日志
- **FR-3**：完整的 MCP 协议支持（资源、提示、采样等）
- **FR-4**：Skills 执行引擎，支持上下文注入和技能组合
- **FR-5**：工具输入验证和安全检查
- **FR-6**：费用追踪和预算管理
- **FR-7**：Computer Use 功能集成到工具系统
- **FR-8**：完善的错误处理和重试机制
- **FR-9**：完整的 SQLite 数据库架构（添加缺失的表）
- **FR-10**：前端 SSE 事件完整处理（确保所有类型都能正确显示）

## Non-Functional Requirements
- **NFR-1**：流式响应延迟 <500ms
- **NFR-2**：对话加载时间 <1s（100+ 消息）
- **NFR-3**：API 请求错误率 <1%
- **NFR-4**：数据库操作安全，无死锁
- **NFR-5**：前端响应流畅，无明显卡顿

## Constraints
- **Technical**：
  - 必须使用 Tauri v2 框架
  - 后端语言限制为 Rust
  - 数据库保持 SQLite + rusqlite
  - 前端框架保持 React + TypeScript + Vite
- **Business**：
  - 项目必须在现有代码基础上完善，不重写
  - 保持向下兼容性（现有配置不丢失）
  - 重构期间保持应用可运行状态
- **Dependencies**：
  - enigo（Computer Use）
  - screenshots（截图）
  - rusqlite（数据库）
  - reqwest（HTTP）
  - tokio（异步运行时）
  - 其他现有 Cargo 依赖

## Assumptions
- [Assumption 1] 前端 API 调用方式保持不变（主要向后兼容）
- [Assumption 2] 现有的 provider 配置和数据库架构可以复用
- [Assumption 3] Computer Use 功能作为独有特色保持
- [Assumption 4] 用户愿意逐步测试新功能（分阶段发布）

## Acceptance Criteria

### AC-1：流式对话正常显示
- **Given**：用户已配置模型并发送消息
- **When**：模型返回流式响应
- **Then**：
  - 前端逐字显示 AI 回复
  - 思考过程正确显示
  - 工具调用结果可见
  - 无乱码或格式问题
- **Verification**：`programmatic` + `human-judgment`
- **Notes**：同时验证 PowerShell 编码问题（使用 UTF-8 终端）

### AC-2：工具调用完整流程
- **Given**：用户发送需要工具的任务
- **When**：LLM 调用工具
- **Then**：
  - 工具执行过程正确显示
  - 工具输入可见
  - 工具输出可见
  - 危险操作有确认提示
  - 执行失败有错误提示
- **Verification**：`human-judgment`

### AC-3：Computer Use 工具可调用
- **Given**：用户发送包含计算机操作的任务
- **When**：LLM 调用 computer_use 工具
- **Then**：
  - 工具参数正确传递
  - 实际执行了对应操作（鼠标移动、点击、输入等）
  - 操作结果反馈给 LLM
  - 截图正确返回
- **Verification**：`human-judgment`

### AC-4：MCP 服务器连接和工具可用
- **Given**：用户配置了 MCP 服务器
- **When**：启动应用并尝试调用 MCP 工具
- **Then**：
  - MCP 服务器正确启动和连接
  - MCP 工具列表正确显示
  - MCP 工具可正确调用
  - 资源读取正常工作
- **Verification**：`programmatic` + `human-judgment`

### AC-5：Skills 执行正确
- **Given**：用户安装了技能并触发
- **When**：技能执行
- **Then**：
  - 技能上下文正确注入
  - 技能输出正确返回
  - 执行过程无错误
- **Verification**：`human-judgment`

### AC-6：费用追踪正常
- **Given**：用户进行对话和工具使用
- **When**：查看费用统计
- **Then**：
  - token 使用量正确统计
  - 费用计算准确
  - 历史记录完整
- **Verification**：`programmatic`

### AC-7：数据库完整性
- **Given**：应用关闭和重启
- **When**：重新打开
- **Then**：
  - 对话历史完整恢复
  - 配置不丢失
  - 无数据损坏
- **Verification**：`programmatic`

### AC-8：错误处理友好
- **Given**：API 请求失败或工具执行出错
- **When**：发生错误
- **Then**：
  - 错误信息清晰可读
  - 有重试机制
  - 用户知道如何处理
- **Verification**：`human-judgment`

## Open Questions
- [ ] 权限系统是否需要完整角色管理，还是仅简单确认？
- [ ] Computer Use 功能是否需要特殊的安全沙箱？
- [ ] 技能市场功能是否需要实现？
- [ ] 费用追踪是否需要导出报表功能？
- [ ] 是否需要实现跨设备同步？
