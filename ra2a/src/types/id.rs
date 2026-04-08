//! Strongly-typed identifiers for the A2A protocol.
//!
//! Each identifier is a newtype wrapper around `String`, providing compile-time
//! safety against accidental ID type misuse. New IDs are generated using `UUIDv7`
//! for time-ordered uniqueness.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            /// Creates a new identifier with a random UUIDv7 value.
            #[must_use]
            pub fn random() -> Self {
                Self(Uuid::now_v7().to_string())
            }

            /// Creates an identifier from an existing string.
            pub fn from_string(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            /// Returns the identifier as a string slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Returns `true` if the identifier is empty.
            #[must_use]
            pub const fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_owned())
            }
        }

        impl From<&String> for $name {
            fn from(s: &String) -> Self {
                Self(s.clone())
            }
        }

        impl From<&$name> for $name {
            fn from(s: &$name) -> Self {
                s.clone()
            }
        }

        impl std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl std::ops::Deref for $name {
            type Target = str;
            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<String> for $name {
            fn eq(&self, other: &String) -> bool {
                self.0 == *other
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::random()
            }
        }
    };
}

define_id!(
    /// Unique identifier for a [`Task`](super::Task).
    TaskId
);

define_id!(
    /// Unique identifier for a context (conversation/session grouping).
    ContextId
);

define_id!(
    /// Unique identifier for a [`Message`](super::Message).
    MessageId
);

define_id!(
    /// Unique identifier for an [`Artifact`](super::Artifact), scoped to a task.
    ArtifactId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_ids_are_unique() {
        let a = TaskId::random();
        let b = TaskId::random();
        assert_ne!(a, b);
    }

    #[test]
    fn serde_round_trip() {
        let id = TaskId::from_string("test-123");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"test-123\"");
        let decoded: TaskId = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn type_safety() {
        let task_id = TaskId::from_string("abc");
        let msg_id = MessageId::from_string("abc");
        // These are different types — comparing them would be a compile error:
        // assert_ne!(task_id, msg_id);
        assert_eq!(task_id.as_str(), msg_id.as_str());
    }
}
