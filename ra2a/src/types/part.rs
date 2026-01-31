//! Message and artifact part types.
//!
//! Parts are the building blocks of messages and artifacts in the A2A protocol.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A discriminated union representing a part of a message or artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    /// Creates a new text part.
    pub fn text(text: impl Into<String>) -> Self {
        Part::Text(TextPart::new(text))
    }

    /// Creates a new file part with bytes.
    pub fn file_bytes(bytes: impl Into<String>, mime_type: Option<String>) -> Self {
        Part::File(FilePart::with_bytes(bytes, mime_type))
    }

    /// Creates a new file part with URI.
    pub fn file_uri(uri: impl Into<String>, mime_type: Option<String>) -> Self {
        Part::File(FilePart::with_uri(uri, mime_type))
    }

    /// Creates a new data part.
    pub fn data(data: HashMap<String, serde_json::Value>) -> Self {
        Part::Data(DataPart::new(data))
    }

    /// Returns true if this is a text part.
    pub fn is_text(&self) -> bool {
        matches!(self, Part::Text(_))
    }

    /// Returns true if this is a file part.
    pub fn is_file(&self) -> bool {
        matches!(self, Part::File(_))
    }

    /// Returns true if this is a data part.
    pub fn is_data(&self) -> bool {
        matches!(self, Part::Data(_))
    }

    /// Returns the text content if this is a text part.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Part::Text(p) => Some(&p.text),
            _ => None,
        }
    }
}

/// Represents a text segment within a message or artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextPart {
    /// The string content of the text part.
    pub text: String,
    /// Optional metadata associated with this part.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl TextPart {
    /// Creates a new text part.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            metadata: None,
        }
    }

    /// Sets the metadata for this part.
    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Represents a file segment within a message or artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilePart {
    /// The file content.
    pub file: FileContent,
    /// Optional metadata associated with this part.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl FilePart {
    /// Creates a new file part with bytes content.
    pub fn with_bytes(bytes: impl Into<String>, mime_type: Option<String>) -> Self {
        Self {
            file: FileContent::Bytes(FileWithBytes {
                bytes: bytes.into(),
                mime_type,
                name: None,
            }),
            metadata: None,
        }
    }

    /// Creates a new file part with URI content.
    pub fn with_uri(uri: impl Into<String>, mime_type: Option<String>) -> Self {
        Self {
            file: FileContent::Uri(FileWithUri {
                uri: uri.into(),
                mime_type,
                name: None,
            }),
            metadata: None,
        }
    }
}

/// File content can be provided as bytes or URI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum FileContent {
    /// File content provided as base64-encoded bytes.
    Bytes(FileWithBytes),
    /// File content located at a URI.
    Uri(FileWithUri),
}

/// Common base fields for file content.
pub trait FileBase {
    /// Returns the MIME type of the file.
    fn mime_type(&self) -> Option<&str>;
    /// Returns the name of the file.
    fn name(&self) -> Option<&str>;
}

/// Represents a file with its content provided as base64-encoded bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileWithBytes {
    /// The base64-encoded content of the file.
    pub bytes: String,
    /// The MIME type of the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// An optional name for the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl FileBase for FileWithBytes {
    fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }

    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

impl FileWithBytes {
    /// Creates a new file with bytes content.
    pub fn new(bytes: impl Into<String>) -> Self {
        Self {
            bytes: bytes.into(),
            mime_type: None,
            name: None,
        }
    }

    /// Sets the MIME type.
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    /// Sets the file name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Represents a file with its content located at a URI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileWithUri {
    /// A URL pointing to the file's content.
    pub uri: String,
    /// The MIME type of the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// An optional name for the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl FileBase for FileWithUri {
    fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }

    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

impl FileWithUri {
    /// Creates a new file with URI content.
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            mime_type: None,
            name: None,
        }
    }

    /// Sets the MIME type.
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    /// Sets the file name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Represents a structured data segment within a message or artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataPart {
    /// The structured data content.
    pub data: HashMap<String, serde_json::Value>,
    /// Optional metadata associated with this part.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl DataPart {
    /// Creates a new data part.
    pub fn new(data: HashMap<String, serde_json::Value>) -> Self {
        Self {
            data,
            metadata: None,
        }
    }

    /// Sets the metadata for this part.
    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_part_serialization() {
        let part = Part::text("Hello, world!");
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"kind\":\"text\""));
        assert!(json.contains("Hello, world!"));
    }

    #[test]
    fn test_file_part_with_bytes() {
        let part = Part::file_bytes("SGVsbG8=", Some("text/plain".into()));
        assert!(part.is_file());
    }

    #[test]
    fn test_part_conversion() {
        let part = Part::text("test");
        assert_eq!(part.as_text(), Some("test"));
    }
}
