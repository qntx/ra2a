//! Extension types for the A2A protocol.
//!
//! Extensions allow agents and clients to negotiate and use additional
//! capabilities beyond the core A2A specification.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents an extension declaration in requests or responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtensionDeclaration {
    /// The unique URI identifying the extension.
    pub uri: String,
    /// Whether this extension is required for the operation.
    #[serde(default)]
    pub required: bool,
    /// Extension-specific parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, serde_json::Value>>,
}

impl ExtensionDeclaration {
    /// Creates a new extension declaration.
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            required: false,
            params: None,
        }
    }

    /// Marks this extension as required.
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Sets extension parameters.
    pub fn with_params(mut self, params: HashMap<String, serde_json::Value>) -> Self {
        self.params = Some(params);
        self
    }
}

/// Context for extension processing in requests and responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExtensionContext {
    /// List of extensions being used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<String>>,
    /// Extension-specific metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl ExtensionContext {
    /// Creates a new empty extension context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an extension URI to the context.
    pub fn add_extension(&mut self, uri: impl Into<String>) {
        self.extensions
            .get_or_insert_with(Vec::new)
            .push(uri.into());
    }

    /// Checks if a specific extension is present.
    pub fn has_extension(&self, uri: &str) -> bool {
        self.extensions
            .as_ref()
            .map(|exts| exts.iter().any(|e| e == uri))
            .unwrap_or(false)
    }

    /// Sets metadata for a specific key.
    pub fn set_metadata(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.metadata
            .get_or_insert_with(HashMap::new)
            .insert(key.into(), value);
    }

    /// Gets metadata for a specific key.
    pub fn get_metadata(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.as_ref().and_then(|m| m.get(key))
    }
}

/// Well-known extension URIs used in the A2A protocol.
pub mod well_known {
    /// Extension for handling long-running tasks with progress updates.
    pub const PROGRESS_TRACKING: &str = "urn:a2a:ext:progress-tracking";
    /// Extension for multi-turn conversation support.
    pub const MULTI_TURN: &str = "urn:a2a:ext:multi-turn";
    /// Extension for file upload/download capabilities.
    pub const FILE_TRANSFER: &str = "urn:a2a:ext:file-transfer";
    /// Extension for structured data output.
    pub const STRUCTURED_OUTPUT: &str = "urn:a2a:ext:structured-output";
    /// Extension for human-in-the-loop workflows.
    pub const HUMAN_IN_LOOP: &str = "urn:a2a:ext:human-in-loop";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_declaration() {
        let ext = ExtensionDeclaration::new("urn:test:extension").required();
        assert!(ext.required);
        assert_eq!(ext.uri, "urn:test:extension");
    }

    #[test]
    fn test_extension_context() {
        let mut ctx = ExtensionContext::new();
        ctx.add_extension("urn:test:ext1");
        ctx.add_extension("urn:test:ext2");

        assert!(ctx.has_extension("urn:test:ext1"));
        assert!(!ctx.has_extension("urn:test:ext3"));
    }

    #[test]
    fn test_extension_metadata() {
        let mut ctx = ExtensionContext::new();
        ctx.set_metadata("key", serde_json::json!("value"));

        assert_eq!(ctx.get_metadata("key"), Some(&serde_json::json!("value")));
    }
}
