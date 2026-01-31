//! Server-side context management.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::types::{Task, TaskPushNotificationConfig};

/// Thread-safe storage for managing request context.
#[derive(Debug, Default)]
pub struct RequestContext {
    /// Request-scoped data.
    data: HashMap<String, serde_json::Value>,
}

impl RequestContext {
    /// Creates a new request context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets a value from the context.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.data.get(key)
    }

    /// Sets a value in the context.
    pub fn set(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.data.insert(key.into(), value);
    }

    /// Removes a value from the context.
    pub fn remove(&mut self, key: &str) -> Option<serde_json::Value> {
        self.data.remove(key)
    }
}

/// Server state shared across all request handlers.
#[derive(Debug)]
pub struct ServerState<E> {
    /// The agent executor.
    pub executor: Arc<E>,
    /// Task storage.
    pub tasks: Arc<RwLock<HashMap<String, Task>>>,
    /// Push notification config storage (task_id -> configs).
    pub push_configs: Arc<RwLock<HashMap<String, Vec<TaskPushNotificationConfig>>>>,
}

impl<E> ServerState<E> {
    /// Creates a new server state.
    pub fn new(executor: E) -> Self {
        Self {
            executor: Arc::new(executor),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            push_configs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Gets a task by ID.
    pub async fn get_task(&self, task_id: &str) -> Option<Task> {
        let tasks = self.tasks.read().await;
        tasks.get(task_id).cloned()
    }

    /// Stores a task.
    pub async fn store_task(&self, task: Task) {
        let mut tasks = self.tasks.write().await;
        tasks.insert(task.id.clone(), task);
    }

    /// Removes a task.
    pub async fn remove_task(&self, task_id: &str) -> Option<Task> {
        let mut tasks = self.tasks.write().await;
        tasks.remove(task_id)
    }

    /// Lists all task IDs.
    pub async fn list_task_ids(&self) -> Vec<String> {
        let tasks = self.tasks.read().await;
        tasks.keys().cloned().collect()
    }

    /// Stores a push notification config.
    pub async fn store_push_config(&self, config: TaskPushNotificationConfig) {
        let mut configs = self.push_configs.write().await;
        let task_configs = configs
            .entry(config.task_id.clone())
            .or_insert_with(Vec::new);

        // Replace if same config_id exists, otherwise add
        if let Some(ref config_id) = config.push_notification_config.id {
            if let Some(pos) = task_configs
                .iter()
                .position(|c| c.push_notification_config.id.as_ref() == Some(config_id))
            {
                task_configs[pos] = config;
                return;
            }
        }
        task_configs.push(config);
    }

    /// Gets a push notification config.
    pub async fn get_push_config(
        &self,
        task_id: &str,
        config_id: Option<&str>,
    ) -> Option<TaskPushNotificationConfig> {
        let configs = self.push_configs.read().await;
        let task_configs = configs.get(task_id)?;

        match config_id {
            Some(id) => task_configs
                .iter()
                .find(|c| c.push_notification_config.id.as_ref().map(|s| s.as_str()) == Some(id))
                .cloned(),
            None => task_configs.first().cloned(),
        }
    }

    /// Lists all push notification configs for a task.
    pub async fn list_push_configs(&self, task_id: &str) -> Vec<TaskPushNotificationConfig> {
        let configs = self.push_configs.read().await;
        configs.get(task_id).cloned().unwrap_or_default()
    }

    /// Deletes a push notification config.
    pub async fn delete_push_config(&self, task_id: &str, config_id: &str) {
        let mut configs = self.push_configs.write().await;
        if let Some(task_configs) = configs.get_mut(task_id) {
            task_configs.retain(|c| {
                c.push_notification_config.id.as_ref().map(|s| s.as_str()) != Some(config_id)
            });
        }
    }
}

impl<E> Clone for ServerState<E> {
    fn clone(&self) -> Self {
        Self {
            executor: Arc::clone(&self.executor),
            tasks: Arc::clone(&self.tasks),
            push_configs: Arc::clone(&self.push_configs),
        }
    }
}
