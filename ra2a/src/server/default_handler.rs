//! Default implementation of the RequestHandler trait.
//!
//! This module provides `DefaultRequestHandler`, a complete implementation that
//! coordinates between the `AgentExecutor`, `TaskStore`, `QueueManager`, and
//! optional push notification components — aligned with Go's `handler.go`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use super::events::{Event, EventQueue, QueueManager};
use super::handler::{EventStream, RequestHandler, SendMessageResponse};
use super::{AgentExecutor, RequestContext};
use crate::error::{A2AError, JsonRpcError, Result};
use crate::types::{
    DeleteTaskPushConfigParams, GetTaskPushConfigParams, ListTaskPushConfigParams,
    MessageSendParams, Task, TaskIdParams, TaskPushConfig, TaskQueryParams, TaskStatus,
};

/// Default request handler for all incoming A2A requests.
///
/// Coordinates between `AgentExecutor`, task storage, `QueueManager`,
/// and optional push notification components — event-driven, aligned with Go.
pub struct DefaultRequestHandler<E: AgentExecutor> {
    executor: Arc<E>,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    queue_manager: Arc<QueueManager>,
    push_configs: Arc<RwLock<HashMap<String, Vec<TaskPushConfig>>>>,
    running_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl<E: AgentExecutor + 'static> DefaultRequestHandler<E> {
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

    /// Gets a task by ID from in-memory storage.
    async fn get_task_internal(&self, task_id: &str) -> Option<Task> {
        self.tasks.read().await.get(task_id).cloned()
    }

    /// Stores a task snapshot.
    async fn store_task(&self, task: &Task) {
        self.tasks
            .write()
            .await
            .insert(task.id.clone(), task.clone());
    }

    /// Applies history length limit to a task.
    fn apply_history_length(task: &mut Task, history_length: Option<i32>) {
        if let Some(len) = history_length {
            if len <= 0 {
                task.history = None;
            } else if let Some(ref mut history) = task.history {
                let len = len as usize;
                if history.len() > len {
                    let start = history.len() - len;
                    *history = history.drain(start..).collect();
                }
            }
        }
    }

    /// Builds a [`RequestContext`] from message params, resolving task/context IDs.
    /// Aligned with Go's `factory.loadExecutionContext` / `createNewExecutionContext`.
    async fn build_request_context(
        &self,
        params: &MessageSendParams,
    ) -> Result<(RequestContext, Arc<EventQueue>)> {
        let message = &params.message;

        // Validate message (aligned with Go's handleSendMessage)
        if message.message_id.is_empty() {
            return Err(JsonRpcError::invalid_params("message ID is required").into());
        }
        if message.parts.is_empty() {
            return Err(JsonRpcError::invalid_params("message parts is required").into());
        }

        // Resolve task_id and context_id
        let (task_id, context_id, stored_task) = match (&message.task_id, &message.context_id) {
            (Some(tid), Some(cid)) => {
                let stored = self.get_task_internal(tid).await;
                (tid.clone(), cid.clone(), stored)
            }
            (Some(tid), None) => {
                let stored = self
                    .get_task_internal(tid)
                    .await
                    .ok_or_else(|| JsonRpcError::task_not_found(tid))?;
                let cid = stored.context_id.clone();
                (tid.clone(), cid, Some(stored))
            }
            (None, Some(cid)) => (uuid::Uuid::new_v4().to_string(), cid.clone(), None),
            (None, None) => (
                uuid::Uuid::new_v4().to_string(),
                uuid::Uuid::new_v4().to_string(),
                None,
            ),
        };

        // Reject if task is in terminal state
        if let Some(ref t) = stored_task {
            if t.status.state.is_terminal() {
                return Err(JsonRpcError::invalid_params(format!(
                    "Task {} is in terminal state: {:?}",
                    task_id, t.status.state
                ))
                .into());
            }
        }

        // Store push notification config if provided
        if let Some(ref config) = params.configuration {
            if let Some(ref push_config) = config.push_notification_config {
                let task_config = TaskPushConfig {
                    task_id: task_id.clone(),
                    push_notification_config: push_config.clone(),
                };
                self.store_push_config(task_config).await;
            }
        }

        // Build RequestContext (aligned with Go's RequestContext)
        let mut ctx = RequestContext::new(&task_id, &context_id);
        ctx.message = Some(message.clone());
        ctx.stored_task = stored_task;
        ctx.metadata = params.metadata.clone();

        let queue = self.queue_manager.get_or_create_queue(&task_id).await;

        debug!(task_id = %task_id, context_id = %context_id, "Built request context");
        Ok((ctx, queue))
    }

    /// Spawns agent execution in a background task, writing events to the queue.
    fn spawn_execution(&self, ctx: RequestContext, queue: Arc<EventQueue>) {
        let executor = Arc::clone(&self.executor);
        let tasks = Arc::clone(&self.tasks);
        let task_id = ctx.task_id.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = executor.execute(&ctx, &queue).await {
                error!(error = %e, task_id = %ctx.task_id, "Agent execution failed");
                // Emit a failed task event
                let mut task = Task::new(&ctx.task_id, &ctx.context_id);
                task.status = TaskStatus::failed(e.to_string());
                tasks.write().await.insert(task.id.clone(), task.clone());
                let _ = queue.send(Event::Task(task));
            }
        });

        // Fire-and-forget; store handle for potential cancellation
        let running_tasks = Arc::clone(&self.running_tasks);
        tokio::spawn(async move {
            running_tasks.write().await.insert(task_id, handle);
        });
    }

    /// Collects events from the queue until a terminal event is received.
    /// Used for non-streaming `message/send`. Stores task snapshots along the way.
    async fn collect_result(
        &self,
        mut rx: tokio::sync::broadcast::Receiver<Event>,
    ) -> Result<SendMessageResponse> {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Persist task snapshots
                    match &event {
                        Event::Task(t) => {
                            self.store_task(t).await;
                        }
                        Event::StatusUpdate(e) => {
                            if let Some(mut t) = self.get_task_internal(&e.task_id).await {
                                t.status = TaskStatus::new(e.status.state);
                                self.store_task(&t).await;
                            }
                        }
                        _ => {}
                    }
                    if event.is_terminal() {
                        return match event {
                            Event::Task(t) => Ok(SendMessageResponse::Task(t)),
                            Event::Message(m) => Ok(SendMessageResponse::Message(m)),
                            Event::StatusUpdate(e) => {
                                // Build final task from store
                                let t = self
                                    .get_task_internal(&e.task_id)
                                    .await
                                    .unwrap_or_else(|| Task::new(&e.task_id, ""));
                                Ok(SendMessageResponse::Task(t))
                            }
                            _ => Err(A2AError::Other("Unexpected terminal event".into()).into()),
                        };
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    return Err(A2AError::Other("Event queue closed unexpectedly".into()).into());
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Event stream lagged by {} messages", n);
                }
            }
        }
    }

    /// Stores a push notification config.
    async fn store_push_config(&self, config: TaskPushConfig) {
        let mut configs = self.push_configs.write().await;
        let task_configs = configs.entry(config.task_id.clone()).or_default();

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
}

#[async_trait]
impl<E: AgentExecutor + 'static> RequestHandler for DefaultRequestHandler<E> {
    async fn on_message_send(&self, params: MessageSendParams) -> Result<SendMessageResponse> {
        let (ctx, queue) = self.build_request_context(&params).await?;
        let task_id = ctx.task_id.clone();

        // Subscribe BEFORE spawning so we don't miss any events
        let rx = queue.subscribe();
        self.spawn_execution(ctx, Arc::clone(&queue));

        // Collect events until terminal
        let mut result = self.collect_result(rx).await?;

        // Apply history length
        if let SendMessageResponse::Task(ref mut t) = result {
            if let Some(ref config) = params.configuration {
                Self::apply_history_length(t, config.history_length);
            }
        }

        self.queue_manager.remove_queue(&task_id).await;
        info!(task_id = %task_id, "Message send completed");
        Ok(result)
    }

    async fn on_message_stream(&self, params: MessageSendParams) -> Result<EventStream> {
        let (ctx, queue) = self.build_request_context(&params).await?;
        let task_id = ctx.task_id.clone();

        // Subscribe BEFORE spawning so we don't miss any events
        let receiver = queue.subscribe();
        self.spawn_execution(ctx, Arc::clone(&queue));
        let stream = futures::stream::unfold(receiver, |mut rx| async move {
            match rx.recv().await {
                Ok(event) => Some((Ok(event), rx)),
                Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Event stream lagged by {} messages", n);
                    Some((Err(A2AError::Other(format!("Lagged by {} events", n))), rx))
                }
            }
        });

        info!(task_id = %task_id, "Started streaming message");
        Ok(Box::pin(stream))
    }

    async fn on_get_task(&self, params: TaskQueryParams) -> Result<Task> {
        let mut task = self
            .get_task_internal(&params.id)
            .await
            .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

        Self::apply_history_length(&mut task, params.history_length);
        Ok(task)
    }

    async fn on_cancel_task(&self, params: TaskIdParams) -> Result<Task> {
        let task = self
            .get_task_internal(&params.id)
            .await
            .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

        if task.status.state.is_terminal() {
            return Err(JsonRpcError::task_not_cancelable(&params.id).into());
        }

        // Abort the running task if exists
        {
            let mut running = self.running_tasks.write().await;
            if let Some(handle) = running.remove(&params.id) {
                handle.abort();
            }
        }

        // Execute cancel via event queue (aligned with Go)
        let queue = self.queue_manager.get_or_create_queue(&params.id).await;
        let mut ctx = RequestContext::new(&task.id, &task.context_id);
        ctx.stored_task = Some(task);
        ctx.metadata = params.metadata.clone();

        // Subscribe BEFORE spawning so we don't miss any events
        let rx = queue.subscribe();

        let executor = Arc::clone(&self.executor);
        let q = Arc::clone(&queue);
        let tasks = Arc::clone(&self.tasks);
        let tid = params.id.clone();
        tokio::spawn(async move {
            if let Err(e) = executor.cancel(&ctx, &q).await {
                error!(error = %e, task_id = %tid, "Cancel execution failed");
                let mut t = Task::new(&tid, &ctx.context_id);
                t.status = TaskStatus::failed(e.to_string());
                tasks.write().await.insert(t.id.clone(), t.clone());
                let _ = q.send(Event::Task(t));
            }
        });

        let result = self.collect_result(rx).await?;
        self.queue_manager.remove_queue(&params.id).await;

        match result {
            SendMessageResponse::Task(t) => {
                info!(task_id = %params.id, "Task canceled");
                Ok(t)
            }
            _ => Err(A2AError::Other("Cancel did not produce a Task".into()).into()),
        }
    }

    async fn on_resubscribe(&self, params: TaskIdParams) -> Result<EventStream> {
        let task = self
            .get_task_internal(&params.id)
            .await
            .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

        if task.status.state.is_terminal() {
            return Err(JsonRpcError::invalid_params(format!(
                "Task {} is in terminal state: {:?}",
                params.id, task.status.state
            ))
            .into());
        }

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
        params: TaskPushConfig,
    ) -> Result<TaskPushConfig> {
        let card = self.executor.agent_card();
        if !card.capabilities.push_notifications.unwrap_or(false) {
            return Err(JsonRpcError::push_notification_not_supported().into());
        }
        self.get_task_internal(&params.task_id)
            .await
            .ok_or_else(|| JsonRpcError::task_not_found(&params.task_id))?;

        self.store_push_config(params.clone()).await;
        Ok(params)
    }

    async fn on_get_push_notification_config(
        &self,
        params: GetTaskPushConfigParams,
    ) -> Result<TaskPushConfig> {
        let card = self.executor.agent_card();
        if !card.capabilities.push_notifications.unwrap_or(false) {
            return Err(JsonRpcError::push_notification_not_supported().into());
        }
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
        params: ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>> {
        let card = self.executor.agent_card();
        if !card.capabilities.push_notifications.unwrap_or(false) {
            return Err(JsonRpcError::push_notification_not_supported().into());
        }
        self.get_task_internal(&params.id)
            .await
            .ok_or_else(|| JsonRpcError::task_not_found(&params.id))?;

        let configs = self.push_configs.read().await;
        Ok(configs.get(&params.id).cloned().unwrap_or_default())
    }

    async fn on_delete_push_notification_config(
        &self,
        params: DeleteTaskPushConfigParams,
    ) -> Result<()> {
        let card = self.executor.agent_card();
        if !card.capabilities.push_notifications.unwrap_or(false) {
            return Err(JsonRpcError::push_notification_not_supported().into());
        }
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

    async fn on_get_extended_agent_card(&self) -> Result<crate::types::AgentCard> {
        Ok(self.executor.agent_card().clone())
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
    use crate::types::{AgentCapabilities, AgentCard, Message, Part, TaskState};

    struct TestAgent {
        card: AgentCard,
    }

    #[async_trait]
    impl AgentExecutor for TestAgent {
        async fn execute(&self, ctx: &RequestContext, queue: &EventQueue) -> Result<()> {
            let mut task = Task::new(&ctx.task_id, &ctx.context_id);
            task.status = TaskStatus::new(TaskState::Completed);
            queue
                .send(Event::Task(task))
                .map_err(|e| A2AError::Other(e.to_string()))?;
            Ok(())
        }

        async fn cancel(&self, ctx: &RequestContext, queue: &EventQueue) -> Result<()> {
            let mut task = Task::new(&ctx.task_id, &ctx.context_id);
            task.status = TaskStatus::new(TaskState::Canceled);
            queue
                .send(Event::Task(task))
                .map_err(|e| A2AError::Other(e.to_string()))?;
            Ok(())
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

        let result = handler.on_message_send(params).await.unwrap();
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

        let message = Message::user(vec![Part::text("Hello")]);
        let params = MessageSendParams::new(message);
        let result = handler.on_message_send(params).await.unwrap();

        let task_id = match result {
            SendMessageResponse::Task(task) => task.id,
            _ => panic!("Expected Task response"),
        };

        let get_params = TaskQueryParams::new(&task_id);
        let task = handler.on_get_task(get_params).await.unwrap();
        assert_eq!(task.id, task_id);
    }

    #[tokio::test]
    async fn test_cancel_task() {
        let agent = create_test_agent();
        let handler = DefaultRequestHandler::new(agent);

        // Insert a Working task directly so it is cancelable
        let mut task = Task::new("cancel-test-id", "ctx-1");
        task.status = TaskStatus::new(TaskState::Working);
        handler.store_task(&task).await;

        let cancel_params = TaskIdParams::new("cancel-test-id");
        let canceled = handler.on_cancel_task(cancel_params).await.unwrap();
        assert_eq!(canceled.status.state, TaskState::Canceled);
    }
}
