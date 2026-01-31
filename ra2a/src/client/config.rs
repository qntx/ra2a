//! Client configuration types.

use crate::types::{PushNotificationConfig, TransportProtocol};

/// Configuration for the A2A client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Whether the client supports streaming.
    pub streaming: bool,
    /// Whether the client prefers polling for updates.
    pub polling: bool,
    /// Ordered list of transport protocols (in order of preference).
    pub supported_transports: Vec<TransportProtocol>,
    /// Whether to use client transport preferences over server preferences.
    pub use_client_preference: bool,
    /// The set of accepted output modes for the client.
    pub accepted_output_modes: Vec<String>,
    /// Push notification callbacks to use for every request.
    pub push_notification_configs: Vec<PushNotificationConfig>,
    /// A list of extension URIs the client supports.
    pub extensions: Vec<String>,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Maximum number of retry attempts.
    pub max_retries: u32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            streaming: true,
            polling: false,
            supported_transports: vec![TransportProtocol::JsonRpc],
            use_client_preference: false,
            accepted_output_modes: vec![],
            push_notification_configs: vec![],
            extensions: vec![],
            timeout_secs: 30,
            max_retries: 3,
        }
    }
}

impl ClientConfig {
    /// Creates a new client configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets whether streaming is enabled.
    pub fn streaming(mut self, enabled: bool) -> Self {
        self.streaming = enabled;
        self
    }

    /// Sets whether polling is preferred.
    pub fn polling(mut self, enabled: bool) -> Self {
        self.polling = enabled;
        self
    }

    /// Sets the accepted output modes.
    pub fn accepted_output_modes(mut self, modes: Vec<String>) -> Self {
        self.accepted_output_modes = modes;
        self
    }

    /// Sets the request timeout.
    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Sets the maximum number of retries.
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Adds a push notification configuration.
    pub fn push_notification(mut self, config: PushNotificationConfig) -> Self {
        self.push_notification_configs.push(config);
        self
    }

    /// Adds an extension URI.
    pub fn extension(mut self, uri: impl Into<String>) -> Self {
        self.extensions.push(uri.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ClientConfig::default();
        assert!(config.streaming);
        assert!(!config.polling);
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_config_builder() {
        let config = ClientConfig::new()
            .streaming(false)
            .timeout(60)
            .max_retries(5);

        assert!(!config.streaming);
        assert_eq!(config.timeout_secs, 60);
        assert_eq!(config.max_retries, 5);
    }
}
