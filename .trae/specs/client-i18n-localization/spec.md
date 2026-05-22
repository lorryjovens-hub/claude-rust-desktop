# 客户端国际化（i18n）与汉化 Spec

## Why
当前客户端界面文字全部为英文，中文用户阅读和使用体验不佳。需要建立完整的国际化系统，支持中英文切换，并在设置界面添加语言选项，实现全局语言设置。

## What Changes
- 建立 i18n 翻译系统，包含中英文翻译文件
- 在 UI Store 中添加语言设置状态，支持持久化到 localStorage
- 创建 useI18n hook 供组件使用
- 汉化所有主要 UI 组件（Sidebar、SettingsPage、Header、ChatInput、MessageBubble、SearchModal 等）
- 在 SettingsPage 中添加语言切换选项
- 默认语言跟随系统语言或上次设置

## Impact
- Affected specs: 无（新增功能）
- Affected code: 
  - 新增: `src/locales/zh.json`, `src/locales/en.json`, `src/hooks/useI18n.ts`
  - 修改: `src/stores/useUIStore.ts`, `src/components/Sidebar.tsx`, `src/components/SettingsPage.tsx`, `src/components/Header.tsx`, `src/components/ChatInput.tsx`, `src/components/MessageBubble.tsx`, `src/components/SearchModal.tsx`, `src/components/CustomizePage.tsx`, `src/components/ProjectsPage.tsx`, `src/components/ArtifactsPage.tsx`, `src/components/ModelsPage.tsx`, `src/components/DesignPage.tsx`

## ADDED Requirements

### Requirement: i18n 翻译系统
系统 SHALL 提供基于 JSON 的翻译文件，支持中英文两种语言，所有 UI 文本均可通过翻译 key 获取。

#### Scenario: 获取翻译文本
- **WHEN** 组件调用 `t('sidebar.newChat')`
- **THEN** 返回当前语言对应的翻译文本

### Requirement: 语言设置持久化
系统 SHALL 将用户选择的语言保存到 localStorage，页面刷新后自动加载上次设置的语言。

#### Scenario: 语言设置持久化
- **WHEN** 用户在设置中选择"中文"
- **THEN** 语言设置为 `zh`，保存到 localStorage，刷新页面后仍为中文

### Requirement: 默认语言检测
系统 SHALL 在首次加载时检测浏览器语言，若为中文则默认使用中文，否则使用英文。

#### Scenario: 首次加载语言检测
- **WHEN** 用户首次打开应用，浏览器语言为 `zh-CN`
- **THEN** 默认使用中文界面

### Requirement: 设置界面语言选项
系统 SHALL 在设置界面"通用"标签中添加"界面语言"选项，提供中英文切换。

#### Scenario: 切换语言
- **WHEN** 用户在设置中选择"中文"或"English"
- **THEN** 界面立即切换到对应语言

## MODIFIED Requirements

### Requirement: Sidebar 组件汉化
**修改前**: 所有文本硬编码为英文
**修改后**: 所有文本通过 i18n 系统获取，支持中英文

### Requirement: SettingsPage 组件汉化
**修改前**: 所有文本硬编码为中文
**修改后**: 所有文本通过 i18n 系统获取，添加语言切换选项

### Requirement: 其他 UI 组件汉化
**修改前**: 各组件文本硬编码
**修改后**: 所有文本通过 i18n 系统获取
