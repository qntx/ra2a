//! Request handlers for A2A server endpoints.

use crate::error::{JsonRpcError, Result};
use crate::types::{
    DeleteTaskPushNotificationConfigParams, GetTaskPushNotificationConfigParams,
    JsonRpcErrorResponse, JsonRpcRequest, JsonRpcSuccessResponse,
    ListTaskPushNotificationConfigParams, MessageSendParams, SendMessageResult, TaskIdParams,
    TaskPushNotificationConfig, TaskQueryParams, TaskResubscriptionParams,
};

use super::{AgentExecutor, ExecutionContext, ServerState};

/// Handles incoming JSON-RPC requests and routes them to the appropriate handler.
pub async fn handle_request<E: AgentExecutor>(
    state: &ServerState<E>,
    request_body: &str,
) -> Result<String> {
    // Parse the request
    let request: JsonRpcRequest<serde_json::Value> = match serde_json::from_str(request_body) {
        Ok(req) => req,
        Err(_) => {
            let error_response = JsonRpcErrorResponse::new(None, JsonRpcError::parse_error());
            return Ok(serde_json::to_string(&error_response)?);
        }
    };

    let response = match request.method.as_str() {
        "message/send" => handle_send_message(state, &request).await,
        "message/stream" => handle_send_message(state, &request).await,
        "tasks/get" => handle_get_task(state, &request).await,
        "tasks/cancel" => handle_cancel_task(state, &request).await,
        "tasks/resubscribe" => handle_resubscribe(state, &request).await,
        "tasks/pushNotificationConfig/set" => handle_set_push_config(state, &request).await,
        "tasks/pushNotificationConfig/get" => handle_get_push_config(state, &request).await,
        "tasks/pushNotificationConfig/list" => handle_list_push_configs(state, &request).await,
        "tasks/pushNotificationConfig/delete" => handle_delete_push_config(state, &request).await,
        "agent/getAuthenticatedExtendedCard" => handle_get_extended_card(state, &request).await,
        _ => {
            let error = JsonRpcError::method_not_found(&request.method);
            Err(error.into())
        }
    };

    let json_response = match response {
        Ok(result) => result,
        Err(e) => {
            let error = match e {
                crate::error::A2AError::JsonRpc(err) => err,
                _ => JsonRpcError::internal_error(e.to_string()),
            };
            serde_json::to_string(&JsonRpcErrorResponse::new(Some(request.id), error))?
        }
    };

    Ok(json_response)
}

/// Handles the message/send request.
async fn handle_send_message<E: AgentExecutor>(
    state: &ServerState<E>,
    request: &JsonRpcRequest<serde_json::Value>,
) -> Result<String> {
    let params: MessageSendParams = match &request.params {
        Some(p) => serde_json::from_value(p.clone())?,
        None => {
            return Err(JsonRpcError::invalid_params("Missing params").into());
        }
    };

    let message = params.message;

    // Get or create task context
    let (task_id, context_id) = match (&message.task_id, &message.context_id) {
        (Some(tid), Some(cid)) => (tid.clone(), cid.clone()),
        (Some(tid), None) => {
            // Look up existing task for context
            if let Some(task) = state.get_task(tid).await {
                (tid.clone(), task.context_id)
            } else {
                (tid.clone(), uuid::Uuid::new_v4().to_string())
            }
        }
        _ => (
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        ),
    };

    let ctx = ExecutionContext::new(&task_id, &context_id);

    // Execute the agent logic
    let task = state.executor.execute(&ctx, &message).await?;

    // Store the task
    state.store_task(task.clone()).await;

    // Build response
    let result = SendMessageResult::Task(task);
    let response = JsonRpcSuccessResponse::new(Some(request.id.clone()), result);

    Ok(serde_json::to_string(&response)?)
}

/// Handles the tasks/get request.
async fn handle_get_task<E: AgentExecutor>(
    state: &ServerState<E>,
    request: &JsonRpcRequest<serde_json::Value>,
) -> Result<String> {
    let params: TaskQueryParams = match &request.params {
        Some(p) => serde_json::from_value(p.clone())?,
        None => {
            return Err(JsonRpcError::invalid_params("Missing params").into());
        }
    };

    let task = state
        .get_task(&params.id)
        .await
        .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

    let response = JsonRpcSuccessResponse::new(Some(request.id.clone()), task);
    Ok(serde_json::to_string(&response)?)
}

/// Handles the tasks/cancel request.
async fn handle_cancel_task<E: AgentExecutor>(
    state: &ServerState<E>,
    request: &JsonRpcRequest<serde_json::Value>,
) -> Result<String> {
    let params: TaskIdParams = match &request.params {
        Some(p) => serde_json::from_value(p.clone())?,
        None => {
            return Err(JsonRpcError::invalid_params("Missing params").into());
        }
    };

    // Check if task exists
    let task = state
        .get_task(&params.id)
        .await
        .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

    // Check if task can be canceled
    if task.status.state.is_terminal() {
        return Err(JsonRpcError::task_not_cancelable(&params.id).into());
    }

    // Get context for cancellation
    let ctx = ExecutionContext::new(&task.id, &task.context_id);

    // Cancel the task through the executor
    let canceled_task = state.executor.cancel(&ctx, &params.id).await?;

    // Store the updated task
    state.store_task(canceled_task.clone()).await;

    let response = JsonRpcSuccessResponse::new(Some(request.id.clone()), canceled_task);
    Ok(serde_json::to_string(&response)?)
}

/// Handles the agent/getAuthenticatedExtendedCard request.
async fn handle_get_extended_card<E: AgentExecutor>(
    state: &ServerState<E>,
    request: &JsonRpcRequest<serde_json::Value>,
) -> Result<String> {
    let card = state.executor.agent_card().clone();
    let response = JsonRpcSuccessResponse::new(Some(request.id.clone()), card);
    Ok(serde_json::to_string(&response)?)
}

/// Handles the tasks/resubscribe request.
async fn handle_resubscribe<E: AgentExecutor>(
    state: &ServerState<E>,
    request: &JsonRpcRequest<serde_json::Value>,
) -> Result<String> {
    let params: TaskResubscriptionParams = match &request.params {
        Some(p) => serde_json::from_value(p.clone())?,
        None => return Err(JsonRpcError::invalid_params("Missing params").into()),
    };

    // Verify task exists
    let task = state
        .get_task(&params.id)
        .await
        .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

    let response = JsonRpcSuccessResponse::new(Some(request.id.clone()), task);
    Ok(serde_json::to_string(&response)?)
}

/// Handles the tasks/pushNotificationConfig/set request.
async fn handle_set_push_config<E: AgentExecutor>(
    state: &ServerState<E>,
    request: &JsonRpcRequest<serde_json::Value>,
) -> Result<String> {
    let config: TaskPushNotificationConfig = match &request.params {
        Some(p) => serde_json::from_value(p.clone())?,
        None => return Err(JsonRpcError::invalid_params("Missing params").into()),
    };

    // Check if push notifications are supported
    let card = state.executor.agent_card();
    let supports_push = card.capabilities.push_notifications.unwrap_or(false);

    if !supports_push {
        return Err(JsonRpcError::push_notification_not_supported().into());
    }

    // Store the config
    state.store_push_config(config.clone()).await;

    let response = JsonRpcSuccessResponse::new(Some(request.id.clone()), config);
    Ok(serde_json::to_string(&response)?)
}

/// Handles the tasks/pushNotificationConfig/get request.
async fn handle_get_push_config<E: AgentExecutor>(
    state: &ServerState<E>,
    request: &JsonRpcRequest<serde_json::Value>,
) -> Result<String> {
    let params: GetTaskPushNotificationConfigParams = match &request.params {
        Some(p) => serde_json::from_value(p.clone())?,
        None => return Err(JsonRpcError::invalid_params("Missing params").into()),
    };

    let config = state
        .get_push_config(&params.id, params.push_notification_config_id.as_deref())
        .await
        .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

    let response = JsonRpcSuccessResponse::new(Some(request.id.clone()), config);
    Ok(serde_json::to_string(&response)?)
}

/// Handles the tasks/pushNotificationConfig/list request.
async fn handle_list_push_configs<E: AgentExecutor>(
    state: &ServerState<E>,
    request: &JsonRpcRequest<serde_json::Value>,
) -> Result<String> {
    let params: ListTaskPushNotificationConfigParams = match &request.params {
        Some(p) => serde_json::from_value(p.clone())?,
        None => return Err(JsonRpcError::invalid_params("Missing params").into()),
    };

    let configs = state.list_push_configs(&params.id).await;

    let response = JsonRpcSuccessResponse::new(Some(request.id.clone()), configs);
    Ok(serde_json::to_string(&response)?)
}

/// Handles the tasks/pushNotificationConfig/delete request.
async fn handle_delete_push_config<E: AgentExecutor>(
    state: &ServerState<E>,
    request: &JsonRpcRequest<serde_json::Value>,
) -> Result<String> {
    let params: DeleteTaskPushNotificationConfigParams = match &request.params {
        Some(p) => serde_json::from_value(p.clone())?,
        None => return Err(JsonRpcError::invalid_params("Missing params").into()),
    };

    state
        .delete_push_config(&params.id, &params.push_notification_config_id)
        .await;

    let response = JsonRpcSuccessResponse::new(Some(request.id.clone()), serde_json::Value::Null);
    Ok(serde_json::to_string(&response)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::JsonRpcErrorCode;

    #[test]
    fn test_method_not_found() {
        let error = JsonRpcError::method_not_found("unknown/method");
        assert_eq!(error.code, JsonRpcErrorCode::MethodNotFound as i32);
    }
}
