# TECH-CONFIG: 配置管理模块

本文档描述Neco项目的配置管理模块设计，采用类型安全的配置结构。

## 1. 模块概述

配置管理模块负责加载、验证和提供所有配置数据。

**设计原则：**
- 类型安全的配置结构（不用HashMap）
- 编译期配置验证
- 统一的配置加载器

## 2. 配置来源

### 2.1 配置目录结构

Neco 支持多级配置目录，按优先级从高到低：

1. **当前项目配置**：`.neco/` 目录
2. **当前项目配置**：`.agents/` 目录
3. **主配置目录**：`~/.config/neco/`
4. **通用配置目录**：`~/.agents/`

```
# 优先级从高到低

.neco/                          # 当前项目 .neco（最高）
├── neco.toml
├── prompts/
├── agents/
├── skills/
└── workflows/

.agents/                        # 当前项目 .agents
├── prompts/
├── agents/
├── skills/
└── workflows/

~/.config/neco/                 # 主配置目录
├── neco.toml
├── neco.<tag>.toml
├── prompts/
├── agents/
├── skills/
└── workflows/

~/.agents/                      # 通用配置目录（最低）
├── prompts/
├── agents/
├── skills/
└── workflows/
```

## 3. 配置数据结构

### 3.1 完整配置

```rust
/// 完整配置（根结构）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model_groups: ModelGroups,
    pub model_providers: ModelProviders,
    pub mcp_servers: McpServers,
    pub system: SystemConfig,
}

/// 模型组配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelGroups(pub HashMap<String, ModelGroup>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelGroup {
    pub models: Vec<ModelRef>,
}

/// 模型引用（支持参数）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    pub provider: String,
    pub name: String,
    #[serde(default)]
    pub params: HashMap<String, Value>,
}

impl ModelRef {
    pub fn parse(s: &str) -> Result<Self, ConfigError> {
        // 格式：provider/model?param=value
        // TODO: 实现解析逻辑
        unimplemented!()
    }
}

/// 模型提供商配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProviders(pub HashMap<String, ModelProvider>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProvider {
    #[serde(rename = "type")]
    pub provider_type: ProviderType,
    pub name: String,
    pub base_url: Url,
    pub api_key: ApiKeyConfig,
    #[serde(default)]
    pub default_params: HashMap<String, Value>,
    #[serde(default)]
    pub retry: RetryConfig,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    #[serde(rename = "openai")]
    OpenAI,
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "openrouter")]
    OpenRouter,
}

/// API密钥配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "lowercase")]
pub enum ApiKeyConfig {
    Env { name: String },
    EnvList { names: Vec<String> },
    Direct { key: SecretString },
}

impl ApiKeyConfig {
    pub fn resolve(&self) -> Result<SecretString, ConfigError> {
        match self {
            Self::Env { name } => std::env::var(name)
                .map(SecretString::from)
                .map_err(|_| ConfigError::EnvVarNotFound(name.clone())),
            Self::EnvList { names } => {
                for name in names {
                    if let Ok(key) = std::env::var(name) {
                        return Ok(SecretString::from(key));
                    }
                }
                Err(ConfigError::NoEnvVarFound)
            }
            Self::Direct { key } => Ok(key.clone()),
        }
    }
}

/// 重试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default)]
    pub initial_backoff: Duration,
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
    #[serde(default)]
    pub max_backoff: Duration,
}

fn default_max_retries() -> u32 { 3 }
fn default_backoff_multiplier() -> f64 { 2.0 }

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

/// MCP服务器配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpServers(pub HashMap<String, McpServerConfig>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub transport: McpTransportConfig,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// MCP传输配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpTransportConfig {
    Stdio {
        command: String,
        args: Vec<String>,
    },
    Http {
        url: Url,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        bearer_token: Option<SecretString>,
    },
}
```

### 3.2 系统配置

```rust
/// 系统配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub storage: StorageConfig,
    pub context: ContextConfig,
    pub tools: ToolsConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub session_dir: PathBuf,
    #[serde(default = "default_compression")]
    pub compression: bool,
}

fn default_compression() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    #[serde(default = "default_compact_threshold")]
    pub auto_compact_threshold: f64,
    #[serde(default = "default_true")]
    pub auto_compact_enabled: bool,
}

fn default_compact_threshold() -> f64 { 0.9 }
fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default)]
    pub timeouts: HashMap<String, Duration>,
    #[serde(default = "default_tool_timeout")]
    pub default_timeout: Duration,
}

fn default_tool_timeout() -> Duration { Duration::from_secs(30) }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default)]
    pub default_mode: RunMode,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum RunMode {
    #[default]
    Direct,
    Repl,
    Daemon,
}
```

## 4. 配置加载器

### 4.1 配置加载流程

```mermaid
sequenceDiagram
    participant App as 应用程序
    participant Loader as ConfigLoader
    participant Parser as TOML解析器
    participant Merger as ConfigMerger
    participant Validator as ConfigValidator

    App->>Loader: load()
    
    loop 遍历配置目录
        Loader->>Parser: 解析文件
        Parser-->>Loader: 返回Value
        Loader->>Merger: 合并配置
    end
    
    Loader->>Validator: 验证配置
    alt 验证失败
        Validator-->>App: 返回错误
    else 验证成功
        Loader-->>App: 返回Config
    end
```

### 4.2 配置加载器实现

```rust
pub struct ConfigLoader {
    config_dirs: Vec<PathBuf>,
}

impl ConfigLoader {
    pub fn new() -> Self {
        let dirs = vec![
            PathBuf::from(".neco"),
            PathBuf::from(".agents"),
            dirs::config_dir().join("neco"),
            dirs::home_dir().unwrap_or_default().join(".agents"),
        ];
        Self { config_dirs: dirs }
    }
    
    pub fn load(&self) -> Result<Config, ConfigError> {
        // TODO: 实现配置加载逻辑
        // 1. 查找所有配置文件
        // 2. 按优先级解析和合并
        // 3. 验证配置
        // 4. 返回Config
        unimplemented!()
    }
    
    pub fn load_workflow_config(&self, workflow_dir: &Path) -> Result<Config, ConfigError> {
        // TODO: 加载工作流特定配置
        unimplemented!()
    }
}
```

### 4.3 配置验证

```rust
pub struct ConfigValidator;

impl ConfigValidator {
    pub fn validate(config: &Config) -> Result<(), ConfigError> {
        // 验证模型组
        Self::validate_model_groups(config)?;
        // 验证提供商
        Self::validate_providers(config)?;
        // 验证MCP服务器
        Self::validate_mcp_servers(config)?;
        
        Ok(())
    }
    
    fn validate_model_groups(config: &Config) -> Result<(), ConfigError> {
        for (name, group) in &config.model_groups.0 {
            if group.models.is_empty() {
                return Err(ConfigError::ValidationError(
                    format!("Model group '{}' has no models", name)
                ));
            }
            
            for model in &group.models {
                if !config.model_providers.0.contains_key(&model.provider) {
                    return Err(ConfigError::ValidationError(
                        format!("Provider '{}' not found for model in group '{}'", 
                            model.provider, name)
                    ));
                }
            }
        }
        Ok(())
    }
    
    fn validate_providers(config: &Config) -> Result<(), ConfigError> {
        for (name, provider) in &config.model_providers.0 {
            // 验证API密钥
            if let Err(e) = provider.api_key.resolve() {
                return Err(ConfigError::ValidationError(
                    format!("Provider '{}': {}", name, e)
                ));
            }
            
            // 验证base_url
            if provider.base_url.host_str().is_none() {
                return Err(ConfigError::ValidationError(
                    format!("Provider '{}' has invalid base_url", name)
                ));
            }
        }
        Ok(())
    }
    
    fn validate_mcp_servers(config: &Config) -> Result<(), ConfigError> {
        // TODO: 验证MCP服务器配置
        Ok(())
    }
}
```

## 5. 配置示例

```toml
# neco.toml

[model_groups.frontier]
models = [{ provider = "zhipuai", name = "glm-4.7" }]

[model_groups.smart]
models = [{ provider = "zhipuai", name = "glm-4.7", params = { reasoning_effort = "high" } }]

[model_groups.balanced]
models = [
    { provider = "zhipuai", name = "glm-4.7" },
    { provider = "minimax-cn", name = "MiniMax-M2.5" }
]

[model_groups.fast]
models = [{ provider = "zhipuai", name = "glm-4.7-flashx" }]

[model_providers.zhipuai]
type = "openai"
name = "ZhipuAI"
base_url = "https://open.bigmodel.cn/api/paas/v4"
api_key = { source = "env", name = "ZHIPU_API_KEY" }
timeout = { secs = 60, nanos = 0 }

[model_providers.zhipuai.retry]
max_retries = 3

[model_providers.minimax-cn]
type = "openai"
name = "MiniMax (CN)"
base_url = "https://api.minimaxi.com/v1"
api_key = { source = "env_list", names = ["MINIMAX_API_KEY", "MINIMAX_API_KEY_2"] }

[mcp_servers.context7]
type = "stdio"
command = "npx"
args = ["-y", "@upstash/context7-mcp"]
env = { MY_ENV_VAR = "MY_ENV_VALUE" }

[mcp_servers.figma]
type = "http"
url = "https://mcp.figma.com/mcp"
headers = { "X-Figma-Region" = "us-east-1" }
bearer_token = { source = "env", name = "FIGMA_OAUTH_TOKEN" }

[system]
[system.storage]
session_dir = "~/.local/neco"
compression = true

[system.context]
auto_compact_threshold = 0.9
auto_compact_enabled = true

[system.tools]
default_timeout = { secs = 30, nanos = 0 }
timeouts = { "fs" = { secs = 10, nanos = 0 }, "mcp" = { secs = 60, nanos = 0 } }

[system.ui]
default_mode = "repl"
```

## 6. 错误类型

```rust
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("配置文件未找到: {0}")]
    FileNotFound(PathBuf),
    
    #[error("解析错误: {0}")]
    ParseError(String),
    
    #[error("验证失败: {0}")]
    ValidationError(String),
    
    #[error("环境变量未找到: {0}")]
    EnvVarNotFound(String),
    
    #[error("没有可用的环境变量")]
    NoEnvVarFound,
    
    #[error("热重载失败")]
    HotReloadFailed,
}
```

---

*关联文档：*
- [TECH.md](TECH.md) - 总体架构文档
- [TECH-MODEL.md](TECH-MODEL.md) - 模型服务模块
- [TECH-SESSION.md](TECH-SESSION.md) - Session管理模块
