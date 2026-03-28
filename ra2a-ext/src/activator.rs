//! Client-side extension activator interceptor.
//!
//! Aligned with Go's `a2aext.NewActivator`. Requests extension activation
//! on outgoing calls by appending supported extension URIs to the
//! `x-a2a-extensions` header.

use std::future::Future;
use std::pin::Pin;

use ra2a::EXTENSIONS_META_KEY;
use ra2a::client::{CallInterceptor, Request};
use ra2a::error::Result;

use crate::util::is_extension_supported;

/// Client-side [`CallInterceptor`] that requests extension activation.
///
/// For each outgoing request, checks the server's [`AgentCard`](ra2a::AgentCard)
/// for supported extensions and appends matching URIs to the
/// `x-a2a-extensions` metadata header.
///
/// # Example
///
/// ```rust,no_run
/// use ra2a_ext::ExtensionActivator;
///
/// let activator = ExtensionActivator::new(vec![
///     "urn:a2a:ext:duration".into(),
///     "urn:a2a:ext:custom".into(),
/// ]);
/// // client.with_interceptor(activator);
/// ```
#[derive(Debug)]
pub struct ExtensionActivator {
    /// Extension URIs this client wishes to activate.
    extension_uris: Vec<String>,
}

impl ExtensionActivator {
    /// Creates a new activator for the given extension URIs.
    pub const fn new(extension_uris: Vec<String>) -> Self {
        Self { extension_uris }
    }
}

impl CallInterceptor for ExtensionActivator {
    fn before<'a>(
        &'a self,
        req: &'a mut Request,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            // If the card has no extensions declared, skip entirely.
            if let Some(card) = &req.card
                && card.capabilities.extensions.is_empty()
            {
                return Ok(());
            }

            for uri in &self.extension_uris {
                if is_extension_supported(req.card.as_ref(), uri) {
                    req.meta.append(EXTENSIONS_META_KEY, uri.clone());
                }
            }
            Ok(())
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;

    use ra2a::client::CallMeta;
    use ra2a::types::{AgentCapabilities, AgentCard, AgentExtension};

    use super::*;

    fn make_card(uris: &[&str]) -> AgentCard {
        AgentCard {
            name: "test".into(),
            url: "https://example.com".into(),
            version: "1.0".into(),
            capabilities: AgentCapabilities {
                extensions: uris
                    .iter()
                    .map(|u| AgentExtension {
                        uri: (*u).into(),
                        description: String::new(),
                        required: false,
                        params: HashMap::default(),
                    })
                    .collect(),
                ..AgentCapabilities::default()
            },
            skills: vec![],
            ..AgentCard::default()
        }
    }

    fn make_request(card: Option<AgentCard>) -> Request {
        Request {
            method: "message/send".into(),
            base_url: "https://example.com".into(),
            meta: CallMeta::default(),
            card,
            payload: Box::new(()),
        }
    }

    #[tokio::test]
    async fn test_activator_filters_by_card() {
        let activator = ExtensionActivator::new(vec![
            "urn:a2a:ext:duration".into(),
            "urn:a2a:ext:missing".into(),
        ]);

        let card = make_card(&["urn:a2a:ext:duration", "urn:a2a:ext:other"]);
        let mut req = make_request(Some(card));

        activator.before(&mut req).await.unwrap();

        let vals = req.meta.get_all(EXTENSIONS_META_KEY);
        assert_eq!(vals, &["urn:a2a:ext:duration"]);
    }

    #[tokio::test]
    async fn test_activator_no_card_sends_all() {
        let activator =
            ExtensionActivator::new(vec!["urn:a2a:ext:a".into(), "urn:a2a:ext:b".into()]);

        let mut req = make_request(None);
        activator.before(&mut req).await.unwrap();

        let vals = req.meta.get_all(EXTENSIONS_META_KEY);
        assert_eq!(vals, &["urn:a2a:ext:a", "urn:a2a:ext:b"]);
    }

    #[tokio::test]
    async fn test_activator_empty_card_extensions_skips() {
        let activator = ExtensionActivator::new(vec!["urn:a2a:ext:duration".into()]);

        let card = make_card(&[]);
        let mut req = make_request(Some(card));

        activator.before(&mut req).await.unwrap();

        let vals = req.meta.get_all(EXTENSIONS_META_KEY);
        assert!(vals.is_empty());
    }
}
