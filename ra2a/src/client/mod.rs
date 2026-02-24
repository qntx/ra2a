//! A2A client module.
//!
//! - [`Transport`] — transport-agnostic interface for all A2A protocol operations
//! - [`Client`] — concrete client wrapping a [`Transport`] with [`CallInterceptor`] support
//! - [`JsonRpcTransport`] — default HTTP/JSON-RPC + SSE transport

mod interceptor;
mod jsonrpc;

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
pub use interceptor::{CallContext, CallInterceptor, CallMeta, Request, Response};
pub use jsonrpc::{JsonRpcTransport, TransportConfig};

use crate::error::Result;
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
    interceptors: Vec<Box<dyn CallInterceptor>>,
    card: std::sync::RwLock<Option<AgentCard>>,
    config: ClientConfig,
}

impl Client {
    /// Creates a new client wrapping the given transport.
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
    pub fn with_config(mut self, config: ClientConfig) -> Self {
        self.config = config;
        self
    }

    /// Adds a call interceptor.
    pub fn with_interceptor(mut self, interceptor: impl CallInterceptor + 'static) -> Self {
        self.interceptors.push(Box::new(interceptor));
        self
    }

    /// Adds a call interceptor after creation.
    pub fn add_interceptor(&mut self, interceptor: impl CallInterceptor + 'static) {
        self.interceptors.push(Box::new(interceptor));
    }

    /// Caches an agent card for capability checks.
    pub fn set_card(&self, card: AgentCard) {
        *self.card.write().unwrap() = Some(card);
    }

    /// Returns the cached agent card, if any.
    pub fn card(&self) -> Option<AgentCard> {
        self.card.read().unwrap().clone()
    }

    // -- Interceptor helpers --------------------------------------------------

    /// Runs all `before` interceptors for the given method.
    async fn intercept_before(&self, method: &str, req: &mut Request) -> Result<()> {
        let ctx = CallContext {
            method: method.to_string(),
            agent_card: self.card(),
        };
        for interceptor in &self.interceptors {
            interceptor.before(&ctx, req).await?;
        }
        Ok(())
    }

    /// Runs all `after` interceptors for the given method.
    async fn intercept_after(&self, method: &str, resp: &mut Response) -> Result<()> {
        let ctx = CallContext {
            method: method.to_string(),
            agent_card: self.card(),
        };
        for interceptor in &self.interceptors {
            interceptor.after(&ctx, resp).await?;
        }
        Ok(())
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
            && let Some(ref modes) = self.config.accepted_output_modes {
                config.accepted_output_modes = modes.clone();
            }
        config.blocking = Some(blocking);
        result
    }

    // ----- A2A protocol methods (delegate to transport with interceptors) ----

    /// Sends a message (non-streaming). Corresponds to `message/send`.
    pub async fn send_message(&self, params: &MessageSendParams) -> Result<SendMessageResult> {
        let params = self.with_default_send_config(params, !self.config.polling);
        let mut req = Request {
            meta: CallMeta::default(),
        };
        self.intercept_before("SendMessage", &mut req).await?;
        let result = self.transport.send_message(&params).await;
        let mut resp = Response {
            meta: CallMeta::default(),
        };
        self.intercept_after("SendMessage", &mut resp).await?;
        result
    }

    /// Sends a message with streaming response. Corresponds to `message/stream`.
    ///
    /// If the cached agent card indicates the agent does not support streaming,
    /// falls back to a non-streaming `send_message` and wraps the result as a
    /// single-element stream.
    pub async fn send_message_stream(&self, params: &MessageSendParams) -> Result<EventStream> {
        let params = self.with_default_send_config(params, true);
        let mut req = Request {
            meta: CallMeta::default(),
        };
        self.intercept_before("SendStreamingMessage", &mut req)
            .await?;

        // Fallback: if agent doesn't support streaming, use non-streaming call
        if let Some(ref card) = self.card()
            && !card.supports_streaming() {
                let result = self.transport.send_message(&params).await?;
                let mut resp = Response {
                    meta: CallMeta::default(),
                };
                self.intercept_after("SendStreamingMessage", &mut resp)
                    .await?;
                let event = match result {
                    SendMessageResult::Task(t) => Event::Task(t),
                    SendMessageResult::Message(m) => Event::Message(m),
                };
                let stream: EventStream = Box::pin(futures::stream::once(async move { Ok(event) }));
                return Ok(stream);
            }

        let result = self.transport.send_message_stream(&params).await;
        let mut resp = Response {
            meta: CallMeta::default(),
        };
        self.intercept_after("SendStreamingMessage", &mut resp)
            .await?;
        result
    }

    /// Retrieves a task. Corresponds to `tasks/get`.
    pub async fn get_task(&self, params: &TaskQueryParams) -> Result<Task> {
        let mut req = Request {
            meta: CallMeta::default(),
        };
        self.intercept_before("GetTask", &mut req).await?;
        let result = self.transport.get_task(params).await;
        let mut resp = Response {
            meta: CallMeta::default(),
        };
        self.intercept_after("GetTask", &mut resp).await?;
        result
    }

    /// Lists tasks. Corresponds to `tasks/list`.
    pub async fn list_tasks(&self, params: &ListTasksRequest) -> Result<ListTasksResponse> {
        let mut req = Request {
            meta: CallMeta::default(),
        };
        self.intercept_before("ListTasks", &mut req).await?;
        let result = self.transport.list_tasks(params).await;
        let mut resp = Response {
            meta: CallMeta::default(),
        };
        self.intercept_after("ListTasks", &mut resp).await?;
        result
    }

    /// Cancels a task. Corresponds to `tasks/cancel`.
    pub async fn cancel_task(&self, params: &TaskIdParams) -> Result<Task> {
        let mut req = Request {
            meta: CallMeta::default(),
        };
        self.intercept_before("CancelTask", &mut req).await?;
        let result = self.transport.cancel_task(params).await;
        let mut resp = Response {
            meta: CallMeta::default(),
        };
        self.intercept_after("CancelTask", &mut resp).await?;
        result
    }

    /// Resubscribes to a task's event stream. Corresponds to `tasks/resubscribe`.
    pub async fn resubscribe(&self, params: &TaskIdParams) -> Result<EventStream> {
        let mut req = Request {
            meta: CallMeta::default(),
        };
        self.intercept_before("ResubscribeToTask", &mut req).await?;
        let result = self.transport.resubscribe(params).await;
        let mut resp = Response {
            meta: CallMeta::default(),
        };
        self.intercept_after("ResubscribeToTask", &mut resp).await?;
        result
    }

    /// Sets push notification config. Corresponds to `tasks/pushNotificationConfig/set`.
    pub async fn set_task_push_config(&self, params: &TaskPushConfig) -> Result<TaskPushConfig> {
        let mut req = Request {
            meta: CallMeta::default(),
        };
        self.intercept_before("SetTaskPushConfig", &mut req).await?;
        let result = self.transport.set_task_push_config(params).await;
        let mut resp = Response {
            meta: CallMeta::default(),
        };
        self.intercept_after("SetTaskPushConfig", &mut resp).await?;
        result
    }

    /// Gets push notification config. Corresponds to `tasks/pushNotificationConfig/get`.
    pub async fn get_task_push_config(
        &self,
        params: &GetTaskPushConfigParams,
    ) -> Result<TaskPushConfig> {
        let mut req = Request {
            meta: CallMeta::default(),
        };
        self.intercept_before("GetTaskPushConfig", &mut req).await?;
        let result = self.transport.get_task_push_config(params).await;
        let mut resp = Response {
            meta: CallMeta::default(),
        };
        self.intercept_after("GetTaskPushConfig", &mut resp).await?;
        result
    }

    /// Lists push notification configs. Corresponds to `tasks/pushNotificationConfig/list`.
    pub async fn list_task_push_config(
        &self,
        params: &ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>> {
        let mut req = Request {
            meta: CallMeta::default(),
        };
        self.intercept_before("ListTaskPushConfig", &mut req)
            .await?;
        let result = self.transport.list_task_push_config(params).await;
        let mut resp = Response {
            meta: CallMeta::default(),
        };
        self.intercept_after("ListTaskPushConfig", &mut resp)
            .await?;
        result
    }

    /// Deletes push notification config. Corresponds to `tasks/pushNotificationConfig/delete`.
    pub async fn delete_task_push_config(&self, params: &DeleteTaskPushConfigParams) -> Result<()> {
        let mut req = Request {
            meta: CallMeta::default(),
        };
        self.intercept_before("DeleteTaskPushConfig", &mut req)
            .await?;
        let result = self.transport.delete_task_push_config(params).await;
        let mut resp = Response {
            meta: CallMeta::default(),
        };
        self.intercept_after("DeleteTaskPushConfig", &mut resp)
            .await?;
        result
    }

    /// Retrieves the agent card from the server.
    ///
    /// If the card is already cached and doesn't support extended cards,
    /// returns the cached version. Otherwise fetches from transport.
    pub async fn get_agent_card(&self) -> Result<AgentCard> {
        // If card is cached and doesn't support extended card, return cached
        if let Some(ref card) = self.card()
            && !card.supports_authenticated_extended_card {
                return Ok(card.clone());
            }

        let mut req = Request {
            meta: CallMeta::default(),
        };
        self.intercept_before("GetAgentCard", &mut req).await?;
        let card = self.transport.get_agent_card().await?;
        let mut resp = Response {
            meta: CallMeta::default(),
        };
        self.intercept_after("GetAgentCard", &mut resp).await?;
        self.set_card(card.clone());
        Ok(card)
    }

    /// Releases transport resources.
    pub async fn destroy(&self) {
        self.transport.destroy().await;
    }
}
