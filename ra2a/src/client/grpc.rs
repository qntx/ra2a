//! gRPC transport — implements [`Transport`] over gRPC.
//!
//! Provides [`GrpcTransport`] as a first-class alternative to
//! [`JsonRpcTransport`](super::JsonRpcTransport). Lives in the `client` module
//! alongside other transport implementations.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::Stream;
use tokio::sync::Mutex;
use tonic::transport::Channel;

use super::{EventStream, ServiceParams, Transport};
use crate::error::{A2AError, Result};
use crate::grpc::convert::{hashmap_to_struct, struct_to_hashmap};
use crate::grpc::proto::{self, a2a_service_client::A2aServiceClient};
use crate::types::{
    AgentCard, Artifact, CancelTaskRequest, DeleteTaskPushNotificationConfigRequest,
    GetExtendedAgentCardRequest, GetTaskPushNotificationConfigRequest, GetTaskRequest,
    ListTaskPushNotificationConfigsRequest, ListTaskPushNotificationConfigsResponse,
    ListTasksRequest, ListTasksResponse, Message, SendMessageRequest, SendMessageResponse,
    StreamResponse, SubscribeToTaskRequest, Task, TaskArtifactUpdateEvent,
    TaskPushNotificationConfig, TaskState, TaskStatus, TaskStatusUpdateEvent,
};

/// gRPC transport for A2A client operations.
///
/// Implements [`Transport`] so it can be used interchangeably with
/// [`JsonRpcTransport`](super::JsonRpcTransport) via [`Client`](super::Client).
///
/// # Example
///
/// ```no_run
/// use ra2a::client::{Client, GrpcTransport};
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

    /// Builds a proto `SendMessageRequest` from native request.
    fn build_send_request(req: &SendMessageRequest) -> proto::SendMessageRequest {
        let message = proto::Message::from(req.message.clone());
        let configuration =
            req.configuration
                .as_ref()
                .map(|config| proto::SendMessageConfiguration {
                    accepted_output_modes: config.accepted_output_modes.clone(),
                    task_push_notification_config: config
                        .task_push_notification_config
                        .as_ref()
                        .map(|tpc| proto::TaskPushNotificationConfig {
                            tenant: tpc.tenant.clone().unwrap_or_default(),
                            id: tpc.id.clone().unwrap_or_default(),
                            task_id: tpc
                                .task_id
                                .as_ref()
                                .map(ToString::to_string)
                                .unwrap_or_default(),
                            url: tpc.url.clone(),
                            token: tpc.token.clone().unwrap_or_default(),
                            authentication: tpc.authentication.as_ref().map(|a| {
                                proto::AuthenticationInfo {
                                    scheme: a.scheme.clone(),
                                    credentials: a.credentials.clone().unwrap_or_default(),
                                }
                            }),
                        }),
                    history_length: config.history_length,
                    return_immediately: config.return_immediately,
                });
        let metadata = req.metadata.clone().and_then(hashmap_to_struct);

        proto::SendMessageRequest {
            tenant: req.tenant.clone().unwrap_or_default(),
            message: Some(message),
            configuration,
            metadata,
        }
    }
}

impl Transport for GrpcTransport {
    fn send_message<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SendMessageResponse>> + Send + 'a>> {
        Box::pin(async move {
            let request = Self::build_send_request(req);
            let response = self
                .client
                .lock()
                .await
                .send_message(request)
                .await
                .map_err(|e| A2AError::Other(e.to_string()))?;

            match response.into_inner().payload {
                Some(proto::send_message_response::Payload::Task(task)) => {
                    Ok(SendMessageResponse::Task(Task::from(task)))
                }
                Some(proto::send_message_response::Payload::Message(msg)) => {
                    Ok(SendMessageResponse::Message(Message::from(msg)))
                }
                None => Err(A2AError::InternalError("empty gRPC response".into())),
            }
        })
    }

    fn send_streaming_message<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + 'a>> {
        Box::pin(async move {
            let request = Self::build_send_request(req);
            let response = self
                .client
                .lock()
                .await
                .send_streaming_message(request)
                .await
                .map_err(|e| A2AError::Other(e.to_string()))?;

            Ok(Box::pin(GrpcEventStream {
                inner: response.into_inner(),
            }) as EventStream)
        })
    }

    fn get_task<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a GetTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + 'a>> {
        Box::pin(async move {
            let request = proto::GetTaskRequest {
                tenant: req.tenant.clone().unwrap_or_default(),
                id: req.id.to_string(),
                history_length: req.history_length,
            };
            let response = self
                .client
                .lock()
                .await
                .get_task(request)
                .await
                .map_err(|e| A2AError::Other(e.to_string()))?;
            Ok(Task::from(response.into_inner()))
        })
    }

    fn list_tasks<'a>(
        &'a self,
        _params: &'a ServiceParams,
        _req: &'a ListTasksRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTasksResponse>> + Send + 'a>> {
        Box::pin(async move {
            Err(A2AError::UnsupportedOperation(
                "list_tasks not yet available over gRPC".into(),
            ))
        })
    }

    fn cancel_task<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a CancelTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + 'a>> {
        Box::pin(async move {
            let request = proto::CancelTaskRequest {
                tenant: req.tenant.clone().unwrap_or_default(),
                id: req.id.to_string(),
                metadata: req.metadata.clone().and_then(hashmap_to_struct),
            };
            let response = self
                .client
                .lock()
                .await
                .cancel_task(request)
                .await
                .map_err(|e| A2AError::Other(e.to_string()))?;
            Ok(Task::from(response.into_inner()))
        })
    }

    fn subscribe_to_task<'a>(
        &'a self,
        _params: &'a ServiceParams,
        req: &'a SubscribeToTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + 'a>> {
        Box::pin(async move {
            let request = proto::SubscribeToTaskRequest {
                tenant: req.tenant.clone().unwrap_or_default(),
                id: req.id.to_string(),
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
            }) as EventStream)
        })
    }

    fn create_task_push_config<'a>(
        &'a self,
        _params: &'a ServiceParams,
        _req: &'a TaskPushNotificationConfig,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + 'a>> {
        Box::pin(async move {
            Err(A2AError::UnsupportedOperation(
                "push config not yet available over gRPC client".into(),
            ))
        })
    }

    fn get_task_push_config<'a>(
        &'a self,
        _params: &'a ServiceParams,
        _req: &'a GetTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + 'a>> {
        Box::pin(async move {
            Err(A2AError::UnsupportedOperation(
                "push config not yet available over gRPC client".into(),
            ))
        })
    }

    fn list_task_push_configs<'a>(
        &'a self,
        _params: &'a ServiceParams,
        _req: &'a ListTaskPushNotificationConfigsRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTaskPushNotificationConfigsResponse>> + Send + 'a>>
    {
        Box::pin(async move {
            Err(A2AError::UnsupportedOperation(
                "push config not yet available over gRPC client".into(),
            ))
        })
    }

    fn delete_task_push_config<'a>(
        &'a self,
        _params: &'a ServiceParams,
        _req: &'a DeleteTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            Err(A2AError::UnsupportedOperation(
                "push config not yet available over gRPC client".into(),
            ))
        })
    }

    fn get_extended_agent_card<'a>(
        &'a self,
        _params: &'a ServiceParams,
        _req: &'a GetExtendedAgentCardRequest,
    ) -> Pin<Box<dyn Future<Output = Result<AgentCard>> + Send + 'a>> {
        Box::pin(async move {
            Err(A2AError::UnsupportedOperation(
                "extended agent card not yet available over gRPC client".into(),
            ))
        })
    }

    fn get_agent_card(&self) -> Pin<Box<dyn Future<Output = Result<AgentCard>> + Send + '_>> {
        Box::pin(async move {
            Err(A2AError::UnsupportedOperation(
                "agent card discovery not available over gRPC".into(),
            ))
        })
    }
}

/// Adapts a tonic streaming response into a [`Stream`] of [`StreamResponse`]s.
struct GrpcEventStream {
    inner: tonic::Streaming<proto::StreamResponse>,
}

impl Stream for GrpcEventStream {
    type Item = Result<StreamResponse>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(response))) => if let Some(event) = convert_stream_response(response) { Poll::Ready(Some(Ok(event))) } else {
                cx.waker().wake_by_ref();
                Poll::Pending
            },
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(A2AError::Other(e.to_string())))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

fn convert_stream_response(response: proto::StreamResponse) -> Option<StreamResponse> {
    match response.payload {
        Some(proto::stream_response::Payload::StatusUpdate(update)) => {
            let status = update
                .status
                .map_or_else(|| TaskStatus::new(TaskState::Unspecified), TaskStatus::from);
            let mut event = TaskStatusUpdateEvent::new(update.task_id, update.context_id, status);
            event.metadata = update.metadata.and_then(struct_to_hashmap);
            Some(StreamResponse::StatusUpdate(event))
        }
        Some(proto::stream_response::Payload::ArtifactUpdate(update)) => {
            let artifact = update
                .artifact
                .map_or_else(|| Artifact::create(vec![]), Artifact::from);
            let mut event =
                TaskArtifactUpdateEvent::new(update.task_id, update.context_id, artifact);
            event.append = update.append;
            event.last_chunk = update.last_chunk;
            event.metadata = update.metadata.and_then(struct_to_hashmap);
            Some(StreamResponse::ArtifactUpdate(event))
        }
        _ => None,
    }
}
