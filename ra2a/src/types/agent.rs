//! Agent card, capability, and security scheme types for the A2A protocol.
//!
//! Aligned with Go's `a2a` package: `agent.go` + `auth.go`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::Metadata;

// ---------------------------------------------------------------------------
// Transport protocol
// ---------------------------------------------------------------------------

/// A2A transport protocol identifier.
///
/// This is an open string type — custom protocols are allowed.
/// Use the provided constants for well-known protocols.
///
/// Aligned with Go's `TransportProtocol` string type: "Custom protocols are
/// allowed and the type MUST NOT be treated as an enum."
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
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true if this is the JSON-RPC protocol.
    pub fn is_jsonrpc(&self) -> bool {
        self.0 == Self::JSONRPC
    }

    /// Returns true if this is the gRPC protocol.
    pub fn is_grpc(&self) -> bool {
        self.0 == Self::GRPC
    }
}

impl std::fmt::Display for TransportProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for TransportProtocol {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for TransportProtocol {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ---------------------------------------------------------------------------
// AgentCard
// ---------------------------------------------------------------------------

/// A self-describing manifest for an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
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
    pub protocol_version: String,
    /// The transport protocol for the preferred endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_transport: Option<TransportProtocol>,
    /// Additional supported transport and URL combinations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_interfaces: Vec<AgentInterface>,
    /// Information about the agent's service provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<AgentProvider>,
    /// URL to the agent's documentation (empty = not set).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub documentation_url: String,
    /// URL to an icon for the agent (empty = not set).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon_url: String,
    /// Security schemes available to authorize requests (keyed by scheme name).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub security_schemes: HashMap<String, SecurityScheme>,
    /// Security requirement objects (OR-of-ANDs). Each map entry is scheme → scopes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security: Vec<HashMap<String, Vec<String>>>,
    /// If true, the agent can provide an extended card to authenticated users.
    #[serde(default, skip_serializing_if = "crate::types::is_false")]
    pub supports_authenticated_extended_card: bool,
    /// JSON Web Signatures computed for this `AgentCard`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signatures: Vec<AgentCardSignature>,
}

impl Default for AgentCard {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            url: String::new(),
            version: "1.0.0".to_string(),
            default_input_modes: vec!["text/plain".to_string()],
            default_output_modes: vec!["text/plain".to_string()],
            capabilities: AgentCapabilities::default(),
            skills: Vec::new(),
            protocol_version: crate::PROTOCOL_VERSION.to_string(),
            preferred_transport: Some(TransportProtocol::new(TransportProtocol::JSONRPC)),
            additional_interfaces: Vec::new(),
            provider: None,
            documentation_url: String::new(),
            icon_url: String::new(),
            security_schemes: HashMap::new(),
            security: Vec::new(),
            supports_authenticated_extended_card: false,
            signatures: Vec::new(),
        }
    }
}

impl AgentCard {
    /// Creates a new `AgentCard` with the given name and URL.
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            ..Default::default()
        }
    }

    /// Returns true if the agent supports streaming.
    #[must_use]
    pub const fn supports_streaming(&self) -> bool {
        self.capabilities.streaming
    }

    /// Returns true if the agent supports push notifications.
    #[must_use]
    pub const fn supports_push_notifications(&self) -> bool {
        self.capabilities.push_notifications
    }

    /// Finds a skill by its ID.
    #[must_use]
    pub fn find_skill(&self, skill_id: &str) -> Option<&AgentSkill> {
        self.skills.iter().find(|s| s.id == skill_id)
    }
}

// ---------------------------------------------------------------------------
// AgentCapabilities
// ---------------------------------------------------------------------------

/// Optional capabilities supported by an agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    /// Indicates if the agent supports SSE streaming.
    #[serde(default, skip_serializing_if = "crate::types::is_false")]
    pub streaming: bool,
    /// Indicates if the agent supports push notifications.
    #[serde(default, skip_serializing_if = "crate::types::is_false")]
    pub push_notifications: bool,
    /// Indicates if the agent provides state transition history.
    #[serde(default, skip_serializing_if = "crate::types::is_false")]
    pub state_transition_history: bool,
    /// Protocol extensions supported by the agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<AgentExtension>,
}

impl AgentCapabilities {
    /// Creates capabilities with streaming enabled.
    #[must_use]
    pub fn with_streaming() -> Self {
        Self {
            streaming: true,
            ..Default::default()
        }
    }

    /// Creates capabilities with push notifications enabled.
    #[must_use]
    pub fn with_push_notifications() -> Self {
        Self {
            push_notifications: true,
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// AgentSkill
// ---------------------------------------------------------------------------

/// A distinct capability or function that an agent can perform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    pub security: Vec<HashMap<String, Vec<String>>>,
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
            security: Vec::new(),
        }
    }

    /// Sets the examples for this skill.
    #[must_use]
    pub fn with_examples(mut self, examples: Vec<String>) -> Self {
        self.examples = examples;
        self
    }
}

// ---------------------------------------------------------------------------
// AgentExtension
// ---------------------------------------------------------------------------

/// A protocol extension supported by an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentExtension {
    /// The unique URI identifying the extension.
    pub uri: String,
    /// Description of how this agent uses the extension.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// If true, the client must understand the extension.
    #[serde(default, skip_serializing_if = "crate::types::is_false")]
    pub required: bool,
    /// Extension-specific configuration parameters.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub params: Metadata,
}

// ---------------------------------------------------------------------------
// AgentInterface / AgentProvider / AgentCardSignature
// ---------------------------------------------------------------------------

/// A combination of a target URL and transport protocol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentInterface {
    /// The URL where this interface is available.
    pub url: String,
    /// The transport protocol supported at this URL.
    pub transport: TransportProtocol,
}

impl AgentInterface {
    /// Creates a new interface.
    pub fn new(url: impl Into<String>, transport: TransportProtocol) -> Self {
        Self {
            url: url.into(),
            transport,
        }
    }
}

/// Represents the service provider of an agent.
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentCardSignature {
    /// The protected JWS header (Base64url-encoded JSON object).
    pub protected: String,
    /// The computed signature (Base64url-encoded).
    pub signature: String,
    /// The unprotected JWS header values.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub header: Metadata,
}

// ---------------------------------------------------------------------------
// Security schemes (merged from security.rs + oauth.rs, aligned with Go auth.go)
// ---------------------------------------------------------------------------

/// A security scheme for securing agent endpoints (OpenAPI 3.0 compatible).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SecurityScheme {
    /// API Key security scheme.
    ApiKey(ApiKeySecurityScheme),
    /// HTTP authentication security scheme.
    Http(HttpAuthSecurityScheme),
    /// OAuth 2.0 security scheme.
    #[serde(rename = "oauth2")]
    OAuth2(Box<OAuth2SecurityScheme>),
    /// OpenID Connect security scheme.
    OpenIdConnect(OpenIdConnectSecurityScheme),
    /// Mutual TLS security scheme.
    MutualTLS(MutualTlsSecurityScheme),
}

/// The location of an API key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    /// In a cookie.
    Cookie,
    /// In a header.
    Header,
    /// In a query parameter.
    Query,
}

/// API key security scheme.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiKeySecurityScheme {
    /// The name of the header, query, or cookie parameter.
    pub name: String,
    /// The location of the API key.
    #[serde(rename = "in")]
    pub location: ApiKeyLocation,
    /// An optional description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl ApiKeySecurityScheme {
    /// Creates a header-based API key scheme.
    pub fn header(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            location: ApiKeyLocation::Header,
            description: String::new(),
        }
    }

    /// Creates a query-based API key scheme.
    pub fn query(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            location: ApiKeyLocation::Query,
            description: String::new(),
        }
    }
}

/// HTTP authentication security scheme.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HttpAuthSecurityScheme {
    /// The HTTP authentication scheme name (e.g. "Bearer").
    pub scheme: String,
    /// An optional description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// A hint for how the bearer token is formatted (e.g. "JWT").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bearer_format: String,
}

impl HttpAuthSecurityScheme {
    /// Creates a Bearer token authentication scheme.
    #[must_use]
    pub fn bearer() -> Self {
        Self {
            scheme: "Bearer".into(),
            description: String::new(),
            bearer_format: String::new(),
        }
    }

    /// Creates a Bearer JWT authentication scheme.
    #[must_use]
    pub fn bearer_jwt() -> Self {
        Self {
            scheme: "Bearer".into(),
            description: String::new(),
            bearer_format: "JWT".into(),
        }
    }

    /// Creates a Basic authentication scheme.
    #[must_use]
    pub fn basic() -> Self {
        Self {
            scheme: "Basic".into(),
            description: String::new(),
            bearer_format: String::new(),
        }
    }
}

/// OAuth 2.0 security scheme.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2SecurityScheme {
    /// Configuration for the supported OAuth 2.0 flows.
    pub flows: OAuthFlows,
    /// An optional description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// URL to the OAuth2 authorization server metadata (RFC 8414).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub oauth2_metadata_url: String,
}

/// OpenID Connect security scheme.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenIdConnectSecurityScheme {
    /// The OpenID Connect Discovery URL.
    pub open_id_connect_url: String,
    /// An optional description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Mutual TLS security scheme.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MutualTlsSecurityScheme {
    /// An optional description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

// ---------------------------------------------------------------------------
// OAuth 2.0 flows (merged from oauth.rs, aligned with Go auth.go)
// ---------------------------------------------------------------------------

/// Configuration for the supported OAuth 2.0 flows.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OAuthFlows {
    /// Configuration for the Authorization Code flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_code: Option<AuthorizationCodeOAuthFlow>,
    /// Configuration for the Client Credentials flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_credentials: Option<ClientCredentialsOAuthFlow>,
    /// Configuration for the Implicit flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implicit: Option<ImplicitOAuthFlow>,
    /// Configuration for the Resource Owner Password flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<PasswordOAuthFlow>,
}

/// OAuth 2.0 Authorization Code flow configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationCodeOAuthFlow {
    /// The authorization URL.
    pub authorization_url: String,
    /// The token URL.
    pub token_url: String,
    /// Available scopes (scope name → description).
    pub scopes: HashMap<String, String>,
    /// Optional refresh URL.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub refresh_url: String,
}

/// OAuth 2.0 Client Credentials flow configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClientCredentialsOAuthFlow {
    /// The token URL.
    pub token_url: String,
    /// Available scopes (scope name → description).
    pub scopes: HashMap<String, String>,
    /// Optional refresh URL.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub refresh_url: String,
}

/// OAuth 2.0 Implicit flow configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImplicitOAuthFlow {
    /// The authorization URL.
    pub authorization_url: String,
    /// Available scopes (scope name → description).
    pub scopes: HashMap<String, String>,
    /// Optional refresh URL.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub refresh_url: String,
}

/// OAuth 2.0 Resource Owner Password flow configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PasswordOAuthFlow {
    /// The token URL.
    pub token_url: String,
    /// Available scopes (scope name → description).
    pub scopes: HashMap<String, String>,
    /// Optional refresh URL.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub refresh_url: String,
}
