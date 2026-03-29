//! Agent card, capability, skill, and related types for the A2A protocol.
//!
//! Maps to proto `AgentCard`, `AgentInterface`, `AgentCapabilities`,
//! `AgentSkill`, `AgentExtension`, `AgentProvider`, `AgentCardSignature`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::Metadata;
use super::security::{SecurityRequirement, SecurityScheme};

/// A2A transport protocol identifier.
///
/// An open string type — custom protocols are allowed.
/// Use the provided constants for well-known protocols.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct TransportProtocol(pub String);

impl TransportProtocol {
    /// JSON-RPC over HTTP.
    pub const JSONRPC: &'static str = "JSONRPC";
    /// gRPC transport.
    pub const GRPC: &'static str = "GRPC";
    /// HTTP+JSON (REST-like).
    pub const HTTP_JSON: &'static str = "HTTP+JSON";

    /// Creates a new transport protocol from a string.
    pub fn new(protocol: impl Into<String>) -> Self {
        Self(protocol.into())
    }

    /// Returns the protocol string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns `true` if this is the JSON-RPC protocol.
    #[must_use]
    pub fn is_jsonrpc(&self) -> bool {
        self.0 == Self::JSONRPC
    }

    /// Returns `true` if this is the gRPC protocol.
    #[must_use]
    pub fn is_grpc(&self) -> bool {
        self.0 == Self::GRPC
    }

    /// Returns `true` if this is the HTTP+JSON protocol.
    #[must_use]
    pub fn is_http_json(&self) -> bool {
        self.0 == Self::HTTP_JSON
    }
}

impl std::fmt::Display for TransportProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for TransportProtocol {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<String> for TransportProtocol {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Declares a combination of a target URL, transport, and protocol version.
///
/// Maps to proto `AgentInterface`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInterface {
    /// The URL where this interface is available.
    pub url: String,
    /// The protocol binding supported at this URL (e.g. `"JSONRPC"`, `"GRPC"`, `"HTTP+JSON"`).
    pub protocol_binding: TransportProtocol,
    /// Optional tenant to set in requests when calling the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// The version of the A2A protocol this interface exposes (e.g. `"1.0"`).
    pub protocol_version: String,
}

impl AgentInterface {
    /// Creates a new interface with the given URL and protocol binding.
    pub fn new(url: impl Into<String>, protocol_binding: TransportProtocol) -> Self {
        Self {
            url: url.into(),
            protocol_binding,
            tenant: None,
            protocol_version: crate::PROTOCOL_VERSION.to_owned(),
        }
    }
}

/// A self-describing manifest for an agent.
///
/// Maps to proto `AgentCard`. v1.0 uses `supported_interfaces` as the primary
/// field for declaring endpoints (replaces the old `url` + `protocolVersion` pattern).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    /// A human-readable name for the agent.
    pub name: String,
    /// A human-readable description of the agent.
    pub description: String,
    /// Ordered list of supported interfaces. First entry is preferred.
    pub supported_interfaces: Vec<AgentInterface>,
    /// Information about the agent's service provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<AgentProvider>,
    /// The agent's own version number.
    pub version: String,
    /// URL to the agent's documentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
    /// Capabilities supported by the agent.
    pub capabilities: AgentCapabilities,
    /// Security schemes available to authorize requests (keyed by scheme name).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub security_schemes: HashMap<String, SecurityScheme>,
    /// Security requirements for contacting the agent (OR-of-ANDs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security_requirements: Vec<SecurityRequirement>,
    /// Default set of supported input MIME types for all skills.
    pub default_input_modes: Vec<String>,
    /// Default set of supported output MIME types for all skills.
    pub default_output_modes: Vec<String>,
    /// Skills the agent can perform.
    pub skills: Vec<AgentSkill>,
    /// JSON Web Signatures computed for this `AgentCard`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signatures: Vec<AgentCardSignature>,
    /// URL to an icon for the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

impl AgentCard {
    /// Creates a new `AgentCard` with minimal required fields.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        interfaces: Vec<AgentInterface>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            supported_interfaces: interfaces,
            provider: None,
            version: "1.0.0".into(),
            documentation_url: None,
            capabilities: AgentCapabilities::default(),
            security_schemes: HashMap::new(),
            security_requirements: Vec::new(),
            default_input_modes: vec!["text/plain".into()],
            default_output_modes: vec!["text/plain".into()],
            skills: Vec::new(),
            signatures: Vec::new(),
            icon_url: None,
        }
    }

    /// Returns `true` if the agent supports streaming.
    #[must_use]
    pub fn supports_streaming(&self) -> bool {
        self.capabilities.streaming.unwrap_or(false)
    }

    /// Returns `true` if the agent supports push notifications.
    #[must_use]
    pub fn supports_push_notifications(&self) -> bool {
        self.capabilities.push_notifications.unwrap_or(false)
    }

    /// Returns `true` if the agent provides an extended agent card when authenticated.
    #[must_use]
    pub fn supports_extended_card(&self) -> bool {
        self.capabilities.extended_agent_card.unwrap_or(false)
    }

    /// Finds a skill by its ID.
    #[must_use]
    pub fn find_skill(&self, skill_id: &str) -> Option<&AgentSkill> {
        self.skills.iter().find(|s| s.id == skill_id)
    }

    /// Returns the preferred (first) interface, if any.
    #[must_use]
    pub fn preferred_interface(&self) -> Option<&AgentInterface> {
        self.supported_interfaces.first()
    }

    /// Finds the first interface matching the given protocol.
    #[must_use]
    pub fn find_interface(&self, protocol: &TransportProtocol) -> Option<&AgentInterface> {
        self.supported_interfaces
            .iter()
            .find(|i| &i.protocol_binding == protocol)
    }
}

/// Optional capabilities supported by an agent.
///
/// Maps to proto `AgentCapabilities`. v1.0 moves `extended_agent_card` here
/// (previously a top-level `AgentCard` field).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    /// Indicates if the agent supports streaming responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub streaming: Option<bool>,
    /// Indicates if the agent supports push notifications.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push_notifications: Option<bool>,
    /// Protocol extensions supported by the agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<AgentExtension>,
    /// Indicates if the agent provides an extended agent card when authenticated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extended_agent_card: Option<bool>,
}

/// A distinct capability or function that an agent can perform.
///
/// Maps to proto `AgentSkill`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkill {
    /// A unique identifier for the skill.
    pub id: String,
    /// A human-readable name for the skill.
    pub name: String,
    /// A detailed description of the skill.
    pub description: String,
    /// Keywords describing the skill's capabilities.
    pub tags: Vec<String>,
    /// Example prompts or scenarios.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
    /// Supported input MIME types (overrides agent defaults).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_modes: Vec<String>,
    /// Supported output MIME types (overrides agent defaults).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_modes: Vec<String>,
    /// Security requirements for this skill (OR-of-ANDs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security_requirements: Vec<SecurityRequirement>,
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
            examples: Vec::new(),
            input_modes: Vec::new(),
            output_modes: Vec::new(),
            security_requirements: Vec::new(),
        }
    }

    /// Sets the examples.
    #[must_use]
    pub fn with_examples(mut self, examples: Vec<String>) -> Self {
        self.examples = examples;
        self
    }
}

/// A protocol extension supported by an agent.
///
/// Maps to proto `AgentExtension`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentExtension {
    /// The unique URI identifying the extension.
    pub uri: String,
    /// Description of how this agent uses the extension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// If `true`, the client must understand and comply with the extension.
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub required: bool,
    /// Extension-specific configuration parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Metadata>,
}

/// Represents the service provider of an agent.
///
/// Maps to proto `AgentProvider`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

/// A JWS signature of an `AgentCard` (RFC 7515).
///
/// Maps to proto `AgentCardSignature`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentCardSignature {
    /// The protected JWS header (Base64url-encoded JSON object).
    pub protected: String,
    /// The computed signature (Base64url-encoded).
    pub signature: String,
    /// The unprotected JWS header values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<Metadata>,
}
