//! Push notification infrastructure: config storage and HTTP sender.
//!
//! Aligned with Go's `a2asrv/push/store.go` and `a2asrv/push/sender.go`.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use tokio::sync::RwLock;

use crate::error::{A2AError, Result};
use crate::types::{PushNotificationConfig, Task};

/// Stores push notification configurations per task.
///
/// Aligned with Go's `PushNotificationConfigStore` interface in `tasks.go`.
pub trait PushNotificationConfigStore: Send + Sync {
    /// Saves a push config for a task. Returns the saved config (with generated ID if empty).
    fn save<'a>(
        &'a self,
        task_id: &'a str,
        config: &'a PushNotificationConfig,
    ) -> Pin<Box<dyn Future<Output = Result<PushNotificationConfig>> + Send + 'a>>;

    /// Retrieves a specific push config by task ID and config ID.
    fn get<'a>(
        &'a self,
        task_id: &'a str,
        config_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<PushNotificationConfig>> + Send + 'a>>;

    /// Lists all push configs for a task.
    fn list<'a>(
        &'a self,
        task_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<PushNotificationConfig>>> + Send + 'a>>;

    /// Deletes a specific push config.
    fn delete<'a>(
        &'a self,
        task_id: &'a str,
        config_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Deletes all push configs for a task.
    fn delete_all<'a>(
        &'a self,
        task_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

/// In-memory implementation of [`PushNotificationConfigStore`].
///
/// Aligned with Go's `InMemoryPushNotificationConfigStore` in `push/store.go`.
#[derive(Debug, Default)]
pub struct InMemoryPushNotificationConfigStore {
    // task_id -> (config_id -> PushNotificationConfig)
    configs: RwLock<HashMap<String, HashMap<String, PushNotificationConfig>>>,
}

impl InMemoryPushNotificationConfigStore {
    /// Creates a new empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl PushNotificationConfigStore for InMemoryPushNotificationConfigStore {
    fn save<'a>(
        &'a self,
        task_id: &'a str,
        config: &'a PushNotificationConfig,
    ) -> Pin<Box<dyn Future<Output = Result<PushNotificationConfig>> + Send + 'a>> {
        let task_id = task_id.to_string();
        let config = config.clone();
        Box::pin(async move {
            validate_push_config(&config)?;

            let mut to_save = config;
            if to_save.id.as_deref().unwrap_or("").is_empty() {
                to_save.id = Some(uuid::Uuid::new_v4().to_string());
            }

            let config_id = to_save.id.clone().unwrap_or_default();
            let mut store = self.configs.write().await;
            store
                .entry(task_id)
                .or_default()
                .insert(config_id, to_save.clone());

            Ok(to_save)
        })
    }

    fn get<'a>(
        &'a self,
        task_id: &'a str,
        config_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<PushNotificationConfig>> + Send + 'a>> {
        let task_id = task_id.to_string();
        let config_id = config_id.to_string();
        Box::pin(async move {
            let store = self.configs.read().await;
            store
                .get(&task_id)
                .and_then(|m| m.get(&config_id))
                .cloned()
                .ok_or_else(|| A2AError::InvalidParams("push config not found".into()))
        })
    }

    fn list<'a>(
        &'a self,
        task_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<PushNotificationConfig>>> + Send + 'a>> {
        let task_id = task_id.to_string();
        Box::pin(async move {
            let store = self.configs.read().await;
            Ok(store
                .get(&task_id)
                .map(|m| m.values().cloned().collect())
                .unwrap_or_default())
        })
    }

    fn delete<'a>(
        &'a self,
        task_id: &'a str,
        config_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        let task_id = task_id.to_string();
        let config_id = config_id.to_string();
        Box::pin(async move {
            let mut store = self.configs.write().await;
            if let Some(m) = store.get_mut(&task_id) {
                m.remove(&config_id);
            }
            Ok(())
        })
    }

    fn delete_all<'a>(
        &'a self,
        task_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        let task_id = task_id.to_string();
        Box::pin(async move {
            let mut store = self.configs.write().await;
            store.remove(&task_id);
            Ok(())
        })
    }
}

/// Validates a push config before saving.
fn validate_push_config(config: &PushNotificationConfig) -> Result<()> {
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
pub trait PushSender: Send + Sync {
    /// Sends a push notification with the task state to the configured endpoint.
    fn send_push<'a>(
        &'a self,
        config: &'a PushNotificationConfig,
        task: &'a Task,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
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

impl PushSender for HttpPushSender {
    fn send_push<'a>(
        &'a self,
        config: &'a PushNotificationConfig,
        task: &'a Task,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(self.send_push(config, task))
    }
}

impl HttpPushSender {
    async fn send_push(&self, config: &PushNotificationConfig, task: &Task) -> Result<()> {
        let json_data = serde_json::to_vec(task)
            .map_err(|e| A2AError::ServerError(format!("failed to serialize task: {e}")))?;

        let mut req = self
            .client
            .post(&config.url)
            .header("Content-Type", "application/json")
            .body(json_data);

        if let Some(ref token) = config.token
            && !token.is_empty()
        {
            req = req.header("X-A2A-Notification-Token", token);
        }

        if let Some(ref auth) = config.authentication
            && let Some(ref credentials) = auth.credentials
            && !credentials.is_empty()
        {
            req = self.apply_auth(req, &auth.scheme, credentials);
        }

        let result = req.send().await;
        self.handle_push_result(result)
    }

    fn apply_auth(
        &self,
        req: reqwest::RequestBuilder,
        scheme: &str,
        credentials: &str,
    ) -> reqwest::RequestBuilder {
        match scheme.to_lowercase().as_str() {
            "bearer" => req.header("Authorization", format!("Bearer {credentials}")),
            "basic" => req.header("Authorization", format!("Basic {credentials}")),
            _ => req,
        }
    }

    fn handle_push_result(
        &self,
        result: std::result::Result<reqwest::Response, reqwest::Error>,
    ) -> Result<()> {
        match result {
            Ok(resp) if !resp.status().is_success() => {
                let msg = format!(
                    "push notification endpoint returned non-success status: {}",
                    resp.status()
                );
                self.maybe_fail(msg)
            }
            Ok(_) => Ok(()),
            Err(e) => self.maybe_fail(format!("failed to send push notification: {e}")),
        }
    }

    fn maybe_fail(&self, msg: String) -> Result<()> {
        if self.fail_on_error {
            Err(A2AError::ServerError(msg))
        } else {
            tracing::error!("{msg}");
            Ok(())
        }
    }
}
