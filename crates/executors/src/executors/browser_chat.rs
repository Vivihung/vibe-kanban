use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use command_group::AsyncGroupChild;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::msg_store::MsgStore;

use super::{ExecutorError, StandardCodingAgentExecutor};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
pub struct ClaudeBrowserChat;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
pub struct M365CopilotChat;

#[async_trait]
impl StandardCodingAgentExecutor for ClaudeBrowserChat {
    async fn spawn(
        &self,
        _current_dir: &Path,
        _prompt: &str,
    ) -> Result<AsyncGroupChild, ExecutorError> {
        // TODO: Implement browser chat spawning for Claude
        Err(ExecutorError::FollowUpNotSupported(
            "ClaudeBrowserChat not yet implemented".to_string(),
        ))
    }

    async fn spawn_follow_up(
        &self,
        _current_dir: &Path,
        _prompt: &str,
        _session_id: &str,
    ) -> Result<AsyncGroupChild, ExecutorError> {
        Err(ExecutorError::FollowUpNotSupported(
            "ClaudeBrowserChat follow-up not yet implemented".to_string(),
        ))
    }

    fn normalize_logs(&self, _raw_logs_event_store: Arc<MsgStore>, _worktree_path: &Path) {
        // TODO: Implement log normalization for browser chat
    }

    fn default_mcp_config_path(&self) -> Option<PathBuf> {
        None // Browser chat doesn't use MCP
    }
}

#[async_trait]
impl StandardCodingAgentExecutor for M365CopilotChat {
    async fn spawn(
        &self,
        _current_dir: &Path,
        _prompt: &str,
    ) -> Result<AsyncGroupChild, ExecutorError> {
        // TODO: Implement browser chat spawning for M365 Copilot
        Err(ExecutorError::FollowUpNotSupported(
            "M365CopilotChat not yet implemented".to_string(),
        ))
    }

    async fn spawn_follow_up(
        &self,
        _current_dir: &Path,
        _prompt: &str,
        _session_id: &str,
    ) -> Result<AsyncGroupChild, ExecutorError> {
        Err(ExecutorError::FollowUpNotSupported(
            "M365CopilotChat follow-up not yet implemented".to_string(),
        ))
    }

    fn normalize_logs(&self, _raw_logs_event_store: Arc<MsgStore>, _worktree_path: &Path) {
        // TODO: Implement log normalization for browser chat
    }

    fn default_mcp_config_path(&self) -> Option<PathBuf> {
        None // Browser chat doesn't use MCP
    }
}