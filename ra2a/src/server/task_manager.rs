//! Task management for A2A server.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::{JsonRpcError, Result};
use crate::types::{Artifact, Message, Task, TaskState, TaskStatus};

/// Manages task lifecycle and storage.
#[derive(Debug)]
pub struct TaskManager {
    /// In-memory task storage.
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    /// Maximum number of tasks to keep in memory.
    max_tasks: usize,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl TaskManager {
    /// Creates a new task manager with the specified maximum capacity.
    pub fn new(max_tasks: usize) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            max_tasks,
        }
    }

    /// Creates a new task with auto-generated IDs.
    pub async fn create_task(&self) -> Task {
        let task = Task::create();
        self.store_task(task.clone()).await;
        task
    }

    /// Creates a new task with the specified IDs.
    pub async fn create_task_with_ids(
        &self,
        task_id: impl Into<String>,
        context_id: impl Into<String>,
    ) -> Task {
        let task = Task::new(task_id, context_id);
        self.store_task(task.clone()).await;
        task
    }

    /// Stores a task.
    pub async fn store_task(&self, task: Task) {
        let mut tasks = self.tasks.write().await;

        // Evict oldest tasks if at capacity
        if tasks.len() >= self.max_tasks && !tasks.contains_key(&task.id) {
            // Simple eviction: remove the first key found
            if let Some(key) = tasks.keys().next().cloned() {
                tasks.remove(&key);
            }
        }

        tasks.insert(task.id.clone(), task);
    }

    /// Gets a task by ID.
    pub async fn get_task(&self, task_id: &str) -> Option<Task> {
        let tasks = self.tasks.read().await;
        tasks.get(task_id).cloned()
    }

    /// Gets a task by ID, returning an error if not found.
    pub async fn require_task(&self, task_id: &str) -> Result<Task> {
        self.get_task(task_id)
            .await
            .ok_or_else(|| JsonRpcError::task_not_found(task_id).into())
    }

    /// Updates a task's status.
    pub async fn update_status(&self, task_id: &str, status: TaskStatus) -> Result<Task> {
        let mut tasks = self.tasks.write().await;
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| JsonRpcError::task_not_found(task_id))?;

        task.status = status;
        Ok(task.clone())
    }

    /// Adds a message to a task's history.
    pub async fn add_message(&self, task_id: &str, message: Message) -> Result<Task> {
        let mut tasks = self.tasks.write().await;
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| JsonRpcError::task_not_found(task_id))?;

        task.add_message(message);
        Ok(task.clone())
    }

    /// Adds an artifact to a task.
    pub async fn add_artifact(&self, task_id: &str, artifact: Artifact) -> Result<Task> {
        let mut tasks = self.tasks.write().await;
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| JsonRpcError::task_not_found(task_id))?;

        task.add_artifact(artifact);
        Ok(task.clone())
    }

    /// Cancels a task.
    pub async fn cancel_task(&self, task_id: &str) -> Result<Task> {
        let mut tasks = self.tasks.write().await;
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| JsonRpcError::task_not_found(task_id))?;

        if task.status.state.is_terminal() {
            return Err(JsonRpcError::task_not_cancelable(task_id).into());
        }

        task.status = TaskStatus::new(TaskState::Canceled);
        Ok(task.clone())
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

    /// Returns the number of tasks.
    pub async fn task_count(&self) -> usize {
        let tasks = self.tasks.read().await;
        tasks.len()
    }

    /// Clears all tasks.
    pub async fn clear(&self) {
        let mut tasks = self.tasks.write().await;
        tasks.clear();
    }
}

impl Clone for TaskManager {
    fn clone(&self) -> Self {
        Self {
            tasks: Arc::clone(&self.tasks),
            max_tasks: self.max_tasks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_task() {
        let manager = TaskManager::default();
        let task = manager.create_task().await;

        assert!(!task.id.is_empty());
        assert_eq!(task.status.state, TaskState::Submitted);
    }

    #[tokio::test]
    async fn test_get_task() {
        let manager = TaskManager::default();
        let task = manager.create_task().await;
        let task_id = task.id.clone();

        let retrieved = manager.get_task(&task_id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, task_id);
    }

    #[tokio::test]
    async fn test_cancel_task() {
        let manager = TaskManager::default();
        let task = manager.create_task().await;
        let task_id = task.id.clone();

        let canceled = manager.cancel_task(&task_id).await.unwrap();
        assert_eq!(canceled.status.state, TaskState::Canceled);
    }

    #[tokio::test]
    async fn test_task_eviction() {
        let manager = TaskManager::new(2);

        let _task1 = manager.create_task().await;
        let _task2 = manager.create_task().await;
        let _task3 = manager.create_task().await;

        // With max_tasks = 2, one task should have been evicted
        assert_eq!(manager.task_count().await, 2);
    }
}
