//! Message types for the A2A protocol.
//!
//! Aligned with Go's `Message` struct — the `kind` discriminator is injected
//! during serialization via a custom `Serialize` impl.

use serde::{Deserialize, Serialize};

use super::{Metadata, Part};

/// Identifies the sender of the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Message from the user/client.
    #[default]
    User,
    /// Message from the agent/service.
    Agent,
}

/// A single message in the conversation between a user and an agent.
///
/// The `"kind": "message"` discriminator is injected on serialization and
/// tolerated (but ignored) on deserialization, matching Go's `MarshalJSON`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// A unique identifier for the message (typically a UUID).
    pub message_id: String,
    /// Identifies the sender of the message.
    pub role: Role,
    /// Content parts that form the message body.
    pub parts: Vec<Part>,
    /// The ID of the task this message is part of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// The context ID for grouping related interactions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    /// Task IDs this message references for additional context.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_task_ids: Vec<String>,
    /// URIs of extensions relevant to this message.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// Extension metadata.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

/// Custom `Serialize` to inject `"kind": "message"` (aligned with Go's `MarshalJSON`).
impl Serialize for Message {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", "message")?;
        map.serialize_entry("messageId", &self.message_id)?;
        map.serialize_entry("role", &self.role)?;
        map.serialize_entry("parts", &self.parts)?;
        if let Some(ref v) = self.task_id {
            map.serialize_entry("taskId", v)?;
        }
        if let Some(ref v) = self.context_id {
            map.serialize_entry("contextId", v)?;
        }
        if !self.reference_task_ids.is_empty() {
            map.serialize_entry("referenceTaskIds", &self.reference_task_ids)?;
        }
        if !self.extensions.is_empty() {
            map.serialize_entry("extensions", &self.extensions)?;
        }
        if !self.metadata.is_empty() {
            map.serialize_entry("metadata", &self.metadata)?;
        }
        map.end()
    }
}

impl Message {
    /// Creates a new message with the given ID, role, and parts.
    pub fn new(message_id: impl Into<String>, role: Role, parts: Vec<Part>) -> Self {
        Self {
            message_id: message_id.into(),
            role,
            parts,
            task_id: None,
            context_id: None,
            reference_task_ids: Vec::new(),
            extensions: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    /// Creates a user message with an auto-generated UUID.
    pub fn user(parts: Vec<Part>) -> Self {
        Self::new(uuid::Uuid::new_v4().to_string(), Role::User, parts)
    }

    /// Creates an agent message with an auto-generated UUID.
    pub fn agent(parts: Vec<Part>) -> Self {
        Self::new(uuid::Uuid::new_v4().to_string(), Role::Agent, parts)
    }

    /// Shorthand: single-text user message.
    pub fn user_text(text: impl Into<String>) -> Self {
        Self::user(vec![Part::text(text)])
    }

    /// Shorthand: single-text agent message.
    pub fn agent_text(text: impl Into<String>) -> Self {
        Self::agent(vec![Part::text(text)])
    }

    /// Builder: sets the task ID.
    pub fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    /// Builder: sets the context ID.
    pub fn with_context_id(mut self, context_id: impl Into<String>) -> Self {
        self.context_id = Some(context_id.into());
        self
    }

    /// Returns the concatenated text content of all text parts.
    pub fn text_content(&self) -> Option<String> {
        let texts: Vec<&str> = self.parts.iter().filter_map(|p| p.as_text()).collect();
        if texts.is_empty() {
            None
        } else {
            Some(texts.join("\n"))
        }
    }

    /// Sets a metadata key-value pair.
    pub fn set_meta(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.metadata.insert(key.into(), value);
    }
}

impl Default for Message {
    fn default() -> Self {
        Self::user(vec![])
    }
}
