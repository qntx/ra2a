//! JSON-RPC 2.0 transport over HTTP with SSE streaming support.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use futures::stream;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap};

use super::{EventStream, ServiceParams, Transport};
use crate::error::{A2AError, Result};
use crate::jsonrpc::{self, JsonRpcRequest, JsonRpcResponse};
use crate::types::{
    AgentCard, CancelTaskRequest, DeleteTaskPushNotificationConfigRequest,
    GetExtendedAgentCardRequest, GetTaskPushNotificationConfigRequest, GetTaskRequest,
    ListTaskPushNotificationConfigsRequest, ListTaskPushNotificationConfigsResponse,
    ListTasksRequest, ListTasksResponse, SendMessageRequest, SendMessageResponse, StreamResponse,
    SubscribeToTaskRequest, Task, TaskPushNotificationConfig,
};

/// Configuration for creating a [`JsonRpcTransport`].
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Base URL of the A2A agent endpoint.
    pub base_url: String,
    /// Request timeout in seconds (default: 30).
    pub timeout_secs: u64,
    /// Additional HTTP headers.
    pub headers: HeaderMap,
    /// Whether to verify TLS certificates (default: true).
    pub verify_tls: bool,
}

impl TransportConfig {
    /// Creates a new config with the given base URL and sensible defaults.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            timeout_secs: 30,
            headers: HeaderMap::new(),
            verify_tls: true,
        }
    }
}

/// JSON-RPC 2.0 transport for the A2A protocol.
///
/// Handles both synchronous (JSON response) and streaming (SSE) requests.
/// SSE events are wrapped in JSON-RPC response envelopes by the server,
/// and this transport unwraps them into protocol [`Event`]s.
#[derive(Debug, Clone)]
pub struct JsonRpcTransport {
    /// HTTP client for making requests.
    client: reqwest::Client,
    /// Base URL for JSON-RPC requests.
    base_url: String,
    /// URL for fetching the agent card.
    card_url: String,
}

impl JsonRpcTransport {
    /// Creates a new transport with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn new(config: TransportConfig) -> Result<Self> {
        let base_url = config.base_url.trim_end_matches('/').to_owned();
        let card_url = crate::agent_card_url(&base_url);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .default_headers(config.headers)
            .danger_accept_invalid_certs(!config.verify_tls)
            .build()
            .map_err(|e| A2AError::Other(e.to_string()))?;

        Ok(Self {
            client,
            base_url,
            card_url,
        })
    }

    /// Creates a transport from a base URL with default settings.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn from_url(base_url: impl Into<String>) -> Result<Self> {
        Self::new(TransportConfig::new(base_url))
    }

    /// Applies interceptor-set [`ServiceParams`] as HTTP headers.
    fn apply_service_params(builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match super::current_service_params() {
            Some(sp) if !sp.is_empty() => {
                let mut b = builder;
                for (key, value) in sp.iter().flat_map(|(k, vs)| vs.iter().map(move |v| (k, v))) {
                    b = b.header(key, value);
                }
                b
            }
            _ => builder,
        }
    }

    /// Sends a typed JSON-RPC request and deserializes the result.
    async fn rpc_call<P, R>(&self, method: &str, params: &P) -> Result<R>
    where
        P: serde::Serialize + Sync,
        R: serde::de::DeserializeOwned,
    {
        let request = JsonRpcRequest::new(method, params);
        let builder = self
            .client
            .post(&self.base_url)
            .header(CONTENT_TYPE, "application/json")
            .json(&request);
        let resp = Self::apply_service_params(builder).send().await?;

        let resp = resp.error_for_status()?;

        let rpc: JsonRpcResponse<R> = resp.json().await?;
        match rpc {
            JsonRpcResponse::Success(s) => Ok(s.result),
            JsonRpcResponse::Error(e) => Err(A2AError::JsonRpc(e.error)),
        }
    }

    /// Sends a JSON-RPC request expecting an SSE stream back.
    ///
    /// Each SSE `data:` line is a JSON-RPC response envelope wrapping a [`StreamResponse`].
    async fn rpc_stream<P>(&self, method: &str, params: &P) -> Result<EventStream>
    where
        P: serde::Serialize + Sync,
    {
        let request = JsonRpcRequest::new(method, params);
        let builder = self
            .client
            .post(&self.base_url)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "text/event-stream")
            .json(&request);
        let resp = Self::apply_service_params(builder).send().await?;

        let resp = resp.error_for_status()?;

        let content_type = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if content_type.contains("text/event-stream") {
            Ok(parse_sse_stream(resp))
        } else {
            // Server returned a single JSON-RPC response instead of SSE.
            let rpc: JsonRpcResponse<StreamResponse> = resp.json().await?;
            match rpc {
                JsonRpcResponse::Success(s) => {
                    let stream: EventStream = Box::pin(stream::iter(vec![Ok(s.result)]));
                    Ok(stream)
                }
                JsonRpcResponse::Error(e) => Err(A2AError::JsonRpc(e.error)),
            }
        }
    }
}

impl Transport for JsonRpcTransport {
    fn send_message<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SendMessageResponse>> + Send + 'a>> {
        Box::pin(async move { self.rpc_call(jsonrpc::METHOD_MESSAGE_SEND, req).await })
    }

    fn send_streaming_message<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + 'a>> {
        Box::pin(async move { self.rpc_stream(jsonrpc::METHOD_MESSAGE_STREAM, req).await })
    }

    fn get_task<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a GetTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + 'a>> {
        Box::pin(async move { self.rpc_call(jsonrpc::METHOD_TASKS_GET, req).await })
    }

    fn list_tasks<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a ListTasksRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTasksResponse>> + Send + 'a>> {
        Box::pin(async move { self.rpc_call(jsonrpc::METHOD_TASKS_LIST, req).await })
    }

    fn cancel_task<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a CancelTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + 'a>> {
        Box::pin(async move { self.rpc_call(jsonrpc::METHOD_TASKS_CANCEL, req).await })
    }

    fn subscribe_to_task<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a SubscribeToTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + 'a>> {
        Box::pin(async move {
            self.rpc_stream(jsonrpc::METHOD_TASKS_RESUBSCRIBE, req)
                .await
        })
    }

    fn create_task_push_config<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a TaskPushNotificationConfig,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + 'a>> {
        Box::pin(async move { self.rpc_call(jsonrpc::METHOD_PUSH_CONFIG_SET, req).await })
    }

    fn get_task_push_config<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a GetTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + 'a>> {
        Box::pin(async move { self.rpc_call(jsonrpc::METHOD_PUSH_CONFIG_GET, req).await })
    }

    fn list_task_push_configs<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a ListTaskPushNotificationConfigsRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTaskPushNotificationConfigsResponse>> + Send + 'a>>
    {
        Box::pin(async move { self.rpc_call(jsonrpc::METHOD_PUSH_CONFIG_LIST, req).await })
    }

    fn delete_task_push_config<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a DeleteTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move { self.rpc_call(jsonrpc::METHOD_PUSH_CONFIG_DELETE, req).await })
    }

    fn get_extended_agent_card<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a GetExtendedAgentCardRequest,
    ) -> Pin<Box<dyn Future<Output = Result<AgentCard>> + Send + 'a>> {
        Box::pin(async move {
            self.rpc_call(jsonrpc::METHOD_GET_EXTENDED_AGENT_CARD, req)
                .await
        })
    }

    fn get_agent_card(&self) -> Pin<Box<dyn Future<Output = Result<AgentCard>> + Send + '_>> {
        Box::pin(async move {
            let builder = self.client.get(&self.card_url);
            let resp = Self::apply_service_params(builder).send().await?;
            let resp = resp.error_for_status()?;
            resp.json().await.map_err(Into::into)
        })
    }
}

/// Tries to extract a complete SSE event from the buffer.
///
/// Scans for double-newline delimiters; returns `None` when more data is needed.
fn try_extract_sse_event(buf: &mut String) -> Option<Result<StreamResponse>> {
    loop {
        let pos = buf.find("\n\n")?;
        let raw = buf[..pos].to_string();
        *buf = buf[pos + 2..].to_string();
        if let Some(event) = parse_sse_message(&raw) {
            return Some(event);
        }
    }
}

/// Parses an HTTP response body as an SSE stream of JSON-RPC–wrapped [`StreamResponse`]s.
fn parse_sse_stream(response: reqwest::Response) -> EventStream {
    let byte_stream = response.bytes_stream();

    let stream = stream::unfold(
        (byte_stream, String::new()),
        |(mut bytes, mut buf)| async move {
            use futures::TryStreamExt;
            loop {
                if let Some(event) = try_extract_sse_event(&mut buf) {
                    return Some((event, (bytes, buf)));
                }
                match bytes.try_next().await {
                    Ok(Some(chunk)) => buf.extend(std::str::from_utf8(&chunk)),
                    Ok(None) => return None,
                    Err(e) => return Some((Err(A2AError::Other(e.to_string())), (bytes, buf))),
                }
            }
        },
    );

    Box::pin(stream)
}

/// Parses a single SSE message block into a [`StreamResponse`].
///
/// The server wraps each event in a JSON-RPC response envelope:
/// ```text
/// event: <kind>
/// data: {"jsonrpc":"2.0","id":"...","result":{...}}
/// ```
fn parse_sse_message(message: &str) -> Option<Result<StreamResponse>> {
    let mut data = String::new();

    for line in message.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.trim());
        }
    }

    if data.is_empty() {
        return None;
    }

    let result: Result<StreamResponse> =
        match serde_json::from_str::<JsonRpcResponse<StreamResponse>>(&data) {
            Ok(JsonRpcResponse::Success(s)) => Ok(s.result),
            Ok(JsonRpcResponse::Error(e)) => Err(A2AError::JsonRpc(e.error)),
            Err(e) => Err(A2AError::Other(format!("SSE parse error: {e}"))),
        };

    Some(result)
}
