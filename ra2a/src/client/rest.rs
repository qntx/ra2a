//! HTTP+JSON REST transport — implements [`Transport`] over the REST protocol binding.
//!
//! Provides [`RestTransport`] as an alternative to [`JsonRpcTransport`](super::JsonRpcTransport).
//! REST endpoints follow the proto `google.api.http` annotations.
//! Error responses are parsed from google.rpc.Status format (AIP-193).

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use futures::stream;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap};

use super::{EventStream, ServiceParams, Transport};
use crate::error::{A2AError, Result};
use crate::types::{
    AgentCard, CancelTaskRequest, DeleteTaskPushNotificationConfigRequest,
    GetExtendedAgentCardRequest, GetTaskPushNotificationConfigRequest, GetTaskRequest,
    ListTaskPushNotificationConfigsRequest, ListTaskPushNotificationConfigsResponse,
    ListTasksRequest, ListTasksResponse, SendMessageRequest, SendMessageResponse, StreamResponse,
    SubscribeToTaskRequest, Task, TaskPushNotificationConfig,
};

/// HTTP+JSON REST transport for the A2A protocol.
///
/// Maps each A2A operation to the corresponding RESTful HTTP endpoint
/// defined by the proto `google.api.http` annotations.
#[derive(Debug, Clone)]
pub struct RestTransport {
    client: reqwest::Client,
    base_url: String,
    card_url: String,
}

impl RestTransport {
    /// Creates a new REST transport with the given base URL.
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        Self::with_config(base_url, Duration::from_secs(30), HeaderMap::new())
    }

    /// Creates a new REST transport with custom configuration.
    pub fn with_config(
        base_url: impl Into<String>,
        timeout: Duration,
        headers: HeaderMap,
    ) -> Result<Self> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let card_url = crate::agent_card_url(&base_url);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .default_headers(headers)
            .build()
            .map_err(|e| A2AError::Other(e.to_string()))?;
        Ok(Self {
            client,
            base_url,
            card_url,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    async fn post_json<Req: serde::Serialize, Resp: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &Req,
    ) -> Result<Resp> {
        let resp = self
            .client
            .post(self.url(path))
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .json(body)
            .send()
            .await?;
        Self::parse_response(resp).await
    }

    async fn get_json<Resp: serde::de::DeserializeOwned>(&self, url: String) -> Result<Resp> {
        let resp = self
            .client
            .get(url)
            .header(ACCEPT, "application/json")
            .send()
            .await?;
        Self::parse_response(resp).await
    }

    async fn delete_request(&self, url: String) -> Result<()> {
        let resp = self.client.delete(url).send().await?;
        if resp.status().is_success() {
            return Ok(());
        }
        Err(Self::parse_error_response(resp).await)
    }

    async fn parse_response<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
        if resp.status().is_success() {
            let body = resp.text().await?;
            serde_json::from_str(&body).map_err(Into::into)
        } else {
            Err(Self::parse_error_response(resp).await)
        }
    }

    async fn parse_error_response(resp: reqwest::Response) -> A2AError {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();

        // Try to parse google.rpc.Status format
        if let Ok(rest_err) = serde_json::from_str::<serde_json::Value>(&body)
            && let Some(err_obj) = rest_err.get("error")
        {
            let message = err_obj
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            let reason = err_obj
                .get("details")
                .and_then(|d| d.as_array())
                .and_then(|arr| arr.first())
                .and_then(|detail| detail.get("reason"))
                .and_then(|r| r.as_str())
                .unwrap_or("");

            return match reason {
                "TASK_NOT_FOUND" => A2AError::TaskNotFound(message.into()),
                "TASK_NOT_CANCELABLE" => A2AError::TaskNotCancelable(message.into()),
                "PUSH_NOTIFICATION_NOT_SUPPORTED" => A2AError::PushNotificationNotSupported,
                "UNSUPPORTED_OPERATION" => A2AError::UnsupportedOperation(message.into()),
                "UNSUPPORTED_CONTENT_TYPE" => A2AError::ContentTypeNotSupported(message.into()),
                "INVALID_AGENT_RESPONSE" => A2AError::InvalidAgentResponse(message.into()),
                "EXTENSION_SUPPORT_REQUIRED" => A2AError::ExtensionSupportRequired(message.into()),
                "VERSION_NOT_SUPPORTED" => A2AError::VersionNotSupported(message.into()),
                _ => A2AError::Other(format!("REST error {status}: {message}")),
            };
        }
        A2AError::Other(format!("REST error {status}: {body}"))
    }

    async fn sse_stream(&self, url: String) -> Result<EventStream> {
        let resp = self
            .client
            .get(url)
            .header(ACCEPT, "text/event-stream")
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Self::parse_error_response(resp).await);
        }

        Ok(parse_rest_sse_stream(resp))
    }

    async fn post_sse_stream<Req: serde::Serialize>(
        &self,
        path: &str,
        body: &Req,
    ) -> Result<EventStream> {
        let resp = self
            .client
            .post(self.url(path))
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "text/event-stream")
            .json(body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Self::parse_error_response(resp).await);
        }

        Ok(parse_rest_sse_stream(resp))
    }
}

impl Transport for RestTransport {
    fn send_message<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SendMessageResponse>> + Send + 'a>> {
        Box::pin(async move { self.post_json("/message:send", req).await })
    }

    fn send_streaming_message<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + 'a>> {
        Box::pin(async move { self.post_sse_stream("/message:stream", req).await })
    }

    fn get_task<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a GetTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + 'a>> {
        Box::pin(async move {
            let mut url = self.url(&format!("/tasks/{}", req.id));
            if let Some(hl) = req.history_length {
                url.push_str(&format!("?historyLength={hl}"));
            }
            self.get_json(url).await
        })
    }

    fn list_tasks<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a ListTasksRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTasksResponse>> + Send + 'a>> {
        Box::pin(async move {
            let mut parts = Vec::new();
            if let Some(ref cid) = req.context_id {
                parts.push(format!("contextId={cid}"));
            }
            if let Some(ref s) = req.status {
                parts.push(format!("status={s}"));
            }
            if let Some(ps) = req.page_size {
                parts.push(format!("pageSize={ps}"));
            }
            if let Some(ref pt) = req.page_token {
                parts.push(format!("pageToken={pt}"));
            }
            if let Some(hl) = req.history_length {
                parts.push(format!("historyLength={hl}"));
            }
            if let Some(ref ts) = req.status_timestamp_after {
                parts.push(format!("statusTimestampAfter={ts}"));
            }
            if let Some(ia) = req.include_artifacts {
                parts.push(format!("includeArtifacts={ia}"));
            }
            let mut url = self.url("/tasks");
            if !parts.is_empty() {
                url.push('?');
                url.push_str(&parts.join("&"));
            }
            self.get_json(url).await
        })
    }

    fn cancel_task<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a CancelTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + 'a>> {
        Box::pin(async move {
            self.post_json(&format!("/tasks/{}:cancel", req.id), req)
                .await
        })
    }

    fn subscribe_to_task<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a SubscribeToTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + 'a>> {
        Box::pin(async move {
            let url = self.url(&format!("/tasks/{}:subscribe", req.id));
            self.sse_stream(url).await
        })
    }

    fn create_task_push_config<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a TaskPushNotificationConfig,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + 'a>> {
        Box::pin(async move {
            let task_id = req
                .task_id
                .as_ref()
                .map(|t| t.to_string())
                .unwrap_or_default();
            self.post_json(&format!("/tasks/{task_id}/pushNotificationConfigs"), req)
                .await
        })
    }

    fn get_task_push_config<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a GetTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + 'a>> {
        Box::pin(async move {
            let url = self.url(&format!(
                "/tasks/{}/pushNotificationConfigs/{}",
                req.task_id, req.id
            ));
            self.get_json(url).await
        })
    }

    fn list_task_push_configs<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a ListTaskPushNotificationConfigsRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTaskPushNotificationConfigsResponse>> + Send + 'a>>
    {
        Box::pin(async move {
            let url = self.url(&format!("/tasks/{}/pushNotificationConfigs", req.task_id));
            self.get_json(url).await
        })
    }

    fn delete_task_push_config<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a DeleteTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let url = self.url(&format!(
                "/tasks/{}/pushNotificationConfigs/{}",
                req.task_id, req.id
            ));
            self.delete_request(url).await
        })
    }

    fn get_extended_agent_card<'a>(
        &'a self,
        _params: &'a ServiceParams,
        _req: &'a GetExtendedAgentCardRequest,
    ) -> Pin<Box<dyn Future<Output = Result<AgentCard>> + Send + 'a>> {
        Box::pin(async move {
            let url = self.url("/extendedAgentCard");
            self.get_json(url).await
        })
    }

    fn get_agent_card(&self) -> Pin<Box<dyn Future<Output = Result<AgentCard>> + Send + '_>> {
        Box::pin(async move { self.get_json(self.card_url.clone()).await })
    }
}

/// Parses an HTTP response as an SSE stream of raw [`StreamResponse`]s.
///
/// Unlike JSON-RPC SSE, REST SSE events contain raw `StreamResponse` JSON
/// (no JSON-RPC envelope).
fn parse_rest_sse_stream(response: reqwest::Response) -> EventStream {
    let byte_stream = response.bytes_stream();
    let stream = stream::unfold(
        (byte_stream, String::new()),
        |(mut byte_stream, mut buffer)| async move {
            use futures::TryStreamExt;
            loop {
                if let Some(pos) = buffer.find("\n\n") {
                    let event_text = buffer[..pos].to_string();
                    buffer = buffer[pos + 2..].to_string();

                    let data = event_text
                        .lines()
                        .filter_map(|line| {
                            line.strip_prefix("data: ").or(line.strip_prefix("data:"))
                        })
                        .collect::<Vec<_>>()
                        .join("\n");

                    if data.is_empty() {
                        continue;
                    }

                    let result: Result<StreamResponse> =
                        serde_json::from_str(&data).map_err(|e| A2AError::Other(e.to_string()));
                    return Some((result, (byte_stream, buffer)));
                }

                match byte_stream.try_next().await {
                    Ok(Some(bytes)) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                    }
                    Ok(None) => return None,
                    Err(e) => {
                        return Some((
                            Err(A2AError::Other(format!("SSE stream error: {e}"))),
                            (byte_stream, buffer),
                        ));
                    }
                }
            }
        },
    );

    Box::pin(stream)
}
