## Cowork 面板对标官方功能 — 完整方案

### 官方 Claude Code 多 Agent 能力

| 官方能力 | 当前状态 | 需要增强 |
|---|---|---|
| 任务自动分解 | ✅ 前端 `splitTask()` 模拟 | 🔴 接入后端 API 真实分解 |
| 多角色 Agent 分配 | ✅ `assignAgents()` 模拟 | 🔴 接入后端 `orchestration` 引擎 |
| 依赖图调度 | ✅ `SubTask.dependencies` | 🟡 DAG 可视化 |
| 并行执行 | ✅ `setInterval` 模拟 | 🔴 后端 `execute_workflow` |
| 上下文传递 | ❌ | 🟡 Agent 间上下文交接 |
| 实时进度 | ✅ `Math.random()` 模拟 | 🔴 SSE 事件流 |
| 人工审核关卡 | ❌ | 🟢 Approval gate |
| 输出合成 | ❌ | 🟢 多 Agent 结果合并 |

### 实施计划

```
Phase 1: 后端真实调用 (现在)
  ├── handleStartSwarm → POST /api/workflow/execute
  ├── 解析返回的 WorkflowTask → Agent + SubTask
  └── 实时状态显示

Phase 2: DAG 可视化
  ├── 力导向依赖图 (已有 TaskNode 组件)
  └── 放大/缩小/拖拽

Phase 3: Agent 管理
  ├── 增删 Agent 角色
  ├── 手动分配子任务
  └── Approval gate 机制

Phase 4: 结果合成
  ├── 多 Agent 输出合并
  ├── 冲突检测
  └── 最终报告生成
```
