//! Transport layer implementations for the A2A client.
//!
//! This module provides different transport protocols for communicating
//! with A2A agents, including JSON-RPC, REST, and gRPC.

mod base;
mod jsonrpc;
mod rest;

pub use base::*;
pub use jsonrpc::*;
pub use rest::*;

// #[cfg(feature = "grpc")]
// mod grpc;

// #[cfg(feature = "grpc")]
// pub use grpc::*;
