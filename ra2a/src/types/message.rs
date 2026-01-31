//! Message types for the A2A protocol.
//!
//! Messages represent the communication between users and agents.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::Part;

/// Identifies the sender of the message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Message from the user/client.
    User,
    /// Message from the agent/service.
    Agent,
}

impl Default for Role {
    fn default() -> Self {
        Self::User
    }
}

/// Represents a single message in the conversation between a user and an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    /// A unique identifier for the message (typically a UUID).
    pub message_id: String,
    /// Identifies the sender of the message.
    pub role: Role,
    /// An array of content parts that form the message body.
    pub parts: Vec<Part>,
    /// The type of this object (always "message").
    #[serde(default = "default_message_kind")]
    pub kind: String,
    /// The ID of the task this message is part of.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// The context ID for this message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    /// A list of other task IDs that this message references.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_task_ids: Option<Vec<String>>,
    /// Reference to a previous message this message is replying to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referencing_message_id: Option<String>,
    /// The URIs of extensions relevant to this message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<String>>,
    /// Optional metadata for extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

fn default_message_kind() -> String {
    "message".to_string()
}

impl Message {
    /// Creates a new message with the given ID, role, and parts.
    pub fn new(message_id: impl Into<String>, role: Role, parts: Vec<Part>) -> Self {
        Self {
            message_id: message_id.into(),
            role,
            parts,
            kind: "message".to_string(),
            task_id: None,
            context_id: None,
            reference_task_ids: None,
            referencing_message_id: None,
            extensions: None,
            metadata: None,
        }
    }

    /// Sets the referencing message ID.
    pub fn with_referencing_message(mut self, message_id: impl Into<String>) -> Self {
        self.referencing_message_id = Some(message_id.into());
        self
    }

    /// Creates a new user message with auto-generated ID.
    pub fn user(parts: Vec<Part>) -> Self {
        Self::new(uuid::Uuid::new_v4().to_string(), Role::User, parts)
    }

    /// Creates a new agent message with auto-generated ID.
    pub fn agent(parts: Vec<Part>) -> Self {
        Self::new(uuid::Uuid::new_v4().to_string(), Role::Agent, parts)
    }

    /// Creates a simple text message from the user.
    pub fn user_text(text: impl Into<String>) -> Self {
        Self::user(vec![Part::text(text)])
    }

    /// Creates a simple text message from the agent.
    pub fn agent_text(text: impl Into<String>) -> Self {
        Self::agent(vec![Part::text(text)])
    }

    /// Sets the task ID for this message.
    pub fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    /// Sets the context ID for this message.
    pub fn with_context_id(mut self, context_id: impl Into<String>) -> Self {
        self.context_id = Some(context_id.into());
        self
    }

    /// Sets the extensions for this message.
    pub fn with_extensions(mut self, extensions: Vec<String>) -> Self {
        self.extensions = Some(extensions);
        self
    }

    /// Sets the metadata for this message.
    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Returns true if this message is from a user.
    pub fn is_user(&self) -> bool {
        self.role == Role::User
    }

    /// Returns true if this message is from an agent.
    pub fn is_agent(&self) -> bool {
        self.role == Role::Agent
    }

    /// Returns the text content of this message if it contains only text parts.
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
    fn test_user_text_message() {
        let msg = Message::user_text("Hello!");
        assert!(msg.is_user());
        assert_eq!(msg.text_content(), Some("Hello!".to_string()));
    }

    #[test]
    fn test_agent_message() {
        let msg = Message::agent_text("Hi there!");
        assert!(msg.is_agent());
        assert_eq!(msg.role, Role::Agent);
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::user_text("Test");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"kind\":\"message\""));
    }
}
