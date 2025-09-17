use axum::{
    Router,
    extract::{Path, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use db::models::{
    execution_process::ExecutionProcessRunReason,
    task_attempt::{TaskAttempt, TaskAttemptError},
};
use deployment::Deployment;
use executors::actions::{
    ExecutorAction, ExecutorActionType,
    browser_chat_request::BrowserChatRequest,
};
use serde::{Deserialize, Serialize};
use services::services::{
    browser_chat_service::{BrowserChatService, NodeBrowserChatService},
    container::ContainerService,
};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Deserialize, TS)]
pub struct SendBrowserChatMessageRequest {
    pub message: String,
    pub agent_type: executors::actions::browser_chat_request::BrowserChatAgentType,
    pub executor_profile_id: executors::profile::ExecutorProfileId,
}

#[derive(Debug, Serialize, TS)]
pub struct SendBrowserChatMessageResponse {
    pub execution_process_id: Uuid,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, TS)]
pub struct BrowserChatHealthResponse {
    pub healthy: bool,
    pub message: String,
}

pub async fn send_browser_chat_message(
    State(deployment): State<DeploymentImpl>,
    Path(task_attempt_id): Path<Uuid>,
    ResponseJson(request): ResponseJson<SendBrowserChatMessageRequest>,
) -> Result<ResponseJson<ApiResponse<SendBrowserChatMessageResponse>>, ApiError> {
    // Get the task attempt
    let task_attempt = TaskAttempt::find_by_id(&deployment.db().pool, task_attempt_id)
        .await?
        .ok_or(ApiError::TaskAttempt(TaskAttemptError::ValidationError("Task attempt not found".to_string())))?;

    // Create browser chat request action
    let browser_chat_request = BrowserChatRequest {
        message: request.message,
        agent_type: request.agent_type,
        executor_profile_id: request.executor_profile_id,
        session_id: None, // Initial request has no session ID
    };

    let executor_action = ExecutorAction::new(
        ExecutorActionType::BrowserChatRequest(browser_chat_request),
        None,
    );

    // Start execution process
    let execution_process = deployment
        .container()
        .start_execution(
            &task_attempt,
            &executor_action,
            &ExecutionProcessRunReason::BrowserChat,
        )
        .await?;

    let response = SendBrowserChatMessageResponse {
        execution_process_id: execution_process.id,
        success: true,
        message: "Browser chat execution started".to_string(),
    };

    Ok(ResponseJson(ApiResponse::success(response)))
}

pub async fn get_browser_chat_health(
    State(_deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<BrowserChatHealthResponse>>, ApiError> {
    // TODO: Make script path configurable
    let browser_chat_service = NodeBrowserChatService::new("./browser-automation".to_string());
    
    match browser_chat_service.health_check().await {
        Ok(healthy) => {
            let response = BrowserChatHealthResponse {
                healthy,
                message: if healthy {
                    "Browser automation environment is ready".to_string()
                } else {
                    "Browser automation environment is not available".to_string()
                },
            };
            Ok(ResponseJson(ApiResponse::success(response)))
        }
        Err(e) => {
            let response = BrowserChatHealthResponse {
                healthy: false,
                message: format!("Health check failed: {}", e),
            };
            Ok(ResponseJson(ApiResponse::success(response)))
        }
    }
}

pub fn router(_deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    Router::new()
        .route("/health", get(get_browser_chat_health))
        .route("/task-attempts/{task_attempt_id}/send", post(send_browser_chat_message))
}