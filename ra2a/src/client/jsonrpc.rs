//! JSON-RPC 2.0 transport over HTTP with SSE streaming support.
//!
//! Aligned with Go's `jsonrpcTransport` in `a2aclient/jsonrpc.go`.

use std::time::Duration;

use async_trait::async_trait;
use futures::stream;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap};

use super::transport::{EventStream, Transport};
use crate::error::{A2AError, Result};
use crate::jsonrpc::{self, JsonRpcRequest, JsonRpcResponse};
use crate::types::{
    AgentCard, DeleteTaskPushConfigParams, Event, GetTaskPushConfigParams,
    ListTaskPushConfigParams, ListTasksRequest, ListTasksResponse, MessageSendParams,
    SendMessageResult, Task, TaskIdParams, TaskPushConfig, TaskQueryParams,
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
    client: reqwest::Client,
    base_url: String,
    card_url: String,
}

impl JsonRpcTransport {
    /// Creates a new transport with the given configuration.
    pub fn new(config: TransportConfig) -> Result<Self> {
        let base_url = config.base_url.trim_end_matches('/').to_string();
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
    pub fn from_url(base_url: impl Into<String>) -> Result<Self> {
        Self::new(TransportConfig::new(base_url))
    }

    /// Sends a typed JSON-RPC request and deserializes the result.
    async fn rpc_call<P, R>(&self, method: &str, params: &P) -> Result<R>
    where
        P: serde::Serialize + Sync,
        R: serde::de::DeserializeOwned,
    {
        let request = JsonRpcRequest::new(method, params);
        let resp = self
            .client
            .post(&self.base_url)
            .header(CONTENT_TYPE, "application/json")
            .json(&request)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(A2AError::Http(resp.error_for_status().unwrap_err()));
        }

        let rpc: JsonRpcResponse<R> = resp.json().await?;
        match rpc {
            JsonRpcResponse::Success(s) => Ok(s.result),
            JsonRpcResponse::Error(e) => Err(A2AError::JsonRpc(e.error)),
        }
    }

    /// Sends a JSON-RPC request expecting an SSE stream back.
    ///
    /// Each SSE `data:` line is a JSON-RPC response envelope wrapping an [`Event`].
    async fn rpc_stream<P>(&self, method: &str, params: &P) -> Result<EventStream>
    where
        P: serde::Serialize + Sync,
    {
        let request = JsonRpcRequest::new(method, params);
        let resp = self
            .client
            .post(&self.base_url)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "text/event-stream")
            .json(&request)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(A2AError::Http(resp.error_for_status().unwrap_err()));
        }

        let content_type = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if content_type.contains("text/event-stream") {
            Ok(parse_sse_stream(resp))
        } else {
            // Server returned a single JSON-RPC response instead of SSE.
            let rpc: JsonRpcResponse<Event> = resp.json().await?;
            match rpc {
                JsonRpcResponse::Success(s) => {
                    Ok(Box::pin(stream::iter(vec![Ok(s.result)])))
                }
                JsonRpcResponse::Error(e) => Err(A2AError::JsonRpc(e.error)),
            }
        }
    }
}

#[async_trait]
impl Transport for JsonRpcTransport {
    async fn send_message(&self, params: &MessageSendParams) -> Result<SendMessageResult> {
        self.rpc_call(jsonrpc::METHOD_MESSAGE_SEND, params).await
    }

    async fn send_message_stream(&self, params: &MessageSendParams) -> Result<EventStream> {
        self.rpc_stream(jsonrpc::METHOD_MESSAGE_STREAM, params).await
    }

    async fn get_task(&self, params: &TaskQueryParams) -> Result<Task> {
        self.rpc_call(jsonrpc::METHOD_TASKS_GET, params).await
    }

    async fn list_tasks(&self, params: &ListTasksRequest) -> Result<ListTasksResponse> {
        self.rpc_call(jsonrpc::METHOD_TASKS_LIST, params).await
    }

    async fn cancel_task(&self, params: &TaskIdParams) -> Result<Task> {
        self.rpc_call(jsonrpc::METHOD_TASKS_CANCEL, params).await
    }

    async fn resubscribe(&self, params: &TaskIdParams) -> Result<EventStream> {
        self.rpc_stream(jsonrpc::METHOD_TASKS_RESUBSCRIBE, params)
            .await
    }

    async fn set_task_push_config(&self, params: &TaskPushConfig) -> Result<TaskPushConfig> {
        self.rpc_call(jsonrpc::METHOD_PUSH_CONFIG_SET, params).await
    }

    async fn get_task_push_config(
        &self,
        params: &GetTaskPushConfigParams,
    ) -> Result<TaskPushConfig> {
        self.rpc_call(jsonrpc::METHOD_PUSH_CONFIG_GET, params).await
    }

    async fn list_task_push_config(
        &self,
        params: &ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>> {
        self.rpc_call(jsonrpc::METHOD_PUSH_CONFIG_LIST, params)
            .await
    }

    async fn delete_task_push_config(&self, params: &DeleteTaskPushConfigParams) -> Result<()> {
        self.rpc_call(jsonrpc::METHOD_PUSH_CONFIG_DELETE, params)
            .await
    }

    async fn get_agent_card(&self) -> Result<AgentCard> {
        let resp = self.client.get(&self.card_url).send().await?;
        if !resp.status().is_success() {
            return Err(A2AError::Http(resp.error_for_status().unwrap_err()));
        }
        resp.json().await.map_err(Into::into)
    }
}

// ---------------------------------------------------------------------------
// SSE parsing (inline — no separate module needed)
// ---------------------------------------------------------------------------

/// Parses an HTTP response body as an SSE stream of JSON-RPC–wrapped [`Event`]s.
fn parse_sse_stream(response: reqwest::Response) -> EventStream {
    let byte_stream = response.bytes_stream();

    let stream = stream::unfold(
        (byte_stream, String::new()),
        |(mut bytes, mut buf)| async move {
            use futures::TryStreamExt;
            loop {
                // Look for a complete SSE message (double newline).
                if let Some(pos) = buf.find("\n\n") {
                    let raw = buf[..pos].to_string();
                    buf = buf[pos + 2..].to_string();

                    if let Some(event) = parse_sse_message(&raw) {
                        return Some((event, (bytes, buf)));
                    }
                    continue;
                }

                match bytes.try_next().await {
                    Ok(Some(chunk)) => {
                        if let Ok(text) = std::str::from_utf8(&chunk) {
                            buf.push_str(text);
                        }
                    }
                    Ok(None) => return None,
                    Err(e) => {
                        return Some((
                            Err(A2AError::Other(e.to_string())),
                            (bytes, buf),
                        ));
                    }
                }
            }
        },
    );

    Box::pin(stream)
}

/// Parses a single SSE message block into a protocol [`Event`].
///
/// The server wraps each event in a JSON-RPC response envelope:
/// ```text
/// event: <kind>
/// data: {"jsonrpc":"2.0","id":"...","result":{...}}
/// ```
fn parse_sse_message(message: &str) -> Option<Result<Event>> {
    let mut data = String::new();

    for line in message.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.trim());
        }
        // "event:" and "id:" lines are ignored — the Event type is
        // determined by deserializing the JSON-RPC result.
    }

    if data.is_empty() {
        return None;
    }

    // Try parsing as JSON-RPC response wrapping an Event.
    let result: Result<Event> =
        match serde_json::from_str::<JsonRpcResponse<Event>>(&data) {
            Ok(JsonRpcResponse::Success(s)) => Ok(s.result),
            Ok(JsonRpcResponse::Error(e)) => Err(A2AError::JsonRpc(e.error)),
            Err(e) => Err(A2AError::Other(format!("SSE parse error: {e}"))),
        };

    Some(result)
}
