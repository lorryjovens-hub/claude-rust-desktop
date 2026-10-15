# 旧 CLI 源码快照

来源：相邻 `claude-code-cli` 仓库，提交 `36a6231ac3134e01c6d9dacafb2094af7afbbe7c`。

此目录保留原 Rust `src`、`crates`、测试、语言资源、Cargo 清单、README 和 MIT 许可，便于核对旧功能和继续移植。未复制 `.git`、`.env`、用户配置、构建产物、第三方 TypeScript 仓库及独立 GUI 项目；来源仓库未修改。

此快照不属于主 Cargo workspace，也不参与 CLI/Desktop 发布构建。原 README 中的功能和性能声明仅是历史资料，不代表当前版本经过验证的能力。

当前入口在 `crates/cli`，引擎统一为 `crates/core`。终端输入、初始化工具及旧模型别名配置来自该快照，并修正 EOF 和模型选择行为。旧的独立 API/工具循环不再用于新 CLI。

迁移后的功能及兼容边界见 [CLI 文档](../docs/cli.md)。
