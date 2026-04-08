//! Content part types for messages and artifacts.
//!
//! Maps to the proto `Part` message with `oneof content { text, raw, url, data }`.
//!
//! v1.0 uses the **JSON member name** as the type discriminator (no `kind` field):
//! ```json
//! {"text": "hello", "mediaType": "text/plain"}
//! {"raw": "<base64>", "filename": "img.png", "mediaType": "image/png"}
//! {"url": "https://...", "mediaType": "image/png"}
//! {"data": {"key": "value"}, "mediaType": "application/json"}
//! ```

use std::collections::HashMap;
use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::Metadata;

/// A content part within a [`Message`](super::Message) or [`Artifact`](super::Artifact).
///
/// Corresponds to the proto `Part` message. The `content` field is a `oneof`
/// discriminated by the JSON member name (`text` / `raw` / `url` / `data`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// The content payload.
    pub content: PartContent,
    /// Optional metadata associated with this part.
    pub metadata: Option<Metadata>,
    /// An optional filename (e.g. `"document.pdf"`).
    pub filename: Option<String>,
    /// The media type of the part content (e.g. `"text/plain"`, `"image/png"`).
    pub media_type: Option<String>,
}

/// The content payload of a [`Part`] — a discriminated union.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartContent {
    /// Plain text content.
    Text(String),
    /// Raw binary content (serialized as base64 in JSON).
    Raw(Vec<u8>),
    /// A URL pointing to the content.
    Url(String),
    /// Arbitrary structured data (JSON value).
    Data(serde_json::Value),
}

impl Part {
    /// Creates a text part.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: PartContent::Text(text.into()),
            metadata: None,
            filename: None,
            media_type: None,
        }
    }

    /// Creates a raw binary part from bytes.
    pub fn raw(data: Vec<u8>, media_type: impl Into<String>) -> Self {
        Self {
            content: PartContent::Raw(data),
            metadata: None,
            filename: None,
            media_type: Some(media_type.into()),
        }
    }

    /// Creates a URL part.
    pub fn url(url: impl Into<String>, media_type: impl Into<String>) -> Self {
        Self {
            content: PartContent::Url(url.into()),
            metadata: None,
            filename: None,
            media_type: Some(media_type.into()),
        }
    }

    /// Creates a structured data part.
    #[must_use]
    pub fn data(value: serde_json::Value) -> Self {
        Self {
            content: PartContent::Data(value),
            metadata: None,
            filename: None,
            media_type: Some("application/json".into()),
        }
    }

    /// Sets the filename.
    #[must_use]
    pub fn with_filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    /// Sets the media type.
    #[must_use]
    pub fn with_media_type(mut self, media_type: impl Into<String>) -> Self {
        self.media_type = Some(media_type.into());
        self
    }

    /// Sets the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Returns the text content if this is a text part.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match &self.content {
            PartContent::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the raw bytes if this is a raw part.
    #[must_use]
    pub fn as_raw(&self) -> Option<&[u8]> {
        match &self.content {
            PartContent::Raw(b) => Some(b),
            _ => None,
        }
    }

    /// Returns the URL string if this is a URL part.
    #[must_use]
    pub fn as_url(&self) -> Option<&str> {
        match &self.content {
            PartContent::Url(u) => Some(u),
            _ => None,
        }
    }

    /// Returns the structured data if this is a data part.
    #[must_use]
    pub const fn as_data(&self) -> Option<&serde_json::Value> {
        match &self.content {
            PartContent::Data(v) => Some(v),
            _ => None,
        }
    }
}

impl Serialize for Part {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut count = 1; // content field
        if self.metadata.is_some() {
            count += 1;
        }
        if self.filename.is_some() {
            count += 1;
        }
        if self.media_type.is_some() {
            count += 1;
        }

        let mut map = serializer.serialize_map(Some(count))?;

        match &self.content {
            PartContent::Text(s) => map.serialize_entry("text", s)?,
            PartContent::Raw(bytes) => {
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                map.serialize_entry("raw", &encoded)?;
            }
            PartContent::Url(u) => map.serialize_entry("url", u)?,
            PartContent::Data(v) => map.serialize_entry("data", v)?,
        }

        if let Some(ref filename) = self.filename {
            map.serialize_entry("filename", filename)?;
        }
        if let Some(ref media_type) = self.media_type {
            map.serialize_entry("mediaType", media_type)?;
        }
        if let Some(ref metadata) = self.metadata {
            map.serialize_entry("metadata", metadata)?;
        }

        map.end()
    }
}

impl<'de> Deserialize<'de> for Part {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(PartVisitor)
    }
}

struct PartVisitor;

impl<'de> Visitor<'de> for PartVisitor {
    type Value = Part;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a Part object with one of: text, raw, url, data")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Part, M::Error> {
        let mut content: Option<PartContent> = None;
        let mut metadata: Option<Metadata> = None;
        let mut filename: Option<String> = None;
        let mut media_type: Option<String> = None;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "text" => {
                    let v: String = map.next_value()?;
                    content = Some(PartContent::Text(v));
                }
                "raw" => {
                    let encoded: String = map.next_value()?;
                    use base64::Engine;
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(&encoded)
                        .map_err(de::Error::custom)?;
                    content = Some(PartContent::Raw(bytes));
                }
                "url" => {
                    let v: String = map.next_value()?;
                    content = Some(PartContent::Url(v));
                }
                "data" => {
                    let v: serde_json::Value = map.next_value()?;
                    content = Some(PartContent::Data(v));
                }
                "filename" => {
                    filename = Some(map.next_value()?);
                }
                "mediaType" => {
                    media_type = Some(map.next_value()?);
                }
                "metadata" => {
                    let m: HashMap<String, serde_json::Value> = map.next_value()?;
                    metadata = Some(m);
                }
                _ => {
                    let _: serde_json::Value = map.next_value()?;
                }
            }
        }

        let content = content
            .ok_or_else(|| de::Error::custom("Part must contain one of: text, raw, url, data"))?;

        Ok(Part {
            content,
            metadata,
            filename,
            media_type,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_text_part() {
        let part = Part::text("hello");
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(json, serde_json::json!({"text": "hello"}));
    }

    #[test]
    fn serialize_text_part_with_media_type() {
        let part = Part::text("hello").with_media_type("text/plain");
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"text": "hello", "mediaType": "text/plain"})
        );
    }

    #[test]
    fn serialize_raw_part() {
        let part =
            Part::raw(b"hello".to_vec(), "application/octet-stream").with_filename("data.bin");
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(json["raw"], "aGVsbG8=");
        assert_eq!(json["filename"], "data.bin");
        assert_eq!(json["mediaType"], "application/octet-stream");
    }

    #[test]
    fn serialize_url_part() {
        let part = Part::url("https://example.com/img.png", "image/png");
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"url": "https://example.com/img.png", "mediaType": "image/png"})
        );
    }

    #[test]
    fn serialize_data_part() {
        let part = Part::data(serde_json::json!({"key": "value"}));
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"data": {"key": "value"}, "mediaType": "application/json"})
        );
    }

    #[test]
    fn deserialize_text_part() {
        let json = r#"{"text": "hello", "mediaType": "text/plain"}"#;
        let part: Part = serde_json::from_str(json).unwrap();
        assert_eq!(part.as_text(), Some("hello"));
        assert_eq!(part.media_type.as_deref(), Some("text/plain"));
    }

    #[test]
    fn deserialize_raw_part() {
        let json = r#"{"raw": "aGVsbG8=", "filename": "data.bin"}"#;
        let part: Part = serde_json::from_str(json).unwrap();
        assert_eq!(part.as_raw(), Some(b"hello".as_slice()));
        assert_eq!(part.filename.as_deref(), Some("data.bin"));
    }

    #[test]
    fn deserialize_url_part() {
        let json = r#"{"url": "https://example.com/img.png"}"#;
        let part: Part = serde_json::from_str(json).unwrap();
        assert_eq!(part.as_url(), Some("https://example.com/img.png"));
    }

    #[test]
    fn round_trip() {
        let original = Part::text("round trip test")
            .with_media_type("text/plain")
            .with_filename("note.txt");
        let json = serde_json::to_string(&original).unwrap();
        let decoded: Part = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }
}
