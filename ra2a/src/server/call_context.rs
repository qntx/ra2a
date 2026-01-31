//! Server call context for A2A request handling.
//!
//! This module provides the `ServerCallContext` type that carries
//! request-scoped information like user identity, extensions, and state.
//! Mirrors Python's `server/context.py`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Represents a user making a request to the A2A server.
pub trait User: Send + Sync {
    /// Returns whether the user is authenticated.
    fn is_authenticated(&self) -> bool;

    /// Returns the user's display name.
    fn user_name(&self) -> &str;

    /// Returns the user's unique identifier, if available.
    fn user_id(&self) -> Option<&str> {
        None
    }
}

/// An unauthenticated user.
#[derive(Debug, Clone, Default)]
pub struct UnauthenticatedUser;

impl User for UnauthenticatedUser {
    fn is_authenticated(&self) -> bool {
        false
    }

    fn user_name(&self) -> &str {
        "anonymous"
    }
}

/// A simple authenticated user.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    /// The user's unique identifier.
    pub id: String,
    /// The user's display name.
    pub name: String,
}

impl AuthenticatedUser {
    /// Creates a new authenticated user.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl User for AuthenticatedUser {
    fn is_authenticated(&self) -> bool {
        true
    }

    fn user_name(&self) -> &str {
        &self.name
    }

    fn user_id(&self) -> Option<&str> {
        Some(&self.id)
    }
}

/// Context for a server call, carrying request-scoped information.
///
/// This struct mirrors Python's `ServerCallContext` and provides:
/// - User identity information
/// - Request/response extensions
/// - Arbitrary state storage
#[derive(Clone)]
pub struct ServerCallContext {
    /// The user making the request.
    user: Arc<dyn User>,
    /// Extensions requested by the client.
    pub requested_extensions: HashSet<String>,
    /// Extensions activated for the response.
    pub activated_extensions: HashSet<String>,
    /// Arbitrary state data for middleware and handlers.
    pub state: HashMap<String, serde_json::Value>,
}

impl std::fmt::Debug for ServerCallContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerCallContext")
            .field("user_name", &self.user.user_name())
            .field("is_authenticated", &self.user.is_authenticated())
            .field("requested_extensions", &self.requested_extensions)
            .field("activated_extensions", &self.activated_extensions)
            .field("state", &self.state)
            .finish()
    }
}

impl Default for ServerCallContext {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerCallContext {
    /// Creates a new server call context with an unauthenticated user.
    pub fn new() -> Self {
        Self {
            user: Arc::new(UnauthenticatedUser),
            requested_extensions: HashSet::new(),
            activated_extensions: HashSet::new(),
            state: HashMap::new(),
        }
    }

    /// Creates a new context with the specified user.
    pub fn with_user(user: impl User + 'static) -> Self {
        Self {
            user: Arc::new(user),
            requested_extensions: HashSet::new(),
            activated_extensions: HashSet::new(),
            state: HashMap::new(),
        }
    }

    /// Creates a new context from an Arc user.
    pub fn with_arc_user(user: Arc<dyn User>) -> Self {
        Self {
            user,
            requested_extensions: HashSet::new(),
            activated_extensions: HashSet::new(),
            state: HashMap::new(),
        }
    }

    /// Returns the user making the request.
    pub fn user(&self) -> &dyn User {
        self.user.as_ref()
    }

    /// Returns whether the user is authenticated.
    pub fn is_authenticated(&self) -> bool {
        self.user.is_authenticated()
    }

    /// Returns the user's display name.
    pub fn user_name(&self) -> &str {
        self.user.user_name()
    }

    /// Sets the requested extensions from client headers.
    pub fn set_requested_extensions(&mut self, extensions: impl IntoIterator<Item = String>) {
        self.requested_extensions = extensions.into_iter().collect();
    }

    /// Adds a requested extension.
    pub fn add_requested_extension(&mut self, extension: impl Into<String>) {
        self.requested_extensions.insert(extension.into());
    }

    /// Checks if a specific extension was requested.
    pub fn is_extension_requested(&self, extension: &str) -> bool {
        self.requested_extensions.contains(extension)
    }

    /// Activates an extension for the response.
    pub fn activate_extension(&mut self, extension: impl Into<String>) {
        self.activated_extensions.insert(extension.into());
    }

    /// Checks if a specific extension is activated.
    pub fn is_extension_activated(&self, extension: &str) -> bool {
        self.activated_extensions.contains(extension)
    }

    /// Gets a value from the state.
    pub fn get_state(&self, key: &str) -> Option<&serde_json::Value> {
        self.state.get(key)
    }

    /// Gets a typed value from the state.
    pub fn get_state_as<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.state
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Sets a value in the state.
    pub fn set_state(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.state.insert(key.into(), value);
    }

    /// Sets a typed value in the state.
    pub fn set_state_value<T: serde::Serialize>(
        &mut self,
        key: impl Into<String>,
        value: &T,
    ) -> Result<(), serde_json::Error> {
        let json_value = serde_json::to_value(value)?;
        self.state.insert(key.into(), json_value);
        Ok(())
    }

    /// Removes a value from the state.
    pub fn remove_state(&mut self, key: &str) -> Option<serde_json::Value> {
        self.state.remove(key)
    }

    /// Returns the HTTP method if set in state.
    pub fn http_method(&self) -> Option<&str> {
        self.state.get("method").and_then(|v| v.as_str())
    }

    /// Returns the HTTP headers if set in state.
    pub fn headers(&self) -> Option<&serde_json::Value> {
        self.state.get("headers")
    }
}

/// Builder for creating `ServerCallContext` instances.
#[derive(Default)]
pub struct ServerCallContextBuilder {
    user: Option<Arc<dyn User>>,
    requested_extensions: HashSet<String>,
    state: HashMap<String, serde_json::Value>,
}

impl std::fmt::Debug for ServerCallContextBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerCallContextBuilder")
            .field("has_user", &self.user.is_some())
            .field("requested_extensions", &self.requested_extensions)
            .field("state", &self.state)
            .finish()
    }
}

impl ServerCallContextBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the user for the context.
    pub fn user(mut self, user: impl User + 'static) -> Self {
        self.user = Some(Arc::new(user));
        self
    }

    /// Sets the user from an Arc.
    pub fn arc_user(mut self, user: Arc<dyn User>) -> Self {
        self.user = Some(user);
        self
    }

    /// Adds a requested extension.
    pub fn requested_extension(mut self, extension: impl Into<String>) -> Self {
        self.requested_extensions.insert(extension.into());
        self
    }

    /// Sets all requested extensions.
    pub fn requested_extensions(mut self, extensions: impl IntoIterator<Item = String>) -> Self {
        self.requested_extensions = extensions.into_iter().collect();
        self
    }

    /// Adds a state value.
    pub fn state(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.state.insert(key.into(), value);
        self
    }

    /// Builds the context.
    pub fn build(self) -> ServerCallContext {
        ServerCallContext {
            user: self.user.unwrap_or_else(|| Arc::new(UnauthenticatedUser)),
            requested_extensions: self.requested_extensions,
            activated_extensions: HashSet::new(),
            state: self.state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unauthenticated_user() {
        let user = UnauthenticatedUser;
        assert!(!user.is_authenticated());
        assert_eq!(user.user_name(), "anonymous");
        assert!(user.user_id().is_none());
    }

    #[test]
    fn test_authenticated_user() {
        let user = AuthenticatedUser::new("user-123", "John Doe");
        assert!(user.is_authenticated());
        assert_eq!(user.user_name(), "John Doe");
        assert_eq!(user.user_id(), Some("user-123"));
    }

    #[test]
    fn test_server_call_context_default() {
        let ctx = ServerCallContext::new();
        assert!(!ctx.is_authenticated());
        assert_eq!(ctx.user_name(), "anonymous");
        assert!(ctx.requested_extensions.is_empty());
        assert!(ctx.activated_extensions.is_empty());
    }

    #[test]
    fn test_server_call_context_with_user() {
        let ctx = ServerCallContext::with_user(AuthenticatedUser::new("u1", "Alice"));
        assert!(ctx.is_authenticated());
        assert_eq!(ctx.user_name(), "Alice");
    }

    #[test]
    fn test_extensions() {
        let mut ctx = ServerCallContext::new();
        ctx.add_requested_extension("ext1");
        ctx.add_requested_extension("ext2");

        assert!(ctx.is_extension_requested("ext1"));
        assert!(!ctx.is_extension_requested("ext3"));

        ctx.activate_extension("ext1");
        assert!(ctx.is_extension_activated("ext1"));
        assert!(!ctx.is_extension_activated("ext2"));
    }

    #[test]
    fn test_state() {
        let mut ctx = ServerCallContext::new();
        ctx.set_state("key1", serde_json::json!("value1"));

        assert_eq!(ctx.get_state("key1"), Some(&serde_json::json!("value1")));
        assert!(ctx.get_state("key2").is_none());

        let removed = ctx.remove_state("key1");
        assert_eq!(removed, Some(serde_json::json!("value1")));
        assert!(ctx.get_state("key1").is_none());
    }

    #[test]
    fn test_builder() {
        let ctx = ServerCallContextBuilder::new()
            .user(AuthenticatedUser::new("u1", "Bob"))
            .requested_extension("ext1")
            .state("method", serde_json::json!("POST"))
            .build();

        assert!(ctx.is_authenticated());
        assert_eq!(ctx.user_name(), "Bob");
        assert!(ctx.is_extension_requested("ext1"));
        assert_eq!(ctx.http_method(), Some("POST"));
    }
}
