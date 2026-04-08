//! gRPC server implementation for the A2A protocol.
//!
//! This module provides a gRPC server that wraps a `RequestHandler` to implement
//! the generated `A2aService` trait.

use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use super::convert::{hashmap_to_struct, none_if_empty, struct_to_hashmap};
use super::proto::{
    self, CancelTaskRequest, DeleteTaskPushNotificationConfigRequest, GetExtendedAgentCardRequest,
    GetTaskPushNotificationConfigRequest, GetTaskRequest, ListTaskPushNotificationConfigsRequest,
    ListTaskPushNotificationConfigsResponse, ListTasksRequest, ListTasksResponse,
    SendMessageRequest, SendMessageResponse, StreamResponse, SubscribeToTaskRequest,
    TaskPushNotificationConfig, a2a_service_server::A2aService,
};
use crate::server::RequestHandler;
use crate::types::{
    AgentCard, CancelTaskRequest as NativeCancelTaskRequest,
    DeleteTaskPushNotificationConfigRequest as NativeDeletePushReq,
    GetTaskPushNotificationConfigRequest as NativeGetPushReq, GetTaskRequest as NativeGetTaskReq,
    ListTaskPushNotificationConfigsRequest as NativeListPushReq,
    ListTasksRequest as NativeListTasksReq, Message as NativeMessage, SendMessageConfiguration,
    SendMessageRequest as NativeSendMessageReq, SendMessageResponse as NativeSendMessageResp,
    SubscribeToTaskRequest as NativeSubscribeReq, TaskId,
    TaskPushNotificationConfig as NativeTaskPushConfig,
};

/// Type alias for streaming response.
type ResponseStream = Pin<Box<dyn Stream<Item = Result<StreamResponse, Status>> + Send>>;

/// gRPC service implementation that wraps a `RequestHandler`.
///
/// This struct implements the generated `A2aService` trait by delegating
/// to the provided `RequestHandler` and converting between proto and native types.
pub struct GrpcServiceImpl<H: RequestHandler> {
    handler: Arc<H>,
    agent_card: Arc<AgentCard>,
}

impl<H: RequestHandler> std::fmt::Debug for GrpcServiceImpl<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GrpcServiceImpl").finish_non_exhaustive()
    }
}

impl<H: RequestHandler> GrpcServiceImpl<H> {
    /// Creates a new gRPC service with the given handler and agent card.
    pub fn new(handler: H, agent_card: AgentCard) -> Self {
        Self {
            handler: Arc::new(handler),
            agent_card: Arc::new(agent_card),
        }
    }

    /// Creates a new gRPC service with shared handler and agent card.
    pub const fn with_shared(handler: Arc<H>, agent_card: Arc<AgentCard>) -> Self {
        Self {
            handler,
            agent_card,
        }
    }
}

#[tonic::async_trait]
impl<H: RequestHandler + Send + Sync + 'static> A2aService for GrpcServiceImpl<H> {
    type SendStreamingMessageStream = ResponseStream;
    type SubscribeToTaskStream = ResponseStream;

    async fn send_message(
        &self,
        request: Request<SendMessageRequest>,
    ) -> Result<Response<SendMessageResponse>, Status> {
        let native_req = convert_send_message_request(request.into_inner())?;

        let result = self
            .handler
            .on_message_send(native_req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let response = match result {
            NativeSendMessageResp::Task(task) => SendMessageResponse {
                payload: Some(proto::send_message_response::Payload::Task(
                    proto::Task::from(task),
                )),
            },
            NativeSendMessageResp::Message(msg) => SendMessageResponse {
                payload: Some(proto::send_message_response::Payload::Message(
                    proto::Message::from(msg),
                )),
            },
        };

        Ok(Response::new(response))
    }

    async fn send_streaming_message(
        &self,
        request: Request<SendMessageRequest>,
    ) -> Result<Response<Self::SendStreamingMessageStream>, Status> {
        let native_req = convert_send_message_request(request.into_inner())?;

        let stream = self
            .handler
            .on_message_stream(native_req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(spawn_event_stream(stream)))
    }

    async fn get_task(
        &self,
        request: Request<GetTaskRequest>,
    ) -> Result<Response<proto::Task>, Status> {
        let req = request.into_inner();

        let native_req = NativeGetTaskReq {
            tenant: none_if_empty(req.tenant),
            id: TaskId::from(req.id.as_str()),
            history_length: req.history_length,
        };

        let task = self
            .handler
            .on_get_task(native_req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(proto::Task::from(task)))
    }

    async fn list_tasks(
        &self,
        request: Request<ListTasksRequest>,
    ) -> Result<Response<ListTasksResponse>, Status> {
        let req = request.into_inner();

        let native_req = NativeListTasksReq {
            tenant: none_if_empty(req.tenant),
            context_id: none_if_empty(req.context_id),
            status: None,
            page_size: req.page_size,
            page_token: none_if_empty(req.page_token),
            history_length: req.history_length,
            status_timestamp_after: None,
            include_artifacts: req.include_artifacts,
        };

        let result = self
            .handler
            .on_list_tasks(native_req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let proto_tasks = result.tasks.into_iter().map(proto::Task::from).collect();

        Ok(Response::new(ListTasksResponse {
            tasks: proto_tasks,
            next_page_token: result.next_page_token,
            page_size: result.page_size,
            total_size: result.total_size,
        }))
    }

    async fn cancel_task(
        &self,
        request: Request<CancelTaskRequest>,
    ) -> Result<Response<proto::Task>, Status> {
        let req = request.into_inner();

        let native_req = NativeCancelTaskRequest {
            tenant: none_if_empty(req.tenant),
            id: TaskId::from(req.id.as_str()),
            metadata: None,
        };

        let task = self
            .handler
            .on_cancel_task(native_req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(proto::Task::from(task)))
    }

    async fn subscribe_to_task(
        &self,
        request: Request<SubscribeToTaskRequest>,
    ) -> Result<Response<Self::SubscribeToTaskStream>, Status> {
        let req = request.into_inner();

        let native_req = NativeSubscribeReq {
            tenant: none_if_empty(req.tenant),
            id: TaskId::from(req.id.as_str()),
        };

        let stream = self
            .handler
            .on_subscribe_to_task(native_req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(spawn_event_stream(stream)))
    }

    async fn create_task_push_notification_config(
        &self,
        request: Request<TaskPushNotificationConfig>,
    ) -> Result<Response<TaskPushNotificationConfig>, Status> {
        let req = request.into_inner();

        let result = self
            .handler
            .on_create_task_push_config(NativeTaskPushConfig::from(req))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(TaskPushNotificationConfig::from(result)))
    }

    async fn get_task_push_notification_config(
        &self,
        request: Request<GetTaskPushNotificationConfigRequest>,
    ) -> Result<Response<TaskPushNotificationConfig>, Status> {
        let req = request.into_inner();

        let native_req = NativeGetPushReq {
            tenant: none_if_empty(req.tenant),
            task_id: TaskId::from(req.task_id.as_str()),
            id: req.id,
        };

        let result = self
            .handler
            .on_get_task_push_config(native_req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(TaskPushNotificationConfig::from(result)))
    }

    async fn list_task_push_notification_configs(
        &self,
        request: Request<ListTaskPushNotificationConfigsRequest>,
    ) -> Result<Response<ListTaskPushNotificationConfigsResponse>, Status> {
        let req = request.into_inner();

        let native_req = NativeListPushReq {
            tenant: none_if_empty(req.tenant),
            task_id: TaskId::from(req.task_id.as_str()),
            page_size: if req.page_size > 0 {
                Some(req.page_size)
            } else {
                None
            },
            page_token: none_if_empty(req.page_token),
        };

        let result = self
            .handler
            .on_list_task_push_configs(native_req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let proto_configs = result
            .configs
            .into_iter()
            .map(TaskPushNotificationConfig::from)
            .collect();

        Ok(Response::new(ListTaskPushNotificationConfigsResponse {
            configs: proto_configs,
            next_page_token: result.next_page_token.unwrap_or_default(),
        }))
    }

    async fn get_extended_agent_card(
        &self,
        _request: Request<GetExtendedAgentCardRequest>,
    ) -> Result<Response<proto::AgentCard>, Status> {
        let card = &self.agent_card;
        Ok(Response::new(proto::AgentCard {
            name: card.name.clone(),
            description: card.description.clone(),
            version: card.version.clone(),
            supported_interfaces: vec![],
            provider: None,
            documentation_url: card.documentation_url.clone(),
            capabilities: None,
            security_schemes: std::collections::HashMap::new(),
            security_requirements: vec![],
            default_input_modes: card.default_input_modes.clone(),
            default_output_modes: card.default_output_modes.clone(),
            skills: vec![],
            signatures: vec![],
            icon_url: card.icon_url.clone(),
        }))
    }

    async fn delete_task_push_notification_config(
        &self,
        request: Request<DeleteTaskPushNotificationConfigRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();

        let native_req = NativeDeletePushReq {
            tenant: none_if_empty(req.tenant),
            task_id: TaskId::from(req.task_id.as_str()),
            id: req.id,
        };

        self.handler
            .on_delete_task_push_config(native_req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(()))
    }
}

/// Converts a proto `SendMessageRequest` to its native equivalent.
fn convert_send_message_request(req: SendMessageRequest) -> Result<NativeSendMessageReq, Status> {
    let message = req
        .message
        .map(NativeMessage::from)
        .ok_or_else(|| Status::invalid_argument("message is required"))?;

    let mut native_req = NativeSendMessageReq::new(message);

    if let Some(config) = req.configuration {
        native_req.configuration = Some(SendMessageConfiguration {
            return_immediately: config.return_immediately,
            accepted_output_modes: config.accepted_output_modes,
            history_length: config.history_length,
            task_push_notification_config: config
                .task_push_notification_config
                .map(NativeTaskPushConfig::from),
        });
    }

    if let Some(metadata) = req.metadata {
        native_req.metadata = Some(struct_to_hashmap(metadata));
    }

    Ok(native_req)
}

/// Spawns a task that converts native events to proto `StreamResponse`s
/// and forwards them through a channel.
fn spawn_event_stream(
    stream: impl Stream<Item = Result<crate::server::Event, crate::error::A2AError>> + Send + 'static,
) -> ResponseStream {
    let (tx, rx) = mpsc::channel(32);

    tokio::spawn(async move {
        let mut stream = std::pin::pin!(stream);
        while let Some(event) = stream.next().await {
            let response = match event {
                Ok(crate::server::Event::StatusUpdate(update)) => {
                    convert_status_update_to_response(update)
                }
                Ok(crate::server::Event::ArtifactUpdate(update)) => {
                    convert_artifact_update_to_response(update)
                }
                Ok(crate::server::Event::Task(_) | crate::server::Event::Message(_)) => {
                    continue;
                }
                Err(_) => break,
            };
            if tx.send(Ok(response)).await.is_err() {
                break;
            }
        }
    });

    Box::pin(ReceiverStream::new(rx))
}

/// Converts a `TaskStatusUpdateEvent` to a proto `StreamResponse`.
fn convert_status_update_to_response(
    update: crate::types::TaskStatusUpdateEvent,
) -> StreamResponse {
    StreamResponse {
        payload: Some(proto::stream_response::Payload::StatusUpdate(
            proto::TaskStatusUpdateEvent {
                task_id: update.task_id.to_string(),
                context_id: update.context_id.to_string(),
                status: Some(proto::TaskStatus::from(update.status)),
                metadata: update.metadata.map(hashmap_to_struct),
            },
        )),
    }
}

fn convert_artifact_update_to_response(
    update: crate::types::TaskArtifactUpdateEvent,
) -> StreamResponse {
    StreamResponse {
        payload: Some(proto::stream_response::Payload::ArtifactUpdate(
            proto::TaskArtifactUpdateEvent {
                task_id: update.task_id.to_string(),
                context_id: update.context_id.to_string(),
                artifact: Some(proto::Artifact::from(update.artifact)),
                append: update.append,
                last_chunk: update.last_chunk,
                metadata: update.metadata.map(hashmap_to_struct),
            },
        )),
    }
}
