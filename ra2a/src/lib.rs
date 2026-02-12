//! # A2A Rust SDK
//!
//! A Rust implementation of the `Agent2Agent` (A2A) Protocol SDK.
//!
//! This crate provides a complete implementation of the A2A protocol for building
//! agentic applications that can communicate with each other following the
//! [Agent2Agent Protocol](https://a2a-protocol.org).
//!
//! ## Features
//!
//! - **A2A Protocol Compliant**: Full implementation of the A2A specification
//! - **Async/Await**: Built on tokio for high-performance async operations
//! - **Type-Safe**: Strongly typed models with serde serialization
//! - **Extensible**: Modular design with optional features

#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod error;
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

// Re-export commonly used types at crate root.
#[cfg(feature = "client")]
pub use client::{
    AuthInterceptor, CallMeta, Client, ClientCallInterceptor, ClientEvent, ClientRequest,
    ClientResponse, Consumer, ExtensionActivator, InterceptedClient, LoggingInterceptor,
    PassthroughClientInterceptor, StaticCallMetaInjector, UpdateEvent,
};
pub use error::{A2AError, Result};
#[cfg(feature = "server")]
pub use server::{
    AgentCardProducer, AgentExecutor, AuthenticatedUser, CallContext, CallInterceptor,
    ConcurrencyConfig, DefaultRequestHandler, Event, EventQueue, HandlerBuilder, HttpPushSender,
    InMemoryPushConfigStore, InterceptedHandler, PassthroughInterceptor, PushConfigStore,
    PushSender, QueueManager, ReferencedTasksLoader, RequestContext, RequestContextInterceptor,
    RequestHandler, RequestMeta, SendMessageResponse, User, EXTENSIONS_META_KEY,
};
pub use types::{
    A2ARequest, A2AResponse, AgentCapabilities, AgentCard, AgentSkill, Artifact,
    JsonRpcErrorResponse, JsonRpcRequest, JsonRpcResponse, JsonRpcSuccessResponse,
    ListTasksRequest, ListTasksResponse, Message, MessageSendParams, Part, PushConfig, RequestId,
    Role, SendMessageResult, Task, TaskArtifactUpdateEvent, TaskIdParams, TaskPushConfig,
    TaskQueryParams, TaskState, TaskStatus, TaskStatusUpdateEvent, TaskVersion,
};

/// Protocol version supported by this SDK.
pub const PROTOCOL_VERSION: &str = "0.3.0";

/// Well-known path for the public agent card endpoint (aligned with Go's `WellKnownAgentCardPath`).
pub const WELL_KNOWN_AGENT_CARD_PATH: &str = "/.well-known/agent-card.json";

/// SDK version
pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");
