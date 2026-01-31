//! Task storage traits and implementations.
//!
//! Defines the interface for persisting and retrieving Task objects.

use async_trait::async_trait;

use crate::error::Result;
use crate::types::Task;

use super::RequestContext;

/// Agent Task Store interface.
///
/// Defines the methods for persisting and retrieving `Task` objects.
#[async_trait]
pub trait TaskStore: Send + Sync {
    /// Saves or updates a task in the store.
    async fn save(&self, task: &Task, context: Option<&RequestContext>) -> Result<()>;

    /// Retrieves a task from the store by ID.
    async fn get(&self, task_id: &str, context: Option<&RequestContext>) -> Result<Option<Task>>;

    /// Deletes a task from the store by ID.
    async fn delete(&self, task_id: &str, context: Option<&RequestContext>) -> Result<()>;

    /// Lists all task IDs in the store.
    async fn list_ids(&self, context: Option<&RequestContext>) -> Result<Vec<String>>;
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
    async fn save(&self, task: &Task, _context: Option<&RequestContext>) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        tasks.insert(task.id.clone(), task.clone());
        Ok(())
    }

    async fn get(&self, task_id: &str, _context: Option<&RequestContext>) -> Result<Option<Task>> {
        let tasks = self.tasks.read().await;
        Ok(tasks.get(task_id).cloned())
    }

    async fn delete(&self, task_id: &str, _context: Option<&RequestContext>) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        tasks.remove(task_id);
        Ok(())
    }

    async fn list_ids(&self, _context: Option<&RequestContext>) -> Result<Vec<String>> {
        let tasks = self.tasks.read().await;
        Ok(tasks.keys().cloned().collect())
    }
}

/// Push notification configuration store interface.
///
/// Mirrors Python's `PushNotificationConfigStore` from `server/tasks/push_notification_config_store.py`.
#[async_trait]
pub trait PushNotificationConfigStore: Send + Sync {
    /// Saves a push notification configuration.
    async fn save(
        &self,
        task_id: &str,
        config: &crate::types::PushNotificationConfig,
    ) -> Result<()>;

    /// Gets a push notification configuration by task ID and config ID.
    async fn get(
        &self,
        task_id: &str,
        config_id: Option<&str>,
    ) -> Result<Option<crate::types::PushNotificationConfig>>;

    /// Lists all push notification configurations for a task.
    async fn list(&self, task_id: &str) -> Result<Vec<crate::types::PushNotificationConfig>>;

    /// Deletes a push notification configuration.
    async fn delete(&self, task_id: &str, config_id: &str) -> Result<()>;

    /// Deletes all push notification configurations for a task.
    async fn delete_all(&self, task_id: &str) -> Result<()>;

    /// Checks if a configuration exists for a task.
    async fn exists(&self, task_id: &str, config_id: Option<&str>) -> Result<bool> {
        Ok(self.get(task_id, config_id).await?.is_some())
    }

    /// Returns the count of configurations for a task.
    async fn count(&self, task_id: &str) -> Result<usize> {
        Ok(self.list(task_id).await?.len())
    }
}

/// In-memory implementation of PushNotificationConfigStore.
#[derive(Debug, Default)]
pub struct InMemoryPushNotificationConfigStore {
    configs: std::sync::Arc<
        tokio::sync::RwLock<
            std::collections::HashMap<String, Vec<crate::types::PushNotificationConfig>>,
        >,
    >,
}

impl InMemoryPushNotificationConfigStore {
    /// Creates a new in-memory push notification config store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl PushNotificationConfigStore for InMemoryPushNotificationConfigStore {
    async fn save(
        &self,
        task_id: &str,
        config: &crate::types::PushNotificationConfig,
    ) -> Result<()> {
        let mut configs = self.configs.write().await;
        let task_configs = configs.entry(task_id.to_string()).or_default();

        // Update if exists, otherwise add
        if let Some(existing) = task_configs.iter_mut().find(|c| c.id == config.id) {
            *existing = config.clone();
        } else {
            task_configs.push(config.clone());
        }
        Ok(())
    }

    async fn get(
        &self,
        task_id: &str,
        config_id: Option<&str>,
    ) -> Result<Option<crate::types::PushNotificationConfig>> {
        let configs = self.configs.read().await;
        if let Some(task_configs) = configs.get(task_id) {
            if let Some(cid) = config_id {
                return Ok(task_configs
                    .iter()
                    .find(|c| c.id.as_deref() == Some(cid))
                    .cloned());
            } else {
                return Ok(task_configs.first().cloned());
            }
        }
        Ok(None)
    }

    async fn list(&self, task_id: &str) -> Result<Vec<crate::types::PushNotificationConfig>> {
        let configs = self.configs.read().await;
        Ok(configs.get(task_id).cloned().unwrap_or_default())
    }

    async fn delete(&self, task_id: &str, config_id: &str) -> Result<()> {
        let mut configs = self.configs.write().await;
        if let Some(task_configs) = configs.get_mut(task_id) {
            task_configs.retain(|c| c.id.as_deref() != Some(config_id));
        }
        Ok(())
    }

    async fn delete_all(&self, task_id: &str) -> Result<()> {
        let mut configs = self.configs.write().await;
        configs.remove(task_id);
        Ok(())
    }
}

/// SQL-based task store using SQLx (requires `sql` feature).
#[cfg(feature = "sql")]
pub mod sql {
    #![allow(unused_imports)]
    use super::*;
    use sqlx::{Database, FromRow, Pool};

    /// SQL table schema for tasks.
    pub const TASK_TABLE_SCHEMA: &str = r#"
        CREATE TABLE IF NOT EXISTS a2a_tasks (
            id TEXT PRIMARY KEY,
            context_id TEXT NOT NULL,
            status_state TEXT NOT NULL,
            status_message TEXT,
            history TEXT,
            artifacts TEXT,
            metadata TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
    "#;

    /// SQL table schema for push notification configs.
    pub const PUSH_CONFIG_TABLE_SCHEMA: &str = r#"
        CREATE TABLE IF NOT EXISTS a2a_push_configs (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            url TEXT NOT NULL,
            token TEXT,
            authentication TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (task_id) REFERENCES a2a_tasks(id) ON DELETE CASCADE
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
                status_state: format!("{:?}", task.status.state),
                status_message: task
                    .status
                    .message
                    .as_ref()
                    .map(|m| serde_json::to_string(m).unwrap_or_default()),
                history: task
                    .history
                    .as_ref()
                    .map(|h| serde_json::to_string(h).unwrap_or_default()),
                artifacts: task
                    .artifacts
                    .as_ref()
                    .map(|a| serde_json::to_string(a).unwrap_or_default()),
                metadata: task
                    .metadata
                    .as_ref()
                    .map(|m| serde_json::to_string(m).unwrap_or_default()),
            }
        }

        /// Converts a TaskRow back to a Task.
        pub fn to_task(&self) -> Result<Task> {
            use crate::types::{TaskState, TaskStatus};

            let state = match self.status_state.as_str() {
                "Submitted" => TaskState::Submitted,
                "Working" => TaskState::Working,
                "InputRequired" => TaskState::InputRequired,
                "Completed" => TaskState::Completed,
                "Canceled" => TaskState::Canceled,
                "Failed" => TaskState::Failed,
                "Rejected" => TaskState::Rejected,
                "AuthRequired" => TaskState::AuthRequired,
                _ => TaskState::Unknown,
            };

            let mut task = Task::new(&self.id, &self.context_id);
            task.status = TaskStatus::new(state);

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_inmemory_task_store() {
        let store = InMemoryTaskStore::new();
        let task = Task::new("task-1", "ctx-1");

        store.save(&task, None).await.unwrap();

        let retrieved = store.get("task-1", None).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, "task-1");

        store.delete("task-1", None).await.unwrap();
        let deleted = store.get("task-1", None).await.unwrap();
        assert!(deleted.is_none());
    }
}
