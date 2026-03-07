# TECH-TOOL: 工具模块

本文档描述Neco项目的工具模块设计，采用统一的工具接口设计。

## 1. 模块概述

工具模块提供Agent与外部系统交互的能力。

**设计原则：**
- 统一的工具执行接口（ToolExecutor）
- 工具注册表管理所有可用工具
- 工具定义与执行分离

## 2. 工具架构

### 2.1 工具系统架构

```mermaid
graph TB
    subgraph "ToolRegistry"
        TR[工具注册表]
    end
    
    subgraph "内置工具"
        FS[fs::read/write/edit/delete]
        AC[activate::mcp/skill]
        MA[multi-agent::spawn/send/report]
        CT[context::observe]
        WF[workflow]
    end
    
    subgraph "外部工具"
        MCP[mcp::*]
        SK[skill::*]
    end
    
    TR --> FS
    TR --> AC
    TR --> MA
    TR --> CT
    TR --> WF
    TR --> MCP
    TR --> SK
    
    subgraph "执行层"
        TE[ToolExecutor]
    end
    
    TR --> TE
```

### 2.2 工具命名规范

| 工具 | 命名格式 | 示例 |
|------|----------|------|
| 文件系统 | `namespace::action` | `fs::read`, `fs::write` |
| MCP | `mcp::server_name` | `mcp::context7` |
| 多智能体 | `multi-agent::action` | `multi-agent::spawn` |
| 上下文 | `context::action` | `context::observe` |
| 工作流 | `workflow::option` | `workflow::approve` |
| 激活 | `activate::type` | `activate::skill` |

## 3. 工具接口设计

### 3.1 ToolExecutor Trait

```rust
/// 工具能力
#[derive(Debug, Clone, Default)]
pub struct ToolCapabilities {
    pub streaming: bool,
    pub requires_network: bool,
    pub resource_level: ResourceLevel,
    pub concurrent: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum ResourceLevel {
    #[default]
    Low,
    Medium,
    High,
}

/// 工具定义
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub id: ToolId,
    pub description: String,
    pub schema: Value,
    pub capabilities: ToolCapabilities,
    pub timeout: Duration,
}

/// 工具执行上下文
pub struct ToolContext {
    pub session_id: SessionId,
    pub agent_id: AgentId,
    pub working_dir: PathBuf,
}

/// 工具执行结果
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub output: String,
    pub data: Option<Value>,
    pub is_error: bool,
}

/// 工具执行器Trait
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    fn definition(&self) -> &ToolDefinition;
    
    async fn execute(
        &self,
        context: &ToolContext,
        args: Value,
    ) -> Result<ToolResult, ToolError>;
}
```

### 3.2 ToolRegistry Trait

```rust
/// 工具注册表Trait
#[async_trait]
pub trait ToolRegistry: Send + Sync {
    fn register(&self, tool: Arc<dyn ToolExecutor>);
    
    fn get(&self, id: &ToolId) -> Option<Arc<dyn ToolExecutor>>;
    
    fn definitions(&self) -> Vec<ToolDefinition>;
    
    fn timeout(&self, id: &ToolId) -> Duration;
    
    fn set_timeout(&self, prefix: &str, duration: Duration);
    
    fn list_tools(&self) -> Vec<ToolId>;
}

/// 工具ID（强类型）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolId(pub String);

impl ToolId {
    pub fn from_parts(namespace: &str, name: &str) -> Self {
        Self(format!("{}::{}", namespace, name))
    }
    
    pub fn namespace(&self) -> Option<&str> {
        self.0.split("::").next()
    }
    
    pub fn name(&self) -> Option<&str> {
        self.0.split("::").nth(1)
    }
}
```

### 3.3 默认工具注册表实现

```rust
/// 默认工具注册表实现
pub struct DefaultToolRegistry {
    tools: RwLock<HashMap<ToolId, Arc<dyn ToolExecutor>>>,
    timeouts: RwLock<HashMap<String, Duration>>,
}

impl DefaultToolRegistry {
    pub fn new() -> Self {
        // TODO: 实现注册表初始化
        // 1. 创建新的RwLock包装的HashMap用于tools
        // 2. 创建新的RwLock包装的HashMap用于timeouts
        // 3. 注册内置工具：fs::read, fs::write, fs::edit, fs::delete
        // 4. 注册多智能体工具：multi-agent::spawn, send, report
        // 5. 注册上下文工具：context::observe
        // 6. 注册MCP和Skill外部工具
    }
}

#[async_trait]
impl ToolRegistry for DefaultToolRegistry {
    // TODO: 实现以下方法
    // - register: 接收ToolExecutor并注册到tools Map
    // - get: 根据ToolId查找并返回ToolExecutor
    // - definitions: 返回所有已注册工具的定义列表
    // - timeout: 返回工具的超时时间（支持最长前缀匹配）
    // - set_timeout: 设置特定工具的超时时间
    // - list_tools: 返回所有已注册工具的ID列表
}
```

## 4. 文件系统工具

### 4.1 工具定义

| 工具 | 功能 | 超时 |
|------|------|------|
| `fs::read` | 读取文件内容 | 5秒 |
| `fs::write` | 写入文件（完全覆盖） | 10秒 |
| `fs::edit` | 编辑文件（基于verify） | 10秒 |
| `fs::delete` | 删除文件 | 5秒 |

### 4.2 fs::read 实现

```rust
pub mod fs {
    pub struct FileReadTool;
    
    #[async_trait]
    impl ToolExecutor for FileReadTool {
        fn definition(&self) -> &ToolDefinition {
            static DEF: Lazy<ToolDefinition> = Lazy::new(|| ToolDefinition {
                id: ToolId("fs::read".into()),
                description: "读取文件内容".into(),
                schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "文件路径"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "起始行号（1-based）"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "最大读取行数"
                        }
                    },
                    "required": ["path"]
                }),
                capabilities: ToolCapabilities::default(),
                timeout: Duration::from_secs(5),
            });
            &DEF
        }
        
        async fn execute(
            &self,
            context: &ToolContext,
            args: Value,
        ) -> Result<ToolResult, ToolError> {
            // TODO: 实现文件读取逻辑
            // 1. 从args中解析path为String
            // 2. 基于context.working_dir验证路径安全性（禁止../逃离工作目录）
            // 3. 调用std::fs::read_to_string读取文件内容
            // 4. 按行分割后应用offset和limit进行截取
            // 5. 返回包含文件内容的ToolResult
            unimplemented!()
        }
    }
}
```

### 4.3 fs::write 实现

```rust
pub struct FileWriteTool;
    
#[async_trait]
impl ToolExecutor for FileWriteTool {
    fn definition(&self) -> &ToolDefinition {
        static DEF: Lazy<ToolDefinition> = Lazy::new(|| ToolDefinition {
            id: ToolId("fs::write".into()),
            description: "写入文件内容（完全覆盖）".into(),
            schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
            capabilities: ToolCapabilities::default(),
            timeout: Duration::from_secs(10),
        });
        &DEF
    }
    
    async fn execute(
        &self,
        context: &ToolContext,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        // TODO: 实现文件写入逻辑
        // 1. 从args解析path和content
        // 2. 基于context.working_dir构建完整路径
        // 3. 检查父目录是否存在，不存在则创建
        // 4. 使用原子写入模式：写入临时文件后rename
        // 5. 返回写入成功的结果
        unimplemented!()
    }
}
```

### 4.4 fs::edit 实现（带verify）

```rust
pub struct FileEditTool;
    
#[async_trait]
impl ToolExecutor for FileEditTool {
    fn definition(&self) -> &ToolDefinition {
        static DEF: Lazy<ToolDefinition> = Lazy::new(|| ToolDefinition {
            id: ToolId("fs::edit".into()),
            description: "基于verify编辑文件内容".into(),
            schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "verify": {
                        "type": "object",
                        "properties": {
                            "line": { "type": "integer" },
                            "content": { "type": "string" }
                        },
                        "required": ["line", "content"]
                    },
                    "new_content": { "type": "string" }
                },
                "required": ["path", "verify", "new_content"]
            }),
            capabilities: ToolCapabilities::default(),
            timeout: Duration::from_secs(10),
        });
        &DEF
    }
    
    async fn execute(
        &self,
        context: &ToolContext,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        // TODO: 实现文件编辑逻辑
        // 1. 解析参数：path, verify.line, verify.content, new_content
        // 2. 读取文件全部内容，按行分割
        // 3. 定位到verify.line指定的行，调用verify_line_content进行验证
        // 4. 验证通过后，将new_content替换该行内容
        // 5. 使用原子写入方式保存修改后的文件
        // 6. 返回编辑成功的结果
        unimplemented!()
    }
}

/// Verify验证结果
#[derive(Debug, Clone, PartialEq)]
pub enum VerifyResult {
    ExactMatch,
    PrefixMatch,
    Mismatch,
    TooShort,
}

/// Verify验证
pub fn verify_line_content(
    actual: &str,
    expected: &str,
) -> VerifyResult {
    // TODO: 实现verify验证逻辑
    // 1. 去除actual和expected的行尾换行符
    // 2. 如果actual和expected完全相等，返回ExactMatch
    // 3. 如果actual以expected开头且expected长度≥20，返回PrefixMatch
    // 4. 如果actual长度不足20且非完全匹配，返回TooShort
    // 5. 其他情况返回Mismatch
    unimplemented!()
}
```

## 5. 工具数据流

```mermaid
sequenceDiagram
    participant A as Agent
    participant TR as ToolRegistry
    participant T as ToolExecutor
    participant F as Filesystem

    A->>TR: 1. list_tools() / get(tool_id)
    TR-->>A: 返回工具定义
    A->>T: 2. execute(context, args)
    T->>F: 3. 读写文件操作
    F-->>T: 返回结果
    T-->>A: 4. ToolResult
```

**数据流说明：**
1. Agent通过ToolRegistry获取可用工具列表或特定工具定义
2. Agent调用ToolExecutor的execute方法，传入执行上下文和参数
3. ToolExecutor执行具体的工具逻辑（如文件读写）
4. 工具执行完成后返回ToolResult给Agent

## 6. 工具执行状态机

```mermaid
stateDiagram-v2
    [*] --> Idle: 工具注册
    Idle --> Resolving: execute()调用
    Resolving --> Validating: 参数解析完成
    Validating --> Executing: 参数验证通过
    Validating --> Failed: 参数验证失败
    Executing --> Processing: 开始执行
    Processing --> Completed: 执行成功
    Processing --> Failed: 执行出错
    Completed --> Idle: 返回结果
    Failed --> Idle: 返回错误
    Idle --> [*]: 工具注销
```

**状态说明：**
| 状态 | 描述 |
|------|------|
| Idle | 工具空闲，可被调用 |
| Resolving | 正在解析参数 |
| Validating | 正在验证参数 |
| Executing | 正在执行工具逻辑 |
| Processing | 正在处理具体操作 |
| Completed | 执行成功完成 |
| Failed | 执行失败 |

**状态转换触发：**
- `execute()` 调用 → Resolving
- 参数解析完成 → Validating
- 验证通过 → Executing
- 验证失败 → Failed
- 执行完成 → Completed
- 执行出错 → Failed

## 7. 工具错误

```rust
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("参数无效: {0}")]
    InvalidArgs(String),
    
    #[error("执行失败: {0}")]
    Execution(String),
    
    #[error("超时")]
    Timeout,
    
    #[error("权限不足")]
    PermissionDenied,
    
    #[error("资源未找到")]
    NotFound,
    
    #[error("工具未找到: {0}")]
    NotFoundTool(String),
    
    #[error("需要确认")]
    ConfirmationRequired,
    
    #[error("序列化错误: {0}")]
    Serialization(#[from] serde_json::Error),
}
```

---

*关联文档：*
- [TECH.md](TECH.md) - 总体架构文档
- [TECH-SESSION.md](TECH-SESSION.md) - Session管理模块
- [TECH-AGENT.md](TECH-AGENT.md) - Agent模块
- [TECH-MCP.md](TECH-MCP.md) - MCP模块
- [TECH-SKILL.md](TECH-SKILL.md) - Skills模块
