//! Push notification sender for the A2A server.
//!
//! This module provides functionality to send push notifications to clients
//! when task status changes occur.

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};

use crate::error::{A2AError, Result};
use crate::types::{
    JsonRpcRequest, PushNotificationAuthenticationInfo, PushNotificationConfig,
    TaskArtifactUpdateEvent, TaskStatusUpdateEvent,
};

/// Payload for push notification requests.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum PushNotificationPayload {
    /// A status update notification.
    StatusUpdate(TaskStatusUpdateEvent),
    /// An artifact update notification.
    ArtifactUpdate(TaskArtifactUpdateEvent),
}

/// A sender for push notifications.
///
/// Handles sending HTTP requests to client-specified callback URLs
/// when task events occur.
#[derive(Debug, Clone)]
pub struct PushNotificationSender {
    http_client: reqwest::Client,
    #[allow(dead_code)]
    timeout_secs: u64,
}

impl PushNotificationSender {
    /// Creates a new push notification sender.
    pub fn new() -> Self {
        Self::with_timeout(30)
    }

    /// Creates a new push notification sender with a custom timeout.
    pub fn with_timeout(timeout_secs: u64) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            http_client,
            timeout_secs,
        }
    }

    /// Sends a status update notification.
    #[instrument(skip(self, config, event), fields(task_id = %event.task_id, url = %config.url))]
    pub async fn send_status_update(
        &self,
        config: &PushNotificationConfig,
        event: &TaskStatusUpdateEvent,
    ) -> Result<()> {
        let payload = PushNotificationPayload::StatusUpdate(event.clone());
        self.send_notification(config, payload).await
    }

    /// Sends an artifact update notification.
    #[instrument(skip(self, config, event), fields(task_id = %event.task_id, url = %config.url))]
    pub async fn send_artifact_update(
        &self,
        config: &PushNotificationConfig,
        event: &TaskArtifactUpdateEvent,
    ) -> Result<()> {
        let payload = PushNotificationPayload::ArtifactUpdate(event.clone());
        self.send_notification(config, payload).await
    }

    /// Sends a notification to the configured callback URL.
    async fn send_notification(
        &self,
        config: &PushNotificationConfig,
        payload: PushNotificationPayload,
    ) -> Result<()> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        // Add authentication headers if configured
        if let Some(ref auth) = config.authentication {
            self.apply_authentication(&mut headers, auth, config.token.as_deref())?;
        } else if let Some(ref token) = config.token {
            // Use token as Bearer auth if no explicit auth config
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", token))
                    .map_err(|e| A2AError::InvalidConfig(e.to_string()))?,
            );
        }

        // Build JSON-RPC notification request
        let request = JsonRpcRequest::new("tasks/pushNotification", payload);

        debug!("Sending push notification to {}", config.url);

        let response = self
            .http_client
            .post(&config.url)
            .headers(headers)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to send push notification: {}", e);
                A2AError::Connection(format!("Push notification failed: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!(
                "Push notification returned error status {}: {}",
                status, body
            );
            return Err(A2AError::Internal(format!(
                "Push notification failed with status {}: {}",
                status, body
            )));
        }

        info!("Push notification sent successfully to {}", config.url);
        Ok(())
    }

    /// Applies authentication to the request headers.
    fn apply_authentication(
        &self,
        headers: &mut HeaderMap,
        auth: &PushNotificationAuthenticationInfo,
        token: Option<&str>,
    ) -> Result<()> {
        // Check supported schemes and apply appropriate authentication
        for scheme in &auth.schemes {
            match scheme.to_lowercase().as_str() {
                "bearer" => {
                    if let Some(creds) = auth.credentials.as_deref().or(token) {
                        headers.insert(
                            AUTHORIZATION,
                            HeaderValue::from_str(&format!("Bearer {}", creds))
                                .map_err(|e| A2AError::InvalidConfig(e.to_string()))?,
                        );
                        return Ok(());
                    }
                }
                "basic" => {
                    if let Some(creds) = &auth.credentials {
                        headers.insert(
                            AUTHORIZATION,
                            HeaderValue::from_str(&format!("Basic {}", creds))
                                .map_err(|e| A2AError::InvalidConfig(e.to_string()))?,
                        );
                        return Ok(());
                    }
                }
                _ => {
                    debug!("Unsupported authentication scheme: {}", scheme);
                }
            }
        }

        Ok(())
    }
}

impl Default for PushNotificationSender {
    fn default() -> Self {
        Self::new()
    }
}

/// Manages push notification sending for multiple tasks.
#[derive(Clone)]
pub struct PushNotificationManager {
    sender: Arc<PushNotificationSender>,
    config_store: Arc<dyn super::PushNotificationConfigStore>,
}

impl std::fmt::Debug for PushNotificationManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PushNotificationManager")
            .field("sender", &self.sender)
            .field("config_store", &"<dyn PushNotificationConfigStore>")
            .finish()
    }
}

impl PushNotificationManager {
    /// Creates a new push notification manager.
    pub fn new(config_store: Arc<dyn super::PushNotificationConfigStore>) -> Self {
        Self {
            sender: Arc::new(PushNotificationSender::new()),
            config_store,
        }
    }

    /// Creates a new push notification manager with a custom sender.
    pub fn with_sender(
        sender: PushNotificationSender,
        config_store: Arc<dyn super::PushNotificationConfigStore>,
    ) -> Self {
        Self {
            sender: Arc::new(sender),
            config_store,
        }
    }

    /// Sends a status update to all configured callbacks for a task.
    #[instrument(skip(self, event), fields(task_id = %event.task_id))]
    pub async fn notify_status_update(&self, event: &TaskStatusUpdateEvent) -> Result<()> {
        let configs = self.config_store.list(&event.task_id).await?;

        if configs.is_empty() {
            debug!("No push notification configs for task {}", event.task_id);
            return Ok(());
        }

        let mut last_error = None;
        for config in configs {
            if let Err(e) = self.sender.send_status_update(&config, event).await {
                error!("Failed to send push notification to {}: {}", config.url, e);
                last_error = Some(e);
            }
        }

        // Return last error if any notifications failed
        if let Some(e) = last_error {
            return Err(e);
        }

        Ok(())
    }

    /// Sends an artifact update to all configured callbacks for a task.
    #[instrument(skip(self, event), fields(task_id = %event.task_id))]
    pub async fn notify_artifact_update(&self, event: &TaskArtifactUpdateEvent) -> Result<()> {
        let configs = self.config_store.list(&event.task_id).await?;

        if configs.is_empty() {
            debug!("No push notification configs for task {}", event.task_id);
            return Ok(());
        }

        let mut last_error = None;
        for config in configs {
            if let Err(e) = self.sender.send_artifact_update(&config, event).await {
                error!("Failed to send push notification to {}: {}", config.url, e);
                last_error = Some(e);
            }
        }

        if let Some(e) = last_error {
            return Err(e);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_notification_sender_creation() {
        let sender = PushNotificationSender::new();
        assert_eq!(sender.timeout_secs, 30);

        let sender = PushNotificationSender::with_timeout(60);
        assert_eq!(sender.timeout_secs, 60);
    }
}
