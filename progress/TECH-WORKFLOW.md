# TECH-WORKFLOW: 工作流模块

本文档描述Neco项目的工作流模块设计，包括工作流定义、节点执行和边条件控制。

## 1. 模块概述

工作流模块实现了一个基于DAG（有向无环图）的工作流引擎，支持节点并行执行、条件转换和状态管理。

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
- **Agent树**：通过`parent_ulid`建立上下级关系

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
        let session_id = SessionId::new();
        
        // 创建工作流Session
        let mut session = WorkflowSession {
            session_id: session_id.clone(),
            definition: workflow_def.clone(),
            node_states: HashMap::new(),
            counters: HashMap::new(),
            variables: workflow_def.workflow_params.clone(),
            active_nodes: HashSet::new(),
            status: WorkflowStatus::Running,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        // 初始化所有节点为Waiting状态
        for node_def in &workflow_def.node_defs {
            session.node_states.insert(
                node_def.id.clone(),
                NodeState::Waiting
            );
        }
        
        // 查找起始节点（没有入边的节点）
        let start_nodes = self.find_start_nodes(&workflow_def)?;
        
        // 启动起始节点
        for node_id in start_nodes {
            self.spawn_node(
                &mut session,
                node_id,
                initial_input.clone()
            ).await?;
        }
        
        // 保存Session状态
        self.save_session(&session).await?;
        
        Ok(session)
    }
    
    /// 查找起始节点（没有入边的节点）
    fn find_start_nodes(
        &self,
        def: &WorkflowDef,
    ) -> Result<Vec<NodeId>, WorkflowError> {
        let all_nodes: HashSet<_> = def.node_defs
            .iter()
            .map(|n| &n.id)
            .collect();
        
        let nodes_with_incoming: HashSet<_> = def.edge_defs
            .iter()
            .map(|e| &e.to)
            .collect();
        
        let start_nodes: Vec<_> = all_nodes
            .difference(&nodes_with_incoming)
            .map(|id| (*id).clone())
            .collect();
        
        if start_nodes.is_empty() {
            return Err(WorkflowError::NoStartNode);
        }
        
        Ok(start_nodes)
    }
    
    /// 生成节点任务
    async fn spawn_node(
        &self,
        session: &mut WorkflowSession,
        node_id: NodeId,
        input: String,
    ) -> Result<(), WorkflowError> {
        // 更新节点状态
        session.node_states.insert(node_id.clone(), NodeState::Running);
        session.active_nodes.insert(node_id.clone());
        session.updated_at = Utc::now();
        
        // 获取节点定义
        let node_def = session.definition.node_defs
            .iter()
            .find(|n| n.id == node_id)
            .ok_or(WorkflowError::NodeNotFound(node_id.clone()))?;
        
        // 创建或恢复Node Session
        let node_session = self.create_or_restore_node_session(
            session,
            node_def,
            input
        ).await?;
        
        // 在后台执行节点
        let engine = Arc::new(self.clone());
        let session_id = session.session_id.clone();
        
        tokio::spawn(async move {
            match engine.execute_node(node_session).await {
                Ok(result) => {
                    engine.handle_node_complete(
                        session_id,
                        node_id,
                        result
                    ).await;
                }
                Err(e) => {
                    engine.handle_node_error(
                        session_id,
                        node_id,
                        e
                    ).await;
                }
            }
        });
        
        Ok(())
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
        // 1. 加载Agent配置和提示词
        self.load_agent_prompts(agent).await?;
        
        // 2. 构建上下文
        let context = self.build_context(agent).await?;
        
        // 3. 循环直到节点完成
        loop {
            // 调用模型
            let response = self.model_client
                .chat_completion(context.clone())
                .await
                .map_err(NodeError::Model)?;
            
            // 处理响应
            let choice = &response.choices[0];
            
            // 检查是否是转场工具调用
            if let Some(tool_calls) = &choice.message.tool_calls {
                for tc in tool_calls {
                    if tc.function.name.starts_with("workflow::") {
                        // 解析转场选项
                        let option = tc.function.name
                            .strip_prefix("workflow::")
                            .unwrap();
                        
                        return Ok(NodeResult {
                            output: choice.message.content.clone()
                                .unwrap_or_default(),
                            selected_option: Some(option.to_string()),
                            metadata: HashMap::new(),
                        });
                    }
                }
                
                // 执行普通工具
                self.execute_tools(agent, tool_calls).await?;
            } else {
                // 普通响应，节点完成
                return Ok(NodeResult {
                    output: choice.message.content.clone()
                        .unwrap_or_default(),
                    selected_option: None,
                    metadata: HashMap::new(),
                });
            }
            
            // 更新上下文
            // ...
        }
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
        let mut workflows = self.running_workflows.write().await;
        let session = workflows.get_mut(&session_id)
            .expect("Session must exist");
        
        // 更新节点状态
        session.node_states.insert(node_id.clone(), NodeState::Success);
        session.active_nodes.remove(&node_id);
        
        // 如果有选择的选项，更新计数器
        if let Some(option) = result.selected_option {
            let counter = session.counters
                .entry(option)
                .or_insert(0);
            *counter += 1;
        }
        
        // 评估出边
        let next_nodes = self.evaluate_edges(
            session,
            &node_id,
            &result
        ).await;
        
        // 启动下一节点
        for next_node_id in next_nodes {
            if let Err(e) = self.spawn_node(
                session,
                next_node_id,
                result.output.clone()
            ).await {
                error!("Failed to spawn node: {}", e);
            }
        }
        
        // 检查工作流是否完成
        if session.active_nodes.is_empty() {
            session.status = WorkflowStatus::Completed;
        }
        
        // 保存状态
        let _ = self.save_session(session).await;
    }
    
    /// 评估边条件
    async fn evaluate_edges(
        &self,
        session: &WorkflowSession,
        current_node: &NodeId,
        result: &NodeResult,
    ) -> Vec<NodeId> {
        let mut next_nodes = Vec::new();
        
        for edge in &session.definition.edge_defs {
            if edge.from != *current_node {
                continue;
            }
            
            // 检查select条件
            if let Some(select_options) = &edge.select {
                // select边：如果结果匹配任一选项，触发
                if let Some(selected) = &result.selected_option {
                    if select_options.contains(selected) {
                        next_nodes.push(edge.to.clone());
                    }
                }
            } else if let Some(require_options) = &edge.require {
                // require边：检查计数器
                let can_execute = require_options
                    .iter()
                    .any(|opt| {
                        session.counters
                            .get(opt)
                            .map(|c| *c > 0)
                            .unwrap_or(false)
                    });
                
                if can_execute {
                    next_nodes.push(edge.to.clone());
                }
            } else {
                // 无条件边：直接触发
                next_nodes.push(edge.to.clone());
            }
        }
        
        next_nodes
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
        let option = args["option"].as_str()
            .ok_or(ToolError::InvalidArgs)?;
        let message = args["message"].as_str()
            .ok_or(ToolError::InvalidArgs)?;
        
        // 触发转场
        // 这将导致当前节点结束，并触发相应的边条件
        
        Ok(ToolResult {
            output: format!(
                "Transition to next node with option: {}",
                option
            ),
            metadata: json!({
                "transition": true,
                "option": option,
                "message": message,
            }),
        })
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
    // 注册 workflow::pass（无条件传递）
    tool_registry.register(WorkflowTransitionTool {
        name: "workflow::pass".to_string(),
        option: "pass".to_string(),
    });
    
    // 动态注册该节点出边的选项
    for edge in &workflow_def.edge_defs {
        if &edge.from == current_node {
            if let Some(select_options) = &edge.select {
                for option in select_options {
                    tool_registry.register(WorkflowTransitionTool {
                        name: format!("workflow::{}", option),
                        option: option.clone(),
                    });
                }
            }
        }
    }
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
