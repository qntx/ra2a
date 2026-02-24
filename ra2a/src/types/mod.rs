//! A2A Protocol types and data models.
//!
//! This module contains all the type definitions for the A2A protocol,
//! including messages, tasks, agent cards, and JSON-RPC structures.

mod agent;
mod extensions;
mod jsonrpc;
mod message;
mod oauth;
mod part;
mod security;
mod task;

pub use agent::*;
pub use extensions::*;
pub use jsonrpc::*;
pub use message::*;
pub use oauth::*;
pub use part::*;
pub use security::*;
pub use task::*;

/// Helper for serde: skip serializing boolean fields when false.
#[must_use]
pub(crate) fn is_false(v: &bool) -> bool {
    !v
}

/// Serde helpers that inject a constant `"kind"` discriminator on serialization
/// while tolerating its presence or absence on deserialization.
///
/// Usage: `#[serde(with = "kind_serde::task", default)]`
pub(crate) mod kind_serde {
    macro_rules! define_kind {
        ($mod_name:ident, $value:expr) => {
            pub mod $mod_name {
                use serde::{Deserialize, Deserializer, Serializer};

                pub fn serialize<S: Serializer>(_: &(), s: S) -> Result<S::Ok, S::Error> {
                    s.serialize_str($value)
                }

                pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<(), D::Error> {
                    let _ = Option::<String>::deserialize(d)?;
                    Ok(())
                }
            }
        };
    }

    define_kind!(task, "task");
    define_kind!(message, "message");
    define_kind!(status_update, "status-update");
    define_kind!(artifact_update, "artifact-update");
}
