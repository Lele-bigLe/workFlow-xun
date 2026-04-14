# 循 (Xun) - Agentic Workflow MCP

循 (Xun) 是一个**独立的、提供工作流规则注入与自检机制**的 Model Context Protocol (MCP) 服务器。
它的核心目标是：对抗大语言模型（LLM）的“目标偏移”与“线性思维”，强制 AI 在执行复杂任务时遵循用户预设的多步工作流（如：记忆搜索 → 上下文读取 → 方案设计 → 代码实现 → 用户确认）。

---

## 核心特性

- **被动规则注入 (`instructions`)**：当 AI (如 VS Code/Cursor 中的大模型代理) 连接此 MCP 时，Xun 会直接将工作流的要求和配置写进系统级 Prompt，强占极高的注意力权重。
- **强制起手式 (`mcp_workFlow_hint` 工具)**：AI 在处理任何任务前，被要求在工具注册完成后调用此工具获取“定制化任务建议”。Xun 的引擎会根据任务的描述自动分级为简单/中等/复杂，并动态跳过不必要的节点（如简单问候无需读代码）。
- **硬性门禁自检 (`mcp_workFlow_check` 工具)**：AI 在完成最后输出前主动抛出已完成步骤，并尽量回传 hint 生成时的快照锚点（`suggested_steps` / `workflow_fingerprint` / `hint_fingerprint`）。Xun 会优先校验“这一轮 hint 定义出的流程实例”是否被完成，而不是盲目重算当前配置，避免状态漂移导致误判。
- **配置校验与元信息**：保存 `workflow.yaml` 或自定义预设前，会先校验重复 node id、非法 `loop_back_to`、required 节点误配 `skip_when` 等结构问题；同时后端可返回 workflow fingerprint、配置路径和 issue 列表，供 GUI 决定是否展示。
- **轻量 MCP 构建链路**：`workFlow` 二进制现在可使用独立的 `mcp` feature 构建，不再强制链接 Tauri GUI 依赖，降低 Windows 冷启动和首次工具注册延迟。
- **白话跳过条件**：`skip_when` 支持自然语言描述（如 `"用户没有明确的任务意图时跳过"`），引擎自动解析内置条件，自定义白话条件透传给 AI 自主判研。
- **可定制工作流配置**：支持预设切换、自定义 YAML 配置、历史快照、可配置数据目录。

## 双产物架构

| 产物 | 二进制名 | 说明 |
|------|---------|------|
| MCP Server | `workFlow` | 纯 Rust 后端，供 AI Client（VS Code Copilot 等）挂载调用 |
| GUI 配置界面 | `workflow-ui` | Tauri V2 + Vue 3 + Tailwind CSS，可视化编辑工作流节点与预设 |

## 目录结构

```
src-tauri/
  src/
    bin/xun_mcp.rs          # MCP 服务器入口
    main.rs                  # Tauri GUI 入口
    lib.rs                   # Tauri 命令层
    workflow/
      definition.rs          # 数据模型 (WorkflowNode, Complexity 等)
      engine.rs              # 工作流引擎 (evaluate/check)
      loader.rs              # 配置加载/保存/历史快照/设置
      default.rs             # 内置预设 (均衡编码流/轻量查阅流)
    workflow_mcp/
      server.rs              # MCP 协议实现 (rmcp)
src/
  App.vue                    # 前端单页面组件
```

## 数据目录

默认 `~/.xun/`，可通过 GUI 设置面板更改，存储在 `~/.xun/settings.yaml`。

```
~/.xun/
  workflow.yaml              # 当前生效的工作流配置
  presets.yaml               # 预设列表（首次启动自动播种内置预设）
  settings.yaml              # 用户设置（数据目录路径等）
  history/
    workflow_1234567890.yaml  # 历史快照（自动保留最近 10 份）
```

## 构建

进入 `src-tauri` 目录执行：

前端与 Tauri CLI 构建链至少需要 Node 18；本次实测中，系统 Node 14 会在 `vue-tsc` 依赖里直接卡在 `??=` 语法解析阶段。

## 运行

```bash
# 开发调试（GUI 热重载）
npm run tauri dev

# 一次性构建双 exe（workFlow.exe + workflow-ui.exe）
npx tauri build --no-bundle

# 仅编译 MCP Server
cargo build --release --bin workFlow --no-default-features --features mcp
```

构建产物位于 `src-tauri/target/release/`。

## 在 VS Code 中配置 MCP

在项目根目录创建 `.vscode/mcp.json`：

```json
{
  "servers": {
    "workFlow": {
      "type": "stdio",
      "command": "D:/sorftwer/xun/src-tauri/target/release/workFlow.exe"
    }
  }
}
```

配置完成后重启 VS Code，并等待 MCP 工具注册完成；确认工具列表里已出现 `mcp_workFlow_hint` 和 `mcp_workFlow_check` 后再开始任务。

如果首次调用直接报 `TypeError: Cannot read properties of undefined (reading 'invoke')`，通常不是 `hint/check` 算法错误，而是当前会话里 MCP 工具还没注册完成。先等待 `mcp_workFlow_hint` / `mcp_workFlow_check` 出现在工具列表，再开始该轮任务。

## MCP 工具返回字段说明

### `mcp_workFlow_hint` 返回

| 字段 | 类型 | 说明 |
|------|------|------|
| `contract_version` | number | hint/check 协议版本，当前为 2 |
| `complexity` | string | 任务复杂度（simple/medium/complex），决定执行深度和必要步骤 |
| `workflow_fingerprint` | string | 当前 workflow.yaml 的稳定指纹，用于 check 阶段检测配置是否漂移 |
| `hint_fingerprint` | string | 当前 hint 快照的稳定指纹，用于保证 hint/check 属于同一轮契约 |
| `expected_step_ids` | array | 本轮 hint 最终展开出的步骤 ID 列表，是 check 的默认校验对象 |
| `suggested_steps` | array | 建议执行的步骤列表，按顺序逐步执行。每项含 `id`（步骤标识）、`name`（显示名）、`action`（具体执行指令）、`skip_conditions`（AI 自主判断的白话跳过条件） |
| `skipped_steps` | array | 已被引擎跳过的步骤，含 `id` 和 `reason`（跳过原因），无需执行 |
| `loop_info` | object/null | 循环回退信息。`loop_node_id`=触发循环的节点、`loop_back_to`=回退目标节点、`re_execute_nodes`=循环时需重新执行的节点列表。为 null 表示无循环 |
| `reminder` | string | 根据复杂度生成的执行提醒 |
| `progress_display` | string | Markdown 格式的进度清单（checkbox 列表），可直接展示给用户 |

### `mcp_workFlow_check` 返回

| 字段 | 类型 | 说明 |
|------|------|------|
| `contract_version` | number | hint/check 协议版本，当前为 2 |
| `status` | string | `ok` / `missing_steps` / `stale_config` / `invalid_hint_snapshot` |
| `passed` | boolean | 是否通过检查（只有 `status=ok` 时为 true） |
| `should_rehint` | boolean | 是否必须重新调用 hint；`stale_config` 与 `invalid_hint_snapshot` 会返回 true |
| `missing_steps` | array | 尚未完成的步骤列表，每项含 `id`、`name`、`action` |
| `missing_required_steps` | array | 尚未完成且当前配置中标记为 required 的步骤子集 |
| `completed_steps` | array | 已完成的步骤 ID 列表（传入参数的回显） |
| `normalized_completed_steps` | array | 归一化、去重后的 completed_steps |
| `unknown_completed_steps` | array | 不属于本轮 hint 快照的 completed_steps 项 |
| `duplicate_completed_steps` | array | 在 completed_steps 中重复上报的步骤 ID |
| `expected_step_ids` | array | 本次 check 实际用于判定的步骤 ID 列表 |
| `current_workflow_fingerprint` | string | check 执行时当前 workflow.yaml 的指纹 |
| `provided_workflow_fingerprint` | string/null | 调用方传回的 workflow_fingerprint；为空表示走兼容模式 |
| `hint_fingerprint` | string | 本次 check 计算并验证后的 hint 指纹 |
| `completion_rate` | number | 当前完成比例，范围 0 到 1 |
| `diagnostics` | array | 诊断信息，例如重复步骤、未知步骤、快照漂移原因 |
| `loop_info` | object/null | 与 hint 相同的循环回退信息；当快照漂移时会返回 null |
| `message` | string | 检查结果文字摘要 |
| `progress_display` | string | 带完成状态的 Markdown checkbox 进度清单 |

推荐的 check 调用方式：除了 `completed_steps`，一并回传上一轮 hint 的 `suggested_steps`、`workflow_fingerprint`、`hint_fingerprint`。这样即使中途有人改了 workflow.yaml，check 也会明确返回 `stale_config`，而不是误把“配置变化”解释成“你漏做了步骤”。

## 自定义工作流 (workflow.yaml)

Xun 在启动时按以下优先级读取配置：
1. `settings.yaml` 中指定的自定义数据目录
2. 可执行文件同目录下的 `workflow.yaml`
3. 当前工作目录下的 `workflow.yaml`
4. `~/.xun/workflow.yaml`

如果均不存在，使用内置的"均衡编码流"默认配置。

**配置示例**：
```yaml
nodes:
  - id: memory_gate
    name: 记忆搜索
    required: false
    skip_when:
      - "用户没有明确的任务意图时跳过"
      - "用户只是打招呼问候时跳过"
    action: "调用 smart_search 搜索相关记忆，命中则 memory_read"

  - id: read_context
    name: 上下文读取
    required: true
    action: "读取相关代码文件，理解现有实现"

  - id: execute
    name: 代码实现
    required: true
    action: "执行代码修改，遵循项目代码规范"

  - id: gate
    name: 交付确认
    required: true
    action: "调用寸止(zhi)工具获取用户反馈，循环直到用户说'结束'"

complexity_rules:
  simple:
    max_files: 2
    nature: ["register", "config", "fix_typo"]
  medium:
    max_files: 5
    nature: ["new_feature", "refactor", "bug_fix"]
  complex:
    nature: ["architecture", "breaking_change", "migration"]
```

## 设计理念

许多传统的辅助工具只解决了流程的**终点**，而没有解决大模型在代码长跨度开发时的**起点迷失**。Xun 的独立形态让它可以附加在任何项目中，仅提供工作流策略的控制引擎，这赋予了开发者搭建高度定制化智能体（Agent）生产线的可能。
