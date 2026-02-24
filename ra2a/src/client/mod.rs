//! A2A client module.
//!
//! - [`Transport`] — transport-agnostic interface for all A2A protocol operations
//! - [`Client`] — concrete client wrapping a [`Transport`] with [`CallInterceptor`] support
//! - [`JsonRpcTransport`] — default HTTP/JSON-RPC + SSE transport
//! - [`GrpcTransport`] — gRPC transport (requires `grpc` feature)

mod interceptor;
mod jsonrpc;

#[cfg(feature = "grpc")]
mod grpc;

use std::any::Any;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
#[cfg(feature = "grpc")]
#[cfg_attr(docsrs, doc(cfg(feature = "grpc")))]
pub use grpc::GrpcTransport;
pub use interceptor::{
    CALL_META, CallInterceptor, CallMeta, PassthroughInterceptor, Request, Response,
    StaticCallMetaInjector, call_meta,
};
pub use jsonrpc::{JsonRpcTransport, TransportConfig};

use crate::error::{A2AError, Result};
use crate::types::{
    AgentCard, DeleteTaskPushConfigParams, Event, GetTaskPushConfigParams,
    ListTaskPushConfigParams, ListTasksRequest, ListTasksResponse, MessageSendParams,
    SendMessageResult, Task, TaskIdParams, TaskPushConfig, TaskQueryParams,
};

/// A boxed stream of protocol [`Event`]s for streaming responses.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<Event>> + Send>>;

/// Transport-agnostic interface for all A2A protocol operations.
///
/// Each method corresponds to a single A2A protocol method. The transport
/// handles serialization, HTTP/gRPC details, and SSE parsing internally.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Sends a message (non-streaming). Corresponds to `message/send`.
    async fn send_message(&self, params: &MessageSendParams) -> Result<SendMessageResult>;

    /// Sends a message with streaming response. Corresponds to `message/stream`.
    async fn send_message_stream(&self, params: &MessageSendParams) -> Result<EventStream>;

    /// Retrieves a task. Corresponds to `tasks/get`.
    async fn get_task(&self, params: &TaskQueryParams) -> Result<Task>;

    /// Lists tasks. Corresponds to `tasks/list`.
    async fn list_tasks(&self, params: &ListTasksRequest) -> Result<ListTasksResponse>;

    /// Cancels a task. Corresponds to `tasks/cancel`.
    async fn cancel_task(&self, params: &TaskIdParams) -> Result<Task>;

    /// Resubscribes to a task's event stream. Corresponds to `tasks/resubscribe`.
    async fn resubscribe(&self, params: &TaskIdParams) -> Result<EventStream>;

    /// Sets push notification config for a task.
    async fn set_task_push_config(&self, params: &TaskPushConfig) -> Result<TaskPushConfig>;

    /// Gets push notification config for a task.
    async fn get_task_push_config(
        &self,
        params: &GetTaskPushConfigParams,
    ) -> Result<TaskPushConfig>;

    /// Lists push notification configs for a task.
    async fn list_task_push_config(
        &self,
        params: &ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>>;

    /// Deletes push notification config for a task.
    async fn delete_task_push_config(&self, params: &DeleteTaskPushConfigParams) -> Result<()>;

    /// Retrieves the agent card.
    async fn get_agent_card(&self) -> Result<AgentCard>;

    /// Releases any resources held by the transport.
    async fn destroy(&self) {}
}

/// Configuration options for [`Client`] behavior.
///
/// Aligned with Go's `a2aclient.Config`.
#[derive(Debug, Clone, Default)]
pub struct ClientConfig {
    /// Default push notification configuration applied to every task.
    pub push_config: Option<crate::types::PushConfig>,
    /// MIME types passed with every message; agents may use these to decide
    /// the result format.
    pub accepted_output_modes: Option<Vec<String>>,
    /// If `true`, non-streaming `send_message` receives a result immediately
    /// (possibly in a non-terminal state) instead of blocking. The caller is
    /// responsible for polling.
    pub polling: bool,
}

/// A2A protocol client.
///
/// Wraps a [`Transport`] and applies [`CallInterceptor`]s before/after each
/// call. Optionally merges default [`ClientConfig`] into outgoing requests.
///
/// # Example
///
/// ```no_run
/// use ra2a::client::{Client, JsonRpcTransport};
/// use ra2a::types::{Message, MessageSendParams, Part};
///
/// # async fn example() -> ra2a::error::Result<()> {
/// let client = Client::from_url("https://agent.example.com")?;
/// let msg = Message::user(vec![Part::text("Hello")]);
/// let result = client.send_message(&MessageSendParams::new(msg)).await?;
/// # Ok(())
/// # }
/// ```
pub struct Client {
    transport: Box<dyn Transport>,
    interceptors: Vec<Arc<dyn CallInterceptor>>,
    card: std::sync::RwLock<Option<AgentCard>>,
    config: ClientConfig,
    base_url: String,
}

impl Client {
    /// Creates a new client wrapping the given transport.
    pub fn new(transport: Box<dyn Transport>) -> Self {
        Self {
            transport,
            interceptors: Vec::new(),
            card: std::sync::RwLock::new(None),
            config: ClientConfig::default(),
            base_url: String::new(),
        }
    }

    /// Creates a client from a base URL using [`JsonRpcTransport`].
    pub fn from_url(base_url: impl Into<String>) -> Result<Self> {
        let url: String = base_url.into();
        let transport = JsonRpcTransport::from_url(&url)?;
        let mut client = Self::new(Box::new(transport));
        client.base_url = url;
        Ok(client)
    }

    /// Sets the base URL exposed to interceptors via [`Request::base_url`].
    ///
    /// This is set automatically by [`from_url`](Self::from_url). Only needed
    /// when constructing a client with [`new`](Self::new) and a custom transport.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Sets client configuration.
    pub fn with_config(mut self, config: ClientConfig) -> Self {
        self.config = config;
        self
    }

    /// Adds a call interceptor.
    pub fn with_interceptor(mut self, interceptor: impl CallInterceptor + 'static) -> Self {
        self.interceptors.push(Arc::new(interceptor));
        self
    }

    /// Adds a call interceptor after creation.
    pub fn add_interceptor(&mut self, interceptor: impl CallInterceptor + 'static) {
        self.interceptors.push(Arc::new(interceptor));
    }

    /// Caches an agent card for capability checks.
    pub fn set_card(&self, card: AgentCard) {
        *self.card.write().unwrap() = Some(card);
    }

    /// Returns the cached agent card, if any.
    pub fn card(&self) -> Option<AgentCard> {
        self.card.read().unwrap().clone()
    }

    /// Runs all `before` interceptors, returning the (possibly modified) payload
    /// and the [`CallMeta`] to propagate to the transport.
    ///
    /// Aligned with Go's generic `interceptBefore[T any]`.
    async fn intercept_before<P: Send + 'static>(
        &self,
        method: &str,
        payload: P,
    ) -> Result<(P, CallMeta)> {
        if self.interceptors.is_empty() {
            return Ok((payload, CallMeta::default()));
        }
        let mut req = Request {
            method: method.to_string(),
            base_url: self.base_url.clone(),
            meta: CallMeta::default(),
            card: self.card(),
            payload: Box::new(payload),
        };
        for interceptor in &self.interceptors {
            interceptor.before(&mut req).await?;
        }
        let meta = req.meta;
        match req.payload.downcast::<P>() {
            Ok(p) => Ok((*p, meta)),
            Err(_) => Err(A2AError::Other(
                "interceptor changed request payload type".into(),
            )),
        }
    }

    /// Runs all `after` interceptors on a transport result, returning the
    /// (possibly modified) result.
    ///
    /// Aligned with Go's generic `interceptAfter[T any]`.
    async fn intercept_after<R: Send + 'static>(
        &self,
        method: &str,
        result: Result<R>,
    ) -> Result<R> {
        if self.interceptors.is_empty() {
            return result;
        }
        let (payload, err) = match result {
            Ok(r) => (Some(Box::new(r) as Box<dyn Any + Send>), None),
            Err(e) => (None, Some(e)),
        };
        // Carry forward the CallMeta set by intercept_before (mirrors Go's CallMetaFrom(ctx))
        let meta = call_meta().unwrap_or_default();
        let mut resp = Response {
            method: method.to_string(),
            base_url: self.base_url.clone(),
            meta,
            card: self.card(),
            payload,
            err,
        };
        // Go's interceptAfter iterates forward (matching implementation, not doc)
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

    /// Wraps an event stream to apply `after` interceptors on each event.
    ///
    /// Aligned with Go's per-event `interceptAfter` in streaming methods
    /// (`SendStreamingMessage`, `ResubscribeToTask`).
    fn wrap_stream(&self, method: &'static str, stream: EventStream) -> EventStream {
        if self.interceptors.is_empty() {
            return stream;
        }
        let interceptors = self.interceptors.clone();
        let card = self.card();
        let base_url = self.base_url.clone();
        let wrapped = stream.then(move |event_result| {
            let interceptors = interceptors.clone();
            let card = card.clone();
            let base_url = base_url.clone();
            async move {
                let (payload, err) = match event_result {
                    Ok(event) => (Some(Box::new(event) as Box<dyn Any + Send>), None),
                    Err(e) => (None, Some(e)),
                };
                let meta = call_meta().unwrap_or_default();
                let mut resp = Response {
                    method: method.to_string(),
                    base_url,
                    meta,
                    card,
                    payload,
                    err,
                };
                for interceptor in &interceptors {
                    interceptor.after(&mut resp).await?;
                }
                if let Some(err) = resp.err {
                    return Err(err);
                }
                match resp.payload {
                    Some(p) => match p.downcast::<Event>() {
                        Ok(e) => Ok(*e),
                        Err(_) => Err(A2AError::Other(
                            "interceptor changed event payload type".into(),
                        )),
                    },
                    None => Err(A2AError::Other("no event payload after interceptor".into())),
                }
            }
        });
        Box::pin(wrapped)
    }

    /// Applies default config to outgoing send params (push config,
    /// accepted output modes, blocking flag).
    fn with_default_send_config(
        &self,
        params: &MessageSendParams,
        blocking: bool,
    ) -> MessageSendParams {
        if self.config.push_config.is_none()
            && self.config.accepted_output_modes.is_none()
            && blocking
        {
            return params.clone();
        }

        let mut result = params.clone();
        let config = result.configuration.get_or_insert_with(Default::default);
        if config.push_notification_config.is_none() {
            config.push_notification_config = self.config.push_config.clone();
        }
        if config.accepted_output_modes.is_empty()
            && let Some(ref modes) = self.config.accepted_output_modes
        {
            config.accepted_output_modes = modes.clone();
        }
        config.blocking = Some(blocking);
        result
    }

    /// Sends a message (non-streaming). Corresponds to `message/send`.
    pub async fn send_message(&self, params: &MessageSendParams) -> Result<SendMessageResult> {
        let params = self.with_default_send_config(params, !self.config.polling);
        let (params, meta) = self.intercept_before("SendMessage", params).await?;
        let result = CALL_META
            .scope(meta, async { self.transport.send_message(&params).await })
            .await;
        self.intercept_after("SendMessage", result).await
    }

    /// Sends a message with streaming response. Corresponds to `message/stream`.
    ///
    /// If the cached agent card indicates the agent does not support streaming,
    /// falls back to a non-streaming `send_message` and wraps the result as a
    /// single-element stream.
    ///
    /// Each event in the stream is individually passed through `after` interceptors,
    /// matching Go's per-event `interceptAfter` behavior.
    pub async fn send_message_stream(&self, params: &MessageSendParams) -> Result<EventStream> {
        let params = self.with_default_send_config(params, true);
        let (params, meta) = self
            .intercept_before("SendStreamingMessage", params)
            .await?;

        // Fallback: if agent doesn't support streaming, use non-streaming call
        if let Some(ref card) = self.card()
            && !card.supports_streaming()
        {
            let result = CALL_META
                .scope(meta, async { self.transport.send_message(&params).await })
                .await;
            let result = self.intercept_after("SendStreamingMessage", result).await?;
            let event = match result {
                SendMessageResult::Task(t) => Event::Task(t),
                SendMessageResult::Message(m) => Event::Message(m),
            };
            let stream: EventStream = Box::pin(futures::stream::once(async move { Ok(event) }));
            return Ok(self.wrap_stream("SendStreamingMessage", stream));
        }

        let stream = CALL_META
            .scope(meta, async {
                self.transport.send_message_stream(&params).await
            })
            .await?;
        Ok(self.wrap_stream("SendStreamingMessage", stream))
    }

    /// Retrieves a task. Corresponds to `tasks/get`.
    pub async fn get_task(&self, params: &TaskQueryParams) -> Result<Task> {
        let (params, meta) = self.intercept_before("GetTask", params.clone()).await?;
        let result = CALL_META
            .scope(meta, async { self.transport.get_task(&params).await })
            .await;
        self.intercept_after("GetTask", result).await
    }

    /// Lists tasks. Corresponds to `tasks/list`.
    pub async fn list_tasks(&self, params: &ListTasksRequest) -> Result<ListTasksResponse> {
        let (params, meta) = self.intercept_before("ListTasks", params.clone()).await?;
        let result = CALL_META
            .scope(meta, async { self.transport.list_tasks(&params).await })
            .await;
        self.intercept_after("ListTasks", result).await
    }

    /// Cancels a task. Corresponds to `tasks/cancel`.
    pub async fn cancel_task(&self, params: &TaskIdParams) -> Result<Task> {
        let (params, meta) = self.intercept_before("CancelTask", params.clone()).await?;
        let result = CALL_META
            .scope(meta, async { self.transport.cancel_task(&params).await })
            .await;
        self.intercept_after("CancelTask", result).await
    }

    /// Resubscribes to a task's event stream. Corresponds to `tasks/resubscribe`.
    ///
    /// Each event in the stream is individually passed through `after` interceptors.
    pub async fn resubscribe(&self, params: &TaskIdParams) -> Result<EventStream> {
        let (params, meta) = self
            .intercept_before("ResubscribeToTask", params.clone())
            .await?;
        let stream = CALL_META
            .scope(meta, async { self.transport.resubscribe(&params).await })
            .await?;
        Ok(self.wrap_stream("ResubscribeToTask", stream))
    }

    /// Sets push notification config. Corresponds to `tasks/pushNotificationConfig/set`.
    pub async fn set_task_push_config(&self, params: &TaskPushConfig) -> Result<TaskPushConfig> {
        let (params, meta) = self
            .intercept_before("SetTaskPushConfig", params.clone())
            .await?;
        let result = CALL_META
            .scope(meta, async {
                self.transport.set_task_push_config(&params).await
            })
            .await;
        self.intercept_after("SetTaskPushConfig", result).await
    }

    /// Gets push notification config. Corresponds to `tasks/pushNotificationConfig/get`.
    pub async fn get_task_push_config(
        &self,
        params: &GetTaskPushConfigParams,
    ) -> Result<TaskPushConfig> {
        let (params, meta) = self
            .intercept_before("GetTaskPushConfig", params.clone())
            .await?;
        let result = CALL_META
            .scope(meta, async {
                self.transport.get_task_push_config(&params).await
            })
            .await;
        self.intercept_after("GetTaskPushConfig", result).await
    }

    /// Lists push notification configs. Corresponds to `tasks/pushNotificationConfig/list`.
    pub async fn list_task_push_config(
        &self,
        params: &ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>> {
        let (params, meta) = self
            .intercept_before("ListTaskPushConfig", params.clone())
            .await?;
        let result = CALL_META
            .scope(meta, async {
                self.transport.list_task_push_config(&params).await
            })
            .await;
        self.intercept_after("ListTaskPushConfig", result).await
    }

    /// Deletes push notification config. Corresponds to `tasks/pushNotificationConfig/delete`.
    pub async fn delete_task_push_config(&self, params: &DeleteTaskPushConfigParams) -> Result<()> {
        let (params, meta) = self
            .intercept_before("DeleteTaskPushConfig", params.clone())
            .await?;
        let result = CALL_META
            .scope(meta, async {
                self.transport.delete_task_push_config(&params).await
            })
            .await;
        self.intercept_after("DeleteTaskPushConfig", result).await
    }

    /// Retrieves the agent card from the server.
    ///
    /// If the card is already cached and doesn't support extended cards,
    /// returns the cached version. Otherwise fetches from transport.
    pub async fn get_agent_card(&self) -> Result<AgentCard> {
        if let Some(ref card) = self.card()
            && !card.supports_authenticated_extended_card
        {
            return Ok(card.clone());
        }

        let (_, meta) = self.intercept_before("GetAgentCard", ()).await?;
        let result = CALL_META
            .scope(meta, async { self.transport.get_agent_card().await })
            .await;
        let card = self.intercept_after("GetAgentCard", result).await?;
        self.set_card(card.clone());
        Ok(card)
    }

    /// Releases transport resources.
    pub async fn destroy(&self) {
        self.transport.destroy().await;
    }
}
