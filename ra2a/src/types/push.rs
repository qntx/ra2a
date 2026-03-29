//! Push notification types for the A2A protocol.
//!
//! Maps to proto `PushNotificationConfig`, `AuthenticationInfo`,
//! `TaskPushNotificationConfig`.

use serde::{Deserialize, Serialize};

use super::id::TaskId;

/// Configuration for setting up push notifications for task updates.
///
/// Maps to proto `PushNotificationConfig`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PushNotificationConfig {
    /// A unique identifier for this push notification configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The callback URL where the agent should send push notifications.
    pub url: String,
    /// Token unique for this task/session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Authentication information required to send the notification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication: Option<AuthenticationInfo>,
}

impl PushNotificationConfig {
    /// Creates a new push notification configuration with a URL.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            id: None,
            url: url.into(),
            token: None,
            authentication: None,
        }
    }
}

/// Authentication details for a push notification endpoint.
///
/// Maps to proto `AuthenticationInfo`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthenticationInfo {
    /// HTTP Authentication Scheme (e.g. `"Bearer"`, `"Basic"`).
    pub scheme: String,
    /// Push notification credentials. Format depends on the scheme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials: Option<String>,
}

/// A container associating a push notification configuration with a specific task.
///
/// Maps to proto `TaskPushNotificationConfig`. This is a **flat** structure —
/// all fields are at the top level per the v1.0 proto specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskPushNotificationConfig {
    /// Optional tenant ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// A unique identifier for this push notification configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The task ID this config is associated with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// The URL where the notification should be sent.
    pub url: String,
    /// A token unique for this task or session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Authentication information required to send the notification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication: Option<AuthenticationInfo>,
}

impl TaskPushNotificationConfig {
    /// Creates a new task push notification config with a URL.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            tenant: None,
            id: None,
            task_id: None,
            url: url.into(),
            token: None,
            authentication: None,
        }
    }
}
