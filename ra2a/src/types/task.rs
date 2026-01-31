//! Task types for the A2A protocol.
//!
//! Tasks represent stateful operations or conversations between clients and agents.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{Message, Part};

/// Defines the lifecycle states of a Task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    /// Task has been submitted but not yet started.
    Submitted,
    /// Task is currently being processed.
    Working,
    /// Task requires additional input from the user.
    InputRequired,
    /// Task has completed successfully.
    Completed,
    /// Task was canceled by the user.
    Canceled,
    /// Task failed due to an error.
    Failed,
    /// Task was rejected by the agent.
    Rejected,
    /// Task requires authentication.
    AuthRequired,
    /// Task state is unknown.
    Unknown,
}

impl Default for TaskState {
    fn default() -> Self {
        Self::Submitted
    }
}

impl TaskState {
    /// Returns true if this state indicates the task is still active.
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Submitted | Self::Working | Self::InputRequired | Self::AuthRequired
        )
    }

    /// Returns true if this state indicates the task has terminated.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Canceled | Self::Failed | Self::Rejected
        )
    }
}

/// Represents the status of a task at a specific point in time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskStatus {
    /// The current state of the task's lifecycle.
    pub state: TaskState,
    /// An optional message providing more details about the current status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
    /// An ISO 8601 datetime string indicating when this status was recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

impl TaskStatus {
    /// Creates a new task status with the given state.
    pub fn new(state: TaskState) -> Self {
        Self {
            state,
            message: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Creates a new task status with a message.
    pub fn with_message(state: TaskState, message: Message) -> Self {
        Self {
            state,
            message: Some(message),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Creates a submitted status.
    pub fn submitted() -> Self {
        Self::new(TaskState::Submitted)
    }

    /// Creates a working status.
    pub fn working() -> Self {
        Self::new(TaskState::Working)
    }

    /// Creates a completed status.
    pub fn completed() -> Self {
        Self::new(TaskState::Completed)
    }

    /// Creates a failed status with an error message.
    pub fn failed(error: impl Into<String>) -> Self {
        Self::with_message(TaskState::Failed, Message::agent(vec![Part::text(error)]))
    }

    /// Creates an input-required status.
    pub fn input_required() -> Self {
        Self::new(TaskState::InputRequired)
    }
}

impl Default for TaskStatus {
    fn default() -> Self {
        Self::submitted()
    }
}

/// Represents a single, stateful operation or conversation between a client and an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    /// A unique identifier (UUID) for the task.
    pub id: String,
    /// A unique identifier for maintaining context across related tasks.
    pub context_id: String,
    /// The current status of the task.
    pub status: TaskStatus,
    /// The type of this object (always "task").
    #[serde(default = "default_task_kind")]
    pub kind: String,
    /// An array of messages exchanged during the task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<Message>>,
    /// A collection of artifacts generated during the task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<Artifact>>,
    /// Optional metadata for extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

fn default_task_kind() -> String {
    "task".to_string()
}

impl Task {
    /// Creates a new task with the given ID and context ID.
    pub fn new(id: impl Into<String>, context_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            context_id: context_id.into(),
            status: TaskStatus::submitted(),
            kind: "task".to_string(),
            history: None,
            artifacts: None,
            metadata: None,
        }
    }

    /// Creates a new task with auto-generated IDs.
    pub fn create() -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let context_id = uuid::Uuid::new_v4().to_string();
        Self::new(id, context_id)
    }

    /// Sets the status of this task.
    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = status;
        self
    }

    /// Sets the history for this task.
    pub fn with_history(mut self, history: Vec<Message>) -> Self {
        self.history = Some(history);
        self
    }

    /// Adds a message to the task's history.
    pub fn add_message(&mut self, message: Message) {
        if let Some(ref mut history) = self.history {
            history.push(message);
        } else {
            self.history = Some(vec![message]);
        }
    }

    /// Sets the artifacts for this task.
    pub fn with_artifacts(mut self, artifacts: Vec<Artifact>) -> Self {
        self.artifacts = Some(artifacts);
        self
    }

    /// Adds an artifact to the task.
    pub fn add_artifact(&mut self, artifact: Artifact) {
        if let Some(ref mut artifacts) = self.artifacts {
            artifacts.push(artifact);
        } else {
            self.artifacts = Some(vec![artifact]);
        }
    }

    /// Sets the metadata for this task.
    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Returns true if the task is in an active state.
    pub fn is_active(&self) -> bool {
        self.status.state.is_active()
    }

    /// Returns true if the task is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        self.status.state.is_terminal()
    }

    /// Returns the current state of the task.
    pub fn state(&self) -> TaskState {
        self.status.state
    }
}

impl Default for Task {
    fn default() -> Self {
        Self::create()
    }
}

/// Represents a file, data structure, or other resource generated by an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Artifact {
    /// A unique identifier for the artifact within the task.
    pub artifact_id: String,
    /// An array of content parts that make up the artifact.
    pub parts: Vec<Part>,
    /// An optional name for the artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// An optional description of the artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The URIs of extensions relevant to this artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<String>>,
    /// Optional metadata for extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl Artifact {
    /// Creates a new artifact with the given ID and parts.
    pub fn new(artifact_id: impl Into<String>, parts: Vec<Part>) -> Self {
        Self {
            artifact_id: artifact_id.into(),
            parts,
            name: None,
            description: None,
            extensions: None,
            metadata: None,
        }
    }

    /// Creates a new artifact with an auto-generated ID.
    pub fn create(parts: Vec<Part>) -> Self {
        Self::new(uuid::Uuid::new_v4().to_string(), parts)
    }

    /// Sets the name for this artifact.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the description for this artifact.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// An event sent by the agent to notify the client of a status update.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskStatusUpdateEvent {
    /// The ID of the task that was updated.
    pub task_id: String,
    /// The context ID associated with the task.
    pub context_id: String,
    /// The new status of the task.
    pub status: TaskStatus,
    /// If true, this is the final event in the stream.
    pub r#final: bool,
    /// The type of this event (always "status-update").
    #[serde(default = "default_status_update_kind")]
    pub kind: String,
    /// Optional metadata for extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

fn default_status_update_kind() -> String {
    "status-update".to_string()
}

impl TaskStatusUpdateEvent {
    /// Creates a new status update event.
    pub fn new(
        task_id: impl Into<String>,
        context_id: impl Into<String>,
        status: TaskStatus,
        r#final: bool,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            context_id: context_id.into(),
            status,
            r#final,
            kind: "status-update".to_string(),
            metadata: None,
        }
    }
}

/// An event sent by the agent to notify the client of an artifact update.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskArtifactUpdateEvent {
    /// The ID of the task this artifact belongs to.
    pub task_id: String,
    /// The context ID associated with the task.
    pub context_id: String,
    /// The artifact that was generated or updated.
    pub artifact: Artifact,
    /// The type of this event (always "artifact-update").
    #[serde(default = "default_artifact_update_kind")]
    pub kind: String,
    /// If true, the content should be appended to a previous artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub append: Option<bool>,
    /// If true, this is the final chunk of the artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_chunk: Option<bool>,
    /// Optional metadata for extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// A unified event type representing all possible streaming events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum StreamingEvent {
    /// A status update event.
    StatusUpdate(TaskStatusUpdateEvent),
    /// An artifact update event.
    ArtifactUpdate(TaskArtifactUpdateEvent),
}

impl StreamingEvent {
    /// Returns true if this is a final event.
    pub fn is_final(&self) -> bool {
        match self {
            Self::StatusUpdate(e) => e.r#final,
            Self::ArtifactUpdate(_) => false,
        }
    }

    /// Returns the task ID from this event.
    pub fn task_id(&self) -> &str {
        match self {
            Self::StatusUpdate(e) => &e.task_id,
            Self::ArtifactUpdate(e) => &e.task_id,
        }
    }

    /// Returns the context ID from this event.
    pub fn context_id(&self) -> &str {
        match self {
            Self::StatusUpdate(e) => &e.context_id,
            Self::ArtifactUpdate(e) => &e.context_id,
        }
    }
}

/// Represents a historical status entry for state transition tracking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskStatusHistoryEntry {
    /// The status at this point in time.
    pub status: TaskStatus,
    /// ISO 8601 timestamp when this status was recorded.
    pub timestamp: String,
}

impl TaskStatusHistoryEntry {
    /// Creates a new history entry with current timestamp.
    pub fn new(status: TaskStatus) -> Self {
        Self {
            status,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

fn default_artifact_update_kind() -> String {
    "artifact-update".to_string()
}

impl TaskArtifactUpdateEvent {
    /// Creates a new artifact update event.
    pub fn new(
        task_id: impl Into<String>,
        context_id: impl Into<String>,
        artifact: Artifact,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            context_id: context_id.into(),
            artifact,
            kind: "artifact-update".to_string(),
            append: None,
            last_chunk: None,
            metadata: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_state_is_active() {
        assert!(TaskState::Submitted.is_active());
        assert!(TaskState::Working.is_active());
        assert!(!TaskState::Completed.is_active());
        assert!(!TaskState::Failed.is_active());
    }

    #[test]
    fn test_task_state_is_terminal() {
        assert!(TaskState::Completed.is_terminal());
        assert!(TaskState::Failed.is_terminal());
        assert!(!TaskState::Submitted.is_terminal());
        assert!(!TaskState::Working.is_terminal());
    }

    #[test]
    fn test_task_creation() {
        let task = Task::create();
        assert!(!task.id.is_empty());
        assert!(!task.context_id.is_empty());
        assert_eq!(task.state(), TaskState::Submitted);
    }

    #[test]
    fn test_task_serialization() {
        let task = Task::new("task-123", "ctx-456");
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"id\":\"task-123\""));
        assert!(json.contains("\"kind\":\"task\""));
    }
}
