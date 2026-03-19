use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use rig::completion::Message;
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::{info, warn};
use ulid::Ulid;

use crate::agent::AnyAgent;
use crate::config::{Config, Provider};
use crate::tools::{AgentBuildConfig, build_agent_with_tools};

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

    pub fn get_active_spawn_count(&self) -> usize {
        let mut total = 0;
        for handle in self.handles.values() {
            if handle.cancelled.load(Ordering::SeqCst) {
                total += 1;
            }
        }
        total
    }

    pub async fn add_pending_message(&self, target_id: Ulid, message: QueuedMessage) {
        let Some(handle) = self.handles.get(&target_id) else {
            return;
        };

        match message.mode {
            InsertMode::Queue => {
                let mut guard = handle.pending_messages.lock().await;
                guard.push(message);
            },
            InsertMode::Interrupt => {
                {
                    let mut guard = handle.pending_messages.lock().await;
                    guard.insert(0, message);
                }

                info!(agent_id = %target_id, "Interrupting current task");
                handle.cancelled.store(true, Ordering::SeqCst);
            },
            InsertMode::Append => {
                let mut guard = handle.history_messages.lock().await;
                guard.push(message);
            },
        }
    }

    pub async fn get_history_messages(&self, id: Ulid) -> Vec<QueuedMessage> {
        if let Some(handle) = self.handles.get(&id) {
            let guard = handle.history_messages.lock().await;
            guard.clone()
        } else {
            Vec::new()
        }
    }

    pub async fn clear_history_messages(&self, id: Ulid) {
        if let Some(handle) = self.handles.get(&id) {
            let mut guard = handle.history_messages.lock().await;
            guard.clear();
        }
    }

    pub async fn drain_pending_messages(&self, id: Ulid) -> Vec<QueuedMessage> {
        if let Some(handle) = self.handles.get(&id) {
            let mut guard = handle.pending_messages.lock().await;
            std::mem::take(&mut *guard)
        } else {
            Vec::new()
        }
    }

    #[expect(dead_code)]
    pub fn interrupt_agent(&self, id: Ulid) {
        let Some(handle) = self.handles.get(&id) else {
            return;
        };

        handle.cancelled.store(true, Ordering::SeqCst);
    }

    #[expect(dead_code)]
    pub fn shutdown_agent(&self, id: Ulid) {
        let Some(handle) = self.handles.get(&id) else {
            return;
        };

        handle.cancelled.store(true, Ordering::SeqCst);
    }

    #[expect(dead_code)]
    pub async fn remove_agent(&mut self, id: Ulid) {
        if id == self.root_id {
            return;
        }

        let handle = self.handles.remove(&id);
        if let Some(handle) = handle {
            handle.cancelled.store(true, Ordering::SeqCst);

            if let Some(parent_id) = handle.parent_id
                && let Some(parent) = self.handles.get_mut(&parent_id)
            {
                let mut children = parent.children.lock().await;
                children.retain(|c| *c != id);
            }
        }

        self.agents.remove(&id);
    }

    pub async fn run_child_agent(&self, child_id: Ulid, message: String, insert_mode: InsertMode) {
        use crate::agent::AgentChat;

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

        let task = async move {
            let history: Vec<Message> = Vec::new();
            let cancel_flag = cancelled.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));

            let response = agent
                .as_ref()
                .chat(&message, &history, None, &cancel_flag)
                .await;

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

        if let Some(_handle) = self.handles.get(&child_id) {
            let jh = tokio::spawn(task);
            jh.await.ok();
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
    let config_arc = Arc::new(config.clone());
    let tree = AgentTree::new(config.clone());
    let shared_tree = new_shared(tree);
    let root_id = shared_tree.lock().await.root_id();

    let build_config = AgentBuildConfig {
        provider,
        api_key,
        model_name,
        config: config_arc,
        shared_tree: shared_tree.clone(),
        agent_id: root_id,
    };

    let agent = build_agent_with_tools(build_config)?;

    Ok((agent, shared_tree, root_id))
}
