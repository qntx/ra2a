//! Content part types for messages and artifacts.
//!
//! Aligned with Go's `Part` interface — a tagged union of `TextPart`, `FilePart`, `DataPart`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A discriminated union representing a content part.
///
/// Serialized with `"kind"` as the JSON tag discriminator, matching the Go spec.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Part {
    /// A text content part.
    Text(TextPart),
    /// A file content part.
    File(FilePart),
    /// A structured data part.
    Data(DataPart),
}

impl Part {
    /// Creates a text part.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(TextPart {
            text: text.into(),
            metadata: Default::default(),
        })
    }

    /// Creates a file part with inline bytes.
    pub fn file_bytes(bytes: impl Into<String>, mime_type: impl Into<Option<String>>) -> Self {
        Self::File(FilePart {
            file: FileContent::Bytes(FileBytes {
                bytes: bytes.into(),
                mime_type: mime_type.into(),
                name: None,
            }),
            metadata: Default::default(),
        })
    }

    /// Creates a file part with a URI reference.
    pub fn file_uri(uri: impl Into<String>, mime_type: impl Into<Option<String>>) -> Self {
        Self::File(FilePart {
            file: FileContent::Uri(FileUri {
                uri: uri.into(),
                mime_type: mime_type.into(),
                name: None,
            }),
            metadata: Default::default(),
        })
    }

    /// Creates a structured data part.
    pub fn data(data: HashMap<String, serde_json::Value>) -> Self {
        Self::Data(DataPart {
            data,
            metadata: Default::default(),
        })
    }

    /// Returns the text content if this is a `Text` variant.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(p) => Some(&p.text),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Concrete part types
// ---------------------------------------------------------------------------

/// A text segment within a message or artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextPart {
    /// The string content.
    pub text: String,
    /// Optional extension metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// A file segment within a message or artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilePart {
    /// The file content — either inline bytes or a URI reference.
    pub file: FileContent,
    /// Optional extension metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// A structured data segment (e.g. JSON object) within a message or artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataPart {
    /// The structured data content.
    pub data: HashMap<String, serde_json::Value>,
    /// Optional extension metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// File content types (aligned with Go's FilePartContent union)
// ---------------------------------------------------------------------------

/// File content — either inline base64 bytes or a URI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum FileContent {
    /// Inline base64-encoded content.
    Bytes(FileBytes),
    /// Content at a URI.
    Uri(FileUri),
}

impl FileContent {
    /// Returns the MIME type if present.
    pub fn mime_type(&self) -> Option<&str> {
        match self {
            Self::Bytes(f) => f.mime_type.as_deref(),
            Self::Uri(f) => f.mime_type.as_deref(),
        }
    }

    /// Returns the file name if present.
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Bytes(f) => f.name.as_deref(),
            Self::Uri(f) => f.name.as_deref(),
        }
    }
}

/// File with inline base64-encoded content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileBytes {
    /// Base64-encoded file content.
    pub bytes: String,
    /// Optional MIME type (e.g. `"application/pdf"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Optional file name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// File with content at a URI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileUri {
    /// URI pointing to the file content.
    pub uri: String,
    /// Optional MIME type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Optional file name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_part_roundtrip() {
        let part = Part::text("Hello, world!");
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"kind\":\"text\""));
        let parsed: Part = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.as_text(), Some("Hello, world!"));
    }

    #[test]
    fn test_file_bytes_part() {
        let part = Part::file_bytes("SGVsbG8=", "text/plain".to_string());
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"kind\":\"file\""));
        assert!(json.contains("\"bytes\":\"SGVsbG8=\""));
    }

    #[test]
    fn test_file_uri_part() {
        let part = Part::file_uri("https://example.com/f.pdf", None);
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"kind\":\"file\""));
        assert!(json.contains("\"uri\":\"https://example.com/f.pdf\""));
    }

    #[test]
    fn test_data_part() {
        let mut data = HashMap::new();
        data.insert("key".into(), serde_json::json!("value"));
        let part = Part::data(data);
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"kind\":\"data\""));
    }
}
