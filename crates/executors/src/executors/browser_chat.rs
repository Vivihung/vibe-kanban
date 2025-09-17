use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use command_group::{AsyncCommandGroup, AsyncGroupChild};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, process::Command};
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
        current_dir: &Path,
        prompt: &str,
    ) -> Result<AsyncGroupChild, ExecutorError> {
        tracing::info!("Starting Claude Browser Chat automation with prompt: {}", prompt);
        
        // Construct path to the browser automation CLI
        let cli_path = current_dir.join("browser-automation/dist/claude-chat-cli.js");
        
        // Check if CLI exists and is built
        if !cli_path.exists() {
            return Err(ExecutorError::FollowUpNotSupported(
                "Browser automation CLI not found. Run 'cd browser-automation && npm run build' first".to_string()
            ));
        }
        
        let mut command = Command::new("node");
        command
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(current_dir)
            .arg(&cli_path)
            .arg("--agent")
            .arg("claude")
            .arg("--message")
            .arg(prompt);

        tracing::debug!("Executing command: node {:?} --agent claude --message {:?}", cli_path, prompt);

        let mut child = command
            .group_spawn()
            .map_err(|e| ExecutorError::Io(e))?;

        // The browser automation handles its own interaction, so we don't need to write to stdin
        // Just close stdin to let the process run independently
        if let Some(mut stdin) = child.inner().stdin.take() {
            let _ = stdin.shutdown().await;
        }

        Ok(child)
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
        current_dir: &Path,
        prompt: &str,
    ) -> Result<AsyncGroupChild, ExecutorError> {
        tracing::info!("Starting M365 Copilot Chat automation with prompt: {}", prompt);
        
        // Construct path to the browser automation CLI
        let cli_path = current_dir.join("browser-automation/dist/m365-chat-cli.js");
        
        // Check if CLI exists and is built
        if !cli_path.exists() {
            return Err(ExecutorError::FollowUpNotSupported(
                "Browser automation CLI not found. Run 'cd browser-automation && npm run build' first".to_string()
            ));
        }
        
        let mut command = Command::new("node");
        command
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(current_dir)
            .arg(&cli_path)
            .arg("--agent")
            .arg("m365")
            .arg("--message")
            .arg(prompt);

        tracing::debug!("Executing command: node {:?} --agent m365 --message {:?}", cli_path, prompt);

        let mut child = command
            .group_spawn()
            .map_err(|e| ExecutorError::Io(e))?;

        // The browser automation handles its own interaction, so we don't need to write to stdin
        // Just close stdin to let the process run independently
        if let Some(mut stdin) = child.inner().stdin.take() {
            let _ = stdin.shutdown().await;
        }

        Ok(child)
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