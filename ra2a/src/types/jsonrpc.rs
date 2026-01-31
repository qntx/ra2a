//! JSON-RPC 2.0 types for the A2A protocol.
//!
//! Defines the request and response structures for JSON-RPC communication.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{AgentCard, Message, Task, TaskArtifactUpdateEvent, TaskStatusUpdateEvent};
use crate::error::JsonRpcError;

/// The JSON-RPC protocol version.
pub const JSONRPC_VERSION: &str = "2.0";

/// A unique identifier for a JSON-RPC request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// String identifier.
    String(String),
    /// Numeric identifier.
    Number(i64),
}

impl From<String> for RequestId {
    fn from(s: String) -> Self {
        RequestId::String(s)
    }
}

impl From<&str> for RequestId {
    fn from(s: &str) -> Self {
        RequestId::String(s.to_string())
    }
}

impl From<i64> for RequestId {
    fn from(n: i64) -> Self {
        RequestId::Number(n)
    }
}

impl Default for RequestId {
    fn default() -> Self {
        RequestId::String(uuid::Uuid::new_v4().to_string())
    }
}

/// Represents a JSON-RPC 2.0 Request object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest<P> {
    /// The version of the JSON-RPC protocol (always "2.0").
    pub jsonrpc: String,
    /// A unique identifier for this request.
    pub id: RequestId,
    /// The method name to be invoked.
    pub method: String,
    /// The parameters for the method invocation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<P>,
}

impl<P> JsonRpcRequest<P> {
    /// Creates a new JSON-RPC request.
    pub fn new(method: impl Into<String>, params: P) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: RequestId::default(),
            method: method.into(),
            params: Some(params),
        }
    }

    /// Creates a new JSON-RPC request with a specific ID.
    pub fn with_id(id: impl Into<RequestId>, method: impl Into<String>, params: P) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: id.into(),
            method: method.into(),
            params: Some(params),
        }
    }
}

/// Represents a successful JSON-RPC 2.0 Response object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcSuccessResponse<R> {
    /// The version of the JSON-RPC protocol (always "2.0").
    pub jsonrpc: String,
    /// The identifier established by the client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    /// The result of the method invocation.
    pub result: R,
}

impl<R> JsonRpcSuccessResponse<R> {
    /// Creates a new successful response.
    pub fn new(id: Option<RequestId>, result: R) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result,
        }
    }
}

/// Represents a JSON-RPC 2.0 Error Response object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorResponse {
    /// The version of the JSON-RPC protocol (always "2.0").
    pub jsonrpc: String,
    /// The identifier established by the client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    /// An object describing the error.
    pub error: JsonRpcError,
}

impl JsonRpcErrorResponse {
    /// Creates a new error response.
    pub fn new(id: Option<RequestId>, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            error,
        }
    }
}

/// Parameters for the message/send request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSendParams {
    /// The message being sent to the agent.
    pub message: Message,
    /// Optional configuration for the send request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<MessageSendConfiguration>,
    /// Optional metadata for extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl MessageSendParams {
    /// Creates new send parameters with a message.
    pub fn new(message: Message) -> Self {
        Self {
            message,
            configuration: None,
            metadata: None,
        }
    }

    /// Sets the configuration.
    pub fn with_configuration(mut self, config: MessageSendConfiguration) -> Self {
        self.configuration = Some(config);
        self
    }
}

/// Configuration options for a message/send or message/stream request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageSendConfiguration {
    /// A list of output MIME types the client accepts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_output_modes: Option<Vec<String>>,
    /// If true, the client will wait for the task to complete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<bool>,
    /// The number of recent messages to retrieve in the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
    /// Configuration for push notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_notification_config: Option<PushNotificationConfig>,
}

/// Parameters for the tasks/get request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskQueryParams {
    /// The unique identifier of the task.
    pub id: String,
    /// The number of recent messages to retrieve.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
    /// Optional metadata associated with the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl TaskQueryParams {
    /// Creates new query parameters.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            history_length: None,
            metadata: None,
        }
    }
}

/// Parameters for the tasks/cancel request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskIdParams {
    /// The unique identifier of the task.
    pub id: String,
    /// Optional metadata associated with the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl TaskIdParams {
    /// Creates new task ID parameters.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            metadata: None,
        }
    }
}

/// Configuration for push notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushNotificationConfig {
    /// The callback URL for push notifications.
    pub url: String,
    /// A unique identifier for this configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// A unique token to validate incoming push notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Optional authentication details for the push notification endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication: Option<PushNotificationAuthenticationInfo>,
}

impl PushNotificationConfig {
    /// Creates a new push notification configuration.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            id: None,
            token: None,
            authentication: None,
        }
    }
}

/// Authentication details for a push notification endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushNotificationAuthenticationInfo {
    /// A list of supported authentication schemes.
    pub schemes: Vec<String>,
    /// Optional credentials for the endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials: Option<String>,
}

/// Associates a push notification config with a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPushNotificationConfig {
    /// The unique identifier of the task.
    pub task_id: String,
    /// The push notification configuration.
    pub push_notification_config: PushNotificationConfig,
}

/// Parameters for getting a push notification config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskPushNotificationConfigParams {
    /// The unique identifier of the task.
    pub id: String,
    /// The ID of the config to retrieve.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_notification_config_id: Option<String>,
    /// Optional metadata associated with the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Parameters for deleting a push notification config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTaskPushNotificationConfigParams {
    /// The unique identifier of the task.
    pub id: String,
    /// The ID of the config to delete.
    pub push_notification_config_id: String,
    /// Optional metadata associated with the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl DeleteTaskPushNotificationConfigParams {
    /// Creates new delete parameters.
    pub fn new(id: impl Into<String>, config_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            push_notification_config_id: config_id.into(),
            metadata: None,
        }
    }
}

/// Parameters for listing push notification configs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTaskPushNotificationConfigParams {
    /// The unique identifier of the task.
    pub id: String,
    /// Optional metadata associated with the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl ListTaskPushNotificationConfigParams {
    /// Creates new list parameters.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            metadata: None,
        }
    }
}

impl GetTaskPushNotificationConfigParams {
    /// Creates new get parameters.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            push_notification_config_id: None,
            metadata: None,
        }
    }

    /// Sets the push notification config ID.
    pub fn with_config_id(mut self, config_id: impl Into<String>) -> Self {
        self.push_notification_config_id = Some(config_id.into());
        self
    }
}

/// Parameters for resubscribing to a task's event stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResubscriptionParams {
    /// The unique identifier of the task.
    pub id: String,
    /// Optional metadata associated with the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl TaskResubscriptionParams {
    /// Creates new resubscription parameters.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            metadata: None,
        }
    }
}

/// Configuration for message streaming request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageStreamConfiguration {
    /// A list of output MIME types the client accepts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_output_modes: Option<Vec<String>>,
    /// The number of recent messages to retrieve in the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
    /// Configuration for push notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_notification_config: Option<PushNotificationConfig>,
}

/// Parameters for streaming message request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageStreamParams {
    /// The message being sent to the agent.
    pub message: Message,
    /// Optional configuration for the stream request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<MessageStreamConfiguration>,
    /// Optional metadata for extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl MessageStreamParams {
    /// Creates new stream parameters with a message.
    pub fn new(message: Message) -> Self {
        Self {
            message,
            configuration: None,
            metadata: None,
        }
    }

    /// Sets the configuration.
    pub fn with_configuration(mut self, config: MessageStreamConfiguration) -> Self {
        self.configuration = Some(config);
        self
    }
}

/// Request for sending a message.
pub type SendMessageRequest = JsonRpcRequest<MessageSendParams>;

/// Request for streaming a message.
pub type SendStreamingMessageRequest = JsonRpcRequest<MessageSendParams>;

/// Request for getting a task.
pub type GetTaskRequest = JsonRpcRequest<TaskQueryParams>;

/// Request for canceling a task.
pub type CancelTaskRequest = JsonRpcRequest<TaskIdParams>;

/// Request for resubscribing to a task.
pub type TaskResubscriptionRequest = JsonRpcRequest<TaskIdParams>;

/// Request for setting push notification config.
pub type SetTaskPushNotificationConfigRequest = JsonRpcRequest<TaskPushNotificationConfig>;

/// Request for getting push notification config.
pub type GetTaskPushNotificationConfigRequest = JsonRpcRequest<GetTaskPushNotificationConfigParams>;

/// Request for listing push notification configs.
pub type ListTaskPushNotificationConfigRequest = JsonRpcRequest<TaskIdParams>;

/// Request for deleting push notification config.
pub type DeleteTaskPushNotificationConfigRequest =
    JsonRpcRequest<DeleteTaskPushNotificationConfigParams>;

/// Request for listing push notification configs.
pub type ListTaskPushNotificationConfigsRequest =
    JsonRpcRequest<ListTaskPushNotificationConfigParams>;

/// Request for getting authenticated extended card.
pub type GetAuthenticatedExtendedCardRequest = JsonRpcRequest<GetAuthenticatedExtendedCardParams>;

/// Parameters for getting authenticated extended card.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetAuthenticatedExtendedCardParams {
    /// Optional metadata associated with the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Request for streaming messages.
pub type MessageStreamRequest = JsonRpcRequest<MessageStreamParams>;

/// Result of a message/send request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SendMessageResult {
    /// A task was created or updated.
    Task(Task),
    /// A direct message reply.
    Message(Message),
}

/// Result of a message/stream request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StreamingMessageResult {
    /// A task was created or updated.
    Task(Task),
    /// A direct message reply.
    Message(Message),
    /// A status update event.
    StatusUpdate(TaskStatusUpdateEvent),
    /// An artifact update event.
    ArtifactUpdate(TaskArtifactUpdateEvent),
}

/// Successful response for sending a message.
pub type SendMessageSuccessResponse = JsonRpcSuccessResponse<SendMessageResult>;

/// Successful response for streaming a message.
pub type SendStreamingMessageSuccessResponse = JsonRpcSuccessResponse<StreamingMessageResult>;

/// Successful response for getting a task.
pub type GetTaskSuccessResponse = JsonRpcSuccessResponse<Task>;

/// Successful response for canceling a task.
pub type CancelTaskSuccessResponse = JsonRpcSuccessResponse<Task>;

/// Successful response for setting push notification config.
pub type SetTaskPushNotificationConfigSuccessResponse =
    JsonRpcSuccessResponse<TaskPushNotificationConfig>;

/// Successful response for getting push notification config.
pub type GetTaskPushNotificationConfigSuccessResponse =
    JsonRpcSuccessResponse<TaskPushNotificationConfig>;

/// Successful response for listing push notification configs.
pub type ListTaskPushNotificationConfigSuccessResponse =
    JsonRpcSuccessResponse<Vec<TaskPushNotificationConfig>>;

/// Successful response for deleting push notification config.
pub type DeleteTaskPushNotificationConfigSuccessResponse = JsonRpcSuccessResponse<()>;

/// Successful response for getting authenticated extended card.
pub type GetAuthenticatedExtendedCardSuccessResponse = JsonRpcSuccessResponse<AgentCard>;

/// A union type representing any JSON-RPC response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcResponse<R> {
    /// A successful response.
    Success(JsonRpcSuccessResponse<R>),
    /// An error response.
    Error(JsonRpcErrorResponse),
}

impl<R> JsonRpcResponse<R> {
    /// Returns true if this is a success response.
    pub fn is_success(&self) -> bool {
        matches!(self, JsonRpcResponse::Success(_))
    }

    /// Returns true if this is an error response.
    pub fn is_error(&self) -> bool {
        matches!(self, JsonRpcResponse::Error(_))
    }
}

/// A discriminated union representing all possible JSON-RPC 2.0 requests
/// supported by the A2A specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum A2ARequest {
    /// Send a message to the agent.
    #[serde(rename = "message/send")]
    SendMessage {
        /// JSON-RPC version.
        jsonrpc: String,
        /// Request ID.
        id: RequestId,
        /// Request parameters.
        params: MessageSendParams,
    },
    /// Stream a message to the agent.
    #[serde(rename = "message/stream")]
    StreamMessage {
        /// JSON-RPC version.
        jsonrpc: String,
        /// Request ID.
        id: RequestId,
        /// Request parameters.
        params: MessageSendParams,
    },
    /// Get a task by ID.
    #[serde(rename = "tasks/get")]
    GetTask {
        /// JSON-RPC version.
        jsonrpc: String,
        /// Request ID.
        id: RequestId,
        /// Request parameters.
        params: TaskQueryParams,
    },
    /// Cancel a task.
    #[serde(rename = "tasks/cancel")]
    CancelTask {
        /// JSON-RPC version.
        jsonrpc: String,
        /// Request ID.
        id: RequestId,
        /// Request parameters.
        params: TaskIdParams,
    },
    /// Resubscribe to a task's event stream.
    #[serde(rename = "tasks/resubscribe")]
    Resubscribe {
        /// JSON-RPC version.
        jsonrpc: String,
        /// Request ID.
        id: RequestId,
        /// Request parameters.
        params: TaskIdParams,
    },
    /// Set push notification configuration.
    #[serde(rename = "tasks/pushNotificationConfig/set")]
    SetPushNotificationConfig {
        /// JSON-RPC version.
        jsonrpc: String,
        /// Request ID.
        id: RequestId,
        /// Request parameters.
        params: TaskPushNotificationConfig,
    },
    /// Get push notification configuration.
    #[serde(rename = "tasks/pushNotificationConfig/get")]
    GetPushNotificationConfig {
        /// JSON-RPC version.
        jsonrpc: String,
        /// Request ID.
        id: RequestId,
        /// Request parameters.
        params: GetTaskPushNotificationConfigParams,
    },
    /// List push notification configurations.
    #[serde(rename = "tasks/pushNotificationConfig/list")]
    ListPushNotificationConfig {
        /// JSON-RPC version.
        jsonrpc: String,
        /// Request ID.
        id: RequestId,
        /// Request parameters.
        params: ListTaskPushNotificationConfigParams,
    },
    /// Delete push notification configuration.
    #[serde(rename = "tasks/pushNotificationConfig/delete")]
    DeletePushNotificationConfig {
        /// JSON-RPC version.
        jsonrpc: String,
        /// Request ID.
        id: RequestId,
        /// Request parameters.
        params: DeleteTaskPushNotificationConfigParams,
    },
    /// Get authenticated extended agent card.
    #[serde(rename = "agent/getAuthenticatedExtendedCard")]
    GetAuthenticatedExtendedCard {
        /// JSON-RPC version.
        jsonrpc: String,
        /// Request ID.
        id: RequestId,
    },
}

impl A2ARequest {
    /// Returns the method name of this request.
    pub fn method(&self) -> &'static str {
        match self {
            Self::SendMessage { .. } => "message/send",
            Self::StreamMessage { .. } => "message/stream",
            Self::GetTask { .. } => "tasks/get",
            Self::CancelTask { .. } => "tasks/cancel",
            Self::Resubscribe { .. } => "tasks/resubscribe",
            Self::SetPushNotificationConfig { .. } => "tasks/pushNotificationConfig/set",
            Self::GetPushNotificationConfig { .. } => "tasks/pushNotificationConfig/get",
            Self::ListPushNotificationConfig { .. } => "tasks/pushNotificationConfig/list",
            Self::DeletePushNotificationConfig { .. } => "tasks/pushNotificationConfig/delete",
            Self::GetAuthenticatedExtendedCard { .. } => "agent/getAuthenticatedExtendedCard",
        }
    }

    /// Returns the request ID.
    pub fn id(&self) -> &RequestId {
        match self {
            Self::SendMessage { id, .. }
            | Self::StreamMessage { id, .. }
            | Self::GetTask { id, .. }
            | Self::CancelTask { id, .. }
            | Self::Resubscribe { id, .. }
            | Self::SetPushNotificationConfig { id, .. }
            | Self::GetPushNotificationConfig { id, .. }
            | Self::ListPushNotificationConfig { id, .. }
            | Self::DeletePushNotificationConfig { id, .. }
            | Self::GetAuthenticatedExtendedCard { id, .. } => id,
        }
    }
}

/// A discriminated union representing all possible successful JSON-RPC 2.0
/// responses for the A2A specification methods.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum A2ASuccessResponse {
    /// Response for message/send.
    SendMessage(SendMessageSuccessResponse),
    /// Response for message/stream.
    StreamMessage(SendStreamingMessageSuccessResponse),
    /// Response for tasks/get.
    GetTask(GetTaskSuccessResponse),
    /// Response for tasks/cancel.
    CancelTask(CancelTaskSuccessResponse),
    /// Response for tasks/pushNotificationConfig/set.
    SetPushNotificationConfig(SetTaskPushNotificationConfigSuccessResponse),
    /// Response for tasks/pushNotificationConfig/get.
    GetPushNotificationConfig(GetTaskPushNotificationConfigSuccessResponse),
    /// Response for tasks/pushNotificationConfig/list.
    ListPushNotificationConfig(ListTaskPushNotificationConfigSuccessResponse),
    /// Response for tasks/pushNotificationConfig/delete.
    DeletePushNotificationConfig(DeleteTaskPushNotificationConfigSuccessResponse),
    /// Response for agent/getAuthenticatedExtendedCard.
    GetAuthenticatedExtendedCard(GetAuthenticatedExtendedCardSuccessResponse),
}

/// A discriminated union representing all possible JSON-RPC 2.0 responses
/// (success or error) for the A2A specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum A2AResponse {
    /// A successful response.
    Success(A2ASuccessResponse),
    /// An error response.
    Error(JsonRpcErrorResponse),
}

impl A2AResponse {
    /// Returns true if this is a success response.
    #[inline]
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    /// Returns true if this is an error response.
    #[inline]
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    /// Returns the error if this is an error response.
    pub fn error(&self) -> Option<&JsonRpcError> {
        match self {
            Self::Error(e) => Some(&e.error),
            _ => None,
        }
    }
}

/// Server-Sent Event wrapper for streaming responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseEvent {
    /// The event type (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// The event data (JSON string).
    pub data: String,
    /// Event ID for reconnection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Retry interval in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<u64>,
}

impl SseEvent {
    /// Creates a new SSE event with data.
    pub fn new(data: impl Into<String>) -> Self {
        Self {
            event: None,
            data: data.into(),
            id: None,
            retry: None,
        }
    }

    /// Creates an SSE event from a serializable value.
    pub fn from_json<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        Ok(Self::new(serde_json::to_string(value)?))
    }

    /// Sets the event type.
    pub fn with_event(mut self, event: impl Into<String>) -> Self {
        self.event = Some(event.into());
        self
    }

    /// Sets the event ID.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Formats the event as SSE wire format.
    pub fn to_sse_string(&self) -> String {
        let mut result = String::new();
        if let Some(ref event) = self.event {
            result.push_str(&format!("event: {}\n", event));
        }
        if let Some(ref id) = self.id {
            result.push_str(&format!("id: {}\n", id));
        }
        if let Some(retry) = self.retry {
            result.push_str(&format!("retry: {}\n", retry));
        }
        // Handle multi-line data
        for line in self.data.lines() {
            result.push_str(&format!("data: {}\n", line));
        }
        result.push('\n');
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Part, Role};

    #[test]
    fn test_request_id_from_string() {
        let id: RequestId = "test-123".into();
        assert_eq!(id, RequestId::String("test-123".to_string()));
    }

    #[test]
    fn test_request_id_from_number() {
        let id: RequestId = 42i64.into();
        assert_eq!(id, RequestId::Number(42));
    }

    #[test]
    fn test_send_message_request() {
        let message = Message::new("msg-1", Role::User, vec![Part::text("Hello")]);
        let params = MessageSendParams::new(message);
        let request: SendMessageRequest = JsonRpcRequest::new("message/send", params);

        assert_eq!(request.method, "message/send");
        assert_eq!(request.jsonrpc, "2.0");
    }

    #[test]
    fn test_json_rpc_response_serialization() {
        let task = Task::new("task-1", "ctx-1");
        let response = JsonRpcSuccessResponse::new(Some(RequestId::String("1".to_string())), task);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"result\""));
    }

    #[test]
    fn test_sse_event_formatting() {
        let event = SseEvent::new(r#"{"test": true}"#)
            .with_event("message")
            .with_id("123");
        let sse_str = event.to_sse_string();
        assert!(sse_str.contains("event: message\n"));
        assert!(sse_str.contains("id: 123\n"));
        assert!(sse_str.contains("data: {\"test\": true}\n"));
    }

    #[test]
    fn test_a2a_response_is_error() {
        let error = JsonRpcError::task_not_found("test-123");
        let response = A2AResponse::Error(JsonRpcErrorResponse::new(None, error));
        assert!(response.is_error());
        assert!(!response.is_success());
        assert!(response.error().is_some());
    }
}
