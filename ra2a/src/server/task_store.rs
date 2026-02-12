//! Task storage traits and implementations.
//!
//! Defines the interface for persisting and retrieving Task objects.

use async_trait::async_trait;

use crate::error::Result;
use crate::types::{ListTasksRequest, ListTasksResponse, Task, TaskVersion};
use crate::server::events::Event;

/// Agent Task Store interface.
///
/// Aligned with Go's `TaskStore` in `tasks.go`. Implementations may use
/// the `prev` version for optimistic concurrency control during updates.
#[async_trait]
pub trait TaskStore: Send + Sync {
    /// Saves or updates a task in the store.
    ///
    /// `event` is the event that triggered this save (for audit / event-sourced stores).
    /// `prev` is the previous version for optimistic concurrency control.
    /// Returns the new version after saving.
    async fn save(
        &self,
        task: &Task,
        event: Option<&Event>,
        prev: TaskVersion,
    ) -> Result<TaskVersion>;

    /// Retrieves a task and its version from the store by ID.
    ///
    /// Returns `None` if the task does not exist.
    async fn get(&self, task_id: &str) -> Result<Option<(Task, TaskVersion)>>;

    /// Deletes a task from the store by ID.
    async fn delete(&self, task_id: &str) -> Result<()>;

    /// Lists tasks matching the given query parameters.
    async fn list(&self, req: &ListTasksRequest) -> Result<ListTasksResponse>;
}

/// A versioned task entry for in-memory storage.
#[derive(Debug, Clone)]
struct VersionedTask {
    task: Task,
    version: TaskVersion,
}

/// In-memory implementation of `TaskStore`.
///
/// Uses a simple monotonic counter for version tracking.
#[derive(Debug, Default)]
pub struct InMemoryTaskStore {
    tasks: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, VersionedTask>>>,
    next_version: std::sync::Arc<std::sync::atomic::AtomicI64>,
}

impl InMemoryTaskStore {
    /// Creates a new in-memory task store.
    #[must_use] 
    pub fn new() -> Self {
        Self {
            tasks: Default::default(),
            next_version: std::sync::Arc::new(std::sync::atomic::AtomicI64::new(1)),
        }
    }
}

#[async_trait]
impl TaskStore for InMemoryTaskStore {
    async fn save(
        &self,
        task: &Task,
        _event: Option<&Event>,
        _prev: TaskVersion,
    ) -> Result<TaskVersion> {
        let ver = TaskVersion(
            self.next_version
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        );
        let mut tasks = self.tasks.write().await;
        tasks.insert(
            task.id.clone(),
            VersionedTask {
                task: task.clone(),
                version: ver,
            },
        );
        Ok(ver)
    }

    async fn get(&self, task_id: &str) -> Result<Option<(Task, TaskVersion)>> {
        let tasks = self.tasks.read().await;
        Ok(tasks
            .get(task_id)
            .map(|vt| (vt.task.clone(), vt.version)))
    }

    async fn delete(&self, task_id: &str) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        tasks.remove(task_id);
        Ok(())
    }

    async fn list(&self, req: &ListTasksRequest) -> Result<ListTasksResponse> {
        let tasks = self.tasks.read().await;
        let mut result: Vec<Task> = tasks.values().map(|vt| vt.task.clone()).collect();

        // Apply context_id filter
        if let Some(ref ctx_id) = req.context_id {
            result.retain(|t| t.context_id == *ctx_id);
        }

        // Apply status filter
        if let Some(ref state) = req.status {
            result.retain(|t| t.status.state == *state);
        }

        let total_size = result.len() as i32;
        let page_size = req.page_size.unwrap_or(50).min(100) as usize;

        // Simple offset-based pagination using page_token as numeric offset
        let offset: usize = req
            .page_token
            .as_deref()
            .and_then(|t| t.parse().ok())
            .unwrap_or(0);

        let paged: Vec<Task> = result.into_iter().skip(offset).take(page_size).collect();
        let next_offset = offset + paged.len();
        let next_page_token = if (next_offset as i32) < total_size {
            Some(next_offset.to_string())
        } else {
            None
        };

        Ok(ListTasksResponse {
            tasks: paged,
            total_size: Some(total_size),
            page_size: Some(page_size as i32),
            next_page_token,
        })
    }
}

/// SQL-based task store using `SQLx` (requires `sql` feature).
///
/// This module provides database-backed implementations of `TaskStore`
/// using `SQLx` for `SQLite`, `PostgreSQL`, and `MySQL`.
#[cfg(any(feature = "sqlite", feature = "postgresql", feature = "mysql"))]
pub mod sql {
    use super::{async_trait, Event, Task, TaskVersion, ListTasksRequest, ListTasksResponse, Result, TaskStore};
    use crate::types::{TaskState, TaskStatus};
    use std::sync::atomic::{AtomicI64, Ordering};

    /// SQL table schema for tasks (`SQLite` compatible).
    pub const TASK_TABLE_SCHEMA_SQLITE: &str = r"
        CREATE TABLE IF NOT EXISTS a2a_tasks (
            id TEXT PRIMARY KEY,
            context_id TEXT NOT NULL,
            status_state TEXT NOT NULL,
            status_message TEXT,
            status_timestamp TEXT,
            history TEXT,
            artifacts TEXT,
            metadata TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        )
    ";

    /// SQL table schema for tasks (`PostgreSQL` compatible).
    pub const TASK_TABLE_SCHEMA_POSTGRES: &str = r"
        CREATE TABLE IF NOT EXISTS a2a_tasks (
            id TEXT PRIMARY KEY,
            context_id TEXT NOT NULL,
            status_state TEXT NOT NULL,
            status_message TEXT,
            status_timestamp TEXT,
            history JSONB,
            artifacts JSONB,
            metadata JSONB,
            created_at TIMESTAMPTZ DEFAULT NOW(),
            updated_at TIMESTAMPTZ DEFAULT NOW()
        )
    ";

    /// Row representation for tasks in SQL.
    #[derive(Debug, Clone)]
    pub struct TaskRow {
        /// The unique task identifier.
        pub id: String,
        /// The context identifier for the task.
        pub context_id: String,
        /// The task state as a string (e.g., "submitted", "working").
        pub status_state: String,
        /// Optional status message serialized as JSON.
        pub status_message: Option<String>,
        /// Status timestamp in ISO 8601 format.
        pub status_timestamp: Option<String>,
        /// Task history serialized as JSON.
        pub history: Option<String>,
        /// Task artifacts serialized as JSON.
        pub artifacts: Option<String>,
        /// Task metadata serialized as JSON.
        pub metadata: Option<String>,
    }

    impl TaskRow {
        /// Converts a Task to a `TaskRow` for storage.
        #[must_use] 
        pub fn from_task(task: &Task) -> Self {
            Self {
                id: task.id.clone(),
                context_id: task.context_id.clone(),
                status_state: task_state_to_string(task.status.state),
                status_message: task
                    .status
                    .message
                    .as_ref()
                    .and_then(|m| serde_json::to_string(m).ok()),
                status_timestamp: task.status.timestamp.clone(),
                history: task
                    .history
                    .as_ref()
                    .and_then(|h| serde_json::to_string(h).ok()),
                artifacts: task
                    .artifacts
                    .as_ref()
                    .and_then(|a| serde_json::to_string(a).ok()),
                metadata: task
                    .metadata
                    .as_ref()
                    .and_then(|m| serde_json::to_string(m).ok()),
            }
        }

        /// Converts a `TaskRow` back to a Task.
        pub fn to_task(&self) -> Result<Task> {
            let state = string_to_task_state(&self.status_state);

            let mut task = Task::new(&self.id, &self.context_id);
            task.status = TaskStatus::new(state);
            task.status.timestamp = self.status_timestamp.clone();

            if let Some(ref msg_json) = self.status_message {
                task.status.message = serde_json::from_str(msg_json).ok();
            }

            if let Some(ref history_json) = self.history {
                task.history = serde_json::from_str(history_json).ok();
            }

            if let Some(ref artifacts_json) = self.artifacts {
                task.artifacts = serde_json::from_str(artifacts_json).ok();
            }

            if let Some(ref metadata_json) = self.metadata {
                task.metadata = serde_json::from_str(metadata_json).ok();
            }

            Ok(task)
        }
    }

    /// Converts `TaskState` to a kebab-case string for storage.
    fn task_state_to_string(state: TaskState) -> String {
        match state {
            TaskState::Submitted => "submitted",
            TaskState::Working => "working",
            TaskState::InputRequired => "input-required",
            TaskState::Completed => "completed",
            TaskState::Canceled => "canceled",
            TaskState::Failed => "failed",
            TaskState::Rejected => "rejected",
            TaskState::AuthRequired => "auth-required",
            TaskState::Unknown => "unknown",
        }
        .to_string()
    }

    /// Converts a string to `TaskState`.
    fn string_to_task_state(s: &str) -> TaskState {
        match s {
            "submitted" => TaskState::Submitted,
            "working" => TaskState::Working,
            "input-required" => TaskState::InputRequired,
            "completed" => TaskState::Completed,
            "canceled" => TaskState::Canceled,
            "failed" => TaskState::Failed,
            "rejected" => TaskState::Rejected,
            "auth-required" => TaskState::AuthRequired,
            _ => TaskState::Unknown,
        }
    }

    /// Generates `TaskStore` implementation for a SQL backend.
    macro_rules! impl_sql_task_store {
        (
            mod_name: $mod:ident,
            feature: $feat:literal,
            pool_type: $Pool:ty,
            task_store_name: $TaskStore:ident,
            task_schema: $task_schema:expr,
            task_save_sql: $task_save:expr,
            task_get_sql: $task_get:expr,
            task_delete_sql: $task_del:expr,
        ) => {
            #[doc = concat!("SQL-backed task store for the `", $feat, "` database backend.")]
            #[cfg(feature = $feat)]
            pub mod $mod {
                use sqlx::Row;

                use super::*;

                type DbPool = $Pool;

                fn db_err(e: sqlx::Error) -> crate::error::A2AError {
                    crate::error::A2AError::Database(e.to_string())
                }

                /// SQL-backed task store.
                #[derive(Debug, Clone)]
                pub struct $TaskStore {
                    pool: DbPool,
                    next_version: std::sync::Arc<AtomicI64>,
                }

                impl $TaskStore {
                    /// Creates a new store with the given connection pool.
                    pub fn new(pool: DbPool) -> Self {
                        Self {
                            pool,
                            next_version: std::sync::Arc::new(AtomicI64::new(1)),
                        }
                    }

                    /// Creates the required tables if they don't exist.
                    pub async fn initialize(&self) -> Result<()> {
                        sqlx::query($task_schema)
                            .execute(&self.pool)
                            .await
                            .map_err(db_err)?;
                        Ok(())
                    }
                }

                #[async_trait]
                impl TaskStore for $TaskStore {
                    async fn save(
                        &self,
                        task: &Task,
                        _event: Option<&Event>,
                        _prev: TaskVersion,
                    ) -> Result<TaskVersion> {
                        let row = TaskRow::from_task(task);
                        sqlx::query($task_save)
                            .bind(&row.id)
                            .bind(&row.context_id)
                            .bind(&row.status_state)
                            .bind(&row.status_message)
                            .bind(&row.status_timestamp)
                            .bind(&row.history)
                            .bind(&row.artifacts)
                            .bind(&row.metadata)
                            .execute(&self.pool)
                            .await
                            .map_err(db_err)?;
                        let ver = TaskVersion(self.next_version.fetch_add(1, Ordering::Relaxed));
                        Ok(ver)
                    }

                    async fn get(&self, task_id: &str) -> Result<Option<(Task, TaskVersion)>> {
                        let row = sqlx::query($task_get)
                            .bind(task_id)
                            .fetch_optional(&self.pool)
                            .await
                            .map_err(db_err)?;

                        match row {
                            Some(r) => {
                                let task_row = TaskRow {
                                    id: r.get("id"),
                                    context_id: r.get("context_id"),
                                    status_state: r.get("status_state"),
                                    status_message: r.get("status_message"),
                                    status_timestamp: r.get("status_timestamp"),
                                    history: r.get("history"),
                                    artifacts: r.get("artifacts"),
                                    metadata: r.get("metadata"),
                                };
                                Ok(Some((task_row.to_task()?, TaskVersion::MISSING)))
                            }
                            None => Ok(None),
                        }
                    }

                    async fn delete(&self, task_id: &str) -> Result<()> {
                        sqlx::query($task_del)
                            .bind(task_id)
                            .execute(&self.pool)
                            .await
                            .map_err(db_err)?;
                        Ok(())
                    }

                    async fn list(&self, _req: &ListTasksRequest) -> Result<ListTasksResponse> {
                        // Basic implementation: return all tasks without filtering
                        let rows = sqlx::query("SELECT id, context_id, status_state, status_message, status_timestamp, history, artifacts, metadata FROM a2a_tasks")
                            .fetch_all(&self.pool)
                            .await
                            .map_err(db_err)?;
                        let tasks: Vec<Task> = rows
                            .iter()
                            .filter_map(|r| {
                                let task_row = TaskRow {
                                    id: r.get("id"),
                                    context_id: r.get("context_id"),
                                    status_state: r.get("status_state"),
                                    status_message: r.get("status_message"),
                                    status_timestamp: r.get("status_timestamp"),
                                    history: r.get("history"),
                                    artifacts: r.get("artifacts"),
                                    metadata: r.get("metadata"),
                                };
                                task_row.to_task().ok()
                            })
                            .collect();
                        let total = tasks.len() as i32;
                        Ok(ListTasksResponse {
                            tasks,
                            total_size: Some(total),
                            page_size: Some(total),
                            next_page_token: None,
                        })
                    }
                }
            }

            #[cfg(feature = $feat)]
            pub use $mod::$TaskStore;
        };
    }

    impl_sql_task_store! {
        mod_name: sqlite,
        feature: "sqlite",
        pool_type: sqlx::SqlitePool,
        task_store_name: SqliteTaskStore,
        task_schema: TASK_TABLE_SCHEMA_SQLITE,
        task_save_sql: r"
            INSERT INTO a2a_tasks (id, context_id, status_state, status_message, status_timestamp, history, artifacts, metadata, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                context_id = excluded.context_id, status_state = excluded.status_state,
                status_message = excluded.status_message, status_timestamp = excluded.status_timestamp,
                history = excluded.history, artifacts = excluded.artifacts,
                metadata = excluded.metadata, updated_at = CURRENT_TIMESTAMP
        ",
        task_get_sql: "SELECT id, context_id, status_state, status_message, status_timestamp, history, artifacts, metadata FROM a2a_tasks WHERE id = ?",
        task_delete_sql: "DELETE FROM a2a_tasks WHERE id = ?",
    }

    impl_sql_task_store! {
        mod_name: postgres,
        feature: "postgresql",
        pool_type: sqlx::PgPool,
        task_store_name: PostgresTaskStore,
        task_schema: TASK_TABLE_SCHEMA_POSTGRES,
        task_save_sql: r"
            INSERT INTO a2a_tasks (id, context_id, status_state, status_message, status_timestamp, history, artifacts, metadata, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
            ON CONFLICT(id) DO UPDATE SET
                context_id = EXCLUDED.context_id, status_state = EXCLUDED.status_state,
                status_message = EXCLUDED.status_message, status_timestamp = EXCLUDED.status_timestamp,
                history = EXCLUDED.history, artifacts = EXCLUDED.artifacts,
                metadata = EXCLUDED.metadata, updated_at = NOW()
        ",
        task_get_sql: "SELECT id, context_id, status_state, status_message, status_timestamp, history::TEXT, artifacts::TEXT, metadata::TEXT FROM a2a_tasks WHERE id = $1",
        task_delete_sql: "DELETE FROM a2a_tasks WHERE id = $1",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_inmemory_task_store() {
        let store = InMemoryTaskStore::new();
        let task = Task::new("task-1", "ctx-1");

        let ver = store
            .save(&task, None, TaskVersion::MISSING)
            .await
            .unwrap();
        assert!(ver.0 > 0);

        let retrieved = store.get("task-1").await.unwrap();
        assert!(retrieved.is_some());
        let (t, v) = retrieved.unwrap();
        assert_eq!(t.id, "task-1");
        assert_eq!(v, ver);

        store.delete("task-1").await.unwrap();
        let deleted = store.get("task-1").await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_inmemory_task_store_list() {
        let store = InMemoryTaskStore::new();
        let t1 = Task::new("task-1", "ctx-a");
        let t2 = Task::new("task-2", "ctx-a");
        let t3 = Task::new("task-3", "ctx-b");

        store.save(&t1, None, TaskVersion::MISSING).await.unwrap();
        store.save(&t2, None, TaskVersion::MISSING).await.unwrap();
        store.save(&t3, None, TaskVersion::MISSING).await.unwrap();

        // List all
        let resp = store
            .list(&ListTasksRequest::default())
            .await
            .unwrap();
        assert_eq!(resp.total_size, Some(3));

        // Filter by context_id
        let resp = store
            .list(&ListTasksRequest {
                context_id: Some("ctx-a".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(resp.total_size, Some(2));
    }
}
