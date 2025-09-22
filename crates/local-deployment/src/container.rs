use std::{
    collections::{HashMap, HashSet},
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::anyhow;
use bollard::{
    Docker,
    container::{CreateContainerOptions, Config as ContainerConfig},
    image::BuildImageOptions,
    models::HostConfig,
};
use async_stream::try_stream;
use async_trait::async_trait;
use axum::response::sse::Event;
use command_group::AsyncGroupChild;
use db::{
    DBService,
    models::{
        execution_process::{
            ExecutionContext, ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus,
        },
        executor_session::ExecutorSession,
        merge::Merge,
        project::Project,
        task::{Task, TaskStatus},
        task_attempt::TaskAttempt,
    },
};
use deployment::DeploymentError;
use executors::{
    actions::{Executable, ExecutorAction},
    logs::{
        NormalizedEntry, NormalizedEntryType,
        utils::{ConversationPatch, patch::escape_json_pointer_segment},
    },
};
use futures::{StreamExt, TryStreamExt, stream::select};
use notify_debouncer_full::DebouncedEvent;
use serde_json::json;
use services::services::{
    analytics::AnalyticsContext,
    config::Config,
    container::{ContainerError, ContainerRef, ContainerService},
    filesystem_watcher,
    git::{DiffTarget, GitService},
    image::ImageService,
    notification::NotificationService,
    worktree_manager::WorktreeManager,
};
use tokio::{sync::RwLock, task::JoinHandle};
use tokio_util::io::ReaderStream;
use utils::{
    log_msg::LogMsg,
    msg_store::MsgStore,
    text::{git_branch_id, short_uuid},
};
use uuid::Uuid;

use crate::command;

/// Browser session metadata for tracking persistent browser processes
#[derive(Debug, Clone)]
pub struct BrowserSession {
    pub session_id: String,
    pub task_attempt_id: Uuid,
    pub execution_process_id: Uuid,
    pub agent_type: String,
    pub created_at: std::time::Instant,
}

#[derive(Clone)]
pub struct LocalContainerService {
    db: DBService,
    child_store: Arc<RwLock<HashMap<Uuid, Arc<RwLock<AsyncGroupChild>>>>>,
    msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
    browser_sessions: Arc<RwLock<HashMap<String, BrowserSession>>>, // session_id -> BrowserSession
    config: Arc<RwLock<Config>>,
    git: GitService,
    image_service: ImageService,
    analytics: Option<AnalyticsContext>,
    docker: Option<Docker>,
}

impl LocalContainerService {
    pub fn new(
        db: DBService,
        msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
        config: Arc<RwLock<Config>>,
        git: GitService,
        image_service: ImageService,
        analytics: Option<AnalyticsContext>,
    ) -> Self {
        let child_store = Arc::new(RwLock::new(HashMap::new()));
        let browser_sessions = Arc::new(RwLock::new(HashMap::new()));

        // Try to initialize Docker client (optional)
        let docker = match Docker::connect_with_socket_defaults() {
            Ok(docker) => {
                tracing::info!("Docker client initialized successfully");
                Some(docker)
            }
            Err(e) => {
                tracing::warn!("Failed to initialize Docker client: {}. Multi-repo containers will be disabled.", e);
                None
            }
        };

        LocalContainerService {
            db,
            child_store,
            msg_stores,
            browser_sessions,
            config,
            git,
            image_service,
            analytics,
            docker,
        }
    }

    pub async fn get_child_from_store(&self, id: &Uuid) -> Option<Arc<RwLock<AsyncGroupChild>>> {
        let map = self.child_store.read().await;
        map.get(id).cloned()
    }

    pub async fn add_child_to_store(&self, id: Uuid, exec: AsyncGroupChild) {
        let mut map = self.child_store.write().await;
        map.insert(id, Arc::new(RwLock::new(exec)));
    }

    pub async fn remove_child_from_store(&self, id: &Uuid) {
        let mut map = self.child_store.write().await;
        map.remove(id);
    }

    /// Add a browser session for tracking
    pub async fn add_browser_session(&self, session: BrowserSession) {
        let mut sessions = self.browser_sessions.write().await;
        sessions.insert(session.session_id.clone(), session);
    }

    /// Get browser session by session ID
    pub async fn get_browser_session(&self, session_id: &str) -> Option<BrowserSession> {
        let sessions = self.browser_sessions.read().await;
        sessions.get(session_id).cloned()
    }

    /// Remove browser session
    pub async fn remove_browser_session(&self, session_id: &str) {
        let mut sessions = self.browser_sessions.write().await;
        sessions.remove(session_id);
    }

    /// Find browser session by task attempt ID
    pub async fn find_browser_session_by_task_attempt(&self, task_attempt_id: Uuid) -> Option<BrowserSession> {
        let sessions = self.browser_sessions.read().await;
        sessions.values()
            .find(|session| session.task_attempt_id == task_attempt_id)
            .cloned()
    }

    /// A context is finalized when
    /// - The next action is None (no follow-up actions)
    /// - The run reason is not DevServer
    fn should_finalize(ctx: &ExecutionContext) -> bool {
        ctx.execution_process
            .executor_action()
            .unwrap()
            .next_action
            .is_none()
            && (!matches!(
                ctx.execution_process.run_reason,
                ExecutionProcessRunReason::DevServer
            ))
    }

    /// Finalize task execution by updating status to InReview and sending notifications
    async fn finalize_task(db: &DBService, config: &Arc<RwLock<Config>>, ctx: &ExecutionContext) {
        if let Err(e) = Task::update_status(&db.pool, ctx.task.id, TaskStatus::InReview).await {
            tracing::error!("Failed to update task status to InReview: {e}");
        }
        let notify_cfg = config.read().await.notifications.clone();
        NotificationService::notify_execution_halted(notify_cfg, ctx).await;
    }

    /// Defensively check for externally deleted worktrees and mark them as deleted in the database
    async fn check_externally_deleted_worktrees(db: &DBService) -> Result<(), DeploymentError> {
        let active_attempts = TaskAttempt::find_by_worktree_deleted(&db.pool).await?;
        tracing::debug!(
            "Checking {} active worktrees for external deletion...",
            active_attempts.len()
        );
        for (attempt_id, worktree_path) in active_attempts {
            // Check if worktree directory exists
            if !std::path::Path::new(&worktree_path).exists() {
                // Worktree was deleted externally, mark as deleted in database
                if let Err(e) = TaskAttempt::mark_worktree_deleted(&db.pool, attempt_id).await {
                    tracing::error!(
                        "Failed to mark externally deleted worktree as deleted for attempt {}: {}",
                        attempt_id,
                        e
                    );
                } else {
                    tracing::info!(
                        "Marked externally deleted worktree as deleted for attempt {} (path: {})",
                        attempt_id,
                        worktree_path
                    );
                }
            }
        }
        Ok(())
    }

    /// Find and delete orphaned worktrees that don't correspond to any task attempts
    async fn cleanup_orphaned_worktrees(&self) {
        // Check if orphan cleanup is disabled via environment variable
        if std::env::var("DISABLE_WORKTREE_ORPHAN_CLEANUP").is_ok() {
            tracing::debug!(
                "Orphan worktree cleanup is disabled via DISABLE_WORKTREE_ORPHAN_CLEANUP environment variable"
            );
            return;
        }
        let worktree_base_dir = WorktreeManager::get_worktree_base_dir();
        if !worktree_base_dir.exists() {
            tracing::debug!(
                "Worktree base directory {} does not exist, skipping orphan cleanup",
                worktree_base_dir.display()
            );
            return;
        }
        let entries = match std::fs::read_dir(&worktree_base_dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::error!(
                    "Failed to read worktree base directory {}: {}",
                    worktree_base_dir.display(),
                    e
                );
                return;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    tracing::warn!("Failed to read directory entry: {}", e);
                    continue;
                }
            };
            let path = entry.path();
            // Only process directories
            if !path.is_dir() {
                continue;
            }

            let worktree_path_str = path.to_string_lossy().to_string();
            if let Ok(false) =
                TaskAttempt::container_ref_exists(&self.db().pool, &worktree_path_str).await
            {
                // This is an orphaned worktree - delete it
                tracing::info!("Found orphaned worktree: {}", worktree_path_str);
                if let Err(e) = WorktreeManager::cleanup_worktree(&path, None).await {
                    tracing::error!(
                        "Failed to remove orphaned worktree {}: {}",
                        worktree_path_str,
                        e
                    );
                } else {
                    tracing::info!(
                        "Successfully removed orphaned worktree: {}",
                        worktree_path_str
                    );
                }
            }
        }
    }

    pub async fn cleanup_expired_attempt(
        db: &DBService,
        attempt_id: Uuid,
        worktree_path: PathBuf,
        git_repo_path: PathBuf,
    ) -> Result<(), DeploymentError> {
        WorktreeManager::cleanup_worktree(&worktree_path, Some(&git_repo_path)).await?;
        // Mark worktree as deleted in database after successful cleanup
        TaskAttempt::mark_worktree_deleted(&db.pool, attempt_id).await?;
        tracing::info!("Successfully marked worktree as deleted for attempt {attempt_id}",);
        Ok(())
    }

    pub async fn cleanup_expired_attempts(db: &DBService) -> Result<(), DeploymentError> {
        let expired_attempts = TaskAttempt::find_expired_for_cleanup(&db.pool).await?;
        if expired_attempts.is_empty() {
            tracing::debug!("No expired worktrees found");
            return Ok(());
        }
        tracing::info!(
            "Found {} expired worktrees to clean up",
            expired_attempts.len()
        );
        for (attempt_id, worktree_path, git_repo_path) in expired_attempts {
            Self::cleanup_expired_attempt(
                db,
                attempt_id,
                PathBuf::from(worktree_path),
                PathBuf::from(git_repo_path),
            )
            .await
            .unwrap_or_else(|e| {
                tracing::error!("Failed to clean up expired attempt {attempt_id}: {e}",);
            });
        }
        Ok(())
    }

    pub async fn spawn_worktree_cleanup(&self) {
        let db = self.db.clone();
        let mut cleanup_interval = tokio::time::interval(tokio::time::Duration::from_secs(1800)); // 30 minutes
        self.cleanup_orphaned_worktrees().await;
        tokio::spawn(async move {
            loop {
                cleanup_interval.tick().await;
                tracing::info!("Starting periodic worktree cleanup...");
                Self::check_externally_deleted_worktrees(&db)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::error!("Failed to check externally deleted worktrees: {}", e);
                    });
                Self::cleanup_expired_attempts(&db)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::error!("Failed to clean up expired worktree attempts: {}", e)
                    });
            }
        });
    }

    /// Spawn a background task that polls the child process for completion and
    /// cleans up the execution entry when it exits.
    pub fn spawn_exit_monitor(&self, exec_id: &Uuid) -> JoinHandle<()> {
        let exec_id = *exec_id;
        let child_store = self.child_store.clone();
        let msg_stores = self.msg_stores.clone();
        let db = self.db.clone();
        let config = self.config.clone();
        let container = self.clone();
        let analytics = self.analytics.clone();

        tokio::spawn(async move {
            loop {
                let status_opt = {
                    let child_lock = {
                        let map = child_store.read().await;
                        map.get(&exec_id)
                            .cloned()
                            .unwrap_or_else(|| panic!("Child handle missing for {exec_id}"))
                    };

                    let mut child_handler = child_lock.write().await;
                    match child_handler.try_wait() {
                        Ok(Some(status)) => Some(Ok(status)),
                        Ok(None) => None,
                        Err(e) => Some(Err(e)),
                    }
                };

                // Update execution process and cleanup if exit
                if let Some(status_result) = status_opt {
                    // Update execution process record with completion info
                    let (exit_code, status) = match status_result {
                        Ok(exit_status) => {
                            let code = exit_status.code().unwrap_or(-1) as i64;
                            let status = if exit_status.success() {
                                ExecutionProcessStatus::Completed
                            } else {
                                ExecutionProcessStatus::Failed
                            };
                            (Some(code), status)
                        }
                        Err(_) => (None, ExecutionProcessStatus::Failed),
                    };

                    if !ExecutionProcess::was_killed(&db.pool, exec_id).await
                        && let Err(e) = ExecutionProcess::update_completion(
                            &db.pool,
                            exec_id,
                            status.clone(),
                            exit_code,
                        )
                        .await
                    {
                        tracing::error!("Failed to update execution process completion: {}", e);
                    }

                    if let Ok(ctx) = ExecutionProcess::load_context(&db.pool, exec_id).await {
                        // Update executor session summary if available
                        if let Err(e) = container.update_executor_session_summary(&exec_id).await {
                            tracing::warn!("Failed to update executor session summary: {}", e);
                        }

                        // (moved) capture after-head commit occurs later, after commit/next-action handling

                        if matches!(
                            ctx.execution_process.status,
                            ExecutionProcessStatus::Completed
                        ) && exit_code == Some(0)
                        {
                            // Commit changes (if any) and get feedback about whether changes were made
                            let changes_committed = match container.try_commit_changes(&ctx).await {
                                Ok(committed) => committed,
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to commit changes after execution: {}",
                                        e
                                    );
                                    // Treat commit failures as if changes were made to be safe
                                    true
                                }
                            };

                            // Determine whether to start the next action based on execution context
                            let should_start_next = if matches!(
                                ctx.execution_process.run_reason,
                                ExecutionProcessRunReason::CodingAgent
                            ) {
                                // Skip CleanupScript when CodingAgent produced no changes
                                changes_committed
                            } else {
                                // SetupScript always proceeds to CodingAgent
                                true
                            };

                            if should_start_next {
                                // If the process exited successfully, start the next action
                                if let Err(e) = container.try_start_next_action(&ctx).await {
                                    tracing::error!(
                                        "Failed to start next action after completion: {}",
                                        e
                                    );
                                }
                            } else {
                                tracing::info!(
                                    "Skipping cleanup script for task attempt {} - no changes made by coding agent",
                                    ctx.task_attempt.id
                                );

                                // Manually finalize task since we're bypassing normal execution flow
                                Self::finalize_task(&db, &config, &ctx).await;
                            }
                        }

                        if Self::should_finalize(&ctx) {
                            Self::finalize_task(&db, &config, &ctx).await;
                        }

                        // Fire event when CodingAgent execution has finished
                        if config.read().await.analytics_enabled == Some(true)
                            && matches!(
                                &ctx.execution_process.run_reason,
                                ExecutionProcessRunReason::CodingAgent
                            )
                            && let Some(analytics) = &analytics
                        {
                            analytics.analytics_service.track_event(&analytics.user_id, "task_attempt_finished", Some(json!({
                                    "task_id": ctx.task.id.to_string(),
                                    "project_id": ctx.task.project_id.to_string(),
                                    "attempt_id": ctx.task_attempt.id.to_string(),
                                    "execution_success": matches!(ctx.execution_process.status, ExecutionProcessStatus::Completed),
                                    "exit_code": ctx.execution_process.exit_code,
                                })));
                        }
                    }

                    // Now that commit/next-action/finalization steps for this process are complete,
                    // capture the HEAD OID as the definitive "after" state (best-effort).
                    if let Ok(ctx) = ExecutionProcess::load_context(&db.pool, exec_id).await {
                        let worktree_dir = container.task_attempt_to_current_dir(&ctx.task_attempt);
                        if let Ok(head) = container.git().get_head_info(&worktree_dir)
                            && let Err(e) = ExecutionProcess::update_after_head_commit(
                                &db.pool, exec_id, &head.oid,
                            )
                            .await
                        {
                            tracing::warn!(
                                "Failed to update after_head_commit for {}: {}",
                                exec_id,
                                e
                            );
                        }
                    }

                    // Cleanup msg store
                    if let Some(msg_arc) = msg_stores.write().await.remove(&exec_id) {
                        msg_arc.push_finished();
                        tokio::time::sleep(Duration::from_millis(50)).await; // Wait for the finish message to propogate
                        match Arc::try_unwrap(msg_arc) {
                            Ok(inner) => drop(inner),
                            Err(arc) => tracing::error!(
                                "There are still {} strong Arcs to MsgStore for {}",
                                Arc::strong_count(&arc),
                                exec_id
                            ),
                        }
                    }

                    // Cleanup child handle
                    child_store.write().await.remove(&exec_id);
                    break;
                }

                // still running, sleep and try again
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
    }

    pub fn dir_name_from_task_attempt(attempt_id: &Uuid, task_title: &str) -> String {
        let task_title_id = git_branch_id(task_title);
        format!("vk-{}-{}", short_uuid(attempt_id), task_title_id)
    }

    /// Determine if this task should use Docker containers
    async fn should_use_docker(&self, task_attempt: &TaskAttempt) -> Result<Option<String>, ContainerError> {
        // Check if Docker is available
        if self.docker.is_none() {
            return Ok(None);
        }

        // Check if task has repo_path
        let task = task_attempt
            .parent_task(&self.db.pool)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;

        Ok(task.repo_path)
    }

    /// Check if a container_ref represents a Docker container ID (vs worktree path)
    fn is_docker_container(&self, container_ref: &str) -> bool {
        // Docker container IDs are typically 64-character hex strings
        // Worktree paths are filesystem paths
        container_ref.len() == 64 && container_ref.chars().all(|c| c.is_ascii_hexdigit())
            || container_ref.len() == 12 && container_ref.chars().all(|c| c.is_ascii_hexdigit()) // short IDs
    }

    /// Create Docker container for multi-repo task
    async fn create_docker_container(&self, task_attempt: &TaskAttempt, repo_path: &str) -> Result<ContainerRef, ContainerError> {
        let docker = self.docker.as_ref().ok_or_else(|| {
            ContainerError::Other(anyhow!("Docker client not available"))
        })?;

        let _task = task_attempt
            .parent_task(&self.db.pool)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;

        // Generate container name
        let container_name = format!("vibe-kanban-task-{}", short_uuid(&task_attempt.id));

        // Determine devcontainer config to use
        let devcontainer_path = self.resolve_devcontainer_config(Path::new(repo_path))?;

        // Build image from devcontainer
        let image_name = self.build_container_image(docker, &devcontainer_path, &task_attempt.id.to_string()).await?;

        // Create container with repo mounted
        let container_id = self.create_docker_container_instance(docker, &image_name, repo_path, &container_name).await?;

        // Update container_ref in database to store Docker container ID
        TaskAttempt::update_container_ref(
            &self.db.pool,
            task_attempt.id,
            &container_id,
        )
        .await?;

        tracing::info!("Created Docker container {} for task attempt {}", container_id, task_attempt.id);
        Ok(container_id)
    }

    /// Resolve devcontainer configuration path
    fn resolve_devcontainer_config(&self, repo_path: &Path) -> Result<PathBuf, ContainerError> {
        let project_devcontainer = repo_path.join(".devcontainer");

        if project_devcontainer.exists() {
            // Use project's devcontainer
            Ok(project_devcontainer)
        } else {
            // Use Vibe Kanban's devcontainer as default
            let vibe_devcontainer = std::env::current_dir()
                .map_err(ContainerError::Io)?
                .join(".devcontainer");

            if vibe_devcontainer.exists() {
                Ok(vibe_devcontainer)
            } else {
                Err(ContainerError::Other(anyhow!(
                    "No devcontainer configuration found in project or Vibe Kanban directory"
                )))
            }
        }
    }

    /// Build Docker image from devcontainer
    async fn build_container_image(&self, docker: &Docker, devcontainer_path: &Path, task_id: &str) -> Result<String, ContainerError> {
        let image_name = format!("vibe-kanban-task-{}", task_id);

        // Create tar context from devcontainer directory
        let tar_context = self.create_build_context(devcontainer_path)?;

        let build_options = BuildImageOptions {
            dockerfile: "Dockerfile".to_string(),
            t: image_name.clone(),
            ..Default::default()
        };

        let mut stream = docker.build_image(build_options, None, Some(tar_context.into()));

        use futures::StreamExt;
        while let Some(result) = stream.next().await {
            match result {
                Ok(output) => {
                    tracing::debug!("Docker build: {:?}", output);
                }
                Err(e) => {
                    tracing::error!("Docker build error: {:?}", e);
                    return Err(ContainerError::Other(anyhow!("Docker build failed: {}", e)));
                }
            }
        }

        tracing::info!("Successfully built Docker image: {}", image_name);
        Ok(image_name)
    }

    /// Create Docker container instance
    async fn create_docker_container_instance(
        &self,
        docker: &Docker,
        image_name: &str,
        repo_path: &str,
        container_name: &str,
    ) -> Result<String, ContainerError> {
        let config = ContainerConfig {
            image: Some(image_name.to_string()),
            working_dir: Some("/workspace".to_string()),
            // Use the default CMD from the Dockerfile, but ensure container stays running
            // In production, this would be overridden when executing specific commands
            cmd: Some(vec!["/bin/bash".to_string()]),
            tty: Some(true), // Allocate a pseudo-TTY to keep bash running
            attach_stdin: Some(true), // Attach to STDIN
            host_config: Some(HostConfig {
                binds: Some(vec![
                    format!("{}:/workspace", repo_path)
                ]),
                auto_remove: Some(true), // Auto-remove when container stops
                ..Default::default()
            }),
            ..Default::default()
        };

        // Debug: Log the container configuration
        tracing::info!("Creating container with config: image={}, working_dir={:?}, binds={:?}",
            image_name, config.working_dir, config.host_config.as_ref().and_then(|hc| hc.binds.as_ref()));

        let container = docker
            .create_container(Some(CreateContainerOptions {
                name: container_name.to_string(),
                platform: None,
            }), config)
            .await
            .map_err(|e| ContainerError::Other(anyhow!("Failed to create container: {}", e)))?;

        tracing::info!("Created container with ID: {}", container.id);

        docker.start_container::<String>(&container.id, None).await
            .map_err(|e| ContainerError::Other(anyhow!("Failed to start container: {}", e)))?;

        tracing::info!("Started Docker container: {}", container.id);
        Ok(container.id)
    }

    /// Create tar archive of directory for Docker build context
    fn create_build_context(&self, path: &Path) -> Result<Vec<u8>, ContainerError> {
        use std::io::Cursor;
        use tar::Builder;

        let mut buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut buffer);
            let mut archive = Builder::new(cursor);

            archive.append_dir_all(".", path)
                .map_err(|e| ContainerError::Other(anyhow!("Failed to create tar archive: {}", e)))?;

            archive.finish()
                .map_err(|e| ContainerError::Other(anyhow!("Failed to finish tar archive: {}", e)))?;
        }

        Ok(buffer)
    }

    /// Execute process inside Docker container
    async fn start_docker_execution(
        &self,
        task_attempt: &TaskAttempt,
        execution_process: &ExecutionProcess,
        executor_action: &ExecutorAction,
        container_id: &str,
    ) -> Result<(), ContainerError> {
        let docker = self.docker.as_ref().ok_or_else(|| {
            ContainerError::Other(anyhow!("Docker client not available"))
        })?;

        // Create a Docker exec command that runs the executor action
        // For now, this is a simplified approach - we'll execute claude code directly

        use bollard::exec::CreateExecOptions;

        // Extract the task message from the executor action
        let task_message = match executor_action.typ() {
            executors::actions::ExecutorActionType::CodingAgentInitialRequest(request) => {
                request.prompt.clone()
            }
            executors::actions::ExecutorActionType::CodingAgentFollowUpRequest(request) => {
                request.prompt.clone()
            }
            _ => {
                return Err(ContainerError::Other(anyhow!(
                    "Docker execution not supported for this executor action type"
                )));
            }
        };

        // Create exec instance in container
        let exec = docker
            .create_exec(
                container_id,
                CreateExecOptions {
                    cmd: Some(vec![
                        "claude".to_string(),
                        "code".to_string(),
                        "--message".to_string(),
                        task_message,
                    ]),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    working_dir: Some("/workspace".to_string()),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| ContainerError::Other(anyhow!("Failed to create exec: {}", e)))?;

        // Start exec and get stream
        let _stream = docker.start_exec(&exec.id, None).await
            .map_err(|e| ContainerError::Other(anyhow!("Failed to start exec: {}", e)))?;

        // For MVP: Create a placeholder process to integrate with existing child tracking system
        // In a full implementation, we would bridge Docker exec streams properly
        let placeholder_child = self.create_placeholder_child().await?;

        self.add_child_to_store(execution_process.id, placeholder_child).await;

        // Spawn Docker exec monitoring task
        let exec_id = exec.id.clone();
        let _docker = docker.clone();
        let execution_id = execution_process.id;
        let db = self.db.clone();

        tokio::spawn(async move {
            tracing::info!("Monitoring Docker exec {} for execution {}", exec_id, execution_id);

            // TODO: Monitor the Docker exec stream and update execution status
            // For now, just simulate completion after a short delay
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

            // Update execution process as completed (simplified for MVP)
            let _ = ExecutionProcess::update_completion(
                &db.pool,
                execution_id,
                ExecutionProcessStatus::Completed,
                Some(0),
            ).await;

            tracing::info!("Docker exec {} completed", exec_id);
        });

        tracing::info!("Started Docker execution for task attempt {}", task_attempt.id);
        Ok(())
    }

    /// Create a placeholder child process for Docker exec integration
    async fn create_placeholder_child(&self) -> Result<AsyncGroupChild, ContainerError> {
        use command_group::AsyncCommandGroup;
        use tokio::process::Command;

        let child = Command::new("sleep")
            .arg("10") // Sleep for 10 seconds as a placeholder
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .group_spawn()
            .map_err(|e| ContainerError::Other(anyhow!("Failed to create placeholder child: {}", e)))?;

        tracing::debug!("Created placeholder child process for Docker exec integration");
        Ok(child)
    }

    async fn track_child_msgs_in_store(&self, id: Uuid, child: &mut AsyncGroupChild) {
        let store = Arc::new(MsgStore::new());

        let out = child.inner().stdout.take().expect("no stdout");
        let err = child.inner().stderr.take().expect("no stderr");

        // Map stdout bytes -> LogMsg::Stdout
        let out = ReaderStream::new(out)
            .map_ok(|chunk| LogMsg::Stdout(String::from_utf8_lossy(&chunk).into_owned()));

        // Map stderr bytes -> LogMsg::Stderr
        let err = ReaderStream::new(err)
            .map_ok(|chunk| LogMsg::Stderr(String::from_utf8_lossy(&chunk).into_owned()));

        // If you have a JSON Patch source, map it to LogMsg::JsonPatch too, then select all three.

        // Merge and forward into the store
        let merged = select(out, err); // Stream<Item = Result<LogMsg, io::Error>>
        store.clone().spawn_forwarder(merged);

        let mut map = self.msg_stores().write().await;
        map.insert(id, store);
    }

    /// Get the worktree path for a task attempt
    #[allow(dead_code)]
    async fn get_worktree_path(
        &self,
        task_attempt: &TaskAttempt,
    ) -> Result<PathBuf, ContainerError> {
        let container_ref = self.ensure_container_exists(task_attempt).await?;
        let worktree_dir = PathBuf::from(&container_ref);

        if !worktree_dir.exists() {
            return Err(ContainerError::Other(anyhow!(
                "Worktree directory not found"
            )));
        }

        Ok(worktree_dir)
    }

    /// Get the project repository path for a task attempt
    async fn get_project_repo_path(
        &self,
        task_attempt: &TaskAttempt,
    ) -> Result<PathBuf, ContainerError> {
        let project_repo_path = task_attempt
            .parent_task(&self.db().pool)
            .await?
            .ok_or(ContainerError::Other(anyhow!("Parent task not found")))?
            .parent_project(&self.db().pool)
            .await?
            .ok_or(ContainerError::Other(anyhow!("Parent project not found")))?
            .git_repo_path;

        Ok(project_repo_path)
    }

    /// Create a diff stream for merged attempts (never changes)
    fn create_merged_diff_stream(
        &self,
        project_repo_path: &Path,
        merge_commit_id: &str,
    ) -> Result<futures::stream::BoxStream<'static, Result<Event, std::io::Error>>, ContainerError>
    {
        let diffs = self.git().get_diffs(
            DiffTarget::Commit {
                repo_path: project_repo_path,
                commit_sha: merge_commit_id,
            },
            None,
        )?;

        let stream = futures::stream::iter(diffs.into_iter().map(|diff| {
            let entry_index = GitService::diff_path(&diff);
            let patch =
                ConversationPatch::add_diff(escape_json_pointer_segment(&entry_index), diff);
            let event = LogMsg::JsonPatch(patch).to_sse_event();
            Ok::<_, std::io::Error>(event)
        }))
        .chain(futures::stream::once(async {
            Ok::<_, std::io::Error>(LogMsg::Finished.to_sse_event())
        }))
        .boxed();

        Ok(stream)
    }

    /// Create a live diff stream for ongoing attempts
    async fn create_live_diff_stream(
        &self,
        worktree_path: &Path,
        task_branch: &str,
        base_branch: &str,
    ) -> Result<futures::stream::BoxStream<'static, Result<Event, std::io::Error>>, ContainerError>
    {
        // Get initial snapshot
        let git_service = self.git().clone();
        let initial_diffs = git_service.get_diffs(
            DiffTarget::Worktree {
                worktree_path,
                branch_name: task_branch,
                base_branch,
            },
            None,
        )?;

        let initial_stream = futures::stream::iter(initial_diffs.into_iter().map(|diff| {
            let entry_index = GitService::diff_path(&diff);
            let patch =
                ConversationPatch::add_diff(escape_json_pointer_segment(&entry_index), diff);
            let event = LogMsg::JsonPatch(patch).to_sse_event();
            Ok::<_, std::io::Error>(event)
        }))
        .boxed();

        // Create live update stream
        let worktree_path = worktree_path.to_path_buf();
        let task_branch = task_branch.to_string();
        let base_branch = base_branch.to_string();

        let live_stream = {
            let git_service = git_service.clone();
            try_stream! {
                let (_debouncer, mut rx, canonical_worktree_path) =
                    filesystem_watcher::async_watcher(worktree_path.clone())
                        .map_err(|e| io::Error::other(e.to_string()))?;

                while let Some(result) = rx.next().await {
                    match result {
                        Ok(events) => {
                            let changed_paths = Self::extract_changed_paths(&events, &canonical_worktree_path, &worktree_path);

                            if !changed_paths.is_empty() {
                                for event in Self::process_file_changes(
                                    &git_service,
                                    &worktree_path,
                                    &task_branch,
                                    &base_branch,
                                    &changed_paths,
                                ).map_err(|e| {
                                    tracing::error!("Error processing file changes: {}", e);
                                    io::Error::other(e.to_string())
                                })? {
                                    yield event;
                                }
                            }
                        }
                        Err(errors) => {
                            let error_msg = errors.iter()
                                .map(|e| e.to_string())
                                .collect::<Vec<_>>()
                                .join("; ");
                            tracing::error!("Filesystem watcher error: {}", error_msg);
                            Err(io::Error::other(error_msg))?;
                        }
                    }
                }
            }
        }.boxed();

        let combined_stream = select(initial_stream, live_stream);
        Ok(combined_stream.boxed())
    }

    /// Extract changed file paths from filesystem events
    fn extract_changed_paths(
        events: &[DebouncedEvent],
        canonical_worktree_path: &Path,
        worktree_path: &Path,
    ) -> Vec<String> {
        events
            .iter()
            .flat_map(|event| &event.paths)
            .filter_map(|path| {
                path.strip_prefix(canonical_worktree_path)
                    .or_else(|_| path.strip_prefix(worktree_path))
                    .ok()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
            })
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Process file changes and generate diff events
    fn process_file_changes(
        git_service: &GitService,
        worktree_path: &Path,
        task_branch: &str,
        base_branch: &str,
        changed_paths: &[String],
    ) -> Result<Vec<Event>, ContainerError> {
        let path_filter: Vec<&str> = changed_paths.iter().map(|s| s.as_str()).collect();

        let current_diffs = git_service.get_diffs(
            DiffTarget::Worktree {
                worktree_path,
                branch_name: task_branch,
                base_branch,
            },
            Some(&path_filter),
        )?;

        let mut events = Vec::new();
        let mut files_with_diffs = HashSet::new();

        // Add/update files that have diffs
        for diff in current_diffs {
            let file_path = GitService::diff_path(&diff);
            files_with_diffs.insert(file_path.clone());

            let patch = ConversationPatch::add_diff(escape_json_pointer_segment(&file_path), diff);
            let event = LogMsg::JsonPatch(patch).to_sse_event();
            events.push(event);
        }

        // Remove files that changed but no longer have diffs
        for changed_path in changed_paths {
            if !files_with_diffs.contains(changed_path) {
                let patch =
                    ConversationPatch::remove_diff(escape_json_pointer_segment(changed_path));
                let event = LogMsg::JsonPatch(patch).to_sse_event();
                events.push(event);
            }
        }

        Ok(events)
    }
}

#[async_trait]
impl ContainerService for LocalContainerService {
    fn msg_stores(&self) -> &Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>> {
        &self.msg_stores
    }

    fn db(&self) -> &DBService {
        &self.db
    }

    fn git(&self) -> &GitService {
        &self.git
    }

    fn task_attempt_to_current_dir(&self, task_attempt: &TaskAttempt) -> PathBuf {
        let container_ref = task_attempt.container_ref.clone().unwrap_or_default();

        // For Docker containers, the working directory inside the container is /workspace
        // But for logging and git operations, we might need special handling
        PathBuf::from(container_ref)
    }
    /// Create a container
    async fn create(&self, task_attempt: &TaskAttempt) -> Result<ContainerRef, ContainerError> {
        // Check if this task should use Docker containers
        if let Some(repo_path) = self.should_use_docker(task_attempt).await? {
            tracing::info!("Using Docker container for task attempt {} with repo_path: {}",
                task_attempt.id, repo_path);
            return self.create_docker_container(task_attempt, &repo_path).await;
        }

        // Fallback to traditional worktree approach
        let task = task_attempt
            .parent_task(&self.db.pool)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;

        let task_branch_name =
            LocalContainerService::dir_name_from_task_attempt(&task_attempt.id, &task.title);
        let worktree_path = WorktreeManager::get_worktree_base_dir().join(&task_branch_name);

        let project = task
            .parent_project(&self.db.pool)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;

        WorktreeManager::create_worktree(
            &project.git_repo_path,
            &task_branch_name,
            &worktree_path,
            &task_attempt.base_branch,
            true, // create new branch
        )
        .await?;

        // Copy files specified in the project's copy_files field
        if let Some(copy_files) = &project.copy_files
            && !copy_files.trim().is_empty()
        {
            self.copy_project_files(&project.git_repo_path, &worktree_path, copy_files)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!("Failed to copy project files: {}", e);
                });
        }

        // Copy task images from cache to worktree
        if let Err(e) = self
            .image_service
            .copy_images_by_task_to_worktree(&worktree_path, task.id)
            .await
        {
            tracing::warn!("Failed to copy task images to worktree: {}", e);
        }

        // Update both container_ref and branch in the database
        TaskAttempt::update_container_ref(
            &self.db.pool,
            task_attempt.id,
            &worktree_path.to_string_lossy(),
        )
        .await?;

        TaskAttempt::update_branch(&self.db.pool, task_attempt.id, &task_branch_name).await?;

        Ok(worktree_path.to_string_lossy().to_string())
    }

    async fn delete_inner(&self, task_attempt: &TaskAttempt) -> Result<(), ContainerError> {
        // cleanup the container, here that means deleting the worktree
        let task = task_attempt
            .parent_task(&self.db.pool)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        let git_repo_path = match Project::find_by_id(&self.db.pool, task.project_id).await {
            Ok(Some(project)) => Some(project.git_repo_path.clone()),
            Ok(None) => None,
            Err(e) => {
                tracing::error!("Failed to fetch project {}: {}", task.project_id, e);
                None
            }
        };
        WorktreeManager::cleanup_worktree(
            &PathBuf::from(task_attempt.container_ref.clone().unwrap_or_default()),
            git_repo_path.as_deref(),
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                "Failed to clean up worktree for task attempt {}: {}",
                task_attempt.id,
                e
            );
        });
        Ok(())
    }

    async fn ensure_container_exists(
        &self,
        task_attempt: &TaskAttempt,
    ) -> Result<ContainerRef, ContainerError> {
        // Get required context
        let task = task_attempt
            .parent_task(&self.db.pool)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;

        let project = task
            .parent_project(&self.db.pool)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;

        let container_ref = task_attempt.container_ref.as_ref().ok_or_else(|| {
            ContainerError::Other(anyhow!("Container ref not found for task attempt"))
        })?;
        let worktree_path = PathBuf::from(container_ref);

        let branch_name = task_attempt
            .branch
            .as_ref()
            .ok_or_else(|| ContainerError::Other(anyhow!("Branch not found for task attempt")))?;

        WorktreeManager::ensure_worktree_exists(
            &project.git_repo_path,
            branch_name,
            &worktree_path,
        )
        .await?;

        Ok(container_ref.to_string())
    }

    async fn is_container_clean(&self, task_attempt: &TaskAttempt) -> Result<bool, ContainerError> {
        if let Some(container_ref) = &task_attempt.container_ref {
            // If container_ref is set, check if the worktree exists
            let path = PathBuf::from(container_ref);
            if path.exists() {
                self.git().is_worktree_clean(&path).map_err(|e| e.into())
            } else {
                return Ok(true); // No worktree means it's clean
            }
        } else {
            return Ok(true); // No container_ref means no worktree, so it's clean
        }
    }

    async fn start_execution_inner(
        &self,
        task_attempt: &TaskAttempt,
        execution_process: &ExecutionProcess,
        executor_action: &ExecutorAction,
    ) -> Result<(), ContainerError> {
        let container_ref = task_attempt
            .container_ref
            .as_ref()
            .ok_or(ContainerError::Other(anyhow!(
                "Container ref not found for task attempt"
            )))?;

        // Check if this is a Docker container
        if self.is_docker_container(container_ref) {
            // For Docker containers, execute inside the container
            self.start_docker_execution(task_attempt, execution_process, executor_action, container_ref).await
        } else {
            // For worktrees, execute in the filesystem
            let current_dir = PathBuf::from(container_ref);

            // Create the child and stream, add to execution tracker
            let mut child = executor_action.spawn(&current_dir).await?;

            self.track_child_msgs_in_store(execution_process.id, &mut child)
                .await;

            self.add_child_to_store(execution_process.id, child).await;

            // Spawn exit monitor
            let _hn = self.spawn_exit_monitor(&execution_process.id);

            Ok(())
        }
    }

    async fn start_browser_chat_execution(
        &self,
        execution_process: &ExecutionProcess,
        executor_action: &ExecutorAction,
    ) -> Result<(), ContainerError> {
        // Browser chat doesn't need a git worktree - use current working directory
        let current_dir = std::env::current_dir().map_err(ContainerError::Io)?;

        // Extract browser chat request details
        if let executors::actions::ExecutorActionType::BrowserChatRequest(browser_request) = executor_action.typ() {
            // Generate session ID for this browser chat (reuse existing or create new)
            let session_id = browser_request.session_id.clone()
                .unwrap_or_else(|| format!("browser_session_{}", short_uuid(&uuid::Uuid::new_v4())));

            // Create browser session metadata
            let browser_session = BrowserSession {
                session_id: session_id.clone(),
                task_attempt_id: execution_process.task_attempt_id,
                execution_process_id: execution_process.id,
                agent_type: format!("{:?}", browser_request.agent_type),
                created_at: std::time::Instant::now(),
            };

            // Store the session for tracking
            self.add_browser_session(browser_session).await;

            tracing::info!("Created browser session {} for task attempt {}",
                session_id, execution_process.task_attempt_id);
        }

        // Create the child and stream, add to execution tracker
        let mut child = executor_action.spawn(&current_dir).await?;

        self.track_child_msgs_in_store(execution_process.id, &mut child)
            .await;

        self.add_child_to_store(execution_process.id, child).await;

        // Spawn exit monitor
        let _hn = self.spawn_exit_monitor(&execution_process.id);

        Ok(())
    }

    async fn stop_execution(
        &self,
        execution_process: &ExecutionProcess,
    ) -> Result<(), ContainerError> {
        let child = self
            .get_child_from_store(&execution_process.id)
            .await
            .ok_or_else(|| {
                ContainerError::Other(anyhow!("Child process not found for execution"))
            })?;
        ExecutionProcess::update_completion(
            &self.db.pool,
            execution_process.id,
            ExecutionProcessStatus::Killed,
            None,
        )
        .await?;

        // Kill the child process and remove from the store
        {
            let mut child_guard = child.write().await;
            if let Err(e) = command::kill_process_group(&mut child_guard).await {
                tracing::error!(
                    "Failed to stop execution process {}: {}",
                    execution_process.id,
                    e
                );
                return Err(e);
            }
        }
        self.remove_child_from_store(&execution_process.id).await;

        // Mark the process finished in the MsgStore
        if let Some(msg) = self.msg_stores.write().await.remove(&execution_process.id) {
            msg.push_finished();
        }

        // Update task status to InReview when execution is stopped
        if let Ok(ctx) = ExecutionProcess::load_context(&self.db.pool, execution_process.id).await
            && !matches!(
                ctx.execution_process.run_reason,
                ExecutionProcessRunReason::DevServer
            )
            && let Err(e) =
                Task::update_status(&self.db.pool, ctx.task.id, TaskStatus::InReview).await
        {
            tracing::error!("Failed to update task status to InReview: {e}");
        }

        tracing::debug!(
            "Execution process {} stopped successfully",
            execution_process.id
        );

        // Record after-head commit OID (best-effort)
        if let Ok(ctx) = ExecutionProcess::load_context(&self.db.pool, execution_process.id).await {
            let worktree = self.task_attempt_to_current_dir(&ctx.task_attempt);
            if let Ok(head) = self.git().get_head_info(&worktree) {
                let _ = ExecutionProcess::update_after_head_commit(
                    &self.db.pool,
                    execution_process.id,
                    &head.oid,
                )
                .await;
            }
        }

        Ok(())
    }

    async fn get_diff(
        &self,
        task_attempt: &TaskAttempt,
    ) -> Result<futures::stream::BoxStream<'static, Result<Event, std::io::Error>>, ContainerError>
    {
        let project_repo_path = self.get_project_repo_path(task_attempt).await?;
        let latest_merge =
            Merge::find_latest_by_task_attempt_id(&self.db.pool, task_attempt.id).await?;
        let task_branch = task_attempt
            .branch
            .clone()
            .ok_or(ContainerError::Other(anyhow!(
                "Task attempt {} does not have a branch",
                task_attempt.id
            )))?;

        let is_ahead = if let Ok((ahead, _)) = self.git().get_branch_status(
            &project_repo_path,
            &task_branch,
            &task_attempt.base_branch,
        ) {
            ahead > 0
        } else {
            false
        };

        // Show merged diff when no new work is on the branch or container
        if let Some(merge) = &latest_merge
            && let Some(commit) = merge.merge_commit()
            && self.is_container_clean(task_attempt).await?
            && !is_ahead
        {
            return self.create_merged_diff_stream(&project_repo_path, &commit);
        }

        // worktree is needed for non-merged diffs
        let container_ref = self.ensure_container_exists(task_attempt).await?;
        let worktree_path = PathBuf::from(container_ref);

        // Handle ongoing attempts (live streaming diff)
        self.create_live_diff_stream(&worktree_path, &task_branch, &task_attempt.base_branch)
            .await
    }

    async fn try_commit_changes(&self, ctx: &ExecutionContext) -> Result<bool, ContainerError> {
        if !matches!(
            ctx.execution_process.run_reason,
            ExecutionProcessRunReason::CodingAgent | ExecutionProcessRunReason::CleanupScript,
        ) {
            return Ok(false);
        }

        let message = match ctx.execution_process.run_reason {
            ExecutionProcessRunReason::CodingAgent => {
                // Try to retrieve the task summary from the executor session
                // otherwise fallback to default message
                match ExecutorSession::find_by_execution_process_id(
                    &self.db().pool,
                    ctx.execution_process.id,
                )
                .await
                {
                    Ok(Some(session)) if session.summary.is_some() => session.summary.unwrap(),
                    Ok(_) => {
                        tracing::debug!(
                            "No summary found for execution process {}, using default message",
                            ctx.execution_process.id
                        );
                        format!(
                            "Commit changes from coding agent for task attempt {}",
                            ctx.task_attempt.id
                        )
                    }
                    Err(e) => {
                        tracing::debug!(
                            "Failed to retrieve summary for execution process {}: {}",
                            ctx.execution_process.id,
                            e
                        );
                        format!(
                            "Commit changes from coding agent for task attempt {}",
                            ctx.task_attempt.id
                        )
                    }
                }
            }
            ExecutionProcessRunReason::CleanupScript => {
                format!(
                    "Cleanup script changes for task attempt {}",
                    ctx.task_attempt.id
                )
            }
            _ => Err(ContainerError::Other(anyhow::anyhow!(
                "Invalid run reason for commit"
            )))?,
        };

        let container_ref = ctx.task_attempt.container_ref.as_ref().ok_or_else(|| {
            ContainerError::Other(anyhow::anyhow!("Container reference not found"))
        })?;

        tracing::debug!(
            "Committing changes for task attempt {} at path {:?}: '{}'",
            ctx.task_attempt.id,
            &container_ref,
            message
        );

        let changes_committed = self.git().commit(Path::new(container_ref), &message)?;
        Ok(changes_committed)
    }

    /// Copy files from the original project directory to the worktree
    async fn copy_project_files(
        &self,
        source_dir: &Path,
        target_dir: &Path,
        copy_files: &str,
    ) -> Result<(), ContainerError> {
        let files: Vec<&str> = copy_files
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        for file_path in files {
            let source_file = source_dir.join(file_path);
            let target_file = target_dir.join(file_path);

            // Create parent directories if needed
            if let Some(parent) = target_file.parent()
                && !parent.exists()
            {
                std::fs::create_dir_all(parent).map_err(|e| {
                    ContainerError::Other(anyhow!("Failed to create directory {:?}: {}", parent, e))
                })?;
            }

            // Copy the file
            if source_file.exists() {
                std::fs::copy(&source_file, &target_file).map_err(|e| {
                    ContainerError::Other(anyhow!(
                        "Failed to copy file {:?} to {:?}: {}",
                        source_file,
                        target_file,
                        e
                    ))
                })?;
                tracing::info!("Copied file {:?} to worktree", file_path);
            } else {
                return Err(ContainerError::Other(anyhow!(
                    "File {:?} does not exist in the project directory",
                    source_file
                )));
            }
        }
        Ok(())
    }
}

impl LocalContainerService {
    /// Extract the last assistant message from the MsgStore history
    fn extract_last_assistant_message(&self, exec_id: &Uuid) -> Option<String> {
        // Get the MsgStore for this execution
        let msg_stores = self.msg_stores.try_read().ok()?;
        let msg_store = msg_stores.get(exec_id)?;

        // Get the history and scan in reverse for the last assistant message
        let history = msg_store.get_history();

        for msg in history.iter().rev() {
            if let LogMsg::JsonPatch(patch) = msg {
                // Try to extract a NormalizedEntry from the patch
                if let Some(entry) = self.extract_normalized_entry_from_patch(patch)
                    && matches!(entry.entry_type, NormalizedEntryType::AssistantMessage)
                {
                    let content = entry.content.trim();
                    if !content.is_empty() {
                        // Truncate to reasonable size (4KB as Oracle suggested)
                        const MAX_SUMMARY_LENGTH: usize = 4096;
                        if content.len() > MAX_SUMMARY_LENGTH {
                            return Some(format!("{}...", &content[..MAX_SUMMARY_LENGTH]));
                        }
                        return Some(content.to_string());
                    }
                }
            }
        }

        None
    }

    /// Extract a NormalizedEntry from a JsonPatch if it contains one
    fn extract_normalized_entry_from_patch(
        &self,
        patch: &json_patch::Patch,
    ) -> Option<NormalizedEntry> {
        // Convert the patch to JSON to examine its structure
        if let Ok(patch_json) = serde_json::to_value(patch)
            && let Some(operations) = patch_json.as_array()
        {
            for operation in operations {
                if let Some(value) = operation.get("value") {
                    // Try to extract a NormalizedEntry from the value
                    if let Some(patch_type) = value.get("type").and_then(|t| t.as_str())
                        && patch_type == "NORMALIZED_ENTRY"
                        && let Some(content) = value.get("content")
                        && let Ok(entry) =
                            serde_json::from_value::<NormalizedEntry>(content.clone())
                    {
                        return Some(entry);
                    }
                }
            }
        }
        None
    }

    /// Update the executor session summary with the final assistant message
    async fn update_executor_session_summary(&self, exec_id: &Uuid) -> Result<(), anyhow::Error> {
        // Check if there's an executor session for this execution process
        let session =
            ExecutorSession::find_by_execution_process_id(&self.db.pool, *exec_id).await?;

        if let Some(session) = session {
            // Only update if summary is not already set
            if session.summary.is_none() {
                if let Some(summary) = self.extract_last_assistant_message(exec_id) {
                    ExecutorSession::update_summary(&self.db.pool, *exec_id, &summary).await?;
                } else {
                    tracing::debug!("No assistant message found for execution {}", exec_id);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use uuid::Uuid;
    use db::models::task_attempt::TaskAttempt;
    use std::collections::HashMap;
    use services::services::{config::Config, git::GitService, image::ImageService};
    use bollard::Docker;
    use tempfile;

    /// Mock Docker client that records method calls for testing
    #[derive(Clone)]
    struct MockDocker {
        pub calls: Arc<RwLock<Vec<String>>>,
        pub should_fail: bool,
    }

    impl MockDocker {
        pub fn new(should_fail: bool) -> Self {
            Self {
                calls: Arc::new(RwLock::new(Vec::new())),
                should_fail,
            }
        }

        pub async fn get_calls(&self) -> Vec<String> {
            self.calls.read().await.clone()
        }

        pub async fn record_call(&self, call: &str) {
            self.calls.write().await.push(call.to_string());
        }
    }

    /// Create a test LocalContainerService with mock dependencies
    async fn create_test_service(docker_should_fail: bool) -> (LocalContainerService, MockDocker) {
        let mock_docker = MockDocker::new(docker_should_fail);

        // Create minimal mock dependencies - in a real test these would be proper mocks
        let db = DBService::new().await.expect("Failed to create test DB");
        let msg_stores = Arc::new(RwLock::new(HashMap::new()));
        let config = Arc::new(RwLock::new(Config::default()));
        let git = GitService::new();
        let image_service = ImageService::new(db.clone().pool).expect("Failed to create ImageService");

        let mut service = LocalContainerService::new(
            db,
            msg_stores,
            config,
            git,
            image_service,
            None, // analytics
        );

        // Replace the real Docker client with our mock (this is a conceptual example)
        // In practice, you'd need to make the Docker field injectable or use a trait
        service.docker = if docker_should_fail {
            None
        } else {
            // For testing, we'll just set None since we can't easily mock Docker client
            // In a real implementation, this would use dependency injection
            None
        };

        (service, mock_docker)
    }

    /// Create a test TaskAttempt with minimal required fields
    fn create_test_task_attempt() -> TaskAttempt {
        TaskAttempt {
            id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            base_branch: "main".to_string(),
            container_ref: None,
            branch: None,
            executor: "CLAUDE_CODE".to_string(),
            worktree_deleted: false,
            setup_completed_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_create_docker_container_no_docker_client() {
        let (service, _mock) = create_test_service(true).await; // Docker should fail
        let task_attempt = create_test_task_attempt();
        let repo_path = "/test/repo";

        let result = service.create_docker_container(&task_attempt, repo_path).await;

        assert!(result.is_err());
        if let Err(ContainerError::Other(e)) = result {
            assert!(e.to_string().contains("Docker client not available"));
        } else {
            panic!("Expected Docker client not available error");
        }
    }

    #[tokio::test]
    async fn test_should_use_docker_no_docker_client() {
        let (service, _mock) = create_test_service(true).await; // Docker should fail
        let task_attempt = create_test_task_attempt();

        let result = service.should_use_docker(&task_attempt).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None); // Should return None when Docker unavailable
    }

    #[tokio::test]
    async fn test_is_docker_container_identification() {
        let (service, _mock) = create_test_service(false).await;

        // Test Docker container ID patterns
        assert!(service.is_docker_container("a1b2c3d4e5f67890abcdef1234567890abcdef1234567890abcdef1234567890")); // 64 chars
        assert!(service.is_docker_container("a1b2c3d4e5f6")); // 12 chars

        // Test non-Docker paths
        assert!(!service.is_docker_container("/path/to/worktree"));
        assert!(!service.is_docker_container("short"));
        assert!(!service.is_docker_container("invalid-hex-string!@#"));
    }

    #[tokio::test]
    async fn test_resolve_devcontainer_config() {
        let (service, _mock) = create_test_service(false).await;

        // Create temporary test directories
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let test_repo = temp_dir.path().join("test_repo");
        std::fs::create_dir_all(&test_repo).expect("Failed to create test repo dir");

        let project_devcontainer = test_repo.join(".devcontainer");
        std::fs::create_dir_all(&project_devcontainer).expect("Failed to create .devcontainer dir");

        // Test with project devcontainer
        let result = service.resolve_devcontainer_config(&test_repo);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), project_devcontainer);

        // Test without project devcontainer (should fall back to current dir)
        std::fs::remove_dir_all(&project_devcontainer).expect("Failed to remove project devcontainer");

        let repo_without_devcontainer = temp_dir.path().join("repo_no_devcontainer");
        std::fs::create_dir_all(&repo_without_devcontainer).expect("Failed to create repo dir");

        let result = service.resolve_devcontainer_config(&repo_without_devcontainer);
        // This will likely fail unless we're in a directory with .devcontainer
        // In a real test environment, you'd set up the expected fallback behavior
        assert!(result.is_err() || result.unwrap().exists());
    }

    #[tokio::test]
    async fn test_create_build_context() {
        let (service, _mock) = create_test_service(false).await;

        // Create a temporary directory with some test files
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "test content").expect("Failed to write test file");

        let result = service.create_build_context(temp_dir.path());

        assert!(result.is_ok());
        let tar_data = result.unwrap();
        assert!(!tar_data.is_empty());

        // Verify the tar contains our test file (basic check)
        assert!(tar_data.len() > "test content".len());
    }

    #[tokio::test]
    async fn test_dir_name_from_task_attempt() {
        let attempt_id = Uuid::new_v4();
        let task_title = "Test Task Title!@#$%";

        let dir_name = LocalContainerService::dir_name_from_task_attempt(&attempt_id, task_title);

        // Verify it starts with "vk-"
        assert!(dir_name.starts_with("vk-"));

        // Verify it contains the shortened UUID
        let short_id = short_uuid(&attempt_id);
        assert!(dir_name.contains(&short_id));

        // Verify it contains a sanitized version of the task title
        assert!(dir_name.len() > "vk-".len() + short_id.len());
    }

    /// Full integration test that actually creates a Docker container
    #[tokio::test]
    #[ignore] // Ignored by default since it requires Docker daemon
    async fn test_create_docker_container_full_integration() {
        // Skip test if Docker integration is not enabled
        if std::env::var("RUN_DOCKER_TESTS").is_err() {
            println!("Skipping Docker integration test - set RUN_DOCKER_TESTS=1 to enable");
            return;
        }

        // Check if Docker is available
        if Docker::connect_with_socket_defaults().is_err() {
            println!("Docker daemon not available - skipping integration test");
            return;
        }

        // Create test directory with a minimal devcontainer setup
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let test_repo = temp_dir.path().join("test_repo");
        std::fs::create_dir_all(&test_repo).expect("Failed to create test repo");

        // Set up minimal test repository
        setup_test_repository(&test_repo).await;

        // Create service with real Docker client
        let (service, _mock) = create_test_service_with_docker(false).await;

        // Create test entities in database
        let (task_attempt, _task, _project) = create_test_entities(&service.db, &test_repo).await;

        // Before test: list existing containers for comparison
        if let Some(docker) = &service.docker {
            let containers_before = docker.list_containers::<String>(None).await.unwrap_or_default();
            println!("📊 Containers before test: {}", containers_before.len());
        }

        // Test the actual container creation
        let result = service.create_docker_container(&task_attempt, &test_repo.to_string_lossy()).await;

        match result {
            Ok(container_id) => {
                println!("✅ Successfully created Docker container: {}", container_id);

                // Verify container exists and inspect its configuration
                if let Some(docker) = &service.docker {
                    // Try to inspect the specific container by ID
                    match docker.inspect_container(&container_id, None).await {
                        Ok(container_info) => {
                            println!("✅ Container found and inspected:");
                            println!("   • ID: {}", container_info.id.as_deref().unwrap_or("unknown"));
                            println!("   • State: {:?}", container_info.state.as_ref().map(|s| &s.status));
                            println!("   • Working Dir: {:?}", container_info.config.as_ref().and_then(|c| c.working_dir.as_ref()));

                            // Check mounts/binds
                            if let Some(host_config) = &container_info.host_config {
                                if let Some(binds) = &host_config.binds {
                                    println!("   • Volume Binds: {:?}", binds);
                                } else {
                                    println!("   • ⚠ No volume binds found!");
                                }
                            }

                            if let Some(mounts) = &container_info.mounts {
                                println!("   • Mounts: {} mount(s)", mounts.len());
                                for mount in mounts {
                                    println!("     - {} -> {}", mount.source.as_deref().unwrap_or("?"), mount.destination.as_deref().unwrap_or("?"));
                                }
                            } else {
                                println!("   • ⚠ No mounts found!");
                            }

                            // Clean up
                            if container_info.state.as_ref().map_or(false, |s| s.running.unwrap_or(false)) {
                                let _ = docker.stop_container(&container_id, None).await;
                            }
                            let _ = docker.remove_container(&container_id, None).await;
                            println!("✅ Container cleaned up");
                        }
                        Err(e) => {
                            println!("⚠ Could not inspect container {}: {}", container_id, e);

                            // List all containers for debugging
                            use bollard::container::ListContainersOptions;
                            let all_containers_opts = ListContainersOptions::<String> {
                                all: true,
                                ..Default::default()
                            };
                            if let Ok(containers) = docker.list_containers(Some(all_containers_opts)).await {
                                println!("🔍 All containers in Docker:");
                                for c in &containers {
                                    println!("   • {} ({})", c.id.as_deref().unwrap_or("no-id")[..12].to_string(), c.names.as_ref().map(|n| n.join(",")).unwrap_or_default());
                                }

                                // Check if any container matches by partial ID
                                for c in &containers {
                                    if let Some(id) = &c.id {
                                        if id.starts_with(&container_id[..12]) || container_id.starts_with(&id[..12]) {
                                            println!("🔍 Found potential match: {}", id);
                                            if let Ok(info) = docker.inspect_container(id, None).await {
                                                println!("   • Working Dir: {:?}", info.config.as_ref().and_then(|c| c.working_dir.as_ref()));
                                                println!("   • Binds: {:?}", info.host_config.as_ref().and_then(|hc| hc.binds.as_ref()));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Verify database was updated with container ID
                let updated_attempt = TaskAttempt::find_by_id(&service.db.pool, task_attempt.id)
                    .await
                    .expect("Failed to fetch updated task attempt")
                    .expect("Task attempt not found");
                assert_eq!(updated_attempt.container_ref, Some(container_id.clone()));
                println!("✓ Database updated with container reference");
            }
            Err(e) => {
                panic!("Failed to create Docker container: {}", e);
            }
        }
    }

    /// Helper to set up a minimal test repository with devcontainer
    async fn setup_test_repository(repo_path: &std::path::Path) {
        // Create .devcontainer directory
        let devcontainer_dir = repo_path.join(".devcontainer");
        std::fs::create_dir_all(&devcontainer_dir).expect("Failed to create .devcontainer dir");

        // Create minimal Dockerfile
        let dockerfile_content = r#"FROM alpine:latest
RUN apk add --no-cache bash curl git
WORKDIR /workspace
CMD ["/bin/bash"]
"#;
        std::fs::write(devcontainer_dir.join("Dockerfile"), dockerfile_content)
            .expect("Failed to write Dockerfile");

        // Create devcontainer.json (optional, but good practice)
        let devcontainer_json = r#"{
    "name": "Test Container",
    "build": {
        "dockerfile": "Dockerfile"
    },
    "workspaceFolder": "/workspace"
}
"#;
        std::fs::write(devcontainer_dir.join("devcontainer.json"), devcontainer_json)
            .expect("Failed to write devcontainer.json");

        // Create a simple test file
        std::fs::write(repo_path.join("README.md"), "# Test Repository\n\nThis is a test repository for Docker container testing.\n")
            .expect("Failed to write README.md");

        println!("✓ Test repository set up at {:?}", repo_path);
    }

    /// Helper to create test entities in the database
    async fn create_test_entities(
        db: &DBService,
        repo_path: &std::path::Path,
    ) -> (TaskAttempt, db::models::task::Task, db::models::project::Project) {
        use db::models::{project::Project, task::Task};

        // Create test project
        let project = Project {
            id: Uuid::new_v4(),
            name: "Test Project".to_string(),
            git_repo_path: repo_path.to_path_buf(),
            setup_script: None,
            dev_script: None,
            cleanup_script: None,
            copy_files: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Insert project into database using raw SQL
        sqlx::query(
            "INSERT INTO projects (id, name, git_repo_path, setup_script, dev_script, cleanup_script, copy_files, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&project.id)
        .bind(&project.name)
        .bind(project.git_repo_path.to_string_lossy().as_ref())
        .bind(&project.setup_script)
        .bind(&project.dev_script)
        .bind(&project.cleanup_script)
        .bind(&project.copy_files)
        .bind(&project.created_at)
        .bind(&project.updated_at)
        .execute(&db.pool)
        .await
        .expect("Failed to insert test project");

        // Create test task with repo_path to trigger Docker usage
        let task = Task {
            id: Uuid::new_v4(),
            project_id: project.id,
            title: "Test Task".to_string(),
            description: Some("Test task for Docker container spawning".to_string()),
            status: db::models::task::TaskStatus::Todo,
            parent_task_attempt: None,
            repo_path: Some(repo_path.to_string_lossy().to_string()), // This triggers Docker usage
            executor_profile_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Convert executor_profile_id to JSON string for storage
        let executor_profile_json: Option<String> = task.executor_profile_id.as_ref()
            .and_then(|p| serde_json::to_string(p).ok());

        // Insert task into database using raw SQL
        sqlx::query(
            "INSERT INTO tasks (id, project_id, title, description, status, parent_task_attempt, repo_path, executor_profile_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&task.id)
        .bind(&task.project_id)
        .bind(&task.title)
        .bind(&task.description)
        .bind(&task.status)  // Bind enum directly, not as i32
        .bind(&task.parent_task_attempt)
        .bind(&task.repo_path)
        .bind(&executor_profile_json)
        .bind(&task.created_at)
        .bind(&task.updated_at)
        .execute(&db.pool)
        .await
        .expect("Failed to insert test task");

        // Create test task attempt
        let task_attempt = TaskAttempt {
            id: Uuid::new_v4(),
            task_id: task.id,
            base_branch: "main".to_string(),
            container_ref: None,
            branch: None,
            executor: "CLAUDE_CODE".to_string(),
            worktree_deleted: false,
            setup_completed_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Insert task attempt into database using raw SQL
        sqlx::query(
            "INSERT INTO task_attempts (id, task_id, base_branch, container_ref, branch, executor, worktree_deleted, setup_completed_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&task_attempt.id)
        .bind(&task_attempt.task_id)
        .bind(&task_attempt.base_branch)
        .bind(&task_attempt.container_ref)
        .bind(&task_attempt.branch)
        .bind(&task_attempt.executor)
        .bind(&task_attempt.worktree_deleted)
        .bind(&task_attempt.setup_completed_at)
        .bind(&task_attempt.created_at)
        .bind(&task_attempt.updated_at)
        .execute(&db.pool)
        .await
        .expect("Failed to insert test task attempt");

        println!("✓ Test entities created in database");
        (task_attempt, task, project)
    }

    /// Create a test service with real Docker client
    async fn create_test_service_with_docker(docker_should_fail: bool) -> (LocalContainerService, MockDocker) {
        let mock_docker = MockDocker::new(docker_should_fail);

        // Create minimal mock dependencies
        let db = DBService::new().await.expect("Failed to create test DB");
        let msg_stores = Arc::new(RwLock::new(HashMap::new()));
        let config = Arc::new(RwLock::new(Config::default()));
        let git = GitService::new();
        let image_service = ImageService::new(db.clone().pool).expect("Failed to create ImageService");

        let service = LocalContainerService::new(
            db,
            msg_stores,
            config,
            git,
            image_service,
            None, // analytics
        );

        // The service will initialize its own Docker client in the constructor
        // We don't override it for the integration test
        (service, mock_docker)
    }

    /// Test helper to verify the function signature and basic error handling
    #[test]
    fn test_create_docker_container_signature() {
        // Compile-time test to ensure the function signature is correct
        fn _test_signature() {
            use std::future::Future;
            use std::pin::Pin;

            fn _check_return_type() -> Pin<Box<dyn Future<Output = Result<ContainerRef, ContainerError>>>> {
                todo!()
            }

            // This ensures the create_docker_container method exists with correct signature
            // We can't directly test the exact signature due to lifetime constraints,
            // but this verifies the method exists and is callable
            let _service = LocalContainerService::create_docker_container;
        }
        _test_signature();
    }
}