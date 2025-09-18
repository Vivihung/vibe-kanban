use chrono::{DateTime, Utc};
use executors::profile::ExecutorProfileId;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, Type, Row, sqlite::SqliteRow};
use ts_rs::TS;
use uuid::Uuid;

use super::project::Project;

#[derive(Debug, Clone, Type, Serialize, Deserialize, PartialEq, TS)]
#[sqlx(type_name = "task_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Todo,
    InProgress,
    InReview,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Task {
    pub id: Uuid,
    pub project_id: Uuid, // Foreign key to Project
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub parent_task_attempt: Option<Uuid>, // Foreign key to parent TaskAttempt
    pub repo_path: Option<String>, // Local repository path for container execution
    pub executor_profile_id: Option<ExecutorProfileId>, // Executor profile for this task
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskWithAttemptStatus {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub parent_task_attempt: Option<Uuid>,
    pub repo_path: Option<String>,
    pub executor_profile_id: Option<ExecutorProfileId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub has_in_progress_attempt: bool,
    pub has_merged_attempt: bool,
    pub last_attempt_failed: bool,
    pub executor: String,
}

#[derive(Debug, Deserialize, TS)]
pub struct CreateTask {
    pub project_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub parent_task_attempt: Option<Uuid>,
    pub repo_path: Option<String>,
    pub executor_profile_id: Option<ExecutorProfileId>,
    pub image_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Deserialize, TS)]
pub struct UpdateTask {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub parent_task_attempt: Option<Uuid>,
    pub repo_path: Option<String>,
    pub executor_profile_id: Option<ExecutorProfileId>,
    pub image_ids: Option<Vec<Uuid>>,
}

impl FromRow<'_, SqliteRow> for Task {
    fn from_row(row: &SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Task {
            id: row.try_get("id")?,
            project_id: row.try_get("project_id")?,
            title: row.try_get("title")?,
            description: row.try_get("description")?,
            status: row.try_get("status")?,
            parent_task_attempt: row.try_get("parent_task_attempt")?,
            repo_path: row.try_get("repo_path")?,
            executor_profile_id: Self::executor_profile_from_json(row.try_get("executor_profile_id")?),
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

impl Task {
    /// Convert JSON string to ExecutorProfileId
    fn executor_profile_from_json(json_str: Option<String>) -> Option<ExecutorProfileId> {
        json_str.and_then(|s| serde_json::from_str(&s).ok())
    }

    /// Convert ExecutorProfileId to JSON string
    fn executor_profile_to_json(profile: &Option<ExecutorProfileId>) -> Option<String> {
        profile.as_ref().and_then(|p| serde_json::to_string(p).ok())
    }

    pub fn to_prompt(&self) -> String {
        if let Some(description) = &self.description {
            format!("Title: {}\n\nDescription:{}", &self.title, description)
        } else {
            self.title.clone()
        }
    }

    pub async fn parent_project(&self, pool: &SqlitePool) -> Result<Option<Project>, sqlx::Error> {
        Project::find_by_id(pool, self.project_id).await
    }

    pub async fn find_by_project_id_with_attempt_status(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Vec<TaskWithAttemptStatus>, sqlx::Error> {
        let records = sqlx::query!(
            r#"SELECT
  t.id                            AS "id!: Uuid",
  t.project_id                    AS "project_id!: Uuid",
  t.title,
  t.description,
  t.status                        AS "status!: TaskStatus",
  t.parent_task_attempt           AS "parent_task_attempt: Uuid",
  t.repo_path,
  t.executor_profile_id           AS "executor_profile_id: String",
  t.created_at                    AS "created_at!: DateTime<Utc>",
  t.updated_at                    AS "updated_at!: DateTime<Utc>",

  CASE WHEN EXISTS (
    SELECT 1
      FROM task_attempts ta
      JOIN execution_processes ep
        ON ep.task_attempt_id = ta.id
     WHERE ta.task_id       = t.id
       AND ep.status        = 'running'
       AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
     LIMIT 1
  ) THEN 1 ELSE 0 END            AS "has_in_progress_attempt!: i64",
  
  CASE WHEN (
    SELECT ep.status
      FROM task_attempts ta
      JOIN execution_processes ep
        ON ep.task_attempt_id = ta.id
     WHERE ta.task_id       = t.id
     AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
     ORDER BY ep.created_at DESC
     LIMIT 1
  ) IN ('failed','killed') THEN 1 ELSE 0 END
                                 AS "last_attempt_failed!: i64",

  ( SELECT ta.executor
      FROM task_attempts ta
      WHERE ta.task_id = t.id
     ORDER BY ta.created_at DESC
      LIMIT 1
    )                               AS "executor!: String"

FROM tasks t
WHERE t.project_id = $1
ORDER BY t.created_at DESC"#,
            project_id
        )
        .fetch_all(pool)
        .await?;

        let tasks = records
            .into_iter()
            .map(|rec| TaskWithAttemptStatus {
                id: rec.id,
                project_id: rec.project_id,
                title: rec.title,
                description: rec.description,
                status: rec.status,
                parent_task_attempt: rec.parent_task_attempt,
                repo_path: rec.repo_path,
                executor_profile_id: Self::executor_profile_from_json(rec.executor_profile_id),
                created_at: rec.created_at,
                updated_at: rec.updated_at,
                has_in_progress_attempt: rec.has_in_progress_attempt != 0,
                has_merged_attempt: false, // TODO use merges table
                last_attempt_failed: rec.last_attempt_failed != 0,
                executor: rec.executor,
            })
            .collect();

        Ok(tasks)
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query(
            r#"SELECT id, project_id, title, description, status, parent_task_attempt, repo_path, executor_profile_id, created_at, updated_at
               FROM tasks
               WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .map(|row| Self::from_row(&row))
        .transpose()
    }

    pub async fn find_by_rowid(pool: &SqlitePool, rowid: i64) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query(
            r#"SELECT id, project_id, title, description, status, parent_task_attempt, repo_path, executor_profile_id, created_at, updated_at
               FROM tasks
               WHERE rowid = $1"#,
        )
        .bind(rowid)
        .fetch_optional(pool)
        .await?
        .map(|row| Self::from_row(&row))
        .transpose()
    }

    pub async fn find_by_id_and_project_id(
        pool: &SqlitePool,
        id: Uuid,
        project_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query(
            r#"SELECT id, project_id, title, description, status, parent_task_attempt, repo_path, executor_profile_id, created_at, updated_at
               FROM tasks
               WHERE id = $1 AND project_id = $2"#,
        )
        .bind(id)
        .bind(project_id)
        .fetch_optional(pool)
        .await?
        .map(|row| Self::from_row(&row))
        .transpose()
    }

    pub async fn create(
        pool: &SqlitePool,
        data: &CreateTask,
        task_id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        let executor_profile_json = Self::executor_profile_to_json(&data.executor_profile_id);

        let row = sqlx::query(
            r#"INSERT INTO tasks (id, project_id, title, description, status, parent_task_attempt, repo_path, executor_profile_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING id, project_id, title, description, status, parent_task_attempt, repo_path, executor_profile_id, created_at, updated_at"#,
        )
        .bind(task_id)
        .bind(data.project_id)
        .bind(&data.title)
        .bind(&data.description)
        .bind(TaskStatus::Todo)
        .bind(data.parent_task_attempt)
        .bind(&data.repo_path)
        .bind(executor_profile_json)
        .fetch_one(pool)
        .await?;

        Ok(Self::from_row(&row)?)
    }

    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        project_id: Uuid,
        title: String,
        description: Option<String>,
        status: TaskStatus,
        parent_task_attempt: Option<Uuid>,
        repo_path: Option<String>,
        executor_profile_id: Option<ExecutorProfileId>,
    ) -> Result<Self, sqlx::Error> {
        let executor_profile_json = Self::executor_profile_to_json(&executor_profile_id);

        let row = sqlx::query(
            r#"UPDATE tasks
               SET title = $3, description = $4, status = $5, parent_task_attempt = $6, repo_path = $7, executor_profile_id = $8
               WHERE id = $1 AND project_id = $2
               RETURNING id, project_id, title, description, status, parent_task_attempt, repo_path, executor_profile_id, created_at, updated_at"#,
        )
        .bind(id)
        .bind(project_id)
        .bind(&title)
        .bind(&description)
        .bind(status)
        .bind(parent_task_attempt)
        .bind(&repo_path)
        .bind(executor_profile_json)
        .fetch_one(pool)
        .await?;

        Ok(Self::from_row(&row)?)
    }

    pub async fn update_status(
        pool: &SqlitePool,
        id: Uuid,
        status: TaskStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE tasks SET status = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            status
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM tasks WHERE id = $1", id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn exists(
        pool: &SqlitePool,
        id: Uuid,
        project_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            "SELECT id as \"id!: Uuid\" FROM tasks WHERE id = $1 AND project_id = $2",
            id,
            project_id
        )
        .fetch_optional(pool)
        .await?;
        Ok(result.is_some())
    }

    pub async fn find_related_tasks_by_attempt_id(
        pool: &SqlitePool,
        attempt_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        // Find both children and parent for this attempt
        let rows = sqlx::query(
            r#"SELECT DISTINCT t.id, t.project_id, t.title, t.description, t.status, t.parent_task_attempt, t.repo_path, t.executor_profile_id, t.created_at, t.updated_at
               FROM tasks t
               WHERE (
                   -- Find children: tasks that have this attempt as parent
                   t.parent_task_attempt = $1
               ) OR (
                   -- Find parent: task that owns the parent attempt of current task
                   EXISTS (
                       SELECT 1 FROM tasks current_task
                       JOIN task_attempts parent_attempt ON current_task.parent_task_attempt = parent_attempt.id
                       WHERE parent_attempt.task_id = t.id
                   )
               )
               -- Exclude the current task itself to prevent circular references
               AND t.id != (SELECT task_id FROM task_attempts WHERE id = $1)
               ORDER BY t.created_at DESC"#,
        )
        .bind(attempt_id)
        .fetch_all(pool)
        .await?;

        rows.iter().map(|row| Self::from_row(row)).collect()
    }
}
