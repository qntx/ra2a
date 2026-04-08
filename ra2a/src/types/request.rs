//! A2A protocol request types.
//!
//! Maps to all proto `*Request` messages. Every request type carries an
//! optional `tenant` field for multi-tenancy support.

use serde::{Deserialize, Serialize};

use super::id::TaskId;
use super::{Message, Metadata, TaskPushNotificationConfig, TaskState};

/// Configuration of a send message request.
///
/// Maps to proto `SendMessageConfiguration`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageConfiguration {
    /// MIME types the client accepts for response parts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_output_modes: Vec<String>,
    /// Push notification configuration for task updates.
    /// Task ID should be empty when sending this in a `SendMessage` request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_push_notification_config: Option<TaskPushNotificationConfig>,
    /// Max number of recent messages to include in the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
    /// If `true`, the operation returns immediately after creating the task,
    /// even if processing is still in progress. Default: `false`.
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub return_immediately: bool,
}

/// Request for `SendMessage` / `SendStreamingMessage`.
///
/// Maps to proto `SendMessageRequest`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendMessageRequest {
    /// Optional tenant ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// The message to send.
    pub message: Message,
    /// Optional configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration: Option<SendMessageConfiguration>,
    /// Optional metadata for extensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl SendMessageRequest {
    /// Creates a new send message request.
    #[must_use]
    pub const fn new(message: Message) -> Self {
        Self {
            tenant: None,
            message,
            configuration: None,
            metadata: None,
        }
    }

    /// Sets the configuration.
    #[must_use]
    pub fn with_configuration(mut self, config: SendMessageConfiguration) -> Self {
        self.configuration = Some(config);
        self
    }
}

/// Request for `GetTask`.
///
/// Maps to proto `GetTaskRequest`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetTaskRequest {
    /// Optional tenant ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// The task ID to retrieve.
    pub id: TaskId,
    /// Max number of recent messages to include.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
}

/// Request for `ListTasks`.
///
/// Maps to proto `ListTasksRequest`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListTasksRequest {
    /// Optional tenant ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// Filter by context ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    /// Filter by task state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<TaskState>,
    /// Maximum number of tasks to return (1-100, default 50).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<i32>,
    /// Pagination token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
    /// Number of recent messages to include per task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
    /// Only return tasks updated after this ISO 8601 timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_timestamp_after: Option<String>,
    /// Whether to include artifacts in the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_artifacts: Option<bool>,
}

/// Request for `CancelTask`.
///
/// Maps to proto `CancelTaskRequest`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CancelTaskRequest {
    /// Optional tenant ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// The task ID to cancel.
    pub id: TaskId,
    /// Optional metadata for extensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

/// Request for `SubscribeToTask`.
///
/// Maps to proto `SubscribeToTaskRequest`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscribeToTaskRequest {
    /// Optional tenant ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// The task ID to subscribe to.
    pub id: TaskId,
}

/// Request for `GetTaskPushNotificationConfig`.
///
/// Maps to proto `GetTaskPushNotificationConfigRequest`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetTaskPushNotificationConfigRequest {
    /// Optional tenant ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// The parent task ID.
    pub task_id: TaskId,
    /// The config ID to retrieve.
    pub id: String,
}

/// Request for `DeleteTaskPushNotificationConfig`.
///
/// Maps to proto `DeleteTaskPushNotificationConfigRequest`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteTaskPushNotificationConfigRequest {
    /// Optional tenant ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// The parent task ID.
    pub task_id: TaskId,
    /// The config ID to delete.
    pub id: String,
}

/// Request for `ListTaskPushNotificationConfigs`.
///
/// Maps to proto `ListTaskPushNotificationConfigsRequest`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListTaskPushNotificationConfigsRequest {
    /// Optional tenant ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// The parent task ID.
    pub task_id: TaskId,
    /// Maximum number of configs to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<i32>,
    /// Pagination token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

/// Request for `GetExtendedAgentCard`.
///
/// Maps to proto `GetExtendedAgentCardRequest`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetExtendedAgentCardRequest {
    /// Optional tenant ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
}
