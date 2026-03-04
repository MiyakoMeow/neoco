# TECH-CONFIG: 配置管理模块

本文档描述Neco项目的配置管理模块设计，包括配置加载、合并策略和访问接口。

## 1. 模块概述

配置管理模块负责加载、验证和提供所有配置数据。配置分为静态配置（用户定义）和动态配置（运行时）。

## 2. 配置来源

### 2.1 配置目录结构

```
~/.config/neco/                    # 主配置目录
├── neco.toml                     # 主配置文件
├── neco.<tag>.toml               # 带标签的配置文件
├── prompts/
│   ├── base.md                   # 基础提示词组件
│   └── multi-agent.md            # 多智能体提示词
├── agents/
│   ├── coder.md                  # Agent定义
│   └── reviewer.md
├── skills/                       # Skills目录
└── workflows/
    └── prd/
        ├── workflow.toml         # 工作流定义
        ├── neco.toml             # 工作流特定配置
        └── agents/
            └── review.md         # 工作流特定Agent
```

### 2.2 配置优先级

```mermaid
graph BT
    subgraph "优先级从高到低"
        A[环境变量]
        B[命令行参数]
        C[工作流特定配置]
        D[带标签配置 neco.<tag>.toml]
        E[主配置 neco.toml]
        F[内置默认值]
    end
    
    A > B > C > D > E > F
```

## 3. 数据结构设计

### 3.1 配置根结构

```rust
/// 完整配置结构
pub struct NecoConfig {
    /// 模型组定义
    pub model_groups: HashMap<String, ModelGroup>,
    
    /// 模型提供商定义
    pub model_providers: HashMap<String, ModelProvider>,
    
    /// MCP服务器定义
    pub mcp_servers: HashMap<String, McpServer>,
    
    /// 系统配置
    pub system: SystemConfig,
    
    /// 配置来源追踪（用于调试）
    _sources: ConfigSources,
}

/// 配置来源追踪
struct ConfigSources {
    files: Vec<PathBuf>,
    env_vars: HashMap<String, String>,
}
```

### 3.2 模型组配置

```rust
/// 模型组：用于故障转移和负载均衡
pub struct ModelGroup {
    /// 模型标识符列表（按优先级排序）
    /// 格式: "provider/model" 或 "provider/model?param=value"
    pub models: Vec<String>,
}

/// 模型标识符解析
pub struct ModelRef {
    /// 提供商ID
    pub provider_id: String,
    /// 模型名称
    pub model_name: String,
    /// 调用参数（覆盖默认值）
    pub params: HashMap<String, String>,
}

impl FromStr for ModelRef {
    type Err = ConfigError;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // 解析格式: "provider/model?temperature=0.1&reasoning_effort=high"
        let (provider_model, query) = s.split_once('?').unwrap_or((s, ""));
        let (provider_id, model_name) = provider_model
            .split_once('/')
            .ok_or(ConfigError::InvalidModelRef)?;
        
        let params = querystring::querify(query)
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        
        Ok(ModelRef {
            provider_id: provider_id.to_string(),
            model_name: model_name.to_string(),
            params,
        })
    }
}
```

### 3.3 模型提供商配置

```rust
/// 模型提供商配置
pub struct ModelProvider {
    /// 提供商类型（决定如何调用）
    pub provider_type: ProviderType,
    
    /// 显示名称
    pub name: String,
    
    /// API基础URL
    pub base_url: Url,
    
    /// API密钥配置
    pub api_key: ApiKeyConfig,
    
    /// 默认请求参数
    pub default_params: HashMap<String, Value>,
    
    /// 超时配置
    pub timeout: Duration,
    
    /// 重试配置
    pub retry: RetryConfig,
}

/// 提供商类型
pub enum ProviderType {
    /// OpenAI兼容API
    OpenAI,
    /// Anthropic API
    Anthropic,
    /// OpenRouter API
    OpenRouter,
    /// OpenAI Responses API（预留）
    OpenAIResponses,
}

/// API密钥配置（三种方式，优先级从高到低）
pub enum ApiKeyConfig {
    /// 单个环境变量
    Env(String),
    /// 多个环境变量（轮询使用）
    EnvList(Vec<String>),
    /// 直接写入（不推荐，仅用于测试）
    Direct(SecretString),
}

impl ApiKeyConfig {
    /// 获取API密钥
    pub fn get_key(&self) -> Result<SecretString, ConfigError> {
        match self {
            ApiKeyConfig::Env(var) => env::var(var)
                .map(SecretString::new)
                .map_err(|_| ConfigError::EnvVarNotFound(var.clone())),
            ApiKeyConfig::EnvList(vars) => {
                for var in vars {
                    if let Ok(key) = env::var(var) {
                        return Ok(SecretString::new(key));
                    }
                }
                Err(ConfigError::NoEnvVarFound)
            }
            ApiKeyConfig::Direct(key) => Ok(key.clone()),
        }
    }
}

/// 重试配置
pub struct RetryConfig {
    /// 最大重试次数
    pub max_retries: u32,
    /// 初始退避时间
    pub initial_backoff: Duration,
    /// 退避乘数
    pub backoff_multiplier: f64,
    /// 最大退避时间
    pub max_backoff: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_secs(1),
            backoff_multiplier: 2.0,
            max_backoff: Duration::from_secs(4),
        }
    }
}
```

### 3.4 MCP服务器配置

```rust
/// MCP服务器配置
pub struct McpServer {
    /// 传输类型
    pub transport: McpTransport,
    
    /// 环境变量
    pub env: HashMap<String, String>,
    
    /// 服务器状态（运行时填充）
    #[serde(skip)]
    pub status: ServerStatus,
}

/// MCP传输方式
pub enum McpTransport {
    /// 本地stdio传输
    Stdio {
        /// 命令
        command: String,
        /// 参数
        args: Vec<String>,
    },
    /// HTTP传输
    Http {
        /// 服务器URL
        url: Url,
        /// Bearer Token环境变量名
        bearer_token_env: Option<String>,
        /// 额外HTTP头
        headers: HashMap<String, String>,
    },
}

impl McpServer {
    /// 判断是否使用stdio模式
    pub fn is_stdio(&self) -> bool {
        matches!(self.transport, McpTransport::Stdio { .. })
    }
    
    /// 获取bearer token（HTTP模式）
    pub fn get_bearer_token(&self) -> Option<String> {
        match &self.transport {
            McpTransport::Http { bearer_token_env, .. } => {
                bearer_token_env.as_ref()
                    .and_then(|env| std::env::var(env).ok())
            }
            _ => None,
        }
    }
}
```

### 3.5 系统配置

```rust
/// 系统级配置
pub struct SystemConfig {
    /// 存储配置
    pub storage: StorageConfig,
    
    /// 上下文压缩配置
    pub context: ContextConfig,
    
    /// 工具配置
    pub tools: ToolsConfig,
    
    /// UI配置
    pub ui: UiConfig,
}

/// 存储配置
pub struct StorageConfig {
    /// Session存储目录
    pub session_dir: PathBuf,
    /// 是否启用压缩
    pub compression: bool,
}

/// 上下文配置
pub struct ContextConfig {
    /// 自动压缩阈值（上下文窗口百分比）
    pub auto_compact_threshold: f64,
    /// 是否启用自动压缩
    pub auto_compact_enabled: bool,
}

/// 工具配置
pub struct ToolsConfig {
    /// 超时配置（按工具前缀匹配）
    pub timeouts: HashMap<String, Duration>,
    /// 默认超时
    pub default_timeout: Duration,
}

impl ToolsConfig {
    /// 获取工具超时（最长前缀匹配）
    pub fn get_timeout(&self, tool_id: &str) -> Duration {
        let mut best_match: Option<(&str, Duration)> = None;
        
        for (prefix, duration) in &self.timeouts {
            if tool_id.starts_with(prefix) {
                if best_match.map_or(true, |(best_prefix, _)| prefix.len() > best_prefix.len()) {
                    best_match = Some((prefix, *duration));
                }
            }
        }
        
        best_match.map_or(self.default_timeout, |(_, d)| d)
    }
}

/// UI配置
pub struct UiConfig {
    /// 默认运行模式
    pub default_mode: RunMode,
}

pub enum RunMode {
    Direct,
    Repl,
    Daemon,
}
```

## 4. 配置合并策略

### 4.1 合并规则

```mermaid
graph TD
    subgraph "合并策略"
        A[标量类型] -->|后覆盖前| B[直接替换]
        C[数组类型] -->|默认替换| D[特殊语法 +追加]
        E[对象类型] -->|递归合并| F[深度合并]
    end
```

**详细规则：**

| 类型 | 策略 | 示例 |
|-----|------|------|
| 标量（字符串、数字、布尔） | 后覆盖前 | `name = "new"` 覆盖旧值 |
| 数组 | 后替换前 | `models = ["a", "b"]` 完全替换 |
| 数组追加 | 特殊语法 `+` | `models = ["+c", "+d"]` 追加元素 |
| 对象 | 递归深度合并 | 字段级合并，子对象递归 |

### 4.2 合并实现

```rust
/// 配置合并器
pub struct ConfigMerger;

impl ConfigMerger {
    /// 合并两个配置值
    pub fn merge(base: &mut Value, override_: Value) {
        match (base, override_) {
            // 都是对象：递归合并
            (Value::Object(base_map), Value::Object(override_map)) => {
                for (key, override_val) in override_map {
                    if let Some(base_val) = base_map.get_mut(&key) {
                        Self::merge(base_val, override_val);
                    } else {
                        base_map.insert(key, override_val);
                    }
                }
            }
            
            // 处理数组追加语法
            (Value::Array(base_arr), Value::Array(override_arr)) => {
                for val in override_arr {
                    if let Value::String(s) = &val {
                        if s.starts_with('+') {
                            // 追加模式
                            base_arr.push(Value::String(s[1..].to_string()));
                        } else {
                            // 替换模式：清空后添加
                            base_arr.clear();
                            base_arr.push(val);
                            break;
                        }
                    } else {
                        base_arr.push(val);
                    }
                }
            }
            
            // 标量：直接替换
            (base, override_) => {
                *base = override_;
            }
        }
    }
}
```

## 5. 配置加载流程

### 5.1 加载时序

```mermaid
sequenceDiagram
    participant App as 应用程序
    participant Loader as ConfigLoader
    participant Parser as TOML解析器
    participant Merger as ConfigMerger
    participant Validator as ConfigValidator

    App->>Loader: load_config()
    
    Loader->>Loader: 查找配置文件
    Note over Loader: 按优先级顺序：
    Note over Loader: neco.toml <br/> neco.*.toml <br/> 工作流配置
    
    loop 遍历配置文件
        Loader->>Parser: 解析文件
        Parser-->>Loader: 返回Value
        Loader->>Merger: 合并配置
    end
    
    Loader->>Validator: 验证配置
    Validator->>Validator: 检查必需字段
    Validator->>Validator: 检查引用完整性
    Validator->>Validator: 验证路径存在性
    
    alt 验证失败
        Validator-->>App: 返回错误
    else 验证成功
        Validator-->>Loader: 返回NecoConfig
        Loader-->>App: 返回配置
    end
```

### 5.2 配置验证

```rust
/// 配置验证器
pub struct ConfigValidator;

impl ConfigValidator {
    /// 验证完整配置
    pub fn validate(config: &NecoConfig) -> Result<(), ConfigError> {
        // 1. 验证模型组引用有效性
        Self::validate_model_groups(config)?;
        
        // 2. 验证提供商配置
        Self::validate_providers(config)?;
        
        // 3. 验证MCP服务器配置
        Self::validate_mcp_servers(config)?;
        
        // 4. 验证目录存在性
        Self::validate_paths(config)?;
        
        Ok(())
    }
    
    fn validate_model_groups(config: &NecoConfig) -> Result<(), ConfigError> {
        for (group_name, group) in &config.model_groups {
            for model_ref_str in &group.models {
                let model_ref = model_ref_str.parse::<ModelRef>()
                    .map_err(|e| ConfigError::InvalidModelRef {
                        group: group_name.clone(),
                        model: model_ref_str.clone(),
                        source: e,
                    })?;
                
                // 检查提供商是否存在
                if !config.model_providers.contains_key(&model_ref.provider_id) {
                    return Err(ConfigError::ProviderNotFound {
                        group: group_name.clone(),
                        provider: model_ref.provider_id,
                    });
                }
            }
        }
        Ok(())
    }
    
    fn validate_providers(config: &NecoConfig) -> Result<(), ConfigError> {
        for (name, provider) in &config.model_providers {
            // 验证API密钥可访问
            match provider.api_key.get_key() {
                Ok(_) => {}
                Err(e) => {
                    warn!("Provider '{}' API key not available: {}", name, e);
                    // 非阻塞，仅警告
                }
            }
        }
        Ok(())
    }
}
```

## 6. 热重载支持

### 6.1 热重载流程

```mermaid
graph TB
    subgraph "热重载机制"
        W[文件系统Watcher] -->|文件变更| D[去抖动]
        D -->|延迟500ms| L[重新加载]
        L -->|解析&合并| M[配置合并]
        M -->|验证| V[配置验证]
        V -->|成功| U[更新共享配置]
        V -->|失败| R[回滚&记录日志]
    end
```

### 6.2 线程安全配置访问

```rust
/// 线程安全的配置管理器
pub struct ConfigManager {
    /// 当前配置（读写锁）
    config: RwLock<Arc<NecoConfig>>,
    /// 配置变更通知
    change_tx: broadcast::Sender<ConfigChange>,
}

impl ConfigManager {
    /// 获取当前配置（只读）
    pub fn get_config(&self) -> Arc<NecoConfig> {
        self.config.read().unwrap().clone()
    }
    
    /// 更新配置（热重载）
    pub fn update_config(&self, new_config: NecoConfig) -> Result<(), ConfigError> {
        // 验证新配置
        ConfigValidator::validate(&new_config)?;
        
        // 计算变更
        let old_config = self.get_config();
        let changes = Self::diff_configs(&old_config, &new_config);
        
        // 更新配置
        *self.config.write().unwrap() = Arc::new(new_config);
        
        // 通知订阅者
        let _ = self.change_tx.send(ConfigChange { changes });
        
        Ok(())
    }
    
    /// 订阅配置变更
    pub fn subscribe_changes(&self) -> broadcast::Receiver<ConfigChange> {
        self.change_tx.subscribe()
    }
}

/// 配置变更通知
pub struct ConfigChange {
    pub changes: Vec<ConfigDiff>,
}

pub enum ConfigDiff {
    ModelGroupAdded(String),
    ModelGroupRemoved(String),
    ModelProviderChanged(String),
    McpServerAdded(String),
    McpServerRemoved(String),
}
```

## 7. 使用示例

### 7.1 TOML配置示例

```toml
# neco.toml - 主配置文件

# 模型组定义
[model_groups.frontier]
models = ["zhipuai/glm-4.7"]

[model_groups.smart]
models = ["zhipuai/glm-4.7?reasoning_effort=high"]

[model_groups.balanced]
models = ["zhipuai/glm-4.7", "minimax-cn/MiniMax-M2.5"]

# 模型提供商定义
[model_providers.zhipuai]
type = "openai"
name = "ZhipuAI"
base = "https://open.bigmodel.cn/api/paas/v4"
api_key_env = "ZHIPU_API_KEY"

[model_providers.zhipuai.retry]
max_retries = 3
initial_backoff = 1
backoff_multiplier = 2

# MCP服务器定义
[mcp_servers.context7]
command = "npx"
args = ["-y", "@upstash/context7-mcp"]

[mcp_servers.context7.env]
MY_ENV_VAR = "MY_ENV_VALUE"

[mcp_servers.figma]
url = "https://mcp.figma.com/mcp"
bearer_token_env = "FIGMA_OAUTH_TOKEN"
http_headers = { "X-Figma-Region" = "us-east-1" }

# 系统配置
[system]

[system.storage]
session_dir = "~/.local/neco"
compression = true

[system.context]
auto_compact_threshold = 0.9
auto_compact_enabled = true

[system.tools]
default_timeout = 30

[system.tools.timeouts]
"fs" = 10
"fs::read" = 5
"mcp" = 60
```

### 7.2 代码使用示例

```rust
use neco_config::{ConfigLoader, ConfigManager};

// 加载配置
let config = ConfigLoader::new()
    .with_config_dir("~/.config/neco")
    .load()?;

// 获取模型组
let model_group = config.model_groups.get("smart")
    .ok_or_else(|| Error::ModelGroupNotFound)?;

// 获取第一个模型
let first_model = &model_group.models[0];
let model_ref = first_model.parse::<ModelRef>()?;

// 获取提供商配置
let provider = config.model_providers.get(&model_ref.provider_id)
    .ok_or_else(|| Error::ProviderNotFound)?;

// 获取API密钥
let api_key = provider.api_key.get_key()?;

// 获取工具超时
let timeout = config.system.tools.get_timeout("fs::read");
```

## 8. 错误类型

```rust
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("配置文件未找到: {0}")]
    FileNotFound(PathBuf),
    
    #[error("TOML解析错误: {0}")]
    ParseError(#[from] toml::de::Error),
    
    #[error("无效的模型引用 '{model}' 在组 '{group}'")]
    InvalidModelRef {
        group: String,
        model: String,
        source: ParseError,
    },
    
    #[error("提供商未找到: {provider} (在组 {group} 中引用)")]
    ProviderNotFound {
        group: String,
        provider: String,
    },
    
    #[error("环境变量未找到: {0}")]
    EnvVarNotFound(String),
    
    #[error("没有可用的环境变量")]
    NoEnvVarFound,
    
    #[error("配置验证失败: {0}")]
    ValidationError(String),
    
    #[error("热重载失败，已回滚")]
    HotReloadFailed,
}
```

---

*关联文档：*
- [TECH.md](TECH.md) - 总体架构文档
- [TECH-MODEL.md](TECH-MODEL.md) - 模型服务模块
- [TECH-SESSION.md](TECH-SESSION.md) - Session管理模块
