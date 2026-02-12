//! gRPC transport implementation for the A2A client.
//!
//! This module provides a gRPC-based transport for communicating with A2A agents.

use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;
use tonic::transport::Channel;

use super::convert::{hashmap_to_struct, struct_to_hashmap};
use super::proto::{
    self, CancelTaskRequest, GetTaskRequest, SendMessageRequest, SubscribeToTaskRequest,
    a2a_service_client::A2aServiceClient,
};
use crate::error::{A2AError, Result};
use crate::types::{
    Message as NativeMessage, MessageSendParams, StreamingEvent, Task as NativeTask, TaskIdParams,
    TaskQueryParams, TaskResubscriptionParams,
};

/// gRPC transport for A2A client operations.
///
/// This transport wraps the generated `A2aServiceClient` and provides
/// a higher-level API that works with native SDK types.
#[derive(Debug, Clone)]
pub struct GrpcTransport {
    client: A2aServiceClient<Channel>,
}

impl GrpcTransport {
    /// Creates a new gRPC transport connected to the given endpoint.
    pub async fn connect(endpoint: impl Into<String>) -> Result<Self> {
        let endpoint = endpoint.into();
        let channel = Channel::from_shared(endpoint)
            .map_err(|e| A2AError::Other(e.to_string()))?
            .connect()
            .await
            .map_err(|e| A2AError::Other(e.to_string()))?;

        Ok(Self {
            client: A2aServiceClient::new(channel),
        })
    }

    /// Creates a new gRPC transport from an existing channel.
    pub fn from_channel(channel: Channel) -> Self {
        Self {
            client: A2aServiceClient::new(channel),
        }
    }

    /// Sends a message to the agent and returns the response.
    pub async fn send_message(&mut self, params: MessageSendParams) -> Result<SendMessageResult> {
        let request = self.build_send_request(params);

        let response = self
            .client
            .send_message(request)
            .await
            .map_err(|e| A2AError::Other(e.to_string()))?;

        let inner = response.into_inner();

        match inner.payload {
            Some(proto::send_message_response::Payload::Task(task)) => {
                Ok(SendMessageResult::Task(NativeTask::from(task)))
            }
            Some(proto::send_message_response::Payload::Message(msg)) => {
                Ok(SendMessageResult::Message(NativeMessage::from(msg)))
            }
            None => Err(A2AError::InternalError("empty response".to_string())),
        }
    }

    /// Sends a streaming message to the agent.
    pub async fn send_streaming_message(
        &mut self,
        params: MessageSendParams,
    ) -> Result<GrpcEventStream> {
        let request = self.build_send_request(params);

        let response = self
            .client
            .send_streaming_message(request)
            .await
            .map_err(|e| A2AError::Other(e.to_string()))?;

        Ok(GrpcEventStream::new(response.into_inner()))
    }

    /// Gets the current state of a task.
    pub async fn get_task(&mut self, params: TaskQueryParams) -> Result<NativeTask> {
        let request = GetTaskRequest {
            tenant: String::new(),
            id: params.id,
            history_length: params.history_length,
        };

        let response = self
            .client
            .get_task(request)
            .await
            .map_err(|e| A2AError::Other(e.to_string()))?;

        Ok(NativeTask::from(response.into_inner()))
    }

    /// Cancels a task.
    pub async fn cancel_task(&mut self, params: TaskIdParams) -> Result<NativeTask> {
        let request = CancelTaskRequest {
            tenant: String::new(),
            id: params.id,
        };

        let response = self
            .client
            .cancel_task(request)
            .await
            .map_err(|e| A2AError::Other(e.to_string()))?;

        Ok(NativeTask::from(response.into_inner()))
    }

    /// Subscribes to task updates.
    pub async fn subscribe_to_task(
        &mut self,
        params: TaskResubscriptionParams,
    ) -> Result<GrpcEventStream> {
        let request = SubscribeToTaskRequest {
            tenant: String::new(),
            id: params.id,
        };

        let response = self
            .client
            .subscribe_to_task(request)
            .await
            .map_err(|e| A2AError::Other(e.to_string()))?;

        Ok(GrpcEventStream::new(response.into_inner()))
    }

    /// Builds a SendMessageRequest from params.
    fn build_send_request(&self, params: MessageSendParams) -> SendMessageRequest {
        let message = proto::Message::from(params.message);

        let configuration = params
            .configuration
            .map(|config| proto::SendMessageConfiguration {
                accepted_output_modes: config.accepted_output_modes.unwrap_or_default(),
                push_notification_config: config
                    .push_notification_config
                    .map(|c| proto::PushNotificationConfig::from(c)),
                history_length: config.history_length.map(|h| h as i32),
                blocking: config.blocking.unwrap_or(false),
            });
        let metadata = params.metadata.and_then(hashmap_to_struct);

        SendMessageRequest {
            tenant: String::new(),
            message: Some(message),
            configuration,
            metadata,
        }
    }
}

/// Result of a send message operation.
#[derive(Debug, Clone)]
pub enum SendMessageResult {
    /// A task was returned.
    Task(NativeTask),
    /// A message was returned.
    Message(NativeMessage),
}

/// Stream of events from gRPC streaming responses.
pub struct GrpcEventStream {
    inner: tonic::Streaming<proto::StreamResponse>,
}

impl GrpcEventStream {
    /// Creates a new event stream.
    fn new(inner: tonic::Streaming<proto::StreamResponse>) -> Self {
        Self { inner }
    }
}

impl Stream for GrpcEventStream {
    type Item = StreamingEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(response))) => {
                let event = convert_stream_response(response);
                Poll::Ready(event)
            }
            Poll::Ready(Some(Err(_))) => Poll::Ready(None),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Converts a proto StreamResponse to a native StreamingEvent.
fn convert_stream_response(response: proto::StreamResponse) -> Option<StreamingEvent> {
    match response.payload {
        Some(proto::stream_response::Payload::StatusUpdate(update)) => Some(
            StreamingEvent::StatusUpdate(crate::types::TaskStatusUpdateEvent {
                task_id: update.task_id,
                context_id: update.context_id,
                kind: "status-update".to_string(),
                status: update
                    .status
                    .map(crate::types::TaskStatus::from)
                    .unwrap_or_else(|| {
                        crate::types::TaskStatus::new(crate::types::TaskState::Unknown)
                    }),
                r#final: false,
                metadata: update.metadata.and_then(struct_to_hashmap),
            }),
        ),
        Some(proto::stream_response::Payload::ArtifactUpdate(update)) => Some(
            StreamingEvent::ArtifactUpdate(crate::types::TaskArtifactUpdateEvent {
                task_id: update.task_id,
                context_id: update.context_id,
                kind: "artifact-update".to_string(),
                artifact: update
                    .artifact
                    .map(crate::types::Artifact::from)
                    .unwrap_or_else(|| crate::types::Artifact::new("", vec![])),
                append: Some(update.append),
                last_chunk: Some(update.last_chunk),
                metadata: update.metadata.and_then(struct_to_hashmap),
            }),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_message_result() {
        let task = NativeTask::new("task-1", "ctx-1");
        let result = SendMessageResult::Task(task);

        match result {
            SendMessageResult::Task(t) => assert_eq!(t.id, "task-1"),
            _ => panic!("expected Task"),
        }
    }
}
