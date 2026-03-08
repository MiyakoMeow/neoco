# 需求文档

## 产品信息

- 名称：NeoCo

- 介绍：

> 原生支持多智能体协作的智能体应用。

---

## 主要解决问题

### 现有多智能体协作方案不完善

多智能体可以用于：

- 并行执行不同操作，提升效率。
- 整理和过滤任务所需信息，保持主模型上下文干净，降低思考干扰和调用成本。

现有的主流AI Agent应用，如Claude Code等各类编码工具、OpenClaw等，在多智能体协作方面，仅提供了以下功能：

- 创建一个子Agent。
- 子Agent任务完成后，接收输出。

当出现异常情况，如任务执行时间过长/偏航等，无法第一时间纠正。

---

## 功能特性

### 模型组/多模型提供商配置

个人认为，未来模型会往专用化、细分化的方向发展。

目前因为算力资源有限，大多数国内一线模型厂商只能提供1-2种模型，但各个模型之间的特征差异明显：

- 一部分模型脑子很好，善于思考，灵感似涌泉。
- 一部分模型更擅长执行，执行准确，输出速度快。
- 这两项都很擅长的模型，价格一般不便宜，或者速度不够快。

以及以下细分需求：

- 部分模型有图像/语言识别能力。
- 有多个提供者可选，或有同一提供者的多个API Key，希望循环使用模型，实现负载均衡或避免中断。
  - 当出现异常，该模型自动重试3次（指数退避：1s, 2s, 4s）均失败时，自动尝试下一个可选模型，或当前模型的下一个API Key。

### 模型调用

- 基于OpenAI Chat Completion API。
  - OpenAI API调用使用 [async-openai](https://crates.io/crates/async-openai) crate (版本 0.33.0)。
- 流式输出
- 工具调用
  - 尽可能支持并行化工具调用
- 不需要支持更多功能，因为这些API就可以实现所有功能

- 为后续支持Anthropic、OpenAI Responses、OpenRouter、Github Copilot等预留接口。

### Session管理

- 存储在`~/.local/neoco/(session_ulid)/agents/(agent_ulid).toml`文件。该文件存储所有的上下文内容。
- **Session ULID与Agent ULID的关系**：
  - Session ULID是顶级容器的ULID，在创建Session时生成
  - Agent ULID是每个Agent实例的ULID，在Agent开始对话时生成（第一个Agent除外，其使用Session ULID）
  - 第一个Agent（最上层）的Agent ULID与Session ULID相同
- Session ULID使用ULID（Universally Unique Lexicographically Sortable Identifier）。使用`ulid`这个crate。

#### 消息内容存储

- 使用TOML存储消息内容：

- 参考：`(agent_ulid).toml`。不同Agent使用不同文件。

```toml
# Agent配置
[agent]
definition_id = "coder"
parent_ulid = "01HF..."  # 上级Agent的ID，最上层Agent此字段省略

# Agent消息列表
[[messages]]
# 整个Session中，所有Agent的所有消息，都拥有唯一整数id。
# id来源：作用于整个Session的原子化自增id分配器。
# 回溯时，指定特定id为x，只保留id <= x的消息。
id = 1
role = "user"
content = "xxx"

[[messages]]
id = 2
role = "assistant"
content = "xxx"
```

- 通过`parent_ulid`字段可以恢复完整的Agent树形结构。

### MCP

- 使用 [rmcp](https://crates.io/crates/rmcp) crate (版本 1.1.0)。
- 同时支持 `stdio` 和 `http` 两种传输模式。

### Skills

- 参考：[agentskills.io](https://agentskills.io/)。
- 按需加载

### 上下文压缩

- 调用模型，关闭thinking模式，关闭所有工具支持，获取输出内容。
- 新的上下文内容，按顺序依次为：
   1. 默认激活的内容。
   2. 刚才的输出内容。
   3. 在此之前动态激活的工具、MCP、Skills。

### 上下文观测

- 提供上下文观测功能，用于查看当前上下文的详细状态信息。
- 支持以下观测维度：
   1. **消息列表**：
      - 消息ID
      - 消息角色（system/user/assistant/tool）
      - 消息内容
      - 消息大小（字符数/预估token数）
      - 消息时间戳
   2. **统计信息**：
      - 总消息数量
      - 各角色消息数量
      - 总上下文大小（字符数/token数）
      - 上下文使用率（相对于模型窗口）
   3. **内容分组**：
      - 系统提示词列表
      - 用户消息列表
      - 助手消息列表
      - 工具返回列表
 - 提供过滤和排序功能：
    - 按角色过滤
    - 按大小排序（默认按ID升序，可通过sort参数覆盖）
    - 按时间排序（默认按ID升序，可通过sort参数覆盖）
- 在TUI模式和后台Agent模式中都可用。

#### 压缩触发时机

- 在当前上下文大小达到模型上下文窗口的特定百分比时，自动触发。百分比默认为`90%`。可在配置文件中，配置是否自动触发以及触发百分比。
- 可手动触发，参考下方TUI模式。

#### 压缩提示词

```markdown
# 任务：压缩上下文

## 目标

整理当前的上下文内容，提取**所有**有价值的信息。

## 详细回答以下问题

1. 需求是什么？
2. 目标在哪里？
3. 现在做到了哪一步？
4. 接下来要做什么？
5. 其它信息。
```

### 上下级智能体之间的协作

- 基于`SubAgent`模式。

- 添加上下级智能体之间的沟通工具，上下级模型之间可以直接在会话中传递内容。
- 上级可以要求下级执行汇报。
- 灵感来自现代公司分工。

- 多层智能体树形结构：
  - 最上层智能体直接与用户对话，每个Session只有一个最上层智能体。
  - 每个智能体都可以有多个下级。可以设置例外情况，例如执行智能体只能用于执行。
  - 上层智能体发现任务可以拆分且并行执行时，生成多个下级智能体。
  - 最终会形成一个动态的树形结构。

#### SubAgent创建行为

- 可以指定使用的Agent（来自配置目录的`agents`下的Agent定义）。
- 默认可以覆盖`model`、`model_group`、`prompts`等字段。

### 模块化提示词与工具，以及按需加载

- 模块化提示词、工具、MCP、Skills等实例的目的是，支持内容的按需加载。
- 添加一个统一的`activate`工具，用于加载未加载的内容。

---

## 实现要求

- 只使用大语言模型。暂不添加对Embeddings、Rerank、Apply等额外模型的支持。

---

## 工具注意事项

- 工具名应小写。

### 工具列表

- 分隔符统一使用`::`。

- `activate`：激活内容，见上文。
  - `mcp`
  - `skill`
- `fs`
  - `read`：读取文件
  - `list`/`ls`：读取目录
  - `write`：完全重写
  - `edit`：编辑已有文件
  - `delete`/`rm`：删除文件/目录
    - 删除目录时，要求目录为空。
- `mcp`
  - `xxx`：对应配置文件`mcp_servers.xxx`。
    - 注：配置名称中的特殊字符（如`-`）会映射为`::`（如`my-tool` → `mcp::my-tool`）
- `multi-agent`
   - `spawn`：生成下级Agent。
     - 参数说明：
       - `agent_id`：要生成的Agent标识（必填）
       - `task`：分配给下级Agent的任务描述（必填）
       - `model_group`：覆盖使用的模型组（可选）
       - `mcp_servers`：额外的MCP服务器列表，追加到Agent定义中的mcp_servers（可选）
       - `skills`：额外的Skills列表，追加到Agent定义中的skills（可选）
   - `send`：向指定Agent传递消息。
- `context`：上下文观测工具。
   - `observe`：查看当前上下文的详细信息。
      - 返回内容包括：
        - 消息列表（默认按ID升序，可通过sort参数覆盖排序方式）
       - 每条消息的类型（system/user/assistant/tool）
       - 每条消息的大小（字符数/预估token数）
       - 总消息数量
       - 总上下文大小
       - 系统提示词内容
       - 用户消息内容
       - 助手消息内容
       - 工具返回内容
     - 格式化为结构化的表格或列表，便于阅读
     - 支持过滤参数（如只查看特定类型的消息）
- `question`：用于向用户提问。仅限TUI运行模式下的非no-ask模式可用。
- 如果有其它需要的工具，直接补充。

### 编辑操作的额外询问

- 以下内容适用于对已有文件的编辑、重写和删除操作。
  - 适用情况：作用于文件。
  - 适用工具：如`fs::write`、`fs::edit`、`fs::delete`等。

- 以下操作用于：确认智能体是否真正了解当前的文件内容，尤其是在文件被修改的情况下。

- 额外添加一个参数`verify`，传入以下两个内容：
  - `line`：指定行号
  - `content`：该行的内容。

- `content`满足以下条件之一即可通过：
  - 完全符合指定行内容（行首空白和行尾换行符需完整匹配）。
  - 为指定行内容的前缀。此时要求给出的`content`长度不小于20个字符。
    - 如果目标行实际长度不足20个字符，则仅允许完全匹配，不支持前缀匹配。
    - 如果目标行实际长度达到或超过20个字符，允许前缀匹配，但`content`长度必须不少于20个字符（或等于整行长度，如果整行本身不足20个字符）。
  - 比较方式：默认按Unicode字符匹配；遇到非UTF-8等编码问题时，降级为字节匹配。
  - 换行符：比较时将LF与CRLF统一视为相同（即忽略行尾换行符差异）。

- 如果验证不通过，拒绝编辑，并提示Agent重新读取内容。

---

## 参考配置方式

- 配置目录支持多级查找，按优先级从高到低：
  1. **当前项目配置**：`.neoco/` 目录（项目根目录下）
  2. **当前项目配置**：`.agents/` 目录（项目根目录下）
  3. **主配置目录**：`~/.config/neoco`
  4. **通用配置目录**：`~/.agents/`
- 本节的所有"配置路径"，都是相对于上述配置目录的路径。

- 配置目录（`~/.config/neoco`）和Session目录（`~/.local/neoco`）分离的原因:
  1. **配置目录**: 存放用户配置、Agent定义等**相对静态**的内容
  2. **Session目录**: 存放运行时数据、消息历史、状态等**动态生成**的内容

- **优先级规则**（从高到低）：
  1. **当前项目** `.neoco/`
  2. **当前项目** `.agents/`
  3. **全局** `~/.config/neoco/`
  4. **全局** `~/.agents/`
   - 例如：`.agents/agents/reviewer.md` > `~/.config/neoco/agents/reviewer.md`

### 基本配置文件

- 配置路径（优先级规则如下）：
  1. **格式优先级**：TOML格式（`.toml`）始终优先于YAML格式（`.yaml`）
  2. **整体优先级**：`neoco.toml` > `neoco.<tag>.toml` > `neoco.yaml` > `neoco.<tag>.yaml`
     - 带标签的配置按`<tag>`数字/字母顺序加载，后加载的覆盖先加载的

**配置合并策略**：

- 标量类型（字符串、数字）：后加载的配置覆盖先加载的配置
- 数组类型：后加载的配置替换先加载的配置。如需追加而非替换，使用特殊语法 `"+<item>"`（例如 `models = ["+new-model"]`），其中 `<item>` 为要追加的元素。这是配置系统的特殊约定，非标准TOML语法。
- 对象类型：深层次合并（递归合并每个字段）
- 格式如下：

```toml

# 模型组定义
[model_groups.frontier]
models = ["zhipuai/glm-4.7"]
# 对应 model_providers.zhipuai （完全匹配）

[model_groups.smart]
models = ["zhipuai/glm-4.7?reasoning_effort=high"]
# 可以在这里配置模型调用参数，语法与URL参数类似

[model_groups.review]
models = ["zhipuai/glm-4.7?reasoning_effort=high&temperature=0.1"]

[model_groups.balanced]
models = ["zhipuai/glm-4.7", "minimax-cn/MiniMax-M2.5"]

[model_groups.fast]
models = ["zhipuai/glm-4.7-flashx"]

[model_groups.image]
models = ["zhipuai/glm-4.6v"]

# 以下设置应内置于代码中
[model_providers.zhipuai]
type = "openai" # 使用OpenAI Chat接口
name = "ZhipuAI"
base = "https://open.bigmodel.cn/api/paas/v4"
api_key_env = "ZHIPU_API_KEY"

# 以下设置应内置于代码中
[model_providers.zhipuai-coding-plan]
type = "openai" # 使用OpenAI Chat接口
name = "ZhipuAI Coding Plan"
base = "https://open.bigmodel.cn/api/coding/paas/v4"
api_key_env = "ZHIPU_API_KEY"

# MiniMax参考配置
[model_providers.minimax-cn]
type = "openai" # 使用OpenAI Chat接口
name = "MiniMax (CN)"
base = "https://api.minimaxi.com/v1"
api_key_envs = ["MINIMAX_API_KEY", "MINIMAX_API_KEY_2"]

# MCP参考：本地stdio形式
# 当command存在时，优先采用本地stdio形式（即使配置了url）
[mcp_servers.context7]
command = "npx"
args = ["-y", "@upstash/context7-mcp"]

[mcp_servers.context7.env]
MY_ENV_VAR = "MY_ENV_VALUE"

# MCP参考：HTTP形式
[mcp_servers.figma]
url = "https://mcp.figma.com/mcp"
bearer_token_env = "FIGMA_OAUTH_TOKEN"
http_headers = { "X-Figma-Region" = "us-east-1" }
```

#### API密钥配置（三种方式，优先级从高到低）

- **方式1（最高优先级）**: 单个环境变量 - `api_key_env = "API_KEY"`
- **方式2**: 多个环境变量（轮询使用，失败则尝试下一个） - `api_key_envs = ["API_KEY_1", "API_KEY_2"]`
  - 所有Key均失败后，视为该provider失败，触发model_group切换或错误返回
- **方式3（最低优先级，不推荐）**: 直接写入密钥 - `api_key = "sk-..."`

**优先级**: `api_key_env` > `api_key_envs` > `api_key`。若同时配置多个方式，按优先级使用最高者。

### 提示词组件定义

- 路径：`prompts/xxx.md`
- 单个Markdown文件即为一个提示词组件，用于插入提示词。
- 该Markdown文件的内容即为该组件的提示词。
- 无头部信息。`xxx`即为这个提示词组件的`name`。

#### 提示词组件头部信息

```yaml
---
id: "fs::read"  # 可选，默认为文件名（不含扩展名）。组件标识，支持::等特殊字符
---
```

| 字段 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `id` | 否 | 文件名（不含扩展名） | 组件标识，支持 `::` 等特殊字符 |

**使用规则**：
1. 提示词组件通过 Agent 定义的 `prompts` 列表显式引用
2. 所有内置提示词组件都可通过在配置目录创建同名文件进行替换
3. 如需动态启用能力，请使用 Skills 机制

**文件命名规则**：
- 文件名使用 `--` 替代 `::`（因为 `::` 无法出现在文件系统中）
- 例如：`fs::read` 对应文件 `fs--read.md`
- 通过 `id` 字段可指定不同的组件标识

#### 内置提示词组件

| 组件名 | 文件名 | 说明 |
|--------|--------|------|
| `base` | `base.md` | 基础提示词，任何时候都加载 |
| `multi-agent` | `multi-agent.md` | 多智能体提示词，可生成下级Agent时加载 |
| `multi-agent-child` | `multi-agent-child.md` | 子Agent提示词，作为子Agent时加载 |
| `fs::read` | `fs--read.md` | 文件读取工具提示词 |
| `fs::write` | `fs--write.md` | 文件写入工具提示词 |
| `fs::edit` | `fs--edit.md` | 文件编辑工具提示词 |
| `fs::delete` | `fs--delete.md` | 文件删除工具提示词 |
| `fs::list` | `fs--list.md` | 目录列表工具提示词 |
| `activate` | `activate.md` | 激活工具提示词 |
| `multi-agent::spawn` | `multi-agent--spawn.md` | 生成子Agent工具提示词 |
| `multi-agent::send` | `multi-agent--send.md` | 发送消息工具提示词 |
| `context::observe` | `context--observe.md` | 上下文观测工具提示词 |
| `context::compact` | `context--compact.md` | 上下文压缩工具提示词 |

**替换规则**：
- 所有内置提示词组件都可通过在配置目录 `prompts/` 下创建同名文件进行替换
- 例如：创建 `prompts/base.md` 替换内置 `base` 组件
- 例如：创建 `prompts/fs--read.md` 替换工具 `fs::read` 的提示词

#### 工具提示词组件

- 工具提示词组件在工具执行时自动加载，作为工具的额外上下文
- 命名格式：`prompts/工具ID--转换后.md`（`::` 转换为 `--`）
- 组件内容会在工具执行时作为上下文提供，帮助Agent正确使用工具

### Agent定义

- 路径：`agents/xxx.md`
- 单个Markdown文件即为一个Agent定义，使用YAML frontmatter定义元数据，Markdown内容作为提示词。

**提示词合并规则：**
- Markdown正文内容作为基础提示词
- frontmatter中的`prompts`列表为追加的提示词组件
- 最终提示词 = Markdown正文 + prompts列表项（按顺序追加）
- 示例：
  ```yaml
  ---
  prompts:
    - base
    - multi-agent
  ---
  # Agent 提示词内容...
  ```
  实际加载时：先加载Markdown正文，再依次追加prompts中定义的组件

#### Agent头部信息

```yaml
---
description: Reviews code for quality and best practices
mode: subagent
model: anthropic/claude-sonnet-4-20250514
temperature: 0.1
model_group: frontier
prompts:
  - base
  - multi-agent
skills:
  - rust-coding
mcp_servers:
  - filesystem
---
# Agent 提示词内容...
```

**model字段支持的两种格式：**

```yaml
# 格式1：字符串形式（简化写法）
model: anthropic/claude-sonnet-4-20250514

# 格式2：对象形式（完整配置）
model:
  provider: anthropic
  name: claude-sonnet-4-20250514
  temperature: 0.1

# 注意：model_group 与 model 同时存在时，优先使用 model_group
# 未设置model字段时：使用模型默认配置（由运行时决定具体模型）
```

**mode字段支持的格式：**

```yaml
# 格式1：字符串形式
mode: primary      # 主Agent（默认）
mode: subagent    # 子Agent

# 格式2：数组形式（多个Agent类型）
mode:
  - primary       # 主Agent
  - subagent      # 子Agent
# 或
mode:
  - subagent     # 只有子Agent

# 注意：数组不能为空，空数组无效（等同于未设置）
```

**字段说明：**

| 字段 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `id` | 否 | 文件名 | Agent标识 |
| `description` | 否 | (无) | Agent描述 |
| `mode` | 否 | `primary` | `primary` / `subagent` / 数组（不能为空，如 `[subagent]` 或 `[primary, subagent]`） |
| `model` | 否 | 使用 `model_group` | 模型配置，支持两种格式：<br>• 字符串：`"anthropic/claude-sonnet-4-20250514"`<br>• 对象：`{ provider, name, temperature }`<br>**与model_group同时存在时，优先使用model_group** |
| `temperature` | 否 | 模型默认 | 温度参数（单独指定时优先） |
| `model_group` | 否 | - | 模型组，**优先于model字段** |
| `prompts` | 否 | `[]` | 提示词列表 |
| `mcp_servers` | 否 | `[]` | MCP服务器列表 |
| `skills` | 否 | `[]` | 技能列表 |
| 其他字段 | 否 | - | 未定义的字段会被自动收集到extras中 |

**关于 `tools` 字段：** Agent不直接定义tools，而是通过`skills`来获取工具能力。如需工具定义，请使用skills机制。

**兼容现有格式：** 所有新增字段均为可选，不提供时使用默认值。

---

## 用户接口

基本的运行逻辑都一致，只在界面上有区别。

### A. 直接输入输出

传入`-m 消息内容`参数，直接执行，输出结果。

- 输出结束后也输出`--session xxxxxxxx`参考参数，用于接续对话上下文。（Session管理部分见下文）

### B. 终端TUI

- 在A的输出内容下方，添加输入框和状态显示。
  - 输入框：上下左右边框线宽1字符。支持多行输入。`Shift+Enter`换行，`Ctrl+hjkl`移动光标。
- 状态显示：固定1行。

#### 命令系统

- 输入框为空时输入`/`，出现命令补全提示。
- `Ctrl+p`打开命令面板。

#### 命令列表

- `/new`：创建新的Session。
- `/exit`：退出应用。
- `/compact`：执行上下文压缩。

### C. 后台运行模式

参考ZeroClaw项目的架构设计:

1. **守护进程**: neoco作为系统服务运行，管理Session生命周期
2. **IPC通信**: 守护进程与前端通过RESTful API交互
3. **状态暴露**: 提供HTTP API查询Session状态和进度
4. **多前端支持**: 支持CLI、Web UI、IDE插件等多种前端

- 与ZeroClaw的主要区别:
  - ZeroClaw是通用自动化工具，NeoCo专注于AI Agent协作
  - NeoCo的Session管理更复杂（支持智能体树）

#### API支持

- **Agent树查询API**：提供Agent层级结构查询接口
  - `GET /api/v1/sessions/{session_id}/agents/tree`：获取Agent树形结构
  - `GET /api/v1/sessions/{session_id}/agents/{agent_id}/messages`：获取Agent消息历史
  - `GET /api/v1/sessions/{session_id}/agents/{agent_id}/tools`：获取Agent工具调用记录
  - `GET /api/v1/sessions/{session_id}/agents/stats`：获取Agent执行统计信息
- **实时事件流**：通过WebSocket/Server-Sent Events推送状态变更
  - Agent状态变更事件
  - 工具调用开始/完成事件

- **权限设计**：
  - 默认无密钥，用户可以选择使用固定密钥。
  - 暂不实现：授权策略、跨域访问控制、速率限制。

### 用户接口要求

- 模式A和B都使用`ratatui`，且共享消息内容渲染逻辑。
  - 使用`ratatui`的`Viewport::Inline`模式（非全屏TUI）。
- 以下逻辑要求分离至不同crate：
  - 核心执行逻辑
  - 终端输出逻辑
  - 后台Agent与外部接口

---

## 总体架构设计

- 尽可能保持可扩展性。
- 避免多个类型具有相同功能。

### 总体架构设计参考

- [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw)
- [OpenFang](https://github.com/RightNow-AI/openfang)

---

## 权限隔离设计

### 权限隔离设计参考

- [OpenFang](https://github.com/RightNow-AI/openfang)

---

## 错误处理机制

1. **模型调用错误**:
   - 网络错误、API错误等: 自动重试3次，每次间隔指数退避（1s, 2s, 4s）
   - 所有重试失败后，尝试model_group中的下一个模型（按配置顺序，不循环）
   - 若model_group中所有模型都尝试失败，将错误返回给调用方（Agent）由其决定后续处理

2. **工具调用错误**:
   - 工具执行失败: 将错误信息返回给Agent，由Agent决定如何处理（重试、跳过或终止）
   - 工具超时（可配置）
     - 默认：30秒超时
     - 工具类型级别默认配置，例如`fs`为10秒，`mcp`为60秒。
     - 可以为指定类别或指定工具配置超时时间，使用前缀匹配。
     - 匹配规则：最长前缀优先（如`fs::read`匹配`fs::read`而非`fs`）

3. **配置错误**:
   - 启动时配置验证失败: 立即报错退出，不启动（仅验证配置目录文件）
   - 运行时配置热加载失败: 回滚到上一版本，记录错误日志
