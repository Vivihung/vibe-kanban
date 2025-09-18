# Multi-Repository VSCode Dev Container Support for Vibe Kanban

## Executive Summary

This specification outlines the design for extending Vibe Kanban to support multi-repository development by dynamically spawning VSCode dev containers with Claude Code pre-configured for each coding agent task. For the initial PoC, users will provide a local folder path that gets mounted into the container, bypassing repository access and authentication complexity while validating the core container orchestration and Claude Code integration concepts.

## Current Architecture Analysis

### Existing Limitations
- **Single Repository Constraint**: Vibe Kanban operates only on the repository mounted to its container at startup
- **Static Environment**: Cannot adapt to different project configurations or dependencies
- **Limited Scope**: Tasks are confined to the mounted repository's context

### PoC Approach
- **Folder Mounting**: Users provide local folder paths that are mounted into containers
- **Simplified Authentication**: Bypasses repository access and credential management
- **Local Development Focus**: Works with repositories already cloned locally by users

### Current Container Service Architecture
- Uses `WorktreeManager` for git worktree isolation per task
- Container service abstracts execution environment
- Browser chat agents already bypass container/worktree creation
- Supports both coding agents (require worktrees) and browser agents (direct execution)

## Requirements

### Functional Requirements (PoC Phase)

#### FR1: Local Folder Management
- **FR1.1**: Accept local folder path when creating tasks
- **FR1.2**: Validate folder exists and is accessible
- **FR1.3**: Mount folder into dev container with proper permissions
- **FR1.4**: Support git repositories with existing worktree isolation

#### FR2: VSCode Dev Container Management
- **FR2.1**: Parse and apply `.devcontainer/devcontainer.json` configurations
- **FR2.2**: Build dev container images with project-specific dependencies
- **FR2.3**: Mount local folder into dev container as `/workspace`
- **FR2.4**: Configure networking for container-to-container communication

#### FR3: Claude Code Integration
- **FR3.1**: Install and configure Claude Code CLI in dev containers
- **FR3.2**: Execute Claude Code tasks within dev container context
- **FR3.3**: Stream execution logs and results back to Vibe Kanban via container logs
- **FR3.4**: Monitor task completion and collect outputs from containers

#### FR4: Container Security and Isolation
- **FR4.1**: Isolate containers from each other and host system
- **FR4.2**: Run containers with appropriate user permissions
- **FR4.3**: Implement resource limits and monitoring
- **FR4.4**: Clean up containers and resources when tasks complete

### Non-Functional Requirements

#### NFR1: Performance
- Container startup time < 2 minutes for typical dev containers
- Support concurrent task execution across multiple repositories
- Efficient resource utilization and cleanup

#### NFR2: Security
- Container isolation and sandboxing
- Secure communication channels between components
- Proper file system permissions for mounted folders

#### NFR3: Reliability
- Robust error handling and recovery
- Container lifecycle management
- Resource leak prevention

## Technical Design

### Architecture Overview

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Vibe Kanban   │    │  Container Host  │    │   Dev Container │
│   (Main App)    │    │   (Docker/K8s)   │    │ (per task/folder)│
├─────────────────┤    ├──────────────────┤    ├─────────────────┤
│ • Task Creation │───▶│ • Image Building │───▶│ • Claude Code   │
│ • UI/API        │    │ • Container Mgmt │    │ • Mounted Folder│
│ • Orchestration │    │ • Volume Mgmt    │    │ • Dev Tools     │
│ • Log Streaming │◀───│ • Log Collection │◀───│ • Log Output    │
└─────────────────┘    └──────────────────┘    └─────────────────┘
          │                                              │
          └──────────── Local Folder Mount ─────────────┘
                        (/host/path → /workspace)

          Communication: Container Logs ──────▶ Vibe Kanban
                        (Progress, results, status updates)
```

### Component Design

#### 1. Local Folder Service
```rust
pub struct LocalFolderService {
    container_service: Box<dyn ContainerService>,
    dev_container_builder: DevContainerBuilder,
}

impl LocalFolderService {
    async fn validate_folder_path(&self, folder_path: &Path) -> Result<FolderContext>;
    async fn setup_dev_container(&self, folder_context: &FolderContext) -> Result<ContainerRef>;
    async fn execute_claude_code_task(&self, container_ref: &ContainerRef, task_prompt: &str) -> Result<()>;
    async fn collect_task_results(&self, container_ref: &ContainerRef) -> Result<TaskResults>;
}
```

#### 2. Dev Container Builder
```rust
pub struct DevContainerBuilder {
    docker_client: DockerClient,
    image_cache: HashMap<String, String>, // folder_hash -> image_id
    claude_code_analyzer: ClaudeCodeConfigAnalyzer,
}

impl DevContainerBuilder {
    async fn parse_devcontainer_config(&self, folder_path: &Path) -> Result<DevContainerConfig>;
    async fn analyze_claude_code_compatibility(&self, config: &DevContainerConfig) -> Result<ClaudeCodeCompatibility>;
    async fn enhance_config_for_claude_code(&self, config: &mut DevContainerConfig) -> Result<()>;
    async fn build_image(&self, config: &DevContainerConfig, folder_hash: &str) -> Result<String>;
    async fn create_container(&self, image_id: &str, folder_path: &Path) -> Result<ContainerRef>;
}
```

#### 3. Claude Code Configuration Analyzer
```rust
pub struct ClaudeCodeConfigAnalyzer {
    required_packages: Vec<String>,
    required_scripts: Vec<String>,
    required_permissions: Vec<String>,
}

#[derive(Debug)]
pub enum ClaudeCodeCompatibility {
    FullyCompatible,          // Has all Claude Code requirements
    MissingRequirements(Vec<String>), // Missing specific items
    NoDevContainer,           // No .devcontainer folder exists
}

impl ClaudeCodeConfigAnalyzer {
    fn new() -> Self;
    async fn analyze_devcontainer(&self, config: &DevContainerConfig) -> Result<ClaudeCodeCompatibility>;
    async fn get_claude_code_requirements() -> DevContainerRequirements;
    async fn merge_requirements(&self, existing: &DevContainerConfig, required: &DevContainerRequirements) -> DevContainerConfig;
}
```

#### 4. Container Security Manager
```rust
pub struct ContainerSecurityManager {
    user_mapping: UserIdMapping,
    resource_limits: ResourceLimits,
    firewall_manager: StandardFirewallManager,
}

impl ContainerSecurityManager {
    async fn apply_security_policies(&self, container_ref: &ContainerRef) -> Result<()>;
    async fn configure_user_permissions(&self, container_ref: &ContainerRef, folder_path: &Path) -> Result<()>;
    async fn set_resource_limits(&self, container_ref: &ContainerRef) -> Result<()>;
    async fn apply_standard_firewall(&self, container_ref: &ContainerRef) -> Result<()>;
}
```

### Data Models

#### Task Extension
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct LocalFolderTask {
    // Existing task fields...
    pub local_folder_path: Option<String>,
    pub dev_container_ref: Option<String>,
    pub folder_context: Option<FolderContext>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FolderContext {
    pub local_path: PathBuf,
    pub mounted_path: PathBuf, // Always /workspace in container
    pub is_git_repo: bool,
    pub git_branch: Option<String>,
    pub git_commit_sha: Option<String>,
    pub devcontainer_config: Option<DevContainerConfig>,
    pub claude_code_compatibility: ClaudeCodeCompatibility,
    pub required_enhancements: Vec<String>,
}
```

### Container Orchestration Strategy

#### Selected Approach: Sibling Containers (PoC)

**Architecture**: Vibe Kanban and task containers run as siblings, sharing the same Docker daemon on the host.

**Implementation Details**:
```rust
pub struct SiblingContainerOrchestrator {
    docker_client: DockerClient,
    shared_network: String, // Docker network for inter-container communication
    volume_manager: VolumeManager,
}

impl SiblingContainerOrchestrator {
    async fn create_task_container(&self, folder_context: &FolderContext) -> Result<ContainerRef> {
        // Mount host folder into container
        // Apply security policies and resource limits
        // Setup log streaming
    }

    async fn execute_task(&self, container_ref: &ContainerRef, task_prompt: &str) -> Result<()>;
    async fn stream_logs(&self, container_ref: &ContainerRef) -> Result<LogStream>;
    async fn collect_results(&self, container_ref: &ContainerRef) -> Result<TaskResults>;
}
```

**Benefits for PoC**:
- **Performance**: Direct Docker API access, no nested virtualization overhead
- **Simplicity**: No complex orchestration layer needed
- **Log Streaming**: Easy access to container logs via Docker API
- **Volume Management**: Direct host folder mounting without additional abstraction layers

**Security Considerations**:
- Containers share Docker daemon (managed risk for PoC)
- User ID mapping for file system access
- Resource limits to prevent container interference
- Log isolation and secure collection

**Alternative Options (Future)**:
- **Docker-in-Docker**: Better isolation but performance overhead
- **Kubernetes Jobs**: Production-grade orchestration for scaling

### Firewall Configuration Strategy

#### Standardized Approach: Use Claude Code Official Firewall

**Implementation Strategy**: Use Claude Code's official firewall script from their repository as the standard configuration for all task containers, regardless of the project's own firewall setup.

**Benefits**:
- **Consistency**: All Claude Code instances have identical network access
- **Reliability**: Official script is tested and maintained by Anthropic
- **Simplicity**: No complex merging or conflict resolution needed
- **Security**: Proven firewall configuration with minimal attack surface

**Implementation**:
```rust
pub struct StandardFirewallManager {
    claude_code_firewall_script: String, // Downloaded from official repo
    container_orchestrator: SiblingContainerOrchestrator,
}

impl StandardFirewallManager {
    async fn download_official_firewall_script() -> Result<String>;
    async fn apply_to_container(&self, container_ref: &ContainerRef) -> Result<()>;
    async fn verify_firewall_active(&self, container_ref: &ContainerRef) -> Result<()>;
}
```

### DevContainer Configuration Strategy

#### Intelligent Configuration Handling

**Logic Flow**:
```rust
pub enum DevContainerAction {
    UseExisting,                    // .devcontainer fully compatible with Claude Code
    EnhanceExisting(Vec<String>),   // .devcontainer exists but needs Claude Code additions
    CreateNew,                      // No .devcontainer folder, create from scratch
}
```

**Implementation Workflow**:
1. **Analyze Existing Configuration**: Check if `.devcontainer/` exists in mounted folder
2. **Assess Claude Code Compatibility**: Determine what's missing for Claude Code functionality
3. **Apply Minimal Changes**: Only modify what's necessary to support Claude Code
4. **Preserve Project Setup**: Maintain existing project-specific configuration

**Claude Code Requirements Detection**:
- **Required Packages**: `@anthropic-ai/claude-code`, `git`, `curl`, `iptables`, `ipset`
- **Firewall Script**: Presence of `init-firewall.sh` with Claude Code domains
- **Permissions**: Sudo permissions for firewall execution
- **Base Image**: Node.js environment or compatible runtime

**Enhancement Strategy**:
```bash
# If missing Claude Code CLI
RUN npm install -g @anthropic-ai/claude-code

# If missing firewall packages
RUN apt update && apt install -y iptables ipset dnsutils aggregate jq

# If missing firewall script
COPY init-firewall.sh /usr/local/bin/
RUN chmod +x /usr/local/bin/init-firewall.sh

# If missing sudo permissions for firewall
RUN echo "node ALL=(root) NOPASSWD: /usr/local/bin/init-firewall.sh" >> /etc/sudoers
```

**Workflow**:
1. **Parse Project DevContainer**: Load existing `devcontainer.json` and `Dockerfile`
2. **Analyze Compatibility**: Check against Claude Code requirements
3. **Generate Enhancements**: Create minimal additions/modifications needed
4. **Build Enhanced Image**: Combine project config with Claude Code requirements
5. **Verify Functionality**: Ensure both project and Claude Code work properly

### Security and Permissions Strategy

#### File System Permissions
```rust
pub struct FileSystemSecurityConfig {
    pub host_uid: u32,
    pub host_gid: u32,
    pub container_uid: u32,
    pub container_gid: u32,
    pub mount_options: Vec<String>, // e.g., ["rw", "nodev", "nosuid"]
}
```

**Benefits**:
- Proper user/group mapping between host and container
- Read/write access to mounted folders maintained
- Security isolation through mount options
- No credential management complexity

#### Container Isolation
- Containers run with mapped user IDs for file access
- Resource limits prevent resource exhaustion
- Network isolation between task containers
- Automatic cleanup of containers and volumes

### Implementation Phases

#### Phase 1: Core Infrastructure (3-4 weeks)
- [ ] Extend task model to support local folder paths
- [ ] Implement folder validation and context service
- [ ] Create dev container builder with Docker client
- [ ] Implement Claude Code configuration analyzer
- [ ] Build intelligent devcontainer enhancement logic
- [ ] Implement sibling container orchestration with shared Docker daemon
- [ ] Setup Docker network for container communication
- [ ] Implement file system permissions and user mapping
- [ ] Build standard firewall manager using Claude Code's official firewall script

#### Phase 2: Claude Code Integration (2-3 weeks)
- [ ] Install Claude Code CLI in dev containers
- [ ] Configure Claude Code execution within container environment
- [ ] Implement task execution routing to Claude Code in containers
- [ ] Setup log streaming from sibling containers to Vibe Kanban
- [ ] Build container lifecycle management (start/stop/cleanup)
- [ ] Create output collection and status monitoring

#### Phase 3: Production Readiness (3-4 weeks)
- [ ] Comprehensive error handling and recovery for container failures
- [ ] Resource monitoring and cleanup for sibling containers
- [ ] Performance optimization (container reuse, image caching)
- [ ] Security hardening and audit of shared Docker daemon access

#### Phase 4: Advanced Features (Future)
- [ ] Repository cloning and authentication (full multi-repo support)
- [ ] Container image caching optimization
- [ ] Migration to Kubernetes orchestration or Docker-in-Docker for better isolation
- [ ] Advanced git features (submodules, LFS, branch management)

## Open Questions and Technical Challenges (PoC Phase)

### 🔒 File System & Security

1. **File System Permissions**
   - How do we handle user/group ID mapping between host and container?
   - Should we run containers as the same user ID as the Vibe Kanban process?
   - What mount options provide the right balance of security and functionality?

2. **Container Security Isolation**
   - What user/group should containers run as for file access?
   - Should we use Docker's user namespace remapping for the PoC?
   - How do we prevent container escape attacks with mounted host folders?

3. **Path Validation and Security**
   - How do we validate that provided folder paths are safe to mount?
   - Should we restrict mounting to certain path prefixes?
   - How do we handle symlinks and other edge cases?

### 🐳 Container Orchestration

4. **Container Lifecycle Management**
   - When do we clean up dev containers (task completion, timeout, manual)?
   - How do we handle container crashes or OOM kills?
   - Should we implement container hibernation for inactive tasks?

5. **Resource Management**
   - What CPU/memory limits should we set per container?
   - How do we prevent resource exhaustion attacks?
   - Should we implement container pooling to reduce startup time?

6. **Sibling Container Networking**
   - How should task containers communicate with Vibe Kanban via shared Docker network?
   - Should we use a dedicated Docker bridge network or the default network?
   - How do we handle port conflicts between multiple sibling containers?
   - What's the best approach for service discovery between containers?

### 🏗️ Dev Container Compatibility

7. **DevContainer Feature Support**
   - Which `devcontainer.json` features should we support initially with sibling containers?
   - How do we handle Docker Compose configurations when containers are siblings?
   - Should we support VS Code extensions installation in task containers?

8. **Existing DevContainer Integration** *(Addressed in spec)*
   - ✅ How do we handle projects that already have .devcontainer folders?
   - ✅ What if a project's devcontainer is missing Claude Code requirements?
   - ✅ How do we preserve project-specific configuration while adding Claude Code support?
   - ✅ Solution: Intelligent analysis and minimal enhancement approach

9. **Firewall Configuration Management** *(Resolved - Use Claude Code Official)*
   - ✅ How do we ensure consistent firewall configuration across all task containers?
   - ✅ What domains does Claude Code need for proper functionality?
   - ✅ How do we avoid conflicts between project and Claude Code firewall requirements?
   - ✅ Solution: Use Claude Code's official firewall script for all containers

10. **Image Building Strategy**
   - Should we build images on-demand or pre-build common configurations?
   - How do we handle Dockerfile-based vs image-based devcontainer configs?
   - What caching strategy minimizes build times while ensuring security?

### 🔧 Claude Code Integration

11. **Container Communication and Monitoring**
   - How should we stream Claude Code execution logs from containers to Vibe Kanban?
   - What's the best approach for real-time log collection (Docker logs API, log files, stdout)?
   - How do we detect task completion and collect final outputs from containers?
   - What status information should be monitored during Claude Code execution?

12. **Execution Environment Parity**
    - How do we ensure Claude Code in containers has equivalent capabilities to local execution?
    - Should we mount the same file system permissions and user context?
    - How do we handle tools that require GUI access (browsers, desktop apps)?

### 📊 Monitoring & Observability

13. **Sibling Container Health Monitoring**
    - What metrics should we track for sibling container health and performance?
    - How do we detect and handle zombie containers in the shared Docker daemon?
    - Should we implement container restart policies for failed task containers?
    - How do we monitor Docker daemon health and resource usage?

14. **Log Management**
    - How do we aggregate logs from multiple containers?
    - Should container logs be streamed in real-time or batch collected?
    - What's the log retention policy for completed tasks?

### 🔄 Data Persistence & Workspace Management

15. **Folder State Management**
    - How do we handle file changes in the mounted folder during task execution?
    - Should we create worktrees within the mounted folder for isolation?
    - How do we prevent conflicts when multiple tasks work on the same folder?

16. **Container Data Persistence**
    - What data (if any) should persist between container restarts?
    - How do we handle development artifacts and build outputs?
    - Should we support container snapshots for debugging?

### 🎯 User Experience

17. **Task Configuration Interface**
    - How should users specify local folder paths in the UI?
    - Should we auto-detect devcontainer configurations in the provided folder?
    - How do we handle folders without devcontainer configs (fallback to default)?

18. **Progress and Status Reporting**
    - How do we communicate container setup progress to users?
    - What status information should be displayed during folder validation and image building?
    - How do we handle and display error states clearly?

## Success Metrics

- **Container Startup Time**: < 2 minutes average for typical dev containers
- **Resource Efficiency**: Support 10+ concurrent containers on standard hardware
- **Reliability**: 99% successful container creation rate
- **Security**: Zero file system permission incidents
- **User Adoption**: 80% of new tasks use local folder functionality within 3 months

## Risk Mitigation

### High-Risk Items
1. **Container Escape**: Implement strict security policies, regular updates
2. **File System Access**: Proper user ID mapping, mount option restrictions, path validation
3. **Resource Exhaustion**: Implement quotas, monitoring, graceful degradation

### Medium-Risk Items
1. **Performance Issues**: Implement caching, optimize image layers
2. **Complex Dev Container Configs**: Start with subset, expand gradually
3. **Network Connectivity**: Implement retry logic, health checks

## Conclusion

This specification provides a pragmatic roadmap for extending Vibe Kanban to support multi-repository development through dynamic VSCode dev containers. The PoC approach using local folder mounting significantly reduces complexity while validating the core concepts of container orchestration and Claude Code integration.

The phased approach allows for incremental delivery, starting with the simplified folder mounting approach and eventually expanding to full repository cloning and authentication capabilities. This implementation will significantly expand Vibe Kanban's utility and position it as a comprehensive development automation platform.