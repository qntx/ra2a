//! # A2A Rust SDK
//!
//! A Rust implementation of the [Agent2Agent (A2A) Protocol](https://a2a-protocol.org) v1.0.
//!
//! This crate provides a complete implementation of the A2A protocol for building
//! agentic applications that can communicate with each other.
//!
//! ## Features
//!
//! - **A2A Protocol v1.0 Compliant**: Full implementation of the latest specification
//! - **Async/Await**: Built on tokio for high-performance async operations
//! - **Type-Safe**: Strongly typed models with newtype IDs and serde serialization
//! - **Extensible**: Modular design with optional features (gRPC, telemetry, SQL)

#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

#[cfg(test)]
extern crate mockito as _;
#[cfg(test)]
extern crate pretty_assertions as _;
#[cfg(test)]
extern crate tokio_test as _;
#[cfg(test)]
extern crate tracing_subscriber as _;

#[cfg(feature = "grpc")]
extern crate google_api_proto as _;
#[cfg(feature = "telemetry")]
extern crate opentelemetry as _;
#[cfg(feature = "telemetry")]
extern crate tracing_opentelemetry as _;

pub mod error;
pub(crate) mod jsonrpc;
pub mod types;

#[cfg(feature = "client")]
#[cfg_attr(docsrs, doc(cfg(feature = "client")))]
pub mod client;

#[cfg(feature = "server")]
#[cfg_attr(docsrs, doc(cfg(feature = "server")))]
pub mod server;

#[cfg(feature = "grpc")]
#[cfg_attr(docsrs, doc(cfg(feature = "grpc")))]
pub mod grpc;

pub use error::{A2AError, Result};
pub use types::{
    AgentCapabilities, AgentCard, AgentInterface, AgentSkill, Artifact, ArtifactId, ContextId,
    ListTasksRequest, ListTasksResponse, Message, MessageId, Metadata, Part, PartContent,
    PushNotificationConfig, Role, SendMessageConfiguration, SendMessageRequest,
    SendMessageResponse, StreamResponse, Task, TaskArtifactUpdateEvent, TaskId, TaskState,
    TaskStatus, TaskStatusUpdateEvent, TransportProtocol,
};

/// Protocol version supported by this SDK.
pub const PROTOCOL_VERSION: &str = "1.0";

/// The service parameter key for the A2A protocol version.
pub const SVC_PARAM_VERSION: &str = "A2A-Version";

/// The service parameter key for A2A extensions.
pub const SVC_PARAM_EXTENSIONS: &str = "A2A-Extensions";

/// Well-known path for the public agent card endpoint.
pub const WELL_KNOWN_AGENT_CARD_PATH: &str = "/.well-known/agent-card.json";

/// Constructs the full agent card URL from a base URL.
///
/// Handles trailing slashes: both `"https://example.com"` and
/// `"https://example.com/"` produce `"https://example.com/.well-known/agent-card.json"`.
#[must_use]
pub fn agent_card_url(base_url: &str) -> String {
    format!(
        "{}{}",
        base_url.trim_end_matches('/'),
        WELL_KNOWN_AGENT_CARD_PATH
    )
}

/// SDK version.
pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");
