//! Task types for the A2A protocol.
//!
//! Aligned with Go's `Task`, `TaskStatus`, `Artifact`, and event types.

use serde::{Deserialize, Serialize};

use super::{Message, Metadata, Part};

// ---------------------------------------------------------------------------
// TaskVersion (aligned with Go's task_version.go)
// ---------------------------------------------------------------------------

/// Version of a task stored on the server, used for optimistic concurrency control.
///
/// A value of `0` (`MISSING`) means version tracking is not active.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub struct TaskVersion(pub i64);

impl TaskVersion {
    /// Special value indicating that version tracking is not active.
    pub const MISSING: Self = Self(0);

    /// Returns `true` if this version is newer than `other`.
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

// ---------------------------------------------------------------------------
// TaskState (aligned with Go's TaskState constants)
// ---------------------------------------------------------------------------

/// Lifecycle states of a Task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    /// Task has been submitted but not yet started.
    #[default]
    Submitted,
    /// Agent is actively working on the task.
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
    /// Task requires authentication to proceed.
    AuthRequired,
    /// Task is in an unknown state.
    Unknown,
}

impl TaskState {
    /// Returns true for terminal (immutable) states.
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Canceled | Self::Failed | Self::Rejected
        )
    }
}

// ---------------------------------------------------------------------------
// TaskStatus
// ---------------------------------------------------------------------------

/// Status of a task at a specific point in time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskStatus {
    /// The current state.
    pub state: TaskState,
    /// Optional message providing more details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
    /// ISO 8601 datetime when this status was recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

impl TaskStatus {
    /// Creates a status with the given state and current timestamp.
    pub fn new(state: TaskState) -> Self {
        Self {
            state,
            message: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Creates a status with a message.
    pub fn with_message(state: TaskState, message: Message) -> Self {
        Self {
            state,
            message: Some(message),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Shorthand constructors for common states.
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
}

impl Default for TaskStatus {
    fn default() -> Self {
        Self::submitted()
    }
}

// ---------------------------------------------------------------------------
// Task (aligned with Go's Task struct + MarshalJSON)
// ---------------------------------------------------------------------------

/// A stateful operation or conversation between a client and an agent.
///
/// The `"kind": "task"` discriminator is injected on serialization.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// Unique identifier (UUID) for the task.
    pub id: String,
    /// Context identifier for grouping related tasks.
    pub context_id: String,
    /// Current status of the task.
    pub status: TaskStatus,
    /// Messages exchanged during the task.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<Message>,
    /// Artifacts generated during the task.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,
    /// Extension metadata.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

impl Serialize for Task {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", "task")?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("contextId", &self.context_id)?;
        map.serialize_entry("status", &self.status)?;
        if !self.history.is_empty() {
            map.serialize_entry("history", &self.history)?;
        }
        if !self.artifacts.is_empty() {
            map.serialize_entry("artifacts", &self.artifacts)?;
        }
        if !self.metadata.is_empty() {
            map.serialize_entry("metadata", &self.metadata)?;
        }
        map.end()
    }
}

impl Task {
    /// Creates a new task with the given IDs.
    pub fn new(id: impl Into<String>, context_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            context_id: context_id.into(),
            status: TaskStatus::submitted(),
            history: Vec::new(),
            artifacts: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    /// Creates a new task with auto-generated UUIDv7 IDs (aligned with Go's `NewTaskID`).
    pub fn create() -> Self {
        Self::new(
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        )
    }

    /// Creates a submitted task from an initial message (aligned with Go's `NewSubmittedTask`).
    pub fn new_submitted(
        id: impl Into<String>,
        context_id: impl Into<String>,
        initial_message: Message,
    ) -> Self {
        Self {
            id: id.into(),
            context_id: context_id.into(),
            status: TaskStatus::new(TaskState::Submitted),
            history: vec![initial_message],
            artifacts: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    /// Returns true if the task is in a terminal state.
    pub const fn is_terminal(&self) -> bool {
        self.status.state.is_terminal()
    }

    /// Returns the current state.
    pub const fn state(&self) -> TaskState {
        self.status.state
    }

    /// Sets a metadata key-value pair.
    pub fn set_meta(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.metadata.insert(key.into(), value);
    }

    /// Truncates history to the last `n` messages.
    pub fn truncate_history(&mut self, n: usize) {
        if self.history.len() > n {
            let start = self.history.len() - n;
            self.history = self.history.split_off(start);
        }
    }

    /// Applies an artifact update event to this task.
    pub fn apply_artifact_update(&mut self, event: &TaskArtifactUpdateEvent) {
        let artifact_id = &event.artifact.artifact_id;
        let existing_idx = self
            .artifacts
            .iter()
            .position(|a| &a.artifact_id == artifact_id);

        if !event.append {
            if let Some(idx) = existing_idx {
                self.artifacts[idx] = event.artifact.clone();
            } else {
                self.artifacts.push(event.artifact.clone());
            }
        } else if let Some(idx) = existing_idx {
            self.artifacts[idx]
                .parts
                .extend(event.artifact.parts.clone());
        }
    }
}

impl Default for Task {
    fn default() -> Self {
        Self::create()
    }
}

// ---------------------------------------------------------------------------
// Artifact (aligned with Go's Artifact struct)
// ---------------------------------------------------------------------------

/// A file, data structure, or other resource generated by an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    /// Unique identifier within the task scope.
    pub artifact_id: String,
    /// Content parts that make up the artifact.
    pub parts: Vec<Part>,
    /// Optional human-readable name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Extension URIs relevant to this artifact.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// Extension metadata.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

impl Artifact {
    /// Creates a new artifact with the given ID and parts.
    pub fn new(artifact_id: impl Into<String>, parts: Vec<Part>) -> Self {
        Self {
            artifact_id: artifact_id.into(),
            parts,
            name: None,
            description: None,
            extensions: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    /// Creates an artifact with an auto-generated UUIDv7 ID.
    pub fn create(parts: Vec<Part>) -> Self {
        Self::new(uuid::Uuid::new_v4().to_string(), parts)
    }

    /// Shorthand: single-text artifact.
    pub fn text(artifact_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(artifact_id, vec![Part::text(text)])
    }
}

// ---------------------------------------------------------------------------
// Event types (aligned with Go's TaskStatusUpdateEvent / TaskArtifactUpdateEvent)
// ---------------------------------------------------------------------------

/// Notifies the client of a task status change (streaming/subscription).
///
/// The `"kind": "status-update"` discriminator is injected on serialization.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatusUpdateEvent {
    /// The task ID.
    pub task_id: String,
    /// The context ID.
    pub context_id: String,
    /// The new status.
    pub status: TaskStatus,
    /// If true, this is the final event in the stream for this interaction.
    #[serde(default)]
    pub r#final: bool,
    /// Extension metadata.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

impl Serialize for TaskStatusUpdateEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", "status-update")?;
        map.serialize_entry("taskId", &self.task_id)?;
        map.serialize_entry("contextId", &self.context_id)?;
        map.serialize_entry("status", &self.status)?;
        map.serialize_entry("final", &self.r#final)?;
        if !self.metadata.is_empty() {
            map.serialize_entry("metadata", &self.metadata)?;
        }
        map.end()
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
            metadata: Metadata::new(),
        }
    }
}

/// Notifies the client of an artifact creation or update (streaming).
///
/// The `"kind": "artifact-update"` discriminator is injected on serialization.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskArtifactUpdateEvent {
    /// The task ID.
    pub task_id: String,
    /// The context ID.
    pub context_id: String,
    /// The artifact data.
    pub artifact: Artifact,
    /// If true, append to a previously sent artifact with the same ID.
    #[serde(default)]
    pub append: bool,
    /// If true, this is the final chunk.
    #[serde(default)]
    pub last_chunk: bool,
    /// Extension metadata.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

impl Serialize for TaskArtifactUpdateEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", "artifact-update")?;
        map.serialize_entry("taskId", &self.task_id)?;
        map.serialize_entry("contextId", &self.context_id)?;
        map.serialize_entry("artifact", &self.artifact)?;
        if self.append {
            map.serialize_entry("append", &true)?;
        }
        if self.last_chunk {
            map.serialize_entry("lastChunk", &true)?;
        }
        if !self.metadata.is_empty() {
            map.serialize_entry("metadata", &self.metadata)?;
        }
        map.end()
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
            metadata: Metadata::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Event enum (aligned with Go's Event interface)
// ---------------------------------------------------------------------------

/// A unified event type for streaming responses.
///
/// Aligned with Go's `Event` interface — represents any event that can be
/// sent over a streaming connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Event {
    /// A complete task snapshot.
    Task(Task),
    /// A direct message.
    Message(Message),
    /// A status change notification.
    StatusUpdate(TaskStatusUpdateEvent),
    /// An artifact creation/update notification.
    ArtifactUpdate(TaskArtifactUpdateEvent),
}

impl Event {
    /// Returns the task ID associated with this event.
    pub fn task_id(&self) -> Option<&str> {
        match self {
            Self::Task(t) => Some(&t.id),
            Self::Message(m) => m.task_id.as_deref(),
            Self::StatusUpdate(e) => Some(&e.task_id),
            Self::ArtifactUpdate(e) => Some(&e.task_id),
        }
    }

    /// Returns the context ID associated with this event.
    pub fn context_id(&self) -> Option<&str> {
        match self {
            Self::Task(t) => Some(&t.context_id),
            Self::Message(m) => m.context_id.as_deref(),
            Self::StatusUpdate(e) => Some(&e.context_id),
            Self::ArtifactUpdate(e) => Some(&e.context_id),
        }
    }

    /// Returns `true` if this is a final/terminal event (execution should stop after this).
    pub fn is_final(&self) -> bool {
        match self {
            Self::Task(t) => t.is_terminal(),
            Self::Message(_) => true,
            Self::StatusUpdate(e) => e.r#final,
            Self::ArtifactUpdate(_) => false,
        }
    }

    /// Returns `true` if the event represents a terminal condition that should
    /// stop the non-streaming event collection loop. Aligned with Go's
    /// `shouldInterruptNonStreaming` + final-event detection.
    pub fn is_terminal(&self) -> bool {
        match self {
            Self::StatusUpdate(e) => e.r#final || e.status.state.is_terminal(),
            Self::Task(t) => t.status.state.is_terminal(),
            Self::Message(_) => true,
            Self::ArtifactUpdate(_) => false,
        }
    }

    /// Returns the SSE event type string and JSON data for this event.
    pub fn to_sse_data(&self) -> (String, String) {
        let (event_type, data) = match self {
            Self::StatusUpdate(e) => (
                "status_update",
                serde_json::to_string(e).unwrap_or_default(),
            ),
            Self::ArtifactUpdate(e) => (
                "artifact_update",
                serde_json::to_string(e).unwrap_or_default(),
            ),
            Self::Task(t) => ("task", serde_json::to_string(t).unwrap_or_default()),
            Self::Message(m) => ("message", serde_json::to_string(m).unwrap_or_default()),
        };
        (event_type.to_string(), data)
    }
}

/// Result of a non-streaming `message/send` — either a Task or a Message.
///
/// Aligned with Go's `SendMessageResult` interface.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SendMessageResult {
    /// A task was created or updated.
    Task(Task),
    /// A direct message reply (no task created).
    Message(Message),
}

impl From<Task> for SendMessageResult {
    fn from(t: Task) -> Self {
        Self::Task(t)
    }
}

impl From<Message> for SendMessageResult {
    fn from(m: Message) -> Self {
        Self::Message(m)
    }
}
