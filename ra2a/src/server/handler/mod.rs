//! Request handler trait, implementations, and JSON-RPC dispatcher.
//!
//! - [`RequestHandler`] — trait for handling all A2A JSON-RPC methods
//! - [`DefaultRequestHandler`] — standard implementation coordinating executor, stores, queues
//! - [`InterceptedHandler`] — decorator applying [`CallInterceptor`](super::CallInterceptor)s

mod default;
mod intercepted;

use std::future::Future;
use std::pin::Pin;

pub use default::DefaultRequestHandler;
use futures::Stream;
pub use intercepted::InterceptedHandler;

use super::ServerState;
use super::event::Event;
use crate::error::{JsonRpcError, Result};
use crate::jsonrpc::{self, JsonRpcErrorResponse, JsonRpcRequest, JsonRpcSuccessResponse};
use crate::types::{
    AgentCard, CancelTaskRequest, DeleteTaskPushNotificationConfigRequest,
    GetExtendedAgentCardRequest, GetTaskPushNotificationConfigRequest, GetTaskRequest,
    ListTaskPushNotificationConfigsRequest, ListTaskPushNotificationConfigsResponse,
    ListTasksRequest, ListTasksResponse, SendMessageRequest, SendMessageResponse,
    SubscribeToTaskRequest, Task, TaskPushNotificationConfig,
};

/// A boxed stream of events for streaming responses.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<Event>> + Send>>;

/// Trait defining the interface for handling A2A protocol requests.
pub trait RequestHandler: Send + Sync {
    /// Handles the `message/send` request (non-streaming).
    fn on_message_send(
        &self,
        req: SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SendMessageResponse>> + Send + '_>>;

    /// Handles the `message/stream` request (streaming).
    fn on_message_stream(
        &self,
        req: SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + '_>>;

    /// Handles the `tasks/get` request.
    fn on_get_task(
        &self,
        req: GetTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + '_>>;

    /// Handles the `tasks/list` request.
    fn on_list_tasks(
        &self,
        req: ListTasksRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTasksResponse>> + Send + '_>> {
        let _ = req;
        Box::pin(async { Ok(ListTasksResponse::default()) })
    }

    /// Handles the `tasks/cancel` request.
    fn on_cancel_task(
        &self,
        req: CancelTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + '_>>;

    /// Handles the `tasks/subscribe` request (streaming).
    fn on_subscribe_to_task(
        &self,
        req: SubscribeToTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + '_>>;

    /// Creates a push notification config.
    fn on_create_task_push_config(
        &self,
        _req: TaskPushNotificationConfig,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + '_>> {
        Box::pin(async { Err(crate::error::A2AError::PushNotificationNotSupported) })
    }

    /// Gets a push notification config.
    fn on_get_task_push_config(
        &self,
        _req: GetTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + '_>> {
        Box::pin(async { Err(crate::error::A2AError::PushNotificationNotSupported) })
    }

    /// Lists push notification configs.
    fn on_list_task_push_configs(
        &self,
        _req: ListTaskPushNotificationConfigsRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTaskPushNotificationConfigsResponse>> + Send + '_>>
    {
        Box::pin(async { Err(crate::error::A2AError::PushNotificationNotSupported) })
    }

    /// Deletes a push notification config.
    fn on_delete_task_push_config(
        &self,
        _req: DeleteTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async { Err(crate::error::A2AError::PushNotificationNotSupported) })
    }

    /// Returns the authenticated extended agent card.
    fn on_get_extended_agent_card(
        &self,
        _req: GetExtendedAgentCardRequest,
    ) -> Pin<Box<dyn Future<Output = Result<AgentCard>> + Send + '_>> {
        Box::pin(async { Err(crate::error::A2AError::ExtendedCardNotConfigured) })
    }
}

/// Dispatches a raw JSON-RPC request body to the appropriate
/// [`RequestHandler`](super::RequestHandler) method and returns the serialized response.
pub async fn handle_request(state: &ServerState, request_body: &str) -> Result<String> {
    // Parse the request envelope
    let request: JsonRpcRequest<serde_json::Value> =
        if let Ok(req) = serde_json::from_str(request_body) {
            req
        } else {
            let error_response = JsonRpcErrorResponse::new(None, JsonRpcError::parse_error());
            return Ok(serde_json::to_string(&error_response)?);
        };

    let id = request.id.clone();
    let handler = &state.handler;

    // Dispatch to the appropriate handler method
    let result: Result<String> = match request.method.as_str() {
        jsonrpc::METHOD_MESSAGE_SEND => {
            let req = parse_params::<SendMessageRequest>(&request)?;
            let resp = handler.on_message_send(req).await?;
            serialize_success(&id, &resp)
        }
        jsonrpc::METHOD_TASKS_GET => {
            let req = parse_params::<GetTaskRequest>(&request)?;
            let task = handler.on_get_task(req).await?;
            serialize_success(&id, &task)
        }
        jsonrpc::METHOD_TASKS_CANCEL => {
            let req = parse_params::<CancelTaskRequest>(&request)?;
            let task = handler.on_cancel_task(req).await?;
            serialize_success(&id, &task)
        }
        jsonrpc::METHOD_MESSAGE_STREAM | jsonrpc::METHOD_TASKS_RESUBSCRIBE => Err(
            JsonRpcError::invalid_request("Streaming methods must be called via the SSE endpoint")
                .into(),
        ),
        jsonrpc::METHOD_TASKS_LIST => {
            let req = parse_params::<ListTasksRequest>(&request)?;
            let resp = handler.on_list_tasks(req).await?;
            serialize_success(&id, &resp)
        }
        jsonrpc::METHOD_PUSH_CONFIG_SET => {
            let req = parse_params::<TaskPushNotificationConfig>(&request)?;
            let config = handler.on_create_task_push_config(req).await?;
            serialize_success(&id, &config)
        }
        jsonrpc::METHOD_PUSH_CONFIG_GET => {
            let req = parse_params::<GetTaskPushNotificationConfigRequest>(&request)?;
            let config = handler.on_get_task_push_config(req).await?;
            serialize_success(&id, &config)
        }
        jsonrpc::METHOD_PUSH_CONFIG_LIST => {
            let req = parse_params::<ListTaskPushNotificationConfigsRequest>(&request)?;
            let configs = handler.on_list_task_push_configs(req).await?;
            serialize_success(&id, &configs)
        }
        jsonrpc::METHOD_PUSH_CONFIG_DELETE => {
            let req = parse_params::<DeleteTaskPushNotificationConfigRequest>(&request)?;
            handler.on_delete_task_push_config(req).await?;
            serialize_success(&id, &serde_json::Value::Null)
        }
        jsonrpc::METHOD_GET_EXTENDED_AGENT_CARD => {
            let req = parse_params::<GetExtendedAgentCardRequest>(&request)?;
            let card = handler.on_get_extended_agent_card(req).await?;
            serialize_success(&id, &card)
        }
        _ => Err(JsonRpcError::method_not_found(&request.method).into()),
    };

    match result {
        Ok(json) => Ok(json),
        Err(e) => {
            let rpc_err = e.to_jsonrpc_error();
            Ok(serde_json::to_string(&JsonRpcErrorResponse::new(
                Some(id),
                rpc_err,
            ))?)
        }
    }
}

/// Extracts and deserializes params from a JSON-RPC request.
pub(crate) fn parse_params<T: serde::de::DeserializeOwned>(
    request: &JsonRpcRequest<serde_json::Value>,
) -> Result<T> {
    match &request.params {
        Some(p) => Ok(serde_json::from_value(p.clone())?),
        None => Err(JsonRpcError::invalid_params("Missing params").into()),
    }
}

/// Serializes a success response.
fn serialize_success<T: serde::Serialize>(
    id: &jsonrpc::RequestId,
    result: &T,
) -> Result<String> {
    let response = JsonRpcSuccessResponse::new(Some(id.clone()), result);
    Ok(serde_json::to_string(&response)?)
}
