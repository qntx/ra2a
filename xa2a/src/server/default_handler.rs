//! Default implementation of the RequestHandler trait.
//!
//! This module provides `DefaultRequestHandler`, a complete implementation that
//! coordinates between the `AgentExecutor`, `TaskStore`, `QueueManager`, and
//! optional push notification components.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::error::{A2AError, JsonRpcError, Result};
use crate::types::{
    DeleteTaskPushNotificationConfigParams, GetTaskPushNotificationConfigParams,
    ListTaskPushNotificationConfigParams, Message, MessageSendParams, Task, TaskIdParams,
    TaskPushNotificationConfig, TaskQueryParams, TaskState, TaskStatus,
};

use super::call_context::ServerCallContext;
use super::events::{Event, EventQueue, QueueManager};
use super::request_handler::{EventStream, RequestHandler, SendMessageResponse};
use super::{AgentExecutor, ExecutionContext};

/// Terminal task states that cannot be modified.
const TERMINAL_STATES: [TaskState; 4] = [
    TaskState::Completed,
    TaskState::Canceled,
    TaskState::Failed,
    TaskState::Rejected,
];

/// Checks if a task state is terminal.
fn is_terminal_state(state: TaskState) -> bool {
    TERMINAL_STATES.contains(&state)
}

/// Default request handler for all incoming A2A requests.
///
/// This handler provides default implementations for all A2A JSON-RPC methods,
/// coordinating between the `AgentExecutor`, task storage, `QueueManager`,
/// and optional push notification components.
pub struct DefaultRequestHandler<E: AgentExecutor> {
    /// The agent executor for processing messages.
    executor: Arc<E>,
    /// Task storage.
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    /// Queue manager for streaming events.
    queue_manager: Arc<QueueManager>,
    /// Push notification config storage.
    push_configs: Arc<RwLock<HashMap<String, Vec<TaskPushNotificationConfig>>>>,
    /// Running agent tasks.
    running_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl<E: AgentExecutor> DefaultRequestHandler<E> {
    /// Creates a new default request handler.
    pub fn new(executor: E) -> Self {
        Self {
            executor: Arc::new(executor),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            queue_manager: Arc::new(QueueManager::new()),
            push_configs: Arc::new(RwLock::new(HashMap::new())),
            running_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Creates a new handler with a shared queue manager.
    pub fn with_queue_manager(executor: E, queue_manager: Arc<QueueManager>) -> Self {
        Self {
            executor: Arc::new(executor),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            queue_manager,
            push_configs: Arc::new(RwLock::new(HashMap::new())),
            running_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Returns the agent card.
    pub fn agent_card(&self) -> &crate::types::AgentCard {
        self.executor.agent_card()
    }

    /// Gets a task by ID.
    async fn get_task_internal(&self, task_id: &str) -> Option<Task> {
        let tasks = self.tasks.read().await;
        tasks.get(task_id).cloned()
    }

    /// Stores a task.
    async fn store_task(&self, task: Task) {
        let mut tasks = self.tasks.write().await;
        tasks.insert(task.id.clone(), task);
    }

    /// Applies history length limit to a task.
    fn apply_history_length(&self, mut task: Task, history_length: Option<i32>) -> Task {
        if let Some(len) = history_length {
            if len >= 0 {
                if let Some(ref mut history) = task.history {
                    let len = len as usize;
                    if history.len() > len {
                        let start = history.len() - len;
                        *history = history.drain(start..).collect();
                    }
                }
            }
        }
        task
    }

    /// Sets up message execution context.
    async fn setup_execution(
        &self,
        params: &MessageSendParams,
        _context: Option<&ServerCallContext>,
    ) -> Result<(String, String, Arc<EventQueue>)> {
        let message = &params.message;

        // Determine task_id and context_id
        let (task_id, context_id) = match (&message.task_id, &message.context_id) {
            (Some(tid), Some(cid)) => (tid.clone(), cid.clone()),
            (Some(tid), None) => {
                // Look up existing task for context
                if let Some(task) = self.get_task_internal(tid).await {
                    (tid.clone(), task.context_id)
                } else {
                    return Err(JsonRpcError::task_not_found(tid).into());
                }
            }
            (None, Some(cid)) => (uuid::Uuid::new_v4().to_string(), cid.clone()),
            (None, None) => (
                uuid::Uuid::new_v4().to_string(),
                uuid::Uuid::new_v4().to_string(),
            ),
        };

        // Check if task exists and is in valid state
        if let Some(existing_task) = self.get_task_internal(&task_id).await {
            if is_terminal_state(existing_task.status.state) {
                return Err(JsonRpcError::invalid_params(format!(
                    "Task {} is in terminal state: {:?}",
                    task_id, existing_task.status.state
                ))
                .into());
            }
        }

        // Get or create event queue
        let queue = self.queue_manager.get_or_create_queue(&task_id).await;

        // Store push notification config if provided
        if let Some(ref config) = params.configuration {
            if let Some(ref push_config) = config.push_notification_config {
                let task_config = TaskPushNotificationConfig {
                    task_id: task_id.clone(),
                    push_notification_config: push_config.clone(),
                };
                self.store_push_config(task_config).await;
            }
        }

        debug!(
            task_id = %task_id,
            context_id = %context_id,
            "Set up execution context"
        );

        Ok((task_id, context_id, queue))
    }

    /// Stores a push notification config.
    async fn store_push_config(&self, config: TaskPushNotificationConfig) {
        let mut configs = self.push_configs.write().await;
        let task_configs = configs
            .entry(config.task_id.clone())
            .or_insert_with(Vec::new);

        // Replace if same config_id exists
        if let Some(ref config_id) = config.push_notification_config.id {
            if let Some(pos) = task_configs
                .iter()
                .position(|c| c.push_notification_config.id.as_ref() == Some(config_id))
            {
                task_configs[pos] = config;
                return;
            }
        }
        task_configs.push(config);
    }

    /// Runs the agent execution and emits events to the queue.
    async fn run_execution(
        executor: Arc<E>,
        task_id: String,
        context_id: String,
        message: Message,
        queue: Arc<EventQueue>,
        tasks: Arc<RwLock<HashMap<String, Task>>>,
    ) {
        let ctx = ExecutionContext::new(&task_id, &context_id);

        // Execute the agent
        match executor.execute(&ctx, &message).await {
            Ok(task) => {
                // Store the task
                {
                    let mut task_store = tasks.write().await;
                    task_store.insert(task.id.clone(), task.clone());
                }

                // Emit final task event
                if let Err(e) = queue.send(Event::Task(task)) {
                    error!(error = %e, "Failed to send task event");
                }
            }
            Err(e) => {
                error!(error = %e, task_id = %task_id, "Agent execution failed");

                // Create a failed task
                let mut task = Task::new(&task_id, &context_id);
                task.status = TaskStatus::failed(e.to_string());

                // Store the failed task
                {
                    let mut task_store = tasks.write().await;
                    task_store.insert(task.id.clone(), task.clone());
                }

                // Emit failed task event
                let _ = queue.send(Event::Task(task));
            }
        }
    }
}

#[async_trait]
impl<E: AgentExecutor + 'static> RequestHandler for DefaultRequestHandler<E> {
    async fn on_message_send(
        &self,
        params: MessageSendParams,
        context: Option<&ServerCallContext>,
    ) -> Result<SendMessageResponse> {
        let (task_id, context_id, _queue) = self.setup_execution(&params, context).await?;
        let message = params.message.clone();

        // Execute synchronously for non-streaming
        let ctx = ExecutionContext::new(&task_id, &context_id);
        let task = self.executor.execute(&ctx, &message).await?;

        // Store the task
        self.store_task(task.clone()).await;

        // Apply history length if configured
        let task = if let Some(ref config) = params.configuration {
            self.apply_history_length(task, config.history_length)
        } else {
            task
        };

        // Clean up queue (not needed for non-streaming)
        self.queue_manager.remove_queue(&task_id).await;

        info!(task_id = %task_id, "Message send completed");
        Ok(SendMessageResponse::Task(task))
    }

    async fn on_message_stream(
        &self,
        params: MessageSendParams,
        context: Option<&ServerCallContext>,
    ) -> Result<EventStream> {
        let (task_id, context_id, queue) = self.setup_execution(&params, context).await?;
        let message = params.message.clone();

        // Spawn execution task
        let executor = Arc::clone(&self.executor);
        let tasks = Arc::clone(&self.tasks);
        let queue_for_exec = Arc::clone(&queue);
        let task_id_clone = task_id.clone();
        let context_id_clone = context_id.clone();

        let handle = tokio::spawn(async move {
            Self::run_execution(
                executor,
                task_id_clone,
                context_id_clone,
                message,
                queue_for_exec,
                tasks,
            )
            .await;
        });

        // Store the running task handle
        {
            let mut running = self.running_tasks.write().await;
            running.insert(task_id.clone(), handle);
        }

        // Create event stream from queue subscription
        let receiver = queue.subscribe();
        let stream = futures::stream::unfold(receiver, |mut rx| async move {
            match rx.recv().await {
                Ok(event) => Some((Ok(event), rx)),
                Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Event stream lagged by {} messages", n);
                    Some((Err(A2AError::Stream(format!("Lagged by {} events", n))), rx))
                }
            }
        });

        info!(task_id = %task_id, "Started streaming message");
        Ok(Box::pin(stream))
    }

    async fn on_get_task(
        &self,
        params: TaskQueryParams,
        _context: Option<&ServerCallContext>,
    ) -> Result<Task> {
        let task = self
            .get_task_internal(&params.id)
            .await
            .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

        let task = self.apply_history_length(task, params.history_length);
        Ok(task)
    }

    async fn on_cancel_task(
        &self,
        params: TaskIdParams,
        _context: Option<&ServerCallContext>,
    ) -> Result<Task> {
        let task = self
            .get_task_internal(&params.id)
            .await
            .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

        if is_terminal_state(task.status.state) {
            return Err(JsonRpcError::task_not_cancelable(&params.id).into());
        }

        // Cancel the running task if exists
        {
            let mut running = self.running_tasks.write().await;
            if let Some(handle) = running.remove(&params.id) {
                handle.abort();
            }
        }

        // Execute cancel on the executor
        let ctx = ExecutionContext::new(&task.id, &task.context_id);
        let canceled_task = self.executor.cancel(&ctx, &params.id).await?;

        // Store the canceled task
        self.store_task(canceled_task.clone()).await;

        // Close the queue
        self.queue_manager.remove_queue(&params.id).await;

        info!(task_id = %params.id, "Task canceled");
        Ok(canceled_task)
    }

    async fn on_resubscribe(
        &self,
        params: TaskIdParams,
        _context: Option<&ServerCallContext>,
    ) -> Result<EventStream> {
        let task = self
            .get_task_internal(&params.id)
            .await
            .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

        if is_terminal_state(task.status.state) {
            return Err(JsonRpcError::invalid_params(format!(
                "Task {} is in terminal state: {:?}",
                params.id, task.status.state
            ))
            .into());
        }

        // Get the queue (must exist for active task)
        let queue = self
            .queue_manager
            .get_queue(&params.id)
            .await
            .map_err(|_| JsonRpcError::task_not_found(&params.id))?;

        let receiver = queue.subscribe();
        let stream = futures::stream::unfold(receiver, |mut rx| async move {
            match rx.recv().await {
                Ok(event) => Some((Ok(event), rx)),
                Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => None,
            }
        });

        info!(task_id = %params.id, "Resubscribed to task");
        Ok(Box::pin(stream))
    }

    async fn on_set_push_notification_config(
        &self,
        params: TaskPushNotificationConfig,
        _context: Option<&ServerCallContext>,
    ) -> Result<TaskPushNotificationConfig> {
        // Check if push notifications are supported
        let card = self.executor.agent_card();
        if !card.capabilities.push_notifications.unwrap_or(false) {
            return Err(JsonRpcError::push_notification_not_supported().into());
        }

        // Verify task exists
        self.get_task_internal(&params.task_id)
            .await
            .ok_or_else(|| JsonRpcError::task_not_found(&params.task_id))?;

        self.store_push_config(params.clone()).await;
        Ok(params)
    }

    async fn on_get_push_notification_config(
        &self,
        params: GetTaskPushNotificationConfigParams,
        _context: Option<&ServerCallContext>,
    ) -> Result<TaskPushNotificationConfig> {
        // Check if push notifications are supported
        let card = self.executor.agent_card();
        if !card.capabilities.push_notifications.unwrap_or(false) {
            return Err(JsonRpcError::push_notification_not_supported().into());
        }

        // Verify task exists
        self.get_task_internal(&params.id)
            .await
            .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

        let configs = self.push_configs.read().await;
        let task_configs = configs
            .get(&params.id)
            .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

        let config = match params.push_notification_config_id {
            Some(ref id) => task_configs
                .iter()
                .find(|c| c.push_notification_config.id.as_ref() == Some(id))
                .cloned(),
            None => task_configs.first().cloned(),
        };

        config.ok_or_else(|| {
            JsonRpcError::internal_error("Push notification config not found").into()
        })
    }

    async fn on_list_push_notification_config(
        &self,
        params: ListTaskPushNotificationConfigParams,
        _context: Option<&ServerCallContext>,
    ) -> Result<Vec<TaskPushNotificationConfig>> {
        // Check if push notifications are supported
        let card = self.executor.agent_card();
        if !card.capabilities.push_notifications.unwrap_or(false) {
            return Err(JsonRpcError::push_notification_not_supported().into());
        }

        // Verify task exists
        self.get_task_internal(&params.id)
            .await
            .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

        let configs = self.push_configs.read().await;
        Ok(configs.get(&params.id).cloned().unwrap_or_default())
    }

    async fn on_delete_push_notification_config(
        &self,
        params: DeleteTaskPushNotificationConfigParams,
        _context: Option<&ServerCallContext>,
    ) -> Result<()> {
        // Check if push notifications are supported
        let card = self.executor.agent_card();
        if !card.capabilities.push_notifications.unwrap_or(false) {
            return Err(JsonRpcError::push_notification_not_supported().into());
        }

        // Verify task exists
        self.get_task_internal(&params.id)
            .await
            .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

        let mut configs = self.push_configs.write().await;
        if let Some(task_configs) = configs.get_mut(&params.id) {
            task_configs.retain(|c| {
                c.push_notification_config.id.as_ref().map(|s| s.as_str())
                    != Some(&params.push_notification_config_id)
            });
        }

        Ok(())
    }
}

impl<E: AgentExecutor> Clone for DefaultRequestHandler<E> {
    fn clone(&self) -> Self {
        Self {
            executor: Arc::clone(&self.executor),
            tasks: Arc::clone(&self.tasks),
            queue_manager: Arc::clone(&self.queue_manager),
            push_configs: Arc::clone(&self.push_configs),
            running_tasks: Arc::clone(&self.running_tasks),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AgentCapabilities, AgentCard, Part};

    struct TestAgent {
        card: AgentCard,
    }

    #[async_trait]
    impl AgentExecutor for TestAgent {
        async fn execute(&self, ctx: &ExecutionContext, _message: &Message) -> Result<Task> {
            Ok(Task::new(&ctx.task_id, &ctx.context_id))
        }

        async fn cancel(&self, ctx: &ExecutionContext, task_id: &str) -> Result<Task> {
            let mut task = Task::new(task_id, &ctx.context_id);
            task.status = TaskStatus::new(TaskState::Canceled);
            Ok(task)
        }

        fn agent_card(&self) -> &AgentCard {
            &self.card
        }
    }

    fn create_test_agent() -> TestAgent {
        TestAgent {
            card: AgentCard::builder("Test Agent", "http://localhost:8080")
                .capabilities(AgentCapabilities {
                    streaming: Some(true),
                    push_notifications: Some(true),
                    ..Default::default()
                })
                .build(),
        }
    }

    #[tokio::test]
    async fn test_message_send() {
        let agent = create_test_agent();
        let handler = DefaultRequestHandler::new(agent);

        let message = Message::user(vec![Part::text("Hello")]);
        let params = MessageSendParams::new(message);

        let result = handler.on_message_send(params, None).await.unwrap();
        match result {
            SendMessageResponse::Task(task) => {
                assert!(!task.id.is_empty());
            }
            _ => panic!("Expected Task response"),
        }
    }

    #[tokio::test]
    async fn test_get_task() {
        let agent = create_test_agent();
        let handler = DefaultRequestHandler::new(agent);

        // First create a task
        let message = Message::user(vec![Part::text("Hello")]);
        let params = MessageSendParams::new(message);
        let result = handler.on_message_send(params, None).await.unwrap();

        let task_id = match result {
            SendMessageResponse::Task(task) => task.id,
            _ => panic!("Expected Task response"),
        };

        // Now get it
        let get_params = TaskQueryParams::new(&task_id);
        let task = handler.on_get_task(get_params, None).await.unwrap();
        assert_eq!(task.id, task_id);
    }

    #[tokio::test]
    async fn test_cancel_task() {
        let agent = create_test_agent();
        let handler = DefaultRequestHandler::new(agent);

        // Create a task
        let message = Message::user(vec![Part::text("Hello")]);
        let params = MessageSendParams::new(message);
        let result = handler.on_message_send(params, None).await.unwrap();

        let task_id = match result {
            SendMessageResponse::Task(task) => task.id,
            _ => panic!("Expected Task response"),
        };

        // Cancel it
        let cancel_params = TaskIdParams::new(&task_id);
        let canceled = handler.on_cancel_task(cancel_params, None).await.unwrap();
        assert_eq!(canceled.status.state, TaskState::Canceled);
    }
}
