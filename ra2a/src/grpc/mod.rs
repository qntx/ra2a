//! gRPC support for the A2A Rust SDK.
//!
//! This module provides gRPC client and server implementations for the A2A protocol.
//! It requires the `grpc` feature to be enabled.
//!
//! # Overview
//!
//! The gRPC module provides:
//! - Generated protobuf types in the `proto` submodule
//! - Type conversion utilities between proto and native SDK types
//! - gRPC client and server implementations (WIP)
//!
//! # Example
//!
//! ```rust,ignore
//! use ra2a::grpc::proto;
//!
//! // Use generated proto types
//! let task = proto::Task::default();
//! ```

// Include the generated protobuf code
#[allow(missing_docs)]
#[allow(clippy::all)]
pub mod proto {
    //! Generated protobuf types and service definitions.
    //!
    //! This module contains types generated from the A2A protocol buffer definitions.
    //! Documentation for these types can be found in the original proto files.
    tonic::include_proto!("a2a.v1");
}

mod convert;

pub use convert::{hashmap_to_struct, json_to_struct, struct_to_hashmap, struct_to_json};

// Re-export commonly used types from proto
pub use proto::a2a_service_client::A2aServiceClient;
pub use proto::a2a_service_server::{A2aService, A2aServiceServer};
