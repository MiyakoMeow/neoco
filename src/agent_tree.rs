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

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum InsertMode {
    #[default]
    Queue,
    Interrupt,
    Append,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    pub content: String,
    pub mode: InsertMode,
    pub from_agent_id: Ulid,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AgentHandle {
    pub id: Ulid,
    pub parent_id: Option<Ulid>,
    pub pending_messages: Arc<Mutex<Vec<QueuedMessage>>>,
    pub children: Arc<Mutex<Vec<Ulid>>>,
}

#[allow(dead_code)]
pub struct AgentTree {
    handles: HashMap<Ulid, AgentHandle>,
    agents: HashMap<Ulid, Arc<AnyAgent>>,
    root_id: Ulid,
    pub config: Config,
}

impl AgentTree {
    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn root_id(&self) -> Ulid {
        self.root_id
    }

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

    #[allow(dead_code)]
    pub fn get_agent(&self, id: Ulid) -> Option<Arc<AnyAgent>> {
        self.agents.get(&id).cloned()
    }

    #[allow(dead_code)]
    pub fn get_parent_id(&self, id: Ulid) -> Option<Ulid> {
        self.handles.get(&id).and_then(|h| h.parent_id)
    }

    #[allow(dead_code)]
    pub fn add_pending_message(&self, target_id: Ulid, message: QueuedMessage) {
        if let Some(handle) = self.handles.get(&target_id) {
            let pending = handle.pending_messages.clone();
            let mut guard = pending.blocking_lock();
            guard.push(message);
        }
    }

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

    #[allow(dead_code)]
    pub fn run_child_agent(&self, child_id: Ulid, message: String, insert_mode: InsertMode) {
        let agent = match self.agents.get(&child_id) {
            Some(a) => a.clone(),
            None => return,
        };

        let parent_id = self.get_parent_id(child_id);
        let handles = self.handles.clone();
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

            if let Some(pid) = parent_id
                && let Some(parent) = handles.get(&pid)
            {
                let pending = parent.pending_messages.clone();
                let mut guard = pending.lock().await;
                guard.push(pending_msg);
            }
        });
    }
}

#[allow(dead_code)]
pub type SharedAgentTree = Arc<Mutex<AgentTree>>;

#[allow(dead_code)]
pub fn new_shared(tree: AgentTree) -> SharedAgentTree {
    Arc::new(Mutex::new(tree))
}

pub async fn create_agent_with_tools(
    config: &Config,
    provider: &Provider,
    api_key: &str,
    model_name: &str,
) -> Result<(AnyAgent, SharedAgentTree, Ulid)> {
    let tree: AgentTree;
    let shared_tree: SharedAgentTree;
    let root_id: Ulid;

    match provider.r#type {
        crate::config::ProviderType::OpenAICompletions => {
            use rig::providers::openai::CompletionsClient;
            let client = CompletionsClient::builder()
                .api_key(api_key)
                .base_url(&provider.base_url)
                .build()
                .context("Failed to create OpenAI Completions client")?;
            tree = AgentTree::new(
                AnyAgent::OpenAICompletions(
                    client
                        .agent(model_name)
                        .tool(crate::tools::ShellTool::new())
                        .default_max_turns(usize::MAX / 2)
                        .build(),
                ),
                config.clone(),
            );
            shared_tree = new_shared(tree);
            root_id = shared_tree.lock().await.root_id();
            let ag = client
                .agent(model_name)
                .tool(crate::tools::ShellTool::new())
                .tool(crate::tools::SpawnTool::new(
                    config.clone(),
                    shared_tree.clone(),
                    root_id,
                ))
                .tool(crate::tools::SendTool::new(shared_tree.clone(), root_id))
                .default_max_turns(usize::MAX / 2)
                .build();
            Ok((AnyAgent::OpenAICompletions(ag), shared_tree, root_id))
        },
        crate::config::ProviderType::OpenAIResponses => {
            use rig::providers::openai::Client as OpenAIClient;
            let client = OpenAIClient::builder()
                .api_key(api_key)
                .base_url(&provider.base_url)
                .build()
                .context("Failed to create OpenAI Responses client")?;
            tree = AgentTree::new(
                AnyAgent::OpenAIResponses(
                    client
                        .agent(model_name)
                        .tool(crate::tools::ShellTool::new())
                        .default_max_turns(usize::MAX / 2)
                        .build(),
                ),
                config.clone(),
            );
            shared_tree = new_shared(tree);
            root_id = shared_tree.lock().await.root_id();
            let ag = client
                .agent(model_name)
                .tool(crate::tools::ShellTool::new())
                .tool(crate::tools::SpawnTool::new(
                    config.clone(),
                    shared_tree.clone(),
                    root_id,
                ))
                .tool(crate::tools::SendTool::new(shared_tree.clone(), root_id))
                .default_max_turns(usize::MAX / 2)
                .build();
            Ok((AnyAgent::OpenAIResponses(ag), shared_tree, root_id))
        },
        crate::config::ProviderType::Anthropic => {
            use rig::providers::anthropic::Client;
            let client = Client::builder()
                .api_key(api_key)
                .base_url(&provider.base_url)
                .anthropic_version("2023-06-01")
                .build()
                .context("Failed to create Anthropic client")?;
            tree = AgentTree::new(
                AnyAgent::Anthropic(
                    client
                        .agent(model_name)
                        .tool(crate::tools::ShellTool::new())
                        .default_max_turns(usize::MAX / 2)
                        .build(),
                ),
                config.clone(),
            );
            shared_tree = new_shared(tree);
            root_id = shared_tree.lock().await.root_id();
            let ag = client
                .agent(model_name)
                .tool(crate::tools::ShellTool::new())
                .tool(crate::tools::SpawnTool::new(
                    config.clone(),
                    shared_tree.clone(),
                    root_id,
                ))
                .tool(crate::tools::SendTool::new(shared_tree.clone(), root_id))
                .default_max_turns(usize::MAX / 2)
                .build();
            Ok((AnyAgent::Anthropic(ag), shared_tree, root_id))
        },
    }
}
