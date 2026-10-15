# CLI / Desktop 能力迁移矩阵

核查日期：2026-10-15。以 `legacy/claude-code-cli/src/cli/mod.rs` 和交接文档为基线。**这是截至 P2/P3 检查点的实施记录，未列为完成的能力仍需后续实现/验收。**历史快照不参与功能完成计数。

| 旧 CLI 入口 | 当前 CLI / 共享实现 | Desktop 入口 / 证据 | 状态及缺项 |
|---|---|---|---|
| `repl/query/rust`、stdin、模型、权限、恢复 | `crates/cli/src/{main,session,config}.rs`、`crates/core/src/runtime.rs` | 主聊天 / 项目会话 | 已等价覆盖；既有 `standalone.rs` 5 个测试 |
| `config show/set/reset` | `crates/cli/src/config.rs` | ProviderSettings、GeneralPreferences | 已等价覆盖；旧配置显式只读导入 |
| `mcp list/add/remove/restart` | 共享 MCP manager 与 `mcp/control.rs`，CLI hosts/restart | 原 MCP 管理页面；服务设置提供跨宿主列表/重启 | 跨进程重启测试确认真实 PID 更换；多宿主显式选择；旧 MCP 配置完整导入仍缺 |
| `skills list/search/help/execute` | `crates/core/src/skills/`、CLI `main.rs` | SkillsPage | 已接入；完整旧格式映射待审计 |
| `init` | CLI `terminal/init.rs`，不覆盖已有指令文件 | 项目创建与信任对话框 | 基础能力已覆盖；旧 SSH/remote 初始化另行审计 |
| `voice -p/--push-to-talk` | CPAL → PCM WAV → `services/voice.rs` 音频 API；文件/连续输入/共享会话 | 聊天麦克风 → VoiceInput；设置 → 服务与团队 | 实施完成，测试通过（原生硬件采集、multipart、模型会话、前端取消）；**真实 STT 与 WebView2 设备/权限尚未验收** |
| `team-sync status/auth/sync/list/create/delete` | `services/team.rs`、`team_server.rs`；CLI `services.rs` | 设置 → 服务与团队（含更新、冲突与旧数据导入） | 已迁入自部署真实 HTTP 协议；两个 CLI 客户端/独立服务进程验证；生产部署与 GUI 手工验收待完成，不提供伪造官方 OAuth |
| `services status/start/stop/team-sync/voice` | `services/lifecycle.rs`，跨进程 SQLite 租约；CLI 前台宿主，voice 显示后端配置 | 设置 → 服务与团队；宿主句柄由 Tauri 持有，退出中止并等待 | **team-sync 链路完成**；voice 是配置状态，不是后台录音宿主 |
| `services auto-dream/magic-docs/plugins/agents` | AutoDream 与 Magic Docs 前台宿主；插件为按需受控子进程；agent 为正常会话 | DreamControls、MagicDocsPanel、InstalledPlugins、AgentLauncher | 已接入实际运行；插件/agent 不伪装为常驻后台服务 |
| `memory status/clear/export/import/dream/auto-dream` | 共享个人记忆传输、Dream 来源去重、租约及自动门槛 | MemoryPanel/DreamControls | 核心、CLI、前端测试通过；Dream 保留原文；生产模型质量仍需实际验收 |
| `magic-docs list/check/update/clear` | `services/magic_docs.rs`；持久化预览、权限和文件冲突检查、自动更新、旧登记导入 | 服务设置 MagicDocsPanel | 真实模型 HTTP fixture 与文件写入测试、UI 可见浏览器 fixture 通过；当前自动上下文来自匹配的个人记忆 |
| `agent --agent-type --prompt` | `services/agents.rs` 与 NativeEngine；别名和自定义工具限制 | AgentLauncher 创建正常共享会话 | CLI 真实模型/工具测试通过；旧完整 AgentDefinition 格式仍待映射 |
| `plugin list/install/remove/update/search/enable/disable` | `services/plugins.rs`，真实 Node/Python 运行、目录检索、执行撤销、PluginRun 权限 | 服务设置 InstalledPlugins | 核心/CLI/前端及可见浏览器 fixture 验证；新 bundle 格式，旧 native/WASM/回调钩子与自动目录转换未完成 |
| `stress-test -c/-i` | `services/diagnostics.rs`，可控本地真实 HTTP 压测 | 服务设置 PeripheralDiagnostics | 真实请求测试通过；统计响应头耗时，不是模型性能对比 |
| `update`、`help --topic` | 共享 semver/有界 HTTP 更新检查、CLI 专题及标准命令帮助 | 服务设置更新检查与现有 updater | HTTP/格式错误正确失败，CLI 帮助测试通过；未部署官方更新源 |
| 历史数据 | provider、团队、个人 memory、旧 sessions 与 Magic Docs 登记支持显式导入 | 设置与记忆/文档面板 | 会话/文档导入有预览、备份、幂等、回滚；旧 MCP/plugin 完整格式导入仍待实施 |

## 扩展能力审计边界

下面保留逐项审计范围；当前相近实现路径是下一阶段核查起点，**不等于已证明行为等价或无需迁移**。

| 旧目录 / 能力 | 当前可比较位置 | 当前结论 |
|---|---|---|
| `learning/{engine,experience,feedback,pattern}.rs` | `memory/caveman_rtk.rs` 的反馈/迭代 | 仍待详细格式及行为映射；禁止把现有反馈直接视为完整学习系统 |
| `performance/{error_classifier,retry_policy,rate_limiter,prompt_cache,smart_router,tool_persistence}.rs` | `native_engine/`、`cache/`、`metrics/`、`db/` | 仍待逐项审计；现有工具轨迹/模型请求存储已有测试，旧智能路由/限流/重试策略尚未等价验收 |
| `i18n/{loader,locales,translator}.rs` | `src/hooks/useI18n.ts`、`src/locales/` | 桌面语言切换已有覆盖；旧语言范围、CLI 文案、复数/日期/方向行为仍待审计 |
| `tools/advanced/{agent,delegation,worker,team,send_message,escalation}.rs` | `tools/`、`multiagent/`、`agent_bus/`、worker | 仍待逐项参数和权限/生命周期映射 |
| `tools/advanced/{bash,powershell,worktree,web_tools,notebook,lsp,tool_search}.rs` | `tools/`、`worktree/`、`ide/`、`research/` | 仍待审计；已共享 Bash 的取消生命周期，不能据此宣称其余等价 |
| `tools/advanced/{task_tools,todo_write,cron,skill,mcp_tools,mcp_bridge,config,brief,ask_question,permission_tools,plan_mode}.rs` | `task/`、`scheduler/`、`skills/`、`mcp/`、`ask_user/`、`permissions/` | 仍待逐项协议、参数、持久化和交互核查 |
| `advanced/{ssh,remote,project_init}.rs` | 项目与共享工具 | 仍待审计 |
| `gui/`（会话、设置、主题、高亮、侧栏、工具轨迹） | React 当前产品、SQLite 会话、既有前端测试 | 基础产品已有对应形态；独有操作待逐项核查，不重新构建 egui |
| `web/`（路由、模型、模板）、`wasm/`（bridge/client/storage） | Tauri Bridge / React、`runtime.rs` | 基础传输已有对应形态；离线存储/导入导出等独有能力待逐项核查，不据技术栈差异免除迁移 |

P0 已交付共享服务上下文（RuntimePaths/数据库/凭据）、结构化状态、增量数据库迁移、跨进程租约和完整 team-sync 运行/停止链路。模型、工具、权限、会话继续复用原 NativeEngine。P1 团队记忆是明确分享的独立范围，个人 MemEx 数据不会自动上传；团队记忆现可显式按项目授权进入模型上下文，绑定团队/端点且不包含凭据。

自动化证据：`crates/core/tests/peripheral_services.rs`、`crates/cli/tests/peripheral.rs`、`src/components/{VoiceInput,ServicesSettings}.test.tsx`。Windows 原生设备证据：显式运行 `native_microphone_capture_releases_the_device -- --ignored`；这不是识别准确率或 WebView2 权限验收。

P2/P3 详细命令、截图与边界见 `D:/code/project/project-harven/claude-code-cli-doc/p2-p3-migration-20261015.md`；最新验证记录见交接文档第 11 节。扩展能力审计表仍保留未完成状态，不以相似模块替代等价验收。
