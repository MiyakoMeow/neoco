# TECH-UI: 用户接口模块

本文档描述Neco项目的用户接口模块设计。

## 1. 模块概述

用户接口模块提供多种交互方式：
- **TUI交互模式**（默认）：不提供任何参数时启动，提供交互式终端界面
- **CLI直接模式**（`-m/--message`）：单次执行模式，发送消息后直接返回结果
- **后台守护进程模式**（`agent`子命令）：启动HTTP API服务器

## 2. 用户接口抽象

```rust
#[async_trait]
pub trait UserInterface: Send + Sync {
    async fn init(&mut self) -> Result<(), UiError>;
    async fn get_input(&mut self) -> Result<UserInput, UiError>;
    async fn render(&mut self, output: &AgentOutput) -> Result<(), UiError>;
    async fn ask(&mut self, question: &str, options: Option<Vec<String>>) -> Result<String, UiError>;
    async fn shutdown(&mut self) -> Result<(), UiError>;
}

pub enum UserInput {
    Message(String),
    Command { name: String, args: Vec<String> },
    Exit,
    Interrupt,
}

pub struct AgentOutput {
    pub content: String,
    pub output_type: OutputType,
}

pub enum OutputType {
    Text,
    Markdown,
    Code { language: String },
    ToolResult { tool_name: String },
    Error,
}
```

## 3. CLI直接模式

```rust
pub struct CliInterface {
    args: CliArgs,
    session_manager: Arc<SessionManager>,
}

#[derive(Debug, Parser)]
#[command(name = "neco")]
#[command(about = "Neco - 多智能体协作AI应用", long_about = None)]
#[command(version)]
pub struct CliArgs {
    /// 子命令（用于启动守护进程模式）
    #[command(subcommand)]
    command: Option<Commands>,
    
    /// 直接发送消息（CLI模式），与TUI模式互斥
    /// 
    /// 提供此参数将进入CLI直接模式，执行后立即退出。
    /// 消息内容不能为空，否则将返回错误。
    #[arg(short = 'm', long, global = true)]
    message: Option<String>,
    
    /// 指定Session ID（用于恢复已有会话）
    /// 
    /// 可在TUI模式或CLI模式下使用。
    /// 在TUI模式下，恢复指定会话的交互。
    /// 在CLI模式下，在指定会话中发送消息。
    #[arg(short = 's', long, global = true)]
    session: Option<SessionId>,
    
    /// 指定配置文件路径（覆盖默认查找路径）
    /// 
    /// 默认按以下优先级查找配置文件：
    /// 1. .neco/neco.toml（当前项目，最高优先级）
    /// 2. .agents/neco.toml（当前项目）
    /// 3. ~/.config/neco/neco.toml（用户主配置）
    /// 4. ~/.agents/neco.toml（通用配置，最低优先级）
    /// 
    /// 提供此参数将跳过默认查找，直接使用指定文件。
    #[arg(short = 'c', long, global = true)]
    config: Option<PathBuf>,
    
    /// 工作目录（默认为当前目录）
    /// 
    /// 指定项目根目录，用于查找配置文件和存储数据。
    #[arg(short = 'w', long, global = true, default_value = ".")]
    working_dir: PathBuf,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// 启动后台守护进程模式，提供HTTP API服务
    /// 
    /// 守护进程将启动HTTP服务器，通过REST API提供会话管理和消息交互功能。
    /// 默认监听地址由配置文件指定。
    Agent {
        /// 指定配置文件路径（覆盖默认查找路径）
        #[arg(short = 'c', long)]
        config: Option<PathBuf>,
    },
}

impl CliInterface {
    pub async fn run(&self) -> Result<i32, UiError> {
        // [TODO] 实现CLI运行逻辑
        // 1. 解析CliArgs参数
        // 2. 加载配置文件：
        //    - 如果提供--config参数，使用指定文件
        //    - 否则按优先级查找：.neco/ → .agents/ → ~/.config/neco/ → ~/.agents/
        // 3. 根据参数决定运行模式：
        //    - command=Some(Commands::Agent) → 启动守护进程模式
        //    - message=Some(msg) → CLI直接模式（执行后立即退出）
        //    - 无参数 → TUI交互模式（默认）
        // 4. 如果提供--session参数，恢复已有会话
        // 5. 处理错误并返回适当的退出码
        // 
        // 错误处理：
        // - message参数为空 → 返回错误（不进入TUI）
        // - 配置文件未找到 → 返回错误并提示查找路径
        // - Session ID无效 → 返回错误并提示
        unimplemented!()
    }
}
```

## 4. TUI交互模式（默认）

### 4.1 REPL界面结构

```rust
pub struct TuiInterface {
    terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
    session_manager: Arc<SessionManager>,
    input_buffer: String,
    output_history: Vec<AgentOutput>,
    mode: TuiMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TuiMode {
    Normal,
    Command,
    MultiLine,
}

impl TuiInterface {
    pub fn new(session_manager: Arc<SessionManager>) -> Result<Self, UiError> {
        // [TODO] 初始化终端
        // 1. 使用crossterm创建终端实例
        // 2. 设置终端原始模式和非阻塞输入
        // 3. 初始化输入缓冲区和输出历史
        // 4. 设置初始TuiMode为Normal
        // 5. 配置终端尺寸监听（用于响应式布局）
        unimplemented!()
    }
    
    pub async fn run(&mut self) -> Result<(), UiError> {
        // [TODO] TUI主循环
        // 1. 进入事件循环，持续读取用户输入直到Exit命令
        // 2. 处理输入：根据TuiMode解析输入（Normal模式发送消息，Command模式执行命令）
        // 3. 执行用户输入：调用Agent处理消息或执行特殊命令
        // 4. 渲染输出：将AgentOutput渲染到终端（消息历史区域）
        // 5. 更新状态栏：显示当前模式、会话信息等
        // 6. 处理Interrupt信号（Ctrl+C）中断当前操作
        // 7. 清理终端设置并退出
        unimplemented!()
    }
}
```

### 4.2 TUI界面布局

#### 启动方式

```bash
# 新建会话（默认）
neco

# 恢复已有会话
neco --session <session_id>

# 指定配置文件
neco --config /path/to/config.toml

# 指定工作目录
neco --working-dir /path/to/project
```

#### 界面布局

```mermaid
graph TB
    subgraph "REPL布局"
        M[消息历史]
        S1[状态栏（上方）]
        I[输入框]
        S2[状态栏（下方）]
    end
    
    M --> S1
    S1 --> I
    I --> S2
```

### 4.3 命令列表

| 命令 | 功能 |
|------|------|
| `/new` | 创建新Session |
| `/exit` | 退出应用 |
| `/compact` | 上下文压缩 |
| `/workflow status` | 工作流状态 |
| `/agents tree` | Agent树结构 |

## 5. 后台守护进程模式（agent子命令）

### 5.1 启动方式

```bash
# 使用默认配置启动
neco agent

# 指定配置文件
neco agent --config /path/to/config.toml

# 指定工作目录
neco agent --working-dir /path/to/project
```

### 5.2 配置结构

### 5.1 API端点

```rust
pub struct DaemonInterface {
    config: DaemonConfig,
    session_manager: Arc<SessionManager>,
    workflow_engine: Arc<WorkflowEngine>,
}

pub struct DaemonConfig {
    // 服务器绑定地址
    pub host: String,
    pub port: u16,
    
    // TLS配置
    pub tls: Option<TlsConfig>,
    
    // 认证配置
    pub auth: AuthConfig,
    
    // 速率限制
    pub rate_limit: RateLimitConfig,
    
    // CORS配置
    pub cors: CorsConfig,
    
    // 服务器配置
    pub server: ServerConfig,
}

pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
    pub client_ca_path: Option<String>, // 客户端证书验证（mTLS）
}

pub struct AuthConfig {
    pub api_keys: Vec<String>,
    pub jwt_secret: Option<String>,
    pub jwt_expiration_sec: Option<u64>,
}

pub struct RateLimitConfig {
    pub enabled: bool,
    pub requests_per_minute: u32,
    pub burst_size: u32,
}

pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
    pub allow_credentials: bool,
    pub max_age_sec: u64,
}

pub struct ServerConfig {
    pub max_connections: usize,
    pub request_timeout_sec: u64,
    pub shutdown_timeout_sec: u64,
    pub worker_threads: Option<usize>,
}

impl DaemonInterface {
    pub async fn run(&self) -> Result<(), UiError> {
        // [TODO] 启动HTTP服务器
        // 1. 从DaemonConfig读取host和port配置
        // 2. 使用HTTP框架（如axum或actix-web）创建服务
        // 3. 注册REST API路由（/api/v1/sessions, /api/v1/workflows等）
        // 4. 挂载SessionManager和WorkflowEngine到应用状态
        // 5. 启动HTTP服务器监听指定地址
        // 6. 处理优雅关闭（处理SIGTERM/SIGINT信号）
        unimplemented!()
    }
}
```

### 5.2 REST API

#### 5.2.1 创建会话

**请求：**
```json
POST /api/v1/sessions
Content-Type: application/json

{
    "session_id": "session_001",
    "config": {
        "model": "gpt-4",
        "temperature": 0.7
    }
}
```

**响应：**
```json
{
    "status": "success",
    "session_id": "session_001",
    "created_at": "2026-03-07T10:00:00Z"
}
```

#### 5.2.2 发送消息

**请求：**
```json
POST /api/v1/sessions/{session_id}/messages
Content-Type: application/json

{
    "content": "帮我分析这段代码",
    "type": "text"
}
```

**响应：**
```json
{
    "status": "success",
    "message_id": "msg_001",
    "output": {
        "content": "分析结果...",
        "type": "text"
    },
    "timestamp": "2026-03-07T10:01:00Z"
}
```

#### 5.2.3 获取会话状态

**请求：**
```json
GET /api/v1/sessions/{session_id}/status
```

**响应：**
```json
{
    "status": "active",
    "session_id": "session_001",
    "message_count": 5,
    "last_activity": "2026-03-07T10:01:00Z"
}
```

#### 5.2.4 终止会话

**请求：**
```json
DELETE /api/v1/sessions/{session_id}
```

**响应：**
```json
{
    "status": "success",
    "session_id": "session_001",
    "terminated_at": "2026-03-07T10:05:00Z"
}
```

#### 5.2.5 错误响应格式

```json
{
    "status": "error",
    "error": {
        "code": "SESSION_NOT_FOUND",
        "message": "会话不存在",
        "details": {}
    }
}
```

## 6. 错误处理

```rust
#[derive(Debug, Error)]
pub enum UiError {
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("终端错误: {0}")]
    Terminal(String),
    
    #[error("Session错误: {0}")]
    Session(#[from] SessionError),
    
    #[error("API错误: {0}")]
    Api(#[from] ApiError),
}

#[derive(Debug, Error)]
pub enum ApiError {
    // 4xx 客户端错误
    #[error("Session未找到")]
    SessionNotFound,
    
    #[error("未授权访问: {0}")]
    Unauthorized(String),
    
    #[error("无效请求: {0}")]
    BadRequest(String),
    
    #[error("冲突: {0}")]
    Conflict(String),
    
    #[error("资源不存在: {0}")]
    NotFound(String),
    
    #[error("请求超时")]
    RequestTimeout,
    
    // 5xx 服务器错误
    #[error("内部错误: {0}")]
    Internal(String),
    
    #[error("服务不可用: {0}")]
    ServiceUnavailable(String),
    
    #[error("网关错误: {0}")]
    BadGateway(String),
}

impl ApiError {
    pub fn status_code(&self) -> u16 {
        match self {
            ApiError::SessionNotFound => 404,
            ApiError::Unauthorized(_) => 401,
            ApiError::BadRequest(_) => 400,
            ApiError::Conflict(_) => 409,
            ApiError::NotFound(_) => 404,
            ApiError::RequestTimeout => 408,
            ApiError::Internal(_) => 500,
            ApiError::ServiceUnavailable(_) => 503,
            ApiError::BadGateway(_) => 502,
        }
    }
}
```

## 7. 使用示例

### 7.1 TUI交互模式

```bash
# 启动交互式会话
$ neco
> 你好，请帮我分析这段代码
[AI响应...]

# 恢复上次会话
$ neco --session 01ARZ3NDEKTSV4RRFFQ69G5FAV
> 继续我们之前的话题
[AI响应...]
```

### 7.2 CLI直接模式

```bash
# 单次查询
$ neco -m "什么是Rust的所有权系统？"
[直接返回结果，退出]

# 在已有会话中查询
$ neco -m "继续解释" --session 01ARZ3NDEKTSV4RRFFQ69G5FAV
[直接返回结果，退出]

# 指定配置文件
$ neco -m "帮我分析" --config ~/.config/neco/custom.toml
```

**错误处理示例**：
```bash
# 消息为空（报错）
$ neco -m ""
error: The value '--message <MESSAGE>' requires a value, but none was supplied

# 未找到配置文件
$ neco --config /nonexistent/config.toml
error: 配置文件未找到: /nonexistent/config.toml
```

### 7.3 后台守护进程模式

```bash
# 启动守护进程
$ neco agent
[INFO] Starting Neco daemon on http://127.0.0.1:8080
[INFO] Config loaded from ~/.config/neco/neco.toml
[INFO] Ready to accept connections

# 使用API（通过curl）
$ curl -X POST http://127.0.0.1:8080/api/v1/sessions \
  -H "Content-Type: application/json" \
  -d '{"config": {"model": "gpt-4"}}'

$ curl -X POST http://127.0.0.1:8080/api/v1/sessions/{session_id}/messages \
  -H "Content-Type: application/json" \
  -d '{"content": "你好，请分析这段代码"}'
```

### 7.4 配置文件查找示例

```bash
# 不指定--config时，自动查找配置文件
$ neco
[INFO] Loading config from: .neco/neco.toml  # 优先级1

# 如果.neco/不存在，尝试下一个
$ neco
[INFO] Loading config from: .agents/neco.toml  # 优先级2

# 如果当前目录没有配置，使用用户配置
$ neco
[INFO] Loading config from: ~/.config/neco/neco.toml  # 优先级3

# 使用--config覆盖默认查找
$ neco --config /custom/path/config.toml
[INFO] Loading config from: /custom/path/config.toml
```

---

*关联文档：*
- [TECH.md](TECH.md) - 总体架构文档
- [TECH-SESSION.md](TECH-SESSION.md) - Session管理模块
- [TECH-WORKFLOW.md](TECH-WORKFLOW.md) - 工作流模块
