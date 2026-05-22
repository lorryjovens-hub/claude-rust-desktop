# 修复前端流式显示和工具调用 Spec

## Why
前端流式对话中文显示可能乱码、工具调用结果不可见、模型选择器不显示自托管模型，导致用户无法正常使用对话和工具功能。

## What Changes
- **修复 SSE 响应 Content-Type**：添加 `charset=utf-8` 防止中文乱码
- **修复工具调用事件映射**：`tool_use_done` 的 `output` 字段映射到前端的 `content`
- **补全 sendMessageNative 工具事件处理**：添加 `tool_use_start`/`tool_use_done`/`tool_arg_delta` 事件处理
- **统一 SSE data 前缀解析**：兼容 `data:` 和 `data: ` 两种格式
- **修复模型选择器**：调用 `/api/providers/models` 填充自托管模型列表
- **添加 ToolUseStart 的 textBefore 字段**：确保工具调用前的文本正确关联

## Impact
- Affected code: bridge/mod.rs, api.ts, MainContent.tsx, tool_loop.rs

## ADDED Requirements

### Requirement: SSE 响应正确编码
系统 SHALL 在所有 SSE 响应中包含 `charset=utf-8`。

#### Scenario: 中文流式回复
- **WHEN** 用户发送中文消息并收到流式回复
- **THEN** 前端正确显示中文字符，无乱码

### Requirement: 工具调用结果可见
系统 SHALL 正确传递工具调用结果到前端。

#### Scenario: 工具执行完成
- **WHEN** LLM 调用工具并收到结果
- **THEN** 前端显示工具名称、输入参数和执行结果

### Requirement: 自托管模型出现在选择器中
系统 SHALL 在模型选择器中显示所有已配置的 provider 模型。

#### Scenario: 用户添加了 DeepSeek provider
- **WHEN** 用户在前端配置了 DeepSeek provider
- **THEN** 模型选择器中出现 DeepSeek 的模型选项

## MODIFIED Requirements

### Requirement: SSE data 前缀解析
前端 SHALL 兼容 `data:` 和 `data: ` 两种 SSE 数据行格式，使用 `line.slice(5).trim()` 统一处理。
