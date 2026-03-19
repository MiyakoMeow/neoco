use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use rig::client::CompletionClient;
use rig::completion::Message;
use serde::Deserialize;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tracing::{info, warn};
use ulid::Ulid;

use crate::agent::{AnyAgent, chat_with_agent};
use crate::config::{Config, Provider};

const DEFAULT_MAX_TURNS: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum InsertMode {
    #[default]
    Queue,
    Interrupt,
    Append,
}

#[derive(Debug, Clone)]
pub struct QueuedMessage {
    pub content: String,
    pub mode: InsertMode,
    pub from_agent_id: Ulid,
}

#[derive(Debug, Clone)]
pub struct AgentHandle {
    #[expect(dead_code)]
    pub id: Ulid,
    pub parent_id: Option<Ulid>,
    pub pending_messages: Arc<Mutex<Vec<QueuedMessage>>>,
    pub history_messages: Arc<Mutex<Vec<QueuedMessage>>>,
    pub children: Arc<Mutex<Vec<Ulid>>>,
    pub tasks: Arc<Mutex<JoinSet<()>>>,
}

pub struct AgentTree {
    handles: HashMap<Ulid, AgentHandle>,
    agents: HashMap<Ulid, Arc<AnyAgent>>,
    root_id: Ulid,
    #[expect(dead_code)]
    pub config: Config,
}

impl AgentTree {
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
                history_messages: Arc::new(Mutex::new(Vec::new())),
                children: Arc::new(Mutex::new(Vec::new())),
                tasks: Arc::new(Mutex::new(JoinSet::new())),
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

    pub fn root_id(&self) -> Ulid {
        self.root_id
    }

    #[expect(dead_code)]
    pub fn update_agent(&mut self, id: Ulid, agent: AnyAgent) {
        if let Some(existing) = self.agents.get_mut(&id) {
            *existing = Arc::new(agent);
        }
    }

    #[expect(dead_code)]
    pub async fn add_child_async(&mut self, parent_id: Ulid, child_agent: AnyAgent) -> Ulid {
        let child_id = Ulid::new();
        self.add_child_with_id(parent_id, child_id, child_agent)
            .await
    }

    pub async fn add_child_with_id(
        &mut self,
        parent_id: Ulid,
        child_id: Ulid,
        child_agent: AnyAgent,
    ) -> Ulid {
        let handle = AgentHandle {
            id: child_id,
            parent_id: Some(parent_id),
            pending_messages: Arc::new(Mutex::new(Vec::new())),
            history_messages: Arc::new(Mutex::new(Vec::new())),
            children: Arc::new(Mutex::new(Vec::new())),
            tasks: Arc::new(Mutex::new(JoinSet::new())),
        };

        self.handles.insert(child_id, handle);
        self.agents.insert(child_id, Arc::new(child_agent));

        if let Some(parent) = self.handles.get_mut(&parent_id) {
            let mut children = parent.children.lock().await;
            children.push(child_id);
        }

        child_id
    }

    pub fn get_agent(&self, id: Ulid) -> Option<Arc<AnyAgent>> {
        self.agents.get(&id).cloned()
    }

    pub fn get_parent_id(&self, id: Ulid) -> Option<Ulid> {
        self.handles.get(&id).and_then(|h| h.parent_id)
    }

    pub fn get_agent_depth(&self, id: Ulid) -> usize {
        let mut depth = 0;
        let mut current_id = Some(id);
        while let Some(cid) = current_id {
            if let Some(handle) = self.handles.get(&cid) {
                current_id = handle.parent_id;
                if current_id.is_some() {
                    depth += 1;
                }
            } else {
                break;
            }
        }
        depth
    }

    pub fn get_children_count(&self, parent_id: Ulid) -> usize {
        self.handles
            .get(&parent_id)
            .map_or(0, |h| h.children.blocking_lock().len())
    }

    pub fn get_active_spawn_count(&self) -> usize {
        self.handles
            .values()
            .map(|h| h.tasks.blocking_lock().len())
            .sum()
    }

    #[expect(dead_code)]
    pub async fn wait_for_child_tasks(&self, id: Ulid) {
        let Some(handle) = self.handles.get(&id) else {
            return;
        };
        let tasks = handle.tasks.clone();
        let mut guard = tasks.lock().await;
        while guard.join_next().await.is_some() {}
    }

    pub async fn add_pending_message(&self, target_id: Ulid, message: QueuedMessage) {
        let Some(handle) = self.handles.get(&target_id) else {
            return;
        };

        match message.mode {
            InsertMode::Queue => {
                let pending = handle.pending_messages.clone();
                let mut guard = pending.lock().await;
                guard.push(message);
            },
            InsertMode::Interrupt => {
                let pending = handle.pending_messages.clone();
                let mut guard = pending.lock().await;
                guard.insert(0, message);
                drop(guard);

                let tasks = handle.tasks.clone();
                let mut tasks_guard = tasks.lock().await;
                info!(agent_id = %target_id, "Interrupting current task");
                tasks_guard.abort_all();
            },
            InsertMode::Append => {
                let history = handle.history_messages.clone();
                let mut guard = history.lock().await;
                guard.push(message);
            },
        }
    }

    pub async fn get_history_messages(&self, id: Ulid) -> Vec<QueuedMessage> {
        if let Some(handle) = self.handles.get(&id) {
            let history = handle.history_messages.clone();
            let guard = history.lock().await;
            guard.clone()
        } else {
            Vec::new()
        }
    }

    pub async fn clear_history_messages(&self, id: Ulid) {
        if let Some(handle) = self.handles.get(&id) {
            let history = handle.history_messages.clone();
            let mut guard = history.lock().await;
            guard.clear();
        }
    }

    pub async fn drain_pending_messages(&self, id: Ulid) -> Vec<QueuedMessage> {
        if let Some(handle) = self.handles.get(&id) {
            let pending = handle.pending_messages.clone();
            let mut guard = pending.lock().await;
            std::mem::take(&mut *guard)
        } else {
            Vec::new()
        }
    }

    pub fn run_child_agent(&self, child_id: Ulid, message: String, insert_mode: InsertMode) {
        let agent = match self.agents.get(&child_id) {
            Some(a) => a.clone(),
            None => return,
        };

        let parent_id = self.get_parent_id(child_id);
        let parent_pending = parent_id
            .and_then(|pid| self.handles.get(&pid))
            .map(|h| h.pending_messages.clone());
        let child_id_clone = child_id;

        let handle_tasks = self.handles.get(&child_id).map(|h| h.tasks.clone());

        let task = async move {
            let history: Vec<Message> = Vec::new();

            let response = match agent.as_ref() {
                AnyAgent::OpenAICompletions(a) => {
                    match chat_with_agent(a, &message, &history, None).await {
                        Ok((resp, _)) => resp,
                        Err(e) => {
                            warn!(agent_id = %child_id_clone, error = %e, "Child agent execution failed");
                            format!("Error: {e}")
                        },
                    }
                },
                AnyAgent::OpenAIResponses(a) => {
                    match chat_with_agent(a, &message, &history, None).await {
                        Ok((resp, _)) => resp,
                        Err(e) => {
                            warn!(agent_id = %child_id_clone, error = %e, "Child agent execution failed");
                            format!("Error: {e}")
                        },
                    }
                },
                AnyAgent::Anthropic(a) => {
                    match chat_with_agent(a, &message, &history, None).await {
                        Ok((resp, _)) => resp,
                        Err(e) => {
                            warn!(agent_id = %child_id_clone, error = %e, "Child agent execution failed");
                            format!("Error: {e}")
                        },
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
        };

        if let Some(tasks_arc) = handle_tasks {
            let mut tasks = tasks_arc.blocking_lock();
            tasks.spawn(task);
        } else {
            tokio::spawn(task);
        }
    }
}

pub type SharedAgentTree = Arc<Mutex<AgentTree>>;

pub fn new_shared(tree: AgentTree) -> SharedAgentTree {
    Arc::new(Mutex::new(tree))
}

pub async fn create_agent_with_tools(
    config: &Config,
    provider: &Provider,
    api_key: &str,
    model_name: &str,
) -> Result<(AnyAgent, SharedAgentTree, Ulid)> {
    let placeholder = match provider.r#type {
        crate::config::ProviderType::OpenAICompletions => {
            use rig::providers::openai::CompletionsClient;
            let client = CompletionsClient::builder()
                .api_key(api_key)
                .base_url(&provider.base_url)
                .build()
                .context("Failed to create OpenAI Completions client")?;
            AnyAgent::OpenAICompletions(
                client
                    .agent(model_name)
                    .tool(crate::tools::ShellTool::new())
                    .default_max_turns(DEFAULT_MAX_TURNS)
                    .build(),
            )
        },
        crate::config::ProviderType::OpenAIResponses => {
            use rig::providers::openai::Client as OpenAIClient;
            let client = OpenAIClient::builder()
                .api_key(api_key)
                .base_url(&provider.base_url)
                .build()
                .context("Failed to create OpenAI Responses client")?;
            AnyAgent::OpenAIResponses(
                client
                    .agent(model_name)
                    .tool(crate::tools::ShellTool::new())
                    .default_max_turns(DEFAULT_MAX_TURNS)
                    .build(),
            )
        },
        crate::config::ProviderType::Anthropic => {
            use rig::providers::anthropic::Client;
            let client = Client::builder()
                .api_key(api_key)
                .base_url(&provider.base_url)
                .anthropic_version(&provider.anthropic_version)
                .build()
                .context("Failed to create Anthropic client")?;
            AnyAgent::Anthropic(
                client
                    .agent(model_name)
                    .tool(crate::tools::ShellTool::new())
                    .default_max_turns(DEFAULT_MAX_TURNS)
                    .build(),
            )
        },
    };

    let tree = AgentTree::new(placeholder, config.clone());
    let shared_tree = new_shared(tree);
    let root_id = shared_tree.lock().await.root_id();

    let agent = match provider.r#type {
        crate::config::ProviderType::OpenAICompletions => {
            use rig::providers::openai::CompletionsClient;
            let client = CompletionsClient::builder()
                .api_key(api_key)
                .base_url(&provider.base_url)
                .build()
                .context("Failed to create OpenAI Completions client")?;
            AnyAgent::OpenAICompletions(
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
            )
        },
        crate::config::ProviderType::OpenAIResponses => {
            use rig::providers::openai::Client as OpenAIClient;
            let client = OpenAIClient::builder()
                .api_key(api_key)
                .base_url(&provider.base_url)
                .build()
                .context("Failed to create OpenAI Responses client")?;
            AnyAgent::OpenAIResponses(
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
            )
        },
        crate::config::ProviderType::Anthropic => {
            use rig::providers::anthropic::Client;
            let client = Client::builder()
                .api_key(api_key)
                .base_url(&provider.base_url)
                .anthropic_version(&provider.anthropic_version)
                .build()
                .context("Failed to create Anthropic client")?;
            AnyAgent::Anthropic(
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
            )
        },
    };

    Ok((agent, shared_tree, root_id))
}
