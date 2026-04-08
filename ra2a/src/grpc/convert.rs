//! Type conversion utilities between native Rust types and protobuf types.
//!
//! This module provides conversion functions and trait implementations
//! to convert between the SDK's native types and the generated protobuf types.

use std::collections::{BTreeMap, HashMap};

use super::proto;
use crate::types::{
    Artifact as NativeArtifact, ArtifactId, ContextId, Message as NativeMessage, MessageId,
    Part as NativePart, PartContent, Role as NativeRole, Task as NativeTask, TaskId,
    TaskState as NativeTaskState, TaskStatus as NativeTaskStatus,
};

impl From<NativeTaskState> for proto::TaskState {
    fn from(state: NativeTaskState) -> Self {
        match state {
            NativeTaskState::Unspecified => Self::Unspecified,
            NativeTaskState::Submitted => Self::Submitted,
            NativeTaskState::Working => Self::Working,
            NativeTaskState::Completed => Self::Completed,
            NativeTaskState::Failed => Self::Failed,
            NativeTaskState::Canceled => Self::Canceled,
            NativeTaskState::InputRequired => Self::InputRequired,
            NativeTaskState::Rejected => Self::Rejected,
            NativeTaskState::AuthRequired => Self::AuthRequired,
        }
    }
}

impl From<proto::TaskState> for NativeTaskState {
    fn from(state: proto::TaskState) -> Self {
        match state {
            proto::TaskState::Submitted => Self::Submitted,
            proto::TaskState::Working => Self::Working,
            proto::TaskState::Completed => Self::Completed,
            proto::TaskState::Failed => Self::Failed,
            proto::TaskState::Canceled => Self::Canceled,
            proto::TaskState::InputRequired => Self::InputRequired,
            proto::TaskState::Rejected => Self::Rejected,
            proto::TaskState::AuthRequired => Self::AuthRequired,
            proto::TaskState::Unspecified => Self::Unspecified,
        }
    }
}

impl From<i32> for NativeTaskState {
    fn from(value: i32) -> Self {
        proto::TaskState::try_from(value).map_or(Self::Unspecified, Self::from)
    }
}

impl From<NativeRole> for proto::Role {
    fn from(role: NativeRole) -> Self {
        match role {
            NativeRole::Unspecified => Self::Unspecified,
            NativeRole::User => Self::User,
            NativeRole::Agent => Self::Agent,
        }
    }
}

impl From<proto::Role> for NativeRole {
    fn from(role: proto::Role) -> Self {
        match role {
            proto::Role::User => Self::User,
            proto::Role::Agent => Self::Agent,
            proto::Role::Unspecified => Self::Unspecified,
        }
    }
}

impl From<i32> for NativeRole {
    fn from(value: i32) -> Self {
        proto::Role::try_from(value).map_or(Self::Unspecified, Self::from)
    }
}

impl From<NativePart> for proto::Part {
    fn from(part: NativePart) -> Self {
        let mut proto_part = Self {
            filename: part.filename.unwrap_or_default(),
            media_type: part.media_type.unwrap_or_default(),
            metadata: part.metadata.and_then(hashmap_to_struct),
            ..Default::default()
        };
        match part.content {
            PartContent::Text(text) => {
                proto_part.content = Some(proto::part::Content::Text(text));
            }
            PartContent::Raw(bytes) => {
                proto_part.content = Some(proto::part::Content::Raw(bytes));
            }
            PartContent::Url(url) => {
                proto_part.content = Some(proto::part::Content::Url(url));
            }
            PartContent::Data(value) => {
                if let Some(prost_val) = json_to_prost_value(value) {
                    proto_part.content = Some(proto::part::Content::Data(prost_val));
                }
            }
        }
        proto_part
    }
}

impl From<proto::Part> for NativePart {
    fn from(part: proto::Part) -> Self {
        let metadata = part.metadata.and_then(struct_to_hashmap);
        let filename = if part.filename.is_empty() {
            None
        } else {
            Some(part.filename)
        };
        let media_type = if part.media_type.is_empty() {
            None
        } else {
            Some(part.media_type)
        };

        let content = match part.content {
            Some(proto::part::Content::Text(text)) => PartContent::Text(text),
            Some(proto::part::Content::Raw(bytes)) => PartContent::Raw(bytes),
            Some(proto::part::Content::Url(url)) => PartContent::Url(url),
            Some(proto::part::Content::Data(value)) => {
                PartContent::Data(prost_value_to_json(value).unwrap_or(serde_json::Value::Null))
            }
            None => PartContent::Text(String::new()),
        };

        Self {
            content,
            metadata,
            filename,
            media_type,
        }
    }
}

impl From<NativeMessage> for proto::Message {
    fn from(msg: NativeMessage) -> Self {
        Self {
            message_id: msg.message_id.to_string(),
            context_id: msg.context_id.unwrap_or_default(),
            task_id: msg.task_id.map_or(String::new(), |t| t.to_string()),
            role: proto::Role::from(msg.role).into(),
            parts: msg.parts.into_iter().map(proto::Part::from).collect(),
            metadata: msg.metadata.and_then(hashmap_to_struct),
            extensions: msg.extensions,
            reference_task_ids: msg
                .reference_task_ids
                .into_iter()
                .map(|id| id.to_string())
                .collect(),
        }
    }
}

impl From<proto::Message> for NativeMessage {
    fn from(msg: proto::Message) -> Self {
        let mut native = Self::new(
            NativeRole::from(msg.role),
            msg.parts.into_iter().map(NativePart::from).collect(),
        );
        native.message_id = MessageId::from(msg.message_id.as_str());
        native.context_id = if msg.context_id.is_empty() {
            None
        } else {
            Some(msg.context_id)
        };
        native.task_id = if msg.task_id.is_empty() {
            None
        } else {
            Some(TaskId::from(msg.task_id.as_str()))
        };
        native.metadata = msg.metadata.and_then(struct_to_hashmap);
        native.extensions = msg.extensions;
        native.reference_task_ids = msg
            .reference_task_ids
            .into_iter()
            .map(|s| TaskId::from(s.as_str()))
            .collect();
        native
    }
}

impl From<NativeArtifact> for proto::Artifact {
    fn from(artifact: NativeArtifact) -> Self {
        Self {
            artifact_id: artifact.artifact_id.to_string(),
            name: artifact.name.unwrap_or_default(),
            description: artifact.description.unwrap_or_default(),
            parts: artifact.parts.into_iter().map(proto::Part::from).collect(),
            metadata: artifact.metadata.and_then(hashmap_to_struct),
            extensions: artifact.extensions,
        }
    }
}

impl From<proto::Artifact> for NativeArtifact {
    fn from(artifact: proto::Artifact) -> Self {
        Self {
            artifact_id: ArtifactId::from(artifact.artifact_id.as_str()),
            name: if artifact.name.is_empty() {
                None
            } else {
                Some(artifact.name)
            },
            description: if artifact.description.is_empty() {
                None
            } else {
                Some(artifact.description)
            },
            parts: artifact.parts.into_iter().map(NativePart::from).collect(),
            metadata: artifact.metadata.and_then(struct_to_hashmap),
            extensions: artifact.extensions,
        }
    }
}

impl From<NativeTaskStatus> for proto::TaskStatus {
    fn from(status: NativeTaskStatus) -> Self {
        Self {
            state: proto::TaskState::from(status.state).into(),
            message: status.message.map(proto::Message::from),
            timestamp: status.timestamp.and_then(|ts| {
                chrono::DateTime::parse_from_rfc3339(&ts)
                    .ok()
                    .map(|dt| prost_types::Timestamp {
                        seconds: dt.timestamp(),
                        nanos: dt.timestamp_subsec_nanos() as i32,
                    })
            }),
        }
    }
}

impl From<proto::TaskStatus> for NativeTaskStatus {
    fn from(status: proto::TaskStatus) -> Self {
        Self {
            state: NativeTaskState::from(status.state),
            message: status.message.map(NativeMessage::from),
            timestamp: status.timestamp.map(|ts| {
                chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            }),
        }
    }
}

impl From<NativeTask> for proto::Task {
    fn from(task: NativeTask) -> Self {
        Self {
            id: task.id.to_string(),
            context_id: task.context_id.to_string(),
            status: Some(proto::TaskStatus::from(task.status)),
            artifacts: task
                .artifacts
                .into_iter()
                .map(proto::Artifact::from)
                .collect(),
            history: task.history.into_iter().map(proto::Message::from).collect(),
            metadata: task.metadata.and_then(hashmap_to_struct),
        }
    }
}

impl From<proto::Task> for NativeTask {
    fn from(task: proto::Task) -> Self {
        let status = task.status.map_or_else(
            || NativeTaskStatus::new(NativeTaskState::Unspecified),
            NativeTaskStatus::from,
        );

        let mut native = Self::new(
            TaskId::from(task.id.as_str()),
            ContextId::from(task.context_id.as_str()),
        );
        native.status = status;
        native.artifacts = task
            .artifacts
            .into_iter()
            .map(NativeArtifact::from)
            .collect();
        native.history = task.history.into_iter().map(NativeMessage::from).collect();
        native.metadata = task.metadata.and_then(struct_to_hashmap);
        native
    }
}

/// Converts a `HashMap` metadata to a protobuf Struct.
#[must_use]
pub fn hashmap_to_struct(map: HashMap<String, serde_json::Value>) -> Option<prost_types::Struct> {
    let fields: BTreeMap<String, prost_types::Value> = map
        .into_iter()
        .filter_map(|(k, v)| json_to_prost_value(v).map(|pv| (k, pv)))
        .collect();
    Some(prost_types::Struct { fields })
}

/// Converts a protobuf Struct to a `HashMap` metadata.
#[must_use]
pub fn struct_to_hashmap(s: prost_types::Struct) -> Option<HashMap<String, serde_json::Value>> {
    let map: HashMap<String, serde_json::Value> = s
        .fields
        .into_iter()
        .filter_map(|(k, v)| prost_value_to_json(v).map(|jv| (k, jv)))
        .collect();
    Some(map)
}

/// Converts a JSON value to a protobuf Struct.
#[must_use]
pub fn json_to_struct(value: serde_json::Value) -> Option<prost_types::Struct> {
    match value {
        serde_json::Value::Object(map) => {
            let fields: BTreeMap<String, prost_types::Value> = map
                .into_iter()
                .filter_map(|(k, v)| json_to_prost_value(v).map(|pv| (k, pv)))
                .collect();
            Some(prost_types::Struct { fields })
        }
        _ => None,
    }
}

/// Converts a protobuf Struct to a JSON value.
#[must_use]
pub fn struct_to_json(s: prost_types::Struct) -> Option<serde_json::Value> {
    let map: serde_json::Map<String, serde_json::Value> = s
        .fields
        .into_iter()
        .filter_map(|(k, v)| prost_value_to_json(v).map(|jv| (k, jv)))
        .collect();
    Some(serde_json::Value::Object(map))
}

/// Converts a JSON value to a prost Value.
fn json_to_prost_value(value: serde_json::Value) -> Option<prost_types::Value> {
    let kind = match value {
        serde_json::Value::Null => prost_types::value::Kind::NullValue(0),
        serde_json::Value::Bool(b) => prost_types::value::Kind::BoolValue(b),
        serde_json::Value::Number(n) => {
            prost_types::value::Kind::NumberValue(n.as_f64().unwrap_or(0.0))
        }
        serde_json::Value::String(s) => prost_types::value::Kind::StringValue(s),
        serde_json::Value::Array(arr) => {
            let values: Vec<prost_types::Value> =
                arr.into_iter().filter_map(json_to_prost_value).collect();
            prost_types::value::Kind::ListValue(prost_types::ListValue { values })
        }
        serde_json::Value::Object(map) => {
            let fields: BTreeMap<String, prost_types::Value> = map
                .into_iter()
                .filter_map(|(k, v)| json_to_prost_value(v).map(|pv| (k, pv)))
                .collect();
            prost_types::value::Kind::StructValue(prost_types::Struct { fields })
        }
    };
    Some(prost_types::Value { kind: Some(kind) })
}

/// Converts a prost Value to a JSON value.
fn prost_value_to_json(value: prost_types::Value) -> Option<serde_json::Value> {
    match value.kind? {
        prost_types::value::Kind::NullValue(_) => Some(serde_json::Value::Null),
        prost_types::value::Kind::BoolValue(b) => Some(serde_json::Value::Bool(b)),
        prost_types::value::Kind::NumberValue(n) => {
            serde_json::Number::from_f64(n).map(serde_json::Value::Number)
        }
        prost_types::value::Kind::StringValue(s) => Some(serde_json::Value::String(s)),
        prost_types::value::Kind::ListValue(list) => {
            let arr: Vec<serde_json::Value> = list
                .values
                .into_iter()
                .filter_map(prost_value_to_json)
                .collect();
            Some(serde_json::Value::Array(arr))
        }
        prost_types::value::Kind::StructValue(s) => struct_to_json(s),
    }
}
