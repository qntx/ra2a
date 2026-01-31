//! REST transport implementation.
//!
//! Implements the A2A protocol over HTTP+JSON REST API.

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
    ListTaskPushNotificationConfigParams, Message, Task, TaskArtifactUpdateEvent, TaskIdParams,
    TaskPushNotificationConfig, TaskQueryParams, TaskResubscriptionParams, TaskStatusUpdateEvent,
};

/// REST transport for A2A protocol.
#[derive(Debug, Clone)]
pub struct RestTransport {
    /// HTTP client.
    client: reqwest::Client,
    /// Base URL of the agent.
    base_url: String,
    /// Agent card URL.
    card_url: String,
}

impl RestTransport {
    /// Creates a new REST transport with the given options.
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

    /// Builds the URL for a given path.
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

/// Parses an SSE byte stream into StreamEvents for REST transport.
fn parse_rest_sse_byte_stream(response: reqwest::Response) -> EventStream<StreamEvent> {
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

                    if let Some(event) = parse_rest_sse_message(&message) {
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

/// Parses a single SSE message from REST API into a StreamEvent.
fn parse_rest_sse_message(message: &str) -> Option<Result<StreamEvent>> {
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

    let result = match event_type.as_deref() {
        Some("status-update") | Some("TaskStatusUpdateEvent") => {
            serde_json::from_str::<TaskStatusUpdateEvent>(&data)
                .map(StreamEvent::StatusUpdate)
                .map_err(|e| A2AError::Json(e))
        }
        Some("artifact-update") | Some("TaskArtifactUpdateEvent") => {
            serde_json::from_str::<TaskArtifactUpdateEvent>(&data)
                .map(StreamEvent::ArtifactUpdate)
                .map_err(|e| A2AError::Json(e))
        }
        Some("task") => serde_json::from_str::<Task>(&data)
            .map(StreamEvent::Task)
            .map_err(|e| A2AError::Json(e)),
        Some("message") => serde_json::from_str::<Message>(&data)
            .map(StreamEvent::Message)
            .map_err(|e| A2AError::Json(e)),
        _ => {
            // Try to auto-detect type
            if let Ok(task) = serde_json::from_str::<Task>(&data) {
                Ok(StreamEvent::Task(task))
            } else if let Ok(msg) = serde_json::from_str::<Message>(&data) {
                Ok(StreamEvent::Message(msg))
            } else if let Ok(status) = serde_json::from_str::<TaskStatusUpdateEvent>(&data) {
                Ok(StreamEvent::StatusUpdate(status))
            } else if let Ok(artifact) = serde_json::from_str::<TaskArtifactUpdateEvent>(&data) {
                Ok(StreamEvent::ArtifactUpdate(artifact))
            } else {
                return None;
            }
        }
    };

    Some(result)
}

#[async_trait]
impl ClientTransport for RestTransport {
    fn transport_type(&self) -> TransportType {
        TransportType::Rest
    }

    async fn send_message(&self, message: Message) -> Result<SendMessageResponse> {
        let response = self
            .client
            .post(self.url("/message"))
            .json(&message)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        // Try to parse as Task first, then as Message
        let text = response.text().await?;
        if let Ok(task) = serde_json::from_str::<Task>(&text) {
            Ok(SendMessageResponse::Task(task))
        } else if let Ok(msg) = serde_json::from_str::<Message>(&text) {
            Ok(SendMessageResponse::Message(msg))
        } else {
            Err(A2AError::Json(
                serde_json::from_str::<Task>(&text).unwrap_err(),
            ))
        }
    }

    async fn send_message_streaming(&self, message: Message) -> Result<EventStream<StreamEvent>> {
        let response = self
            .client
            .post(self.url("/message/stream"))
            .header(ACCEPT, "text/event-stream")
            .json(&message)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if content_type.contains("text/event-stream") {
            Ok(parse_rest_sse_byte_stream(response))
        } else {
            // Fallback to single response
            let text = response.text().await?;
            let event = if let Ok(task) = serde_json::from_str::<Task>(&text) {
                StreamEvent::Task(task)
            } else if let Ok(msg) = serde_json::from_str::<Message>(&text) {
                StreamEvent::Message(msg)
            } else {
                return Err(A2AError::Json(
                    serde_json::from_str::<Task>(&text).unwrap_err(),
                ));
            };
            Ok(Box::pin(stream::iter(vec![Ok(event)])))
        }
    }

    async fn get_task(&self, params: TaskQueryParams) -> Result<Task> {
        let mut url = self.url(&format!("/tasks/{}", params.id));
        if let Some(len) = params.history_length {
            url = format!("{}?historyLength={}", url, len);
        }

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        Ok(response.json().await?)
    }

    async fn cancel_task(&self, params: TaskIdParams) -> Result<Task> {
        let response = self
            .client
            .post(self.url(&format!("/tasks/{}/cancel", params.id)))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        Ok(response.json().await?)
    }

    async fn set_task_push_notification_config(
        &self,
        config: TaskPushNotificationConfig,
    ) -> Result<TaskPushNotificationConfig> {
        let response = self
            .client
            .put(self.url(&format!("/tasks/{}/pushNotificationConfig", config.task_id)))
            .json(&config.push_notification_config)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        Ok(response.json().await?)
    }

    async fn get_task_push_notification_config(
        &self,
        params: GetTaskPushNotificationConfigParams,
    ) -> Result<TaskPushNotificationConfig> {
        let mut url = self.url(&format!("/tasks/{}/pushNotificationConfig", params.id));
        if let Some(ref config_id) = params.push_notification_config_id {
            url = format!("{}/{}", url, config_id);
        }

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        Ok(response.json().await?)
    }

    async fn list_task_push_notification_configs(
        &self,
        params: ListTaskPushNotificationConfigParams,
    ) -> Result<Vec<TaskPushNotificationConfig>> {
        let response = self
            .client
            .get(self.url(&format!("/tasks/{}/pushNotificationConfigs", params.id)))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        Ok(response.json().await?)
    }

    async fn delete_task_push_notification_config(
        &self,
        params: DeleteTaskPushNotificationConfigParams,
    ) -> Result<()> {
        let response = self
            .client
            .delete(self.url(&format!(
                "/tasks/{}/pushNotificationConfig/{}",
                params.id, params.push_notification_config_id
            )))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        Ok(())
    }

    async fn resubscribe(
        &self,
        params: TaskResubscriptionParams,
    ) -> Result<EventStream<StreamEvent>> {
        let response = self
            .client
            .get(self.url(&format!("/tasks/{}/stream", params.id)))
            .header(ACCEPT, "text/event-stream")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        Ok(parse_rest_sse_byte_stream(response))
    }

    async fn get_agent_card(&self) -> Result<AgentCard> {
        let response = self.client.get(&self.card_url).send().await?;

        if !response.status().is_success() {
            return Err(A2AError::Http(response.error_for_status().unwrap_err()));
        }

        Ok(response.json().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rest_transport_creation() {
        let transport = RestTransport::from_url("https://example.com").unwrap();
        assert_eq!(transport.base_url, "https://example.com");
        assert_eq!(transport.url("/message"), "https://example.com/message");
    }

    #[test]
    fn test_parse_rest_sse_message() {
        let message = "event: task\ndata: {\"id\":\"t1\",\"context_id\":\"c1\",\"status\":{\"state\":\"working\"},\"kind\":\"task\"}";
        let result = parse_rest_sse_message(message);
        assert!(result.is_some());
    }
}
