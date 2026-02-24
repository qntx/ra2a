//! gRPC transport — implements [`client::Transport`](crate::client::Transport) over gRPC.
//!
//! Provides [`GrpcTransport`] as a first-class alternative to
//! [`JsonRpcTransport`](crate::client::JsonRpcTransport).

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures::Stream;
use tokio::sync::Mutex;
use tonic::transport::Channel;

use super::convert::{hashmap_to_struct, struct_to_hashmap};
use super::proto::{
    self, CancelTaskRequest, GetTaskRequest, SendMessageRequest, SubscribeToTaskRequest,
    a2a_service_client::A2aServiceClient,
};
use crate::client::{EventStream, Transport};
use crate::error::{A2AError, Result};
use crate::types::{
    AgentCard, Artifact, DeleteTaskPushConfigParams, Event, GetTaskPushConfigParams,
    ListTaskPushConfigParams, ListTasksRequest, ListTasksResponse, Message, MessageSendParams,
    SendMessageResult, Task, TaskArtifactUpdateEvent, TaskIdParams, TaskPushConfig,
    TaskQueryParams, TaskState, TaskStatus, TaskStatusUpdateEvent,
};

// ---------------------------------------------------------------------------
// GrpcTransport
// ---------------------------------------------------------------------------

/// gRPC transport for A2A client operations.
///
/// Implements [`Transport`] so it can be used interchangeably with
/// [`JsonRpcTransport`](crate::client::JsonRpcTransport) via [`Client`](crate::client::Client).
///
/// # Example
///
/// ```no_run
/// use ra2a::client::Client;
/// use ra2a::grpc::GrpcTransport;
///
/// # async fn example() -> ra2a::error::Result<()> {
/// let transport = GrpcTransport::connect("http://localhost:50051").await?;
/// let client = Client::new(Box::new(transport));
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct GrpcTransport {
    // Arc<Mutex> because tonic client methods take &mut self
    client: Arc<Mutex<A2aServiceClient<Channel>>>,
}

impl GrpcTransport {
    /// Connects to a gRPC endpoint.
    pub async fn connect(endpoint: impl Into<String>) -> Result<Self> {
        let endpoint = endpoint.into();
        let channel = Channel::from_shared(endpoint)
            .map_err(|e| A2AError::Other(e.to_string()))?
            .connect()
            .await
            .map_err(|e| A2AError::Other(e.to_string()))?;
        Ok(Self::from_channel(channel))
    }

    /// Creates a transport from an existing [`Channel`].
    #[must_use]
    pub fn from_channel(channel: Channel) -> Self {
        Self {
            client: Arc::new(Mutex::new(A2aServiceClient::new(channel))),
        }
    }

    /// Builds a proto `SendMessageRequest` from native params.
    fn build_send_request(params: &MessageSendParams) -> SendMessageRequest {
        let message = proto::Message::from(params.message.clone());
        let configuration =
            params
                .configuration
                .as_ref()
                .map(|config| proto::SendMessageConfiguration {
                    accepted_output_modes: config.accepted_output_modes.clone(),
                    push_notification_config: config
                        .push_notification_config
                        .as_ref()
                        .map(|pc| proto::PushNotificationConfig::from(pc.clone())),
                    history_length: config.history_length,
                    blocking: config.blocking.unwrap_or(false),
                });
        let metadata = if params.metadata.is_empty() {
            None
        } else {
            hashmap_to_struct(params.metadata.clone())
        };

        SendMessageRequest {
            tenant: String::new(),
            message: Some(message),
            configuration,
            metadata,
        }
    }
}

#[async_trait]
impl Transport for GrpcTransport {
    async fn send_message(&self, params: &MessageSendParams) -> Result<SendMessageResult> {
        let request = Self::build_send_request(params);
        let response = self
            .client
            .lock()
            .await
            .send_message(request)
            .await
            .map_err(|e| A2AError::Other(e.to_string()))?;

        match response.into_inner().payload {
            Some(proto::send_message_response::Payload::Task(task)) => {
                Ok(SendMessageResult::Task(Task::from(task)))
            }
            Some(proto::send_message_response::Payload::Message(msg)) => {
                Ok(SendMessageResult::Message(Message::from(msg)))
            }
            None => Err(A2AError::InternalError("empty gRPC response".into())),
        }
    }

    async fn send_message_stream(&self, params: &MessageSendParams) -> Result<EventStream> {
        let request = Self::build_send_request(params);
        let response = self
            .client
            .lock()
            .await
            .send_streaming_message(request)
            .await
            .map_err(|e| A2AError::Other(e.to_string()))?;

        Ok(Box::pin(GrpcEventStream {
            inner: response.into_inner(),
        }))
    }

    async fn get_task(&self, params: &TaskQueryParams) -> Result<Task> {
        let request = GetTaskRequest {
            tenant: String::new(),
            id: params.id.clone(),
            history_length: params.history_length,
        };
        let response = self
            .client
            .lock()
            .await
            .get_task(request)
            .await
            .map_err(|e| A2AError::Other(e.to_string()))?;
        Ok(Task::from(response.into_inner()))
    }

    async fn list_tasks(&self, _params: &ListTasksRequest) -> Result<ListTasksResponse> {
        // gRPC proto does not define a ListTasks RPC yet
        Err(A2AError::UnsupportedOperation(
            "list_tasks not available over gRPC".into(),
        ))
    }

    async fn cancel_task(&self, params: &TaskIdParams) -> Result<Task> {
        let request = CancelTaskRequest {
            tenant: String::new(),
            id: params.id.clone(),
        };
        let response = self
            .client
            .lock()
            .await
            .cancel_task(request)
            .await
            .map_err(|e| A2AError::Other(e.to_string()))?;
        Ok(Task::from(response.into_inner()))
    }

    async fn resubscribe(&self, params: &TaskIdParams) -> Result<EventStream> {
        let request = SubscribeToTaskRequest {
            tenant: String::new(),
            id: params.id.clone(),
        };
        let response = self
            .client
            .lock()
            .await
            .subscribe_to_task(request)
            .await
            .map_err(|e| A2AError::Other(e.to_string()))?;

        Ok(Box::pin(GrpcEventStream {
            inner: response.into_inner(),
        }))
    }

    async fn set_task_push_config(&self, _params: &TaskPushConfig) -> Result<TaskPushConfig> {
        Err(A2AError::UnsupportedOperation(
            "push config not available over gRPC".into(),
        ))
    }

    async fn get_task_push_config(
        &self,
        _params: &GetTaskPushConfigParams,
    ) -> Result<TaskPushConfig> {
        Err(A2AError::UnsupportedOperation(
            "push config not available over gRPC".into(),
        ))
    }

    async fn list_task_push_config(
        &self,
        _params: &ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>> {
        Err(A2AError::UnsupportedOperation(
            "push config not available over gRPC".into(),
        ))
    }

    async fn delete_task_push_config(&self, _params: &DeleteTaskPushConfigParams) -> Result<()> {
        Err(A2AError::UnsupportedOperation(
            "push config not available over gRPC".into(),
        ))
    }

    async fn get_agent_card(&self) -> Result<AgentCard> {
        Err(A2AError::UnsupportedOperation(
            "agent card discovery not available over gRPC".into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// GrpcEventStream — adapts tonic::Streaming to client::EventStream
// ---------------------------------------------------------------------------

/// Adapts a tonic streaming response into a [`Stream`] of [`Event`]s.
struct GrpcEventStream {
    inner: tonic::Streaming<proto::StreamResponse>,
}

impl Stream for GrpcEventStream {
    type Item = Result<Event>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(response))) => match convert_stream_response(response) {
                Some(event) => Poll::Ready(Some(Ok(event))),
                None => {
                    // Skip unknown payload types, poll again
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            },
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(A2AError::Other(e.to_string())))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Converts a proto `StreamResponse` to a native [`Event`].
fn convert_stream_response(response: proto::StreamResponse) -> Option<Event> {
    match response.payload {
        Some(proto::stream_response::Payload::StatusUpdate(update)) => {
            let status = update
                .status
                .map_or_else(|| TaskStatus::new(TaskState::Unknown), TaskStatus::from);
            let mut event =
                TaskStatusUpdateEvent::new(update.task_id, update.context_id, status, false);
            event.metadata = update
                .metadata
                .and_then(struct_to_hashmap)
                .unwrap_or_default();
            Some(Event::StatusUpdate(event))
        }
        Some(proto::stream_response::Payload::ArtifactUpdate(update)) => {
            let artifact = update
                .artifact
                .map_or_else(|| Artifact::new("", vec![]), Artifact::from);
            let mut event =
                TaskArtifactUpdateEvent::new(update.task_id, update.context_id, artifact);
            event.append = update.append;
            event.last_chunk = update.last_chunk;
            event.metadata = update
                .metadata
                .and_then(struct_to_hashmap)
                .unwrap_or_default();
            Some(Event::ArtifactUpdate(event))
        }
        _ => None,
    }
}
