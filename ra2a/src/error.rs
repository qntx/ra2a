//! Error types for the A2A SDK.
//!
//! This module defines the error types used throughout the SDK, following
//! the JSON-RPC 2.0 error specification and A2A-specific error codes.
//! Client-specific error types mirror Python's `client/errors.py`.

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// A specialized Result type for A2A operations.
pub type Result<T> = std::result::Result<T, A2AError>;

/// The main error type for the A2A SDK.
#[derive(Error, Debug)]
pub enum A2AError {
    /// JSON-RPC protocol errors
    #[error("JSON-RPC error: {0}")]
    JsonRpc(#[from] JsonRpcError),

    /// HTTP transport errors
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization/deserialization errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// URL parsing errors
    #[error("Invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Connection errors
    #[error("Connection error: {0}")]
    Connection(String),

    /// Timeout errors
    #[error("Operation timed out: {0}")]
    Timeout(String),

    /// Stream errors
    #[error("Stream error: {0}")]
    Stream(String),

    /// Internal errors
    #[error("Internal error: {0}")]
    Internal(String),

    /// Client HTTP error with status code and body.
    #[error("HTTP {status}: {message}")]
    ClientHttp {
        /// HTTP status code.
        status: u16,
        /// Error message or response body.
        message: String,
        /// Optional response body.
        body: Option<String>,
    },

    /// Client JSON parsing error.
    #[error("Failed to parse JSON response: {message}")]
    ClientJson {
        /// Error message.
        message: String,
        /// Raw response that failed to parse.
        raw_response: Option<String>,
    },

    /// Client JSON-RPC error from server response.
    #[error("JSON-RPC error [{code}]: {message}")]
    ClientJsonRpc {
        /// JSON-RPC error code.
        code: i32,
        /// Error message.
        message: String,
        /// Additional error data.
        data: Option<serde_json::Value>,
    },

    /// Client timeout error.
    #[error("Request timed out after {duration_secs}s: {message}")]
    ClientTimeout {
        /// Timeout duration in seconds.
        duration_secs: u64,
        /// Error message.
        message: String,
    },

    /// Client invalid state error.
    #[error("Invalid client state: {message}")]
    ClientInvalidState {
        /// Error message.
        message: String,
    },

    /// Agent card error.
    #[error("Agent card error: {message}")]
    AgentCard {
        /// Error message.
        message: String,
    },

    /// Unsupported transport error.
    #[error("Unsupported transport: {transport}")]
    UnsupportedTransport {
        /// Transport type name.
        transport: String,
    },

    /// Server error - used by server-side code.
    #[error("Server error: {0}")]
    Server(#[from] ServerError),
}

impl A2AError {
    /// Creates a client HTTP error.
    pub fn client_http(status: u16, message: impl Into<String>) -> Self {
        Self::ClientHttp {
            status,
            message: message.into(),
            body: None,
        }
    }

    /// Creates a client HTTP error with body.
    pub fn client_http_with_body(
        status: u16,
        message: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self::ClientHttp {
            status,
            message: message.into(),
            body: Some(body.into()),
        }
    }

    /// Creates a client JSON error.
    pub fn client_json(message: impl Into<String>) -> Self {
        Self::ClientJson {
            message: message.into(),
            raw_response: None,
        }
    }

    /// Creates a client JSON error with raw response.
    pub fn client_json_with_response(
        message: impl Into<String>,
        raw_response: impl Into<String>,
    ) -> Self {
        Self::ClientJson {
            message: message.into(),
            raw_response: Some(raw_response.into()),
        }
    }

    /// Creates a client JSON-RPC error.
    pub fn client_jsonrpc(code: i32, message: impl Into<String>) -> Self {
        Self::ClientJsonRpc {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Creates a client JSON-RPC error with data.
    pub fn client_jsonrpc_with_data(
        code: i32,
        message: impl Into<String>,
        data: serde_json::Value,
    ) -> Self {
        Self::ClientJsonRpc {
            code,
            message: message.into(),
            data: Some(data),
        }
    }

    /// Creates a client timeout error.
    pub fn client_timeout(duration_secs: u64, message: impl Into<String>) -> Self {
        Self::ClientTimeout {
            duration_secs,
            message: message.into(),
        }
    }

    /// Creates a client invalid state error.
    pub fn client_invalid_state(message: impl Into<String>) -> Self {
        Self::ClientInvalidState {
            message: message.into(),
        }
    }

    /// Creates an agent card error.
    pub fn agent_card(message: impl Into<String>) -> Self {
        Self::AgentCard {
            message: message.into(),
        }
    }

    /// Creates an unsupported transport error.
    pub fn unsupported_transport(transport: impl Into<String>) -> Self {
        Self::UnsupportedTransport {
            transport: transport.into(),
        }
    }

    /// Returns true if this is a client-side error.
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::ClientHttp { .. }
                | Self::ClientJson { .. }
                | Self::ClientJsonRpc { .. }
                | Self::ClientTimeout { .. }
                | Self::ClientInvalidState { .. }
        )
    }

    /// Returns true if this is a timeout error.
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout(_) | Self::ClientTimeout { .. })
    }

    /// Returns true if this is a connection error.
    pub fn is_connection_error(&self) -> bool {
        matches!(self, Self::Connection(_) | Self::Http(_))
    }

    /// Extracts the JSON-RPC error code if this is a JSON-RPC error.
    pub fn jsonrpc_code(&self) -> Option<i32> {
        match self {
            Self::JsonRpc(e) => Some(e.code),
            Self::ClientJsonRpc { code, .. } => Some(*code),
            _ => None,
        }
    }
}

/// Server-side error wrapper.
#[derive(Error, Debug)]
pub struct ServerError {
    /// The underlying JSON-RPC error.
    pub error: JsonRpcError,
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl ServerError {
    /// Creates a new server error.
    pub fn new(error: JsonRpcError) -> Self {
        Self { error }
    }

    /// Creates a server error from an error code.
    pub fn from_code(code: JsonRpcErrorCode) -> Self {
        Self {
            error: JsonRpcError::new(code, code.default_message()),
        }
    }

    /// Creates a server error with a custom message.
    pub fn with_message(code: JsonRpcErrorCode, message: impl Into<String>) -> Self {
        Self {
            error: JsonRpcError::new(code, message),
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
            Self::TaskNotFound => "Task not found",
            Self::TaskNotCancelable => "Task cannot be canceled",
            Self::PushNotificationNotSupported => "Push Notification is not supported",
            Self::UnsupportedOperation => "This operation is not supported",
            Self::ContentTypeNotSupported => "Incompatible content types",
            Self::InvalidAgentResponse => "Invalid agent response",
            Self::AuthenticatedExtendedCardNotConfigured => {
                "Authenticated Extended Card is not configured"
            }
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
    fn test_client_http_error() {
        let error = A2AError::client_http(404, "Not Found");
        assert!(error.is_client_error());
        assert!(!error.is_timeout());
        assert_eq!(error.to_string(), "HTTP 404: Not Found");
    }

    #[test]
    fn test_client_jsonrpc_error() {
        let error = A2AError::client_jsonrpc(-32001, "Task not found");
        assert!(error.is_client_error());
        assert_eq!(error.jsonrpc_code(), Some(-32001));
    }

    #[test]
    fn test_client_timeout_error() {
        let error = A2AError::client_timeout(30, "Request timed out");
        assert!(error.is_timeout());
        assert!(error.is_client_error());
    }

    #[test]
    fn test_server_error() {
        let server_error = ServerError::from_code(JsonRpcErrorCode::TaskNotFound);
        assert_eq!(server_error.error.code, -32001);

        let server_error =
            ServerError::with_message(JsonRpcErrorCode::InternalError, "Custom error");
        assert_eq!(server_error.error.message, "Custom error");
    }
}
