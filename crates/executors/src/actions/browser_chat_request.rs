use std::path::Path;

use async_trait::async_trait;
use command_group::AsyncGroupChild;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    actions::Executable,
    executors::ExecutorError,
    profile::ExecutorProfileId,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "lowercase")]
pub enum BrowserChatAgentType {
    Claude,
    #[serde(rename = "m365")]
    M365Copilot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
pub struct BrowserChatRequest {
    pub message: String,
    pub agent_type: BrowserChatAgentType,
    pub executor_profile_id: ExecutorProfileId,
}

#[async_trait]
impl Executable for BrowserChatRequest {
    async fn spawn(&self, _current_dir: &Path) -> Result<AsyncGroupChild, ExecutorError> {
        use std::process::Stdio;
        use tokio::process::Command;
        use command_group::AsyncCommandGroup;
        
        // Determine the browser automation command based on agent type
        let (script_name, agent_arg) = match self.agent_type {
            BrowserChatAgentType::Claude => ("dist/claude-chat-cli.js", "claude"),
            BrowserChatAgentType::M365Copilot => ("dist/m365-chat-cli.js", "m365"),
        };

        // Build the Node.js command to run browser automation
        let mut cmd = Command::new("node");
        cmd.arg(format!("./browser-automation/{}", script_name))
           .arg("--agent")
           .arg(agent_arg)
           .arg("--message")
           .arg(&self.message)
           .stdin(Stdio::piped())
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());

        // Use group_spawn to create AsyncGroupChild directly
        let child = cmd.group_spawn().map_err(ExecutorError::Io)?;
        Ok(child)
    }
}