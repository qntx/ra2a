//! Push notification infrastructure: config storage and HTTP sender.
//!
//! Aligned with Go's `a2asrv/push/store.go` and `a2asrv/push/sender.go`.

use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::error::{A2AError, Result};
use crate::types::{PushConfig, Task};

/// Stores push notification configurations per task.
///
/// Aligned with Go's `PushConfigStore` interface in `tasks.go`.
#[async_trait]
pub trait PushConfigStore: Send + Sync {
    /// Saves a push config for a task. Returns the saved config (with generated ID if empty).
    async fn save(&self, task_id: &str, config: &PushConfig) -> Result<PushConfig>;

    /// Retrieves a specific push config by task ID and config ID.
    async fn get(&self, task_id: &str, config_id: &str) -> Result<PushConfig>;

    /// Lists all push configs for a task.
    async fn list(&self, task_id: &str) -> Result<Vec<PushConfig>>;

    /// Deletes a specific push config.
    async fn delete(&self, task_id: &str, config_id: &str) -> Result<()>;

    /// Deletes all push configs for a task.
    async fn delete_all(&self, task_id: &str) -> Result<()>;
}

/// In-memory implementation of [`PushConfigStore`].
///
/// Aligned with Go's `InMemoryPushConfigStore` in `push/store.go`.
#[derive(Debug, Default)]
pub struct InMemoryPushConfigStore {
    // task_id -> (config_id -> PushConfig)
    configs: RwLock<HashMap<String, HashMap<String, PushConfig>>>,
}

impl InMemoryPushConfigStore {
    /// Creates a new empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl PushConfigStore for InMemoryPushConfigStore {
    async fn save(&self, task_id: &str, config: &PushConfig) -> Result<PushConfig> {
        validate_push_config(config)?;

        let mut to_save = config.clone();
        if to_save.id.is_empty() {
            to_save.id = uuid::Uuid::new_v4().to_string();
        }

        let config_id = to_save.id.clone();
        let mut store = self.configs.write().await;
        store
            .entry(task_id.to_string())
            .or_default()
            .insert(config_id, to_save.clone());

        Ok(to_save)
    }

    async fn get(&self, task_id: &str, config_id: &str) -> Result<PushConfig> {
        let store = self.configs.read().await;
        store
            .get(task_id)
            .and_then(|m| m.get(config_id))
            .cloned()
            .ok_or_else(|| A2AError::InvalidParams("push config not found".into()))
    }

    async fn list(&self, task_id: &str) -> Result<Vec<PushConfig>> {
        let store = self.configs.read().await;
        Ok(store
            .get(task_id)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default())
    }

    async fn delete(&self, task_id: &str, config_id: &str) -> Result<()> {
        let mut store = self.configs.write().await;
        if let Some(m) = store.get_mut(task_id) {
            m.remove(config_id);
        }
        Ok(())
    }

    async fn delete_all(&self, task_id: &str) -> Result<()> {
        let mut store = self.configs.write().await;
        store.remove(task_id);
        Ok(())
    }
}

/// Validates a push config before saving.
fn validate_push_config(config: &PushConfig) -> Result<()> {
    if config.url.is_empty() {
        return Err(A2AError::InvalidParams(
            "push config URL cannot be empty".into(),
        ));
    }
    // Basic URL validation
    if !config.url.starts_with("http://") && !config.url.starts_with("https://") {
        return Err(A2AError::InvalidParams(
            "push config URL must be http or https".into(),
        ));
    }
    Ok(())
}

/// Sends push notifications about task state changes.
///
/// Aligned with Go's `PushSender` interface in `tasks.go`.
#[async_trait]
pub trait PushSender: Send + Sync {
    /// Sends a push notification with the task state to the configured endpoint.
    async fn send_push(&self, config: &PushConfig, task: &Task) -> Result<()>;
}

/// HTTP-based push notification sender.
///
/// Aligned with Go's `HTTPPushSender` in `push/sender.go`.
pub struct HttpPushSender {
    client: reqwest::Client,
    fail_on_error: bool,
}

/// Configuration for [`HttpPushSender`].
#[derive(Debug, Clone)]
pub struct HttpPushSenderConfig {
    /// HTTP request timeout.
    pub timeout: std::time::Duration,
    /// If true, push failures will propagate as errors (may cancel execution).
    pub fail_on_error: bool,
}

impl Default for HttpPushSenderConfig {
    fn default() -> Self {
        Self {
            timeout: std::time::Duration::from_secs(30),
            fail_on_error: false,
        }
    }
}

impl HttpPushSender {
    /// Creates a new sender with default configuration (30s timeout, errors logged).
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(HttpPushSenderConfig::default())
    }

    /// Creates a new sender with custom configuration.
    #[must_use]
    pub fn with_config(config: HttpPushSenderConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            fail_on_error: config.fail_on_error,
        }
    }
}

impl Default for HttpPushSender {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PushSender for HttpPushSender {
    async fn send_push(&self, config: &PushConfig, task: &Task) -> Result<()> {
        let json_data = serde_json::to_vec(task)
            .map_err(|e| A2AError::ServerError(format!("failed to serialize task: {e}")))?;

        let mut req = self
            .client
            .post(&config.url)
            .header("Content-Type", "application/json")
            .body(json_data);

        // Attach verification token header (Go: X-A2A-Notification-Token)
        if !config.token.is_empty() {
            req = req.header("X-A2A-Notification-Token", &config.token);
        }

        // Apply authentication from push config
        if let Some(ref auth) = config.authentication
            && !auth.credentials.is_empty()
        {
            let credentials = &auth.credentials;
            for scheme in &auth.schemes {
                match scheme.to_lowercase().as_str() {
                    "bearer" => {
                        req = req.header("Authorization", format!("Bearer {credentials}"));
                        break;
                    }
                    "basic" => {
                        req = req.header("Authorization", format!("Basic {credentials}"));
                        break;
                    }
                    _ => {}
                }
            }
        }

        match req.send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let msg = format!(
                        "push notification endpoint returned non-success status: {}",
                        resp.status()
                    );
                    if self.fail_on_error {
                        return Err(A2AError::ServerError(msg));
                    }
                    tracing::error!("{msg}");
                }
                Ok(())
            }
            Err(e) => {
                let msg = format!("failed to send push notification: {e}");
                if self.fail_on_error {
                    Err(A2AError::ServerError(msg))
                } else {
                    tracing::error!("{msg}");
                    Ok(())
                }
            }
        }
    }
}
