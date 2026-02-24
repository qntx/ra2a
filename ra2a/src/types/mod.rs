//! A2A Protocol types and data models.
//!
//! Core type definitions for messages, tasks, agent cards, events,
//! JSON-RPC structures, and security schemes.
//!
//! Aligned with Go's `a2a` package layout.

mod agent;
mod message;
mod params;
mod part;
mod push;
mod task;

pub use agent::{
    AgentCapabilities, AgentCard, AgentCardSignature, AgentExtension, AgentInterface,
    AgentProvider, AgentSkill, ApiKeyLocation, ApiKeySecurityScheme, AuthorizationCodeOAuthFlow,
    ClientCredentialsOAuthFlow, HttpAuthSecurityScheme, ImplicitOAuthFlow, MutualTlsSecurityScheme,
    OAuth2SecurityScheme, OAuthFlows, OpenIdConnectSecurityScheme, PasswordOAuthFlow,
    SecurityScheme, TransportProtocol,
};
pub use message::{Message, Role};
pub use params::{
    ListTasksRequest, ListTasksResponse, MessageSendConfig, MessageSendParams, TaskIdParams,
    TaskQueryParams,
};
pub use part::{DataPart, FileBytes, FileContent, FilePart, FileUri, Part, TextPart};
pub use push::{
    DeleteTaskPushConfigParams, GetTaskPushConfigParams, ListTaskPushConfigParams, PushAuthInfo,
    PushConfig, TaskPushConfig,
};
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
