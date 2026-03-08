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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDefinition {
    pub id: NodeUlid,
    pub agent_ulid: Option<AgentUlid>,
    #[serde(default)]
    pub new_session: bool,
}

/// 边定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeDefinition {
    pub from: NodeUlid,
    pub to: NodeUlid,
    #[serde(default)]
    pub select: Option<Vec<String>>,
    #[serde(default)]
    pub require: Option<Vec<Requirement>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    pub option: String,
    pub min_count: u32,
    pub param_ref: Option<String>,
}

/// 节点ID（强类型ULID）
/// 
/// 节点ID采用ULID格式，确保跨工作流的一致性命名。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct NodeUlid(pub Ulid);

impl NodeUlid {
    pub fn new() -> Self {
        Self(Ulid::new())
    }
    
    pub fn from_string(s: &str) -> Result<Self, IdError> {
        Ok(Self(Ulid::from_string(s)?))
    }
    
    pub fn as_str(&self) -> &str {
        self.0.encode().as_str()
    }
}

// 自定义反序列化实现
impl<'de> Deserialize<'de> for NodeUlid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_string(&s).map_err(|e| serde::de::Error::custom(e))
    }
}
```

### 3.2 工作流运行时（动态状态）

```rust
/// 工作流运行时状态
#[derive(Debug, Clone)]
pub struct WorkflowRuntime {
    pub session_ulid: SessionUlid,
    definition: Arc<WorkflowDefinition>,
    node_states: DashMap<NodeUlid, NodeRuntimeState>,
    counters: DashMap<CounterKey, u32>,
    variables: DashMap<VariableKey, Value>,
    active_nodes: DashSet<NodeUlid>,
    transition_messages: DashMap<NodeUlid, String>,
    status: WorkflowStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
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
        // 2. 初始化空的active_nodes HashSet<NodeUlid>
        // 3. 初始化空的node_states HashMap<NodeUlid, NodeRuntimeState>
        // 4. 初始化空的counters HashMap<String, u32>
        // 5. 初始化空的transition_messages DashMap<NodeUlid, String>
        // 6. 设置status为WorkflowStatus::Ready
        // 7. 设置created_at和updated_at为当前UTC时间
        unimplemented!()
    }
    
    pub fn start_node(&mut self, node_ulid: NodeUlid, agent_ulid: AgentUlid) {
        // TODO: 实现节点启动逻辑
        // 1. 检查节点是否已在active_nodes中
        // 2. 创建NodeRuntimeState::Running { agent_ulid }
        // 3. 将状态插入node_states
        // 4. 将node_ulid加入active_nodes
        // 5. 更新updated_at为当前时间
        unimplemented!()
    }
    
    pub fn complete_node(&mut self, node_ulid: &NodeUlid, output: String) {
        // TODO: 实现节点完成逻辑
        // 1. 更新node_states中该节点的状态为Success { output }
        // 2. 从active_nodes HashSet中移除该node_ulid
        // 3. 更新updated_at为当前时间
        todo!()
    }
    
    pub fn increment_counter(&mut self, option: &str) {
        // TODO: 实现计数器递增逻辑
        // 1. 使用CounterKey包装option
        // 2. 使用counters.entry(key).or_insert(0)获取或创建计数器
        // 3. 对获取的可变引用执行加1操作
        unimplemented!()
    }
    
    pub fn get_counter(&self, option: &str) -> u32 {
        // TODO: 实现获取计数器值逻辑
        // 1. 使用CounterKey包装option
        // 2. 调用counters.get(key)查找计数器
        // 3. 如果Some(v)返回*v，否则返回0
        unimplemented!()
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkflowEvent {
    WorkflowStarted {
        session_ulid: SessionUlid,
        definition_id: String,
    },
    NodeStarted {
        session_ulid: SessionUlid,
        node_ulid: NodeUlid,
        agent_ulid: AgentUlid,
    },
    NodeCompleted {
        session_ulid: SessionUlid,
        node_ulid: NodeUlid,
        output: String,
    },
    NodeFailed {
        session_ulid: SessionUlid,
        node_ulid: NodeUlid,
        error: String,
    },
    NodeTransitionIntent {
        session_ulid: SessionUlid,
        node_ulid: NodeUlid,
        message: Option<String>,
    },
    EdgeTriggered {
        session_ulid: SessionUlid,
        from: NodeUlid,
        to: NodeUlid,
        option: Option<String>,
    },
    WorkflowCompleted {
        session_ulid: SessionUlid,
    },
    WorkflowFailed {
        session_ulid: SessionUlid,
        reason: String,
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
        node_ulid: NodeUlid,
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
    ) -> Vec<NodeUlid> {
        // TODO: 查找起始节点
        // 1. 创建HashSet收集所有有入边的节点ID
        // 2. 遍历所有edges，将target（to）加入HashSet
        // 3. 遍历所有nodes，返回不在HashSet中的节点（无入边的节点）
        todo!()
    }
    
    pub fn evaluate_edges(
        &self,
        runtime: &WorkflowRuntime,
        current_node: &NodeUlid,
    ) -> Vec<NodeUlid> {
        // TODO: 评估边的条件以确定下一个节点
        // 1. 查找定义中从current_node出发的所有边
        // 2. 对每条边调用evaluate_requirement评估条件
        // 3. 收集所有条件满足的边的target节点
        // 4. 返回目标节点ID列表
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
[[edges]]
from = "review-prd"
to = "write-prd"
select = [{ option = "reject" }]  # 触发时 counters.reject += 1

[[edges]]
from = "write-prd"
to = "write-tech-doc"
require = [
  { option = "approve_prd", min_count = 1 }  # 需要 counters.approve_prd > 0
]

# 支持参数引用
[[edges]]
from = "review-prd"
to = "final-approve"
require = [
  { option = "@params.min_approvers", min_count = 1, param_ref = "min_approvers" }  # 引用workflow_params
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
    node_ulid: NodeUlid,
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
    node_ulid: NodeUlid,
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
    node_ulid: NodeUlid,
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
select = ["reject"]

[[edges]]
from = "review-prd"
to = "write-tech-doc"
require = [
  { option = "approve_prd", min_count = 1 }
]

[[edges]]
from = "write-tech-doc"
to = "review-tech-doc"

[[edges]]
from = "review-tech-doc"
to = "write-tech-doc"
select = ["reject"]

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
select = ["reject"]

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

```rust
#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("节点未找到: {0}")]
    NodeNotFound(NodeUlid),
    
    #[error("没有起始节点")]
    NoStartNode,
    
    #[error("检测到循环依赖")]
    CycleDetected,
    
    #[error("工作流已完成")]
    WorkflowCompleted,
    
    #[error("死锁检测：超过5分钟无进度")]
    DeadlockDetected,
    
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
