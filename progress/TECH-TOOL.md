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
| 工作流 | `workflow::option` | `workflow::pass`, `workflow::option` |
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
    /// JSON Schema 格式的参数定义
    /// 使用 JSON Schema Draft 2020-12 规范
    /// 参考：https://json-schema.org/draft/2020-12/release-notes
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
    pub output: ToolOutput,
    pub is_error: bool,
}

#[derive(Debug, Clone)]
pub enum ToolOutput {
    Text(String),
    Json(Value),
    Binary(Vec<u8>),
    Empty,
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

    pub fn from_parts_validated(namespace: &str, name: &str) -> Result<Self, ToolError> {
        // 验证namespace：只允许小写字母、数字、连字符
        if !namespace.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(ToolError::InvalidArgs(format!(
                "Invalid namespace '{}': only lowercase letters, digits, and hyphens allowed",
                namespace
            )));
        }

        // 验证name：只允许小写字母、数字、连字符、下划线
        if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
            return Err(ToolError::InvalidArgs(format!(
                "Invalid name '{}': only lowercase letters, digits, hyphens, and underscores allowed",
                name
            )));
        }

        Ok(Self(format!("{}::{}", namespace, name)))
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
        let mut registry = Self {
            tools: RwLock::new(HashMap::new()),
            timeouts: RwLock::new(HashMap::new()),
        };

        // 注册内置工具（按优先级排序）
        // 1. 核心工具（文件系统）：fs::read, fs::write, fs::edit, fs::delete
        registry.register(FileReadTool);
        registry.register(FileWriteTool);
        registry.register(FileEditTool);
        registry.register(FileDeleteTool);
        
        // 2. 上下文工具：context::observe
        // 依赖注入：observer 实例由外部容器在运行时提供
        registry.register(ContextObserveTool::new(/* observer: Arc<dyn ContextObserver> */));
        
        // 2.1 上下文工具：context::compact
        // 依赖注入：compression_service 实例由外部容器在运行时提供
        registry.register(ContextCompactTool::new(/* compression_service: Arc<CompressionService> */));
        
        // 3. 多智能体工具：multi-agent::spawn, multi-agent::send, multi-agent::report
        registry.register(MultiAgentSpawnTool);
        registry.register(MultiAgentSendTool);
        registry.register(MultiAgentReportTool);
        
        // 4. 激活工具：activate::skill, activate::mcp
        registry.register(ActivateSkillTool);
        registry.register(ActivateMcpTool);
        
        // 5. 工作流工具：workflow::pass, workflow::option
        registry.register(WorkflowOptionTool);
        registry.register(WorkflowPassTool);

        // 注意：MCP和Skill外部工具在运行时动态注册

        registry
    }
}

#[async_trait]
impl ToolRegistry for DefaultToolRegistry {
    async fn register<T: ToolExecutor + Send + Sync + 'static>(&mut self, tool: T) {
        let def = tool.definition();
        let executor = Arc::new(tool);
        self.tools.write().await.insert(def.id.clone(), executor);
    }
    
    async fn get(&self, id: &ToolId) -> Option<Arc<dyn ToolExecutor>> {
        self.tools.read().await.get(id).cloned()
    }
    
    async fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.read().await.values()
            .map(|tool| tool.definition().clone())
            .collect()
    }
    
    async fn timeout(&self, id: &ToolId) -> Option<Duration> {
        let tools = self.tools.read().await;
        if let Some(tool) = tools.get(id) {
            Some(tool.definition().timeout)
        } else {
            self.timeouts.read().await.get(id.0.as_str()).copied()
        }
    }
    
    async fn set_timeout(&self, id: ToolId, timeout: Duration) {
        self.timeouts.write().await.insert(id.0, timeout);
    }
    
    async fn list_tools(&self) -> Vec<ToolId> {
        self.tools.read().await.keys().cloned().collect()
    }
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
            // 2. 验证路径安全性：
            //    a. 使用std::fs::canonicalize规范化路径，解析所有符号链接和相对路径
            //    b. 确保规范化后的绝对路径以context.working_dir的规范化路径为前缀
            //    c. 防止路径遍历攻击（../）、符号链接逃逸、硬链接逃逸
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
            // 2. 验证路径安全性：
            //    a. 使用std::fs::canonicalize规范化路径，解析所有符号链接和相对路径
            //    b. 确保规范化后的绝对路径以context.working_dir的规范化路径为前缀
            //    c. 防止路径遍历攻击（../）、符号链接逃逸、硬链接逃逸
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
            // 2. 验证路径安全性：
            //    a. 使用std::fs::canonicalize规范化路径，解析所有符号链接和相对路径
            //    b. 确保规范化后的绝对路径以context.working_dir的规范化路径为前缀
            //    c. 防止路径遍历攻击（../）、符号链接逃逸、硬链接逃逸
            // 3. 读取文件全部内容，按行分割
            // 4. 定位到verify.line指定的行，调用verify_line_content进行验证
            // 5. 验证通过后，将new_content替换该行内容
            // 6. 使用原子写入方式保存修改后的文件
            // 7. 返回编辑成功的结果
        unimplemented!()
    }
}

/// Verify验证结果
#[derive(Debug, Clone, PartialEq)]
#[must_use = "VerifyResult must be handled"]
pub enum VerifyResult {
    ExactMatch,
    PrefixMatch,
    Mismatch,
    TooShort,
}

/// Verify验证配置
pub struct VerifyConfig {
    /// 前缀匹配的最小长度阈值
    pub prefix_match_threshold: usize,
}

impl Default for VerifyConfig {
    fn default() -> Self {
        Self {
            prefix_match_threshold: 20,
        }
    }
}

/// Verify验证
pub fn verify_line_content(
    actual: &str,
    expected: &str,
) -> VerifyResult {
    verify_line_content_with_config(actual, expected, &VerifyConfig::default())
}

/// 使用自定义配置的Verify验证
pub fn verify_line_content_with_config(
    actual: &str,
    expected: &str,
    config: &VerifyConfig,
) -> VerifyResult {
    // TODO: 实现verify验证逻辑
    // 1. 去除actual和expected的行尾换行符
    // 2. 如果actual和expected完全相等，返回ExactMatch
    // 3. 如果actual以expected开头且expected长度≥config.prefix_match_threshold，返回PrefixMatch
    // 4. 如果actual长度不足config.prefix_match_threshold且非完全匹配，返回TooShort
    // 5. 其他情况返回Mismatch
    unimplemented!()
}
```

### 4.5 fs::delete 实现

```rust
pub struct FileDeleteTool;
    
#[async_trait]
impl ToolExecutor for FileDeleteTool {
    fn definition(&self) -> &ToolDefinition {
        static DEF: Lazy<ToolDefinition> = Lazy::new(|| ToolDefinition {
            id: ToolId("fs::delete".into()),
            description: "删除文件".into(),
            schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "要删除的文件路径"
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
        // TODO: 实现文件删除逻辑
        // 1. 从args解析path为String
        // 2. 验证路径安全性：
        //    a. 使用std::fs::canonicalize规范化路径，解析所有符号链接和相对路径
        //    b. 确保规范化后的绝对路径以context.working_dir的规范化路径为前缀
        //    c. 防止路径遍历攻击（../）、符号链接逃逸、硬链接逃逸
        // 3. 检查文件是否存在
        // 4. 调用std::fs::remove_file删除文件
        // 5. 返回删除成功的结果
        unimplemented!()
    }
}
```

## 5. 上下文工具

> 上下文工具帮助 Agent 管理内存，遵循 Arena Allocator 心智模型。

### 5.1.1 工具定义

| 工具 | 功能 | 超时 |
|------|------|------|
| `context::observe` | 观测上下文状态，获取 Dashboard | 5秒 |
| `context::compact` | 主动压缩上下文（Layer A） | 30秒 |

### 5.1.2 context::observe

```rust
impl ContextObserveTool {
    pub fn new(observer: Arc<dyn ContextObserver>) -> Self {
        Self { observer }
    }
}

pub struct ContextObserveTool {
    observer: Arc<dyn ContextObserver>,
}

#[async_trait]
impl ToolExecutor for ContextObserveTool {
    fn definition(&self) -> &ToolDefinition {
        static DEF: Lazy<ToolDefinition> = Lazy::new(|| ToolDefinition {
            id: ToolId("context::observe".into()),
            description: "观测上下文状态，获取内存使用仪表盘".into(),
            schema: json!({
                "type": "object",
                "properties": {
                    "filter": {
                        "type": "object",
                        "description": "可选的过滤条件"
                    }
                }
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
        // TODO: 实现上下文观测
        // 1. 从 args 解析 filter 参数
        // 2. 调用 ContextObserver 获取上下文状态
        // 3. 构建 Dashboard 返回：
        //    • Usage: xx% (used/total)
        //    • Steps since tag: xx
        //    • Pruning status: Stage X
        //    • Est. turns left: ~xx
        unimplemented!()
    }
}
```

### 5.1.3 context::compact

```rust
impl ContextCompactTool {
    pub fn new(compression_service: Arc<CompressionService>) -> Self {
        Self { compression_service }
    }
}

pub struct ContextCompactTool {
    compression_service: Arc<CompressionService>,
}

#[async_trait]
impl ToolExecutor for ContextCompactTool {
    fn definition(&self) -> &ToolDefinition {
        static DEF: Lazy<ToolDefinition> = Lazy::new(|| ToolDefinition {
            id: ToolId("context::compact".into()),
            description: "主动压缩上下文，将历史消息压缩为摘要（Agent主动管理内存）".into(),
            schema: json!({
                "type": "object",
                "properties": {
                    "tag": {
                        "type": "string",
                        "description": "压缩起点标记，从该标记到当前位置的消息将被压缩"
                    }
                },
                "required": ["tag"]
            }),
            capabilities: ToolCapabilities::default(),
            timeout: Duration::from_secs(30),
        });
        &DEF
    }
    
    async fn execute(
        &self,
        context: &ToolContext,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        // TODO: 实现上下文压缩 (Layer A: Agent 主动压缩)
        // 1. 从 args 解析 tag 参数
        // 2. 定位 tag 位置到当前位置的消息区间
        // 3. 调用 CompressionService 生成摘要
        // 4. 替换消息区间为 summary
        // 5. 保留原始历史（添加 backup tag）
        unimplemented!()
    }
}
```

## 6. 工具数据流

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

## 7. 工具执行状态机

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

## 8. 工具错误

```rust
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("参数无效: {0}")]
    InvalidArgs(String),
    
    #[error("执行失败: {0}")]
    Execution(#[source] std::io::Error),
    
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

impl ToolError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Timeout | Self::Execution(e) if e.kind() == std::io::ErrorKind::NotFound)
    }
}
```

---

*关联文档：*
- [TECH.md](TECH.md) - 总体架构文档
- [TECH-SESSION.md](TECH-SESSION.md) - Session管理模块
- [TECH-AGENT.md](TECH-AGENT.md) - Agent模块
- [TECH-MCP.md](TECH-MCP.md) - MCP模块
- [TECH-SKILL.md](TECH-SKILL.md) - Skills模块
