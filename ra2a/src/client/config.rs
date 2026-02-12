//! Client configuration types.

use crate::types::{PushConfig, TransportProtocol};

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
    pub push_notification_configs: Vec<PushConfig>,
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
    #[must_use] 
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets whether streaming is enabled.
    #[must_use] 
    pub const fn streaming(mut self, enabled: bool) -> Self {
        self.streaming = enabled;
        self
    }

    /// Sets whether polling is preferred.
    #[must_use] 
    pub const fn polling(mut self, enabled: bool) -> Self {
        self.polling = enabled;
        self
    }

    /// Sets the accepted output modes.
    #[must_use] 
    pub fn accepted_output_modes(mut self, modes: Vec<String>) -> Self {
        self.accepted_output_modes = modes;
        self
    }

    /// Sets the request timeout.
    #[must_use] 
    pub const fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Sets the maximum number of retries.
    #[must_use] 
    pub const fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Adds a push notification configuration.
    #[must_use] 
    pub fn push_notification(mut self, config: PushConfig) -> Self {
        self.push_notification_configs.push(config);
        self
    }

    /// Adds an extension URI.
    pub fn extension(mut self, uri: impl Into<String>) -> Self {
        self.extensions.push(uri.into());
        self
    }

    /// Applies default send configuration to a [`MessageSendParams`].
    ///
    /// Aligned with Go's `Client.withDefaultSendConfig`:
    /// - Sets `blocking` based on whether the client is in polling mode.
    /// - Fills in `push_notification_config` from client config if not already set.
    /// - Fills in `accepted_output_modes` from client config if not already set.
    pub fn apply_send_defaults(&self, params: &mut crate::types::MessageSendParams) {
        let config = params.configuration.get_or_insert_with(Default::default);

        // Set blocking: true unless polling mode
        if config.blocking.is_none() {
            config.blocking = Some(!self.polling);
        }

        // Apply default push notification config
        if config.push_notification_config.is_none() {
            if let Some(first) = self.push_notification_configs.first() {
                config.push_notification_config = Some(first.clone());
            }
        }

        // Apply default accepted output modes
        if config.accepted_output_modes.is_none() && !self.accepted_output_modes.is_empty() {
            config.accepted_output_modes = Some(self.accepted_output_modes.clone());
        }
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

    #[test]
    fn test_apply_send_defaults() {
        use crate::types::{Message, MessageSendParams};

        let config = ClientConfig::new()
            .polling(true)
            .accepted_output_modes(vec!["text/plain".into()]);

        let mut params = MessageSendParams::new(Message::user(vec![]));
        config.apply_send_defaults(&mut params);

        let send_config = params.configuration.unwrap();
        // Polling mode → blocking = false
        assert_eq!(send_config.blocking, Some(false));
        assert_eq!(
            send_config.accepted_output_modes,
            Some(vec!["text/plain".to_string()])
        );
    }

    #[test]
    fn test_apply_send_defaults_does_not_overwrite() {
        use crate::types::{Message, MessageSendConfig, MessageSendParams};

        let config = ClientConfig::new()
            .accepted_output_modes(vec!["text/plain".into()]);

        let mut params = MessageSendParams::new(Message::user(vec![]))
            .with_configuration(MessageSendConfig {
                blocking: Some(false),
                accepted_output_modes: Some(vec!["application/json".into()]),
                ..Default::default()
            });

        config.apply_send_defaults(&mut params);

        let send_config = params.configuration.unwrap();
        // Should NOT overwrite existing values
        assert_eq!(send_config.blocking, Some(false));
        assert_eq!(
            send_config.accepted_output_modes,
            Some(vec!["application/json".to_string()])
        );
    }
}
