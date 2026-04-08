//! A2A client module.
//!
//! - [`Transport`] — transport-agnostic interface for all A2A protocol operations
//! - [`Client`] — concrete client wrapping a [`Transport`] with [`CallInterceptor`] support
//! - [`JsonRpcTransport`] — default HTTP/JSON-RPC + SSE transport
//! - [`GrpcTransport`] — gRPC transport (requires `grpc` feature)

mod factory;
mod interceptor;
mod jsonrpc;
mod rest;

#[cfg(feature = "grpc")]
mod grpc;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub use factory::{ClientFactory, TenantTransportDecorator, TransportBuilder};
use futures::Stream;
#[cfg(feature = "grpc")]
#[cfg_attr(docsrs, doc(cfg(feature = "grpc")))]
pub use grpc::GrpcTransport;
pub use interceptor::{
    CallInterceptor, PassthroughInterceptor, Request, Response, SERVICE_PARAMS, ServiceParams,
    StaticParamsInjector, current_service_params,
};
pub use jsonrpc::{JsonRpcTransport, TransportConfig};
pub use rest::RestTransport;

use crate::error::{A2AError, Result};
use crate::types::{
    AgentCard, CancelTaskRequest, DeleteTaskPushNotificationConfigRequest,
    GetExtendedAgentCardRequest, GetTaskPushNotificationConfigRequest, GetTaskRequest,
    ListTaskPushNotificationConfigsRequest, ListTaskPushNotificationConfigsResponse,
    ListTasksRequest, ListTasksResponse, SendMessageRequest, SendMessageResponse, StreamResponse,
    SubscribeToTaskRequest, Task, TaskPushNotificationConfig, TransportProtocol,
};

/// A boxed stream of [`StreamResponse`] events.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<StreamResponse>> + Send>>;

/// Transport-agnostic interface for all A2A protocol operations.
///
/// Each method corresponds to a single A2A protocol method and receives
/// [`ServiceParams`] for propagating A2A service parameters (version, extensions).
pub trait Transport: Send + Sync {
    /// Sends a message (non-streaming). Corresponds to `message/send`.
    fn send_message<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SendMessageResponse>> + Send + 'a>>;

    /// Sends a message with streaming response. Corresponds to `message/stream`.
    fn send_streaming_message<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + 'a>>;

    /// Retrieves a task. Corresponds to `tasks/get`.
    fn get_task<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a GetTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + 'a>>;

    /// Lists tasks. Corresponds to `tasks/list`.
    fn list_tasks<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a ListTasksRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTasksResponse>> + Send + 'a>>;

    /// Cancels a task. Corresponds to `tasks/cancel`.
    fn cancel_task<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a CancelTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + 'a>>;

    /// Subscribes to task updates. Corresponds to `tasks/subscribe`.
    fn subscribe_to_task<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a SubscribeToTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + 'a>>;

    /// Creates a push notification config. Corresponds to `pushNotificationConfigs/create`.
    fn create_task_push_config<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a TaskPushNotificationConfig,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + 'a>>;

    /// Gets a push notification config.
    fn get_task_push_config<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a GetTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + 'a>>;

    /// Lists push notification configs.
    fn list_task_push_configs<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a ListTaskPushNotificationConfigsRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTaskPushNotificationConfigsResponse>> + Send + 'a>>;

    /// Deletes a push notification config.
    fn delete_task_push_config<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a DeleteTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Gets the extended agent card.
    fn get_extended_agent_card<'a>(
        &'a self,
        params: &'a ServiceParams,
        req: &'a GetExtendedAgentCardRequest,
    ) -> Pin<Box<dyn Future<Output = Result<AgentCard>> + Send + 'a>>;

    /// Retrieves the public agent card (well-known endpoint).
    fn get_agent_card(&self) -> Pin<Box<dyn Future<Output = Result<AgentCard>> + Send + '_>>;

    /// Releases any resources held by the transport.
    fn destroy(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
}

/// Configuration options for [`Client`] behavior.
#[derive(Debug, Clone, Default)]
pub struct ClientConfig {
    /// Default push notification configuration applied to every task.
    pub push_config: Option<TaskPushNotificationConfig>,
    /// MIME types passed with every message.
    pub accepted_output_modes: Vec<String>,
    /// Preferred transport protocols for transport selection.
    pub preferred_transports: Vec<TransportProtocol>,
}

/// A2A protocol client.
///
/// Wraps a [`Transport`] and applies [`CallInterceptor`]s before/after each call.
pub struct Client {
    transport: Box<dyn Transport>,
    interceptors: Vec<Arc<dyn CallInterceptor>>,
    card: std::sync::RwLock<Option<AgentCard>>,
    config: ClientConfig,
}

impl Client {
    /// Creates a new client wrapping the given transport.
    #[must_use]
    pub fn new(transport: Box<dyn Transport>) -> Self {
        Self {
            transport,
            interceptors: Vec::new(),
            card: std::sync::RwLock::new(None),
            config: ClientConfig::default(),
        }
    }

    /// Creates a client from a base URL using [`JsonRpcTransport`].
    pub fn from_url(base_url: impl Into<String>) -> Result<Self> {
        let transport = JsonRpcTransport::from_url(base_url)?;
        Ok(Self::new(Box::new(transport)))
    }

    /// Sets client configuration.
    #[must_use]
    pub fn with_config(mut self, config: ClientConfig) -> Self {
        self.config = config;
        self
    }

    /// Adds a call interceptor.
    #[must_use]
    pub fn with_interceptor(mut self, interceptor: impl CallInterceptor + 'static) -> Self {
        self.interceptors.push(Arc::new(interceptor));
        self
    }

    /// Adds a pre-built call interceptor from an `Arc`.
    #[must_use]
    pub fn with_interceptor_arc(mut self, interceptor: Arc<dyn CallInterceptor>) -> Self {
        self.interceptors.push(interceptor);
        self
    }

    /// Caches an agent card for capability checks.
    pub fn set_card(&self, card: AgentCard) {
        *self.card.write().unwrap() = Some(card);
    }

    /// Returns the cached agent card, if any.
    #[must_use]
    pub fn card(&self) -> Option<AgentCard> {
        self.card.read().unwrap().clone()
    }

    /// Runs all `before` interceptors, returning the (possibly modified) payload
    /// and the [`ServiceParams`] to propagate to the transport.
    async fn intercept_before<P: Send + 'static>(
        &self,
        method: &str,
        payload: P,
    ) -> Result<(P, ServiceParams)> {
        if self.interceptors.is_empty() {
            return Ok((payload, ServiceParams::default()));
        }
        let mut req = Request {
            method: method.to_owned(),
            card: self.card(),
            service_params: ServiceParams::default(),
            payload: Box::new(payload),
        };
        for interceptor in &self.interceptors {
            interceptor.before(&mut req).await?;
        }
        let params = req.service_params;
        match req.payload.downcast::<P>() {
            Ok(p) => Ok((*p, params)),
            Err(_) => Err(A2AError::Other(
                "interceptor changed request payload type".into(),
            )),
        }
    }

    /// Runs all `after` interceptors on a transport result.
    async fn intercept_after<R: Send + 'static>(
        &self,
        method: &str,
        result: Result<R>,
    ) -> Result<R> {
        use std::any::Any;
        if self.interceptors.is_empty() {
            return result;
        }
        let (payload, err) = match result {
            Ok(r) => (Some(Box::new(r) as Box<dyn Any + Send>), None),
            Err(e) => (None, Some(e)),
        };
        let mut resp = Response {
            method: method.to_owned(),
            card: self.card(),
            payload,
            err,
        };
        for interceptor in &self.interceptors {
            interceptor.after(&mut resp).await?;
        }
        if let Some(err) = resp.err {
            return Err(err);
        }
        match resp.payload {
            Some(p) => match p.downcast::<R>() {
                Ok(r) => Ok(*r),
                Err(_) => Err(A2AError::Other(
                    "interceptor changed response payload type".into(),
                )),
            },
            None => Err(A2AError::Other(
                "no response payload after interceptor".into(),
            )),
        }
    }

    /// Sends a message (non-streaming).
    pub async fn send_message(&self, req: &SendMessageRequest) -> Result<SendMessageResponse> {
        let (req, sp) = self.intercept_before("SendMessage", req.clone()).await?;
        let result = SERVICE_PARAMS
            .scope(sp.clone(), async {
                self.transport.send_message(&sp, &req).await
            })
            .await;
        self.intercept_after("SendMessage", result).await
    }

    /// Sends a message with streaming response.
    pub async fn send_streaming_message(&self, req: &SendMessageRequest) -> Result<EventStream> {
        let (req, sp) = self
            .intercept_before("SendStreamingMessage", req.clone())
            .await?;

        // Fallback: if agent doesn't support streaming, use non-streaming call
        if let Some(ref card) = self.card()
            && !card.supports_streaming()
        {
            let result = SERVICE_PARAMS
                .scope(sp.clone(), async {
                    self.transport.send_message(&sp, &req).await
                })
                .await;
            let result = self.intercept_after("SendStreamingMessage", result).await?;
            let event = match result {
                SendMessageResponse::Task(t) => StreamResponse::Task(t),
                SendMessageResponse::Message(m) => StreamResponse::Message(m),
            };
            return Ok(Box::pin(futures::stream::once(async move { Ok(event) })) as EventStream);
        }

        let stream = SERVICE_PARAMS
            .scope(sp.clone(), async {
                self.transport.send_streaming_message(&sp, &req).await
            })
            .await?;
        Ok(stream)
    }

    /// Retrieves a task.
    pub async fn get_task(&self, req: &GetTaskRequest) -> Result<Task> {
        let (req, sp) = self.intercept_before("GetTask", req.clone()).await?;
        let result = SERVICE_PARAMS
            .scope(sp.clone(), async {
                self.transport.get_task(&sp, &req).await
            })
            .await;
        self.intercept_after("GetTask", result).await
    }

    /// Lists tasks.
    pub async fn list_tasks(&self, req: &ListTasksRequest) -> Result<ListTasksResponse> {
        let (req, sp) = self.intercept_before("ListTasks", req.clone()).await?;
        let result = SERVICE_PARAMS
            .scope(sp.clone(), async {
                self.transport.list_tasks(&sp, &req).await
            })
            .await;
        self.intercept_after("ListTasks", result).await
    }

    /// Cancels a task.
    pub async fn cancel_task(&self, req: &CancelTaskRequest) -> Result<Task> {
        let (req, sp) = self.intercept_before("CancelTask", req.clone()).await?;
        let result = SERVICE_PARAMS
            .scope(sp.clone(), async {
                self.transport.cancel_task(&sp, &req).await
            })
            .await;
        self.intercept_after("CancelTask", result).await
    }

    /// Subscribes to task updates.
    pub async fn subscribe_to_task(&self, req: &SubscribeToTaskRequest) -> Result<EventStream> {
        let (req, sp) = self
            .intercept_before("SubscribeToTask", req.clone())
            .await?;
        SERVICE_PARAMS
            .scope(sp.clone(), async {
                self.transport.subscribe_to_task(&sp, &req).await
            })
            .await
    }

    /// Retrieves the public agent card.
    ///
    /// Returns the cached version if available and the agent doesn't support
    /// extended cards.
    pub async fn get_agent_card(&self) -> Result<AgentCard> {
        if let Some(ref card) = self.card()
            && !card.supports_extended_card()
        {
            return Ok(card.clone());
        }

        let result = self.transport.get_agent_card().await;
        let card = self.intercept_after("GetAgentCard", result).await?;
        self.set_card(card.clone());
        Ok(card)
    }

    /// Releases transport resources.
    pub async fn destroy(&self) {
        self.transport.destroy().await;
    }
}
