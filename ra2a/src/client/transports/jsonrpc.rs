//! JSON-RPC transport implementation.
//!
//! Implements the A2A protocol over JSON-RPC 2.0 with HTTP transport.

use async_trait::async_trait;
use futures::stream;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue};
use std::time::Duration;

use super::{
    ClientTransport, EventStream, SendMessageResponse, StreamEvent, TransportOptions, TransportType,
};
use crate::error::{A2AError, Result};
use crate::types::{
    AgentCard, DeleteTaskPushNotificationConfigParams, GetTaskPushNotificationConfigParams,
    JsonRpcRequest, JsonRpcResponse, ListTaskPushNotificationConfigParams, Message,
    MessageSendParams, SendMessageResult, Task, TaskArtifactUpdateEvent, TaskIdParams,
    TaskPushNotificationConfig, TaskQueryParams, TaskResubscriptionParams, TaskStatusUpdateEvent,
};

/// JSON-RPC transport for A2A protocol.
#[derive(Debug)]
pub struct JsonRpcTransport {
    /// HTTP client.
    client: reqwest::Client,
    /// Base URL of the agent.
    base_url: String,
    /// Agent card URL.
    card_url: String,
}

impl Clone for JsonRpcTransport {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
            card_url: self.card_url.clone(),
        }
    }
}

impl JsonRpcTransport {
    /// Creates a new JSON-RPC transport with the given options.
    pub fn new(options: TransportOptions) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/json, text/event-stream"),
        );

        for (name, value) in &options.headers {
            if let (Ok(name), Ok(value)) = (
                reqwest::header::HeaderName::try_from(name.as_str()),
                HeaderValue::from_str(value),
            ) {
                headers.insert(name, value);
            }
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(options.timeout_secs))
            .default_headers(headers)
            .danger_accept_invalid_certs(!options.verify_tls)
            .build()
            .map_err(|e| A2AError::Connection(e.to_string()))?;

        let base_url = options.base_url.trim_end_matches('/').to_string();
        let card_url = format!("{}/.well-known/agent.json", base_url);

        Ok(Self {
            client,
            base_url,
            card_url,
        })
    }

    /// Creates a new transport from a base URL with default options.
    pub fn from_url(base_url: impl Into<String>) -> Result<Self> {
        Self::new(TransportOptions::new(base_url))
    }

    /// Sends a JSON-RPC request and returns the result.
    async fn send_request<P, R>(&self, method: &str, params: P) -> Result<R>
    where
        P: serde::Serialize + Send + Sync,
        R: serde::de::DeserializeOwned,
    {
        let request: JsonRpcRequest<P> = JsonRpcRequest::new(method, params);

        let response = self
            .client
            .post(&self.base_url)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        let json_response: JsonRpcResponse<R> = response.json().await?;

        match json_response {
            JsonRpcResponse::Success(success) => Ok(success.result),
            JsonRpcResponse::Error(error) => Err(A2AError::JsonRpc(error.error)),
        }
    }

    /// Sends a streaming JSON-RPC request.
    async fn send_streaming_request<P>(
        &self,
        method: &str,
        params: P,
    ) -> Result<EventStream<StreamEvent>>
    where
        P: serde::Serialize + Send + Sync,
    {
        let request: JsonRpcRequest<P> = JsonRpcRequest::new(method, params);

        let response = self
            .client
            .post(&self.base_url)
            .header(ACCEPT, "text/event-stream")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        // Check if response is SSE
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if content_type.contains("text/event-stream") {
            // Parse SSE stream
            let stream = parse_sse_byte_stream(response);
            Ok(stream)
        } else {
            // Fallback to single response
            let json_response: JsonRpcResponse<SendMessageResult> = response.json().await?;
            match json_response {
                JsonRpcResponse::Success(success) => {
                    let event = match success.result {
                        SendMessageResult::Task(task) => StreamEvent::Task(task),
                        SendMessageResult::Message(msg) => StreamEvent::Message(msg),
                    };
                    Ok(Box::pin(stream::iter(vec![Ok(event)])))
                }
                JsonRpcResponse::Error(error) => Err(A2AError::JsonRpc(error.error)),
            }
        }
    }
}

/// Parses an SSE byte stream into StreamEvents.
fn parse_sse_byte_stream(response: reqwest::Response) -> EventStream<StreamEvent> {
    let bytes_stream = response.bytes_stream();

    let stream = stream::unfold(
        (bytes_stream, String::new()),
        |(mut byte_stream, mut buf)| async move {
            use futures::TryStreamExt;
            loop {
                // Check for complete SSE message in buffer
                if let Some(pos) = buf.find("\n\n") {
                    let message = buf[..pos].to_string();
                    buf = buf[pos + 2..].to_string();

                    if let Some(event) = parse_sse_message(&message) {
                        return Some((event, (byte_stream, buf)));
                    }
                    continue;
                }

                // Read more data
                match byte_stream.try_next().await {
                    Ok(Some(bytes)) => {
                        if let Ok(text) = std::str::from_utf8(&bytes) {
                            buf.push_str(text);
                        }
                    }
                    Ok(None) => return None,
                    Err(e) => {
                        return Some((Err(A2AError::Stream(e.to_string())), (byte_stream, buf)));
                    }
                }
            }
        },
    );

    Box::pin(stream)
}

/// Parses a single SSE message into a StreamEvent.
fn parse_sse_message(message: &str) -> Option<Result<StreamEvent>> {
    let mut event_type = None;
    let mut data = String::new();

    for line in message.lines() {
        if let Some(rest) = line.strip_prefix("event:") {
            event_type = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.trim());
        }
    }

    if data.is_empty() {
        return None;
    }

    let parse_jsonrpc_status = || -> Result<StreamEvent> {
        let resp: JsonRpcResponse<TaskStatusUpdateEvent> = serde_json::from_str(&data)?;
        match resp {
            JsonRpcResponse::Success(s) => Ok(StreamEvent::StatusUpdate(s.result)),
            JsonRpcResponse::Error(e) => Err(A2AError::JsonRpc(e.error)),
        }
    };

    let parse_jsonrpc_artifact = || -> Result<StreamEvent> {
        let resp: JsonRpcResponse<TaskArtifactUpdateEvent> = serde_json::from_str(&data)?;
        match resp {
            JsonRpcResponse::Success(s) => Ok(StreamEvent::ArtifactUpdate(s.result)),
            JsonRpcResponse::Error(e) => Err(A2AError::JsonRpc(e.error)),
        }
    };

    let result: Result<StreamEvent> = match event_type.as_deref() {
        Some("status-update") | Some("TaskStatusUpdateEvent") => parse_jsonrpc_status(),
        Some("artifact-update") | Some("TaskArtifactUpdateEvent") => parse_jsonrpc_artifact(),
        _ => {
            // Try to parse as task or message
            if let Ok(resp) = serde_json::from_str::<JsonRpcResponse<Task>>(&data) {
                match resp {
                    JsonRpcResponse::Success(s) => Ok(StreamEvent::Task(s.result)),
                    JsonRpcResponse::Error(e) => Err(A2AError::JsonRpc(e.error)),
                }
            } else if let Ok(resp) = serde_json::from_str::<JsonRpcResponse<Message>>(&data) {
                match resp {
                    JsonRpcResponse::Success(s) => Ok(StreamEvent::Message(s.result)),
                    JsonRpcResponse::Error(e) => Err(A2AError::JsonRpc(e.error)),
                }
            } else {
                return None;
            }
        }
    };

    Some(result)
}

#[async_trait]
impl ClientTransport for JsonRpcTransport {
    fn transport_type(&self) -> TransportType {
        TransportType::JsonRpc
    }

    async fn send_message(&self, message: Message) -> Result<SendMessageResponse> {
        let params = MessageSendParams::new(message);
        let result: SendMessageResult = self.send_request("message/send", params).await?;

        Ok(match result {
            SendMessageResult::Task(task) => SendMessageResponse::Task(task),
            SendMessageResult::Message(msg) => SendMessageResponse::Message(msg),
        })
    }

    async fn send_message_streaming(&self, message: Message) -> Result<EventStream<StreamEvent>> {
        let params = MessageSendParams::new(message);
        self.send_streaming_request("message/stream", params).await
    }

    async fn get_task(&self, params: TaskQueryParams) -> Result<Task> {
        self.send_request("tasks/get", params).await
    }

    async fn cancel_task(&self, params: TaskIdParams) -> Result<Task> {
        self.send_request("tasks/cancel", params).await
    }

    async fn set_task_push_notification_config(
        &self,
        config: TaskPushNotificationConfig,
    ) -> Result<TaskPushNotificationConfig> {
        self.send_request("tasks/pushNotificationConfig/set", config)
            .await
    }

    async fn get_task_push_notification_config(
        &self,
        params: GetTaskPushNotificationConfigParams,
    ) -> Result<TaskPushNotificationConfig> {
        self.send_request("tasks/pushNotificationConfig/get", params)
            .await
    }

    async fn list_task_push_notification_configs(
        &self,
        params: ListTaskPushNotificationConfigParams,
    ) -> Result<Vec<TaskPushNotificationConfig>> {
        self.send_request("tasks/pushNotificationConfig/list", params)
            .await
    }

    async fn delete_task_push_notification_config(
        &self,
        params: DeleteTaskPushNotificationConfigParams,
    ) -> Result<()> {
        self.send_request("tasks/pushNotificationConfig/delete", params)
            .await
    }

    async fn resubscribe(
        &self,
        params: TaskResubscriptionParams,
    ) -> Result<EventStream<StreamEvent>> {
        self.send_streaming_request("tasks/resubscribe", params)
            .await
    }

    async fn get_agent_card(&self) -> Result<AgentCard> {
        let response = self.client.get(&self.card_url).send().await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        let card: AgentCard = response.json().await?;
        Ok(card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_creation() {
        let transport = JsonRpcTransport::from_url("https://example.com").unwrap();
        assert_eq!(transport.base_url, "https://example.com");
        assert_eq!(
            transport.card_url,
            "https://example.com/.well-known/agent.json"
        );
    }

    #[test]
    fn test_parse_sse_message() {
        let message = "event: status-update\ndata: {\"jsonrpc\":\"2.0\",\"id\":\"1\",\"result\":{\"task_id\":\"t1\",\"context_id\":\"c1\",\"status\":{\"state\":\"working\"},\"final\":false,\"kind\":\"status-update\"}}";
        let result = parse_sse_message(message);
        assert!(result.is_some());
    }
}
