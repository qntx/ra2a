//! A2A protocol response types.
//!
//! Maps to proto `ListTasksResponse` and `ListTaskPushNotificationConfigResponse`.

use serde::{Deserialize, Serialize};

use super::{Task, TaskPushNotificationConfig};

/// Response for `ListTasks`.
///
/// Maps to proto `ListTasksResponse`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ListTasksResponse {
    /// Array of tasks matching the criteria.
    pub tasks: Vec<Task>,
    /// Token for the next page. Empty if no more results.
    #[serde(default)]
    pub next_page_token: String,
    /// The page size used.
    #[serde(default)]
    pub page_size: i32,
    /// Total number of tasks available (before pagination).
    #[serde(default)]
    pub total_size: i32,
}

/// Response for `ListTaskPushNotificationConfigs`.
///
/// Maps to proto `ListTaskPushNotificationConfigsResponse`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ListTaskPushNotificationConfigsResponse {
    /// The list of push notification configurations.
    #[serde(default)]
    pub configs: Vec<TaskPushNotificationConfig>,
    /// Token for the next page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}
