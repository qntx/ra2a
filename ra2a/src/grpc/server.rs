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
    AgentCard, DeleteTaskPushConfigParams, GetTaskPushConfigParams, Message as NativeMessage,
    MessageSendConfig, MessageSendParams, PushConfig as NativePushConfig, TaskIdParams,
    TaskQueryParams,
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
    pub fn with_shared(handler: Arc<H>, agent_card: Arc<AgentCard>) -> Self {
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

        // Build send params
        let mut params = MessageSendParams::new(message);

        // Apply configuration
        if let Some(config) = req.configuration {
            let mut send_config = MessageSendConfig::default();
            send_config.blocking = Some(config.blocking);
            if let Some(history_length) = config.history_length {
                if history_length > 0 {
                    send_config.history_length = Some(history_length);
                }
            }
            send_config.accepted_output_modes = Some(config.accepted_output_modes.clone());
            if let Some(push_config) = config.push_notification_config {
                send_config.push_notification_config = Some(NativePushConfig::from(push_config));
            }
            params.configuration = Some(send_config);
        }

        // Apply metadata
        if let Some(metadata) = req.metadata {
            params.metadata = struct_to_hashmap(metadata);
        }

        // Call handler
        let result = self
            .handler
            .on_message_send(params)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Convert response
        let response = match result {
            crate::server::SendMessageResponse::Task(task) => SendMessageResponse {
                payload: Some(proto::send_message_response::Payload::Task(
                    proto::Task::from(task),
                )),
            },
            crate::server::SendMessageResponse::Message(msg) => SendMessageResponse {
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

        // Build send params
        let mut params = MessageSendParams::new(message);

        // Apply configuration
        if let Some(config) = req.configuration {
            let mut send_config = MessageSendConfig::default();
            send_config.blocking = Some(config.blocking);
            if let Some(history_length) = config.history_length {
                if history_length > 0 {
                    send_config.history_length = Some(history_length);
                }
            }
            if !config.accepted_output_modes.is_empty() {
                send_config.accepted_output_modes = Some(config.accepted_output_modes);
            }
            if let Some(push_config) = config.push_notification_config {
                send_config.push_notification_config = Some(NativePushConfig::from(push_config));
            }
            params.configuration = Some(send_config);
        }

        // Apply metadata
        if let Some(metadata) = req.metadata {
            params.metadata = struct_to_hashmap(metadata);
        }

        // Get streaming response from handler
        let stream = self
            .handler
            .on_message_stream(params)
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

        let mut params = TaskQueryParams::new(&req.id);
        if let Some(history_length) = req.history_length {
            if history_length > 0 {
                params.history_length = Some(history_length);
            }
        }

        let task = self
            .handler
            .on_get_task(params)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(proto::Task::from(task)))
    }

    async fn list_tasks(
        &self,
        _request: Request<ListTasksRequest>,
    ) -> Result<Response<ListTasksResponse>, Status> {
        Err(Status::unimplemented("list_tasks not yet implemented"))
    }

    async fn cancel_task(
        &self,
        request: Request<CancelTaskRequest>,
    ) -> Result<Response<proto::Task>, Status> {
        let req = request.into_inner();

        let params = TaskIdParams::new(&req.id);

        let task = self
            .handler
            .on_cancel_task(params)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(proto::Task::from(task)))
    }

    async fn subscribe_to_task(
        &self,
        request: Request<SubscribeToTaskRequest>,
    ) -> Result<Response<Self::SubscribeToTaskStream>, Status> {
        let req = request.into_inner();

        let params = TaskIdParams::new(&req.id);

        let stream = self
            .handler
            .on_resubscribe(params)
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
            .map(|c| NativePushConfig::from(c))
            .ok_or_else(|| Status::invalid_argument("config is required"))?;

        let params = crate::types::TaskPushConfig {
            task_id: req.task_id.clone(),
            push_notification_config: config,
        };

        let result = self
            .handler
            .on_set_push_notification_config(params)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(TaskPushNotificationConfig {
            tenant: String::new(),
            id: result
                .push_notification_config
                .id
                .clone()
                .unwrap_or_default(),
            task_id: result.task_id,
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

        let params = GetTaskPushConfigParams {
            id: req.task_id.clone(),
            push_notification_config_id: if req.id.is_empty() {
                None
            } else {
                Some(req.id.clone())
            },
            metadata: None,
        };

        let result = self
            .handler
            .on_get_push_notification_config(params)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(TaskPushNotificationConfig {
            tenant: String::new(),
            id: result
                .push_notification_config
                .id
                .clone()
                .unwrap_or_default(),
            task_id: result.task_id,
            push_notification_config: Some(proto::PushNotificationConfig::from(
                result.push_notification_config,
            )),
        }))
    }

    async fn list_task_push_notification_config(
        &self,
        _request: Request<ListTaskPushNotificationConfigRequest>,
    ) -> Result<Response<ListTaskPushNotificationConfigResponse>, Status> {
        Err(Status::unimplemented(
            "list_task_push_notification_config not yet implemented",
        ))
    }

    async fn get_extended_agent_card(
        &self,
        _request: Request<GetExtendedAgentCardRequest>,
    ) -> Result<Response<proto::AgentCard>, Status> {
        // Convert native AgentCard to proto AgentCard
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

        let params = DeleteTaskPushConfigParams {
            id: req.task_id,
            push_notification_config_id: req.id,
            metadata: None,
        };

        self.handler
            .on_delete_push_notification_config(params)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(()))
    }
}

/// Converts a TaskStatusUpdateEvent to a proto StreamResponse.
fn convert_status_update_to_response(
    update: crate::types::TaskStatusUpdateEvent,
) -> StreamResponse {
    StreamResponse {
        payload: Some(proto::stream_response::Payload::StatusUpdate(
            proto::TaskStatusUpdateEvent {
                task_id: update.task_id,
                context_id: update.context_id,
                status: Some(proto::TaskStatus::from(update.status)),
                metadata: update.metadata.and_then(hashmap_to_struct),
            },
        )),
    }
}

/// Converts a TaskArtifactUpdateEvent to a proto StreamResponse.
fn convert_artifact_update_to_response(
    update: crate::types::TaskArtifactUpdateEvent,
) -> StreamResponse {
    StreamResponse {
        payload: Some(proto::stream_response::Payload::ArtifactUpdate(
            proto::TaskArtifactUpdateEvent {
                task_id: update.task_id,
                context_id: update.context_id,
                artifact: Some(proto::Artifact::from(update.artifact)),
                append: update.append.unwrap_or(false),
                last_chunk: update.last_chunk.unwrap_or(false),
                metadata: update.metadata.and_then(hashmap_to_struct),
            },
        )),
    }
}

/// Builder for creating and running a gRPC server.
pub struct GrpcServerBuilder {
    addr: String,
}

impl GrpcServerBuilder {
    /// Creates a new builder with the given address.
    pub fn new(addr: impl Into<String>) -> Self {
        Self { addr: addr.into() }
    }

    /// Builds and runs the gRPC server.
    pub async fn serve<H: RequestHandler + Send + Sync + 'static>(
        self,
        handler: H,
        agent_card: AgentCard,
    ) -> Result<(), tonic::transport::Error> {
        let service = GrpcServiceImpl::new(handler, agent_card);

        tonic::transport::Server::builder()
            .add_service(super::proto::a2a_service_server::A2aServiceServer::new(
                service,
            ))
            .serve(self.addr.parse().expect("invalid address"))
            .await
    }
}

/// Convenience function to start a gRPC server.
pub async fn serve_grpc<H: RequestHandler + Send + Sync + 'static>(
    addr: impl Into<String>,
    handler: H,
    agent_card: AgentCard,
) -> Result<(), tonic::transport::Error> {
    GrpcServerBuilder::new(addr)
        .serve(handler, agent_card)
        .await
}
