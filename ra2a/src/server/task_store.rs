//! Task storage traits and implementations.
//!
//! Defines the interface for persisting and retrieving Task objects.

use async_trait::async_trait;

use crate::error::Result;
use crate::types::Task;

/// Agent Task Store interface.
///
/// Defines the methods for persisting and retrieving `Task` objects.
#[async_trait]
pub trait TaskStore: Send + Sync {
    /// Saves or updates a task in the store.
    async fn save(&self, task: &Task) -> Result<()>;

    /// Retrieves a task from the store by ID.
    async fn get(&self, task_id: &str) -> Result<Option<Task>>;

    /// Deletes a task from the store by ID.
    async fn delete(&self, task_id: &str) -> Result<()>;

    /// Lists all task IDs in the store.
    async fn list_ids(&self) -> Result<Vec<String>>;
}

/// In-memory implementation of TaskStore.
#[derive(Debug, Default)]
pub struct InMemoryTaskStore {
    tasks: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, Task>>>,
}

impl InMemoryTaskStore {
    /// Creates a new in-memory task store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TaskStore for InMemoryTaskStore {
    async fn save(&self, task: &Task) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        tasks.insert(task.id.clone(), task.clone());
        Ok(())
    }

    async fn get(&self, task_id: &str) -> Result<Option<Task>> {
        let tasks = self.tasks.read().await;
        Ok(tasks.get(task_id).cloned())
    }

    async fn delete(&self, task_id: &str) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        tasks.remove(task_id);
        Ok(())
    }

    async fn list_ids(&self) -> Result<Vec<String>> {
        let tasks = self.tasks.read().await;
        Ok(tasks.keys().cloned().collect())
    }
}

/// SQL-based task store using SQLx (requires `sql` feature).
///
/// This module provides database-backed implementations of `TaskStore`
/// using SQLx for SQLite, PostgreSQL, and MySQL.
#[cfg(any(feature = "sqlite", feature = "postgresql", feature = "mysql"))]
pub mod sql {
    use super::*;
    use crate::types::{TaskState, TaskStatus};

    /// SQL table schema for tasks (SQLite compatible).
    pub const TASK_TABLE_SCHEMA_SQLITE: &str = r#"
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
    "#;

    /// SQL table schema for tasks (PostgreSQL compatible).
    pub const TASK_TABLE_SCHEMA_POSTGRES: &str = r#"
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
    "#;

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
        /// Converts a Task to a TaskRow for storage.
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

        /// Converts a TaskRow back to a Task.
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

    /// Converts TaskState to a kebab-case string for storage.
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

    /// Converts a string to TaskState.
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
                use super::*;
                use sqlx::Row;

                type DbPool = $Pool;

                fn db_err(e: sqlx::Error) -> crate::error::A2AError {
                    crate::error::A2AError::Database(e.to_string())
                }

                /// SQL-backed task store.
                #[derive(Debug, Clone)]
                pub struct $TaskStore {
                    pool: DbPool,
                }

                impl $TaskStore {
                    /// Creates a new store with the given connection pool.
                    pub fn new(pool: DbPool) -> Self {
                        Self { pool }
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
                    async fn save(&self, task: &Task) -> Result<()> {
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
                        Ok(())
                    }

                    async fn get(&self, task_id: &str) -> Result<Option<Task>> {
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
                                Ok(Some(task_row.to_task()?))
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

                    async fn list_ids(&self) -> Result<Vec<String>> {
                        let rows = sqlx::query("SELECT id FROM a2a_tasks")
                            .fetch_all(&self.pool)
                            .await
                            .map_err(db_err)?;
                        Ok(rows.iter().map(|r| r.get("id")).collect())
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
        task_save_sql: r#"
            INSERT INTO a2a_tasks (id, context_id, status_state, status_message, status_timestamp, history, artifacts, metadata, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                context_id = excluded.context_id, status_state = excluded.status_state,
                status_message = excluded.status_message, status_timestamp = excluded.status_timestamp,
                history = excluded.history, artifacts = excluded.artifacts,
                metadata = excluded.metadata, updated_at = CURRENT_TIMESTAMP
        "#,
        task_get_sql: "SELECT id, context_id, status_state, status_message, status_timestamp, history, artifacts, metadata FROM a2a_tasks WHERE id = ?",
        task_delete_sql: "DELETE FROM a2a_tasks WHERE id = ?",
    }

    impl_sql_task_store! {
        mod_name: postgres,
        feature: "postgresql",
        pool_type: sqlx::PgPool,
        task_store_name: PostgresTaskStore,
        task_schema: TASK_TABLE_SCHEMA_POSTGRES,
        task_save_sql: r#"
            INSERT INTO a2a_tasks (id, context_id, status_state, status_message, status_timestamp, history, artifacts, metadata, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
            ON CONFLICT(id) DO UPDATE SET
                context_id = EXCLUDED.context_id, status_state = EXCLUDED.status_state,
                status_message = EXCLUDED.status_message, status_timestamp = EXCLUDED.status_timestamp,
                history = EXCLUDED.history, artifacts = EXCLUDED.artifacts,
                metadata = EXCLUDED.metadata, updated_at = NOW()
        "#,
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

        store.save(&task).await.unwrap();

        let retrieved = store.get("task-1").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, "task-1");

        store.delete("task-1").await.unwrap();
        let deleted = store.get("task-1").await.unwrap();
        assert!(deleted.is_none());
    }
}
