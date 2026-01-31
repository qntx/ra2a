//! Agent card and capability types for the A2A protocol.
//!
//! The AgentCard is a self-describing manifest that provides essential metadata
//! about an agent including identity, capabilities, skills, and security requirements.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::SecurityScheme;

/// Supported A2A transport protocols.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportProtocol {
    /// JSON-RPC over HTTP.
    #[serde(rename = "JSONRPC")]
    JsonRpc,
    /// gRPC transport.
    #[serde(rename = "GRPC")]
    Grpc,
    /// HTTP+JSON (REST-like).
    #[serde(rename = "HTTP+JSON")]
    HttpJson,
}

impl Default for TransportProtocol {
    fn default() -> Self {
        Self::JsonRpc
    }
}

/// The AgentCard is a self-describing manifest for an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentCard {
    /// A human-readable name for the agent.
    pub name: String,
    /// A human-readable description of the agent.
    pub description: String,
    /// The preferred endpoint URL for interacting with the agent.
    pub url: String,
    /// The agent's own version number.
    pub version: String,
    /// Default set of supported input MIME types.
    pub default_input_modes: Vec<String>,
    /// Default set of supported output MIME types.
    pub default_output_modes: Vec<String>,
    /// A declaration of optional capabilities supported by the agent.
    pub capabilities: AgentCapabilities,
    /// The set of skills the agent can perform.
    pub skills: Vec<AgentSkill>,
    /// The version of the A2A protocol this agent supports.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    /// The transport protocol for the preferred endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_transport: Option<String>,
    /// A list of additional supported interfaces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_interfaces: Option<Vec<AgentInterface>>,
    /// Information about the agent's service provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<AgentProvider>,
    /// An optional URL to the agent's documentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
    /// An optional URL to an icon for the agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    /// A declaration of the security schemes available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_schemes: Option<HashMap<String, SecurityScheme>>,
    /// A list of security requirement objects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<Vec<HashMap<String, Vec<String>>>>,
    /// If true, the agent can provide an extended card to authenticated users.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_authenticated_extended_card: Option<bool>,
    /// JSON Web Signatures computed for this AgentCard.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signatures: Option<Vec<AgentCardSignature>>,
}

impl AgentCard {
    /// Creates a new AgentCard builder.
    pub fn builder(name: impl Into<String>, url: impl Into<String>) -> AgentCardBuilder {
        AgentCardBuilder::new(name, url)
    }

    /// Returns true if the agent supports streaming.
    pub fn supports_streaming(&self) -> bool {
        self.capabilities.streaming.unwrap_or(false)
    }

    /// Returns true if the agent supports push notifications.
    pub fn supports_push_notifications(&self) -> bool {
        self.capabilities.push_notifications.unwrap_or(false)
    }

    /// Finds a skill by its ID.
    pub fn find_skill(&self, skill_id: &str) -> Option<&AgentSkill> {
        self.skills.iter().find(|s| s.id == skill_id)
    }

    /// Returns all skill IDs.
    pub fn skill_ids(&self) -> Vec<&str> {
        self.skills.iter().map(|s| s.id.as_str()).collect()
    }
}

/// Builder for creating an AgentCard.
#[derive(Debug)]
pub struct AgentCardBuilder {
    card: AgentCard,
}

impl AgentCardBuilder {
    /// Creates a new builder with required fields.
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            card: AgentCard {
                name: name.into(),
                description: String::new(),
                url: url.into(),
                version: "1.0.0".to_string(),
                default_input_modes: vec!["text/plain".to_string()],
                default_output_modes: vec!["text/plain".to_string()],
                capabilities: AgentCapabilities::default(),
                skills: vec![],
                protocol_version: Some(crate::PROTOCOL_VERSION.to_string()),
                preferred_transport: Some("JSONRPC".to_string()),
                additional_interfaces: None,
                provider: None,
                documentation_url: None,
                icon_url: None,
                security_schemes: None,
                security: None,
                supports_authenticated_extended_card: None,
                signatures: None,
            },
        }
    }

    /// Sets the description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.card.description = description.into();
        self
    }

    /// Sets the version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.card.version = version.into();
        self
    }

    /// Sets the capabilities.
    pub fn capabilities(mut self, capabilities: AgentCapabilities) -> Self {
        self.card.capabilities = capabilities;
        self
    }

    /// Adds a skill.
    pub fn skill(mut self, skill: AgentSkill) -> Self {
        self.card.skills.push(skill);
        self
    }

    /// Sets multiple skills.
    pub fn skills(mut self, skills: Vec<AgentSkill>) -> Self {
        self.card.skills = skills;
        self
    }

    /// Sets the input modes.
    pub fn input_modes(mut self, modes: Vec<String>) -> Self {
        self.card.default_input_modes = modes;
        self
    }

    /// Sets the output modes.
    pub fn output_modes(mut self, modes: Vec<String>) -> Self {
        self.card.default_output_modes = modes;
        self
    }

    /// Sets the provider.
    pub fn provider(mut self, provider: AgentProvider) -> Self {
        self.card.provider = Some(provider);
        self
    }

    /// Builds the AgentCard.
    pub fn build(self) -> AgentCard {
        self.card
    }
}

/// Defines optional capabilities supported by an agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentCapabilities {
    /// Indicates if the agent supports Server-Sent Events (SSE) for streaming.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streaming: Option<bool>,
    /// Indicates if the agent supports push notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_notifications: Option<bool>,
    /// Indicates if the agent provides state transition history.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_transition_history: Option<bool>,
    /// A list of protocol extensions supported by the agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<AgentExtension>>,
}

impl AgentCapabilities {
    /// Creates new capabilities with streaming enabled.
    pub fn with_streaming() -> Self {
        Self {
            streaming: Some(true),
            ..Default::default()
        }
    }

    /// Creates new capabilities with push notifications enabled.
    pub fn with_push_notifications() -> Self {
        Self {
            push_notifications: Some(true),
            ..Default::default()
        }
    }
}

/// Represents a distinct capability or function that an agent can perform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentSkill {
    /// A unique identifier for the skill.
    pub id: String,
    /// A human-readable name for the skill.
    pub name: String,
    /// A detailed description of the skill.
    pub description: String,
    /// A set of keywords describing the skill's capabilities.
    pub tags: Vec<String>,
    /// Example prompts or scenarios that this skill can handle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<String>>,
    /// The set of supported input MIME types for this skill.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_modes: Option<Vec<String>>,
    /// The set of supported output MIME types for this skill.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_modes: Option<Vec<String>>,
    /// Security schemes necessary for this skill.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<Vec<HashMap<String, Vec<String>>>>,
}

impl AgentSkill {
    /// Creates a new skill with required fields.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        tags: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            tags,
            examples: None,
            input_modes: None,
            output_modes: None,
            security: None,
        }
    }

    /// Sets the examples for this skill.
    pub fn with_examples(mut self, examples: Vec<String>) -> Self {
        self.examples = Some(examples);
        self
    }
}

/// A declaration of a protocol extension supported by an Agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentExtension {
    /// The unique URI identifying the extension.
    pub uri: String,
    /// A human-readable description of how this agent uses the extension.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// If true, the client must understand the extension.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Optional extension-specific configuration parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, serde_json::Value>>,
}

/// Declares a combination of a target URL and transport protocol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentInterface {
    /// The URL where this interface is available.
    pub url: String,
    /// The transport protocol supported at this URL.
    pub transport: String,
}

impl AgentInterface {
    /// Creates a new interface.
    pub fn new(url: impl Into<String>, transport: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            transport: transport.into(),
        }
    }
}

/// Represents the service provider of an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentProvider {
    /// The name of the agent provider's organization.
    pub organization: String,
    /// A URL for the agent provider's website.
    pub url: String,
}

impl AgentProvider {
    /// Creates a new provider.
    pub fn new(organization: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            organization: organization.into(),
            url: url.into(),
        }
    }
}

/// Represents a JWS signature of an AgentCard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentCardSignature {
    /// The protected JWS header (Base64url-encoded JSON object).
    pub protected: String,
    /// The computed signature (Base64url-encoded).
    pub signature: String,
    /// The unprotected JWS header values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<HashMap<String, serde_json::Value>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_card_builder() {
        let card = AgentCard::builder("Test Agent", "https://example.com")
            .description("A test agent")
            .version("1.0.0")
            .skill(AgentSkill::new(
                "greet",
                "Greeting",
                "Greets the user",
                vec!["greeting".to_string()],
            ))
            .build();

        assert_eq!(card.name, "Test Agent");
        assert_eq!(card.skills.len(), 1);
        assert_eq!(card.skills[0].id, "greet");
    }

    #[test]
    fn test_agent_capabilities() {
        let caps = AgentCapabilities::with_streaming();
        assert_eq!(caps.streaming, Some(true));
        assert_eq!(caps.push_notifications, None);
    }

    #[test]
    fn test_agent_card_serialization() {
        let card = AgentCard::builder("Test", "https://test.com")
            .description("Test agent")
            .build();
        let json = serde_json::to_string(&card).unwrap();
        assert!(json.contains("\"name\":\"Test\""));
    }
}
