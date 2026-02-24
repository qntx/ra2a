//! Content part types for messages and artifacts.
//!
//! Aligned with Go's `Part` interface — a tagged union of `TextPart`, `FilePart`, `DataPart`.

use serde::{Deserialize, Serialize};

use super::Metadata;

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
    pub fn file_bytes(bytes: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::File(FilePart {
            file: FileContent::Bytes(FileBytes {
                bytes: bytes.into(),
                mime_type: mime_type.into(),
                name: String::new(),
            }),
            metadata: Default::default(),
        })
    }

    /// Creates a file part with a URI reference.
    pub fn file_uri(uri: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::File(FilePart {
            file: FileContent::Uri(FileUri {
                uri: uri.into(),
                mime_type: mime_type.into(),
                name: String::new(),
            }),
            metadata: Default::default(),
        })
    }

    /// Creates a structured data part.
    pub fn data(data: Metadata) -> Self {
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
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

/// A file segment within a message or artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilePart {
    /// The file content — either inline bytes or a URI reference.
    pub file: FileContent,
    /// Optional extension metadata.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

/// A structured data segment (e.g. JSON object) within a message or artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataPart {
    /// The structured data content.
    pub data: Metadata,
    /// Optional extension metadata.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
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
    /// Returns the MIME type (empty = not set).
    pub fn mime_type(&self) -> &str {
        match self {
            Self::Bytes(f) => &f.mime_type,
            Self::Uri(f) => &f.mime_type,
        }
    }

    /// Returns the file name (empty = not set).
    pub fn name(&self) -> &str {
        match self {
            Self::Bytes(f) => &f.name,
            Self::Uri(f) => &f.name,
        }
    }
}

/// File with inline base64-encoded content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileBytes {
    /// Base64-encoded file content.
    pub bytes: String,
    /// MIME type (e.g. `"application/pdf"`, empty = not set).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mime_type: String,
    /// File name (empty = not set).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

/// File with content at a URI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileUri {
    /// URI pointing to the file content.
    pub uri: String,
    /// MIME type (empty = not set).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mime_type: String,
    /// File name (empty = not set).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}
