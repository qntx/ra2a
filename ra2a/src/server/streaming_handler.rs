//! Streaming request handlers for the A2A server.
//!
//! This module provides handlers for streaming JSON-RPC requests,
//! delivering real-time task updates via Server-Sent Events.

use std::sync::Arc;
use tracing::{debug, error, instrument};

use super::{
    AgentExecutor, Event, ExecutionContext, QueueManager, ServerState, SseResponse,
    SseStreamBuilder,
};
use crate::error::{JsonRpcError, Result};
use crate::types::{
    JsonRpcRequest, MessageSendParams, Task, TaskIdParams, TaskStatus, TaskStatusUpdateEvent,
};

/// Configuration for streaming handlers.
#[derive(Debug, Clone)]
pub struct StreamingConfig {
    /// Maximum number of events to buffer per task.
    pub buffer_size: usize,
    /// Whether to send initial task state on subscription.
    pub send_initial_state: bool,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            buffer_size: 100,
            send_initial_state: true,
        }
    }
}

/// Handles streaming message requests.
///
/// This function processes a `message/stream` request and returns an SSE
/// response that delivers task updates in real-time.
#[instrument(skip(state, queue_manager, request), fields(method = "message/stream"))]
pub async fn handle_streaming_message<E: AgentExecutor + 'static>(
    state: &ServerState<E>,
    queue_manager: &QueueManager,
    request: &JsonRpcRequest<serde_json::Value>,
    config: &StreamingConfig,
) -> Result<SseResponse> {
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

    debug!("Creating streaming response for task {}", task_id);

    // Get or create event queue for this task
    let queue = queue_manager.get_or_create_queue(&task_id).await;
    let receiver = queue.subscribe();

    // Build SSE response
    let mut builder = SseStreamBuilder::new().request_id(request.id.clone());

    // Send initial task state if configured
    if config.send_initial_state {
        let initial_task = Task::new(&task_id, &context_id).with_status(TaskStatus::submitted());
        builder = builder.initial_event(Event::Task(initial_task.clone()));
        state.store_task(initial_task).await;
    }

    // Spawn background task to execute the agent
    let executor = Arc::clone(&state.executor);
    let tasks = Arc::clone(&state.tasks);
    let queue_clone = Arc::clone(&queue);
    let task_id_clone = task_id.clone();
    let context_id_clone = context_id.clone();

    tokio::spawn(async move {
        let ctx = ExecutionContext::new(&task_id_clone, &context_id_clone);

        // Send working status
        let working_status = TaskStatusUpdateEvent::new(
            &task_id_clone,
            &context_id_clone,
            TaskStatus::working(),
            false,
        );
        let _ = queue_clone.send(Event::StatusUpdate(working_status));

        // Execute the agent
        match executor.execute(&ctx, &message).await {
            Ok(task) => {
                // Send final task state
                let final_event = TaskStatusUpdateEvent::new(
                    &task.id,
                    &task.context_id,
                    task.status.clone(),
                    task.status.state.is_terminal(),
                );
                let _ = queue_clone.send(Event::StatusUpdate(final_event));
                let _ = queue_clone.send(Event::Task(task.clone()));
                // Store task
                let mut tasks_guard = tasks.write().await;
                tasks_guard.insert(task.id.clone(), task);
            }
            Err(e) => {
                error!("Agent execution failed: {}", e);
                let failed_status = TaskStatusUpdateEvent::new(
                    &task_id_clone,
                    &context_id_clone,
                    TaskStatus::failed(e.to_string()),
                    true,
                );
                let _ = queue_clone.send(Event::StatusUpdate(failed_status));
            }
        }
    });

    Ok(builder.build_from_receiver(receiver))
}

/// Handles task resubscription requests.
///
/// Allows clients to reconnect to an existing task's event stream.
#[instrument(
    skip(state, queue_manager, request),
    fields(method = "tasks/resubscribe")
)]
pub async fn handle_resubscribe<E: AgentExecutor>(
    state: &ServerState<E>,
    queue_manager: &QueueManager,
    request: &JsonRpcRequest<serde_json::Value>,
    config: &StreamingConfig,
) -> Result<SseResponse> {
    let params: TaskIdParams = match &request.params {
        Some(p) => serde_json::from_value(p.clone())?,
        None => {
            return Err(JsonRpcError::invalid_params("Missing params").into());
        }
    };

    // Verify task exists
    let task = state
        .get_task(&params.id)
        .await
        .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

    debug!("Resubscribing to task {}", params.id);

    // Get existing queue or return error if task is completed
    let queue = match queue_manager.get_queue(&params.id).await {
        Ok(q) => q,
        Err(_) => {
            // Task might be completed, create new queue and send current state
            if task.status.state.is_terminal() {
                // For terminal tasks, just send the final state
                let builder = SseStreamBuilder::new()
                    .request_id(request.id.clone())
                    .initial_event(Event::Task(task));
                return Ok(builder.build_from_stream(futures::stream::empty()));
            }
            queue_manager.get_or_create_queue(&params.id).await
        }
    };

    let receiver = queue.subscribe();

    let mut builder = SseStreamBuilder::new().request_id(request.id.clone());

    // Send current task state
    if config.send_initial_state {
        builder = builder.initial_event(Event::Task(task));
    }

    Ok(builder.build_from_receiver(receiver))
}

/// Streaming executor trait for agents that support real-time updates.
///
/// Implement this trait to provide fine-grained control over streaming
/// task execution with event emission.
#[async_trait::async_trait]
pub trait StreamingAgentExecutor: AgentExecutor {
    /// Executes a task with streaming support.
    ///
    /// The implementation should emit events through the provided queue
    /// as the task progresses.
    async fn execute_streaming(
        &self,
        ctx: &ExecutionContext,
        message: &crate::types::Message,
        queue: Arc<super::EventQueue>,
    ) -> Result<Task>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_config_default() {
        let config = StreamingConfig::default();
        assert_eq!(config.buffer_size, 100);
        assert!(config.send_initial_state);
    }
}
