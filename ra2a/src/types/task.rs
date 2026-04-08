//! Task, `TaskState`, and `TaskStatus` types for the A2A protocol.
//!
//! Maps to proto `Task`, `TaskState`, `TaskStatus` messages.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::id::{ContextId, TaskId};
use super::{Artifact, Message, Metadata};

/// Lifecycle states of a Task.
///
/// Serialized as `"TASK_STATE_SUBMITTED"`, `"TASK_STATE_COMPLETED"`, etc.
/// per the proto `TaskState` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TaskState {
    /// The task is in an unknown or indeterminate state.
    #[default]
    Unspecified,
    /// Task has been submitted and is awaiting execution.
    Submitted,
    /// Agent is actively working on the task.
    Working,
    /// Task has completed successfully. Terminal state.
    Completed,
    /// Task failed due to an error. Terminal state.
    Failed,
    /// Task was canceled before it finished. Terminal state.
    Canceled,
    /// Task requires additional input from the user. Interrupted state.
    InputRequired,
    /// Task was rejected by the agent. Terminal state.
    Rejected,
    /// Task requires authentication to proceed. Interrupted state.
    AuthRequired,
}

impl TaskState {
    /// Returns `true` for terminal (immutable) states where no further changes
    /// to the task are permitted.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Canceled | Self::Rejected
        )
    }

    /// Returns `true` for interrupted states (task is paused, awaiting action).
    #[must_use]
    pub const fn is_interrupted(self) -> bool {
        matches!(self, Self::InputRequired | Self::AuthRequired)
    }

    /// Parses a `TaskState` from its proto string representation.
    #[must_use]
    pub fn from_state_str(s: &str) -> Self {
        match s {
            "TASK_STATE_SUBMITTED" => Self::Submitted,
            "TASK_STATE_WORKING" => Self::Working,
            "TASK_STATE_COMPLETED" => Self::Completed,
            "TASK_STATE_FAILED" => Self::Failed,
            "TASK_STATE_CANCELED" => Self::Canceled,
            "TASK_STATE_INPUT_REQUIRED" => Self::InputRequired,
            "TASK_STATE_REJECTED" => Self::Rejected,
            "TASK_STATE_AUTH_REQUIRED" => Self::AuthRequired,
            _ => Self::Unspecified,
        }
    }

    /// Returns the proto enum string representation.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Unspecified => "TASK_STATE_UNSPECIFIED",
            Self::Submitted => "TASK_STATE_SUBMITTED",
            Self::Working => "TASK_STATE_WORKING",
            Self::Completed => "TASK_STATE_COMPLETED",
            Self::Failed => "TASK_STATE_FAILED",
            Self::Canceled => "TASK_STATE_CANCELED",
            Self::InputRequired => "TASK_STATE_INPUT_REQUIRED",
            Self::Rejected => "TASK_STATE_REJECTED",
            Self::AuthRequired => "TASK_STATE_AUTH_REQUIRED",
        }
    }
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for TaskState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TaskState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "TASK_STATE_UNSPECIFIED" | "" => Ok(Self::Unspecified),
            "TASK_STATE_SUBMITTED" => Ok(Self::Submitted),
            "TASK_STATE_WORKING" => Ok(Self::Working),
            "TASK_STATE_COMPLETED" => Ok(Self::Completed),
            "TASK_STATE_FAILED" => Ok(Self::Failed),
            "TASK_STATE_CANCELED" => Ok(Self::Canceled),
            "TASK_STATE_INPUT_REQUIRED" => Ok(Self::InputRequired),
            "TASK_STATE_REJECTED" => Ok(Self::Rejected),
            "TASK_STATE_AUTH_REQUIRED" => Ok(Self::AuthRequired),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &[
                    "TASK_STATE_UNSPECIFIED",
                    "TASK_STATE_SUBMITTED",
                    "TASK_STATE_WORKING",
                    "TASK_STATE_COMPLETED",
                    "TASK_STATE_FAILED",
                    "TASK_STATE_CANCELED",
                    "TASK_STATE_INPUT_REQUIRED",
                    "TASK_STATE_REJECTED",
                    "TASK_STATE_AUTH_REQUIRED",
                ],
            )),
        }
    }
}

/// Status of a task at a specific point in time.
///
/// Maps to proto `TaskStatus`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    #[must_use]
    pub fn new(state: TaskState) -> Self {
        Self {
            state,
            message: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Creates a status with a message.
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
        Self::with_message(
            TaskState::Failed,
            Message::agent(vec![super::Part::text(error)]),
        )
    }
}

impl Default for TaskStatus {
    fn default() -> Self {
        Self::submitted()
    }
}

/// A stateful operation or conversation between a client and an agent.
///
/// Maps to proto `Task`. v1.0 does **not** use a `kind` discriminator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// Unique identifier for the task.
    pub id: TaskId,
    /// Context identifier for grouping related tasks.
    pub context_id: ContextId,
    /// Current status of the task.
    pub status: TaskStatus,
    /// Artifacts generated during the task.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,
    /// Messages exchanged during the task (conversation history).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<Message>,
    /// Extension metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl Task {
    /// Creates a new task with the given IDs and submitted status.
    pub fn new(id: impl Into<TaskId>, context_id: impl Into<ContextId>) -> Self {
        Self {
            id: id.into(),
            context_id: context_id.into(),
            status: TaskStatus::submitted(),
            artifacts: Vec::new(),
            history: Vec::new(),
            metadata: None,
        }
    }

    /// Creates a new task with auto-generated `UUIDv7` IDs.
    #[must_use]
    pub fn create() -> Self {
        Self::new(TaskId::random(), ContextId::random())
    }

    /// Creates a submitted task from an initial message.
    pub fn new_submitted(
        id: impl Into<TaskId>,
        context_id: impl Into<ContextId>,
        initial_message: Message,
    ) -> Self {
        Self {
            id: id.into(),
            context_id: context_id.into(),
            status: TaskStatus::new(TaskState::Submitted),
            history: vec![initial_message],
            artifacts: Vec::new(),
            metadata: None,
        }
    }

    /// Returns `true` if the task is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.status.state.is_terminal()
    }

    /// Returns the current state.
    #[must_use]
    pub const fn state(&self) -> TaskState {
        self.status.state
    }

    /// Truncates history to the last `n` messages.
    pub fn truncate_history(&mut self, n: usize) {
        if self.history.len() > n {
            let start = self.history.len() - n;
            self.history = self.history.split_off(start);
        }
    }

    /// Applies an artifact update event to this task.
    pub fn apply_artifact_update(&mut self, event: &super::TaskArtifactUpdateEvent) {
        let artifact_id = &event.artifact.artifact_id;
        let existing_idx = self
            .artifacts
            .iter()
            .position(|a| a.artifact_id == *artifact_id);

        if !event.append {
            if let Some(existing) = existing_idx.and_then(|i| self.artifacts.get_mut(i)) {
                *existing = event.artifact.clone();
            } else {
                self.artifacts.push(event.artifact.clone());
            }
        } else if let Some(existing) = existing_idx.and_then(|i| self.artifacts.get_mut(i)) {
            existing.parts.extend(event.artifact.parts.clone());
        }
    }
}

impl Default for Task {
    fn default() -> Self {
        Self::create()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_state_serde() {
        assert_eq!(
            serde_json::to_string(&TaskState::Completed).unwrap(),
            "\"TASK_STATE_COMPLETED\""
        );
        let decoded: TaskState = serde_json::from_str("\"TASK_STATE_WORKING\"").unwrap();
        assert_eq!(decoded, TaskState::Working);
    }

    #[test]
    fn task_state_terminal() {
        assert!(TaskState::Completed.is_terminal());
        assert!(TaskState::Failed.is_terminal());
        assert!(TaskState::Canceled.is_terminal());
        assert!(TaskState::Rejected.is_terminal());
        assert!(!TaskState::Working.is_terminal());
        assert!(!TaskState::InputRequired.is_terminal());
    }

    #[test]
    fn task_state_interrupted() {
        assert!(TaskState::InputRequired.is_interrupted());
        assert!(TaskState::AuthRequired.is_interrupted());
        assert!(!TaskState::Working.is_interrupted());
    }

    #[test]
    fn task_no_kind_field() {
        let task = Task::create();
        let json = serde_json::to_value(&task).unwrap();
        assert!(json.get("kind").is_none(), "v1.0 must not emit 'kind'");
    }

    #[test]
    fn task_round_trip() {
        let task = Task::new(TaskId::from("t1"), ContextId::from("c1"));
        let json = serde_json::to_string(&task).unwrap();
        let decoded: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(task.id, decoded.id);
        assert_eq!(task.context_id, decoded.context_id);
    }
}
