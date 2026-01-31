//! Task updater utility for managing task state updates.
//!
//! Provides a convenient API for updating task status, artifacts, and history.
//! Mirrors Python's task update utilities.

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::Result;
use crate::types::{Artifact, Message, Part, Task, TaskState};

use super::events::{Event, EventQueue};
use super::task_store::TaskStore;

/// A utility for updating task state and emitting events.
///
/// `TaskUpdater` provides a fluent API for modifying task state and
/// automatically emitting appropriate events to connected clients.
pub struct TaskUpdater<S: TaskStore> {
    task: Arc<RwLock<Task>>,
    store: Arc<S>,
    queue: Option<Arc<EventQueue>>,
}

impl<S: TaskStore> TaskUpdater<S> {
    /// Creates a new task updater.
    pub fn new(task: Task, store: Arc<S>) -> Self {
        Self {
            task: Arc::new(RwLock::new(task)),
            store,
            queue: None,
        }
    }

    /// Creates a new task updater with an event queue.
    pub fn with_queue(task: Task, store: Arc<S>, queue: Arc<EventQueue>) -> Self {
        Self {
            task: Arc::new(RwLock::new(task)),
            store,
            queue: Some(queue),
        }
    }

    /// Returns a clone of the current task.
    pub async fn task(&self) -> Task {
        self.task.read().await.clone()
    }

    /// Returns the task ID.
    pub async fn task_id(&self) -> String {
        self.task.read().await.id.clone()
    }

    /// Updates the task state.
    pub async fn set_state(&self, state: TaskState) -> Result<()> {
        {
            let mut task = self.task.write().await;
            task.status.state = state;
        }
        self.save_and_notify().await
    }

    /// Updates the task state with a message.
    pub async fn set_state_with_message(&self, state: TaskState, message: Message) -> Result<()> {
        {
            let mut task = self.task.write().await;
            task.status.state = state;
            task.status.message = Some(message.clone());

            // Add to history if not already there
            if let Some(ref mut history) = task.history {
                history.push(message);
            } else {
                task.history = Some(vec![message]);
            }
        }
        self.save_and_notify().await
    }

    /// Sets the task to working state.
    pub async fn start_working(&self) -> Result<()> {
        self.set_state(TaskState::Working).await
    }

    /// Sets the task to completed state.
    pub async fn complete(&self) -> Result<()> {
        self.set_state(TaskState::Completed).await
    }

    /// Sets the task to completed state with a message.
    pub async fn complete_with_message(&self, message: Message) -> Result<()> {
        self.set_state_with_message(TaskState::Completed, message)
            .await
    }

    /// Sets the task to failed state.
    pub async fn fail(&self, error_message: impl Into<String>) -> Result<()> {
        let message = Message::agent_text(error_message);
        self.set_state_with_message(TaskState::Failed, message)
            .await
    }

    /// Sets the task to canceled state.
    pub async fn cancel(&self) -> Result<()> {
        self.set_state(TaskState::Canceled).await
    }

    /// Sets the task to input required state.
    pub async fn require_input(&self, prompt: impl Into<String>) -> Result<()> {
        let message = Message::agent_text(prompt);
        self.set_state_with_message(TaskState::InputRequired, message)
            .await
    }

    /// Adds a message to the task.
    pub async fn add_message(&self, message: Message) -> Result<()> {
        {
            let mut task = self.task.write().await;
            task.status.message = Some(message.clone());

            if let Some(ref mut history) = task.history {
                history.push(message);
            } else {
                task.history = Some(vec![message]);
            }
        }
        self.save_and_notify().await
    }

    /// Adds a text message from the agent.
    pub async fn add_agent_message(&self, text: impl Into<String>) -> Result<()> {
        self.add_message(Message::agent_text(text)).await
    }

    /// Adds an artifact to the task.
    pub async fn add_artifact(&self, artifact: Artifact) -> Result<()> {
        {
            let mut task = self.task.write().await;
            if let Some(ref mut artifacts) = task.artifacts {
                artifacts.push(artifact);
            } else {
                task.artifacts = Some(vec![artifact]);
            }
        }
        self.save_and_notify().await
    }

    /// Adds a text artifact.
    pub async fn add_text_artifact(
        &self,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<()> {
        let artifact = Artifact {
            artifact_id: uuid::Uuid::new_v4().to_string(),
            name: Some(name.into()),
            description: None,
            parts: vec![Part::text(text)],
            metadata: None,
            extensions: None,
        };
        self.add_artifact(artifact).await
    }

    /// Updates task metadata.
    pub async fn set_metadata(
        &self,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Result<()> {
        {
            let mut task = self.task.write().await;
            if let Some(ref mut metadata) = task.metadata {
                metadata.insert(key.into(), value);
            } else {
                let mut map = std::collections::HashMap::new();
                map.insert(key.into(), value);
                task.metadata = Some(map);
            }
        }
        self.save_and_notify().await
    }

    /// Saves the task and emits events.
    async fn save_and_notify(&self) -> Result<()> {
        let task = self.task.read().await.clone();

        // Save to store
        self.store.save(&task, None).await?;

        // Emit event if queue is available
        if let Some(ref queue) = self.queue {
            let event = Event::Task(task);
            let _ = queue.send(event);
        }

        Ok(())
    }
}

/// Builder for creating task updaters.
pub struct TaskUpdaterBuilder<S: TaskStore> {
    task_id: String,
    context_id: String,
    store: Arc<S>,
    queue: Option<Arc<EventQueue>>,
}

impl<S: TaskStore> TaskUpdaterBuilder<S> {
    /// Creates a new builder.
    pub fn new(task_id: impl Into<String>, context_id: impl Into<String>, store: Arc<S>) -> Self {
        Self {
            task_id: task_id.into(),
            context_id: context_id.into(),
            store,
            queue: None,
        }
    }

    /// Sets the event queue.
    pub fn with_queue(mut self, queue: Arc<EventQueue>) -> Self {
        self.queue = Some(queue);
        self
    }

    /// Builds the task updater with a new task.
    pub fn build(self) -> TaskUpdater<S> {
        let task = Task::new(&self.task_id, &self.context_id);
        match self.queue {
            Some(queue) => TaskUpdater::with_queue(task, self.store, queue),
            None => TaskUpdater::new(task, self.store),
        }
    }

    /// Builds the task updater with an existing task.
    pub fn build_with_task(self, task: Task) -> TaskUpdater<S> {
        match self.queue {
            Some(queue) => TaskUpdater::with_queue(task, self.store, queue),
            None => TaskUpdater::new(task, self.store),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::task_store::InMemoryTaskStore;
    use super::*;

    #[tokio::test]
    async fn test_task_updater_state_changes() {
        let store = Arc::new(InMemoryTaskStore::new());
        let task = Task::new("task-1", "ctx-1");
        let updater = TaskUpdater::new(task, store.clone());

        // Start working
        updater.start_working().await.unwrap();
        let task = updater.task().await;
        assert_eq!(task.status.state, TaskState::Working);

        // Complete
        updater.complete().await.unwrap();
        let task = updater.task().await;
        assert_eq!(task.status.state, TaskState::Completed);

        // Verify stored
        let stored = store.get("task-1", None).await.unwrap();
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().status.state, TaskState::Completed);
    }

    #[tokio::test]
    async fn test_task_updater_messages() {
        let store = Arc::new(InMemoryTaskStore::new());
        let task = Task::new("task-2", "ctx-2");
        let updater = TaskUpdater::new(task, store);

        updater.add_agent_message("Processing...").await.unwrap();
        let task = updater.task().await;

        assert!(task.status.message.is_some());
        assert!(task.history.is_some());
        assert_eq!(task.history.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_task_updater_artifacts() {
        let store = Arc::new(InMemoryTaskStore::new());
        let task = Task::new("task-3", "ctx-3");
        let updater = TaskUpdater::new(task, store);

        updater
            .add_text_artifact("result.txt", "Hello, World!")
            .await
            .unwrap();
        let task = updater.task().await;

        assert!(task.artifacts.is_some());
        let artifacts = task.artifacts.unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].name, Some("result.txt".to_string()));
    }

    #[tokio::test]
    async fn test_task_updater_builder() {
        let store = Arc::new(InMemoryTaskStore::new());
        let updater = TaskUpdaterBuilder::new("task-4", "ctx-4", store).build();

        let task = updater.task().await;
        assert_eq!(task.id, "task-4");
        assert_eq!(task.context_id, "ctx-4");
    }
}
