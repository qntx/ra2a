//! Streaming event types and response wrappers for the A2A protocol.
//!
//! Maps to proto `StreamResponse`, `SendMessageResponse`,
//! `TaskStatusUpdateEvent`, and `TaskArtifactUpdateEvent`.
//!
//! v1.0 uses **JSON member names** as discriminators (no `kind` field):
//! ```json
//! {"task": {...}}
//! {"message": {...}}
//! {"statusUpdate": {...}}
//! {"artifactUpdate": {...}}
//! ```

use std::collections::HashMap;

use serde::de;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::id::{ContextId, TaskId};
use super::{Artifact, Message, Metadata, Task, TaskStatus};

/// Notifies the client of a task status change.
///
/// Maps to proto `TaskStatusUpdateEvent`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatusUpdateEvent {
    /// The task ID.
    pub task_id: TaskId,
    /// The context ID.
    pub context_id: ContextId,
    /// The new status.
    pub status: TaskStatus,
    /// Optional metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl TaskStatusUpdateEvent {
    /// Creates a new status update event.
    pub fn new(
        task_id: impl Into<TaskId>,
        context_id: impl Into<ContextId>,
        status: TaskStatus,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            context_id: context_id.into(),
            status,
            metadata: None,
        }
    }
}

/// Notifies the client of an artifact creation or update.
///
/// Maps to proto `TaskArtifactUpdateEvent`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskArtifactUpdateEvent {
    /// The task ID.
    pub task_id: TaskId,
    /// The context ID.
    pub context_id: ContextId,
    /// The artifact data.
    pub artifact: Artifact,
    /// If `true`, append to a previously sent artifact with the same ID.
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub append: bool,
    /// If `true`, this is the final chunk of the artifact.
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub last_chunk: bool,
    /// Optional metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl TaskArtifactUpdateEvent {
    /// Creates a new artifact update event.
    pub fn new(
        task_id: impl Into<TaskId>,
        context_id: impl Into<ContextId>,
        artifact: Artifact,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            context_id: context_id.into(),
            artifact,
            append: false,
            last_chunk: false,
            metadata: None,
        }
    }
}

/// A streaming event wrapper — discriminated by JSON member name.
///
/// Maps to proto `StreamResponse` (oneof payload).
///
/// Serialized as:
/// - `{"task": {...}}`
/// - `{"message": {...}}`
/// - `{"statusUpdate": {...}}`
/// - `{"artifactUpdate": {...}}`
#[derive(Debug, Clone, PartialEq)]
pub enum StreamResponse {
    /// A complete task snapshot.
    Task(Task),
    /// A direct message.
    Message(Message),
    /// A task status change notification.
    StatusUpdate(TaskStatusUpdateEvent),
    /// An artifact creation/update notification.
    ArtifactUpdate(TaskArtifactUpdateEvent),
}

impl StreamResponse {
    /// Returns the task ID associated with this event.
    #[must_use]
    pub fn task_id(&self) -> &TaskId {
        match self {
            Self::Task(t) => &t.id,
            Self::Message(m) => m.task_id.as_ref().unwrap_or(&EMPTY_TASK_ID),
            Self::StatusUpdate(e) => &e.task_id,
            Self::ArtifactUpdate(e) => &e.task_id,
        }
    }

    /// Returns the context ID associated with this event.
    #[must_use]
    pub fn context_id(&self) -> &str {
        match self {
            Self::Task(t) => t.context_id.as_str(),
            Self::Message(m) => m.context_id.as_deref().unwrap_or(""),
            Self::StatusUpdate(e) => e.context_id.as_str(),
            Self::ArtifactUpdate(e) => e.context_id.as_str(),
        }
    }

    /// Returns `true` if this event represents a terminal condition.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        match self {
            Self::Task(t) => t.is_terminal(),
            Self::Message(_) => true,
            Self::StatusUpdate(e) => e.status.state.is_terminal(),
            Self::ArtifactUpdate(_) => false,
        }
    }
}

static EMPTY_TASK_ID: TaskId = TaskId(String::new());

impl Serialize for StreamResponse {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            Self::Task(v) => map.serialize_entry("task", v)?,
            Self::Message(v) => map.serialize_entry("message", v)?,
            Self::StatusUpdate(v) => map.serialize_entry("statusUpdate", v)?,
            Self::ArtifactUpdate(v) => map.serialize_entry("artifactUpdate", v)?,
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for StreamResponse {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw: HashMap<String, serde_json::Value> = HashMap::deserialize(deserializer)?;

        if let Some(v) = raw.get("task") {
            let task: Task = serde_json::from_value(v.clone()).map_err(de::Error::custom)?;
            Ok(Self::Task(task))
        } else if let Some(v) = raw.get("message") {
            let msg: Message = serde_json::from_value(v.clone()).map_err(de::Error::custom)?;
            Ok(Self::Message(msg))
        } else if let Some(v) = raw.get("statusUpdate") {
            let evt: TaskStatusUpdateEvent =
                serde_json::from_value(v.clone()).map_err(de::Error::custom)?;
            Ok(Self::StatusUpdate(evt))
        } else if let Some(v) = raw.get("artifactUpdate") {
            let evt: TaskArtifactUpdateEvent =
                serde_json::from_value(v.clone()).map_err(de::Error::custom)?;
            Ok(Self::ArtifactUpdate(evt))
        } else {
            Err(de::Error::custom(
                "StreamResponse must contain one of: task, message, statusUpdate, artifactUpdate",
            ))
        }
    }
}

/// Result of a non-streaming `message/send` — either a Task or a Message.
///
/// Maps to proto `SendMessageResponse` (oneof payload).
///
/// Serialized as `{"task": {...}}` or `{"message": {...}}`.
#[derive(Debug, Clone, PartialEq)]
pub enum SendMessageResponse {
    /// A task was created or updated.
    Task(Task),
    /// A direct message reply.
    Message(Message),
}

impl Serialize for SendMessageResponse {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            Self::Task(v) => map.serialize_entry("task", v)?,
            Self::Message(v) => map.serialize_entry("message", v)?,
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for SendMessageResponse {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw: HashMap<String, serde_json::Value> = HashMap::deserialize(deserializer)?;

        if let Some(v) = raw.get("task") {
            let task: Task = serde_json::from_value(v.clone()).map_err(de::Error::custom)?;
            Ok(Self::Task(task))
        } else if let Some(v) = raw.get("message") {
            let msg: Message = serde_json::from_value(v.clone()).map_err(de::Error::custom)?;
            Ok(Self::Message(msg))
        } else {
            Err(de::Error::custom(
                "SendMessageResponse must contain one of: task, message",
            ))
        }
    }
}

impl From<Task> for SendMessageResponse {
    fn from(t: Task) -> Self {
        Self::Task(t)
    }
}

impl From<Message> for SendMessageResponse {
    fn from(m: Message) -> Self {
        Self::Message(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_response_task_serde() {
        let task = Task::create();
        let resp = StreamResponse::Task(task.clone());
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("task").is_some());
        assert!(json.get("kind").is_none());
        let decoded: StreamResponse = serde_json::from_value(json).unwrap();
        assert!(matches!(decoded, StreamResponse::Task(_)));
    }

    #[test]
    fn stream_response_status_update_serde() {
        let evt = TaskStatusUpdateEvent::new(
            TaskId::from("t1"),
            ContextId::from("c1"),
            TaskStatus::working(),
        );
        let resp = StreamResponse::StatusUpdate(evt);
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("statusUpdate").is_some());
        let decoded: StreamResponse = serde_json::from_value(json).unwrap();
        assert!(matches!(decoded, StreamResponse::StatusUpdate(_)));
    }

    #[test]
    fn send_message_response_serde() {
        let msg = Message::agent_text("hello");
        let resp = SendMessageResponse::Message(msg);
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("message").is_some());
        assert!(json.get("kind").is_none());
        let decoded: SendMessageResponse = serde_json::from_value(json).unwrap();
        assert!(matches!(decoded, SendMessageResponse::Message(_)));
    }
}
