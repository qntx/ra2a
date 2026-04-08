//! Extension propagation interceptors for agent-to-agent chaining.
//!
//! Aligned with Go's `a2aext.NewServerPropagator` and `a2aext.NewClientPropagator`.
//!
//! When an agent (B) acts as both server and client in a chain (A → B → C),
//! the [`ServerPropagator`] extracts extension-related metadata and headers
//! from the incoming request (A → B), stores them in a [`PropagatorContext`],
//! and the [`ClientPropagator`] injects them into the outgoing request (B → C).
//!
//! ## Data flow
//!
//! ```text
//! A → [HTTP] → B (ServerPropagator.before extracts → PropagatorContext)
//!                   → handler wraps executor in PropagatorContext::scope()
//!                     → B calls C (ClientPropagator.before injects from task_local)
//! ```
//!
//! The user must wrap downstream client calls within [`PropagatorContext::scope()`]
//! so that the [`ClientPropagator`] can access the extracted data via `task_local`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use ra2a::SVC_PARAM_EXTENSIONS;
use ra2a::error::A2AError;
use ra2a::types::AgentCard;

use crate::util::is_extension_supported;

tokio::task_local! {
    /// Mutable cell for propagator data. Must be initialized via
    /// [`init_propagation`] before [`ServerPropagator`] can store data.
    static PROPAGATOR_CTX: RefCell<Option<PropagatorContext>>;
}

/// Extension data extracted by [`ServerPropagator`] for downstream propagation.
///
/// Aligned with Go's internal `propagatorContext` struct.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct PropagatorContext {
    /// HTTP headers to propagate (key → values).
    pub request_headers: HashMap<String, Vec<String>>,
    /// Payload metadata to propagate (key → value).
    pub metadata: HashMap<String, serde_json::Value>,
}

impl PropagatorContext {
    /// Reads the current task-local propagator context, if set.
    #[must_use]
    pub fn current() -> Option<Self> {
        PROPAGATOR_CTX
            .try_with(|cell| cell.borrow().clone())
            .ok()
            .flatten()
    }

    /// Stores this context in the task-local cell.
    ///
    /// Requires that the current task is running within [`init_propagation`].
    /// Returns `true` if stored successfully.
    #[must_use]
    pub fn install(self) -> bool {
        PROPAGATOR_CTX
            .try_with(|cell| {
                *cell.borrow_mut() = Some(self);
            })
            .is_ok()
    }

    /// Executes a future with this context directly available via task-local.
    ///
    /// This is a convenience wrapper for simple cases where you already have
    /// the context and want to make it available to [`ClientPropagator`].
    pub async fn scope<F: Future>(self, f: F) -> F::Output {
        PROPAGATOR_CTX.scope(RefCell::new(Some(self)), f).await
    }
}

/// Wraps a future with an empty propagation scope.
///
/// Call this around your request handler so that [`ServerPropagator`] can store
/// extracted data and [`ClientPropagator`] can read it later.
///
/// # Example
///
/// ```rust,ignore
/// let result = ra2a_ext::init_propagation(async {
///     // ServerPropagator.before() stores data here
///     // handler runs
///     // ClientPropagator.before() reads data here
///     handle_request(req).await
/// }).await;
/// ```
pub async fn init_propagation<F: Future>(f: F) -> F::Output {
    PROPAGATOR_CTX.scope(RefCell::new(None), f).await
}

/// Predicate function for filtering metadata keys on the server side.
///
/// Receives the list of requested extension URIs and the metadata key.
/// Returns `true` if the key should be propagated.
pub(crate) type ServerMetadataPredicate = Arc<dyn Fn(&[String], &str) -> bool + Send + Sync>;

/// Predicate function for filtering request headers on the server side.
///
/// Receives the header key. Returns `true` if the header should be propagated.
pub(crate) type ServerHeaderPredicate = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Configuration for [`ServerPropagator`].
///
/// Both predicates are optional — sensible defaults are used when `None`.
#[derive(Default)]
#[non_exhaustive]
pub struct ServerPropagatorConfig {
    /// Determines which payload metadata keys are propagated.
    ///
    /// Default: propagate keys whose name matches a client-requested extension URI.
    pub metadata_predicate: Option<ServerMetadataPredicate>,
    /// Determines which request headers are propagated.
    ///
    /// Default: propagate only the `x-a2a-extensions` header.
    pub header_predicate: Option<ServerHeaderPredicate>,
}

impl std::fmt::Debug for ServerPropagatorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerPropagatorConfig")
            .field("metadata_predicate", &self.metadata_predicate.is_some())
            .field("header_predicate", &self.header_predicate.is_some())
            .finish()
    }
}

/// Server-side [`CallInterceptor`](ra2a::server::CallInterceptor) that extracts
/// extension-related metadata and headers from incoming requests.
///
/// The extracted data is stored in a [`PropagatorContext`] via `task_local`.
/// The handler must be wrapped in [`init_propagation`] for this to work.
/// [`ClientPropagator`] reads the stored context when making downstream calls.
///
/// Aligned with Go's `a2aext.NewServerPropagator`.
pub struct ServerPropagator {
    /// Metadata filter predicate.
    metadata_predicate: ServerMetadataPredicate,
    /// Header filter predicate.
    header_predicate: ServerHeaderPredicate,
}

impl ServerPropagator {
    /// Creates a new server propagator with default configuration.
    ///
    /// Default behavior:
    /// - Propagates metadata keys matching client-requested extension URIs
    /// - Propagates the `x-a2a-extensions` header
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(ServerPropagatorConfig::default())
    }

    /// Creates a new server propagator with custom configuration.
    #[must_use]
    pub fn with_config(config: ServerPropagatorConfig) -> Self {
        let metadata_predicate = config.metadata_predicate.unwrap_or_else(|| {
            Arc::new(|requested_uris: &[String], key: &str| requested_uris.iter().any(|u| u == key))
        });

        let header_predicate = config.header_predicate.unwrap_or_else(|| {
            Arc::new(|key: &str| key.eq_ignore_ascii_case(SVC_PARAM_EXTENSIONS))
        });

        Self {
            metadata_predicate,
            header_predicate,
        }
    }
}

impl Default for ServerPropagator {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ServerPropagator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerPropagator").finish_non_exhaustive()
    }
}

impl ServerPropagator {
    fn propagate_server(&self, ctx: &mut ra2a::server::CallContext, req: &ra2a::server::Request) {
        let mut prop_ctx = PropagatorContext::default();

        let requested = ctx.requested_extension_uris();

        extract_metadata(
            req,
            &requested,
            &self.metadata_predicate,
            &mut prop_ctx.metadata,
        );

        let request_meta = ctx.request_meta();
        for (header_name, header_values) in request_meta.iter() {
            if (self.header_predicate)(header_name) {
                prop_ctx
                    .request_headers
                    .insert(header_name.to_owned(), header_values.to_vec());
            }
        }

        if let Some(ext_values) = prop_ctx.request_headers.get(SVC_PARAM_EXTENSIONS) {
            for uri in ext_values {
                ctx.activate_extension(uri);
            }
        }

        // Best-effort install; fails silently if not inside init_propagation.
        let _installed = prop_ctx.install();
    }
}

impl ra2a::server::CallInterceptor for ServerPropagator {
    fn before<'a>(
        &'a self,
        ctx: &'a mut ra2a::server::CallContext,
        req: &'a mut ra2a::server::Request,
    ) -> Pin<Box<dyn Future<Output = Result<(), A2AError>> + Send + 'a>> {
        self.propagate_server(ctx, req);
        Box::pin(std::future::ready(Ok(())))
    }

    fn after<'a>(
        &'a self,
        _ctx: &'a ra2a::server::CallContext,
        _resp: &'a mut ra2a::server::Response,
    ) -> Pin<Box<dyn Future<Output = Result<(), A2AError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

/// Extracts matching metadata from known request payload types.
fn extract_metadata(
    req: &ra2a::server::Request,
    requested: &[String],
    predicate: &ServerMetadataPredicate,
    out: &mut HashMap<String, serde_json::Value>,
) {
    if let Some(params) = req.downcast_ref::<ra2a::SendMessageRequest>()
        && let Some(ref meta) = params.metadata
    {
        collect_matching_metadata(meta, requested, predicate, out);
    }
}

/// Collects metadata entries that pass the predicate.
fn collect_matching_metadata(
    metadata: &ra2a::Metadata,
    requested: &[String],
    predicate: &ServerMetadataPredicate,
    out: &mut HashMap<String, serde_json::Value>,
) {
    for (k, v) in metadata {
        if predicate(requested, k) {
            out.insert(k.clone(), v.clone());
        }
    }
}

/// Predicate function for filtering metadata keys on the client side.
///
/// Receives the target server's agent card (if available), the list of
/// requested extension URIs, and the metadata key.
pub(crate) type ClientMetadataPredicate =
    Arc<dyn Fn(Option<&AgentCard>, &[String], &str) -> bool + Send + Sync>;

/// Predicate function for filtering request headers on the client side.
///
/// Receives the target server's agent card (if available), the header key
/// and value. Returns `true` if the header should be forwarded.
pub(crate) type ClientHeaderPredicate =
    Arc<dyn Fn(Option<&AgentCard>, &str, &str) -> bool + Send + Sync>;

/// Configuration for [`ClientPropagator`].
#[derive(Default)]
#[non_exhaustive]
pub struct ClientPropagatorConfig {
    /// Determines which payload metadata keys are propagated.
    ///
    /// Default: propagate keys that are requested extensions and supported by
    /// the downstream server.
    pub metadata_predicate: Option<ClientMetadataPredicate>,
    /// Determines which request headers are propagated.
    ///
    /// Default: propagate `x-a2a-extensions` header values for extensions
    /// supported by the downstream server.
    pub header_predicate: Option<ClientHeaderPredicate>,
}

impl std::fmt::Debug for ClientPropagatorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientPropagatorConfig")
            .field("metadata_predicate", &self.metadata_predicate.is_some())
            .field("header_predicate", &self.header_predicate.is_some())
            .finish()
    }
}

/// Client-side [`CallInterceptor`](ra2a::client::CallInterceptor) that injects
/// propagated extension data into outgoing requests.
///
/// Reads [`PropagatorContext`] from the task-local (set by [`ServerPropagator`])
/// and injects matching metadata and headers into the outgoing request.
///
/// Aligned with Go's `a2aext.NewClientPropagator`.
pub struct ClientPropagator {
    /// Metadata filter predicate.
    metadata_predicate: ClientMetadataPredicate,
    /// Header filter predicate.
    header_predicate: ClientHeaderPredicate,
}

impl ClientPropagator {
    /// Creates a new client propagator with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(ClientPropagatorConfig::default())
    }

    /// Creates a new client propagator with custom configuration.
    #[must_use]
    pub fn with_config(config: ClientPropagatorConfig) -> Self {
        let metadata_predicate = config
            .metadata_predicate
            .unwrap_or_else(|| Arc::new(default_client_metadata_predicate));

        let header_predicate = config
            .header_predicate
            .unwrap_or_else(|| Arc::new(default_client_header_predicate));

        Self {
            metadata_predicate,
            header_predicate,
        }
    }
}

impl Default for ClientPropagator {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ClientPropagator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientPropagator").finish_non_exhaustive()
    }
}

impl ClientPropagator {
    fn propagate_client(&self, req: &mut ra2a::client::Request) {
        let Some(prop_ctx) = PropagatorContext::current() else {
            return;
        };

        let requested: Vec<String> = prop_ctx
            .request_headers
            .get(SVC_PARAM_EXTENSIONS)
            .cloned()
            .unwrap_or_default();

        if !prop_ctx.metadata.is_empty() {
            inject_metadata(
                &mut *req.payload,
                &prop_ctx.metadata,
                req.card.as_ref(),
                &requested,
                &self.metadata_predicate,
            );
        }

        for (name, val) in prop_ctx
            .request_headers
            .iter()
            .flat_map(|(k, vs)| vs.iter().map(move |v| (k, v)))
        {
            if (self.header_predicate)(req.card.as_ref(), name, val) {
                req.service_params.append(name, val);
            }
        }
    }
}

impl ra2a::client::CallInterceptor for ClientPropagator {
    fn before<'a>(
        &'a self,
        req: &'a mut ra2a::client::Request,
    ) -> Pin<Box<dyn Future<Output = ra2a::error::Result<()>> + Send + 'a>> {
        self.propagate_client(req);
        Box::pin(std::future::ready(Ok(())))
    }
}

/// Default metadata predicate for [`ClientPropagator`]: propagates keys that
/// are in the requested list and supported by the downstream agent card.
fn default_client_metadata_predicate(
    card: Option<&AgentCard>,
    requested: &[String],
    key: &str,
) -> bool {
    requested.iter().any(|u| u == key) && is_extension_supported(card, key)
}

/// Default header predicate for [`ClientPropagator`]: propagates
/// `x-a2a-extensions` header values for extensions supported by the downstream agent.
fn default_client_header_predicate(card: Option<&AgentCard>, key: &str, val: &str) -> bool {
    key.eq_ignore_ascii_case(SVC_PARAM_EXTENSIONS) && is_extension_supported(card, val)
}

/// Injects matching metadata into known outgoing payload types.
fn inject_metadata(
    payload: &mut dyn std::any::Any,
    metadata: &HashMap<String, serde_json::Value>,
    card: Option<&AgentCard>,
    requested: &[String],
    predicate: &ClientMetadataPredicate,
) {
    if let Some(params) = payload.downcast_mut::<ra2a::SendMessageRequest>() {
        let meta = params.metadata.get_or_insert_with(Default::default);
        inject_matching_metadata(meta, metadata, card, requested, predicate);
    }
}

/// Inserts metadata entries that pass the predicate into the target map.
fn inject_matching_metadata(
    target: &mut ra2a::Metadata,
    source: &HashMap<String, serde_json::Value>,
    card: Option<&AgentCard>,
    requested: &[String],
    predicate: &ClientMetadataPredicate,
) {
    for (k, v) in source {
        if predicate(card, requested, k) {
            target.insert(k.clone(), v.clone());
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests use unwrap for brevity")]
mod tests {
    use ra2a::client::{CallInterceptor as _, ServiceParams};
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

    #[tokio::test]
    async fn test_client_propagator_injects_headers() {
        let propagator = ClientPropagator::new();
        let card = make_card(&["urn:a2a:ext:duration"]);

        let mut prop_ctx = PropagatorContext::default();
        prop_ctx.request_headers.insert(
            SVC_PARAM_EXTENSIONS.to_owned(),
            vec!["urn:a2a:ext:duration".into()],
        );

        let mut req = ra2a::client::Request {
            method: "message/send".into(),
            service_params: ServiceParams::default(),
            card: Some(card),
            payload: Box::new(()),
        };

        prop_ctx
            .scope(async {
                propagator.before(&mut req).await.unwrap();
            })
            .await;

        let vals = req.service_params.get_all(SVC_PARAM_EXTENSIONS);
        assert_eq!(vals, &["urn:a2a:ext:duration"]);
    }

    #[tokio::test]
    async fn test_client_propagator_filters_unsupported() {
        let propagator = ClientPropagator::new();
        let card = make_card(&["urn:a2a:ext:other"]);

        let mut prop_ctx = PropagatorContext::default();
        prop_ctx.request_headers.insert(
            SVC_PARAM_EXTENSIONS.to_owned(),
            vec!["urn:a2a:ext:duration".into()],
        );

        let mut req = ra2a::client::Request {
            method: "message/send".into(),
            service_params: ServiceParams::default(),
            card: Some(card),
            payload: Box::new(()),
        };

        prop_ctx
            .scope(async {
                propagator.before(&mut req).await.unwrap();
            })
            .await;

        let vals = req.service_params.get_all(SVC_PARAM_EXTENSIONS);
        assert!(vals.is_empty());
    }

    #[tokio::test]
    async fn test_client_propagator_no_context_is_noop() {
        let propagator = ClientPropagator::new();

        let mut req = ra2a::client::Request {
            method: "message/send".into(),
            service_params: ServiceParams::default(),
            card: None,
            payload: Box::new(()),
        };

        propagator.before(&mut req).await.unwrap();
        assert!(req.service_params.is_empty());
    }

    #[tokio::test]
    async fn test_propagator_context_install_and_read() {
        let ctx = PropagatorContext {
            request_headers: {
                let mut m = HashMap::new();
                m.insert("x-test".into(), vec!["val1".into()]);
                m
            },
            metadata: HashMap::new(),
        };

        init_propagation(async {
            assert!(PropagatorContext::current().is_none());
            assert!(ctx.install());
            let read = PropagatorContext::current().unwrap();
            assert_eq!(
                read.request_headers.get("x-test").unwrap(),
                &["val1".to_owned()]
            );
        })
        .await;
    }
}
