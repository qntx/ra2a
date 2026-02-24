//! gRPC infrastructure for the A2A protocol.
//!
//! This module provides shared gRPC infrastructure (proto types, conversions)
//! and the server-side gRPC service implementation. The client-side
//! [`GrpcTransport`](crate::client::GrpcTransport) lives in [`crate::client`].
//!
//! # Architecture
//!
//! - **Proto types**: Generated protobuf definitions in [`proto`]
//! - **Conversions**: Bidirectional native ↔ proto type conversions
//! - **Server**: [`GrpcServiceImpl`] implements the generated `A2aService` trait
//!
//! The server follows the SDK's composable design — users own the tonic server
//! and attach [`GrpcServiceImpl`] via [`A2aServiceServer`].

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

pub(crate) mod convert;
mod server;

pub use convert::{hashmap_to_struct, json_to_struct, struct_to_hashmap, struct_to_json};
pub use proto::a2a_service_client::A2aServiceClient;
pub use proto::a2a_service_server::{A2aService, A2aServiceServer};
pub use server::GrpcServiceImpl;
