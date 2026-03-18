use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use rig::client::CompletionClient;
use rig::completion::Message;
use serde::Deserialize;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tracing::{info, warn};
use ulid::Ulid;

use crate::agent::AnyAgent;
use crate::config::{Config, Provider, ProviderType};
use crate::tools::{SendTool, ShellTool, SpawnTool};

const DEFAULT_MAX_TURNS: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
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
    pub cancelled: Arc<AtomicBool>,
}

impl AgentHandle {
    fn new(id: Ulid, parent_id: Option<Ulid>) -> Self {
        Self {
            id,
            parent_id,
            pending_messages: Arc::new(Mutex::new(Vec::new())),
            history_messages: Arc::new(Mutex::new(Vec::new())),
            children: Arc::new(Mutex::new(Vec::new())),
            tasks: Arc::new(Mutex::new(JoinSet::new())),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub struct AgentTree {
    handles: HashMap<Ulid, AgentHandle>,
    agents: HashMap<Ulid, Arc<AnyAgent>>,
    root_id: Ulid,
    #[expect(dead_code)]
    config: Config,
}

impl AgentTree {
    pub fn new(config: Config) -> Self {
        let root_id = Ulid::new();
        let mut handles = HashMap::new();
        handles.insert(root_id, AgentHandle::new(root_id, None));

        Self {
            handles,
            agents: HashMap::new(),
            root_id,
            config,
        }
    }

    pub fn root_id(&self) -> Ulid {
        self.root_id
    }

    pub async fn add_child_with_id(
        &mut self,
        parent_id: Ulid,
        child_id: Ulid,
        child_agent: AnyAgent,
    ) -> Ulid {
        let handle = AgentHandle::new(child_id, Some(parent_id));
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

    pub async fn get_children_count(&self, parent_id: Ulid) -> usize {
        if let Some(handle) = self.handles.get(&parent_id) {
            handle.children.lock().await.len()
        } else {
            0
        }
    }

    pub async fn get_active_spawn_count(&self) -> usize {
        let mut total = 0;
        for handle in self.handles.values() {
            let guard = handle.tasks.lock().await;
            total += guard.len();
        }
        total
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
                handle.cancelled.store(true, Ordering::SeqCst);
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

    #[expect(dead_code)]
    pub async fn interrupt_agent(&self, id: Ulid) {
        if let Some(handle) = self.handles.get(&id) {
            handle.cancelled.store(true, Ordering::SeqCst);
            let tasks = handle.tasks.clone();
            let mut tasks_guard = tasks.lock().await;
            tasks_guard.abort_all();
        }
    }

    pub async fn run_child_agent(&self, child_id: Ulid, message: String, insert_mode: InsertMode) {
        use crate::agent::chat_with_agent;

        let agent = match self.agents.get(&child_id) {
            Some(a) => a.clone(),
            None => return,
        };

        let parent_id = self.get_parent_id(child_id);
        let parent_pending = parent_id
            .and_then(|pid| self.handles.get(&pid))
            .map(|h| h.pending_messages.clone());
        let child_id_clone = child_id;

        let cancelled = self.handles.get(&child_id).map(|h| h.cancelled.clone());

        let handle_tasks = self.handles.get(&child_id).map(|h| h.tasks.clone());

        let task = async move {
            let history: Vec<Message> = Vec::new();
            let cancel_flag = cancelled.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));

            let response = match agent.as_ref() {
                AnyAgent::OpenAICompletions(a) => {
                    chat_with_agent(a, &message, &history, None, &cancel_flag).await
                },
                AnyAgent::OpenAIResponses(a) => {
                    chat_with_agent(a, &message, &history, None, &cancel_flag).await
                },
                AnyAgent::Anthropic(a) => {
                    chat_with_agent(a, &message, &history, None, &cancel_flag).await
                },
            };

            let response = match response {
                Ok((resp, _)) => resp,
                Err(e) => {
                    warn!(agent_id = %child_id_clone, error = %e, "Child agent execution failed");
                    format!("Error: {e}")
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
            let mut tasks = tasks_arc.lock().await;
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

struct AgentBuildContext<'a> {
    provider: &'a Provider,
    api_key: &'a str,
    model_name: &'a str,
    shared_tree: &'a SharedAgentTree,
    root_id: Ulid,
    config: &'a Config,
}

fn build_agent_for_provider(ctx: &AgentBuildContext<'_>) -> Result<AnyAgent> {
    match ctx.provider.r#type {
        ProviderType::OpenAICompletions => {
            use rig::providers::openai::CompletionsClient;
            let client = CompletionsClient::builder()
                .api_key(ctx.api_key)
                .base_url(&ctx.provider.base_url)
                .build()
                .context("Failed to create OpenAI Completions client")?;

            let agent = client
                .agent(ctx.model_name)
                .tool(ShellTool::new())
                .tool(SpawnTool::new(
                    ctx.config.clone(),
                    ctx.shared_tree.clone(),
                    ctx.root_id,
                ))
                .tool(SendTool::new(ctx.shared_tree.clone(), ctx.root_id))
                .default_max_turns(DEFAULT_MAX_TURNS)
                .build();

            Ok(AnyAgent::OpenAICompletions(agent))
        },
        ProviderType::OpenAIResponses => {
            use rig::providers::openai::Client as OpenAIClient;
            let client = OpenAIClient::builder()
                .api_key(ctx.api_key)
                .base_url(&ctx.provider.base_url)
                .build()
                .context("Failed to create OpenAI Responses client")?;

            let agent = client
                .agent(ctx.model_name)
                .tool(ShellTool::new())
                .tool(SpawnTool::new(
                    ctx.config.clone(),
                    ctx.shared_tree.clone(),
                    ctx.root_id,
                ))
                .tool(SendTool::new(ctx.shared_tree.clone(), ctx.root_id))
                .default_max_turns(DEFAULT_MAX_TURNS)
                .build();

            Ok(AnyAgent::OpenAIResponses(agent))
        },
        ProviderType::Anthropic => {
            use rig::providers::anthropic::Client;
            let client = Client::builder()
                .api_key(ctx.api_key)
                .base_url(&ctx.provider.base_url)
                .anthropic_version(&ctx.provider.anthropic_version)
                .build()
                .context("Failed to create Anthropic client")?;

            let agent = client
                .agent(ctx.model_name)
                .tool(ShellTool::new())
                .tool(SpawnTool::new(
                    ctx.config.clone(),
                    ctx.shared_tree.clone(),
                    ctx.root_id,
                ))
                .tool(SendTool::new(ctx.shared_tree.clone(), ctx.root_id))
                .default_max_turns(DEFAULT_MAX_TURNS)
                .build();

            Ok(AnyAgent::Anthropic(agent))
        },
    }
}

pub async fn create_agent_with_tools(
    config: &Config,
    provider: &Provider,
    api_key: &str,
    model_name: &str,
) -> Result<(AnyAgent, SharedAgentTree, Ulid)> {
    let tree = AgentTree::new(config.clone());
    let shared_tree = new_shared(tree);
    let root_id = shared_tree.lock().await.root_id();

    let ctx = AgentBuildContext {
        provider,
        api_key,
        model_name,
        shared_tree: &shared_tree,
        root_id,
        config,
    };

    let agent = build_agent_for_provider(&ctx)?;

    Ok((agent, shared_tree, root_id))
}
