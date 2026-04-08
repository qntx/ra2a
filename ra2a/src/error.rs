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

    /// Optimistic concurrency control failed during task update.
    #[error("concurrent task modification")]
    ConcurrentTaskModification,

    /// The client requested use of a required extension but did not declare support.
    #[error("extension support required: {0}")]
    ExtensionSupportRequired(String),

    /// The A2A protocol version specified in the request is not supported.
    #[error("version not supported: {0}")]
    VersionNotSupported(String),

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
    #[must_use]
    pub const fn is_transport_error(&self) -> bool {
        matches!(self, Self::Http(_) | Self::Json(_))
    }

    /// Extracts the JSON-RPC error code if this is a JSON-RPC error.
    #[must_use]
    pub const fn jsonrpc_code(&self) -> Option<i32> {
        if let Self::JsonRpc(e) = self {
            Some(e.code)
        } else {
            None
        }
    }

    /// Converts this error to a [`JsonRpcError`] for transport serialization.
    #[must_use]
    pub fn to_jsonrpc_error(&self) -> JsonRpcError {
        let (code, default_msg) = self.jsonrpc_error_code();
        let message = {
            let s = self.to_string();
            if s.is_empty() {
                default_msg.to_owned()
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
    const fn jsonrpc_error_code(&self) -> (JsonRpcErrorCode, &'static str) {
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
            Self::ConcurrentTaskModification => (
                JsonRpcErrorCode::ConcurrentTaskModification,
                "Concurrent task modification",
            ),
            Self::ExtensionSupportRequired(_) => (
                JsonRpcErrorCode::ExtensionSupportRequired,
                "Extension support required",
            ),
            Self::VersionNotSupported(_) => (
                JsonRpcErrorCode::VersionNotSupported,
                "Version not supported",
            ),
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
    /// Optimistic concurrency control failed.
    ConcurrentTaskModification = -32008,
    /// Extension support required.
    ExtensionSupportRequired = -32009,
    /// Version not supported.
    VersionNotSupported = -32010,
}

impl JsonRpcErrorCode {
    /// Returns the default message for this error code.
    #[must_use]
    pub const fn default_message(&self) -> &'static str {
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
            Self::ConcurrentTaskModification => "Concurrent task modification",
            Self::ExtensionSupportRequired => "Extension support required",
            Self::VersionNotSupported => "Version not supported",
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
            -32008 => Self::ConcurrentTaskModification,
            -32009 => Self::ExtensionSupportRequired,
            -32010 => Self::VersionNotSupported,
            _ => Self::InternalError,
        }
    }
}

/// REST error detail per google.rpc.ErrorInfo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestErrorInfo {
    /// Always `"type.googleapis.com/google.rpc.ErrorInfo"`.
    #[serde(rename = "@type")]
    pub at_type: String,
    /// Machine-readable reason code (e.g. `"TASK_NOT_FOUND"`).
    pub reason: String,
    /// Error domain, always `"a2a-protocol.org"`.
    pub domain: String,
    /// Additional metadata (timestamp, taskId, etc.).
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub metadata: std::collections::HashMap<String, String>,
}

/// Inner status object in a REST error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestStatusError {
    /// HTTP status code.
    pub code: u16,
    /// gRPC status string (e.g. `"NOT_FOUND"`).
    pub status: String,
    /// Human-readable error message.
    pub message: String,
    /// Error details array.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<serde_json::Value>,
}

/// A google.rpc.Status error response for the HTTP+JSON REST binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestError {
    /// The HTTP status code for this response.
    #[serde(skip)]
    pub http_status: u16,
    /// The error payload.
    pub error: RestStatusError,
}

impl A2AError {
    /// Converts this error to a [`RestError`] in google.rpc.Status format (AIP-193).
    #[must_use]
    pub fn to_rest_error(&self, task_id: Option<&str>) -> RestError {
        let (http_status, grpc_status, reason) = self.rest_error_details();
        let mut metadata = std::collections::HashMap::new();
        metadata.insert(
            "timestamp".into(),
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        );
        if let Some(tid) = task_id
            && !tid.is_empty()
        {
            metadata.insert("taskId".into(), tid.to_owned());
        }
        let info = RestErrorInfo {
            at_type: "type.googleapis.com/google.rpc.ErrorInfo".into(),
            reason: reason.into(),
            domain: "a2a-protocol.org".into(),
            metadata,
        };
        RestError {
            http_status,
            error: RestStatusError {
                code: http_status,
                status: grpc_status.into(),
                message: self.to_string(),
                details: vec![serde_json::to_value(info).unwrap_or_default()],
            },
        }
    }

    /// Maps this error to (HTTP status, gRPC status string, reason).
    const fn rest_error_details(&self) -> (u16, &'static str, &'static str) {
        match self {
            Self::ParseError(_) | Self::InvalidRequest(_) | Self::InvalidParams(_) => {
                (400, "INVALID_ARGUMENT", "INVALID_REQUEST")
            }
            Self::MethodNotFound(_) => (404, "NOT_FOUND", "METHOD_NOT_FOUND"),
            Self::TaskNotFound(_) => (404, "NOT_FOUND", "TASK_NOT_FOUND"),
            Self::TaskNotCancelable(_) => (400, "FAILED_PRECONDITION", "TASK_NOT_CANCELABLE"),
            Self::PushNotificationNotSupported => {
                (501, "UNIMPLEMENTED", "PUSH_NOTIFICATION_NOT_SUPPORTED")
            }
            Self::UnsupportedOperation(_) => (501, "UNIMPLEMENTED", "UNSUPPORTED_OPERATION"),
            Self::ContentTypeNotSupported(_) => {
                (400, "INVALID_ARGUMENT", "UNSUPPORTED_CONTENT_TYPE")
            }
            Self::InvalidAgentResponse(_) => (500, "INTERNAL", "INVALID_AGENT_RESPONSE"),
            Self::ExtendedCardNotConfigured => (
                400,
                "FAILED_PRECONDITION",
                "EXTENDED_AGENT_CARD_NOT_CONFIGURED",
            ),
            Self::Unauthenticated(_) => (401, "UNAUTHENTICATED", "UNAUTHENTICATED"),
            Self::Unauthorized(_) => (403, "PERMISSION_DENIED", "UNAUTHORIZED"),
            Self::ConcurrentTaskModification => (409, "ABORTED", "CONCURRENT_TASK_MODIFICATION"),
            Self::ExtensionSupportRequired(_) => {
                (400, "FAILED_PRECONDITION", "EXTENSION_SUPPORT_REQUIRED")
            }
            Self::VersionNotSupported(_) => (501, "UNIMPLEMENTED", "VERSION_NOT_SUPPORTED"),
            _ => (500, "INTERNAL", "INTERNAL_ERROR"),
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

    /// Creates a parse error.
    #[must_use]
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
    #[must_use]
    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            JsonRpcErrorCode::MethodNotFound,
            format!("Method '{method}' not found"),
        )
    }

    /// Creates an invalid params error.
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(JsonRpcErrorCode::InvalidParams, message)
    }

    /// Returns the error code as an enum variant.
    #[must_use]
    pub fn error_code(&self) -> JsonRpcErrorCode {
        JsonRpcErrorCode::from(self.code)
    }
}
