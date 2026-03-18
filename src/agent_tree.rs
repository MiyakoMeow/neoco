use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use rig::client::CompletionClient;
use rig::completion::Message;
use serde::Deserialize;
use tokio::sync::Mutex;
use ulid::Ulid;

use crate::agent::{AnyAgent, chat_with_agent};
use crate::config::{Config, Provider};

const DEFAULT_MAX_TURNS: usize = 1000;

/// Message insertion mode for inter-agent communication.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum InsertMode {
    #[default]
    Queue,
    // TODO: Implement interrupt mode - stop current execution and handle message immediately
    Interrupt,
    // TODO: Implement append mode - add to end of conversation history
    Append,
}

/// A message queued for delivery to an agent.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    /// The content of the message.
    pub content: String,
    /// How the message should be inserted.
    pub mode: InsertMode,
    /// The ID of the agent that sent this message.
    pub from_agent_id: Ulid,
}

/// Handle for an agent in the tree, containing its metadata and queues.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AgentHandle {
    /// Unique identifier for this agent.
    pub id: Ulid,
    /// ID of the parent agent, if any.
    pub parent_id: Option<Ulid>,
    /// Queue of pending messages for this agent.
    pub pending_messages: Arc<Mutex<Vec<QueuedMessage>>>,
    /// List of child agent IDs.
    pub children: Arc<Mutex<Vec<Ulid>>>,
}

/// Tree structure for managing multiple agents and their hierarchical relationships.
#[allow(dead_code)]
pub struct AgentTree {
    handles: HashMap<Ulid, AgentHandle>,
    agents: HashMap<Ulid, Arc<AnyAgent>>,
    root_id: Ulid,
    /// Configuration for the agent.
    pub config: Config,
}

impl AgentTree {
    /// Creates a new `AgentTree` with the given root agent.
    pub fn new(root_agent: AnyAgent, config: Config) -> Self {
        let root_id = Ulid::new();
        let mut handles = HashMap::new();
        let mut agents = HashMap::new();

        handles.insert(
            root_id,
            AgentHandle {
                id: root_id,
                parent_id: None,
                pending_messages: Arc::new(Mutex::new(Vec::new())),
                children: Arc::new(Mutex::new(Vec::new())),
            },
        );
        agents.insert(root_id, Arc::new(root_agent));

        Self {
            handles,
            agents,
            root_id,
            config,
        }
    }

    /// Returns the root agent's ID.
    #[allow(dead_code)]
    pub fn root_id(&self) -> Ulid {
        self.root_id
    }

    /// Updates an agent in the tree.
    #[allow(dead_code)]
    pub fn update_agent(&mut self, id: Ulid, agent: AnyAgent) {
        if let Some(existing) = self.agents.get_mut(&id) {
            *existing = Arc::new(agent);
        }
    }

    /// Adds a child agent to the specified parent.
    /// NOTE: This method must be called outside of an async lock context,
    /// or use the async version `add_child_async` instead.
    #[allow(dead_code)]
    pub fn add_child(&mut self, parent_id: Ulid, child_agent: AnyAgent) -> Ulid {
        let child_id = Ulid::new();

        let handle = AgentHandle {
            id: child_id,
            parent_id: Some(parent_id),
            pending_messages: Arc::new(Mutex::new(Vec::new())),
            children: Arc::new(Mutex::new(Vec::new())),
        };

        self.handles.insert(child_id, handle);
        self.agents.insert(child_id, Arc::new(child_agent));

        if let Some(parent) = self.handles.get_mut(&parent_id) {
            parent.children.blocking_lock().push(child_id);
        }

        child_id
    }

    /// Adds a child agent asynchronously (for use in async contexts).
    #[allow(dead_code)]
    pub async fn add_child_async(&mut self, parent_id: Ulid, child_agent: AnyAgent) -> Ulid {
        let child_id = Ulid::new();

        let handle = AgentHandle {
            id: child_id,
            parent_id: Some(parent_id),
            pending_messages: Arc::new(Mutex::new(Vec::new())),
            children: Arc::new(Mutex::new(Vec::new())),
        };

        self.handles.insert(child_id, handle);
        self.agents.insert(child_id, Arc::new(child_agent));

        if let Some(parent) = self.handles.get_mut(&parent_id) {
            let mut children = parent.children.lock().await;
            children.push(child_id);
        }

        child_id
    }

    /// Gets an agent by its ID.
    #[allow(dead_code)]
    pub fn get_agent(&self, id: Ulid) -> Option<Arc<AnyAgent>> {
        self.agents.get(&id).cloned()
    }

    /// Gets the parent ID of an agent.
    #[allow(dead_code)]
    pub fn get_parent_id(&self, id: Ulid) -> Option<Ulid> {
        self.handles.get(&id).and_then(|h| h.parent_id)
    }

    /// Adds a pending message to an agent's queue.
    /// Uses `blocking_lock` for synchronous context.
    #[allow(dead_code)]
    pub fn add_pending_message(&self, target_id: Ulid, message: QueuedMessage) {
        if let Some(handle) = self.handles.get(&target_id) {
            let pending = handle.pending_messages.clone();
            let mut guard = pending.blocking_lock();
            guard.push(message);
        }
    }

    /// Adds a pending message asynchronously (for use in async contexts).
    #[allow(dead_code)]
    pub async fn add_pending_message_async(&self, target_id: Ulid, message: QueuedMessage) {
        if let Some(handle) = self.handles.get(&target_id) {
            let pending = handle.pending_messages.clone();
            let mut guard = pending.lock().await;
            guard.push(message);
        }
    }

    /// Drains and returns all pending messages for an agent.
    #[allow(dead_code)]
    pub async fn drain_pending_messages(&self, id: Ulid) -> Vec<QueuedMessage> {
        if let Some(handle) = self.handles.get(&id) {
            let pending = handle.pending_messages.clone();
            let mut guard = pending.lock().await;
            std::mem::take(&mut *guard)
        } else {
            Vec::new()
        }
    }

    /// Gets all pending messages for an agent without removing them.
    #[allow(dead_code)]
    pub async fn get_pending_messages(&self, id: Ulid) -> Vec<QueuedMessage> {
        if let Some(handle) = self.handles.get(&id) {
            let pending = handle.pending_messages.clone();
            let guard = pending.lock().await;
            guard.clone()
        } else {
            Vec::new()
        }
    }

    /// Runs a child agent asynchronously with the given message.
    #[allow(dead_code)]
    pub fn run_child_agent(&self, child_id: Ulid, message: String, insert_mode: InsertMode) {
        let agent = match self.agents.get(&child_id) {
            Some(a) => a.clone(),
            None => return,
        };

        let parent_id = self.get_parent_id(child_id);
        // Clone only the parent's pending_messages Arc instead of the entire handles HashMap
        let parent_pending = parent_id
            .and_then(|pid| self.handles.get(&pid))
            .map(|h| h.pending_messages.clone());
        let child_id_clone = child_id;

        tokio::spawn(async move {
            let history: Vec<Message> = Vec::new();

            let response = match agent.as_ref() {
                AnyAgent::OpenAICompletions(a) => {
                    match chat_with_agent(a, &message, &history, None).await {
                        Ok((resp, _)) => resp,
                        Err(e) => format!("Error: {e}"),
                    }
                },
                AnyAgent::OpenAIResponses(a) => {
                    match chat_with_agent(a, &message, &history, None).await {
                        Ok((resp, _)) => resp,
                        Err(e) => format!("Error: {e}"),
                    }
                },
                AnyAgent::Anthropic(a) => {
                    match chat_with_agent(a, &message, &history, None).await {
                        Ok((resp, _)) => resp,
                        Err(e) => format!("Error: {e}"),
                    }
                },
            };

            let pending_msg = QueuedMessage {
                content: format!("[from child_{child_id_clone}] {response}"),
                mode: insert_mode,
                from_agent_id: child_id_clone,
            };

            if let Some(pending) = parent_pending {
                let mut guard = pending.lock().await;
                guard.push(pending_msg);
            }
        });
    }
}

/// Type alias for a thread-safe, shared reference to an `AgentTree`.
#[allow(dead_code)]
pub type SharedAgentTree = Arc<Mutex<AgentTree>>;

/// Creates a new shared `AgentTree` from an existing tree.
#[allow(dead_code)]
pub fn new_shared(tree: AgentTree) -> SharedAgentTree {
    Arc::new(Mutex::new(tree))
}

/// Creates an agent with spawn and send tools integrated.
pub async fn create_agent_with_tools(
    config: &Config,
    provider: &Provider,
    api_key: &str,
    model_name: &str,
) -> Result<(AnyAgent, SharedAgentTree, Ulid)> {
    match provider.r#type {
        crate::config::ProviderType::OpenAICompletions => {
            use rig::providers::openai::CompletionsClient;
            let client = CompletionsClient::builder()
                .api_key(api_key)
                .base_url(&provider.base_url)
                .build()
                .context("Failed to create OpenAI Completions client")?;

            // Create placeholder agent to build tree
            let placeholder = AnyAgent::OpenAICompletions(
                client
                    .agent(model_name)
                    .tool(crate::tools::ShellTool::new())
                    .default_max_turns(DEFAULT_MAX_TURNS)
                    .build(),
            );

            let tree = AgentTree::new(placeholder, config.clone());
            let shared_tree = new_shared(tree);
            let root_id = shared_tree.lock().await.root_id();

            // Create agent with all tools using the same client instance
            let agent = AnyAgent::OpenAICompletions(
                client
                    .agent(model_name)
                    .tool(crate::tools::ShellTool::new())
                    .tool(crate::tools::SpawnTool::new(
                        config.clone(),
                        shared_tree.clone(),
                        root_id,
                    ))
                    .tool(crate::tools::SendTool::new(shared_tree.clone(), root_id))
                    .default_max_turns(DEFAULT_MAX_TURNS)
                    .build(),
            );

            Ok((agent, shared_tree, root_id))
        },
        crate::config::ProviderType::OpenAIResponses => {
            use rig::providers::openai::Client as OpenAIClient;
            let client = OpenAIClient::builder()
                .api_key(api_key)
                .base_url(&provider.base_url)
                .build()
                .context("Failed to create OpenAI Responses client")?;

            // Create placeholder agent to build tree
            let placeholder = AnyAgent::OpenAIResponses(
                client
                    .agent(model_name)
                    .tool(crate::tools::ShellTool::new())
                    .default_max_turns(DEFAULT_MAX_TURNS)
                    .build(),
            );

            let tree = AgentTree::new(placeholder, config.clone());
            let shared_tree = new_shared(tree);
            let root_id = shared_tree.lock().await.root_id();

            // Create agent with all tools using the same client instance
            let agent = AnyAgent::OpenAIResponses(
                client
                    .agent(model_name)
                    .tool(crate::tools::ShellTool::new())
                    .tool(crate::tools::SpawnTool::new(
                        config.clone(),
                        shared_tree.clone(),
                        root_id,
                    ))
                    .tool(crate::tools::SendTool::new(shared_tree.clone(), root_id))
                    .default_max_turns(DEFAULT_MAX_TURNS)
                    .build(),
            );

            Ok((agent, shared_tree, root_id))
        },
        crate::config::ProviderType::Anthropic => {
            use rig::providers::anthropic::Client;
            let client = Client::builder()
                .api_key(api_key)
                .base_url(&provider.base_url)
                .anthropic_version("2023-06-01")
                .build()
                .context("Failed to create Anthropic client")?;

            // Create placeholder agent to build tree
            let placeholder = AnyAgent::Anthropic(
                client
                    .agent(model_name)
                    .tool(crate::tools::ShellTool::new())
                    .default_max_turns(DEFAULT_MAX_TURNS)
                    .build(),
            );

            let tree = AgentTree::new(placeholder, config.clone());
            let shared_tree = new_shared(tree);
            let root_id = shared_tree.lock().await.root_id();

            // Create agent with all tools using the same client instance
            let agent = AnyAgent::Anthropic(
                client
                    .agent(model_name)
                    .tool(crate::tools::ShellTool::new())
                    .tool(crate::tools::SpawnTool::new(
                        config.clone(),
                        shared_tree.clone(),
                        root_id,
                    ))
                    .tool(crate::tools::SendTool::new(shared_tree.clone(), root_id))
                    .default_max_turns(DEFAULT_MAX_TURNS)
                    .build(),
            );

            Ok((agent, shared_tree, root_id))
        },
    }
}
