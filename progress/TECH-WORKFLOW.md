# TECH-WORKFLOW: 工作流模块

本文档描述Neco项目的工作流模块设计，包括工作流定义、节点执行和边条件控制。

## 1. 模块概述

工作流模块实现了一个基于有向图工作流的引擎，支持节点并行执行、条件转换、循环控制和状态管理。

## 2. 核心概念

### 2.1 双层架构

```mermaid
graph TB
    subgraph "第一层：工作流层 Workflow-Level"
        W[Workflow Session]
        N1[节点: write-prd]
        N2[节点: review-prd]
        N3[节点: write-tech-doc]
        
        W --> N1
        N1 -->|approve| N3
        N1 -->|reject| N1
    end
    
    subgraph "第二层：节点层 Node-Level"
        A1[Agent树
           最上级=节点Agent]
        A2[Agent树
           最上级=节点Agent]
        A3[Agent树
           最上级=节点Agent]
    end
    
    N1 -.对应.> A1
    N2 -.对应.> A2
    N3 -.对应.> A3
```

**关键理解：**
- **工作流图**：定义"做什么任务"（任务编排）
- **Agent树**：定义"怎么做任务"（任务执行）
- **工作流边**：控制节点之间的转换
- **Agent层级**：通过`parent_ulid`建立上下级关系

### 2.2 工作流Session层次

```mermaid
graph TB
    subgraph "工作流Session"
        WS[WorkflowSession
            - 计数器
            - 全局变量
            - 节点状态]
    end
    
    subgraph "节点Session"
        NS1[NodeSession: write-prd
            - Agent树
            - 消息历史]
        NS2[NodeSession: review-prd
            - Agent树
            - 消息历史]
    end
    
    WS --> NS1
    WS --> NS2
```

## 3. 数据结构设计

### 3.1 工作流定义

```rust
/// 工作流定义（来自workflow.toml）
#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowDef {
    /// 工作流名称
    pub name: String,
    
    /// 工作流描述
    pub description: Option<String>,
    
    /// 工作流参数
    #[serde(default)]
    pub workflow_params: HashMap<String, Value>,
    
    /// 节点定义
    #[serde(rename = "nodes")]
    pub node_defs: Vec<NodeDef>,
    
    /// 边定义
    #[serde(rename = "edges", default)]
    pub edge_defs: Vec<EdgeDef>,
}

/// 节点定义
#[derive(Debug, Clone, Deserialize)]
pub struct NodeDef {
    /// 节点ID（kebab-case）
    pub id: NodeId,
    
    /// 使用的Agent标识（可选，默认=id）
    pub agent: Option<String>,
    
    /// 是否为每次传递创建新Session
    #[serde(default)]
    pub new_session: bool,
}

/// 边定义
#[derive(Debug, Clone, Deserialize)]
pub struct EdgeDef {
    /// 源节点
    pub from: NodeId,
    
    /// 目标节点
    pub to: NodeId,
    
    /// 触发选项（触发时计数器+1）
    #[serde(default)]
    pub select: Option<Vec<String>>,
    
    /// 执行条件（要求计数器>0）
    #[serde(default)]
    pub require: Option<Vec<String>>,
}

/// 节点ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct NodeId(pub String);
```

### 3.2 工作流运行时状态

```rust
/// 工作流Session（运行时状态）
pub struct WorkflowSession {
    /// Session ID
    pub session_id: SessionId,
    
    /// 工作流定义
    pub definition: Arc<WorkflowDef>,
    
    /// 节点执行状态
    pub node_states: HashMap<NodeId, NodeState>,
    
    /// 边计数器（全局共享）
    pub counters: HashMap<String, u32>,
    
    /// 工作流变量
    pub variables: HashMap<String, Value>,
    
    /// 当前活动节点
    pub active_nodes: HashSet<NodeId>,
    
    /// 工作流状态
    pub status: WorkflowStatus,
    
    /// 创建时间
    pub created_at: DateTime<Utc>,
    
    /// 最后更新时间
    pub updated_at: DateTime<Utc>,
}

/// 节点状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
    /// 等待执行
    Waiting,
    /// 执行中
    Running,
    /// 执行成功
    Success,
    /// 执行失败
    Failed,
    /// 被跳过
    Skipped,
}

/// 工作流状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowStatus {
    /// 准备就绪
    Ready,
    /// 运行中
    Running,
    /// 已暂停
    Paused,
    /// 已完成
    Completed,
    /// 已失败
    Failed,
}

/// 工作流状态信息
#[derive(Debug, Clone)]
pub struct WorkflowStatusInfo {
    /// Session ID
    pub session_id: SessionId,
    /// 工作流状态
    pub status: WorkflowStatus,
    /// 节点执行状态
    pub node_states: HashMap<NodeId, NodeState>,
    /// 活动节点数量
    pub active_nodes_count: usize,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 最后更新时间
    pub updated_at: DateTime<Utc>,
}

/// 节点Session
pub struct NodeSession {
    /// 节点Session ID
    pub id: SessionId,
    
    /// 所属工作流Session
    pub workflow_session_id: SessionId,
    
    /// 节点ID
    pub node_id: NodeId,
    
    /// 节点Agent ULID
    pub agent_ulid: AgentUlid,
    
    /// 节点状态
    pub state: NodeState,
    
    /// 执行历史（用于回溯）
    pub execution_count: u32,
}
```

## 4. 工作流引擎

### 4.1 引擎核心

```rust
/// 工作流引擎
pub struct WorkflowEngine {
    /// 配置
    config: EngineConfig,
    
    /// 运行中的工作流
    running_workflows: Arc<RwLock<HashMap<SessionId, WorkflowHandle>>>,
    
    /// 节点执行器
    node_executor: Arc<dyn NodeExecutor>,
}

impl WorkflowEngine {
    /// 启动工作流
    pub async fn start_workflow(
        &self,
        workflow_def: Arc<WorkflowDef>,
        initial_input: String,
    ) -> Result<WorkflowSession, WorkflowError> {
        // TODO: 实现工作流启动逻辑
        // 1. 创建session并初始化状态
        // 2. 查找起始节点
        // 3. 启动起始节点执行
        // 4. 保存session状态
        unimplemented!()
    }
    
    /// 查找起始节点（没有入边的节点）
    fn find_start_nodes(
        &self,
        def: &WorkflowDef,
    ) -> Result<Vec<NodeId>, WorkflowError> {
        // TODO: 实现起始节点查找逻辑
        // 返回没有入边的节点作为起始节点
        unimplemented!()
    }
    
    /// 生成节点任务
    async fn spawn_node(
        &self,
        session: &mut WorkflowSession,
        node_id: NodeId,
        input: String,
    ) -> Result<(), WorkflowError> {
        // TODO: 实现节点任务生成逻辑
        // 1. 更新节点状态为Running
        // 2. 创建或恢复Node Session
        // 3. 在后台异步执行节点
        unimplemented!()
    }
}
```

### 4.2 节点执行

```rust
/// 节点执行器接口
#[async_trait]
pub trait NodeExecutor: Send + Sync {
    async fn execute(
        &self,
        node_session: &NodeSession,
        agent: &mut Agent,
    ) -> Result<NodeResult, NodeError>;
}

/// 节点执行结果
#[derive(Debug, Clone)]
pub struct NodeResult {
    /// 输出内容
    pub output: String,
    /// 选择的选项（用于边条件）
    pub selected_option: Option<String>,
    /// 元数据
    pub metadata: HashMap<String, Value>,
}

/// 默认节点执行器
pub struct DefaultNodeExecutor {
    model_client: Arc<dyn ModelClient>,
    tool_registry: Arc<ToolRegistry>,
}

#[async_trait]
impl NodeExecutor for DefaultNodeExecutor {
    async fn execute(
        &self,
        node_session: &NodeSession,
        agent: &mut Agent,
    ) -> Result<NodeResult, NodeError> {
        // TODO: 实现节点执行逻辑
        // 1. 加载Agent配置和提示词
        // 2. 构建上下文
        // 3. 循环调用模型直到节点完成
        // 4. 处理工具调用和转场选项
        unimplemented!()
    }
}
```

## 5. 边条件控制

### 5.1 条件评估

```rust
impl WorkflowEngine {
    /// 处理节点完成
    async fn handle_node_complete(
        &self,
        session_id: SessionId,
        node_id: NodeId,
        result: NodeResult,
    ) {
        // TODO: 实现节点完成处理逻辑
        // 1. 更新节点状态为Success
        // 2. 更新计数器（如果选择了选项）
        // 3. 评估出边条件，确定下一个节点
        // 4. 启动下一节点执行
        // 5. 检查工作流是否完成
        // 6. 保存session状态
        unimplemented!()
    }
    
    /// 评估边条件
    async fn evaluate_edges(
        &self,
        session: &WorkflowSession,
        current_node: &NodeId,
        result: &NodeResult,
    ) -> Vec<NodeId> {
        // TODO: 实现边条件评估逻辑
        // 1. 检查select条件（匹配选项时触发）
        // 2. 检查require条件（计数器满足时触发）
        // 3. 无条件边直接触发
        // 4. 返回下一个要执行的节点列表
        unimplemented!()
    }
}
```

### 5.2 条件语法

```toml
[[edges]]
from = "review-prd"
to = "write-prd"
select = ["reject"]  # 触发时 counters.reject += 1

[[edges]]
from = "write-prd"
to = "write-tech-doc"
require = ["approve_prd"]  # 需要 counters.approve_prd > 0

# 支持参数引用
[[edges]]
from = "review-prd"
to = "final-approve"
require = ["@params.min_approvers"]  # 引用workflow_params
```

## 6. 转场工具

### 6.1 工具定义

```rust
/// 工作流转场工具
pub struct WorkflowTransitionTool {
    workflow_session_id: SessionId,
    current_node: NodeId,
}

impl ToolProvider for WorkflowTransitionTool {
    fn name(&self) -> &str {
        "workflow"
    }
    
    fn description(&self) -> &str {
        "控制工作流节点之间的转换"
    }
    
    fn schema(&self) -> Value {
        // TODO: 定义转场工具的JSON Schema
        json!({
            "type": "object",
            "properties": {
                "option": {
                    "type": "string",
                    "description": "转场选项，如 'approve', 'reject', 'pass'"
                },
                "message": {
                    "type": "string",
                    "description": "传递给下一节点的消息"
                }
            },
            "required": ["option", "message"]
        })
    }
    
    async fn execute(
        &self,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        // TODO: 实现转场工具执行逻辑
        // 1. 解析option和message参数
        // 2. 触发节点转场
        // 3. 返回执行结果
        unimplemented!()
    }
}
```

### 6.2 动态工具注册

```rust
/// 为工作流节点注册转场工具
pub fn register_workflow_tools(
    tool_registry: &mut ToolRegistry,
    workflow_def: &WorkflowDef,
    current_node: &NodeId,
) {
    // TODO: 实现动态工具注册逻辑
    // 1. 注册无条件传递工具 workflow::pass
    // 2. 动态注册当前节点的出边选项
    // 3. 为每个select选项创建对应的转场工具
    unimplemented!()
}
```

## 7. PRD工作流示例

### 7.1 完整工作流定义

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
agent = "review"  # 使用 agents/review.md
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
require = ["approve_prd"]

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
require = ["approve_tech"]

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
require = ["approve"]
```

### 7.2 数据流图

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

## 8. 工作流控制API

```rust
/// 工作流控制接口
#[async_trait]
pub trait WorkflowControl: Send + Sync {
    /// 暂停工作流
    async fn pause(
        &self,
        session_id: SessionId,
    ) -> Result<(), WorkflowError>;
    
    /// 恢复工作流
    async fn resume(
        &self,
        session_id: SessionId,
    ) -> Result<(), WorkflowError>;
    
    /// 终止工作流
    async fn terminate(
        &self,
        session_id: SessionId,
        reason: String,
    ) -> Result<(), WorkflowError>;
    
    /// 获取工作流状态
    async fn get_status(
        &self,
        session_id: SessionId,
    ) -> Result<WorkflowStatusInfo, WorkflowError>;
    
    /// 手动触发节点
    async fn trigger_node(
        &self,
        session_id: SessionId,
        node_id: NodeId,
        input: String,
    ) -> Result<(), WorkflowError>;
}
```

## 9. 错误处理

> **注意**: 所有模块错误类型统一在 `neco-core` 中汇总为 `AppError`。见 [TECH.md#53-统一错误类型设计](TECH.md#53-统一错误类型设计)。

```rust
#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("节点未找到: {0}")]
    NodeNotFound(NodeId),
    
    #[error("没有起始节点")]
    NoStartNode,
    
    #[error("检测到循环依赖")]
    CycleDetected,
    
    #[error("节点执行错误: {0}")]
    NodeExecution(#[from] NodeError),
    
    #[error("存储错误: {0}")]
    Storage(#[from] StorageError),
    
    #[error("工作流 {0} 不存在")]
    WorkflowNotFound(SessionId),
    
    #[error("工作流已完成")]
    WorkflowCompleted,
    
    #[error("死锁检测：超过5分钟无进度")]
    DeadlockDetected,
}

#[derive(Debug, Error)]
pub enum NodeError {
    #[error("模型调用错误: {0}")]
    Model(#[source] ModelError),
    
    #[error("工具执行错误: {0}")]
    Tool(#[source] ToolError),
    
    #[error("Agent未找到")]
    AgentNotFound,
    
    #[error("节点超时")]
    Timeout,
}
```

---

*关联文档：*
- [TECH.md](TECH.md) - 总体架构文档
- [TECH-SESSION.md](TECH-SESSION.md) - Session管理模块
- [TECH-AGENT.md](TECH-AGENT.md) - 多智能体协作模块
