# TECH-MODEL: 模型服务模块

本文档描述Neco项目的模型服务模块设计。

## 1. 模块概述

模型服务模块负责与各种LLM提供商交互，提供统一的调用接口，支持故障转移、重试和流式输出。

## 2. 架构设计

### 2.1 模块结构

```mermaid
graph TB
    subgraph "Model Service"
        MC[ModelClient trait]
        
        subgraph "Providers"
            OP[OpenAI Provider]
            AP[Anthropic Provider（预留）]
            ORP[OpenRouter Provider（预留）]
        end
        
        subgraph "Features"
            RT[Retry Handler]
            FB[Fallback Handler]
            ST[Streaming Handler]
        end
    end
    
    MC --> OP
    MC --> AP
    MC --> ORP
    
    MC --> RT
    MC --> FB
    MC --> ST
```

## 3. 模型客户端接口

### 3.1 ModelClient Trait

```rust
/// 模型能力
#[derive(Debug, Clone)]
pub struct ModelCapabilities {
    pub streaming: bool,
    pub tools: bool,
    pub functions: bool,
    pub json_mode: bool,
    pub vision: bool,
    pub context_window: usize,
}

/// 聊天完成请求
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ModelMessage<'static>>,
    pub stream: bool,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub tool_choice: Option<String>,
    pub response_format: Option<String>,
    pub stop: Option<Vec<String>>,
    pub extra_params: HashMap<String, Value>,
}

/// 聊天完成响应
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

#[derive(Debug, Clone)]
pub struct Choice {
    pub index: usize,
    pub message: Message,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// 模型客户端接口
#[async_trait]
pub trait ModelClient: Send + Sync {
    async fn chat_completion(
        &self,
        request: ChatRequest,
    ) -> Result<ChatResponse, ModelError>;
    
    async fn chat_completion_stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<Result<ChatChunk, ModelError>>, ModelError>;
    
    fn capabilities(&self) -> ModelCapabilities;
}
```

## 4. 类型定义

### 4.1 消息类型

```rust
/// 聊天消息
#[derive(Debug, Clone)]
#[serde(tag = "role")]
pub enum ModelMessage<'a> {
    System { content: &'a str },
    User { content: &'a str },
    Assistant { content: &'a str, tool_calls: Option<Vec<ToolCall>> },
    Tool { tool_call_id: &'a str, content: &'a str },
}
```

### 4.2 工具调用

```rust
/// 工具调用
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}
```

### 4.3 工具定义

```rust
/// 工具定义
#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}
```

### 4.4 流式响应块

```rust
/// 流式响应块
#[derive(Debug, Clone)]
pub struct ChatChunk {
    pub id: String,
    pub choices: Vec<ChunkChoice>,
}
```

### 4.5 消息（响应中的消息）

```rust
#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
}
```

### 4.6 模型引用

```rust
/// 模型引用
#[derive(Debug, Clone)]
pub struct ModelRef {
    pub name: String,
    pub provider: String,
}
```

### 4.7 重试配置

```rust
/// 重试配置
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        // [TODO] 实现要点说明
        // 1. 设置默认重试次数为3次
        // 2. 初始延迟1000ms
        // 3. 最大延迟30000ms
        // 4. 退避倍数为2.0
        unimplemented!()
    }
}
```

## 5. 业务流程图

### 5.1 故障转移流程

```mermaid
flowchart TD
    A[开始请求] --> B{选择模型}
    B --> C[调用模型]
    C --> D{成功?}
    D -->|是| E[返回响应]
    D -->|否| F{可重试错误?}
    F -->|是| G[重试]
    F -->|否| H{有下一个模型?}
    G --> C
    H -->|是| I[切换模型]
    H -->|否| J[返回错误]
    I --> C
```

### 5.2 重试流程

```mermaid
flowchart TD
    A[首次调用] --> B{成功?}
    B -->|是| C[返回结果]
    B -->|否| D{次数 < max_retries?}
    D -->|是| E[计算延迟]
    E --> F[等待]
    F --> G[重试调用]
    G --> B
    D -->|否| H[返回错误]
```

### 5.3 流式处理流程

```mermaid
flowchart TD
    A[开始流式请求] --> B[创建流]
    B --> C{有数据?}
    C -->|是| D[解析Chunk]
    D --> E[回调处理]
    E --> C
    C -->|否| F{完成?}
    F -->|是| G[返回完整响应]
    F -->|否| H[等待更多数据]
    H --> C
```

## 6. 模型组客户端

```rust
pub struct ModelGroupClient {
    name: String,
    models: Vec<ModelRef>,
    clients: HashMap<String, Arc<dyn ModelClient>>,
    retry_config: RetryConfig,
}

impl ModelGroupClient {
    pub async fn chat_completion(
        &self,
        mut request: ChatRequest,
    ) -> Result<ChatResponse, ModelError> {
        // TODO: 实现故障转移逻辑
        // 1. 遍历模型列表，按优先级顺序尝试
        // 2. 对每个模型：设置模型参数并调用
        // 3. 调用失败时检查错误类型
        // 4. 可重试错误进行指数退避重试
        // 5. 不可重试错误或重试耗尽时切换下一个模型
        // 6. 所有模型都失败时返回聚合错误
        unimplemented!()
    }
}
```

## 7. OpenAI客户端实现

### 7.1 客户端结构

```rust
pub struct OpenAiClient {
    config: OpenAiClientConfig,
    inner: Client<OpenAIConfig>,
}

pub struct OpenAiClientConfig {
    pub api_key: Secret<String>,
    pub base_url: Url,
}

impl OpenAiClient {
    pub fn new(config: OpenAiClientConfig) -> Result<Self, ConfigError> {
        // TODO: 实现构造逻辑
        // 1. 使用config创建OpenAIConfig
        // 2. 从环境变量或config获取API Key
        // 3. 配置Base URL和超时
        // 4. 创建并返回OpenAiClient实例
        unimplemented!()
    }
}

#[async_trait]
impl ModelClient for OpenAiClient {
    async fn chat_completion(
        &self,
        request: ChatRequest,
    ) -> Result<ChatResponse, ModelError> {
        // TODO: 实现聊天完成请求
        // 1. 将ChatRequest转换为OpenAI API格式
        // 2. 调用OpenAI聊天完成API
        // 3. 处理API错误并转换为我们自己的ModelError
        // 4. 将响应转换回ChatResponse格式
        unimplemented!()
    }
    
    async fn chat_completion_stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<Result<ChatChunk, ModelError>>, ModelError> {
        // TODO: 实现流式API
        // 1. 将ChatRequest转换为OpenAI API格式
        // 2. 调用OpenAI流式API (sse::Event)
        // 3. 将SSE事件转换为ChatChunk
        // 4. 处理连接错误和重连逻辑
        unimplemented!()
    }
    
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            streaming: true,
            tools: true,
            functions: true,
            json_mode: true,
            vision: false,
            context_window: 128_000,
        }
    }
}
```

## 8. 流式输出处理

```rust
use futures::StreamExt;

pub struct StreamHandler;

impl StreamHandler {
    pub async fn collect_full_response(
        stream: BoxStream<Result<ChatChunk, ModelError>>,
    ) -> Result<String, ModelError> {
        // TODO: 收集完整响应
        // 1. 遍历流中的所有chunk
        // 2. 累加每个chunk的content字段
        // 3. 处理stream结束信号
        // 4. 检查finish_reason确定是否正常结束
        // 5. 返回拼接后的完整文本
        unimplemented!()
    }
    
    pub async fn process_with_callback<F>(
        stream: BoxStream<Result<ChatChunk, ModelError>>,
        mut callback: F,
    ) -> Result<ChatResponse, ModelError>
    where
        F: FnMut(&str),
    {
        // TODO: 实时处理流
        // 1. 对每个chunk调用callback进行实时处理
        // 2. 同时收集增量内容用于构建最终响应
        // 3. 检测并合并工具调用增量(delta)
        // 4. 处理流结束，组装完整ChatResponse
        unimplemented!()
    }
}
```

## 9. 工具调用处理

```rust
pub struct ToolCallHandler;

impl ToolCallHandler {
    pub fn parse_tool_calls(response: &ChatResponse) -> Vec<ToolCall> {
        // TODO: 从响应中解析工具调用
        // 1. 检查choices中第一个choice的消息
        // 2. 从message.tool_calls字段提取工具调用列表
        // 3. 解析每个工具调用的id、type、function
        // 4. 处理流式响应中的增量工具调用
        unimplemented!()
    }
    
    pub fn build_tool_message(
        tool_call_id: &str,
        result: &str,
    ) -> Message {
        // TODO: 构建工具响应消息
        // 1. 创建role为tool的消息
        // 2. 设置tool_call_id关联到原始调用
        // 3. 设置content为工具执行结果
        // 4. 返回构建好的Message
        unimplemented!()
    }
}
```

## 10. 错误处理

```rust
#[derive(Debug, Error)]
pub enum ModelError {
    #[error("API错误: {0}")]
    Api(String),
    
    #[error("网络错误: {0}")]
    Network(#[from] reqwest::Error),
    
    #[error("速率限制: {0}")]
    RateLimit(String),
    
    #[error("服务器错误: {status} - {message}")]
    ServerError { status: u16, message: String },
    
    #[error("客户端未找到: {0}")]
    ClientNotFound(String),
    
    #[error("模型组 {group} 中所有模型都失败")]
    AllModelsFailed { group: String },
    
    #[error("配置错误: {0}")]
    Config(#[from] ConfigError),
    
    #[error("超时")]
    Timeout,
}
```

---

*关联文档：*
- [TECH.md](TECH.md) - 总体架构文档
- [TECH-CONFIG.md](TECH-CONFIG.md) - 配置管理模块
- [TECH-SESSION.md](TECH-SESSION.md) - Session管理模块
