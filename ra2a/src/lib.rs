//! # A2A Rust SDK
//!
//! A Rust implementation of the Agent2Agent (A2A) Protocol SDK.
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
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use ra2a::client::{A2AClient, Client};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), ra2a::error::A2AError> {
//!     let client = A2AClient::new("https://agent.example.com")?;
//!     let card = client.get_agent_card().await?;
//!     println!("Connected to agent: {}", card.name);
//!     Ok(())
//! }
//! ```

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

// Internal utility modules
mod crypto;
mod http_helpers;

/// Task helper utilities for creating and manipulating A2A objects.
///
/// This module provides utility functions similar to Python's `a2a/utils/helpers.py`.
pub mod task_helpers;

/// Telemetry and tracing utilities for A2A operations.
///
/// This module provides comprehensive OpenTelemetry integration including:
/// - Span builders with automatic attribute recording
/// - Pre-defined spans for common A2A operations
/// - Metrics collection and timing utilities
///
/// Enable the `telemetry` feature for full OpenTelemetry integration.
pub mod telemetry;

// Re-export commonly used types at crate root
pub use error::{A2AError, Result};
pub use types::{
    // Request/Response types
    A2ARequest,
    A2AResponse,
    A2ASuccessResponse,
    // Agent types
    AgentCapabilities,
    AgentCard,
    AgentSkill,
    // Task types
    Artifact,
    // Extension types
    ExtensionContext,
    ExtensionDeclaration,
    JsonRpcErrorResponse,
    JsonRpcRequest,
    JsonRpcResponse,
    JsonRpcSuccessResponse,
    // Message types
    Message,
    MessageSendConfiguration,
    MessageSendParams,
    MessageStreamParams,
    Part,
    PushNotificationConfig,
    RequestId,
    Role,
    SseEvent,
    StreamingEvent,
    Task,
    TaskArtifactUpdateEvent,
    // Parameter types
    TaskIdParams,
    TaskPushNotificationConfig,
    TaskQueryParams,
    TaskResubscriptionParams,
    TaskState,
    TaskStatus,
    TaskStatusUpdateEvent,
};

// Re-export client types when client feature is enabled
#[cfg(feature = "client")]
pub use client::{
    CallbackConsumer, Client, ClientEvent, CollectingConsumer, Consumer, UpdateEvent, run_consumer,
    send_and_consume,
};

// Re-export server types when server feature is enabled
#[cfg(feature = "server")]
pub use server::{
    AgentExecutor, DefaultRequestHandler, Event, EventQueue, ExecutionContext, QueueManager,
    RequestHandler, SendMessageResponse, ServerCallContext,
};

/// Protocol version supported by this SDK
pub const PROTOCOL_VERSION: &str = "0.3.0";

/// SDK version
pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");
