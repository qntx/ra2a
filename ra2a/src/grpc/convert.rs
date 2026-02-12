//! Type conversion utilities between native Rust types and protobuf types.
//!
//! This module provides conversion functions and trait implementations
//! to convert between the SDK's native types and the generated protobuf types.

use std::collections::{BTreeMap, HashMap};

use super::proto;
use crate::types::{
    Artifact as NativeArtifact, DataPart, FileContent, FilePart as NativeFilePart, FileWithBytes,
    FileWithUri, Message as NativeMessage, Part as NativePart, PushConfig as NativePushConfig,
    Role as NativeRole, Task as NativeTask, TaskState as NativeTaskState,
    TaskStatus as NativeTaskStatus, TextPart,
};

impl From<NativeTaskState> for proto::TaskState {
    fn from(state: NativeTaskState) -> Self {
        match state {
            NativeTaskState::Submitted => Self::Submitted,
            NativeTaskState::Working => Self::Working,
            NativeTaskState::Completed => Self::Completed,
            NativeTaskState::Failed => Self::Failed,
            NativeTaskState::Canceled => Self::Canceled,
            NativeTaskState::InputRequired => Self::InputRequired,
            NativeTaskState::Rejected => Self::Rejected,
            NativeTaskState::AuthRequired => Self::AuthRequired,
            NativeTaskState::Unknown => Self::Unspecified,
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
            proto::TaskState::Unspecified => Self::Unknown,
        }
    }
}

impl From<i32> for NativeTaskState {
    fn from(value: i32) -> Self {
        proto::TaskState::try_from(value)
            .map_or(Self::Unknown, Self::from)
    }
}

impl From<NativeRole> for proto::Role {
    fn from(role: NativeRole) -> Self {
        match role {
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
            proto::Role::Unspecified => Self::User,
        }
    }
}

impl From<i32> for NativeRole {
    fn from(value: i32) -> Self {
        proto::Role::try_from(value)
            .map_or(Self::User, Self::from)
    }
}

impl From<NativePart> for proto::Part {
    fn from(part: NativePart) -> Self {
        match part {
            NativePart::Text(text_part) => {
                let mut proto_part = Self { content: Some(proto::part::Content::Text(text_part.text)), ..Default::default() };
                if let Some(metadata) = text_part.metadata {
                    proto_part.metadata = hashmap_to_struct(metadata);
                }
                proto_part
            }
            NativePart::File(file_part) => {
                let mut proto_part = Self::default();
                match file_part.file {
                    FileContent::Bytes(file_bytes) => {
                        // Base64 decode and set as raw bytes
                        if let Ok(bytes) = base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            &file_bytes.bytes,
                        ) {
                            proto_part.content = Some(proto::part::Content::Raw(bytes));
                        }
                        proto_part.filename = file_bytes.name.unwrap_or_default();
                        proto_part.media_type = file_bytes.mime_type.unwrap_or_default();
                    }
                    FileContent::Uri(file_uri) => {
                        proto_part.content = Some(proto::part::Content::Url(file_uri.uri));
                        proto_part.filename = file_uri.name.unwrap_or_default();
                        proto_part.media_type = file_uri.mime_type.unwrap_or_default();
                    }
                }
                if let Some(metadata) = file_part.metadata {
                    proto_part.metadata = hashmap_to_struct(metadata);
                }
                proto_part
            }
            NativePart::Data(data_part) => {
                let mut proto_part = Self::default();
                // Convert HashMap to prost Value
                let json_value = serde_json::Value::Object(
                    data_part
                        .data
                        .into_iter()
                        .collect::<serde_json::Map<String, serde_json::Value>>(),
                );
                if let Some(prost_val) = json_to_prost_value(json_value) {
                    proto_part.content = Some(proto::part::Content::Data(prost_val));
                }
                if let Some(metadata) = data_part.metadata {
                    proto_part.metadata = hashmap_to_struct(metadata);
                }
                proto_part
            }
        }
    }
}

impl From<proto::Part> for NativePart {
    fn from(part: proto::Part) -> Self {
        let metadata = part.metadata.and_then(struct_to_hashmap);

        match part.content {
            Some(proto::part::Content::Text(text)) => Self::Text(TextPart { text, metadata }),
            Some(proto::part::Content::Raw(bytes)) => {
                // Base64 encode the bytes
                let encoded =
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
                let file_bytes = FileWithBytes {
                    bytes: encoded,
                    name: if part.filename.is_empty() {
                        None
                    } else {
                        Some(part.filename)
                    },
                    mime_type: if part.media_type.is_empty() {
                        None
                    } else {
                        Some(part.media_type)
                    },
                };
                Self::File(NativeFilePart {
                    file: FileContent::Bytes(file_bytes),
                    metadata,
                })
            }
            Some(proto::part::Content::Url(url)) => {
                let file_uri = FileWithUri {
                    uri: url,
                    name: if part.filename.is_empty() {
                        None
                    } else {
                        Some(part.filename.clone())
                    },
                    mime_type: if part.media_type.is_empty() {
                        None
                    } else {
                        Some(part.media_type.clone())
                    },
                };
                Self::File(NativeFilePart {
                    file: FileContent::Uri(file_uri),
                    metadata,
                })
            }
            Some(proto::part::Content::Data(value)) => {
                // Convert prost Value to HashMap
                let data = if let Some(serde_json::Value::Object(map)) = prost_value_to_json(value) {
                    map.into_iter().collect()
                } else {
                    HashMap::new()
                };
                Self::Data(DataPart { data, metadata })
            }
            None => {
                // Default to empty text part
                Self::Text(TextPart {
                    text: String::new(),
                    metadata,
                })
            }
        }
    }
}

impl From<NativeMessage> for proto::Message {
    fn from(msg: NativeMessage) -> Self {
        Self {
            message_id: msg.message_id,
            context_id: msg.context_id.unwrap_or_default(),
            task_id: msg.task_id.unwrap_or_default(),
            role: proto::Role::from(msg.role).into(),
            parts: msg.parts.into_iter().map(proto::Part::from).collect(),
            metadata: msg.metadata.and_then(hashmap_to_struct),
            extensions: msg.extensions.unwrap_or_default(),
            reference_task_ids: msg.reference_task_ids.unwrap_or_default(),
        }
    }
}

impl From<proto::Message> for NativeMessage {
    fn from(msg: proto::Message) -> Self {
        let mut native = Self::new(
            msg.message_id,
            NativeRole::from(msg.role),
            msg.parts.into_iter().map(NativePart::from).collect(),
        );
        native.context_id = if msg.context_id.is_empty() {
            None
        } else {
            Some(msg.context_id)
        };
        native.task_id = if msg.task_id.is_empty() {
            None
        } else {
            Some(msg.task_id)
        };
        native.metadata = msg.metadata.and_then(struct_to_hashmap);
        native.extensions = if msg.extensions.is_empty() {
            None
        } else {
            Some(msg.extensions)
        };
        native.reference_task_ids = if msg.reference_task_ids.is_empty() {
            None
        } else {
            Some(msg.reference_task_ids)
        };
        native
    }
}

impl From<NativeArtifact> for proto::Artifact {
    fn from(artifact: NativeArtifact) -> Self {
        Self {
            artifact_id: artifact.artifact_id,
            name: artifact.name.unwrap_or_default(),
            description: artifact.description.unwrap_or_default(),
            parts: artifact.parts.into_iter().map(proto::Part::from).collect(),
            metadata: artifact.metadata.and_then(hashmap_to_struct),
            extensions: artifact.extensions.unwrap_or_default(),
        }
    }
}

impl From<proto::Artifact> for NativeArtifact {
    fn from(artifact: proto::Artifact) -> Self {
        Self {
            artifact_id: artifact.artifact_id,
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
            extensions: if artifact.extensions.is_empty() {
                None
            } else {
                Some(artifact.extensions)
            },
        }
    }
}

impl From<NativeTaskStatus> for proto::TaskStatus {
    fn from(status: NativeTaskStatus) -> Self {
        Self {
            state: proto::TaskState::from(status.state).into(),
            message: status.message.map(proto::Message::from),
            timestamp: status.timestamp.and_then(|ts| {
                // Parse ISO 8601 timestamp
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
            id: task.id,
            context_id: task.context_id,
            status: Some(proto::TaskStatus::from(task.status)),
            artifacts: task
                .artifacts
                .unwrap_or_default()
                .into_iter()
                .map(proto::Artifact::from)
                .collect(),
            history: task
                .history
                .unwrap_or_default()
                .into_iter()
                .map(proto::Message::from)
                .collect(),
            metadata: task.metadata.and_then(hashmap_to_struct),
        }
    }
}

impl From<proto::Task> for NativeTask {
    fn from(task: proto::Task) -> Self {
        let status = task
            .status.map_or_else(|| NativeTaskStatus::new(NativeTaskState::Unknown), NativeTaskStatus::from);

        let mut native = Self::new(task.id, task.context_id);
        native.status = status;
        native.artifacts = if task.artifacts.is_empty() {
            None
        } else {
            Some(
                task.artifacts
                    .into_iter()
                    .map(NativeArtifact::from)
                    .collect(),
            )
        };
        native.history = if task.history.is_empty() {
            None
        } else {
            Some(task.history.into_iter().map(NativeMessage::from).collect())
        };
        native.metadata = task.metadata.and_then(struct_to_hashmap);
        native
    }
}

impl From<NativePushConfig> for proto::PushNotificationConfig {
    fn from(config: NativePushConfig) -> Self {
        Self {
            id: config.id.unwrap_or_default(),
            url: config.url,
            token: config.token.unwrap_or_default(),
            authentication: config.authentication.map(|auth| proto::AuthenticationInfo {
                // Use first scheme if available
                scheme: auth.schemes.first().cloned().unwrap_or_default(),
                credentials: auth.credentials.unwrap_or_default(),
            }),
        }
    }
}

impl From<proto::PushNotificationConfig> for NativePushConfig {
    fn from(config: proto::PushNotificationConfig) -> Self {
        Self {
            id: if config.id.is_empty() {
                None
            } else {
                Some(config.id)
            },
            url: config.url,
            token: if config.token.is_empty() {
                None
            } else {
                Some(config.token)
            },
            authentication: config
                .authentication
                .map(|auth| crate::types::PushAuthInfo {
                    schemes: if auth.scheme.is_empty() {
                        vec![]
                    } else {
                        vec![auth.scheme]
                    },
                    credentials: if auth.credentials.is_empty() {
                        None
                    } else {
                        Some(auth.credentials)
                    },
                }),
        }
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
            let fields: std::collections::BTreeMap<String, prost_types::Value> = map
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_state_conversion() {
        let native = NativeTaskState::Working;
        let proto: proto::TaskState = native.into();
        assert_eq!(proto, proto::TaskState::Working);

        let back: NativeTaskState = proto.into();
        assert_eq!(back, NativeTaskState::Working);
    }

    #[test]
    fn test_role_conversion() {
        let native = NativeRole::Agent;
        let proto: proto::Role = native.into();
        assert_eq!(proto, proto::Role::Agent);

        let back: NativeRole = proto.into();
        assert_eq!(back, NativeRole::Agent);
    }

    #[test]
    fn test_json_struct_roundtrip() {
        // Note: protobuf stores all numbers as f64, so integers become floats
        let json = serde_json::json!({
            "key": "value",
            "number": 42.0,
            "nested": {
                "inner": true
            }
        });

        let s = json_to_struct(json.clone()).unwrap();
        let back = struct_to_json(s).unwrap();

        assert_eq!(json, back);
    }
}
