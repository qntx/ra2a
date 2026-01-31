//! Client-side task manager for tracking task state.
//!
//! Manages the client's view of a task, processing streaming events
//! and maintaining the current task state.

use crate::error::{A2AError, Result};
use crate::types::{
    Artifact, Message, Task, TaskArtifactUpdateEvent, TaskState, TaskStatusUpdateEvent,
};

use super::transports::StreamEvent;

/// Manages a task's state on the client side.
///
/// Processes streaming events and maintains an up-to-date view of the task.
#[derive(Debug, Clone)]
pub struct ClientTaskManager {
    /// The current task state.
    task: Option<Task>,
    /// Whether we've received a final event.
    is_complete: bool,
}

impl ClientTaskManager {
    /// Creates a new empty task manager.
    pub fn new() -> Self {
        Self {
            task: None,
            is_complete: false,
        }
    }

    /// Creates a task manager with an initial task.
    pub fn with_task(task: Task) -> Self {
        Self {
            task: Some(task),
            is_complete: false,
        }
    }

    /// Processes a stream event and updates the task state.
    pub fn process_event(&mut self, event: &StreamEvent) -> Result<()> {
        match event {
            StreamEvent::Task(task) => {
                self.task = Some(task.clone());
                if task.status.state.is_terminal() {
                    self.is_complete = true;
                }
            }
            StreamEvent::Message(msg) => {
                self.add_message_to_history(msg.clone());
            }
            StreamEvent::StatusUpdate(update) => {
                self.process_status_update(update)?;
            }
            StreamEvent::ArtifactUpdate(update) => {
                self.process_artifact_update(update)?;
            }
        }
        Ok(())
    }

    /// Processes a status update event.
    fn process_status_update(&mut self, event: &TaskStatusUpdateEvent) -> Result<()> {
        if let Some(ref mut task) = self.task {
            // Verify task and context IDs match
            if task.id != event.task_id {
                return Err(A2AError::InvalidConfig(format!(
                    "Task ID mismatch: expected {}, got {}",
                    task.id, event.task_id
                )));
            }

            // Update task status
            task.status = event.status.clone();

            // Add status message to history if present
            if let Some(ref msg) = event.status.message {
                self.add_message_to_history(msg.clone());
            }

            // Mark as complete if final
            if event.r#final {
                self.is_complete = true;
            }
        } else {
            // Create a new task from the event
            self.task = Some(
                Task::new(&event.task_id, &event.context_id).with_status(event.status.clone()),
            );

            if event.r#final {
                self.is_complete = true;
            }
        }
        Ok(())
    }

    /// Processes an artifact update event.
    fn process_artifact_update(&mut self, event: &TaskArtifactUpdateEvent) -> Result<()> {
        if let Some(ref mut task) = self.task {
            // Verify task ID matches
            if task.id != event.task_id {
                return Err(A2AError::InvalidConfig(format!(
                    "Task ID mismatch: expected {}, got {}",
                    task.id, event.task_id
                )));
            }

            let artifact = &event.artifact;

            if event.append.unwrap_or(false) {
                // Append to existing artifact
                self.append_to_artifact(&artifact.artifact_id, &artifact.parts);
            } else {
                // Add or replace artifact
                self.add_or_replace_artifact(artifact.clone());
            }
        } else {
            // Create a new task from the event
            let mut task = Task::new(&event.task_id, &event.context_id);
            task.artifacts = Some(vec![event.artifact.clone()]);
            self.task = Some(task);
        }
        Ok(())
    }

    /// Adds a message to the task's history.
    fn add_message_to_history(&mut self, message: Message) {
        if let Some(ref mut task) = self.task {
            task.add_message(message);
        }
    }

    /// Appends parts to an existing artifact.
    fn append_to_artifact(&mut self, artifact_id: &str, parts: &[crate::types::Part]) {
        if let Some(ref mut task) = self.task {
            if let Some(ref mut artifacts) = task.artifacts {
                if let Some(artifact) = artifacts.iter_mut().find(|a| a.artifact_id == artifact_id)
                {
                    artifact.parts.extend(parts.iter().cloned());
                }
            }
        }
    }

    /// Adds or replaces an artifact in the task.
    fn add_or_replace_artifact(&mut self, artifact: Artifact) {
        if let Some(ref mut task) = self.task {
            if let Some(ref mut artifacts) = task.artifacts {
                if let Some(existing) = artifacts
                    .iter_mut()
                    .find(|a| a.artifact_id == artifact.artifact_id)
                {
                    *existing = artifact;
                } else {
                    artifacts.push(artifact);
                }
            } else {
                task.artifacts = Some(vec![artifact]);
            }
        }
    }

    /// Returns a reference to the current task, if any.
    pub fn task(&self) -> Option<&Task> {
        self.task.as_ref()
    }

    /// Returns a mutable reference to the current task, if any.
    pub fn task_mut(&mut self) -> Option<&mut Task> {
        self.task.as_mut()
    }

    /// Takes ownership of the current task.
    pub fn take_task(&mut self) -> Option<Task> {
        self.task.take()
    }

    /// Returns true if the task has completed (received final event).
    pub fn is_complete(&self) -> bool {
        self.is_complete
    }

    /// Returns the current task state, if a task exists.
    pub fn state(&self) -> Option<TaskState> {
        self.task.as_ref().map(|t| t.status.state)
    }

    /// Returns true if the task is in an active (non-terminal) state.
    pub fn is_active(&self) -> bool {
        self.task
            .as_ref()
            .map(|t| t.status.state.is_active())
            .unwrap_or(false)
    }

    /// Resets the task manager to its initial state.
    pub fn reset(&mut self) {
        self.task = None;
        self.is_complete = false;
    }
}

impl Default for ClientTaskManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TaskStatus;

    #[test]
    fn test_new_manager() {
        let manager = ClientTaskManager::new();
        assert!(manager.task().is_none());
        assert!(!manager.is_complete());
    }

    #[test]
    fn test_process_task_event() {
        let mut manager = ClientTaskManager::new();
        let task = Task::new("task-1", "ctx-1");

        manager
            .process_event(&StreamEvent::Task(task.clone()))
            .unwrap();

        assert!(manager.task().is_some());
        assert_eq!(manager.task().unwrap().id, "task-1");
    }

    #[test]
    fn test_process_status_update() {
        let mut manager = ClientTaskManager::new();
        let task = Task::new("task-1", "ctx-1");
        manager.task = Some(task);

        let update = TaskStatusUpdateEvent::new("task-1", "ctx-1", TaskStatus::working(), false);

        manager
            .process_event(&StreamEvent::StatusUpdate(update))
            .unwrap();

        assert_eq!(manager.state(), Some(TaskState::Working));
        assert!(!manager.is_complete());
    }

    #[test]
    fn test_process_final_event() {
        let mut manager = ClientTaskManager::new();
        let task = Task::new("task-1", "ctx-1");
        manager.task = Some(task);

        let update = TaskStatusUpdateEvent::new("task-1", "ctx-1", TaskStatus::completed(), true);

        manager
            .process_event(&StreamEvent::StatusUpdate(update))
            .unwrap();

        assert!(manager.is_complete());
        assert_eq!(manager.state(), Some(TaskState::Completed));
    }

    #[test]
    fn test_take_task() {
        let mut manager = ClientTaskManager::with_task(Task::new("task-1", "ctx-1"));
        let task = manager.take_task();
        assert!(task.is_some());
        assert!(manager.task().is_none());
    }
}
