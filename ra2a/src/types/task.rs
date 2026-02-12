//! Task types for the A2A protocol.
//!
//! Tasks represent stateful operations or conversations between clients and agents.

use std::collections::HashMap;

use serde::{Deserialize, Serialize, Serializer};

use super::{Message, Part};

/// Helper for serde: skip serializing boolean fields when false.
#[must_use] 
pub fn is_false(v: &bool) -> bool {
    !v
}

/// Version of a task stored on the server, used for optimistic concurrency control.
///
/// Aligned with Go's `TaskVersion` in `task_version.go`.
/// A value of `0` (`MISSING`) means version tracking is not active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
pub struct TaskVersion(pub i64);

impl TaskVersion {
    /// Special value indicating that version tracking is not active.
    pub const MISSING: Self = Self(0);

    /// Returns `true` if this version is newer than `other`.
    ///
    /// When either version is `MISSING`, the semantics match Go:
    /// - If `other` is missing, this is always considered "after".
    /// - If `self` is missing but `other` is not, this is NOT "after".
    #[must_use]
    pub fn after(self, other: Self) -> bool {
        if other == Self::MISSING {
            return true;
        }
        if self == Self::MISSING {
            return false;
        }
        self.0 > other.0
    }
}

impl From<i64> for TaskVersion {
    fn from(v: i64) -> Self {
        Self(v)
    }
}

/// Defines the lifecycle states of a Task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum TaskState {
    /// Task has been submitted but not yet started.
    #[default]
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


impl TaskState {
    /// Returns true if this state indicates the task is still active.
    #[must_use] 
    pub const fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Submitted | Self::Working | Self::InputRequired | Self::AuthRequired
        )
    }

    /// Returns true if this state indicates the task has terminated.
    #[must_use] 
    pub const fn is_terminal(&self) -> bool {
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
    #[must_use] 
    pub fn new(state: TaskState) -> Self {
        Self {
            state,
            message: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Creates a new task status with a message.
    #[must_use] 
    pub fn with_message(state: TaskState, message: Message) -> Self {
        Self {
            state,
            message: Some(message),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Creates a submitted status.
    #[must_use] 
    pub fn submitted() -> Self {
        Self::new(TaskState::Submitted)
    }

    /// Creates a working status.
    #[must_use] 
    pub fn working() -> Self {
        Self::new(TaskState::Working)
    }

    /// Creates a completed status.
    #[must_use] 
    pub fn completed() -> Self {
        Self::new(TaskState::Completed)
    }

    /// Creates a failed status with an error message.
    pub fn failed(error: impl Into<String>) -> Self {
        Self::with_message(TaskState::Failed, Message::agent(vec![Part::text(error)]))
    }

    /// Creates an input-required status.
    #[must_use] 
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
///
/// The `kind` field is injected during JSON serialization as `"task"` (aligned
/// with Go's `Task.MarshalJSON`). It is not stored on the struct.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// A unique identifier (UUID) for the task.
    pub id: String,
    /// A unique identifier for maintaining context across related tasks.
    pub context_id: String,
    /// The current status of the task.
    pub status: TaskStatus,
    /// An array of messages exchanged during the task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<Message>>,
    /// A collection of artifacts generated during the task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<Artifact>>,
    /// Optional metadata for extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,

    /// Ignored during deserialization; injected as `"task"` on serialization.
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    kind: Option<String>,
}

impl Serialize for Task {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct TaskWithKind<'a> {
            kind: &'static str,
            id: &'a str,
            context_id: &'a str,
            status: &'a TaskStatus,
            #[serde(skip_serializing_if = "Option::is_none")]
            history: &'a Option<Vec<Message>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            artifacts: &'a Option<Vec<Artifact>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            metadata: &'a Option<HashMap<String, serde_json::Value>>,
        }
        TaskWithKind {
            kind: "task",
            id: &self.id,
            context_id: &self.context_id,
            status: &self.status,
            history: &self.history,
            artifacts: &self.artifacts,
            metadata: &self.metadata,
        }
        .serialize(serializer)
    }
}

impl Task {
    /// Creates a new task with the given ID and context ID.
    pub fn new(id: impl Into<String>, context_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            context_id: context_id.into(),
            status: TaskStatus::submitted(),
            history: None,
            artifacts: None,
            metadata: None,
            kind: None,
        }
    }

    /// Creates a new task with auto-generated IDs.
    #[must_use] 
    pub fn create() -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let context_id = uuid::Uuid::new_v4().to_string();
        Self::new(id, context_id)
    }

    /// Sets the status of this task.
    #[must_use] 
    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = status;
        self
    }

    /// Sets the history for this task.
    #[must_use] 
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
    #[must_use] 
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
    #[must_use] 
    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Returns true if the task is in an active state.
    #[must_use] 
    pub const fn is_active(&self) -> bool {
        self.status.state.is_active()
    }

    /// Returns true if the task is in a terminal state.
    #[must_use] 
    pub const fn is_terminal(&self) -> bool {
        self.status.state.is_terminal()
    }

    /// Returns the current state of the task.
    #[must_use] 
    pub const fn state(&self) -> TaskState {
        self.status.state
    }

    /// Returns the number of messages in the task's history.
    pub fn message_count(&self) -> usize {
        self.history.as_ref().map_or(0, Vec::len)
    }

    /// Returns the number of artifacts attached to the task.
    pub fn artifact_count(&self) -> usize {
        self.artifacts.as_ref().map_or(0, Vec::len)
    }

    /// Returns the last message in the task's history.
    #[must_use] 
    pub fn last_message(&self) -> Option<&Message> {
        self.history.as_ref().and_then(|h| h.last())
    }

    /// Returns the last user message in the task's history.
    #[must_use] 
    pub fn last_user_message(&self) -> Option<&Message> {
        self.history
            .as_ref()
            .and_then(|h| h.iter().rev().find(|m| m.role == super::Role::User))
    }

    /// Returns the last agent message in the task's history.
    #[must_use] 
    pub fn last_agent_message(&self) -> Option<&Message> {
        self.history
            .as_ref()
            .and_then(|h| h.iter().rev().find(|m| m.role == super::Role::Agent))
    }

    /// Finds an artifact by its ID.
    #[must_use] 
    pub fn artifact_by_id(&self, artifact_id: &str) -> Option<&Artifact> {
        self.artifacts
            .as_ref()
            .and_then(|arts| arts.iter().find(|a| a.artifact_id == artifact_id))
    }

    /// Returns `true` if the task is waiting for user input.
    #[must_use] 
    pub const fn is_waiting_for_input(&self) -> bool {
        matches!(
            self.status.state,
            TaskState::InputRequired | TaskState::AuthRequired
        )
    }

    /// Validates that the task is in the expected state.
    #[must_use] 
    pub fn is_in_state(&self, expected: TaskState) -> bool {
        self.status.state == expected
    }

    /// Validates that the task is in one of the expected states.
    #[must_use] 
    pub fn is_in_any_state(&self, expected: &[TaskState]) -> bool {
        expected.contains(&self.status.state)
    }

    /// Updates the task status to a new state.
    pub fn set_state(&mut self, state: TaskState) {
        self.status = TaskStatus::new(state);
    }

    /// Updates the task status with a new state and message.
    pub fn set_state_with_message(&mut self, state: TaskState, message: Message) {
        self.status = TaskStatus::with_message(state, message);
    }

    /// Truncates history to the last `n` messages.
    ///
    /// If `len` is `None` or the history is shorter, this is a no-op.
    pub fn truncate_history(&mut self, len: Option<usize>) {
        if let (Some(max), Some(history)) = (len, self.history.as_mut())
            && history.len() > max {
                let start = history.len() - max;
                *history = history.split_off(start);
            }
    }

    /// Appends artifact data from an update event.
    ///
    /// When `append` is `true`, parts are added to an existing artifact.
    /// When `false`, the artifact is replaced or inserted.
    pub fn apply_artifact_update(&mut self, event: &TaskArtifactUpdateEvent) {
        let artifacts = self.artifacts.get_or_insert_with(Vec::new);
        let artifact_id = &event.artifact.artifact_id;
        let append_parts = event.append;

        let existing_idx = artifacts.iter().position(|a| &a.artifact_id == artifact_id);

        if !append_parts {
            if let Some(idx) = existing_idx {
                artifacts[idx] = event.artifact.clone();
            } else {
                artifacts.push(event.artifact.clone());
            }
        } else if let Some(idx) = existing_idx {
            artifacts[idx].parts.extend(event.artifact.parts.clone());
        }
    }

    /// Inserts or merges a metadata key-value pair.
    pub fn set_metadata(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.metadata
            .get_or_insert_with(HashMap::new)
            .insert(key.into(), value);
    }

    /// Creates a [`TaskStatusUpdateEvent`] from the current task state.
    #[must_use] 
    pub fn status_update_event(&self, is_final: bool) -> TaskStatusUpdateEvent {
        TaskStatusUpdateEvent::new(&self.id, &self.context_id, self.status.clone(), is_final)
    }
}

impl Default for Task {
    fn default() -> Self {
        Self::create()
    }
}

/// Represents a file, data structure, or other resource generated by an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
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
    #[must_use] 
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

    /// Creates a text artifact with the given content.
    pub fn text(artifact_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(artifact_id, vec![Part::text(text)])
    }

    /// Creates a data artifact with the given JSON content.
    pub fn data(artifact_id: impl Into<String>, data: HashMap<String, serde_json::Value>) -> Self {
        Self::new(artifact_id, vec![Part::data(data)])
    }
}

/// An event sent by the agent to notify the client of a status update.
///
/// The `kind` field is injected as `"status-update"` during serialization
/// (aligned with Go's `TaskStatusUpdateEvent.MarshalJSON`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatusUpdateEvent {
    /// The ID of the task that was updated.
    pub task_id: String,
    /// The context ID associated with the task.
    pub context_id: String,
    /// The new status of the task.
    pub status: TaskStatus,
    /// If true, this is the final event in the stream.
    pub r#final: bool,
    /// Optional metadata for extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,

    /// Ignored during deserialization; injected as `"status-update"` on serialization.
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    kind: Option<String>,
}

impl Serialize for TaskStatusUpdateEvent {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Helper<'a> {
            kind: &'static str,
            task_id: &'a str,
            context_id: &'a str,
            status: &'a TaskStatus,
            r#final: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            metadata: &'a Option<HashMap<String, serde_json::Value>>,
        }
        Helper {
            kind: "status-update",
            task_id: &self.task_id,
            context_id: &self.context_id,
            status: &self.status,
            r#final: self.r#final,
            metadata: &self.metadata,
        }
        .serialize(serializer)
    }
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
            metadata: None,
            kind: None,
        }
    }
}

/// An event sent by the agent to notify the client of an artifact update.
///
/// The `kind` field is injected as `"artifact-update"` during serialization
/// (aligned with Go's `TaskArtifactUpdateEvent.MarshalJSON`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskArtifactUpdateEvent {
    /// The ID of the task this artifact belongs to.
    pub task_id: String,
    /// The context ID associated with the task.
    pub context_id: String,
    /// The artifact that was generated or updated.
    pub artifact: Artifact,
    /// If true, the content should be appended to a previous artifact.
    #[serde(default)]
    pub append: bool,
    /// If true, this is the final chunk of the artifact.
    #[serde(default)]
    pub last_chunk: bool,
    /// Optional metadata for extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,

    /// Ignored during deserialization; injected as `"artifact-update"` on serialization.
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    kind: Option<String>,
}

impl Serialize for TaskArtifactUpdateEvent {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Helper<'a> {
            kind: &'static str,
            task_id: &'a str,
            context_id: &'a str,
            artifact: &'a Artifact,
            #[serde(skip_serializing_if = "crate::types::task::is_false")]
            append: bool,
            #[serde(skip_serializing_if = "crate::types::task::is_false")]
            last_chunk: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            metadata: &'a Option<HashMap<String, serde_json::Value>>,
        }
        Helper {
            kind: "artifact-update",
            task_id: &self.task_id,
            context_id: &self.context_id,
            artifact: &self.artifact,
            append: self.append,
            last_chunk: self.last_chunk,
            metadata: &self.metadata,
        }
        .serialize(serializer)
    }
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
            append: false,
            last_chunk: false,
            metadata: None,
            kind: None,
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

    #[test]
    fn test_task_kind_roundtrip() {
        // Serialize injects "kind":"task"
        let task = Task::new("t1", "c1");
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"kind\":\"task\""));

        // Deserialize accepts JSON with "kind" present
        let parsed: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "t1");

        // Deserialize accepts JSON without "kind" field
        let no_kind = r#"{"id":"t2","contextId":"c2","status":{"state":"submitted"}}"#;
        let parsed2: Task = serde_json::from_str(no_kind).unwrap();
        assert_eq!(parsed2.id, "t2");
    }

    #[test]
    fn test_status_update_event_kind_roundtrip() {
        let event = TaskStatusUpdateEvent::new("t1", "c1", TaskStatus::working(), false);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"kind\":\"status-update\""));

        let parsed: TaskStatusUpdateEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.task_id, "t1");
    }

    #[test]
    fn test_artifact_update_event_kind_roundtrip() {
        let artifact = Artifact::text("a1", "hello");
        let event = TaskArtifactUpdateEvent::new("t1", "c1", artifact);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"kind\":\"artifact-update\""));

        let parsed: TaskArtifactUpdateEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.task_id, "t1");
    }

    #[test]
    fn test_task_version_after() {
        let v0 = TaskVersion::MISSING;
        let v1 = TaskVersion(1);
        let v2 = TaskVersion(2);

        // Both missing: both consider themselves "after"
        assert!(v0.after(v0));

        // v1 after missing → true
        assert!(v1.after(v0));

        // missing after v1 → false
        assert!(!v0.after(v1));

        // v2 after v1 → true
        assert!(v2.after(v1));

        // v1 after v2 → false
        assert!(!v1.after(v2));
    }
}
