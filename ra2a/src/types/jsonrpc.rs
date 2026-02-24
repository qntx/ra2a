//! JSON-RPC 2.0 types for the A2A protocol.
//!
//! Defines the request and response structures for JSON-RPC communication.

use serde::{Deserialize, Serialize};

use super::{Message, Metadata, PushConfig, Task};
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
        Self::String(s)
    }
}

impl From<&str> for RequestId {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<i64> for RequestId {
    fn from(n: i64) -> Self {
        Self::Number(n)
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::String(uuid::Uuid::new_v4().to_string())
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
    #[must_use]
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
    pub configuration: Option<MessageSendConfig>,
    /// Metadata for extensions.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

impl MessageSendParams {
    /// Creates new send parameters with a message.
    #[must_use]
    pub fn new(message: Message) -> Self {
        Self {
            message,
            configuration: None,
            metadata: Metadata::new(),
        }
    }

    /// Sets the configuration.
    #[must_use]
    pub fn with_configuration(mut self, config: MessageSendConfig) -> Self {
        self.configuration = Some(config);
        self
    }
}

/// Configuration options for a message/send or message/stream request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageSendConfig {
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
    pub push_notification_config: Option<PushConfig>,
}

/// Parameters for the tasks/get request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskQueryParams {
    /// The unique identifier of the task.
    pub id: String,
    /// The number of recent messages to retrieve.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
    /// Metadata associated with the request.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

impl TaskQueryParams {
    /// Creates new query parameters.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            history_length: None,
            metadata: Metadata::new(),
        }
    }
}

/// Parameters for the tasks/cancel request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskIdParams {
    /// The unique identifier of the task.
    pub id: String,
    /// Metadata associated with the request.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

impl TaskIdParams {
    /// Creates new task ID parameters.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            metadata: Metadata::new(),
        }
    }
}

/// Parameters for getting authenticated extended card.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetAuthenticatedExtendedCardParams {
    /// Metadata associated with the request.
    #[serde(default, skip_serializing_if = "Metadata::is_empty")]
    pub metadata: Metadata,
}

/// Parameters for listing tasks (aligned with Go's `ListTasksRequest`).
///
/// JSON field names use `snake_case` to match Go's json tags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListTasksRequest {
    /// The context ID to filter tasks by.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    /// Filter by task state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<super::TaskState>,
    /// Maximum number of tasks to return (1-100, default 50).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<i32>,
    /// Token for retrieving the next page of results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
    /// Number of recent messages to include per task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
    /// Only return tasks updated after this ISO 8601 timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated_after: Option<String>,
    /// Whether to include artifacts in the response.
    #[serde(default, skip_serializing_if = "crate::types::is_false")]
    pub include_artifacts: bool,
}

/// Response for listing tasks (aligned with Go's `ListTasksResponse`).
///
/// JSON field names use `snake_case` to match Go's json tags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListTasksResponse {
    /// The tasks matching the query.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<Task>,
    /// Total number of tasks available (before pagination).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_size: Option<i32>,
    /// Maximum number of tasks returned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<i32>,
    /// Token for retrieving the next page. Empty if no more results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

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
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    /// Returns true if this is an error response.
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
}

/// A JSON-RPC 2.0 response that is either a success or error.
pub type A2AResponse = JsonRpcResponse<serde_json::Value>;

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
        let request: JsonRpcRequest<MessageSendParams> =
            JsonRpcRequest::new("message/send", params);

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
    fn test_a2a_response_is_error() {
        let error = JsonRpcError::task_not_found("test-123");
        let response: A2AResponse = JsonRpcResponse::Error(JsonRpcErrorResponse::new(None, error));
        assert!(response.is_error());
        assert!(!response.is_success());
    }
}
