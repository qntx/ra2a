//! Client factory for creating A2A clients based on agent capabilities.
//!
//! The ClientFactory automatically selects the appropriate transport
//! based on the agent's advertised capabilities in its AgentCard.

use crate::error::{A2AError, Result};
use crate::types::AgentCard;

use super::transports::{
    ClientTransport, JsonRpcTransport, RestTransport, TransportOptions, TransportType,
};

/// Factory for creating A2A client transports.
///
/// Automatically selects the best transport based on agent capabilities.
#[derive(Debug, Clone)]
pub struct ClientFactory {
    /// Default transport options.
    options: TransportOptions,
    /// Preferred transport type (if any).
    preferred_transport: Option<TransportType>,
}

impl ClientFactory {
    /// Creates a new client factory with default options.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            options: TransportOptions::new(base_url),
            preferred_transport: None,
        }
    }

    /// Creates a factory with custom transport options.
    pub fn with_options(options: TransportOptions) -> Self {
        Self {
            options,
            preferred_transport: None,
        }
    }

    /// Sets the preferred transport type.
    pub fn prefer_transport(mut self, transport: TransportType) -> Self {
        self.preferred_transport = Some(transport);
        self
    }

    /// Sets the request timeout.
    pub fn timeout(mut self, secs: u64) -> Self {
        self.options = self.options.timeout(secs);
        self
    }

    /// Adds a Bearer token for authentication.
    pub fn bearer_auth(mut self, token: impl Into<String>) -> Self {
        self.options = self.options.bearer_auth(token);
        self
    }

    /// Adds an API key header.
    pub fn api_key(mut self, header_name: impl Into<String>, key: impl Into<String>) -> Self {
        self.options = self.options.api_key(header_name, key);
        self
    }

    /// Creates a client transport based on the agent card.
    ///
    /// Automatically selects the best transport based on:
    /// 1. User's preferred transport (if set)
    /// 2. Agent's preferred transport
    /// 3. Available additional interfaces
    /// 4. Default to JSON-RPC
    pub fn create_transport(&self, card: &AgentCard) -> Result<Box<dyn ClientTransport>> {
        let transport_type = self.select_transport_type(card);
        self.create_transport_of_type(transport_type)
    }

    /// Creates a transport of the specified type.
    pub fn create_transport_of_type(
        &self,
        transport_type: TransportType,
    ) -> Result<Box<dyn ClientTransport>> {
        match transport_type {
            TransportType::JsonRpc => {
                let transport = JsonRpcTransport::new(self.options.clone())?;
                Ok(Box::new(transport))
            }
            TransportType::Rest => {
                let transport = RestTransport::new(self.options.clone())?;
                Ok(Box::new(transport))
            }
            TransportType::Grpc => Err(A2AError::InvalidConfig(
                "gRPC transport not yet implemented".to_string(),
            )),
        }
    }

    /// Creates a JSON-RPC transport.
    pub fn create_jsonrpc_transport(&self) -> Result<JsonRpcTransport> {
        JsonRpcTransport::new(self.options.clone())
    }

    /// Creates a REST transport.
    pub fn create_rest_transport(&self) -> Result<RestTransport> {
        RestTransport::new(self.options.clone())
    }

    /// Selects the transport type based on agent card and preferences.
    fn select_transport_type(&self, card: &AgentCard) -> TransportType {
        // 1. User's explicit preference takes precedence
        if let Some(preferred) = self.preferred_transport {
            return preferred;
        }

        // 2. Check agent's preferred transport
        if let Some(ref preferred) = card.preferred_transport {
            if let Ok(transport) = preferred.parse::<TransportType>() {
                return transport;
            }
        }

        // 3. Check additional interfaces for supported transports
        if let Some(ref interfaces) = card.additional_interfaces {
            for interface in interfaces {
                if let Ok(transport) = interface.transport.parse::<TransportType>() {
                    // Prefer gRPC if available, then REST, then JSON-RPC
                    match transport {
                        TransportType::Grpc => {
                            // gRPC not yet implemented, skip
                        }
                        TransportType::Rest => return TransportType::Rest,
                        TransportType::JsonRpc => {}
                    }
                }
            }
        }

        // 4. Default to JSON-RPC
        TransportType::JsonRpc
    }
}

impl Default for ClientFactory {
    fn default() -> Self {
        Self {
            options: TransportOptions::new(""),
            preferred_transport: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AgentSkill;

    fn create_test_card() -> AgentCard {
        AgentCard::builder("Test Agent", "https://example.com")
            .description("A test agent")
            .skill(AgentSkill::new(
                "test",
                "Test Skill",
                "A test skill",
                vec!["test".to_string()],
            ))
            .build()
    }

    #[test]
    fn test_factory_creation() {
        let factory = ClientFactory::new("https://example.com");
        assert!(factory.preferred_transport.is_none());
    }

    #[test]
    fn test_transport_selection_default() {
        let factory = ClientFactory::new("https://example.com");
        let card = create_test_card();
        let transport_type = factory.select_transport_type(&card);
        assert_eq!(transport_type, TransportType::JsonRpc);
    }

    #[test]
    fn test_transport_selection_preferred() {
        let factory =
            ClientFactory::new("https://example.com").prefer_transport(TransportType::Rest);
        let card = create_test_card();
        let transport_type = factory.select_transport_type(&card);
        assert_eq!(transport_type, TransportType::Rest);
    }

    #[test]
    fn test_create_jsonrpc_transport() {
        let factory = ClientFactory::new("https://example.com");
        let transport = factory.create_jsonrpc_transport();
        assert!(transport.is_ok());
    }

    #[test]
    fn test_create_rest_transport() {
        let factory = ClientFactory::new("https://example.com");
        let transport = factory.create_rest_transport();
        assert!(transport.is_ok());
    }
}
