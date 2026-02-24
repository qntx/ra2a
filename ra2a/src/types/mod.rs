//! A2A Protocol types and data models.
//!
//! Core type definitions for messages, tasks, agent cards, events,
//! JSON-RPC structures, and security schemes.
//!
//! Aligned with Go's `a2a` package layout.

mod agent;
mod jsonrpc;
mod message;
mod part;
mod push;
mod task;

// -- Agent card, capabilities & security (merged from agent.rs + security.rs + oauth.rs) --
pub use agent::{
    AgentCapabilities, AgentCard, AgentCardBuilder, AgentCardSignature, AgentExtension,
    AgentInterface, AgentProvider, AgentSkill, ApiKeyLocation, ApiKeySecurityScheme,
    AuthorizationCodeOAuthFlow, ClientCredentialsOAuthFlow, HttpAuthSecurityScheme,
    ImplicitOAuthFlow, MutualTlsSecurityScheme, OAuth2SecurityScheme, OAuthFlows,
    OpenIdConnectSecurityScheme, PasswordOAuthFlow, SecurityScheme, TransportProtocol,
};
// -- JSON-RPC wire types --
pub use jsonrpc::{
    A2AResponse, GetAuthenticatedExtendedCardParams, JSONRPC_VERSION, JsonRpcErrorResponse,
    JsonRpcRequest, JsonRpcResponse, JsonRpcSuccessResponse, ListTasksRequest, ListTasksResponse,
    MessageSendConfig, MessageSendParams, RequestId, TaskIdParams, TaskQueryParams,
};
// -- Message --
pub use message::{Message, Role};
// -- Content parts --
pub use part::{DataPart, FileBytes, FileContent, FilePart, FileUri, Part, TextPart};
// -- Push notification types (aligned with Go's push.go) --
pub use push::{
    DeleteTaskPushConfigParams, GetTaskPushConfigParams, ListTaskPushConfigParams, PushAuthInfo,
    PushConfig, TaskPushConfig,
};
// -- Task & events --
pub use task::{
    Artifact, Event, SendMessageResult, Task, TaskArtifactUpdateEvent, TaskState, TaskStatus,
    TaskStatusUpdateEvent, TaskVersion,
};

/// Extension metadata map used throughout the A2A protocol.
///
/// Aligned with Go's `map[string]any` used for metadata fields.
pub type Metadata = std::collections::HashMap<String, serde_json::Value>;

/// Helper for serde: skip serializing boolean fields when false.
pub(crate) fn is_false(v: &bool) -> bool {
    !v
}
