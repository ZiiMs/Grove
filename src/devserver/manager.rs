use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use super::process::DevServer;
use crate::app::{Action, DevServerConfig};

pub struct DevServerManager {
    servers: HashMap<Uuid, DevServer>,
    action_tx: UnboundedSender<Action>,
}

impl DevServerManager {
    pub fn new(action_tx: UnboundedSender<Action>) -> Self {
        Self {
            servers: HashMap::new(),
            action_tx,
        }
    }

    pub fn has_running_server(&self) -> bool {
        self.servers.values().any(|s| s.status().is_running())
    }

    pub fn running_servers(&self) -> Vec<(Uuid, String, Option<u16>)> {
        self.servers
            .iter()
            .filter_map(|(id, server)| {
                if server.status().is_running() {
                    Some((*id, server.agent_name().to_string(), server.status().port()))
                } else {
                    None
                }
            })
            .collect()
    }

    pub async fn start(
        &mut self,
        agent_id: Uuid,
        agent_name: String,
        config: &DevServerConfig,
        worktree: &Path,
    ) -> Result<()> {
        let server = self.servers.entry(agent_id).or_default();
        server.set_agent_name(agent_name.clone());
        server
            .start(
                config,
                worktree,
                agent_id,
                agent_name,
                self.action_tx.clone(),
            )
            .await
    }

    pub async fn stop(&mut self, agent_id: Uuid) -> Result<()> {
        if let Some(server) = self.servers.get_mut(&agent_id) {
            server.stop().await?;
        }
        Ok(())
    }

    pub async fn stop_all(&mut self) -> Result<()> {
        for server in self.servers.values_mut() {
            let _ = server.stop().await;
        }
        Ok(())
    }

    pub fn get(&self, agent_id: Uuid) -> Option<&DevServer> {
        self.servers.get(&agent_id)
    }

    pub fn get_mut(&mut self, agent_id: Uuid) -> Option<&mut DevServer> {
        self.servers.get_mut(&agent_id)
    }

    pub fn remove(&mut self, agent_id: Uuid) {
        self.servers.remove(&agent_id);
    }

    pub fn is_running(&self, agent_id: Uuid) -> bool {
        self.servers
            .get(&agent_id)
            .map(|s| s.status().is_running())
            .unwrap_or(false)
    }

    pub fn get_tmux_session(&self, agent_id: Uuid) -> Option<String> {
        self.servers
            .get(&agent_id)
            .and_then(|s| s.tmux_session().map(String::from))
    }
}

pub type SharedDevServerManager = Arc<tokio::sync::Mutex<DevServerManager>>;
