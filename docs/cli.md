# CLI 与 Desktop 共用代码

```text
Cargo workspace
├── crates/core       claude-core：模型、工具、权限、会话、配置、SQLite
├── crates/cli        claude-code-cli：命令解析、REPL、流式输出
├── src-tauri         claude-desktop-tauri：Tauri、React Bridge、系统凭据适配
└── src-tauri/worker  agent-worker：计算机操作辅助进程
```

`claude-code` 直接调用 `claude-core::NativeEngine`，无需启动 Desktop、WebView 或 30085 服务。核心不依赖 Tauri。桌面库重新导出共享模块，保持原有 Rust 调用路径兼容。

## 构建

```powershell
# 原生 CLI，可独立于 npm/Tauri 编译
cargo build --release -p claude-code-cli
./src-tauri/target/release/claude-code.exe --help

# 一次产出 CLI、Desktop 和桌面安装包
npm ci
npm run build:all

# 开发构建 / 单独构建 / 编译检查
npm run build:all -- --debug --no-bundle
npm run build:cli
npm run build:desktop
npm run check:rust
```

统一脚本将产物放在 `artifacts/<版本>-<模式>-<时间>/`。Cargo 缓存和 Tauri 安装包路径仍在 `src-tauri/target`。Windows 安装包由 Tauri 生成；macOS/Linux 需在对应平台构建并安装 Tauri 的系统依赖。也可以继续使用 `npm run tauri -- build`。

自定义 OpenAI/Anthropic 协议由原生 Rust 客户端处理。内置 provider harness 仍需 **Node.js 22+** 和 `runtime/provider-runtime.mjs`；统一构建脚本会打包并复制脚本。只运行 `cargo build` 时，自定义协议不需要该脚本；使用内置 harness 请用统一构建脚本。

## 使用

```powershell
# 使用 Desktop 已保存的 provider，模型 ID 必须匹配
claude-code --provider my-provider --model my-model --path D:\code\my-project

# 环境变量仅注入当前运行，不写入 providers.json
$env:DEEPSEEK_API_KEY = '<your key>'
claude-code --model deepseek-chat query -p '检查项目并解释测试失败原因'

# 单次执行和机器可读输出
claude-code --model deepseek-chat --print '概述项目结构'
'解释这段代码' | claude-code --model deepseek-chat --print --output-format json
claude-code --model deepseek-chat query -p '读取 README' --output-format stream-json

# 任意兼容端点；传入环境变量名而非命令行明文密钥
claude-code --model custom-model --base-url https://example.com --api-format openai --api-key-env MY_API_KEY query -p '检查项目'

claude-code sessions list
claude-code --resume <session-id>
claude-code config show
claude-code config set model deepseek-chat
claude-code config set base_url https://api.deepseek.com
claude-code config set api_format openai
claude-code config import-legacy C:\Users\you\.claude-code\settings.json
claude-code mcp list
claude-code mcp add my-server node -- server.mjs
claude-code skills list
claude-code skills execute <skill-id> '任务说明'
```

默认数据目录与 Desktop 的 `com.claude.desktop` 应用目录一致，可用 `--info` 查看。`--data-dir` 或 `CLAUDE_RUST_DATA_DIR` 可选择隔离目录。两端使用相同的 `config.json`、`claude_desktop.db` 和 MCP 配置。CLI 创建的会话是绑定项目的 Code 会话，沿用项目目录校验和 Code 提示词；恢复会话时使用原项目，传入冲突的 `--path` 会报错。

当前项目的 `CLAUDE.md` 和 `AGENTS.md` 会加入 CLI 请求。模型参数和环境密钥仅在内存生效。`config show` 隐去凭据；`config set api_key` 与 Desktop 的 provider 配置一样会保存密钥到本地配置，优先使用环境变量。Tauri 的登录 token 和系统安全存储仍由 Desktop 管理，CLI 不尝试读取或降级存储这些 token。

REPL 支持 `/help`、`/status`、`/history`、`/tools`、`/exit`；Ctrl+C 取消当前任务，EOF 退出。普通输出只在 stdout 写模型文本，日志和交互提示写 stderr。`json` 返回最终对象；`stream-json` 每行一个事件，以 `type=result` 结束。执行失败返回非零退出码。

## 权限

默认 `--permission-mode ask`。交互会话在终端确认工具操作；单次脚本遇到需要批准的操作立即失败，不自动授权，也不等待五分钟。

- `ask`：按共享权限规则询问。
- `accept-edits`：允许编辑，其他敏感操作仍需确认。
- `plan`：共享核心的只读规划模式。
- `bypass`：显式放开工具权限，适用于你已授权的自动化环境。

## 旧 CLI 合入范围

以下记录当前迁移状态。用户已要求继续补齐外围功能，后续任务、实现顺序和验收标准见 [CLI / Desktop 外围功能迁移交接](cli-desktop-migration-handoff.md)；保留源码快照不代表这些功能已交付。

旧仓库完整的自有 Rust 源码和测试保存在 `legacy/claude-code-cli`，不参与发布构建。活跃 CLI 保留 `query -p`、`repl -p`、`rust`（REPL 别名）、`--model`、`--verbose`、`--no-interactive`、`--info`、配置、MCP、技能和项目初始化入口，改用共享引擎；根级项目路径短参数现在为 `-C`，长参数 `--path` 保留。

`voice`、`team-sync` 和团队同步宿主已接入共享核心，详见下文。`services` 当前只运行 `team-sync`，其他旧服务不会显示为可启动。`magic-docs`、`plugin`、`agent`、`stress-test`、memory dream/auto-dream 及原 GUI/Web/WASM 的独有能力仍待迁移。旧配置导入目前迁移 provider、模型与 token 上限；旧团队记忆支持显式导入，旧个人 memory 支持显式 JSON 导入；session、插件和 MCP 数据不会自动转换。完整映射见 [能力矩阵](cli-desktop-capability-matrix.md)。

## 语音

使用原生 CPAL 麦克风采集（Windows WASAPI），把 PCM 编码为 WAV；文件输入支持 WAV、WebM、Ogg、MP3、MP4/M4A。识别走 **OpenAI-compatible 音频转写 multipart API**，不会向聊天 API 发送音频占位字符串。后端必须提供 `file`、`model`、`response_format=json` 接口并返回 `{"text":"..."}`；可部署自己的兼容 STT 服务。没有配置后端或凭据时明确报错，不自动连接付费服务。

```powershell
$env:VOICE_API_KEY = '<audio backend credential>'
claude-code voice-config https://your-stt-host/v1/audio/transcriptions --model whisper-1 --credential VOICE_API_KEY
claude-code voice --file D:\audio\recording.wav
claude-code voice --push-to-talk
claude-code voice --push-to-talk --continuous --send
claude-code --resume <session-id> voice --file D:\audio\recording.wav --send
```

`--push-to-talk`（兼容 `-p`）以 Enter 开始/停止一段录音；`--continuous` 连续采集多个片段，`/exit` 退出。`--send` 将识别文本交给同一个共享引擎会话；不指定时输出 JSON 转写结果。Ctrl+C 取消采集或转写，采集线程退出并释放设备。录音/文件上限 25 MiB；转写请求超时 120 秒。原生音频依赖已包含在 CLI 构建中；Linux 构建需 ALSA 开发库，macOS 使用 CoreAudio，两平台尚未验收。

Desktop 在「设置 → 服务与团队」配置地址、模型及凭据（系统安全存储或宿主环境变量，失败不会降级写入配置文件），再点聊天输入框麦克风。MediaRecorder 采集真实音频，每段最多 60 秒；停止后释放设备，再转写。支持编辑结果、继续录音、使用文本和取消。关闭面板或组件卸载会释放麦克风并取消转写。Windows 原生麦克风采集/再次打开已测试；WebView2 的真实权限交互和真实 STT 识别仍需验收。

## 团队同步与服务宿主

仓库提供独立的 protocol v1 服务端。服务端由运维提供 token-to-team 映射，每个 bearer token 只能访问一个团队；客户端 `auth` 必须经过真实 HTTP 验证后才保存配置。它是自部署协议，**不是 Claude.ai Team OAuth**。生产部署需配置 HTTPS 反向代理、凭据、数据目录备份和端点；默认仅绑定回环地址。

```powershell
# 独立终端启动服务端。此 JSON 是运维配置的凭据映射，不会自动生成有效云端 token。
$env:TEAM_SERVER_TOKENS = '{"replace-with-operator-issued-token":"engineering"}'
claude-code --data-dir D:\team-server team-sync serve --bind 127.0.0.1:38080

# 客户端使用自己的数据目录，不依赖 Desktop 或 30085。
$env:TEAM_SYNC_TOKEN = 'replace-with-operator-issued-token'
claude-code --data-dir D:\team-client-a team-sync auth engineering --endpoint http://127.0.0.1:38080
claude-code --data-dir D:\team-client-a team-sync create 'Architecture' --content 'Use Rust' --tags 'decision,backend'
claude-code --data-dir D:\team-client-a team-sync sync
claude-code --data-dir D:\team-client-a team-sync list
claude-code --data-dir D:\team-client-a team-sync update <id> 'Architecture' --content 'Revised decision'
claude-code --data-dir D:\team-client-a team-sync delete <id>
claude-code --data-dir D:\team-client-a team-sync conflicts
claude-code --data-dir D:\team-client-a team-sync resolve <id> --keep local
claude-code --data-dir D:\team-client-a team-sync sync

# 前台常驻；另一终端可控制同一实例。
claude-code --data-dir D:\team-client-a services start team-sync --interval 60
claude-code --data-dir D:\team-client-a services status
claude-code --data-dir D:\team-client-a services stop team-sync

# 旧团队 JSON：预览不写入；正式导入先备份目标，再原子导入。
claude-code --data-dir D:\team-client-a team-sync import-legacy C:\Users\you\.claude-code\team --preview
claude-code --data-dir D:\team-client-a team-sync import-legacy C:\Users\you\.claude-code\team
```

只同步用户显式创建/导入的团队记忆（ID、标题、内容、标签、版本、删除标记），不读取个人记忆、provider 配置或模型密钥；团队记忆暂不自动注入模型上下文。不同团队的数据、游标和冲突隔离。团队绑定服务端地址，切换到另一个服务端需隔离数据目录，避免复用旧游标。冲突保留双方版本；`--keep local` 重新基于远端版本上传，`--keep remote` 接受远端（包括删除）。删除以 tombstone 传播。超时或离线不清空本地待上传内容；再次 `sync` 或宿主下次轮询重试。同一 mutation ID 重发不会重复创建/增加版本。

宿主默认前台运行，不隐式后台化。`stop` 等待实例实际退出；Ctrl+C 中断请求并释放租约。进程异常退出后租约最多 15 秒失效。Desktop「设置 → 服务与团队」可认证、增改删、同步、处理冲突、预览/导入旧团队数据及启动/停止自动同步；SQLite 租约让 CLI 和 Desktop 使用同一数据目录时只有一个同步宿主。关闭 Desktop 会停止它自己持有的宿主。认证和服务错误会显示，失败不会报告同步成功。

导入保留旧 ID，并将旧 author/time/etag 等原始载荷留在 `team_import_sources`；旧 etag 不用作远端版本，旧 `.team_token` 不导入。导入遇到 ID 内容冲突整体回滚，不覆盖当前记忆；重复导入相同载荷跳过。备份保存在所选 `RuntimePaths.data_dir/backups`，源目录保持不变。

## 验证

```powershell
cargo test -p claude-core --lib --tests
cargo test -p claude-code-cli
cargo check -p claude-desktop-tauri --all-targets
cargo test -p claude-desktop-tauri --lib --tests
```

CLI 集成测试使用临时数据目录和本机模拟模型服务，实际启动可执行文件，覆盖流式输出、工具读写、权限中断、会话恢复、失败退出和旧配置只读导入，不使用真实 API 密钥。

## 个人记忆传输

`memory status` 查询当前后端。`memory clear --yes` 清空该后端个人记忆，保留会话与团队数据。

```powershell
claude-code memory import D:\backup\legacy-memory.json --preview
claude-code memory import D:\backup\legacy-memory.json
claude-code memory export D:\backup\personal-memory.json
```

导入/导出要求 local 后端，支持旧 MemoryEntry 数组和新版本化归档。导入前备份完整 SQLite，报告返回路径；重复导入跳过，冲突整批回滚。导出路径必须不存在。桌面长期记忆面板提供对应预览、确认与下载。dream/auto-dream 尚未实现。


## P2/P3 additions (2026-10-15)

Dream/AutoDream, Magic Docs previews and automatic maintenance, agent roles, versioned executable plugin bundles, existing-host MCP restart, project-scoped team model context, legacy session/document imports, local diagnostics and manifest update checks are now exposed by the CLI. Run `claude-code help --topic plugins` (also `memory`, `mcp`, `agents`, `magic-docs`, `team-sync`, `services`, `voice`, `update`) or `help COMMAND` for details.

Complete command examples, format boundaries, screenshots and remaining work are maintained in `D:/code/project/project-harven/claude-code-cli-doc/p2-p3-migration-20261015.md`. This checkpoint does not provide full old plugin/MCP import compatibility or production STT/team deployment.
