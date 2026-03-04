# TECH-SESSION: Session管理模块

本文档描述Neco项目的Session管理模块设计，包括Session生命周期、消息存储和上下文管理。

## 1. 模块概述

Session管理模块负责管理对话Session的生命周期、消息存储和Agent树形结构。它是整个系统的核心状态管理中心。

## 2. 核心概念

### 2.1 标识符体系

```mermaid
classDiagram
    class SessionId {
        +Ulid ulid
        +new() SessionId
    }
    
    class AgentUlid {
        +Ulid ulid
        +SessionId session_id
    }
    
    class NodeSessionId {
        +Ulid ulid
        +SessionId workflow_session_id
        +NodeId node_id
    }
    
    SessionId --> AgentUlid : 包含
    SessionId --> NodeSessionId : 工作流Session
```

**标识符规则：**

| 标识符 | 生成时机 | 关系 | 用途 |
|-------|---------|------|------|
| SessionId | 创建Session时 | 顶级容器 | 标识整个对话或工作流 |
| AgentUlid | Agent实例化时 | 第一个=SessionId | 标识Agent实例 |
| NodeSessionId | 工作流节点启动时 | 归属Workflow Session | 标识工作流节点 |

### 2.2 Session类型

```rust
/// Session类型
pub enum SessionType {
    /// 直接模式：单次对话
    Direct {
        message: String,
    },
    /// REPL模式：交互式对话
    Repl,
    /// 工作流模式：结构化流程
    Workflow {
        workflow_def: WorkflowDef,
        current_node: Option<NodeId>,
    },
}
```

## 3. 数据结构设计

### 3.1 Session结构

```rust
/// Session是顶层容器
pub struct Session {
    /// Session唯一标识
    pub id: SessionId,
    
    /// Session类型
    pub session_type: SessionType,
    
    /// 根Agent（用户直接对话的Agent）
    pub root_agent: AgentUlid,
    
    /// 所有Agent的映射
    pub agents: HashMap<AgentUlid, Agent>,
    
    /// 消息ID分配器（Session范围内唯一）
    pub id_allocator: MessageIdAllocator,
    
    /// 创建时间
    pub created_at: DateTime<Utc>,
    
    /// 最后更新时间
    pub updated_at: DateTime<Utc>,
    
    /// 元数据
    pub metadata: SessionMetadata,
}

/// Session元数据
pub struct SessionMetadata {
    /// 用户标识
    pub user_id: Option<String>,
    /// 工作目录
    pub working_dir: PathBuf,
    /// 初始提示
    pub initial_prompt: Option<String>,
    /// 自定义数据
    pub custom_data: HashMap<String, Value>,
}

/// 消息ID分配器
pub struct MessageIdAllocator {
    counter: AtomicU64,
}

impl MessageIdAllocator {
    pub fn new() -> Self {
        // TODO: 创建MessageIdAllocator实例
        // 1. 初始化counter为1
        unimplemented!()
    }
    
    /// 获取下一个消息ID
    pub fn next_id(&self) -> u64 {
        // TODO: 获取下一个消息ID
        // 1. 使用fetch_add原子操作递增counter
        // 2. 返回之前的值
        unimplemented!()
    }
}
```

### 3.2 Agent结构

```rust
/// Agent实例
pub struct Agent {
    /// Agent唯一标识
    pub ulid: AgentUlid,
    
    /// 上级Agent（None表示根Agent）
    pub parent_ulid: Option<AgentUlid>,
    
    /// 下级Agent列表
    pub children: Vec<AgentUlid>,
    
    /// Agent配置
    pub config: AgentConfig,
    
    /// 消息历史
    pub messages: Vec<Message>,
    
    /// Agent状态
    pub state: AgentState,
    
    /// 激活的工具列表
    pub active_tools: Vec<ToolId>,
    
    /// 激活的MCP服务器
    pub active_mcp_servers: Vec<String>,
    
    /// 激活的Skills
    pub active_skills: Vec<String>,
    
    /// 创建时间
    pub created_at: DateTime<Utc>,
    
    /// 最后活动时间
    pub last_activity: DateTime<Utc>,
}

/// Agent配置
pub struct AgentConfig {
    /// 使用的模型组
    pub model_group: String,
    /// 激活的提示词组件
    pub prompts: Vec<String>,
    /// Agent定义来源
    pub agent_def: Option<PathBuf>,
}

/// Agent状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentState {
    /// 空闲
    Idle,
    /// 运行中
    Running,
    /// 等待工具调用完成
    WaitingForTool,
    /// 等待用户输入
    WaitingForUser,
    /// 已完成
    Completed,
    /// 错误状态
    Error,
}
```

### 3.3 消息结构

```rust
/// 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 消息ID（Session范围内唯一）
    pub id: u64,
    
    /// 角色
    pub role: Role,
    
    /// 内容
    pub content: String,
    
    /// 工具调用（Assistant角色时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    
    /// 工具调用ID（Tool角色时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    
    /// 元数据（如token使用量）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MessageMetadata>,
}

/// 角色
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// 消息元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}
```

## 4. Agent树结构

### 4.1 树形关系

```mermaid
graph TD
    subgraph "Session"
        S[Session Root]
    end
    
    subgraph "Agent树"
        A1[Agent 1<br/>ULID = SessionId]
        A2[Agent 1.1<br/>parent=A1]
        A3[Agent 1.2<br/>parent=A1]
        A4[Agent 1.1.1<br/>parent=A2]
        A5[Agent 1.2.1<br/>parent=A3]
        A6[Agent 1.2.2<br/>parent=A3]
    end
    
    S --> A1
    A1 --> A2
    A1 --> A3
    A2 --> A4
    A3 --> A5
    A3 --> A6
```

### 4.2 Agent关系管理

```rust
impl Session {
    /// 创建根Agent
    pub fn create_root_agent(
        &mut self,
        config: AgentConfig,
    ) -> Result<AgentUlid, SessionError> {
        // TODO: 创建根Agent实现
        // 1. 生成AgentUlid（使用SessionId的ULID）
        // 2. 创建Agent实例，设置parent_ulid为None
        // 3. 初始化Agent状态为Idle
        // 4. 将Agent添加到agents HashMap中
        // 5. 设置root_agent为新创建的AgentUlid
        // 6. 返回AgentUlid
        unimplemented!()
    }
    
    /// 创建子Agent
    pub fn spawn_child_agent(
        &mut self,
        parent_ulid: AgentUlid,
        config: AgentConfig,
    ) -> Result<AgentUlid, SessionError> {
        // TODO: 创建子Agent实现
        // 1. 验证父Agent存在
        // 2. 生成新的ULID
        // 3. 创建Agent实例，设置parent_ulid为parent_ulid
        // 4. 将新Agent添加到父Agent的children列表
        // 5. 插入新Agent到agents HashMap
        // 6. 返回新AgentUlid
        unimplemented!()
    }
    
    /// 获取Agent的所有祖先
    pub fn get_ancestors(
        &self,
        ulid: &AgentUlid,
    ) -> Vec<AgentUlid> {
        // TODO: 获取Agent的所有祖先
        // 1. 从当前Agent开始向上遍历parent_ulid
        // 2. 收集所有祖先AgentUlid直到根Agent
        // 3. 返回祖先列表
        unimplemented!()
    }
    
    /// 获取Agent的所有后代（递归）
    pub fn get_descendants(
        &self,
        ulid: &AgentUlid,
    ) -> Vec<AgentUlid> {
        // TODO: 获取Agent的所有后代（递归）
        // 1. 使用DFS或BFS遍历子树
        // 2. 收集所有后代AgentUlid
        // 3. 返回后代列表
        unimplemented!()
    }
}
```

## 5. 存储设计

### 5.1 文件存储结构

```
~/.local/neco/
└── {session_id}/                    # Session目录
    ├── session.toml                 # Session元数据
    ├── {agent_ulid}.toml           # Agent消息文件
    └── workflow_state.toml         # 工作流状态（如果是工作流）
```

### 5.2 TOML文件格式

**Session元数据文件（session.toml）：**

```toml
[session]
id = "01HF8X5JQC8ZXJ3YKZ0J9K2D9Z"
type = "workflow"  # direct, repl, workflow
created_at = "2026-03-04T10:00:00Z"
updated_at = "2026-03-04T10:30:00Z"
root_agent = "01HF8X5JQC8ZXJ3YKZ0J9K2D9Z"

[metadata]
user_id = "user123"
working_dir = "/home/user/projects"

[workflow]
workflow_id = "prd"
current_node = "write-prd"

[[agents]]
ulid = "01HF8X5JQC8ZXJ3YKZ0J9K2D9Z"
parent = null
state = "running"
last_activity = "2026-03-04T10:25:00Z"

[[agents]]
ulid = "01HF8X5JQC8ZXJ3YKZ0J9K2E0A"
parent = "01HF8X5JQC8ZXJ3YKZ0J9K2D9Z"
state = "idle"
last_activity = "2026-03-04T10:20:00Z"
```

**Agent消息文件（{agent_ulid}.toml）：**

```toml
# Agent配置
[config]
model_group = "smart"
prompts = ["base", "multi-agent"]

# 层级关系
parent_ulid = "01HF8X5JQC8ZXJ3YKZ0J9K2D9Z"  # 可选，根Agent省略

# 激活的工具/MCP/Skills
[active]
tools = ["fs::read", "fs::write"]
mcp_servers = ["context7"]
skills = []

# 消息列表
[[messages]]
id = 1
role = "system"
content = "你是一个 helpful assistant。"
timestamp = "2026-03-04T10:00:00Z"

[[messages]]
id = 2
role = "user"
content = "帮我读取文件 README.md"
timestamp = "2026-03-04T10:01:00Z"

[[messages]]
id = 3
role = "assistant"
content = null
timestamp = "2026-03-04T10:01:05Z"

[[messages.tool_calls]]
id = "call_1"
type = "function"

[messages.tool_calls.function]
name = "fs::read"
arguments = '{"path": "README.md"}'

[[messages]]
id = 4
role = "tool"
content = "# Project README\n..."
tool_call_id = "call_1"
timestamp = "2026-03-04T10:01:06Z"

[[messages]]
id = 5
role = "assistant"
content = "README.md 的内容是：..."
timestamp = "2026-03-04T10:01:10Z"

[messages.metadata]
prompt_tokens = 100
completion_tokens = 50
total_tokens = 150
```

### 5.3 存储后端Trait

```rust
/// 存储后端接口
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// 保存Session元数据
    async fn save_session_meta(
        &self,
        session: &Session,
    ) -> Result<(), StorageError>;
    
    /// 加载Session元数据
    async fn load_session_meta(
        &self,
        session_id: SessionId,
    ) -> Result<SessionMeta, StorageError>;
    
    /// 保存Agent数据
    async fn save_agent(
        &self,
        agent: &Agent,
    ) -> Result<(), StorageError>;
    
    /// 加载Agent数据
    async fn load_agent(
        &self,
        ulid: AgentUlid,
    ) -> Result<Agent, StorageError>;
    
    /// 追加消息到Agent
    async fn append_message(
        &self,
        ulid: AgentUlid,
        message: &Message,
    ) -> Result<(), StorageError>;
    
    /// 列出Session中的所有Agent
    async fn list_agents(
        &self,
        session_id: SessionId,
    ) -> Result<Vec<AgentUlid>, StorageError>;
    
    /// 删除Session
    async fn delete_session(
        &self,
        session_id: SessionId,
    ) -> Result<(), StorageError>;
}

/// 文件系统存储实现
pub struct FileStorage {
    base_dir: PathBuf,
}

impl FileStorage {
    pub fn new(base_dir: PathBuf) -> Self {
        // TODO: 创建FileStorage实例
        // 1. 设置base_dir
        // 2. 可选：验证目录存在并可写
        Self { base_dir }
    }
    
    fn session_dir(&self, session_id: &SessionId) -> PathBuf {
        // TODO: 获取Session目录路径
        // 1. 组合base_dir和session_id字符串
        // 2. 返回完整路径
        unimplemented!()
    }
    
    fn agent_file(&self, ulid: &AgentUlid) -> PathBuf {
        // TODO: 获取Agent文件路径
        // 1. 获取session_dir
        // 2. 组合ulid字符串和".toml"后缀
        // 3. 返回完整路径
        unimplemented!()
    }
}

#[async_trait]
impl StorageBackend for FileStorage {
    async fn save_session_meta(
        &self,
        session: &Session,
    ) -> Result<(), StorageError> {
        // TODO: 保存Session元数据
        // 1. 创建Session目录
        // 2. 将Session序列化为SessionMeta
        // 3. 序列化为TOML格式
        // 4. 写入session.toml文件
        unimplemented!()
    }
    
    async fn save_agent(
        &self,
        agent: &Agent,
    ) -> Result<(), StorageError> {
        // TODO: 保存Agent数据
        // 1. 创建Agent目录（如果不存在）
        // 2. 将Agent序列化为AgentData
        // 3. 序列化为TOML格式
        // 4. 写入Agent TOML文件
        unimplemented!()
    }
    
    // TODO: 实现其他StorageBackend方法
}
```

## 6. Session生命周期

### 6.1 创建Session

```rust
impl SessionManager {
    /// 创建新Session
    pub async fn create_session(
        &self,
        session_type: SessionType,
        root_agent_config: AgentConfig,
    ) -> Result<Session, SessionError> {
        // TODO: 创建新Session实现
        // 1. 生成SessionId
        // 2. 创建Session实例，初始化字段
        // 3. 创建根Agent并添加到Session
        // 4. 保存Session元数据到存储
        // 5. 添加到内存缓存
        // 6. 返回Session
        unimplemented!()
    }
}
```

### 6.2 恢复Session

```rust
impl SessionManager {
    /// 加载已有Session
    pub async fn load_session(
        &self,
        session_id: SessionId,
    ) -> Result<Session, SessionError> {
        // TODO: 加载已有Session实现
        // 1. 先检查内存缓存
        // 2. 如果缓存不存在，从存储加载Session元数据
        // 3. 获取Session中的所有Agent列表
        // 4. 创建Session实例，初始化字段
        // 5. 加载所有Agent数据
        // 6. 添加到内存缓存
        // 7. 返回Session
        unimplemented!()
    }
}
```

### 6.3 消息处理流程

```rust
impl Session {
    /// 添加消息到Agent
    pub async fn add_message(
        &mut self,
        ulid: AgentUlid,
        role: Role,
        content: String,
        tool_calls: Option<Vec<ToolCall>>,
        tool_call_id: Option<String>,
    ) -> Result<u64, SessionError> {
        // TODO: 添加消息到Agent实现
        // 1. 验证Agent存在
        // 2. 生成下一个消息ID
        // 3. 创建Message实例
        // 4. 添加消息到Agent的消息列表
        // 5. 更新Agent的最后活动时间
        // 6. 异步保存消息到存储
        // 7. 更新Session的更新时间
        // 8. 返回消息ID
        unimplemented!()
    }
    
    /// 获取Agent的完整消息历史
    pub fn get_message_history(
        &self,
        ulid: AgentUlid,
        up_to_id: Option<u64>,
    ) -> Result<Vec<&Message>, SessionError> {
        // TODO: 获取Agent的完整消息历史
        // 1. 验证Agent存在
        // 2. 根据up_to_id过滤消息列表
        // 3. 返回消息引用列表
        unimplemented!()
    }
    
    /// 回溯到指定消息ID（删除之后的所有消息）
    pub async fn rewind_to(
        &mut self,
        ulid: AgentUlid,
        message_id: u64,
    ) -> Result<(), SessionError> {
        // TODO: 回溯到指定消息ID实现
        // 1. 验证Agent存在
        // 2. 保留id <= message_id的消息
        // 3. 重新保存整个Agent数据到存储
        unimplemented!()
    }
}
```

## 7. 上下文管理

### 7.1 上下文组装

```rust
/// 上下文构建器
pub struct ContextBuilder {
    system_messages: Vec<String>,
    conversation: Vec<Message>,
    active_tools: Vec<Tool>,
}

impl ContextBuilder {
    pub fn new() -> Self {
        // TODO: 创建ContextBuilder实例
        // 1. 初始化system_messages为空Vec
        // 2. 初始化conversation为空Vec
        // 3. 初始化active_tools为空Vec
        unimplemented!()
    }
    
    /// 添加系统提示
    pub fn add_system_prompt(&mut self,
        prompt: &str,
    ) -> &mut Self {
        // TODO: 添加系统提示
        // 1. 将prompt添加到system_messages
        // 2. 返回self支持链式调用
        unimplemented!()
    }
    
    /// 添加Agent消息历史
    pub fn with_agent_history(
        &mut self,
        agent: &Agent,
    ) -> &mut Self {
        // TODO: 添加Agent消息历史
        // 1. 将Agent的所有消息添加到conversation
        // 2. 返回self支持链式调用
        unimplemented!()
    }
    
    /// 添加激活的工具
    pub fn with_active_tools(
        &mut self,
        tools: Vec<Tool>,
    ) -> &mut Self {
        // TODO: 添加激活的工具
        // 1. 设置active_tools为传入的tools
        // 2. 返回self支持链式调用
        unimplemented!()
    }
    
    /// 构建最终上下文
    pub fn build(self) -> ChatRequest {
        // TODO: 构建最终上下文
        // 1. 组装系统消息（如果有）
        // 2. 添加对话历史
        // 3. 创建ChatRequest实例
        // 4. 根据active_tools设置tools字段
        // 5. 设置其他默认参数
        unimplemented!()
    }
}
```

## 8. 错误处理

```rust
#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Agent未找到: {0}")]
    AgentNotFound,
    
    #[error("Session未找到: {0}")]
    SessionNotFound(SessionId),
    
    #[error("存储错误: {0}")]
    Storage(#[from] StorageError),
    
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("序列化错误: {0}")]
    Serialization(#[from] toml::ser::Error),
    
    #[error("反序列化错误: {0}")]
    Deserialization(#[from] toml::de::Error),
    
    #[error("无效的Agent关系")]
    InvalidAgentRelation,
    
    #[error("消息ID冲突")]
    MessageIdConflict,
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("序列化错误: {0}")]
    Serialization(String),
    
    #[error("文件损坏: {0}")]
    CorruptedFile(PathBuf),
}
```

---

*关联文档：*
- [TECH.md](TECH.md) - 总体架构文档
- [TECH-MODEL.md](TECH-MODEL.md) - 模型服务模块
- [TECH-AGENT.md](TECH-AGENT.md) - 多智能体协作模块
