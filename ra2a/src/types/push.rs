//! Push notification types for the A2A protocol.
//!
//! Aligned with Go's `push.go` in the `a2a` package.

use serde::{Deserialize, Serialize};

use super::Metadata;

// ---------------------------------------------------------------------------
// PushConfig
// ---------------------------------------------------------------------------

/// Configuration for a push notification endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushConfig {
    /// The callback URL for push notifications.
    pub url: String,
    /// A unique identifier for this configuration.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// A unique token to validate incoming push notifications.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
    /// Optional authentication details for the push notification endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication: Option<PushAuthInfo>,
}

impl PushConfig {
    /// Creates a new push notification configuration.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            id: String::new(),
            token: String::new(),
            authentication: None,
        }
    }
}

// ---------------------------------------------------------------------------
// PushAuthInfo
// ---------------------------------------------------------------------------

/// Authentication details for a push notification endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushAuthInfo {
    /// A list of supported authentication schemes.
    pub schemes: Vec<String>,
    /// Optional credentials for the endpoint.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub credentials: String,
}

// ---------------------------------------------------------------------------
// TaskPushConfig
// ---------------------------------------------------------------------------

/// Associates a push notification config with a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPushConfig {
    /// The unique identifier of the task.
    pub task_id: String,
    /// The push notification configuration.
    pub push_notification_config: PushConfig,
}

// ---------------------------------------------------------------------------
// Push config request parameters (aligned with Go's push.go)
// ---------------------------------------------------------------------------

/// Parameters for getting a push notification config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTaskPushConfigParams {
    /// The unique identifier of the task.
    pub id: String,
    /// The ID of the config to retrieve.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub push_notification_config_id: String,
    /// Optional metadata associated with the request.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

impl GetTaskPushConfigParams {
    /// Creates new get parameters.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            push_notification_config_id: String::new(),
            metadata: Metadata::new(),
        }
    }
}

/// Parameters for listing push notification configs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTaskPushConfigParams {
    /// The unique identifier of the task.
    pub id: String,
    /// Optional metadata associated with the request.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

impl ListTaskPushConfigParams {
    /// Creates new list parameters.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            metadata: Metadata::new(),
        }
    }
}

/// Parameters for deleting a push notification config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteTaskPushConfigParams {
    /// The unique identifier of the task.
    pub id: String,
    /// The ID of the config to delete.
    pub push_notification_config_id: String,
    /// Optional metadata associated with the request.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

impl DeleteTaskPushConfigParams {
    /// Creates new delete parameters.
    pub fn new(id: impl Into<String>, config_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            push_notification_config_id: config_id.into(),
            metadata: Metadata::new(),
        }
    }
}
