//! A2A Protocol v1.0 types and data models.
//!
//! Core type definitions for messages, tasks, agent cards, events,
//! security schemes, push notifications, and request/response types.
//!
//! All types are derived from the authoritative `a2a.proto` specification.

pub mod agent;
pub mod artifact;
pub mod event;
pub mod id;
pub mod message;
pub mod part;
pub mod push;
pub mod request;
pub mod response;
pub mod security;
pub mod task;

pub use agent::{
    AgentCapabilities, AgentCard, AgentCardSignature, AgentExtension, AgentInterface,
    AgentProvider, AgentSkill, TransportProtocol,
};
pub use artifact::Artifact;
pub use event::{
    SendMessageResponse, StreamResponse, TaskArtifactUpdateEvent, TaskStatusUpdateEvent,
};
pub use id::{ArtifactId, ContextId, MessageId, TaskId};
pub use message::{Message, Role};
pub use part::{Part, PartContent};
pub use push::{AuthenticationInfo, PushNotificationConfig, TaskPushNotificationConfig};
pub use request::{
    CancelTaskRequest, CreateTaskPushNotificationConfigRequest,
    DeleteTaskPushNotificationConfigRequest, GetExtendedAgentCardRequest,
    GetTaskPushNotificationConfigRequest, GetTaskRequest, ListTaskPushNotificationConfigRequest,
    ListTasksRequest, SendMessageConfiguration, SendMessageRequest, SubscribeToTaskRequest,
};
pub use response::{ListTaskPushNotificationConfigResponse, ListTasksResponse};
pub use security::{
    ApiKeyLocation, ApiKeySecurityScheme, AuthorizationCodeOAuthFlow, ClientCredentialsOAuthFlow,
    DeviceCodeOAuthFlow, HttpAuthSecurityScheme, MutualTlsSecurityScheme, OAuth2SecurityScheme,
    OAuthFlow, OpenIdConnectSecurityScheme, SecurityRequirement, SecurityScheme,
};
pub use task::{Task, TaskState, TaskStatus};

/// Extension metadata map used throughout the A2A protocol.
///
/// Corresponds to proto `google.protobuf.Struct`.
pub type Metadata = std::collections::HashMap<String, serde_json::Value>;

/// Helper for serde: skip serializing boolean fields when `false`.
pub(crate) fn is_false(v: &bool) -> bool {
    !v
}
