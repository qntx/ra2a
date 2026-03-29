//! Client factory for automatic transport selection from `AgentCard`.
//!
//! [`ClientFactory`] inspects `AgentCard.supported_interfaces` and selects the
//! best compatible transport based on protocol binding and client preferences,
//! aligned with Go's `factory.go`.
//!
//! [`TenantTransportDecorator`] wraps any [`Transport`] to automatically inject
//! a tenant ID into every request.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use super::{
    CallInterceptor, Client, ClientConfig, EventStream, JsonRpcTransport, RestTransport,
    ServiceParams, Transport, TransportConfig,
};
use crate::error::{A2AError, Result};
use crate::types::{
    AgentCard, AgentInterface, CancelTaskRequest, DeleteTaskPushNotificationConfigRequest,
    GetExtendedAgentCardRequest, GetTaskPushNotificationConfigRequest, GetTaskRequest,
    ListTaskPushNotificationConfigsRequest, ListTaskPushNotificationConfigsResponse,
    ListTasksRequest, ListTasksResponse, SendMessageRequest, SendMessageResponse,
    SubscribeToTaskRequest, Task, TaskPushNotificationConfig, TransportProtocol,
};

/// Trait for building a [`Transport`] from an [`AgentInterface`].
pub trait TransportBuilder: Send + Sync {
    /// Attempts to create a transport connection to the given endpoint.
    fn build<'a>(
        &'a self,
        endpoint: &'a AgentInterface,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn Transport>>> + Send + 'a>>;
}

/// Builds a [`JsonRpcTransport`].
struct JsonRpcBuilder;

impl TransportBuilder for JsonRpcBuilder {
    fn build<'a>(
        &'a self,
        endpoint: &'a AgentInterface,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn Transport>>> + Send + 'a>> {
        Box::pin(async move {
            let transport = JsonRpcTransport::new(TransportConfig::new(&endpoint.url))?;
            Ok(Box::new(transport) as Box<dyn Transport>)
        })
    }
}

/// Builds a [`RestTransport`].
struct RestBuilder;

impl TransportBuilder for RestBuilder {
    fn build<'a>(
        &'a self,
        endpoint: &'a AgentInterface,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn Transport>>> + Send + 'a>> {
        Box::pin(async move {
            let transport = RestTransport::new(&endpoint.url)?;
            Ok(Box::new(transport) as Box<dyn Transport>)
        })
    }
}

/// Factory for creating [`Client`] instances with automatic transport selection.
///
/// Inspects `AgentCard.supported_interfaces` and selects the best compatible
/// transport based on registered builders and client preferences.
///
/// # Default transports
///
/// - JSON-RPC (`JSONRPC`) — primary/fallback
/// - REST (`HTTP+JSON`)
///
/// # Example
///
/// ```ignore
/// let client = ClientFactory::new()
///     .create_from_card(&card).await?;
/// ```
pub struct ClientFactory {
    config: ClientConfig,
    interceptors: Vec<Arc<dyn CallInterceptor>>,
    builders: HashMap<String, Arc<dyn TransportBuilder>>,
}

impl ClientFactory {
    /// Creates a new factory with default transport builders (JSON-RPC + REST).
    #[must_use]
    pub fn new() -> Self {
        let mut builders: HashMap<String, Arc<dyn TransportBuilder>> = HashMap::new();
        builders.insert(
            TransportProtocol::JSONRPC.to_string(),
            Arc::new(JsonRpcBuilder),
        );
        builders.insert(
            TransportProtocol::HTTP_JSON.to_string(),
            Arc::new(RestBuilder),
        );
        Self {
            config: ClientConfig::default(),
            interceptors: Vec::new(),
            builders,
        }
    }

    /// Sets the client configuration.
    #[must_use]
    pub fn with_config(mut self, config: ClientConfig) -> Self {
        self.config = config;
        self
    }

    /// Adds a call interceptor.
    #[must_use]
    pub fn with_interceptor(mut self, interceptor: Arc<dyn CallInterceptor>) -> Self {
        self.interceptors.push(interceptor);
        self
    }

    /// Registers a custom transport builder for a protocol binding.
    #[must_use]
    pub fn with_transport(
        mut self,
        protocol: impl Into<String>,
        builder: Arc<dyn TransportBuilder>,
    ) -> Self {
        self.builders.insert(protocol.into(), builder);
        self
    }

    /// Creates a [`Client`] from an [`AgentCard`] by selecting the best transport.
    ///
    /// Iterates through `card.supported_interfaces`, finds matching registered
    /// builders, sorts by client preference, and returns the first successful connection.
    pub async fn create_from_card(&self, card: &AgentCard) -> Result<Client> {
        if card.supported_interfaces.is_empty() {
            return Err(A2AError::InvalidParams(
                "agent card has no supported interfaces".into(),
            ));
        }

        let mut candidates: Vec<(&AgentInterface, &Arc<dyn TransportBuilder>, usize)> = Vec::new();

        for iface in &card.supported_interfaces {
            let protocol = iface.protocol_binding.as_str();
            if let Some(builder) = self.builders.get(protocol) {
                let priority = self
                    .config
                    .preferred_transports
                    .iter()
                    .position(|p| p.as_str() == protocol)
                    .unwrap_or(usize::MAX);
                candidates.push((iface, builder, priority));
            }
        }

        if candidates.is_empty() {
            let protocols: Vec<_> = card
                .supported_interfaces
                .iter()
                .map(|i| i.protocol_binding.to_string())
                .collect();
            return Err(A2AError::Other(format!(
                "no compatible transports found: available protocols - [{}]",
                protocols.join(", ")
            )));
        }

        candidates.sort_by_key(|&(_, _, priority)| priority);

        let mut errors = Vec::new();
        for (iface, builder, _) in &candidates {
            match builder.build(iface).await {
                Ok(mut transport) => {
                    if let Some(ref tenant) = iface.tenant
                        && !tenant.is_empty()
                    {
                        transport = Box::new(TenantTransportDecorator {
                            inner: transport,
                            tenant: tenant.clone(),
                        });
                    }
                    let mut client = Client::new(transport);
                    client.set_card(card.clone());
                    for interceptor in &self.interceptors {
                        client = client.with_interceptor_arc(Arc::clone(interceptor));
                    }
                    return Ok(client);
                }
                Err(e) => errors.push(format!("{}: {e}", iface.url)),
            }
        }

        Err(A2AError::Other(format!(
            "all transports failed: {}",
            errors.join("; ")
        )))
    }

    /// Creates a [`Client`] by fetching the agent card from the well-known URL,
    /// then selecting the best transport.
    pub async fn create_from_url(&self, base_url: &str) -> Result<Client> {
        let card_url = crate::agent_card_url(base_url);
        let resp = reqwest::get(&card_url)
            .await
            .map_err(|e| A2AError::Other(format!("failed to fetch agent card: {e}")))?;
        let card: AgentCard = resp
            .json()
            .await
            .map_err(|e| A2AError::Other(format!("failed to parse agent card: {e}")))?;
        self.create_from_card(&card).await
    }
}

impl Default for ClientFactory {
    fn default() -> Self {
        Self::new()
    }
}

/// Wraps a [`Transport`] to automatically inject a tenant ID into every request.
///
/// Applied automatically by [`ClientFactory`] when the selected `AgentInterface`
/// has a non-empty `tenant` field, aligned with Go's `tenantTransportDecorator`.
pub struct TenantTransportDecorator {
    inner: Box<dyn Transport>,
    tenant: String,
}

impl Transport for TenantTransportDecorator {
    fn send_message<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SendMessageResponse>> + Send + 'a>> {
        let mut req = req.clone();
        req.tenant = Some(self.tenant.clone());
        Box::pin(async move { self.inner.send_message(params, &req).await })
    }

    fn send_streaming_message<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + 'a>> {
        let mut req = req.clone();
        req.tenant = Some(self.tenant.clone());
        Box::pin(async move { self.inner.send_streaming_message(params, &req).await })
    }

    fn get_task<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a GetTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + 'a>> {
        let mut req = req.clone();
        req.tenant = Some(self.tenant.clone());
        Box::pin(async move { self.inner.get_task(params, &req).await })
    }

    fn list_tasks<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a ListTasksRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTasksResponse>> + Send + 'a>> {
        let mut req = req.clone();
        req.tenant = Some(self.tenant.clone());
        Box::pin(async move { self.inner.list_tasks(params, &req).await })
    }

    fn cancel_task<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a CancelTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + 'a>> {
        let mut req = req.clone();
        req.tenant = Some(self.tenant.clone());
        Box::pin(async move { self.inner.cancel_task(params, &req).await })
    }

    fn subscribe_to_task<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a SubscribeToTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + 'a>> {
        let mut req = req.clone();
        req.tenant = Some(self.tenant.clone());
        Box::pin(async move { self.inner.subscribe_to_task(params, &req).await })
    }

    fn create_task_push_config<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a TaskPushNotificationConfig,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + 'a>> {
        let mut req = req.clone();
        req.tenant = Some(self.tenant.clone());
        Box::pin(async move { self.inner.create_task_push_config(params, &req).await })
    }

    fn get_task_push_config<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a GetTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + 'a>> {
        let mut req = req.clone();
        req.tenant = Some(self.tenant.clone());
        Box::pin(async move { self.inner.get_task_push_config(params, &req).await })
    }

    fn list_task_push_configs<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a ListTaskPushNotificationConfigsRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTaskPushNotificationConfigsResponse>> + Send + 'a>>
    {
        let mut req = req.clone();
        req.tenant = Some(self.tenant.clone());
        Box::pin(async move { self.inner.list_task_push_configs(params, &req).await })
    }

    fn delete_task_push_config<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a DeleteTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        let mut req = req.clone();
        req.tenant = Some(self.tenant.clone());
        Box::pin(async move { self.inner.delete_task_push_config(params, &req).await })
    }

    fn get_extended_agent_card<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a GetExtendedAgentCardRequest,
    ) -> Pin<Box<dyn Future<Output = Result<AgentCard>> + Send + 'a>> {
        let mut req = req.clone();
        req.tenant = Some(self.tenant.clone());
        Box::pin(async move { self.inner.get_extended_agent_card(params, &req).await })
    }

    fn get_agent_card(&self) -> Pin<Box<dyn Future<Output = Result<AgentCard>> + Send + '_>> {
        self.inner.get_agent_card()
    }
}
