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
        // TODO: Implement browser chat process spawning
        // This will integrate with the browser automation service
        Err(ExecutorError::FollowUpNotSupported(
            format!("BrowserChatRequest for {:?} not yet implemented", self.agent_type),
        ))
    }
}