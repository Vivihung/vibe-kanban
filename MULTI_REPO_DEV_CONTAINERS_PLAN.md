# Multi-Repo Dev Container MVP Implementation Plan

## Executive Summary

This plan outlines the minimal viable implementation to add multi-repository support to Vibe Kanban using dev containers. Users will be able to specify a local repository path via a new `repo` parameter, and coding agent tasks will execute inside containers with the repository mounted as workspace.

## MVP Scope

### Core Functionality
- Add `repo` parameter to task creation (optional absolute path)
- For coding agents: if `repo` provided, execute inside container with mounted repository
- Use Vibe Kanban's `.devcontainer` config as default, project's config if available
- Stream container logs back to Vibe Kanban UI (like existing executors)

### Non-Goals (Future Iterations)
- Complex devcontainer configuration merging
- Container reuse/pooling
- Multi-branch/worktree support
- Advanced security policies
- Performance optimizations

## Detailed Code Changes

### 1. Database Schema Changes

**File**: `crates/db/migrations/YYYYMMDD_add_repo_path_to_tasks.sql`
```sql
-- Add repo_path column to tasks table
ALTER TABLE tasks ADD COLUMN repo_path TEXT;
```

**File**: `crates/db/src/models/task.rs`
```rust
// Add to Task struct
pub struct Task {
    // ... existing fields
    pub repo_path: Option<String>,
}

// Add to CreateTask struct
pub struct CreateTask {
    // ... existing fields
    pub repo_path: Option<String>,
}

// Update create/update methods to handle repo_path
impl Task {
    pub async fn create(
        pool: &SqlitePool,
        create_task: &CreateTask,
        task_id: Uuid,
    ) -> Result<Task, sqlx::Error> {
        // Update INSERT query to include repo_path
    }

    pub async fn update(
        pool: &SqlitePool,
        task_id: Uuid,
        project_id: Uuid,
        title: String,
        description: Option<String>,
        status: TaskStatus,
        parent_task_attempt: Option<Uuid>,
        repo_path: Option<String>, // Add this parameter
    ) -> Result<Task, sqlx::Error> {
        // Update query to include repo_path
    }
}
```

### 2. API Changes

**File**: `crates/server/src/routes/tasks.rs`
```rust
#[derive(Deserialize)]
pub struct CreateTaskRequest {
    // ... existing fields
    pub repo_path: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateTaskRequest {
    // ... existing fields
    pub repo_path: Option<String>,
}

// Update create_task handler
pub async fn create_task(
    State(app_state): State<AppState>,
    Json(request): Json<CreateTaskRequest>,
) -> Result<Json<CreateTaskResponse>, AppError> {
    // Validate repo_path if provided (must be absolute path, must exist)
    if let Some(ref repo_path) = request.repo_path {
        validate_repo_path(repo_path)?;
    }

    let create_task = CreateTask {
        // ... existing fields
        repo_path: request.repo_path,
    };

    // ... rest of handler
}

fn validate_repo_path(repo_path: &str) -> Result<(), AppError> {
    let path = Path::new(repo_path);

    // Must be absolute path
    if !path.is_absolute() {
        return Err(AppError::BadRequest("repo_path must be absolute path".into()));
    }

    // Must exist and be directory
    if !path.exists() || !path.is_dir() {
        return Err(AppError::BadRequest("repo_path must be existing directory".into()));
    }

    Ok(())
}
```

### 3. Frontend Changes

**File**: `shared/types.ts`
```typescript
export interface Task {
  // ... existing fields
  repo_path?: string;
}

export interface CreateTaskRequest {
  // ... existing fields
  repo_path?: string;
}
```

**File**: `frontend/src/components/tasks/CreateTaskModal.tsx`
```tsx
const [repoPath, setRepoPath] = useState<string>('');

// Add to form
<div className="space-y-2">
  <Label htmlFor="repo-path">Repository Path (Optional)</Label>
  <Input
    id="repo-path"
    placeholder="/absolute/path/to/repository"
    value={repoPath}
    onChange={(e) => setRepoPath(e.target.value)}
  />
  <p className="text-xs text-muted-foreground">
    Local absolute path to repository for coding agent tasks
  </p>
</div>

// Include in request
const handleSubmit = async () => {
  const request: CreateTaskRequest = {
    // ... existing fields
    repo_path: repoPath.trim() || undefined,
  };
  // ... submit logic
};
```

**File**: `frontend/src/components/tasks/TaskCard.tsx`
```tsx
// Add repo path display
{task.repo_path && (
  <div className="flex items-center gap-1 text-xs text-muted-foreground">
    <FolderIcon className="h-3 w-3" />
    <span className="truncate">{task.repo_path}</span>
  </div>
)}
```

### 4. Container Executor Implementation

**File**: `crates/executors/src/executors/container.rs`
```rust
use std::path::{Path, PathBuf};
use bollard::{Docker, container::{CreateContainerOptions, Config}, image::BuildImageOptions};
use tokio_stream::StreamExt;

pub struct ContainerExecutor {
    docker: Docker,
}

impl ContainerExecutor {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let docker = Docker::connect_with_socket_defaults()?;
        Ok(Self { docker })
    }

    pub async fn execute_task(
        &self,
        task_message: &str,
        repo_path: &Path,
        task_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {

        // 1. Determine devcontainer config to use
        let devcontainer_path = self.resolve_devcontainer_config(repo_path)?;

        // 2. Build container image
        let image_name = self.build_container_image(&devcontainer_path, task_id).await?;

        // 3. Create and start container with repo mounted
        let container_id = self.create_container(&image_name, repo_path, task_id).await?;

        // 4. Execute Claude Code inside container
        self.execute_claude_code(&container_id, task_message).await?;

        // 5. Cleanup
        self.cleanup_container(&container_id).await?;

        Ok(())
    }

    fn resolve_devcontainer_config(&self, repo_path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let project_devcontainer = repo_path.join(".devcontainer");

        if project_devcontainer.exists() {
            // Use project's devcontainer
            Ok(project_devcontainer)
        } else {
            // Use Vibe Kanban's devcontainer as default
            Ok(PathBuf::from("/workspace/.devcontainer"))
        }
    }

    async fn build_container_image(
        &self,
        devcontainer_path: &Path,
        task_id: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let image_name = format!("vibe-kanban-task-{}", task_id);

        // Build image from devcontainer
        let build_options = BuildImageOptions {
            dockerfile: "Dockerfile".to_string(),
            t: image_name.clone(),
            ..Default::default()
        };

        let tar_context = self.create_build_context(devcontainer_path)?;

        let mut stream = self.docker.build_image(build_options, None, Some(tar_context.into()));

        while let Some(result) = stream.next().await {
            match result {
                Ok(output) => {
                    // Log build progress
                    tracing::debug!("Docker build: {:?}", output);
                }
                Err(e) => {
                    tracing::error!("Docker build error: {:?}", e);
                    return Err(e.into());
                }
            }
        }

        Ok(image_name)
    }

    async fn create_container(
        &self,
        image_name: &str,
        repo_path: &Path,
        task_id: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let container_name = format!("vibe-kanban-task-{}", task_id);

        let config = Config {
            image: Some(image_name.to_string()),
            working_dir: Some("/workspace".to_string()),
            host_config: Some(bollard::container::HostConfig {
                binds: Some(vec![
                    format!("{}:/workspace", repo_path.to_string_lossy())
                ]),
                auto_remove: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };

        let container = self.docker
            .create_container(Some(CreateContainerOptions {
                name: container_name.clone(),
                platform: None,
            }), config)
            .await?;

        self.docker.start_container::<String>(&container.id, None).await?;

        Ok(container.id)
    }

    async fn execute_claude_code(
        &self,
        container_id: &str,
        task_message: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use bollard::exec::{CreateExecOptions, StartExecResults};

        let exec = self.docker
            .create_exec(
                container_id,
                CreateExecOptions {
                    cmd: Some(vec![
                        "claude".to_string(),
                        "code".to_string(),
                        "--message".to_string(),
                        task_message.to_string(),
                    ]),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await?;

        let mut stream = self.docker.start_exec(&exec.id, None).await?;

        while let Some(result) = stream.next().await {
            match result {
                Ok(StartExecResults::Attached { log }) => {
                    // Stream logs back to Vibe Kanban
                    tracing::info!("Claude Code output: {}", String::from_utf8_lossy(&log));
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("Exec error: {:?}", e);
                    return Err(e.into());
                }
            }
        }

        Ok(())
    }

    async fn cleanup_container(&self, container_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Stop container (auto_remove will clean it up)
        let _ = self.docker.stop_container(container_id, None).await;
        Ok(())
    }

    fn create_build_context(&self, devcontainer_path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Create tar archive of devcontainer directory
        // This is a simplified version - in practice, you'd want proper tar creation
        use tar::Builder;
        use std::io::Cursor;

        let mut buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut buffer);
            let mut archive = Builder::new(cursor);
            archive.append_dir_all(".", devcontainer_path)?;
            archive.finish()?;
        }

        Ok(buffer)
    }
}
```

### 5. Executor Integration

**File**: `crates/executors/src/executors/mod.rs`
```rust
pub mod container;
```

**File**: `crates/executors/src/lib.rs`
```rust
// Add container executor to the executor matching logic
use crate::executors::container::ContainerExecutor;

pub async fn execute_task(
    task: &Task,
    message: &str,
    // ... other parameters
) -> Result<(), Box<dyn std::error::Error>> {
    // If task has repo_path and is coding agent, use container executor
    if let Some(repo_path) = &task.repo_path {
        if is_coding_agent(&task.profile) {
            let container_executor = ContainerExecutor::new()?;
            return container_executor.execute_task(
                message,
                Path::new(repo_path),
                &task.id.to_string()
            ).await;
        }
    }

    // Otherwise, use existing executor logic
    match task.profile {
        // ... existing executor matching
    }
}

fn is_coding_agent(profile: &ProfileVariantLabel) -> bool {
    matches!(profile.profile.as_str(),
        "claude-code" | "cursor" | "codex" | "qwen" | "opencode" | "gemini" | "amp"
    )
}
```

### 6. Dependencies

**File**: `crates/executors/Cargo.toml`
```toml
[dependencies]
# ... existing dependencies
bollard = "0.17"
tar = "0.4"
```

## Testing Plan

### Manual Testing
1. Create task with `repo` parameter pointing to existing repository
2. Verify container is created with repository mounted
3. Confirm Claude Code executes inside container
4. Check logs are streamed back to UI
5. Verify container cleanup after task completion

### Edge Cases to Test
- Invalid repo paths (non-existent, relative paths)
- Repositories with existing `.devcontainer` configuration
- Repositories without `.devcontainer` (should use default)
- Docker daemon not running
- Container build failures

## Deployment Considerations

### Prerequisites
- Docker daemon running on host
- User running Vibe Kanban must have Docker permissions
- Sufficient disk space for container images

### Configuration
- No additional configuration needed for MVP
- Future: Docker daemon URL configuration for remote Docker hosts

## Future Enhancements (Post-MVP)

1. **Container Reuse**: Cache and reuse containers for same repository
2. **Configuration Merging**: Intelligent merging of project and Claude Code devcontainer configs
3. **Multi-Branch Support**: Support for git worktrees within repositories
4. **Security Enhancements**: User namespace mapping, resource limits
5. **Performance Optimizations**: Image caching, parallel container operations
6. **UI Improvements**: Repository browser, devcontainer status indicators

## Risk Mitigation

### High-Risk Items
1. **Docker Security**: Containers share host Docker daemon
   - Mitigation: Run with appropriate user permissions, consider Docker-in-Docker for production

2. **File System Permissions**: Host folder mounting permission issues
   - Mitigation: Proper user ID mapping between host and container

3. **Resource Exhaustion**: Multiple containers consuming system resources
   - Mitigation: Implement container limits, cleanup procedures

### Medium-Risk Items
1. **Build Failures**: Devcontainer build issues
   - Mitigation: Fallback to default Vibe Kanban devcontainer, better error handling

2. **Network Connectivity**: Container network access issues
   - Mitigation: Use proven Claude Code devcontainer network configuration

## Success Metrics

- **Functionality**: Successfully execute Claude Code tasks in mounted repositories
- **Performance**: Container startup time < 2 minutes for typical configurations
- **Reliability**: 95% successful task execution rate with proper error handling
- **User Experience**: Clear error messages and progress indication

## Implementation Timeline

- **Week 1**: Database schema, API changes, basic frontend UI
- **Week 2**: Container executor implementation, Docker integration
- **Week 3**: Integration testing, error handling, log streaming
- **Week 4**: Manual testing, bug fixes, documentation

This MVP validates the core concept with minimal risk to existing functionality while providing a clear path for future enhancements.