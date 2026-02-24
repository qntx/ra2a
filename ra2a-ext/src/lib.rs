//! # ra2a-ext
//!
//! Extension utilities for the [A2A Rust SDK](https://crates.io/crates/ra2a).
//!
//! This crate provides [`CallInterceptor`](ra2a::client::CallInterceptor) and
//! server-side [`CallInterceptor`](ra2a::server::CallInterceptor) implementations
//! for activating and propagating A2A protocol extensions across agent chains.
//!
//! Aligned with Go's `a2aext` package.
//!
//! ## Components
//!
//! - **[`ExtensionActivator`]** — Client interceptor that requests extension
//!   activation on outgoing calls, filtering by server-supported extensions.
//! - **[`ServerPropagator`]** — Server interceptor that extracts extension-related
//!   metadata and headers from incoming requests for downstream propagation.
//! - **[`ClientPropagator`]** — Client interceptor that injects propagated extension
//!   data into outgoing requests (for agent-to-agent chaining: A → B → C).
//!
//! ## Usage
//!
//! ```rust,no_run
//! use ra2a_ext::ExtensionActivator;
//! // Attach to a client to auto-request extensions on every call:
//! let activator = ExtensionActivator::new(vec![
//!     "urn:a2a:ext:duration".into(),
//! ]);
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

mod activator;
mod propagator;
mod util;

pub use activator::ExtensionActivator;
pub use propagator::{
    ClientPropagator, ClientPropagatorConfig, PropagatorContext, ServerPropagator,
    ServerPropagatorConfig, init_propagation,
};
