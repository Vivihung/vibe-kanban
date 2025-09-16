use std::{
    path::Path,
    process::Stdio,
};

use anyhow::Error as AnyhowError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::Command;
use ts_rs::TS;
use uuid::Uuid;

use executors::actions::browser_chat_request::{BrowserChatAgentType, BrowserChatRequest};

#[derive(Debug, Error)]
pub enum BrowserChatError {
    #[error("Node.js process spawn failed: {0}")]
    SpawnFailed(#[from] std::io::Error),
    #[error("Browser automation script not found: {0}")]
    ScriptNotFound(String),
    #[error("Browser automation failed: {0}")]
    AutomationFailed(String),
    #[error(transparent)]
    Other(#[from] AnyhowError),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
pub struct BrowserChatResponse {
    pub success: bool,
    pub message: String,
    pub error: Option<String>,
    pub session_id: Option<String>,
}

#[async_trait]
pub trait BrowserChatService {
    /// Send a message to a browser-based chat agent
    async fn send_message(
        &self,
        request: &BrowserChatRequest,
        execution_id: Uuid,
    ) -> Result<BrowserChatResponse, BrowserChatError>;

    /// Check if the browser automation environment is ready
    async fn health_check(&self) -> Result<bool, BrowserChatError>;
}

pub struct NodeBrowserChatService {
    script_path: String,
}

impl NodeBrowserChatService {
    pub fn new(script_path: String) -> Self {
        Self { script_path }
    }

    /// Get the script path for the given agent type
    fn get_agent_script_path(&self, agent_type: &BrowserChatAgentType) -> String {
        match agent_type {
            BrowserChatAgentType::Claude => {
                format!("{}/claude-automation.js", self.script_path)
            }
            BrowserChatAgentType::M365Copilot => {
                format!("{}/m365-automation.js", self.script_path)
            }
        }
    }

    /// Validate that required scripts exist
    async fn validate_script_exists(&self, script_path: &str) -> Result<(), BrowserChatError> {
        if !Path::new(script_path).exists() {
            return Err(BrowserChatError::ScriptNotFound(script_path.to_string()));
        }
        Ok(())
    }
}

#[async_trait]
impl BrowserChatService for NodeBrowserChatService {
    async fn send_message(
        &self,
        request: &BrowserChatRequest,
        execution_id: Uuid,
    ) -> Result<BrowserChatResponse, BrowserChatError> {
        let script_path = self.get_agent_script_path(&request.agent_type);
        
        // Validate script exists
        self.validate_script_exists(&script_path).await?;

        // Prepare the command to run the Node.js script
        let mut cmd = Command::new("node");
        cmd.arg(&script_path)
            .arg("--message")
            .arg(&request.message)
            .arg("--execution-id")
            .arg(&execution_id.to_string())
            .arg("--agent-type")
            .arg(&format!("{:?}", request.agent_type))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Execute the command
        let output = cmd.output().await?;

        if output.status.success() {
            // Parse the JSON response from stdout
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<BrowserChatResponse>(&stdout) {
                Ok(response) => Ok(response),
                Err(e) => {
                    tracing::error!("Failed to parse browser chat response: {}", e);
                    Ok(BrowserChatResponse {
                        success: false,
                        message: "Failed to parse automation response".to_string(),
                        error: Some(format!("JSON parse error: {}", e)),
                        session_id: None,
                    })
                }
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::error!("Browser automation script failed: {}", stderr);
            
            Err(BrowserChatError::AutomationFailed(format!(
                "Script execution failed with exit code {}: {}",
                output.status.code().unwrap_or(-1),
                stderr
            )))
        }
    }

    async fn health_check(&self) -> Result<bool, BrowserChatError> {
        // Check if Node.js is available
        let node_check = Command::new("node")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await?;

        if !node_check.success() {
            return Ok(false);
        }

        // Check if required scripts exist
        for agent_type in [BrowserChatAgentType::Claude, BrowserChatAgentType::M365Copilot] {
            let script_path = self.get_agent_script_path(&agent_type);
            if !Path::new(&script_path).exists() {
                tracing::warn!("Browser automation script not found: {}", script_path);
                return Ok(false);
            }
        }

        Ok(true)
    }
}