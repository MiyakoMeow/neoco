# 需求文档

## 产品信息

- 名称：Neco

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

### 工作流固定问题

当前许多开发者、企业等，已经开发出了Agent独有的工作流，例如：

- PRD（需求文档、技术文档、实施计划）
- TDD（测试驱动开发）
- 让模型A开发，模型B检查

但是这个工作流仍然需要手动推进每一步，仍然有自动化空间。

并且我希望这个工作流是可以共享的。

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
  - OpenAI API调用使用`async-openai`这个crate。
- 流式输出
- 工具调用
  - 尽可能支持并行化工具调用
- 不需要支持更多功能，因为这些API就可以实现所有功能

- 为后续支持Anthropic、OpenAI Responses、OpenRouter、Github Copilot等预留接口。

### Session管理

- 存储在`~/.local/neco/(session_id)/(agent_ulid).toml`文件。该文件存储所有的上下文内容。
- **Session ID与Agent ULID的关系**：
  - Session ID是顶级容器的ULID，在创建Session时生成
  - Agent ULID是每个Agent实例的ULID，在Agent开始对话时生成（第一个Agent除外，其使用Session ID）
  - 第一个Agent（最上层）的Agent ULID与Session ID相同
- Session ID使用ULID（Universally Unique Lexicographically Sortable Identifier）。使用`ulid`这个crate。

#### 消息内容存储

- 使用TOML存储消息内容：

- 参考：`(agent_ulid).toml`。不同Agent使用不同文件。

```toml
# Agent配置
prompts = ["base"]

# Agent层级关系（用于SubAgent模式）
parent_ulid = "01HF..."  # 上级Agent的ULID，最上层Agent此字段省略不填

# Agent消息列表
[[messages]]
# 整个Session中，所有工作流节点、所有Agent的所有消息，都拥有唯一整数id。
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

- 使用`rmcp`这个crate。
- 同时支持`local`和`http`两种形式。

### Skills

- 参考：[agentskills.io](https://agentskills.io/)。
- 按需加载

### 上下文压缩

- 调用模型，关闭thinking模式，关闭所有工具支持，获取输出内容。
- 新的上下文内容，按顺序依次为：
  1. 默认激活的内容。
  2. 刚才的输出内容。
  3. 在此之前动态激活的工具、MCP、Skills。

#### 压缩触发时机

- 在当前上下文大小达到模型上下文窗口的特定百分比时，自动触发。百分比默认为`90%`。可在配置文件中，配置是否自动触发以及触发百分比。
- 可手动触发，参考下方REPL模式。

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

- 可以指定使用的Agent（来自配置目录或工作流目录的`agents`下的Agent定义）。
- 默认可以覆盖`model`、`model_group`、`prompts`等字段。
  - 来源优先级：Agent定义可来自配置目录或工作流目录的agents定义，优先级为工作流目录 > 配置目录（即工作流目录中的定义覆盖配置目录）。详细规则见后文“Agent查找优先级”部分。

### 自定义工作流

- 在没有定义工作流时，默认工作流只有一个节点。

- 使用Mermaid图 + 每个节点一个.md文件表示node

- 多个节点可以同时运行。
- 如果箭头有出节点，则必须调用出节点工具，该节点才能结束运行。
  - 使用转场工具`workflow::<option>`触发下游节点（`workflow::pass`表示无条件传递）。
  - 边条件：`select`触发时计数器+1，`require`要求计数器>0才能执行。
  - 转场时需带上`message`参数传递信息内容。
  - **增强条件系统（表达式引擎）**：支持复杂的表达式条件，基于`rhai`引擎实现：
    - 表达式语法：支持算术运算、比较运算、逻辑运算、变量引用
    - 变量系统：工作流Session存储全局变量，节点Session存储局部变量
    - 内置函数：支持字符串处理、数值计算、数组操作等常用函数
    - 条件示例：`if: approval_count >= 2 && quality_score > 0.7`

- 节点选项：
  - new-session表示为该节点创建一个新的节点Session（归属于工作流Session），而非复用已有节点Session

#### 工作流Session层次结构

- 工作流Session：存储工作流状态（计数器、全局变量）
- 节点Session：工作流Session的子Session，存储节点执行上下文
- `new-session`创建的节点Session自动关联到工作流Session

#### 工作流组合与复用

- **子工作流（Sub-workflow）**：支持将工作流作为节点嵌入其他工作流
  - 子工作流定义在独立的目录中，拥有自己的配置、Agent和提示词
  - 子工作流执行时创建独立的工作流Session，与父工作流Session关联
  - 子工作流可接收父工作流传递的参数，并返回执行结果

- **参数化工作流模板**：支持创建可配置的工作流模板
  - 模板定义：在工作流根目录添加`template.toml`，声明参数和默认值
  - 参数传递：实例化时提供参数值，替换模板中的占位符
  - 条件参数化：条件表达式可使用模板参数，实现动态条件
  - 示例：创建可配置的代码审查工作流，参数包括最少审查人数、质量阈值等

- **工作流库与共享**：
  - 支持从Git仓库或其他源导入工作流模板
  - 提供工作流模板注册和发现机制
  - 支持工作流模板版本管理

#### 使用示例

- 定义PRD流程
- 执行/审阅循环流程

#### 重要概念：双层结构区分

Neco系统中存在**两个独立的层次结构**，它们在不同层面运作：

##### **1. 工作流节点之间的图结构（Workflow-Level Graph）**

- **定义方式**：通过Mermaid图（`workflow.mermaid`）静态定义
- **结构类型**：有向图（DAG），节点之间通过边（edges）连接
- **转换控制**：由边条件（`select`/`require`计数器）控制节点之间的转换
- **存储位置**：工作流Session存储计数器、全局变量
- **生命周期**：工作流启动时创建，工作流完成时销毁
- **示例**：`WRITE_PRD --> REVIEW_PRD`（节点之间的转换）

##### **2. 单个节点下的Agent树结构（Node-Level Agent Tree）**

- **定义方式**：运行时动态创建（Agent实例化）
- **结构类型**：树形结构，Agent之间通过`parent_ulid`建立上下级关系
- **协作方式**：上下级Agent通过通信工具直接传递内容
- **存储位置**：节点Session下的Agent TOML文件
- **生命周期**：节点启动时创建Agent树，节点完成时销毁
- **示例**：上级Agent创建多个下级Agent并行研究不同主题

##### **关键区别**

- 工作流图定义"**做什么任务**"（任务编排）
- Agent树定义"**怎么做任务**"（任务执行）
- 工作流边控制**节点之间**的转换，不控制**Agent之间**的关系
- `parent_ulid`用于Agent树的上下级关系，不用于工作流节点之间的关系

##### **重要补充：工作流节点Agent定位**

- **工作流的节点Agent同时也是节点内的最上级Agent**。
  - **节点Agent的ULID与节点Session ID相同**（遵循前述Session ID与Agent ULID关系规则）。
  - 消息存储路径为`~/.local/neco/(workflow_session_id)/(node_session_id).toml`（通用路径格式见前述Session管理部分，此处workflow_session_id对应通用格式中的session_id，node_session_id对应agent_ulid）。

### 模块化提示词与工具，以及按需加载

- 模块化提示词、工具、MCP、Skills等实例的目的是，支持内容的按需加载。
- 添加一个统一的`Activate`工具，用于加载未加载的内容。

---

## 实现要求

- 只使用大语言模型。暂不添加对Embeddings、Rerank、Apply等额外模型的支持。

---

## 工具注意事项

- 工具名应小写。

### 工具列表

- 分隔符统一使用`::`。
  - 注：此规则适用于所有工具，包括前面提到的`workflow`工具（例如`workflow::<option>`、`workflow::pass`）。

- `activate`：激活内容，见上文。
  - `mcp`
  - `skill`
- `fs`
  1. `read`
  2. `write`：完全重写
  3. `edit`：编辑已有文件
  4. `delete`：删除文件
- `mcp`
  - `xxx`：对应配置文件`mcp_servers.xxx`。
- `multi-agent`
  - `spawn`：生成下级Agent。
  - `send`：向指定Agent传递消息。
- `question`：用于向用户提问。仅限REPL运行模式下的非no-ask模式可用。
- 如果有其它需要的工具，直接补充。

### 工具：读取文件

- 工具名：`fs::read`。

#### Hashline

- 实现 Hashline 技术。Agent 读到的每一行代码，开头都会加上一个强绑定的内容哈希值（行哈希）。
  - 格式类似下文的`AKVK`，称为“行哈希”。
    - 行哈希位于行首，作为前缀（格式为“哈希|内容”）。

```text
AKVK|function hello() {
VNXJ|  return "world";
AIMB|}
```

- 假设当前行号为`N`，则每一行的哈希值来源于：
  - 将第`MAX(N-4,1)`行到第`N`行的内容合并为一个字符串，再计算哈希值。
  - 使用xxHash方法和`xxhash-rust`这个crate。

- 特别注意：在工具介绍中，将Hashline机制解释清楚。

##### 哈希冲突处理

- 在**读取文件**时，如果检测到哈希冲突（不同内容产生相同哈希值），使用以下算法调整：
  1. 初始哈希使用 `xxhash(content_window, seed=0)`
  2. 若冲突，使用 `xxhash(content_window, seed=1)`，依次递增 seed
  3. 重复直到获得唯一哈希（seed 上限为 255，超过则报错）
- 确保每个行哈希在文件中唯一标识其内容。

### 工具：编辑文件

- 工具名：`fs::edit`。
- 传入开始行哈希和结束行哈希（都是闭区间），以及修改后的内容。
  - 必须同时匹配起始行哈希和结束行哈希（精确匹配）才能应用编辑。
  - 如果哈希匹配失败（例如文件已更改或哈希冲突），返回明确的冲突错误给调用方Agent。
  - 冲突策略：在接收到开始行哈希和结束行哈希及修改内容时，先在目标文件中查找所有匹配位置，只有在“唯一命中”时才应用修改并返回成功，否则不做修改并返回冲突/错误状态。

---

## 参考配置方式

- 配置目录：`~/.config/neco`
- 本节的所有“配置路径”，都是相对于配置目录的路径。

- 配置目录（\`~/.config/neco\`）和Session目录（\`~/.local/neco\`）分离的原因:
  1. **配置目录**: 存放用户配置、Agent定义、工作流定义等**相对静态**的内容
  2. **Session目录**: 存放运行时数据、消息历史、状态等**动态生成**的内容

### 基本配置文件

- 配置路径（优先级规则如下）：
  1. **格式优先级**：TOML格式（`.toml`）始终优先于YAML格式（`.yaml`）
  2. **整体优先级**：`neco.toml` > `neco.<tag>.toml` > `neco.yaml` > `neco.<tag>.yaml`
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
- **方式3（最低优先级，不推荐）**: 直接写入密钥 - `api_key = "sk-..."`

**优先级**: `api_key_env` > `api_key_envs` > `api_key`。若同时配置多个方式，按优先级使用最高者。

### 提示词组件定义

- 路径：`prompts/xxx.md`
- 单个Markdown文件即为一个提示词组件，用于插入提示词。
- 该Markdown文件的内容即为该组件的提示词。
- 无头部信息。`xxx`即为这个提示词组件的`name`。

#### 内置提示词组件

- `base`：任何时候都加载。包含如何加载未加载的内容的提示。
- `multi-agent`：如果这个Agent可以生成下级Agent，则加载。
- `multi-agent-child`：如果这个模型有上级Agent，则加载。

#### 工具提示词组件

- 在工具定义处，随工具加载。

### Agent定义

- 路径：`agents/xxx.md`
- 单个Markdown文件即为一个Agent定义。
- 该Markdown文件的内容即为该Agent的提示词。

#### Agent头部信息

```yaml
# （可选）激活的提示词组件。按顺序激活。
# 如果未定义此字段，默认只加载`base`组件
prompts:
  - base
  - multi-agent 
```

### 工作流定义

- 工作流根路径：`workflows/xxx/`

- 类似配置目录，可以单独为工作流配置`neco.toml`、`prompts`、`agents`、`skills`等。

#### 参考：PRD工作流

- 相对工作流根路径的路径：`workflow.mermaid`

```mermaid
flowchart TD
    START([开始]) --> WRITE_PRD[write-prd]

    WRITE_PRD --> REVIEW_PRD[review / new-session]
    REVIEW_PRD -->|select:approve_prd,reject| WRITE_PRD
    WRITE_PRD -->|require:approve_prd| WRITE_TECH_DOC[write-tech-doc]

    WRITE_TECH_DOC --> REVIEW_TECH_DOC[review / new-session]
    REVIEW_TECH_DOC -->|select:approve_tech,reject| WRITE_TECH_DOC
    WRITE_TECH_DOC -->|require:approve_tech| WRITE_IMPL[write-impl]
    
    WRITE_IMPL --> REVIEW_IMPL[review / new-session]
    REVIEW_IMPL -->|select:approve,reject| WRITE_IMPL
    REVIEW_IMPL -->|require:approve| END([完成])
```

- **Agent查找优先级**：
  1. `workflows/xxx/agents/`（工作流特定，优先）
  2. `~/.config/neco/agents/`（全局配置，后备）
  同名Agent：工作流特定覆盖全局配置

- 此时，根据该PRD工作流节点配置，工作流目录或配置目录的`agents`目录，应该有：
  1. `write-prd.md`
  2. `write-tech-doc.md`
  3. `write-impl.md`
  4. `review.md`

---

## 用户接口

基本的运行逻辑都一致，只在界面上有区别。

### A. 直接输入输出

传入`-m 消息内容`参数，直接执行，输出结果。

- 输出结束后也输出`--session xxxxxxxx`参考参数，用于接续对话上下文。（Session管理部分见下文）

### B. 终端REPL

- 在A的输出内容下方，添加输入框和状态显示。
  - 输入框：上下左右边框线宽1字符。支持多行输入。`Shift+Enter`换行，`Ctrl+hjkl`移动光标。
  - 状态显示：固定1行。

#### 工作流与Agent树可视化

- **工作流状态显示**：在状态显示区域下方添加工作流可视化面板
  - 显示当前工作流图结构，高亮当前活动节点
  - 显示节点状态：等待、执行中、成功、失败、跳过
  - 显示边条件状态：计数器值、表达式求值结果
  - 支持缩放和平移，适应复杂工作流

- **Agent树形结构显示**：在消息历史区域右侧添加Agent树面板
  - 树状显示当前节点内的Agent层级关系
  - 显示每个Agent的状态：活跃、等待、完成、错误
  - 显示Agent间通信关系和消息统计
  - 支持展开/折叠节点，查看详细状态

- **交互操作**：
  - 点击工作流节点：查看节点详细信息和执行历史
  - 点击Agent节点：查看Agent消息记录和工具调用历史
  - 快捷键：`Ctrl+w`切换工作流面板，`Ctrl+a`切换Agent树面板
  - 实时更新：工作流状态变化时自动刷新显示

- **命令扩展**：
  - `/workflow status`：显示工作流详细状态
  - `/workflow graph`：导出工作流图为Mermaid或图片
  - `/agents tree`：显示Agent树详细结构
  - `/agents stats`：显示Agent执行统计信息

- 工作流、Agent树的TUI显示设计，延后至设计阶段。

#### 命令系统

- 输入框为空时输入`/`，出现命令补全提示。
- `Ctrl+p`打开命令面板。

#### 命令列表

- `/new`：创建新的Session。
- `/exit`：退出应用。
- `/compact`：执行上下文压缩。

### C. 后台运行模式

参考ZeroClaw项目的架构设计:

1. **守护进程**: neco作为系统服务运行，管理Session生命周期
2. **IPC通信**: 使用gRPC或Unix Socket与前端交互
3. **状态暴露**: 提供HTTP API查询Session状态和进度
4. **多前端支持**: 支持CLI、Web UI、IDE插件等多种前端

- 与ZeroClaw的主要区别:
  - ZeroClaw是通用自动化工具，Neco专注于AI Agent协作
  - Neco的Session管理更复杂（支持智能体树）

#### API支持

- **工作流状态API**：提供RESTful/gRPC接口查询工作流执行状态
  - `GET /api/v1/workflows/{workflow_id}/status`：获取工作流整体状态
  - `GET /api/v1/workflows/{workflow_id}/graph`：获取工作流图结构（Mermaid/JSON格式）
  - `GET /api/v1/workflows/{workflow_id}/nodes/{node_id}/status`：获取节点详细状态
  - `GET /api/v1/workflows/{workflow_id}/variables`：获取工作流变量和表达式求值结果
  - `POST /api/v1/workflows/{workflow_id}/control`：控制工作流执行（暂停、继续、终止）
- **Agent树查询API**：提供Agent层级结构查询接口
  - `GET /api/v1/sessions/{session_id}/agents/tree`：获取Agent树形结构
  - `GET /api/v1/sessions/{session_id}/agents/{agent_id}/messages`：获取Agent消息历史
  - `GET /api/v1/sessions/{session_id}/agents/{agent_id}/tools`：获取Agent工具调用记录
  - `GET /api/v1/sessions/{session_id}/agents/stats`：获取Agent执行统计信息
- **实时事件流**：通过WebSocket/Server-Sent Events推送状态变更
  - 工作流节点状态变更事件
  - Agent状态变更事件
  - 工具调用开始/完成事件
  - 条件表达式求值结果事件

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
   - 若model_group中所有模型都尝试失败，将错误返回给调用方（Agent/workflow节点）由其决定后续处理

2. **工具调用错误**:
   - 工具执行失败: 将错误信息返回给Agent，由Agent决定如何处理（重试、跳过或终止）
   - Agent对工具错误的最终决定即为节点状态（无需额外workflow配置介入）
   - 工具超时（可配置）
     - 默认：30秒超时
     - 工具类型级别默认配置，例如`fs`为10秒，`mcp`为60秒。
     - 可以为指定类别或指定工具配置超时时间，使用前缀匹配。

3. **配置错误**:
   - 启动时配置验证失败: 立即报错退出，不启动
   - 运行时配置热加载失败: 回滚到上一版本，记录错误日志

4. **工作流错误**:
   - 节点执行失败: 根据workflow配置决定是否继续或中断
   - 死锁检测: 超过5分钟无进度时，中断工作流并报错
