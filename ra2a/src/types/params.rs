//! A2A protocol request parameter types.
//!
//! These are the structured parameter types used in A2A method calls.
//! Aligned with Go's core types in `core.go`.

use serde::{Deserialize, Serialize};

use super::{Message, Metadata, PushConfig, Task};

/// Parameters for the message/send request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSendParams {
    /// The message being sent to the agent.
    pub message: Message,
    /// Optional configuration for the send request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<MessageSendConfig>,
    /// Metadata for extensions.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

impl MessageSendParams {
    /// Creates new send parameters with a message.
    #[must_use]
    pub fn new(message: Message) -> Self {
        Self {
            message,
            configuration: None,
            metadata: Metadata::new(),
        }
    }

    /// Sets the configuration.
    #[must_use]
    pub fn with_configuration(mut self, config: MessageSendConfig) -> Self {
        self.configuration = Some(config);
        self
    }
}

/// Configuration options for a message/send or message/stream request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageSendConfig {
    /// A list of output MIME types the client accepts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_output_modes: Vec<String>,
    /// If true, the client will wait for the task to complete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<bool>,
    /// The number of recent messages to retrieve in the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
    /// Configuration for push notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_notification_config: Option<PushConfig>,
}

/// Parameters for the tasks/get request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskQueryParams {
    /// The unique identifier of the task.
    pub id: String,
    /// The number of recent messages to retrieve.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
    /// Metadata associated with the request.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

impl TaskQueryParams {
    /// Creates new query parameters.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            history_length: None,
            metadata: Metadata::new(),
        }
    }
}

/// Parameters for the tasks/cancel request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskIdParams {
    /// The unique identifier of the task.
    pub id: String,
    /// Metadata associated with the request.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

impl TaskIdParams {
    /// Creates new task ID parameters.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            metadata: Metadata::new(),
        }
    }
}

/// Parameters for listing tasks (aligned with Go's `ListTasksRequest`).
///
/// JSON field names use `snake_case` to match Go's json tags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListTasksRequest {
    /// The context ID to filter tasks by.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    /// Filter by task state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<super::TaskState>,
    /// Maximum number of tasks to return (1-100, default 50).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<i32>,
    /// Token for retrieving the next page of results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
    /// Number of recent messages to include per task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
    /// Only return tasks updated after this ISO 8601 timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated_after: Option<String>,
    /// Whether to include artifacts in the response.
    #[serde(default, skip_serializing_if = "crate::types::is_false")]
    pub include_artifacts: bool,
}

/// Response for listing tasks (aligned with Go's `ListTasksResponse`).
///
/// JSON field names use `snake_case` to match Go's json tags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListTasksResponse {
    /// The tasks matching the query.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<Task>,
    /// Total number of tasks available (before pagination).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_size: Option<i32>,
    /// Maximum number of tasks returned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<i32>,
    /// Token for retrieving the next page. Empty if no more results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}
