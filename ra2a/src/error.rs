//! Error types for the A2A SDK.
//!
//! This module defines error types aligned with the A2A protocol specification.
//! Protocol-level errors mirror the Go reference implementation's sentinel errors
//! and map directly to JSON-RPC error codes.
//!
//! # Error Categories
//!
//! - **Protocol errors**: A2A-specific errors (e.g., task not found, unsupported operation)
//! - **Transport errors**: HTTP, JSON, URL parsing errors
//! - **Infrastructure errors**: Database, I/O errors

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A specialized Result type for A2A operations.
pub type Result<T> = std::result::Result<T, A2AError>;

/// The main error type for the A2A SDK.
///
/// Protocol-level variants are aligned with Go reference implementation sentinel errors
/// (`a2a.ErrTaskNotFound`, `a2a.ErrInvalidParams`, etc.) and map to JSON-RPC error codes.
#[derive(Error, Debug)]
pub enum A2AError {
    // === A2A Protocol Errors (aligned with Go sentinel errors) ===
    /// Server received payload that was not well-formed.
    #[error("parse error: {0}")]
    ParseError(String),

    /// Server received a well-formed payload which was not a valid request.
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// A method does not exist or is not supported.
    #[error("method not found: {0}")]
    MethodNotFound(String),

    /// Parameters provided for the method were invalid.
    #[error("invalid params: {0}")]
    InvalidParams(String),

    /// An unexpected error occurred on the server during processing.
    #[error("internal error: {0}")]
    InternalError(String),

    /// Reserved for implementation-defined server errors.
    #[error("server error: {0}")]
    ServerError(String),

    /// A task with the provided ID was not found.
    #[error("task not found: {0}")]
    TaskNotFound(String),

    /// The task was in a state where it could not be canceled.
    #[error("task cannot be canceled: {0}")]
    TaskNotCancelable(String),

    /// The agent does not support push notifications.
    #[error("push notification not supported")]
    PushNotificationNotSupported,

    /// The requested operation is not supported by the agent.
    #[error("unsupported operation: {0}")]
    UnsupportedOperation(String),

    /// Incompatibility between requested content types and agent's capabilities.
    #[error("incompatible content types: {0}")]
    ContentTypeNotSupported(String),

    /// The agent returned a response that does not conform to the specification.
    #[error("invalid agent response: {0}")]
    InvalidAgentResponse(String),

    /// The agent does not have an authenticated extended card configured.
    #[error("extended card not configured")]
    ExtendedCardNotConfigured,

    /// The request does not have valid authentication credentials.
    #[error("unauthenticated: {0}")]
    Unauthenticated(String),

    /// The caller does not have permission to execute the specified operation.
    #[error("permission denied: {0}")]
    Unauthorized(String),

    // === Transport Errors ===
    /// JSON-RPC protocol error received from remote.
    #[error("JSON-RPC error: {0}")]
    JsonRpc(#[from] JsonRpcError),

    /// HTTP transport error.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// URL parsing error.
    #[error("invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),

    // === Infrastructure Errors ===
    /// Database error from SQL storage operations.
    #[error("database error: {0}")]
    Database(String),

    /// Generic I/O or connection error.
    #[error("{0}")]
    Other(String),
}

impl A2AError {
    /// Returns `true` if this is a transport-level error (HTTP, JSON, connection).
    pub fn is_transport_error(&self) -> bool {
        matches!(self, Self::Http(_) | Self::Json(_) | Self::InvalidUrl(_))
    }

    /// Extracts the JSON-RPC error code if this is a JSON-RPC error.
    pub fn jsonrpc_code(&self) -> Option<i32> {
        if let Self::JsonRpc(e) = self {
            Some(e.code)
        } else {
            None
        }
    }

    /// Converts this error to a [`JsonRpcError`] for transport serialization.
    pub fn to_jsonrpc_error(&self) -> JsonRpcError {
        let (code, default_msg) = self.jsonrpc_error_code();
        let message = {
            let s = self.to_string();
            if s.is_empty() {
                default_msg.to_string()
            } else {
                s
            }
        };
        JsonRpcError {
            code: code as i32,
            message,
            data: None,
        }
    }

    /// Maps this error to its corresponding [`JsonRpcErrorCode`] and default message.
    fn jsonrpc_error_code(&self) -> (JsonRpcErrorCode, &'static str) {
        match self {
            Self::ParseError(_) => (JsonRpcErrorCode::ParseError, "Parse error"),
            Self::InvalidRequest(_) => (JsonRpcErrorCode::InvalidRequest, "Invalid request"),
            Self::MethodNotFound(_) => (JsonRpcErrorCode::MethodNotFound, "Method not found"),
            Self::InvalidParams(_) => (JsonRpcErrorCode::InvalidParams, "Invalid params"),
            Self::InternalError(_) => (JsonRpcErrorCode::InternalError, "Internal error"),
            Self::ServerError(_) => (JsonRpcErrorCode::ServerError, "Server error"),
            Self::TaskNotFound(_) => (JsonRpcErrorCode::TaskNotFound, "Task not found"),
            Self::TaskNotCancelable(_) => {
                (JsonRpcErrorCode::TaskNotCancelable, "Task not cancelable")
            }
            Self::PushNotificationNotSupported => (
                JsonRpcErrorCode::PushNotificationNotSupported,
                "Push notification not supported",
            ),
            Self::UnsupportedOperation(_) => (
                JsonRpcErrorCode::UnsupportedOperation,
                "Unsupported operation",
            ),
            Self::ContentTypeNotSupported(_) => (
                JsonRpcErrorCode::ContentTypeNotSupported,
                "Content type not supported",
            ),
            Self::InvalidAgentResponse(_) => (
                JsonRpcErrorCode::InvalidAgentResponse,
                "Invalid agent response",
            ),
            Self::ExtendedCardNotConfigured => (
                JsonRpcErrorCode::AuthenticatedExtendedCardNotConfigured,
                "Extended card not configured",
            ),
            Self::Unauthenticated(_) => (JsonRpcErrorCode::Unauthenticated, "Unauthenticated"),
            Self::Unauthorized(_) => (JsonRpcErrorCode::Unauthorized, "Permission denied"),
            _ => (JsonRpcErrorCode::InternalError, "Internal error"),
        }
    }
}

/// JSON-RPC 2.0 error codes as defined in the specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum JsonRpcErrorCode {
    /// Invalid JSON was received by the server.
    ParseError = -32700,
    /// The JSON sent is not a valid Request object.
    InvalidRequest = -32600,
    /// The method does not exist / is not available.
    MethodNotFound = -32601,
    /// Invalid method parameter(s).
    InvalidParams = -32602,
    /// Internal JSON-RPC error.
    InternalError = -32603,

    // A2A-specific error codes
    /// Reserved for implementation-defined server errors.
    ServerError = -32000,
    /// Task not found.
    TaskNotFound = -32001,
    /// Task cannot be canceled.
    TaskNotCancelable = -32002,
    /// Push notification not supported.
    PushNotificationNotSupported = -32003,
    /// Operation not supported.
    UnsupportedOperation = -32004,
    /// Content type not supported.
    ContentTypeNotSupported = -32005,
    /// Invalid agent response.
    InvalidAgentResponse = -32006,
    /// Authenticated extended card not configured.
    AuthenticatedExtendedCardNotConfigured = -32007,
    /// Request does not have valid authentication credentials.
    Unauthenticated = -31401,
    /// Caller does not have permission to execute the specified operation.
    Unauthorized = -31403,
}

impl JsonRpcErrorCode {
    /// Returns the default message for this error code.
    pub fn default_message(&self) -> &'static str {
        match self {
            Self::ParseError => "Invalid JSON payload",
            Self::InvalidRequest => "Request payload validation error",
            Self::MethodNotFound => "Method not found",
            Self::InvalidParams => "Invalid parameters",
            Self::InternalError => "Internal error",
            Self::ServerError => "Server error",
            Self::TaskNotFound => "Task not found",
            Self::TaskNotCancelable => "Task cannot be canceled",
            Self::PushNotificationNotSupported => "Push Notification is not supported",
            Self::UnsupportedOperation => "This operation is not supported",
            Self::ContentTypeNotSupported => "Incompatible content types",
            Self::InvalidAgentResponse => "Invalid agent response",
            Self::AuthenticatedExtendedCardNotConfigured => {
                "Authenticated Extended Card is not configured"
            }
            Self::Unauthenticated => "Unauthenticated",
            Self::Unauthorized => "Permission denied",
        }
    }
}

impl From<i32> for JsonRpcErrorCode {
    fn from(code: i32) -> Self {
        match code {
            -32700 => Self::ParseError,
            -32600 => Self::InvalidRequest,
            -32601 => Self::MethodNotFound,
            -32602 => Self::InvalidParams,
            -32603 => Self::InternalError,
            -32001 => Self::TaskNotFound,
            -32002 => Self::TaskNotCancelable,
            -32003 => Self::PushNotificationNotSupported,
            -32004 => Self::UnsupportedOperation,
            -32005 => Self::ContentTypeNotSupported,
            -32006 => Self::InvalidAgentResponse,
            -32007 => Self::AuthenticatedExtendedCardNotConfigured,
            -31401 => Self::Unauthenticated,
            -31403 => Self::Unauthorized,
            _ => Self::InternalError,
        }
    }
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize, Error)]
pub struct JsonRpcError {
    /// A number indicating the error type.
    pub code: i32,
    /// A short description of the error.
    pub message: String,
    /// Additional information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl JsonRpcError {
    /// Creates a new JSON-RPC error.
    pub fn new(code: JsonRpcErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: code as i32,
            message: message.into(),
            data: None,
        }
    }

    /// Creates a new JSON-RPC error with additional data.
    pub fn with_data(
        code: JsonRpcErrorCode,
        message: impl Into<String>,
        data: serde_json::Value,
    ) -> Self {
        Self {
            code: code as i32,
            message: message.into(),
            data: Some(data),
        }
    }

    /// Creates a parse error.
    pub fn parse_error() -> Self {
        Self::new(
            JsonRpcErrorCode::ParseError,
            JsonRpcErrorCode::ParseError.default_message(),
        )
    }

    /// Creates an invalid request error.
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(JsonRpcErrorCode::InvalidRequest, message)
    }

    /// Creates a method not found error.
    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            JsonRpcErrorCode::MethodNotFound,
            format!("Method '{}' not found", method),
        )
    }

    /// Creates an invalid params error.
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(JsonRpcErrorCode::InvalidParams, message)
    }

    /// Creates an internal error.
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new(JsonRpcErrorCode::InternalError, message)
    }

    /// Creates a task not found error.
    pub fn task_not_found(task_id: &str) -> Self {
        Self::new(
            JsonRpcErrorCode::TaskNotFound,
            format!("Task '{}' not found", task_id),
        )
    }

    /// Creates a task not cancelable error.
    pub fn task_not_cancelable(task_id: &str) -> Self {
        Self::new(
            JsonRpcErrorCode::TaskNotCancelable,
            format!("Task '{}' cannot be canceled", task_id),
        )
    }

    /// Creates a push notification not supported error.
    pub fn push_notification_not_supported() -> Self {
        Self::new(
            JsonRpcErrorCode::PushNotificationNotSupported,
            JsonRpcErrorCode::PushNotificationNotSupported.default_message(),
        )
    }

    /// Creates an unsupported operation error.
    pub fn unsupported_operation(operation: &str) -> Self {
        Self::new(
            JsonRpcErrorCode::UnsupportedOperation,
            format!("Operation '{}' is not supported", operation),
        )
    }

    /// Returns the error code as an enum variant.
    pub fn error_code(&self) -> JsonRpcErrorCode {
        JsonRpcErrorCode::from(self.code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_conversion() {
        assert_eq!(JsonRpcErrorCode::from(-32700), JsonRpcErrorCode::ParseError);
        assert_eq!(
            JsonRpcErrorCode::from(-32001),
            JsonRpcErrorCode::TaskNotFound
        );
    }

    #[test]
    fn test_json_rpc_error_serialization() {
        let error = JsonRpcError::task_not_found("test-123");
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("-32001"));
        assert!(json.contains("test-123"));
    }

    #[test]
    fn test_sentinel_errors_to_jsonrpc() {
        let err = A2AError::TaskNotFound("task-1".into());
        let rpc = err.to_jsonrpc_error();
        assert_eq!(rpc.code, JsonRpcErrorCode::TaskNotFound as i32);

        let err = A2AError::InternalError("boom".into());
        let rpc = err.to_jsonrpc_error();
        assert_eq!(rpc.code, JsonRpcErrorCode::InternalError as i32);
    }

    #[test]
    fn test_other_and_database_errors() {
        let err = A2AError::Other("something".into());
        let rpc = err.to_jsonrpc_error();
        assert_eq!(rpc.code, JsonRpcErrorCode::InternalError as i32);

        let err = A2AError::Database("db down".into());
        let rpc = err.to_jsonrpc_error();
        assert_eq!(rpc.code, JsonRpcErrorCode::InternalError as i32);
    }
}
