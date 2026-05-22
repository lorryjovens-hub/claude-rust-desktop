# Permission Mode Selector (权限模式选择器) Spec

## Why
当前 Claude agent 执行任务时，所有工具调用都需要用户确认权限，导致任务频繁被取消。用户需要类似 Claude Code 的权限模式选择器，可以在"询问权限"、"接受编辑"、"计划模式"和"全托管模式"之间切换，避免每次工具调用都需要手动确认。

## What Changes
- 在 Header 区域添加权限模式选择器（类似 Claude Code 的 UI）
- 新增 4 种权限模式：Ask permissions、Accept edits、Plan mode、Bypass permissions
- 修改后端 PermissionManager，根据当前模式自动决定是否需要确认
- 权限模式通过 localStorage 持久化
- 全托管模式（Bypass permissions）下，所有工具调用自动通过，无需确认

## Impact
- Affected specs: 无（新增功能）
- Affected code:
  - 新增: `src/components/PermissionModeSelector.tsx`
  - 修改: `src/stores/useChatStore.ts`, `src/components/MainContent.tsx`, `src-tauri/src/permissions/manager.rs`, `src-tauri/src/permissions/mod.rs`

## ADDED Requirements

### Requirement: 权限模式选择器
系统 SHALL 在 Header 区域提供权限模式选择器，包含 4 种模式选项：
1. **Ask permissions (询问权限)**: 所有工具调用都需要用户确认
2. **Accept edits (接受编辑)**: 自动接受文件写入等编辑类操作，只询问危险操作
3. **Plan mode (计划模式)**: 只读模式，禁止所有修改操作
4. **Bypass permissions (全托管模式)**: 所有操作自动通过，无需任何确认

#### Scenario: 切换权限模式
- **WHEN** 用户点击权限模式选择器并选择"全托管模式"
- **THEN** 权限模式设置为 bypass，所有后续工具调用自动通过，无需确认

### Requirement: 权限模式持久化
系统 SHALL 将用户选择的权限模式保存到 localStorage 中，页面刷新后自动恢复。

#### Scenario: 权限模式持久化
- **WHEN** 用户选择"全托管模式"后刷新页面
- **THEN** 权限模式仍为"全托管模式"

### Requirement: 全托管模式警告
系统 SHALL 在首次切换到"全托管模式"时显示警告提示，告知用户此模式下所有操作将自动执行。

#### Scenario: 全托管模式警告
- **WHEN** 用户首次选择"全托管模式"
- **THEN** 显示确认对话框，用户确认后切换模式，并记录已确认状态

## MODIFIED Requirements

### Requirement: PermissionManager 权限检查
**修改前**: 所有工具调用都通过 ruleset 和 dangerous tools 列表决定是否需要确认
**修改后**: 根据当前权限模式决定是否需要确认
- Bypass permissions: 所有操作自动通过
- Plan mode: 只读操作自动通过，写操作被拒绝
- Accept edits: 只读和编辑操作自动通过，危险操作需要确认
- Ask permissions: 使用原有 ruleset 逻辑
