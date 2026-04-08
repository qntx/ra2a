//! Internal helper functions for extension support checks.

use ra2a::types::AgentCard;

/// Checks whether the given extension URI is supported by the agent card.
///
/// If no card is provided (e.g. client created from an `AgentInterface`),
/// assumes the extension is supported — matching Go's pragmatic default.
pub(crate) fn is_extension_supported(card: Option<&AgentCard>, ext_uri: &str) -> bool {
    let Some(c) = card else {
        // No card available — assume server supports all extensions.
        return true;
    };
    c.capabilities.extensions.iter().any(|e| e.uri == ext_uri)
}

#[cfg(test)]
mod tests {
    use ra2a::types::{AgentCapabilities, AgentCard, AgentExtension};

    use super::*;

    #[test]
    fn test_no_card_assumes_supported() {
        assert!(is_extension_supported(None, "urn:any:ext"));
    }

    #[test]
    fn test_card_with_matching_extension() {
        let card = card_with_extensions(vec!["urn:a2a:ext:duration"]);
        assert!(is_extension_supported(Some(&card), "urn:a2a:ext:duration"));
    }

    #[test]
    fn test_card_without_matching_extension() {
        let card = card_with_extensions(vec!["urn:a2a:ext:other"]);
        assert!(!is_extension_supported(Some(&card), "urn:a2a:ext:duration"));
    }

    #[test]
    fn test_card_with_no_extensions() {
        let card = card_with_extensions(vec![]);
        assert!(!is_extension_supported(Some(&card), "urn:a2a:ext:any"));
    }

    fn card_with_extensions(uris: Vec<&str>) -> AgentCard {
        use ra2a::types::AgentInterface;
        let mut card = AgentCard::new(
            "test",
            "test agent",
            vec![AgentInterface::new(
                "https://example.com",
                ra2a::types::TransportProtocol::new("JSONRPC"),
            )],
        );
        card.capabilities = AgentCapabilities {
            extensions: uris
                .into_iter()
                .map(|u| AgentExtension {
                    uri: u.into(),
                    description: None,
                    required: false,
                    params: None,
                })
                .collect(),
            ..AgentCapabilities::default()
        };
        card
    }
}
