//! Message types for the A2A protocol.
//!
//! Maps to the proto `Message` and `Role` definitions.
//! v1.0 does **not** use a `kind` discriminator on messages.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::id::{MessageId, TaskId};
use super::{Metadata, Part};

/// Identifies the sender of a message.
///
/// Serialized as `"ROLE_USER"` / `"ROLE_AGENT"` / `"ROLE_UNSPECIFIED"` per proto enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Role {
    /// Unspecified role.
    #[default]
    Unspecified,
    /// Message from the user/client.
    User,
    /// Message from the agent/service.
    Agent,
}

impl Role {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Unspecified => "ROLE_UNSPECIFIED",
            Self::User => "ROLE_USER",
            Self::Agent => "ROLE_AGENT",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Role {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Role {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "ROLE_USER" => Ok(Self::User),
            "ROLE_AGENT" => Ok(Self::Agent),
            "ROLE_UNSPECIFIED" | "" => Ok(Self::Unspecified),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["ROLE_USER", "ROLE_AGENT", "ROLE_UNSPECIFIED"],
            )),
        }
    }
}

/// A single message in the conversation between a user and an agent.
///
/// Maps to the proto `Message` message. v1.0 does **not** emit a `kind` field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// Unique identifier for the message (required).
    pub message_id: MessageId,
    /// The context ID this message belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    /// The task ID this message belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// Identifies the sender.
    pub role: Role,
    /// Content parts that form the message body.
    pub parts: Vec<Part>,
    /// Optional metadata for extensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// URIs of extensions relevant to this message.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// Task IDs this message references for additional context.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_task_ids: Vec<TaskId>,
}

impl Message {
    /// Creates a new message with the given role and parts, auto-generating an ID.
    #[must_use]
    pub fn new(role: Role, parts: Vec<Part>) -> Self {
        Self {
            message_id: MessageId::random(),
            context_id: None,
            task_id: None,
            role,
            parts,
            metadata: None,
            extensions: Vec::new(),
            reference_task_ids: Vec::new(),
        }
    }

    /// Creates a user message with an auto-generated ID.
    #[must_use]
    pub fn user(parts: Vec<Part>) -> Self {
        Self::new(Role::User, parts)
    }

    /// Creates an agent message with an auto-generated ID.
    #[must_use]
    pub fn agent(parts: Vec<Part>) -> Self {
        Self::new(Role::Agent, parts)
    }

    /// Shorthand: single-text user message.
    pub fn user_text(text: impl Into<String>) -> Self {
        Self::user(vec![Part::text(text)])
    }

    /// Shorthand: single-text agent message.
    pub fn agent_text(text: impl Into<String>) -> Self {
        Self::agent(vec![Part::text(text)])
    }

    /// Sets the task ID.
    #[must_use]
    pub fn with_task_id(mut self, task_id: impl Into<TaskId>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    /// Sets the context ID.
    #[must_use]
    pub fn with_context_id(mut self, context_id: impl Into<String>) -> Self {
        self.context_id = Some(context_id.into());
        self
    }

    /// Sets the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Returns the concatenated text content of all text parts.
    #[must_use]
    pub fn text_content(&self) -> Option<String> {
        let texts: Vec<&str> = self.parts.iter().filter_map(|p| p.as_text()).collect();
        if texts.is_empty() {
            None
        } else {
            Some(texts.join("\n"))
        }
    }
}

impl Default for Message {
    fn default() -> Self {
        Self::user(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_serde() {
        assert_eq!(serde_json::to_string(&Role::User).unwrap(), "\"ROLE_USER\"");
        assert_eq!(
            serde_json::to_string(&Role::Agent).unwrap(),
            "\"ROLE_AGENT\""
        );
        let decoded: Role = serde_json::from_str("\"ROLE_USER\"").unwrap();
        assert_eq!(decoded, Role::User);
    }

    #[test]
    fn message_round_trip() {
        let msg = Message::user_text("hello")
            .with_task_id(TaskId::from("task-1"))
            .with_context_id("ctx-1");
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg.message_id, decoded.message_id);
        assert_eq!(decoded.task_id, Some(TaskId::from("task-1")));
        assert_eq!(decoded.context_id, Some("ctx-1".to_owned()));
        assert_eq!(decoded.role, Role::User);
    }

    #[test]
    fn message_no_kind_field() {
        let msg = Message::user_text("test");
        let json = serde_json::to_value(&msg).unwrap();
        assert!(json.get("kind").is_none(), "v1.0 must not emit 'kind'");
    }
}
