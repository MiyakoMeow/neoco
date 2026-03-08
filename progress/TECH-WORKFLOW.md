# TECH-WORKFLOW: 工作流模块

本文档描述NeoCo项目的工作流模块设计，采用领域驱动设计，分离工作流定义与运行时状态。

## 1. 模块概述

工作流模块实现了一个基于有向图工作流的引擎，支持节点并行执行、条件转换、循环控制和状态管理。

**设计原则：**
- 工作流定义（WorkflowDef）不含运行时状态
- 工作流运行时（WorkflowRuntime）不含执行逻辑
- 引擎负责协调，不持有状态

## 2. 核心概念

### 2.1 双层架构

```mermaid
graph TB
    subgraph "工作流定义层（静态）"
        WD[WorkflowDef<br/>nodes + edges]
    end
    
    subgraph "工作流运行时层（动态）"
        WR[WorkflowRuntime<br/>node_states + counters]
    end
    
    subgraph "执行层"
        E[WorkflowEngine<br/>协调执行]
    end
    
    WD --> E
    E --> WR
```

**关键理解：**
- **工作流图**：定义"做什么任务"（任务编排）
- **Agent树**：定义"怎么做任务"（任务执行）
- **工作流边**：控制节点之间的转换
- **Agent层级**：通过`parent_ulid`建立上下级关系

### 2.2 工作流Session层次

```mermaid
graph TB
    subgraph "工作流Runtime"
        WR[WorkflowRuntime<br/>- 计数器<br/>- 全局变量<br/>- 节点状态]
    end
    
    subgraph "节点Runtime"
        NR1[NodeRuntime: write-prd<br/>- Agent<br/>- 消息]
        NR2[NodeRuntime: review-prd<br/>- Agent<br/>- 消息]
    end
    
    WR --> NR1
    WR --> NR2
```

## 3. 工作流领域模型

### 3.1 工作流定义（静态配置）

```rust
/// 工作流定义（静态配置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub params: WorkflowParams,
    pub nodes: Vec<NodeDefinition>,
    pub edges: Vec<EdgeDefinition>,
}

/// 工作流参数
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowParams(pub HashMap<String, Value>);

/// 节点定义
/// 
/// **节点ID设计说明：**
/// - 工作流定义中使用 **字符串（kebab-case）** 作为节点标识，如 "write-prd"
/// - 此设计与 REQUIREMENT.md 保持一致，便于人类理解和配置文件编写
/// - TODO: 与 TECH.md 中的 NodeUlid 进行区分：
///   - NodeUlid（TECH.md）用于运行时内部标识节点实例
///   - 字符串ID（本文档）用于工作流定义和配置文件
///   - 具体实现细节待确认：可能需要在运行时建立两者之间的映射
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDefinition {
    pub id: String,  // 使用kebab-case字符串ID，如"write-prd"
    #[serde(default)]
    pub agent: Option<String>,  // Agent标识，默认使用id作为agent标识
    #[serde(default)]
    pub new_session: bool,
}

/// 转场选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub option: String,
}

/// 需求条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    pub option: String,
    pub min_count: u32,
    #[serde(default)]
    pub param_ref: Option<String>,
}

/// 边定义
/// 
/// **节点ID说明**：from/to 字段使用字符串（kebab-case）作为节点引用，
/// 与 NodeDefinition.id 保持一致。"END" 是特殊关键字，表示工作流结束。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeDefinition {
    pub from: String,  // 节点字符串ID（对应NodeDefinition.id）
    pub to: String,    // 节点字符串ID，"END"表示工作流结束
    #[serde(default)]
    pub select: Option<Vec<SelectOption>>,  // select触发时计数器+1
    #[serde(default)]
    pub require: Option<Vec<Requirement>>,  // require要求计数器>0
}

// 节点ID使用字符串（kebab-case），如"write-prd"
// 不再使用ULID，保持与需求文档一致
// TODO: 运行时可能需要将字符串ID映射为NodeUlid（见TECH.md定义）
```

### 3.2 工作流运行时（动态状态）

```rust
/// 工作流运行时状态
/// 
/// **节点ID说明**：node_states 和 active_nodes 使用字符串（kebab-case）作为键，
/// 与工作流定义中的 NodeDefinition.id 保持一致。
/// TODO: 运行时内部可选择使用 NodeUlid（见TECH.md）作为实例标识，但HashMap键仍使用字符串ID
#[derive(Debug, Clone)]
pub struct WorkflowRuntime {
    pub session_ulid: SessionUlid,
    definition: Arc<WorkflowDefinition>,
    node_states: DashMap<String, NodeRuntimeState>,  // Key: 节点字符串ID（kebab-case）
    counters: DashMap<CounterKey, u32>,
    variables: DashMap<VariableKey, Value>,
    active_nodes: DashSet<String>,  // 节点字符串ID集合（kebab-case）
    transition_messages: DashMap<String, String>,  // Key: 节点字符串ID
    status: WorkflowStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    last_progress_at: DateTime<Utc>,                 // 最后进度时间，用于死锁检测
}

/// 计数器键（强类型）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CounterKey(String);

impl CounterKey {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

/// 变量键（强类型）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VariableKey(String);

impl VariableKey {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl WorkflowRuntime {
    pub fn new(
        session_ulid: SessionUlid,
        definition: WorkflowDefinition,
    ) -> Self {
        // TODO: 实现工作流运行时初始化
        // 1. 接收session_ulid和definition作为参数
        // 2. 初始化空的active_nodes HashSet<String>
        // 3. 初始化空的node_states HashMap<String, NodeRuntimeState>
        // 4. 初始化空的counters HashMap<String, u32>
        // 5. 初始化空的transition_messages DashMap<String, String>
        // 6. 设置status为WorkflowStatus::Ready
        // 7. 设置created_at和updated_at为当前UTC时间
        // 8. 设置last_progress_at为当前UTC时间（用于死锁检测）
        unimplemented!()
    }
    
    pub fn start_node(&mut self, node_id: &str, agent_ulid: AgentUlid) {
        // TODO: 实现节点启动逻辑
        // 1. 检查节点是否已在active_nodes中
        // 2. 创建NodeRuntimeState::Running { agent_ulid }
        // 3. 将状态插入node_states
        // 4. 将node_id加入active_nodes
        // 5. 更新updated_at和last_progress_at为当前时间
        unimplemented!()
    }
    
    pub fn complete_node(&mut self, node_id: &str, output: String) {
        // TODO: 实现节点完成逻辑
        // 1. 更新node_states中该节点的状态为Success { output }
        // 2. 从active_nodes HashSet中移除该node_id
        // 3. 更新updated_at和last_progress_at为当前时间
        todo!()
    }
    
    pub fn increment_counter(&mut self, option: &str) {
        // TODO: 实现计数器递增逻辑（select触发时调用）
        // 1. 使用CounterKey包装option
        // 2. 使用counters.entry(key).or_insert(0)获取或创建计数器
        // 3. 对获取的可变引用执行加1操作
        // 4. 更新last_progress_at为当前时间
        unimplemented!()
    }
    
    pub fn get_counter(&self, option: &str) -> u32 {
        // TODO: 实现获取计数器值逻辑（require评估时调用）
        // 1. 使用CounterKey包装option
        // 2. 调用counters.get(key)查找计数器
        // 3. 如果Some(v)返回*v，否则返回0
        unimplemented!()
    }
    
    pub fn check_deadlock(&self, timeout_minutes: u64) -> bool {
        // TODO: 死锁检测：检查是否超过指定时间无进度
        // 1. 获取当前UTC时间
        // 2. 计算当前时间与last_progress_at的差值
        // 3. 如果差值超过timeout_minutes分钟，返回true（检测到死锁）
        // 4. 否则返回false
        todo!()
    }
}

/// 节点运行时状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeRuntimeState {
    Waiting,
    Running { agent_ulid: AgentUlid },
    Success { output: String },
    Failed { error: String },
    Skipped,
}

/// 工作流状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowStatus {
    Ready,
    Running,
    Paused,
    Completed,
    Failed,
}
```

## 3.2 仓储接口

```rust
use async_trait::async_trait;
use dashmap::{DashMap, DashSet};

/// 工作流Session存储设计
/// 
/// 工作流Session存储工作流运行时状态，包括：
/// 
/// 1. **计数器（select/require）**
///    - `counters: DashMap<CounterKey, u32>`
///    - select触发时：调用`increment_counter(option)`使计数器+1
///    - require评估时：调用`get_counter(option)`获取计数器值，要求>0
///    - 计数器在工作流全局作用域内共享，不同边的相同选项名共享同一计数器
/// 
/// 2. **全局变量**
///    - `variables: DashMap<VariableKey, Value>`
///    - 存储工作流级别的共享数据，如`initial_input`等
///    - 可通过`@params.<param_name>`在边条件中引用workflow_params
/// 
/// 3. **节点执行上下文**
///    - `node_states: DashMap<String, NodeRuntimeState>`：节点状态（Waiting/Running/Success/Failed/Skipped）
///      - Key使用字符串（kebab-case），与NodeDefinition.id保持一致
///    - `active_nodes: DashSet<String>`：当前活动节点集合（kebab-case字符串ID）
///    - `transition_messages: DashMap<String, String>`：节点转场时传递的消息（Key为kebab-case字符串ID）
/// 
/// 4. **死锁检测**
///    - `last_progress_at: DateTime<Utc>`：记录最后进度时间
///    - 超过5分钟（DEADLOCK_TIMEOUT_MINUTES）无进度时触发中断

/// 工作流仓储接口
#[async_trait]
pub trait WorkflowRepository: Send + Sync {
    async fn save(&self, runtime: &WorkflowRuntime) -> Result<(), StorageError>;
    async fn find_by_id(&self, session_ulid: &SessionUlid) -> Result<Option<WorkflowRuntime>, StorageError>;
    async fn find_by_status(&self, status: WorkflowStatus) -> Result<Vec<WorkflowRuntime>, StorageError>;
    async fn delete(&self, session_ulid: &SessionUlid) -> Result<(), StorageError>;
}

/// 存储错误类型
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("数据未找到: {0}")]
    NotFound(String),
    
    #[error("数据库错误: {0}")]
    Database(String),
    
    #[error("序列化错误: {0}")]
    Serialization(String),
}
```

## 3.3 事件类型

```rust
/// 工作流事件类型
/// 
/// **节点ID说明**：事件中的 node_id 字段使用字符串（kebab-case）格式，
/// 与工作流定义中的节点ID保持一致。
/// TODO: 与 TECH.md 中的 NodeUlid 进行区分，具体映射关系待实现确认。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkflowEvent {
    WorkflowStarted {
        session_ulid: SessionUlid,
        definition_id: String,
    },
    NodeStarted {
        session_ulid: SessionUlid,
        node_id: String,  // 节点字符串ID（kebab-case），对应NodeDefinition.id
        agent_ulid: AgentUlid,
    },
    NodeCompleted {
        session_ulid: SessionUlid,
        node_id: String,  // 节点字符串ID（kebab-case）
        output: String,
    },
    NodeFailed {
        session_ulid: SessionUlid,
        node_id: String,  // 节点字符串ID（kebab-case）
        error: String,
    },
    NodeTransitionIntent {
        session_ulid: SessionUlid,
        node_id: String,  // 节点字符串ID（kebab-case）
        message: Option<String>,
    },
    EdgeTriggered {
        session_ulid: SessionUlid,
        from: String,  // 源节点字符串ID
        to: String,    // 目标节点字符串ID
        option: Option<String>,
    },
    WorkflowCompleted {
        session_ulid: SessionUlid,
    },
    WorkflowFailed {
        session_ulid: SessionUlid,
        reason: String,
    },
    DeadlockDetected {
        session_ulid: SessionUlid,
        timeout_minutes: u64,
    },
}

pub trait EventPublisher: Send + Sync {
    async fn publish(&self, event: WorkflowEvent) -> Result<(), WorkflowError>;
}
```

## 4. 工作流引擎

### 4.1 引擎核心

```rust
/// 工作流引擎
/// 
/// 引擎协调工作流执行，负责节点调度和状态管理。
/// 事件发布见 [TECH-SESSION.md#3-消息模型设计](TECH-SESSION.md#3-消息模型设计)
/// 
/// **节点ID说明**：引擎内部使用字符串（kebab-case）作为节点标识，
/// 与工作流定义中的 NodeDefinition.id 保持一致。
/// TODO: 与 TECH.md 中的 NodeUlid 进行区分，运行时可能需要建立字符串ID到NodeUlid的映射
pub struct WorkflowEngine {
    agent_engine: Arc<AgentEngine>,
    event_publisher: Arc<dyn EventPublisher>,
    workflow_repository: Arc<dyn WorkflowRepository>,
}

impl WorkflowEngine {
    pub async fn start_workflow(
        &self,
        definition: WorkflowDefinition,
        initial_input: String,
    ) -> Result<WorkflowRuntime, WorkflowError> {
        // TODO: 实现工作流启动逻辑
        // 1. 调用WorkflowRuntime::new创建运行时实例
        // 2. 将initial_input存入runtime.variables，键名为"initial_input"
        // 3. 调用find_start_nodes查找所有起始节点
        // 4. 对每个起始节点创建Agent，并将initial_input作为第一条消息发送
        // 5. 发布WorkflowStarted事件到event_publisher
        // 6. 返回创建的runtime
        unimplemented!()
    }
    
    pub async fn handle_node_complete(
        &self,
        runtime: &mut WorkflowRuntime,
        node_id: &str,
        output: String,
    ) -> Result<(), WorkflowError> {
        // TODO: 实现节点完成处理
        // 1. 调用runtime.complete_node更新节点状态
        // 2. 调用evaluate_edges查找满足条件的出边
        // 3. 对每个目标节点调用agent_engine启动Agent
        // 4. 发布NodeCompleted事件
        // 5. 检查是否所有节点都已完成，若是则发布WorkflowCompleted
        unimplemented!()
    }
    
    pub fn find_start_nodes(
        &self,
        definition: &WorkflowDefinition,
    ) -> Vec<String> {
        // TODO: 查找起始节点
        // 1. 创建HashSet收集所有有入边的节点ID
        // 2. 遍历所有edges，将to字段加入HashSet
        // 3. 遍历所有nodes，返回不在HashSet中的节点（无入边的节点）
        // 4. 返回节点ID字符串列表
        todo!()
    }
    
    pub fn evaluate_edges(
        &self,
        runtime: &WorkflowRuntime,
        current_node: &str,
    ) -> Vec<String> {
        // TODO: 评估边的条件以确定下一个节点
        // 1. 查找定义中从current_node出发的所有边
        // 2. 对每条边调用evaluate_requirement评估条件
        // 3. 收集所有条件满足的边的target节点
        // 4. 返回目标节点ID字符串列表
        todo!()
    }
}
```

### 4.2 执行流程

工作流引擎通过以下步骤协调节点执行：

```mermaid
sequenceDiagram
    participant User
    participant Engine as WorkflowEngine
    participant Runtime as WorkflowRuntime
    participant Agent as AgentEngine
    participant Store as Repository

    User->>Engine: start_workflow(def, input)
    Engine->>Runtime: 创建WorkflowRuntime
    Engine->>Store: save(runtime)
    Engine->>Runtime: find_start_nodes()
    Engine->>Agent: 启动起始节点
    Agent->>Runtime: NodeStarted事件
    Runtime-->>Store: 更新状态
    
    Note over Agent: 节点执行中...
    
    Agent-->>Engine: 节点完成回调
    Engine->>Runtime: handle_node_complete()
    Engine->>Runtime: evaluate_edges()
    
    alt 有后续节点
        Engine->>Agent: 触发下一节点
        Agent->>Runtime: NodeStarted事件
    else 无后续节点
        Engine->>Runtime: 标记工作流完成
        Engine->>Runtime: WorkflowCompleted事件
    end
    
    Engine-->>User: 返回runtime
```

**执行步骤说明：**

1. **启动工作流**：创建运行时实例，保存到存储，查找并启动起始节点
2. **节点执行**：Agent引擎负责执行具体任务
3. **边评估**：节点完成后，引擎评估边条件确定下一节点
4. **状态更新**：每个事件都会触发运行时状态更新和持久化

## 5. 边条件控制

### 5.1 条件语法

```toml
# select：触发时计数器+1，可多次累加
[[edges]]
from = "review-prd"
to = "write-prd"
select = [{ option = "reject" }]  # 触发时 counters.reject += 1

# require：要求计数器>0才能执行
[[edges]]
from = "write-prd"
to = "write-tech-doc"
require = [
  { option = "approve_prd", min_count = 1 }  # 需要 counters.approve_prd >= 1
]

# 参数引用：使用 @params.<param_name> 格式
[[edges]]
from = "review-prd"
to = "final-approve"
require = [
  { option = "@params.min_approvers", min_count = 1 }  # 引用workflow_params.min_approvers
]
```

### 5.2 条件评估实现

```rust
impl WorkflowEngine {
    fn evaluate_requirement(
        req: &Requirement,
        counters: &HashMap<String, u32>,
        params: &WorkflowParams,
    ) -> bool {
        // TODO: 实现需求条件评估逻辑
        // 1. 检查req.param_ref是否以"@params."开头
        // 2. 如果是参数引用：从params中提取对应的参数值作为threshold
        // 3. 如果不是：使用req.min_count作为threshold
        // 4. 从counters中获取req.option对应的计数器值
        // 5. 比较计数器值是否 >= threshold，返回比较结果
        unimplemented!()
    }
}
```

## 6. 转场工具

### 6.1 workflow工具

```rust
pub struct WorkflowTransitionTool {
    runtime: Arc<RwLock<WorkflowRuntime>>,
    node_id: String,
}

#[async_trait]
impl ToolExecutor for WorkflowTransitionTool {
    fn definition(&self) -> &ToolDefinition {
        static DEF: Lazy<ToolDefinition> = Lazy::new(|| ToolDefinition {
            id: ToolId::new("workflow", "option"),
            description: "控制工作流节点之间的转换".into(),
            schema: json!({
                "type": "object",
                "properties": {
                    "option": {
                        "type": "string",
                        "description": "转场选项"
                    },
                    "message": {
                        "type": "string",
                        "description": "传递给下一节点的消息"
                    }
                },
                "required": ["option"]
            }),
            capabilities: ToolCapabilities::default(),
            timeout: Duration::from_secs(30),
            category: ToolCategory::Common,
        });
        &DEF
    }
    
    async fn execute(
        &self,
        context: &ToolContext,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        // TODO: 实现转场工具逻辑
        // 1. 从args中解析option（必选）和message（可选）
        // 2. 获取runtime的write lock
        // 3. 调用runtime.increment_counter(option)增加计数
        // 4. 发布NodeTransition事件
        // 5. 返回转场成功的ToolResult
        unimplemented!()
    }
}

/// 无条件转场工具 - 对应需求文档的 workflow::pass
pub struct PassTool {
    runtime: Arc<RwLock<WorkflowRuntime>>,
    node_id: String,
}

#[async_trait]
impl ToolExecutor for PassTool {
    fn definition(&self) -> &ToolDefinition {
        static DEF: Lazy<ToolDefinition> = Lazy::new(|| ToolDefinition {
            id: ToolId::new("workflow", "pass"),
            description: "记录无条件转场意图，不直接触发后续节点（由引擎统一评估）".into(),
            schema: json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "传递给下一节点的消息（可选）"
                    }
                },
                "required": []
            }),
            capabilities: ToolCapabilities::default(),
            timeout: Duration::from_secs(30),
            category: ToolCategory::Common,
        });
        &DEF
    }
    
    async fn execute(
        &self,
        context: &ToolContext,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        // 记录转场意图，不直接触发后续节点
        // 1. 从args中解析message（可选）
        // 2. 获取runtime的write lock
        // 3. 将message存入runtime的转场消息存储（不触发后续节点）
        // 4. 发布NodeTransitionIntent事件（而非NodeTransition）
        // 5. 返回转场意图已记录的结果
        //
        // 注意：后续节点的触发统一由引擎在handle_node_complete()中处理，
        // 这样可以避免同一条出边的双重调度风险。
        unimplemented!()
    }
}

/// 注册工作流工具
pub async fn register_workflow_tools(
    registry: &dyn ToolRegistry,
    runtime: Arc<RwLock<WorkflowRuntime>>,
    node_id: &str,
) {
    // TODO: 注册工作流相关工具
    // 1. 注册 workflow 工具（WorkflowTransitionTool）
    // 2. 注册 pass 工具（PassTool）
    // 注意：使用异步接口与 ToolRegistry 保持一致
    todo!()
}
```

## 7. 工作流定义示例

```toml
# workflows/prd/workflow.toml
name = "PRD工作流"
description = "产品需求文档生成与审阅流程"

[workflow_params]
min_approvers = 2
quality_threshold = 0.7

# 节点定义
[[nodes]]
id = "write-prd"
new_session = false

[[nodes]]
id = "review-prd"
agent = "review"
new_session = true

[[nodes]]
id = "write-tech-doc"
new_session = false

[[nodes]]
id = "review-tech-doc"
agent = "review"
new_session = true

[[nodes]]
id = "write-impl"
new_session = false

[[nodes]]
id = "review-impl"
agent = "review"
new_session = true

# 边定义
[[edges]]
from = "write-prd"
to = "review-prd"

[[edges]]
from = "review-prd"
to = "write-prd"
select = [{ option = "reject" }]  # 触发时 counters.reject += 1

[[edges]]
from = "review-prd"
to = "write-tech-doc"
require = [
  { option = "approve_prd", min_count = 1 }  # 需要 counters.approve_prd >= 1
]

[[edges]]
from = "write-tech-doc"
to = "review-tech-doc"

[[edges]]
from = "review-tech-doc"
to = "write-tech-doc"
select = [{ option = "reject" }]

[[edges]]
from = "review-tech-doc"
to = "write-impl"
require = [
  { option = "approve_tech", min_count = 1 }
]

[[edges]]
from = "write-impl"
to = "review-impl"

[[edges]]
from = "review-impl"
to = "write-impl"
select = [{ option = "reject" }]

[[edges]]
from = "review-impl"
to = "END"
require = [
  { option = "approve", min_count = 1 }
]
```

### 7.1 数据流图

```mermaid
graph LR
    WP[write-prd]
    RP[review-prd]
    WT[write-tech-doc]
    RT[review-tech-doc]
    WI[write-impl]
    RI[review-impl]
    END[END]
    
    WP --> RP
    RP -->|reject| WP
    RP -->|approve_prd| WT
    
    WT --> RT
    RT -->|reject| WT
    RT -->|approve_tech| WI
    
    WI --> RI
    RI -->|reject| WI
    RI -->|approve| END
```

## 8. 控制API

> 见 [TECH-SESSION.md](TECH-SESSION.md) 中的Session生命周期管理

```rust
#[async_trait]
pub trait WorkflowControl: Send + Sync {
    async fn pause(&self, session_ulid: SessionUlid) -> Result<(), WorkflowError>;
    async fn resume(&self, session_ulid: SessionUlid) -> Result<(), WorkflowError>;
    async fn terminate(&self, session_ulid: SessionUlid, reason: String) -> Result<(), WorkflowError>;
    async fn get_status(&self, session_ulid: SessionUlid) -> Result<WorkflowStatusInfo, WorkflowError>;
}
```

## 9. 错误处理

/// 死锁检测超时时间（分钟）
/// 
/// 超过此时间无进度（无节点完成、无计数器变化）时，触发死锁检测中断
pub const DEADLOCK_TIMEOUT_MINUTES: u64 = 5;

#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("节点未找到: {0}")]
    NodeNotFound(String),
    
    #[error("没有起始节点")]
    NoStartNode,
    
    #[error("检测到循环依赖")]
    CycleDetected,
    
    #[error("工作流已完成")]
    WorkflowCompleted,
    
    #[error("死锁检测：超过{0}分钟无进度")]
    DeadlockDetected(u64),
    
    #[error("存储错误: {0}")]
    Storage(#[from] StorageError),
    
    #[error("事件发布失败: {0}")]
    EventPublishFailed(String),
}

impl WorkflowError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Storage(e) if e.is_retryable())
    }
}
```

---

*关联文档：*
- [TECH.md](TECH.md) - 总体架构文档
- [TECH-SESSION.md](TECH-SESSION.md) - Session管理模块（SessionUlid定义、消息模型）
- [TECH-AGENT.md](TECH-AGENT.md) - 多智能体协作模块（AgentEngine）
- [TECH-TOOL.md](TECH-TOOL.md) - 工具系统（ToolExecutor、ToolRegistry）
- [TECH-CONFIG.md](TECH-CONFIG.md) - 配置管理（WorkflowDef加载）
