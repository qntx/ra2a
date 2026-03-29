//! gRPC server implementation for the A2A protocol.
//!
//! This module provides a gRPC server that wraps a `RequestHandler` to implement
//! the generated `A2aService` trait.

use std::pin::Pin;
use std::sync::Arc;

use futures::Stream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use super::convert::{hashmap_to_struct, struct_to_hashmap};
use super::proto::{
    self, CancelTaskRequest, CreateTaskPushNotificationConfigRequest,
    DeleteTaskPushNotificationConfigRequest, GetExtendedAgentCardRequest,
    GetTaskPushNotificationConfigRequest, GetTaskRequest, ListTaskPushNotificationConfigRequest,
    ListTaskPushNotificationConfigResponse, ListTasksRequest, ListTasksResponse,
    SendMessageRequest, SendMessageResponse, StreamResponse, SubscribeToTaskRequest,
    TaskPushNotificationConfig, a2a_service_server::A2aService,
};
use crate::server::RequestHandler;
use crate::types::{
    AgentCard, CancelTaskRequest as NativeCancelTaskRequest,
    CreateTaskPushNotificationConfigRequest as NativeCreatePushReq,
    DeleteTaskPushNotificationConfigRequest as NativeDeletePushReq,
    GetTaskPushNotificationConfigRequest as NativeGetPushReq, GetTaskRequest as NativeGetTaskReq,
    ListTaskPushNotificationConfigRequest as NativeListPushReq,
    ListTasksRequest as NativeListTasksReq, Message as NativeMessage,
    PushNotificationConfig as NativePushConfig, SendMessageConfiguration,
    SendMessageRequest as NativeSendMessageReq, SendMessageResponse as NativeSendMessageResp,
    SubscribeToTaskRequest as NativeSubscribeReq, TaskId,
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
        let req = request.into_inner();

        // Convert proto message to native type
        let message = req
            .message
            .map(NativeMessage::from)
            .ok_or_else(|| Status::invalid_argument("message is required"))?;

        let mut native_req = NativeSendMessageReq::new(message);

        if let Some(config) = req.configuration {
            let mut send_config = SendMessageConfiguration {
                blocking: config.blocking,
                accepted_output_modes: config.accepted_output_modes,
                ..Default::default()
            };
            if let Some(history_length) = config.history_length {
                send_config.history_length = Some(history_length);
            }
            if let Some(push_config) = config.push_notification_config {
                send_config.push_notification_config = Some(NativePushConfig::from(push_config));
            }
            native_req.configuration = Some(send_config);
        }

        if let Some(metadata) = req.metadata {
            native_req.metadata = struct_to_hashmap(metadata);
        }

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
        let req = request.into_inner();

        // Convert proto message to native type
        let message = req
            .message
            .map(NativeMessage::from)
            .ok_or_else(|| Status::invalid_argument("message is required"))?;

        let mut native_req = NativeSendMessageReq::new(message);

        if let Some(config) = req.configuration {
            let mut send_config = SendMessageConfiguration {
                blocking: config.blocking,
                accepted_output_modes: config.accepted_output_modes,
                ..Default::default()
            };
            if let Some(history_length) = config.history_length {
                send_config.history_length = Some(history_length);
            }
            if let Some(push_config) = config.push_notification_config {
                send_config.push_notification_config = Some(NativePushConfig::from(push_config));
            }
            native_req.configuration = Some(send_config);
        }

        if let Some(metadata) = req.metadata {
            native_req.metadata = struct_to_hashmap(metadata);
        }

        let stream = self
            .handler
            .on_message_stream(native_req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Create channel for streaming
        let (tx, rx) = mpsc::channel(32);

        // Spawn task to convert and forward events
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = stream;

            while let Some(event) = stream.next().await {
                let response = match event {
                    Ok(crate::server::Event::StatusUpdate(update)) => {
                        convert_status_update_to_response(update)
                    }
                    Ok(crate::server::Event::ArtifactUpdate(update)) => {
                        convert_artifact_update_to_response(update)
                    }
                    Ok(crate::server::Event::Task(_)) => continue,
                    Ok(crate::server::Event::Message(_)) => continue,
                    Err(_) => break,
                };
                if tx.send(Ok(response)).await.is_err() {
                    break;
                }
            }
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream) as ResponseStream))
    }

    async fn get_task(
        &self,
        request: Request<GetTaskRequest>,
    ) -> Result<Response<proto::Task>, Status> {
        let req = request.into_inner();

        let native_req = NativeGetTaskReq {
            tenant: if req.tenant.is_empty() {
                None
            } else {
                Some(req.tenant)
            },
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
            tenant: if req.tenant.is_empty() {
                None
            } else {
                Some(req.tenant)
            },
            context_id: if req.context_id.is_empty() {
                None
            } else {
                Some(req.context_id)
            },
            status: None,
            page_size: req.page_size,
            page_token: if req.page_token.is_empty() {
                None
            } else {
                Some(req.page_token)
            },
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
            tenant: if req.tenant.is_empty() {
                None
            } else {
                Some(req.tenant)
            },
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
            tenant: if req.tenant.is_empty() {
                None
            } else {
                Some(req.tenant)
            },
            id: TaskId::from(req.id.as_str()),
        };

        let stream = self
            .handler
            .on_subscribe_to_task(native_req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Create channel for streaming
        let (tx, rx) = mpsc::channel(32);

        // Spawn task to convert and forward events
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = stream;

            while let Some(event) = stream.next().await {
                let response = match event {
                    Ok(crate::server::Event::StatusUpdate(update)) => {
                        convert_status_update_to_response(update)
                    }
                    Ok(crate::server::Event::ArtifactUpdate(update)) => {
                        convert_artifact_update_to_response(update)
                    }
                    Ok(crate::server::Event::Task(_)) => continue,
                    Ok(crate::server::Event::Message(_)) => continue,
                    Err(_) => break,
                };
                if tx.send(Ok(response)).await.is_err() {
                    break;
                }
            }
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream) as ResponseStream))
    }

    async fn create_task_push_notification_config(
        &self,
        request: Request<CreateTaskPushNotificationConfigRequest>,
    ) -> Result<Response<TaskPushNotificationConfig>, Status> {
        let req = request.into_inner();

        let config = req
            .config
            .map(NativePushConfig::from)
            .ok_or_else(|| Status::invalid_argument("config is required"))?;

        let native_req = NativeCreatePushReq {
            tenant: if req.tenant.is_empty() {
                None
            } else {
                Some(req.tenant)
            },
            task_id: TaskId::from(req.task_id.as_str()),
            config_id: req.config_id,
            config,
        };

        let result = self
            .handler
            .on_create_task_push_config(native_req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(TaskPushNotificationConfig {
            tenant: String::new(),
            id: result.id,
            task_id: result.task_id.to_string(),
            push_notification_config: Some(proto::PushNotificationConfig::from(
                result.push_notification_config,
            )),
        }))
    }

    async fn get_task_push_notification_config(
        &self,
        request: Request<GetTaskPushNotificationConfigRequest>,
    ) -> Result<Response<TaskPushNotificationConfig>, Status> {
        let req = request.into_inner();

        let native_req = NativeGetPushReq {
            tenant: if req.tenant.is_empty() {
                None
            } else {
                Some(req.tenant)
            },
            task_id: TaskId::from(req.task_id.as_str()),
            id: req.id,
        };

        let result = self
            .handler
            .on_get_task_push_config(native_req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(TaskPushNotificationConfig {
            tenant: String::new(),
            id: result.id,
            task_id: result.task_id.to_string(),
            push_notification_config: Some(proto::PushNotificationConfig::from(
                result.push_notification_config,
            )),
        }))
    }

    async fn list_task_push_notification_config(
        &self,
        request: Request<ListTaskPushNotificationConfigRequest>,
    ) -> Result<Response<ListTaskPushNotificationConfigResponse>, Status> {
        let req = request.into_inner();

        let native_req = NativeListPushReq {
            tenant: if req.tenant.is_empty() {
                None
            } else {
                Some(req.tenant)
            },
            task_id: TaskId::from(req.task_id.as_str()),
            page_size: if req.page_size > 0 {
                Some(req.page_size)
            } else {
                None
            },
            page_token: if req.page_token.is_empty() {
                None
            } else {
                Some(req.page_token)
            },
        };

        let result = self
            .handler
            .on_list_task_push_configs(native_req)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let proto_configs = result
            .configs
            .into_iter()
            .map(|c| TaskPushNotificationConfig {
                tenant: String::new(),
                id: c.id,
                task_id: c.task_id.to_string(),
                push_notification_config: Some(proto::PushNotificationConfig::from(
                    c.push_notification_config,
                )),
            })
            .collect();

        Ok(Response::new(ListTaskPushNotificationConfigResponse {
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
            tenant: if req.tenant.is_empty() {
                None
            } else {
                Some(req.tenant)
            },
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
                metadata: update.metadata.and_then(hashmap_to_struct),
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
                metadata: update.metadata.and_then(hashmap_to_struct),
            },
        )),
    }
}
