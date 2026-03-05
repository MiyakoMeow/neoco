# Neco 技术架构文档

本文档描述Neco项目的技术架构，包括模块划分、数据结构设计、数据流向和模块间关系。

## 1. 架构概述

Neco是一个原生支持多智能体协作的智能体应用，采用Rust开发，遵循模块化、可扩展的设计原则。

### 1.1 核心设计原则

- **模块化设计**：每个功能模块独立成crate，通过清晰的trait接口交互
- **类型安全**：利用Rust的类型系统防止无效状态
- **异步优先**：基于tokio的异步运行时，支持高并发
- **可扩展性**：通过trait抽象支持不同的模型提供商、工具和通道
- **数据驱动**：所有状态变化通过显式的数据结构传递

### 1.2 架构分层

```mermaid
graph TB
    subgraph "用户接口层 UI Layer"
        CLI[CLI模式]
        REPL[REPL模式]
        Daemon[守护进程模式]
    end

    subgraph "应用核心层 Application Core"
        SessionMgr[Session管理]
        Workflow[工作流引擎]
        AgentMgr[Agent管理]
        ToolMgr[工具管理]
    end

    subgraph "服务层 Service Layer"
        ModelService[模型服务]
        MCPService[MCP服务]
        SkillService[Skills服务]
        ContextService[上下文服务]
    end

    subgraph "基础设施层 Infrastructure"
        Config[配置管理]
        Storage[存储层]
        Network[网络层]
    end

    subgraph "内核抽象层 Kernel Core"
        Core[neco-core]
        Types[核心类型定义]
        Traits[抽象接口定义]
    end

    CLI --> SessionMgr
    REPL --> SessionMgr
    Daemon --> SessionMgr
    
    SessionMgr --> AgentMgr
    SessionMgr --> Workflow
    AgentMgr --> ToolMgr
    
    AgentMgr --> ModelService
    ToolMgr --> MCPService
    ToolMgr --> SkillService
    AgentMgr --> ContextService
    
    ModelService --> Config
    ModelService --> Network
    SessionMgr --> Storage
    
    SessionMgr --> Core
    AgentMgr --> Core
    ToolMgr --> Core
    Workflow --> Core
    Config --> Core
    Storage --> Core
    
    Core --> Types
    Core --> Traits
```

### 1.3 数据流全景图

```mermaid
sequenceDiagram
    participant User as 用户
    participant UI as UI层
    participant Session as Session管理
    participant Agent as Agent引擎
    participant Context as 上下文服务
    participant Model as 模型服务
    participant Tool as 工具执行
    participant Storage as 存储层

    User->>UI: 输入消息
    UI->>Session: 创建/获取Session
    Session->>Agent: 路由消息
    Agent->>Context: 构建上下文
    Context->>Session: 获取消息历史
    Context->>Context: 检查上下文大小
    alt 超过阈值
        Context->>Context: 触发压缩
    end
    Context-->>Agent: 返回格式化上下文
    Agent->>Model: 发送请求
    alt 需要工具
        Model-->>Agent: 工具调用请求
        Agent->>Tool: 执行工具
        Tool-->>Agent: 返回结果
        Agent->>Model: 发送工具结果
        Model-->>Agent: 最终响应
    else 直接响应
        Model-->>Agent: 响应内容
    end
    Agent->>Session: 存储消息
    Agent->>Storage: 持久化
    Agent-->>UI: 返回响应
    UI-->>User: 显示结果
```

## 2. Crate划分

基于功能内聚和依赖关系，项目划分为以下crate：

| Crate | 职责 | 关键依赖 |
|-------|------|----------|
| `neco-core` | 核心类型和trait定义 | - |
| `neco-config` | 配置管理 | neco-core, toml |
| `neco-model` | 模型调用服务 | neco-core, async-openai (0.33.0) |
| `neco-session` | Session管理 | neco-core, ulid |
| `neco-mcp` | MCP客户端 | neco-core, rmcp (1.1.0) |
| `neco-skill` | Skills管理 | neco-core |
| `neco-context` | 上下文管理（压缩+观测） | neco-core |
| `neco-agent` | Agent逻辑 | neco-core, neco-model, neco-tool |
| `neco-workflow` | 工作流引擎 | neco-core, neco-agent |
| `neco-tool` | 工具实现 | neco-core, neco-fs, neco-mcp |
| `neco-fs` | 文件系统工具 | neco-core |
| `neco-ui` | 用户接口 | neco-core, ratatui |
| `neco-daemon` | 守护进程 | neco-core, axum |
| `neco` | 主入口 | 所有上述crate |

### 2.1 Crate依赖关系

```mermaid
graph TD
    neco[neco]
    
    subgraph "Binary"
        neco
    end
    
    subgraph "Interface"
        ui[neco-ui]
        daemon[neco-daemon]
    end
    
    subgraph "Orchestration"
        agent[neco-agent]
        workflow[neco-workflow]
        session[neco-session]
    end
    
    subgraph "Services"
        model[neco-model]
        mcp[neco-mcp]
        skill[neco-skill]
        context[neco-context]
        tool[neco-tool]
        fs[neco-fs]
    end
    
    subgraph "Foundation"
        config[neco-config]
        core[neco-core]
    end
    
    neco --> ui
    neco --> daemon
    neco --> agent
    neco --> workflow
    neco --> session
    neco --> config
    
    ui --> session
    ui --> agent
    daemon --> session
    daemon --> agent
    
    session --> agent
    session --> config
    
    workflow --> agent
    workflow --> session
    
    agent --> model
    agent --> tool
    agent --> core
    
    tool --> fs
    tool --> mcp
    tool --> skill
    
    model --> config
    mcp --> config
    skill --> config
    context --> core
    
    config --> core
    model --> core
    mcp --> core
    skill --> core
    context --> core
    agent --> core
    workflow --> core
    session --> core
    tool --> core
    fs --> core
    ui --> core
    daemon --> core
```

### 2.2 依赖反转接口

为解决 `session → agent → context → session` 的循环依赖问题，在 `neco-core` 中定义抽象接口：

```mermaid
graph LR
    subgraph "依赖关系"
        Context[neco-context] -->|依赖| SessionContainer[SessionContainer trait]
        Session[neco-session] -->|实现| SessionContainer
    end
```

**SessionContainer 接口定义：**

| 方法 | 描述 |
|------|------|
| `session_id(&self) -> &SessionId` | 获取Session ID |
| `root_agent_id(&self) -> Option<&AgentUlid>` | 获取根Agent ID |
| `agent_count(&self) -> usize` | 获取Agent数量 |
| `message_count(&self) -> usize` | 获取消息数量 |
| `get_messages(&self) -> Vec<Message>` | 获取所有消息 |
| `get_agent(&self, ulid: &AgentUlid) -> Option<Agent>` | 获取指定Agent |
| `add_message(&self, message: Message) -> Result<MessageId, SessionError>` | 添加消息 |

**依赖反转说明：**
- `neco-context` 依赖 `neco-core::SessionContainer` trait
- `neco-session` 实现 `SessionContainer` trait
- 运行时通过依赖注入传递具体实现

## 3. 核心数据结构设计

### 3.1 标识符系统

```mermaid
classDiagram
    class Ulid {
        +u128 value
        +new() Ulid
        +to_string() String
        +from_string(s: &str) Result~Ulid~
    }
    
    class SessionId {
        +Ulid ulid
    }
    
    class AgentUlid {
        +Ulid ulid
        +SessionId session_id
    }
    
    class MessageId {
        +u64 id
        +SessionId session_id
    }
    
    SessionId --> Ulid
    AgentUlid --> Ulid
    AgentUlid --> SessionId
    MessageId --> SessionId
```

**设计说明：**

- **SessionId**: 顶级容器标识，创建工作流或对话时生成
- **AgentUlid**: 每个Agent实例的唯一标识
  - 第一个Agent的ULID与SessionId相同
  - 后续Agent生成新的ULID
  - 通过`parent_ulid`建立树形关系
- **MessageId**: Session范围内的唯一消息ID，使用原子自增分配器

### 3.2 Session数据结构

```mermaid
classDiagram
    class Session {
        +SessionId id
        +SessionType type
        +Agent root_agent
        +HashMap~AgentUlid, Agent~ agents
        +MessageIdAllocator id_allocator
        +DateTime created_at
        +DateTime updated_at
    }
    
    class SessionType {
        <<enumeration>>
        Direct
        Repl
        Workflow
    }
    
    class MessageIdAllocator {
        +AtomicU64 counter
        +next_id() u64
    }
    
    class Agent {
        +AgentUlid ulid
        +Option~AgentUlid~ parent_ulid
        +AgentConfig config
        +Vec~Message~ messages
        +AgentState state
    }
    
    class AgentState {
        <<enumeration>>
        Idle
        Running
        WaitingForTool
        WaitingForUser
        Completed
        Error
    }
    
    class Message {
        +u64 id
        +Role role
        +Content content
        +Option~Vec~ToolCall~~ tool_calls
        +DateTime timestamp
    }
    
    class Role {
        <<enumeration>>
        System
        User
        Assistant
        Tool
    }
    
    Session --> SessionType
    Session --> MessageIdAllocator
    Session --> Agent
    Agent --> AgentState
    Agent --> Message
    Message --> Role
```

### 3.3 工作流数据结构

```mermaid
classDiagram
    class Workflow {
        +SessionId session_id
        +WorkflowDef definition
        +WorkflowState state
        +HashMap~NodeId, NodeSession~ node_sessions
        +HashMap~String, u32~ counters
    }
    
    class WorkflowDef {
        +String name
        +Vec~NodeDef~ nodes
        +Vec~EdgeDef~ edges
        +HashMap~String, Value~ params
    }
    
    class NodeDef {
        +NodeId id
        +Option~String~ agent_id
        +bool new_session
    }
    
    class EdgeDef {
        +NodeId from
        +NodeId to
        +Option~Vec~String~~ select
        +Option~Vec~String~~ require
    }
    
    class WorkflowState {
        <<enumeration>>
        Pending
        Running
        Paused
        Completed
        Failed
    }
    
    class NodeSession {
        +SessionId id
        +NodeId node_id
        +Agent node_agent
        +NodeState state
    }
    
    class NodeState {
        <<enumeration>>
        Waiting
        Running
        Success
        Failed
        Skipped
    }
    
    Workflow --> WorkflowDef
    Workflow --> WorkflowState
    Workflow --> NodeSession
    WorkflowDef --> NodeDef
    WorkflowDef --> EdgeDef
    NodeSession --> NodeState
```

**关键设计：**

- **双层结构**：
  - 工作流层（Workflow-Level）：管理节点图结构和转换控制
  - 节点层（Node-Level）：每个节点有自己的Agent树
  
- **计数器系统**：边的`select`触发时计数器+1，`require`检查计数器>0

- **节点Agent**：工作流节点的Agent同时也是该节点的最上级Agent，其ULID与节点Session ID相同

### 3.4 配置数据结构

```mermaid
classDiagram
    class NecoConfig {
        +HashMap~String, ModelGroup~ model_groups
        +HashMap~String, ModelProvider~ model_providers
        +HashMap~String, McpServer~ mcp_servers
        +ConfigPaths paths
    }
    
    class ModelGroup {
        +Vec~String~ models
    }
    
    class ModelProvider {
        +ProviderType type
        +String name
        +String base_url
        +ApiKeyConfig api_key
    }
    
    class ProviderType {
        <<enumeration>>
        OpenAI
        Anthropic
        OpenRouter
    }
    
    class ApiKeyConfig {
        <<enumeration>>
        Env(String)
        EnvList(Vec~String~)
        Direct(String)
    }
    
    class McpServer {
        +Option~String~ command
        +Vec~String~ args
        +Option~String~ url
        +Option~String~ bearer_token_env
        +HashMap~String, String~ env
    }
    
    NecoConfig --> ModelGroup
    NecoConfig --> ModelProvider
    NecoConfig --> McpServer
    ModelProvider --> ProviderType
    ModelProvider --> ApiKeyConfig
```

### 3.5 工具数据结构

```mermaid
classDiagram
    class Tool {
        +ToolId id
        +String name
        +String description
        +Value parameters_schema
    }
    
    class ToolId {
        <<enumeration>>
        Activate
        Fs(FsTool)
        Mcp(String)
        MultiAgent(MaTool)
        Question
        Context(ContextTool)
        Workflow(String)
    }
    
    class FsTool {
        <<enumeration>>
        Read
        Write
        Edit
        Delete
    }
    
    class MaTool {
        <<enumeration>>
        Spawn
        Send
    }
    
    class ContextTool {
        <<enumeration>>
        Observe
    }
    
    class ToolCall {
        +String id
        +ToolId tool
        +Value arguments
    }
    
    class ToolResult {
        +String tool_call_id
        +Result~Value, ToolError~ result
    }
    
    Tool --> ToolId
    ToolId --> FsTool
    ToolId --> MaTool
    ToolId --> ContextTool
    ToolCall --> ToolId
```

### 3.6 上下文观测数据结构

```rust
/// 消息摘要信息
#[derive(Debug, Clone, Serialize)]
pub struct MessageSummary {
    /// 消息ID
    pub id: u64,
    /// 消息角色
    pub role: Role,
    /// 消息内容预览（最多100字符）
    pub preview: String,
    /// 字符数
    pub char_count: usize,
    /// 预估token数
    pub estimated_tokens: usize,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 是否包含工具调用
    pub has_tool_calls: bool,
    /// 工具调用数量
    pub tool_calls_count: usize,
}

/// 上下文统计信息
#[derive(Debug, Clone, Serialize)]
pub struct ContextStatistics {
    /// 总消息数量
    pub total_messages: usize,
    /// 各角色消息数量
    pub messages_by_role: HashMap<Role, usize>,
    /// 总字符数
    pub total_chars: usize,
    /// 总预估token数
    pub total_tokens: usize,
    /// 上下文使用率（相对于模型窗口）
    pub usage_percentage: f64,
    /// 模型上下文窗口大小
    pub context_window: usize,
}

/// 上下文观测结果
#[derive(Debug, Clone, Serialize)]
pub struct ContextObservation {
    /// Agent ULID
    pub agent_ulid: AgentUlid,
    /// 统计信息
    pub statistics: ContextStatistics,
    /// 消息摘要列表
    pub messages: Vec<MessageSummary>,
    /// 按角色分组的消息ID
    pub messages_by_role: HashMap<Role, Vec<u64>>,
    /// 观测时间
    pub observed_at: DateTime<Utc>,
}

/// 上下文过滤选项
#[derive(Debug, Clone, Default)]
pub struct ContextFilter {
    /// 只包含指定角色的消息
    pub roles: Option<Vec<Role>>,
    /// 最小消息ID
    pub min_id: Option<u64>,
    /// 最大消息ID
    pub max_id: Option<u64>,
    /// 是否包含工具调用
    pub with_tool_calls: Option<bool>,
}

/// 上下文排序选项
#[derive(Debug, Clone, Copy)]
pub enum ContextSortOrder {
    /// 按ID升序
    IdAsc,
    /// 按ID降序
    IdDesc,
    /// 按大小升序
    SizeAsc,
    /// 按大小降序
    SizeDesc,
    /// 按时间升序
    TimeAsc,
    /// 按时间降序
    TimeDesc,
}
```

**上下文观测工具说明**：

- **context::observe**: 查看当前上下文的详细信息
  - 参数：
    - `roles`: 可选，过滤特定角色的消息
    - `min_id`/`max_id`: 可选，按ID范围过滤
    - `with_tool_calls`: 可选，只显示包含工具调用的消息
    - `sort`: 可选，排序方式（id_asc/id_desc/size_asc/size_desc/time_asc/time_desc）
    - `format`: 可选，输出格式（table/json/summary）
  - 返回：
    - 统计信息（消息数、token数、使用率等）
    - 消息列表（ID、角色、大小、预览等）
    - 按角色分组的信息


## 4. 数据流向

### 4.1 SubAgent创建流程

```mermaid
sequenceDiagram
    participant Parent as 上级Agent
    participant AgentMgr as Agent管理
    participant Session as Session
    participant Child as 下级Agent

    Parent->>AgentMgr: spawn_sub_agent(config)
    AgentMgr->>Session: 生成新AgentUlid
    Session->>Session: 分配MessageId
    
    AgentMgr->>Child: 创建Agent实例
    Child->>Child: 初始化配置
    Child->>Child: 加载prompts
    
    AgentMgr->>Session: 存储Agent关系<br/>parent_ulid = Parent.ulid
    
    Child-->>Parent: 返回Agent句柄
    
    loop Agent通信
        Parent->>Child: send_message(msg)
        Child->>Child: 处理消息
        Child-->>Parent: 返回结果
    end
```

### 4.2 工作流执行流程

```mermaid
sequenceDiagram
    participant User as 用户
    participant Workflow as 工作流引擎
    participant NodeMgr as 节点管理
    participant Session as Session管理
    participant Agent as 节点Agent
    participant Edge as 边控制器

    User->>Workflow: 启动工作流
    Workflow->>Workflow: 加载workflow.toml
    Workflow->>Session: 创建Workflow Session
    
    Workflow->>NodeMgr: 查找起始节点
    
    loop 节点执行
        NodeMgr->>Session: 创建Node Session
        NodeMgr->>Agent: 启动节点Agent
        
        alt new_session = true
            Agent->>Agent: 创建新上下文
        else new_session = false
            Agent->>Session: 恢复已有上下文
        end
        
        Agent->>Agent: 执行节点任务
        
        alt 调用转场工具
            Agent->>Edge: workflow::option(msg)
            Edge->>Workflow: 更新计数器
        end
        
        Agent-->>NodeMgr: 节点完成
        
        NodeMgr->>Edge: 评估出边
        Edge->>Edge: 检查require条件
        
        alt 条件满足
            Edge-->>Workflow: 触发下一节点
        else 多个分支
            Edge-->>Workflow: 并行触发
        end
    end
    
    Workflow-->>User: 工作流完成
```

### 4.3 模型调用与故障转移流程

```mermaid
sequenceDiagram
    participant Caller as 调用方
    participant ModelSvc as 模型服务
    participant Provider as 提供商
    participant Fallback as 故障转移

    Caller->>ModelSvc: chat_completion(request)
    ModelSvc->>ModelSvc: 解析model_group
    
    loop 遍历模型列表
        ModelSvc->>Provider: 尝试调用
        
        alt 成功
            Provider-->>ModelSvc: 返回响应
            ModelSvc-->>Caller: 返回结果
        else 失败
            Provider-->>ModelSvc: 返回错误
            ModelSvc->>ModelSvc: 指数退避重试(1s, 2s, 4s)
            
            alt 重试3次后仍失败
                ModelSvc->>Fallback: 尝试下一个模型
            end
        end
    end
    
    alt 所有模型失败
        Fallback-->>ModelSvc: 返回最终错误
        ModelSvc-->>Caller: 返回错误
    end
```

### 4.4 MCP工具调用流程

```mermaid
sequenceDiagram
    participant Agent as Agent
    participant ToolMgr as 工具管理
    participant McpClient as MCP客户端
    participant Server as MCP服务器

    Agent->>ToolMgr: 调用mcp::xxx
    ToolMgr->>McpClient: 路由到对应服务器
    
    alt stdio模式
        McpClient->>Server: 通过stdin发送请求
        Server->>Server: 执行工具
        Server-->>McpClient: 通过stdout返回结果
    else HTTP模式
        McpClient->>Server: HTTP POST请求
        Server->>Server: 执行工具
        Server-->>McpClient: HTTP响应
    end
    
    McpClient-->>ToolMgr: 返回工具结果
    ToolMgr-->>Agent: 格式化结果
```

### 4.5 上下文压缩流程

```mermaid
sequenceDiagram
    participant Agent as Agent
    participant ContextSvc as 上下文服务
    participant ModelSvc as 模型服务
    participant Session as Session

    Agent->>ContextSvc: 添加消息
    ContextSvc->>ContextSvc: 计算上下文大小
    
    alt 大小超过阈值(默认90%)
        ContextSvc->>ContextSvc: 触发自动压缩
        ContextSvc->>ModelSvc: 发送压缩请求
        Note right of ModelSvc: 关闭thinking<br/>关闭工具支持
        ModelSvc-->>ContextSvc: 返回压缩结果
        ContextSvc->>Session: 存储压缩消息
        ContextSvc->>Agent: 返回新上下文
    else 手动触发
        Agent->>ContextSvc: compact()
        ContextSvc->>ModelSvc: 发送压缩请求
        ModelSvc-->>ContextSvc: 返回压缩结果
        ContextSvc->>Session: 存储压缩消息
    end
```

## 5. 模块间接口设计

### 5.1 核心Trait定义

```rust
// neco-core/src/traits.rs

/// 可配置的组件
pub trait Configurable {
    type Config;
    fn configure(config: Self::Config) -> Self {
        // TODO: 实现配置逻辑
        todo!()
    }
}

/// Agent能力提供者
#[async_trait]
pub trait AgentCapability: Send + Sync {
    async fn execute(&self, input: AgentInput) -> Result<AgentOutput, AgentError> {
        // TODO: 实现Agent执行逻辑
        // - 解析输入消息
        // - 构建上下文
        // - 调用模型或工具
        // - 返回结果
        todo!()
    }
}

/// 模型客户端
#[async_trait]
pub trait ModelClient: Send + Sync {
    async fn chat_completion(&self, request: ChatRequest) -> Result<ChatResponse, ModelError> {
        // TODO: 实现模型调用逻辑
        // - 解析请求参数
        // - 选择合适模型
        // - 发送API请求
        // - 处理响应
        todo!()
    }
    async fn chat_completion_stream(&self, request: ChatRequest) -> Result<ChatStream, ModelError> {
        // TODO: 实现流式模型调用逻辑
        // - 解析请求参数
        // - 选择合适模型
        // - 建立流式连接
        // - 返回流式响应
        todo!()
    }
}

/// 工具提供者
#[async_trait]
pub trait ToolProvider: Send + Sync {
    fn name(&self) -> &str {
        // TODO: 返回工具名称
        todo!()
    }
    fn description(&self) -> &str {
        // TODO: 返回工具描述
        todo!()
    }
    fn schema(&self) -> Value {
        // TODO: 返回工具参数的JSON Schema
        todo!()
    }
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        // TODO: 实现工具执行逻辑
        // - 验证参数
        // - 执行工具逻辑
        // - 返回结果或错误
        todo!()
    }
}

/// 存储后端
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn save_session(&self, session: &Session) -> Result<(), StorageError> {
        // TODO: 实现Session保存逻辑
        // - 序列化Session数据
        // - 写入存储系统
        todo!()
    }
    async fn load_session(&self, id: SessionId) -> Result<Session, StorageError> {
        // TODO: 实现Session加载逻辑
        // - 从存储系统读取数据
        // - 反序列化Session对象
        todo!()
    }
    async fn save_agent(&self, session_id: SessionId, agent: &Agent) -> Result<(), StorageError> {
        // TODO: 实现Agent保存逻辑
        // - 序列化Agent数据
        // - 关联到Session保存
        todo!()
    }
    async fn load_agent(&self, ulid: AgentUlid) -> Result<Agent, StorageError> {
        // TODO: 实现Agent加载逻辑
        // - 根据ULID查找Agent
        // - 反序列化Agent对象
        todo!()
    }
}
```

### 5.2 事件系统

```mermaid
graph LR
    subgraph "事件发布者"
        A[Agent]
        W[工作流]
        S[Session]
        M[模型]
    end
    
    subgraph "事件总线"
        Bus[EventBus]
    end
    
    subgraph "事件消费者"
        UI[UI更新]
        Log[日志]
        Metric[指标]
    end
    
    A -->|AgentEvent| Bus
    W -->|WorkflowEvent| Bus
    S -->|SessionEvent| Bus
    M -->|ModelEvent| Bus
    
    Bus --> UI
    Bus --> Log
    Bus --> Metric
```

**事件类型定义：**

```rust
pub enum Event {
    Session(SessionEvent),
    Agent(AgentEvent),
    Workflow(WorkflowEvent),
    Model(ModelEvent),
    Tool(ToolEvent),
}

pub enum AgentEvent {
    Created { ulid: AgentUlid, parent: Option<AgentUlid> },
    MessageAdded { ulid: AgentUlid, message_id: u64 },
    StateChanged { ulid: AgentUlid, state: AgentState },
    ToolCalled { ulid: AgentUlid, tool: ToolId },
}

pub enum WorkflowEvent {
    Started { session_id: SessionId },
    NodeStarted { node_id: NodeId },
    NodeCompleted { node_id: NodeId, result: NodeResult },
    Transition { from: NodeId, to: NodeId },
    Completed { session_id: SessionId },
}

// TODO: 实现事件处理逻辑
// - 事件序列化/反序列化
// - 事件路由分发
// - 事件监听器管理
```

#### 5.2.1 TriggerEngine 触发器

> 参考 OpenFang 的 TriggerEngine 设计

```mermaid
graph LR
    E[Event] --> T[TriggerEngine]
    T -->|匹配模式| M1[All]
    T -->|匹配模式| M2[Lifecycle]
    T -->|匹配模式| M3[AgentSpawned]
    T -->|匹配模式| M4[SystemKeyword]
    T -->|匹配模式| M5[MemoryUpdate]
    T -->|匹配模式| M6[ContentMatch]
```

**触发模式定义：**

| 模式 | 描述 |
|------|------|
| `All` | 匹配所有事件 |
| `Lifecycle` | 生命周期事件 |
| `AgentSpawned` | Agent创建匹配 |
| `AgentTerminated` | Agent终止 |
| `SystemKeyword` | 关键词匹配 |
| `MemoryUpdate` | 内存更新 |
| `ContentMatch` | 内容匹配 |

### 5.3 统一错误类型设计

> **设计原则**: 所有模块错误类型统一在 `neco-core` 中定义，便于错误传播和转换。

```rust
/// 统一错误类型 - 应用层错误
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Session错误: {0}")]
    Session(#[from] SessionError),
    
    #[error("Agent错误: {0}")]
    Agent(#[from] AgentError),
    
    #[error("工作流错误: {0}")]
    Workflow(#[from] WorkflowError),
    
    #[error("模型错误: {0}")]
    Model(#[from] ModelError),
    
    #[error("工具错误: {0}")]
    Tool(#[from] ToolError),
    
    #[error("配置错误: {0}")]
    Config(#[from] ConfigError),
    
    #[error("存储错误: {0}")]
    Storage(#[from] StorageError),
    
    #[error("MCP错误: {0}")]
    Mcp(#[from] McpError),
    
    #[error("上下文错误: {0}")]
    Context(#[from] ContextError),
    
    #[error("Skill错误: {0}")]
    Skill(#[from] SkillError),
}
```

## 6. 存储设计

### 6.1 文件系统布局

```text
~/.config/neco/           # 配置目录
├── neco.toml            # 主配置
├── prompts/
│   ├── base.md
│   └── multi-agent.md
├── agents/
│   ├── coder.md
│   └── reviewer.md
└── workflows/
    └── prd/
        ├── workflow.toml
        └── agents/
            └── review.md

~/.local/neco/           # 数据目录
└── {session_id}/        # Session目录
    ├── workflow.toml    # 工作流状态（如果是工作流）
    ├── {agent_ulid}.toml # Agent消息文件
    └── ...
```

### 6.2 Agent TOML格式

```toml
# Agent配置
prompts = ["base", "multi-agent"]

# 层级关系
parent_ulid = "01HF..."  # 可选

# 消息列表
[[messages]]
id = 1
role = "user"
content = "..."

[[messages]]
id = 2
role = "assistant"
content = "..."
tool_calls = [
    { id = "call_1", type = "function", function = { name = "fs::read", arguments = "..." } }
]
```

### 6.3 工作流Session TOML格式

```toml
# 工作流状态
workflow_id = "prd"
created_at = "2026-03-04T10:00:00Z"

# 计数器
[counters]
approve_prd = 1
reject = 0

# 全局变量
[variables]
quality_score = 0.85

# 节点状态
[[nodes]]
id = "write-prd"
state = "completed"
agent_ulid = "01HF..."

[[nodes]]
id = "review-prd"
state = "running"
agent_ulid = "01HG..."
```

## 7. 错误处理策略

### 7.1 错误分类

| 错误类型 | 处理策略 | 恢复机制 |
|---------|---------|---------|
| 模型调用错误 | 自动重试3次 → 故障转移 | 指数退避(1s, 2s, 4s) |
| 工具调用错误 | 返回给Agent决定 | Agent决定重试/跳过/终止 |
| 配置错误 | 启动时panic | 修复配置后重启 |
| 工作流错误 | 根据配置决定 | 死锁检测(5分钟) |
| 存储错误 | 记录日志，返回错误 | 手动修复 |

### 7.2 错误传播

```mermaid
graph TD
    subgraph "错误来源"
        Model[模型错误]
        Tool[工具错误]
        Config[配置错误]
        Workflow[工作流错误]
    end
    
    subgraph "错误转换"
        E1[ModelError]
        E2[ToolError]
        E3[ConfigError]
        E4[WorkflowError]
    end
    
    subgraph "统一错误"
        App[AppError]
    end
    
    Model --> E1
    Tool --> E2
    Config --> E3
    Workflow --> E4
    
    E1 --> App
    E2 --> App
    E3 --> App
    E4 --> App
```

## 8. 并发设计

### 8.1 并发模型

```mermaid
graph TB
    subgraph "Tokio Runtime"
        subgraph "Session任务"
            S1[Session 1]
            S2[Session 2]
            S3[Session N]
        end
        
        subgraph "Agent任务"
            A1[Agent 1.1]
            A2[Agent 1.2]
        end
        
        subgraph "工作流任务"
            W1[节点1]
            W2[节点2]
        end
        
        subgraph "IO任务"
            I1[模型请求]
            I2[MCP调用]
            I3[文件IO]
        end
    end
    
    S1 --> A1
    S1 --> A2
    S1 --> W1
    S1 --> W2
    
    A1 --> I1
    A1 --> I2
    W1 --> I3
```

### 8.2 同步原语使用

| 场景 | 原语 | 说明 |
|-----|------|------|
| Session消息ID分配 | `AtomicU64` | 原子自增，无锁 |
| Agent状态变更 | `RwLock<AgentState>` | 多读单写 |
| 工作流计数器 | `Mutex<HashMap>` | 多线程安全 |
| Session缓存 | `Arc<RwLock<Session>>` | 共享可变 |
| 配置热重载 | `RwLock<Config>` | 读取频繁，写入少 |

## 9. 安全设计

> 参考 OpenFang 的 16 层安全体系设计

### 9.1 安全架构分层

```mermaid
graph TB
    subgraph "N Layer 安全体系 (对齐OpenFang 16层)"
        S1[1. WASM双计量沙箱]
        S2[2. Merkle哈希链审计]
        S3[3. 信息流污点追踪]
        S4[4. Ed25519签名清单]
        S5[5. SSRF防护]
        S6[6. 密钥零化]
        S7[7. OFP双向认证]
        S8[8. Capability能力门]
        S9[9. 安全响应头]
        S10[10. 健康端点管控]
        S11[11. 子进程沙箱]
        S12[12. Prompt注入扫描]
        S13[13. 循环守卫]
        S14[14. 会话修复]
        S15[15. 路径穿越防护]
        S16[16. GCRA速率限制]
    end
    
    S1 --> S2
    S2 --> S3
    S3 --> S4
    S4 --> S5
    S5 --> S6
    S6 --> S7
    S7 --> S8
    S8 --> S9
    S9 --> S10
    S10 --> S11
    S11 --> S12
    S12 --> S13
    S13 --> S14
    S14 --> S15
    S15 --> S16
```

### 9.2 核心安全机制

| 层级 | 安全机制 | 描述 |
|------|----------|------|
| L1 | WASM双计量沙箱 | 燃料计量+时代中断 |
| L2 | Merkle哈希链审计 | 加密链接，防篡改 |
| L3 | 信息流污点追踪 | 敏感信息追踪 |
| L4 | Ed25519签名清单 | 身份密码学验证 |
| L5 | SSRF防护 | 阻止内网IP、云元数据 |
| L6 | 密钥零化 | Zeroizing自动内存擦除 |
| L7 | OFP双向认证 | HMAC-SHA256 P2P认证 |
| L8 | Capability能力门 | RBAC能力驱动访问控制 |
| L9 | 安全响应头 | CSP, HSTS, X-Frame-Options |
| L10 | 健康端点管控 | 公开/私有诊断分离 |
| L11 | 子进程沙箱 | env_clear+进程树隔离 |
| L12 | Prompt注入扫描 | 检测override/exfiltration |
| L13 | 循环守卫 | SHA256循环检测+断路器 |
| L14 | 会话修复 | 7阶段消息历史验证 |
| L15 | 路径穿越防护 | 规范化+symlink转义检测 |
| L16 | GCRA速率限制 | 成本感知令牌桶限流 |

### 9.3 Capability能力驱动安全模型

```mermaid
classDiagram
    class Capability {
        <<enumeration>>
        FileRead
        FileWrite
        FileDelete
        NetConnect
        ShellExec
        AgentSpawn
        AgentMessage
    }
    
    class AgentManifest {
        +identity: Ed25519PublicKey
        +capabilities: Vec~Capability~
        +signature: Ed25519Signature
    }
    
    class CapabilityGate {
        +check(agent, capability): bool
        +authorize(agent, tool): Result
    }
    
    class Tool {
        +required_capability: Capability
    }
    
    AgentManifest --> Capability: 声明
    CapabilityGate --> Capability: 检查
    Tool --> Capability: 要求
```

**Capability 定义：**

```rust
/// 能力枚举
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum Capability {
    FileRead,
    FileWrite,
    FileDelete,
    NetConnect,
    ShellExec,
    AgentSpawn,
    AgentMessage,
    ToolExecute(String),
}

/// Agent能力清单
pub struct AgentManifest {
    pub identity: Ed25519PublicKey,
    pub capabilities: Vec<Capability>,
    pub signature: Ed25519Signature,
}
```

### 9.4 输入验证

| 输入类型 | 验证方式 |
|---------|---------|
| 配置文件 | Schema验证 + 运行时检查 |
| 工具参数 | JSON Schema验证 |
| 用户输入 | 长度限制 + 内容过滤 |
| 模型输出 | 敏感信息脱敏 |

### 9.5 应急停止机制

```mermaid
stateDiagram-v2
    [*] --> Normal: 正常运行
    Normal --> ToolFreeze: 触发工具冻结
    ToolFreeze --> Normal: 手动恢复
    Normal --> NetworkKill: 触发网络切断
    NetworkKill --> Normal: 手动恢复
    Normal --> DomainBlock: 触发域名屏蔽
    DomainBlock --> Normal: 手动恢复
    Normal --> KillAll: 触发完全停止
    KillAll --> [*]
```

**E-Stop 级别：**

| 级别 | 名称 | 描述 |
|------|------|------|
| L1 | ToolFreeze | 冻结指定工具 |
| L2 | NetworkKill | 禁用网络访问 |
| L3 | DomainBlock | 屏蔽指定域名 |
| L4 | KillAll | 完全停止 |

---

## 10. 扩展性设计

> 参考 ZeroClaw 的 Trait-driven 架构设计

### 10.1 扩展点总览

```mermaid
graph TB
    subgraph "扩展层"
        P[Provider抽象]
        T[Tool抽象]
        C[Channel抽象]
        M[Memory抽象]
    end
    
    subgraph "工厂层"
        F[Factory注册中心]
    end
    
    P --> F
    T --> F
    C --> F
    M --> F
```

### 10.2 核心扩展 Trait 定义

| Trait | 位置 | 扩展方式 |
|-------|------|----------|
| `ModelClient` | neco-model | 实现 trait + 注册到 Factory |
| `ToolProvider` | neco-tool | 实现 trait + 注册到 Registry |
| `StorageBackend` | neco-session | 实现 trait + 配置启用 |
| `Channel` | neco-daemon | 实现 trait + 配置通道 |
| `Memory` | neco-context | 实现 trait + 配置后端 |

### 10.3 Factory 注册机制

```mermaid
sequenceDiagram
    participant App as 应用启动
    participant Factory as Factory注册中心
    participant Provider as 模型提供商
    
    App->>Factory: 注册 Provider 类型
    Factory->>Factory: 存储类型映射
    
    App->>Factory: create("provider_name")
    Factory->>Provider: 实例化
    Provider-->>App: 返回 Provider 实例
```

### 10.4 动态组件发现

```mermaid
graph LR
    A[配置文件] --> B[ConfigLoader]
    B --> C[动态发现]
    C --> D[Factory注册]
    D --> E[运行时使用]
```

### 10.5 Feature Flag 配置

| Feature | 描述 | 默认 |
|---------|------|------|
| `model-openai` | OpenAI 提供商支持 | 启用 |
| `model-anthropic` | Anthropic 提供商支持 | 禁用 |
| `mcp-stdio` | MCP stdio 传输 | 启用 |
| `mcp-http` | MCP HTTP 传输 | 启用 |
| `workflow` | 工作流引擎 | 启用 |
| `cli` | CLI 界面 | 启用 |
| `daemon` | 守护进程模式 | 启用 |

### 10.6 内核抽象层

> 参考 OpenFang 的 Kernel Handle Trait 设计

```mermaid
graph TB
    subgraph "运行时"
        R1[CLI运行时]
        R2[REPL运行时]
        R3[Daemon运行时]
    end
    
    subgraph "NecoKernel Trait"
        NK[NecoKernel]
    end
    
    subgraph "内核实现"
        K1[KernelImpl]
    end
    
    R1 --> NK
    R2 --> NK
    R3 --> NK
    NK --> K1
```

**NecoKernel Trait 定义：**

| 方法 | 描述 |
|------|------|
| `agent_engine(&self) -> &dyn AgentEngine` | 获取Agent引擎 |
| `workflow_engine(&self) -> &dyn WorkflowEngine` | 获取工作流引擎 |
| `tool_registry(&self) -> &dyn ToolRegistry` | 获取工具注册表 |
| `context_manager(&self) -> &dyn ContextManager` | 获取上下文管理器 |
| `security_policy(&self) -> &dyn SecurityPolicy` | 获取安全策略 |

## 11. 性能设计

### 11.1 性能目标

| 指标 | 目标值 | 参考 |
|------|--------|------|
| 内存占用 | <50MB | OpenFang ~40MB |
| 冷启动 | <200ms | OpenFang <200ms |
| 二进制大小 | <20MB | OpenFang ~32MB |
| 工具超时 | 默认30s | 可配置 |
| 上下文上限 | 模型限制 | - |

### 11.2 优化策略

| 优化点 | 策略 |
|-------|------|
| 上下文大小 | 压缩阈值(默认90%)、增量更新 |
| 模型调用 | 连接池、请求合并 |
| 存储 | 异步IO、批量写入 |
| 内存 | Session缓存LRU、消息分页 |

### 11.3 资源限制

- 工具超时：默认30s（可配置）
- 上下文上限：模型限制
- 并发Agent数：由运行时配置决定

### 11.4 编译优化配置

```toml
[profile.release]
opt-level = "z"
lto = "fat"
codegen-units = 1
strip = true
```

## 12. 参考项目

### 12.1 ZeroClaw

| 维度 | ZeroClaw | Neco |
|------|----------|------|
| **定位** | Rust 原生自主 AI 助手运行时 | 多智能体协作 AI 应用 |
| **架构** | Trait-driven + Factory | Trait-driven + 依赖反转 |
| **内存占用** | <5MB | 待优化 |
| **启动速度** | <10ms | 待优化 |
| **Provider** | 多 Provider 抽象 | 多 Provider 支持 |
| **安全** | OTP/E-Stop/配对/沙箱 | 10 层安全体系 |
| **通信** | IPC + Channel 抽象 | EventBus + 工具调用 |

**核心借鉴点：**
- 统一 Trait 接口定义（Provider, Channel, Tool, Memory）
- Factory 注册机制实现动态组件发现
- 分层安全模型（OTP + E-Stop + 沙箱）
- 极致性能优化（opt-level = "z", lto = "fat"）

### 12.2 OpenFang

| 维度 | OpenFang | Neco |
|------|----------|------|
| **规模** | 137K LOC, 14 crates | 待评估 |
| **架构** | Kernel Handle Trait | 依赖反转接口 |
| **安全** | 16 层独立安全系统 | 10 层安全体系 |
| **通信** | EventBus + Trigger | EventBus |
| **工具** | Wasmtime 双计量沙箱 | 工具执行沙箱 |
| **Provider** | 27 个 LLM 驱动 | 多 Provider 支持 |
| **Channel** | 40 个消息适配器 | MCP 协议 |

**核心借鉴点：**
- Kernel Handle Trait 解耦内核与运行时
- EventBus + TriggerEngine 事件驱动架构
- Capability 能力驱动安全模型
- 防御深度（Defense in Depth）安全理念

### 12.3 架构对比总结

```mermaid
graph TB
    subgraph "Neco 架构定位"
        N[多智能体协作]
    end
    
    subgraph "ZeroClaw 特点"
        Z1[Trait-driven]
        Z2[极致性能]
        Z3[IPC通信]
    end
    
    subgraph "OpenFang 特点"
        O1[模块化内核]
        O2[事件驱动]
        O3[16层安全]
    end
    
    N --> Z1
    N --> O1
    N --> O2
    N --> Z3
```

**Neco 架构演进方向：**
1. 引入 Kernel Handle Trait 模式解耦核心模块
2. 完善 EventBus + Trigger 事件驱动机制
3. 扩展安全体系至 16 层
4. 优化性能达到 ZeroClaw 级别

---

*文档版本：0.2.0*
*最后更新：2026-03-06*
