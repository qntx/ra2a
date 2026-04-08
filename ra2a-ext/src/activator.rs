//! Client-side extension activator interceptor.
//!
//! Aligned with Go's `a2aext.NewActivator`. Requests extension activation
//! on outgoing calls by appending supported extension URIs to the
//! `x-a2a-extensions` header.

use std::future::Future;
use std::pin::Pin;

use ra2a::SVC_PARAM_EXTENSIONS;
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
    #[must_use]
    pub const fn new(extension_uris: Vec<String>) -> Self {
        Self { extension_uris }
    }
}

impl CallInterceptor for ExtensionActivator {
    fn before<'a>(
        &'a self,
        req: &'a mut Request,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        self.activate(req);
        Box::pin(std::future::ready(Ok(())))
    }
}

impl ExtensionActivator {
    fn activate(&self, req: &mut Request) {
        // If the card has no extensions declared, skip entirely.
        if let Some(card) = &req.card
            && card.capabilities.extensions.is_empty()
        {
            return;
        }

        for uri in &self.extension_uris {
            if is_extension_supported(req.card.as_ref(), uri) {
                req.service_params.append(SVC_PARAM_EXTENSIONS, uri.clone());
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests use unwrap for brevity")]
mod tests {
    use ra2a::client::ServiceParams;
    use ra2a::types::{
        AgentCapabilities, AgentCard, AgentExtension, AgentInterface, TransportProtocol,
    };

    use super::*;

    fn make_card(uris: &[&str]) -> AgentCard {
        let mut card = AgentCard::new(
            "test",
            "test agent",
            vec![AgentInterface::new(
                "https://example.com",
                TransportProtocol::new("JSONRPC"),
            )],
        );
        card.capabilities = AgentCapabilities {
            extensions: uris
                .iter()
                .map(|u| AgentExtension {
                    uri: (*u).into(),
                    description: None,
                    required: false,
                    params: None,
                })
                .collect(),
            ..AgentCapabilities::default()
        };
        card
    }

    fn make_request(card: Option<AgentCard>) -> Request {
        Request {
            method: "message/send".into(),
            card,
            service_params: ServiceParams::default(),
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

        let vals = req.service_params.get_all(SVC_PARAM_EXTENSIONS);
        assert_eq!(vals, &["urn:a2a:ext:duration"]);
    }

    #[tokio::test]
    async fn test_activator_no_card_sends_all() {
        let activator =
            ExtensionActivator::new(vec!["urn:a2a:ext:a".into(), "urn:a2a:ext:b".into()]);

        let mut req = make_request(None);
        activator.before(&mut req).await.unwrap();

        let vals = req.service_params.get_all(SVC_PARAM_EXTENSIONS);
        assert_eq!(vals, &["urn:a2a:ext:a", "urn:a2a:ext:b"]);
    }

    #[tokio::test]
    async fn test_activator_empty_card_extensions_skips() {
        let activator = ExtensionActivator::new(vec!["urn:a2a:ext:duration".into()]);

        let card = make_card(&[]);
        let mut req = make_request(Some(card));

        activator.before(&mut req).await.unwrap();

        let vals = req.service_params.get_all(SVC_PARAM_EXTENSIONS);
        assert!(vals.is_empty());
    }
}
