//! Internal JSON-RPC 2.0 wire types and protocol constants.
//!
//! These types are transport-layer details and NOT part of the public API.
//! Items are only used when `client` or `server` features are enabled.

use serde::{Deserialize, Serialize};

use crate::error::JsonRpcError;

/// JSON-RPC 2.0 protocol version.
pub(crate) const VERSION: &str = "2.0";

/// Method name for `message/send`.
pub(crate) const METHOD_MESSAGE_SEND: &str = "message/send";
/// Method name for `message/stream`.
pub(crate) const METHOD_MESSAGE_STREAM: &str = "message/stream";
/// Method name for `tasks/get`.
pub(crate) const METHOD_TASKS_GET: &str = "tasks/get";
/// Method name for `tasks/list`.
pub(crate) const METHOD_TASKS_LIST: &str = "tasks/list";
/// Method name for `tasks/cancel`.
pub(crate) const METHOD_TASKS_CANCEL: &str = "tasks/cancel";
/// Method name for `tasks/resubscribe`.
pub(crate) const METHOD_TASKS_RESUBSCRIBE: &str = "tasks/resubscribe";
/// Method name for `tasks/pushNotificationConfig/get`.
pub(crate) const METHOD_PUSH_CONFIG_GET: &str = "tasks/pushNotificationConfig/get";
/// Method name for `tasks/pushNotificationConfig/set`.
pub(crate) const METHOD_PUSH_CONFIG_SET: &str = "tasks/pushNotificationConfig/set";
/// Method name for `tasks/pushNotificationConfig/list`.
pub(crate) const METHOD_PUSH_CONFIG_LIST: &str = "tasks/pushNotificationConfig/list";
/// Method name for `tasks/pushNotificationConfig/delete`.
pub(crate) const METHOD_PUSH_CONFIG_DELETE: &str = "tasks/pushNotificationConfig/delete";
/// Method name for `agent/getAuthenticatedExtendedCard`.
pub(crate) const METHOD_GET_EXTENDED_AGENT_CARD: &str = "agent/getAuthenticatedExtendedCard";

/// A unique identifier for a JSON-RPC request.
///
/// Can be a string, a number, or null — per JSON-RPC 2.0 spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum RequestId {
    /// String identifier.
    String(String),
    /// Numeric identifier.
    Number(i64),
}

impl Default for RequestId {
    fn default() -> Self {
        Self::String(uuid::Uuid::new_v4().to_string())
    }
}

/// A JSON-RPC 2.0 request object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JsonRpcRequest<P> {
    /// Protocol version (always "2.0").
    pub jsonrpc: String,
    /// Request identifier.
    pub id: RequestId,
    /// Method name to invoke.
    pub method: String,
    /// Method parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<P>,
}

impl<P> JsonRpcRequest<P> {
    /// Creates a new JSON-RPC request.
    pub(crate) fn new(method: impl Into<String>, params: P) -> Self {
        Self {
            jsonrpc: VERSION.to_owned(),
            id: RequestId::default(),
            method: method.into(),
            params: Some(params),
        }
    }
}

/// A successful JSON-RPC 2.0 response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JsonRpcSuccessResponse<R> {
    /// Protocol version (always "2.0").
    pub jsonrpc: String,
    /// The request identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    /// The result payload.
    pub result: R,
}

impl<R> JsonRpcSuccessResponse<R> {
    /// Creates a new success response.
    pub(crate) fn new(id: Option<RequestId>, result: R) -> Self {
        Self {
            jsonrpc: VERSION.to_owned(),
            id,
            result,
        }
    }
}

/// A JSON-RPC 2.0 error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JsonRpcErrorResponse {
    /// Protocol version (always "2.0").
    pub jsonrpc: String,
    /// The request identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    /// The error object.
    pub error: JsonRpcError,
}

impl JsonRpcErrorResponse {
    /// Creates a new error response.
    pub(crate) fn new(id: Option<RequestId>, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: VERSION.to_owned(),
            id,
            error,
        }
    }
}

/// A JSON-RPC 2.0 response — either success or error.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum JsonRpcResponse<R> {
    /// A successful response.
    Success(JsonRpcSuccessResponse<R>),
    /// An error response.
    Error(JsonRpcErrorResponse),
}
