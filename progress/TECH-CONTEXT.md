# TECH-CONTEXT: 上下文管理模块

本文档描述NeoCo项目的上下文管理模块设计，采用领域驱动设计，分离领域模型与基础设施。

> **核心理念**：Context Window 就是一块 Arena Allocator。管理上下文不是"写prompt"，而是内存管理。

## 0. Arena Allocator 心智模型

### 0.1 核心隐喻

将 LLM 的上下文窗口想象成一块预先分配好的连续内存：

| Arena Allocator | Context Window |
|-----------------|----------------|
| 固定大小的内存块 | 固定上限的 token 窗口 |
| 指针只往前推 (bump) | 只能在末尾追加消息 |
| 连续内存块，无碎片 | 前缀稳定，KV Cache 命中 |
| 批量释放而非逐个回收 | 按区间压缩，而非单条消息 |

### 0.2 三条铁律

```mermaid
graph TB
    subgraph "Context Window 铁律"
        T1[铁律一: 固定上限<br/>128K token 不可扩展] --> T2[铁律二: 前缀改变代价灾难<br/>KV Cache 完全失效]
        T2 --> T3[铁律三: 注意力不均匀<br/>两端热，中间冷]
    end
```

**铁律一**：固定上限。128K token 就是 128K，是最稀缺的资源。

**铁律二**：前缀改变是灾难性的。KV Cache 匹配从第一个 token 逐一比对，第一个不同之后全部 cache miss。

**铁律三**：注意力不均匀。LLM 对开头和末尾 token 注意力最强，中间会衰减（Lost in the Middle）。

### 0.3 五条设计原则

| 原则 | Arena 对应 | 上下文工程实践 |
|------|------------|---------------|
| Append-Only AMAP | 指针只往前推 | 在末尾追加 user/assistant/tool_result，不在中间插入 |
| Demand Paging | 按需分配对象 | 技能按需加载，不预装到 system prompt |
| Spatial Locality | 相邻分配 | 相关信息物理相邻，指南附着在 tool_result 上 |
| Goldilocks Zone | 最佳 arena 大小 | 维持 40-70% 使用率 |
| 批量释放 | reset 而非 free | 按区间压缩，不逐条删除 |

### 0.4 Pruning 和 RAG 是补救手段

```text
原则（布局）                    补救手段（trick）
─────────────────────────────────────────────────
Append-only → 前缀稳定         Pruning → 布局失效后的止损
Demand Paging → 不浪费空间     RAG → 信息被 prune 后的恢复
Spatial Locality → 注意力集中  
Goldilocks Zone → 信噪比最优   
```

**核心原则**：先把布局做对，补救手段自然用得少。

---

## 1. 模块概述

上下文管理模块负责：
1. 监控上下文大小，触发自动或手动压缩
2. 提供上下文观测功能

### 1.1 模块边界

```mermaid
graph LR
    subgraph "neoco-context"
        CM[ContextManager]
        CS[CompressionService]
        CO[ContextObserver]
        TC[TokenCounter]
    end
    
    subgraph "依赖模块"
        Session[neoco-session]
        Model[neoco-model]
        Agent[neoco-agent]
    end
    
    Agent --> CM
    CM --> Session
    CM --> Model
    CO --> Session
```

### 1.2 核心职责

| 组件 | 职责 |
|------|------|
| `ContextManager` | 上下文生命周期管理、触发压缩 |
| `CompressionService` | 执行压缩逻辑、调用模型生成摘要 |
| `ContextObserver` | 提供上下文观测能力 |
| `TokenCounter` | Token数量估算 |

## 2. 核心概念

### 2.1 压缩触发条件

> **设计原则**：Pruning 是布局失效后的止损操作，不是核心策略。

```mermaid
graph TD
    A[检查上下文大小] --> B{超过阈值?}
    B -->|否| D[继续正常流程]
    B -->|是| E{Agent自觉?}
    E -->|是| F[Layer A: Agent主动压缩<br/>tag起点 → squash为summary]
    E -->|否| G[Layer B: 系统Pruning<br/>安全网]
    F --> H[高质量压缩<br/>Agent知道signal vs noise]
    G --> I[三阶段Pruning]
    
    I --> I1[Stage 1: Soft Trim<br/>缩减大tool_result]
    I1 --> I2[Stage 2: Hard Clear<br/>替换为占位符]
    I2 --> I3[Stage 3: 分级压缩<br/>按事件类型差异化]
```

**两层压缩模型：**

| Layer | 触发条件 | 压缩质量 | 说明 |
|-------|---------|---------|------|
| Layer A (Agent主动) | Agent自觉判断 | 高 | Agent判断"这段研究/调试已完成" → tag起点 → squash为summary |
| Layer B (系统Pruning) | 布局失效 | 低 | 安全网，理想情况下永不触发 |

**触发方式：**

| 方式 | 触发条件 | 说明 |
|-----|---------|------|
| Agent主动 | Agent调用 context::compact | Agent自觉管理内存 |
| 自动触发 | 上下文大小 > 窗口×阈值(默认90%) | 触发 Layer B |
| 手动触发 | /compact命令 | 用户主动 |

## 3. 核心Trait定义

### 3.1 ContextManager

```rust
#[async_trait]
pub trait ContextManager: Send + Sync {
    /// 构建上下文消息列表
    async fn build_context(
        &self,
        agent_ulid: &AgentUlid,
        max_tokens: usize,
    ) -> Result<Vec<ModelMessage>, ContextError>;
    
    /// 检查是否需要压缩
    async fn should_compact(&self, agent_ulid: &AgentUlid) -> bool;
    
    /// 执行压缩
    async fn compact(
        &self,
        agent_ulid: &AgentUlid,
    ) -> Result<CompactResult, ContextError>;
    
    /// 获取上下文统计信息
    async fn get_stats(
        &self,
        agent_ulid: &AgentUlid,
    ) -> Result<ContextStats, ContextError>;
}
```

### 3.2 ContextManager实现

```rust
pub struct ContextManagerImpl {
    session_repo: Arc<dyn SessionRepository>,
    message_repo: Arc<dyn MessageRepository>,
    compression_service: Arc<CompressionService>,
    config: ContextConfig,
}

#[async_trait]
impl ContextManager for ContextManagerImpl {
    async fn build_context(
        &self,
        agent_ulid: &AgentUlid,
        max_tokens: usize,
    ) -> Result<Vec<ModelMessage>, ContextError> {
        // TODO: 实现上下文构建
        // 1. 从MessageRepository获取消息
        // 2. 按token限制截断
        // 3. 转换为ModelMessage返回
        unimplemented!()
    }
    
    async fn should_compact(&self, agent_ulid: &AgentUlid) -> bool {
        // TODO: 实现压缩检查
        // 1. 从MessageRepository获取消息
        // 2. 计算token数量
        // 3. 判断是否超过阈值
        unimplemented!()
    }
    
    async fn compact(
        &self,
        agent_ulid: &AgentUlid,
    ) -> Result<CompactResult, ContextError> {
        // TODO: 实现压缩
        // 1. 获取消息列表
        // 2. 调用CompressionService
        // 3. 截断旧消息
        // 4. 添加摘要消息
        unimplemented!()
    }
    
    async fn get_stats(
        &self,
        agent_ulid: &AgentUlid,
    ) -> Result<ContextStats, ContextError> {
        // TODO: 实现统计获取
        unimplemented!()
    }
}

## 4. 数据流

### 4.1 消息获取流程

```mermaid
sequenceDiagram
    participant Agent as neoco-agent
    participant CM as ContextManager
    participant MR as MessageRepository
    participant TC as TokenCounter

    Agent->>CM: build_context(agent_id, max_tokens)
    CM->>MR: list_messages(agent_id)
    MR-->>CM: Vec<Message>
    CM->>TC: estimate_tokens(messages)
    CM->>CM: 按token限制截断
    CM->>CM: 转换为ModelMessage
    CM-->>Agent: Vec<ModelMessage>
```

### 4.2 压缩执行流程

```mermaid
sequenceDiagram
    participant Agent as neoco-agent
    participant CM as ContextManager
    participant CS as CompressionService
    participant Model as neoco-model
    participant MR as MessageRepository

    Agent->>CM: compact(agent_id)
    CM->>MR: list_messages(agent_id)
    MR-->>CM: Vec<Message>
    CM->>CS: compact(messages)
    CS->>Model: chat_completion(压缩提示)
    Model-->>CS: 摘要文本
    CS-->>CM: CompactResult
    CM->>MR: truncate(agent_id, keep_ids)
    CM->>MR: append(摘要消息)
    CM-->>Agent: CompactResult
```

### 4.3 观测流程

```mermaid
sequenceDiagram
    participant User as 用户
    participant Tool as context::observe
    participant CO as ContextObserver
    participant MR as MessageRepository

    User->>Tool: observe(filter?)
    Tool->>CO: observe(agent_id, filter)
    CO->>MR: list_messages(agent_id)
    MR-->>CO: Vec<Message>
    CO->>CO: 生成统计信息
    CO-->>Tool: ContextObservation
    Tool-->>User: 格式化输出
```

## 5. 上下文观测

### 5.1 Goldilocks Zone

> 维持 40-70% 使用率是最佳状态。

```mermaid
graph LR
    subgraph "Usage Zone"
        L1[0-20%<br/>太空<br/>预装太多无关信息]
        L2[20-40%<br/>偏瘦]
        L3[40-70%<br/>Goldilocks Zone<br/>最佳状态]
        L4[70-90%<br/>偏满]
        L5[90-100%<br/>太满<br/>Pruning频繁触发]
    end
```

| 区域 | 使用率 | 问题 |
|------|-------|------|
| 太空 | 0-20% | 预装了太多无关信息，Agent在噪声中迷失 |
| Goldilocks | 40-70% | 信噪比最优，有足够空间给工具调用 |
| 太满 | 90%+ | Pruning频繁触发，Agent变成金鱼 |

### 5.2 Context Dashboard

Agent 通过 context::observe 工具可以看到上下文仪表盘：

```text
[Context Dashboard]
• Usage:           78.2% (100k/128k)
• Steps since tag: 35 (last: 'auth-refactor')
• Pruning status:  Stage 1 approaching
• Est. turns left: ~12
```

Agent 可根据此信息主动决定：
- 78% 使用率 → 决定主动压缩
- 12% 使用率 → 继续工作

### 5.3 观测接口定义

```rust
#[async_trait]
pub trait ContextObserver: Send + Sync {
    async fn observe(
        &self,
        agent: &Agent,
        filter: Option<ContextFilter>,
    ) -> Result<ContextObservation, ContextError>;
}

pub struct ContextFilter {
    pub roles: Option<Vec<Role>>,
    pub min_id: Option<MessageId>,
    pub max_id: Option<MessageId>,
    pub with_tool_calls: Option<bool>,
}

pub struct ContextObservation {
    pub messages: Vec<MessageSummary>,
    pub stats: ContextStats,
}

pub struct MessageSummary {
    pub id: MessageId,
    pub role: Role,
    pub content_preview: String,
    pub size_chars: usize,
    pub size_tokens: usize,
    pub timestamp: DateTime<Utc>,
}

pub struct ContextStats {
    pub total_messages: usize,
    pub total_chars: usize,
    pub total_tokens: usize,
    pub usage_percent: f64,
    pub role_counts: HashMap<Role, usize>,
    pub steps_since_tag: usize,
    pub last_tag: Option<String>,
    pub pruning_stage: Option<PruningStage>,
    pub estimated_turns_left: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruningStage {
    Stage1SoftTrim,
    Stage2HardClear,
    Stage3分级压缩,
}
```

### 5.4 context::observe 工具

```rust
pub struct ContextObserveTool {
    observer: Arc<dyn ContextObserver>,
}

#[async_trait]
impl ToolExecutor for ContextObserveTool {
    fn definition(&self) -> &ToolDefinition {
        // [TODO] 实现工具定义
        // 1. 定义工具ID和描述
        // 2. 定义参数schema
        // 3. 设置超时时间
        unimplemented!()
    }
    
    async fn execute(
        &self,
        context: &ToolContext,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        // TODO: 实现观测功能
        unimplemented!()
    }
}
```

## 6. 上下文压缩

### 6.1 压缩配置

```rust
pub struct ContextConfig {
    pub auto_compact_enabled: bool,
    pub auto_compact_threshold: f64,
    pub compact_model_group: ModelGroupRef,
    pub keep_recent_messages: usize,
}

pub struct ModelGroupRef(String);

impl ModelGroupRef {
    // TODO: 实现构造方法
    pub fn new(s: impl Into<String>) -> Self {
        todo!()
    }
    
    // TODO: 实现转换为字符串
    pub fn as_str(&self) -> &str {
        todo!()
    }
}

impl Default for ContextConfig {
    // TODO: 实现默认值
    fn default() -> Self {
        todo!()
    }
}
```

### 6.2 压缩结果

```rust
pub struct CompactResult {
    pub original_count: usize,
    pub compacted_count: usize,
    pub summary: String,
    pub preserved_ids: Vec<MessageId>,
    pub token_savings: TokenSavings,
    pub duration: Duration,
}

#[derive(Debug, Clone)]
pub struct TokenSavings {
    pub before: u32,
    pub after: u32,
    pub saved: u32,
    pub saved_percent: f64,
}
```

### 6.3 压缩服务

```rust
pub struct CompressionService {
    model_client: Arc<dyn ModelClient>,
    config: ContextConfig,
    token_counter: Arc<dyn TokenCounter>,
}

impl CompressionService {
    pub fn should_compact(&self, messages: &[Message], context_window: usize) -> bool {
        // [TODO] 实现压缩条件检查
        // 1. 计算当前token数量
        // 2. 计算阈值
        // 3. 比较判断是否需要压缩
        unimplemented!()
    }
    
    pub async fn compact(
        &self,
        messages: &[Message],
    ) -> Result<CompactResult, ContextError> {
        // TODO: 实现压缩逻辑
        // 1. 分离保留/压缩消息
        // 2. 调用模型生成摘要
        // 3. 构建新消息列表
        // 4. 返回结果
        unimplemented!()
    }
}
```

## 7. Token计数

```rust
pub trait TokenCounter: Send + Sync {
    fn estimate_string_tokens(&self, text: &str) -> usize;
    fn estimate_tokens(&self, messages: &[Message]) -> usize;
    fn estimate_message_tokens(&self, message: &Message) -> usize;
}

pub struct SimpleCounter;

impl TokenCounter for SimpleCounter {
    fn estimate_string_tokens(&self, text: &str) -> usize {
        // [TODO] 实现字符串token估算
        // 1. 考虑字符编码和token化方式
        // 2. 返回估算的token数量
        unimplemented!()
    }
    
    fn estimate_tokens(&self, messages: &[Message]) -> usize {
        // [TODO] 实现消息列表token估算
        // 1. 遍历每条消息
        // 2. 累加每条消息的token数
        unimplemented!()
    }
    
    fn estimate_message_tokens(&self, message: &Message) -> usize {
        // [TODO] 实现单条消息token估算
        // 1. 计算内容部分的token
        // 2. 计算tool_calls部分的token
        // 3. 考虑role等额外开销
        unimplemented!()
    }
}
```

## 8. 错误处理

```rust
#[derive(Debug, Error)]
pub enum ContextError {
    #[error("Agent不存在: {0}")]
    AgentNotFound(AgentUlid),
    
    #[error("模型调用错误: {0}")]
    Model(#[from] ModelError),
    
    #[error("没有可压缩的消息")]
    NothingToCompact,
    
    #[error("Token计算错误: {0}")]
    TokenCalculation(String),
    
    #[error("配置错误: {0}")]
    Config(String),
}
```

---

*关联文档：*
- [TECH.md](TECH.md) - 总体架构文档
- [TECH-SESSION.md](TECH-SESSION.md) - Session管理模块
- [TECH-MODEL.md](TECH-MODEL.md) - 模型服务模块
