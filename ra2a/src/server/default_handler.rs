//! Default implementation of the `RequestHandler` trait.
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
use super::handler::{EventStream, RequestHandler};
use super::push::{InMemoryPushConfigStore, PushConfigStore, PushSender};
use super::task_store::{InMemoryTaskStore, TaskStore};
use super::{AgentExecutor, RequestContext, RequestContextInterceptor};
use crate::error::{A2AError, Result};
use crate::types::{
    DeleteTaskPushConfigParams, GetTaskPushConfigParams, ListTaskPushConfigParams,
    ListTasksRequest, ListTasksResponse, MessageSendParams, SendMessageResult, Task, TaskIdParams,
    TaskPushConfig, TaskQueryParams, TaskStatus,
};

/// Default request handler for all incoming A2A requests.
///
/// Coordinates between `AgentExecutor`, task storage, `QueueManager`,
/// and optional push notification components — event-driven, aligned with Go.
pub struct DefaultRequestHandler {
    executor: Arc<dyn AgentExecutor>,
    agent_card: crate::types::AgentCard,
    task_store: Arc<dyn TaskStore>,
    queue_manager: Arc<QueueManager>,
    push_config_store: Arc<dyn PushConfigStore>,
    push_sender: Option<Arc<dyn PushSender>>,
    req_context_interceptors: Vec<Arc<dyn RequestContextInterceptor>>,
    running_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl DefaultRequestHandler {
    /// Creates a new default request handler with the given executor and agent card.
    pub fn new(
        executor: impl AgentExecutor + 'static,
        agent_card: crate::types::AgentCard,
    ) -> Self {
        Self::new_from_boxed(Box::new(executor), agent_card)
    }

    /// Creates a new default request handler from a boxed executor.
    ///
    /// Used by [`HandlerBuilder`](super::HandlerBuilder) which stores
    /// the executor as `Box<dyn AgentExecutor>`.
    pub fn new_from_boxed(
        executor: Box<dyn AgentExecutor>,
        agent_card: crate::types::AgentCard,
    ) -> Self {
        Self {
            executor: Arc::from(executor),
            agent_card,
            task_store: Arc::new(InMemoryTaskStore::new()),
            queue_manager: Arc::new(QueueManager::new()),
            push_config_store: Arc::new(InMemoryPushConfigStore::new()),
            push_sender: None,
            req_context_interceptors: Vec::new(),
            running_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Sets a custom queue manager (default: in-memory).
    ///
    /// Aligned with Go's `WithEventQueueManager`.
    pub fn with_queue_manager(mut self, manager: Arc<QueueManager>) -> Self {
        self.queue_manager = manager;
        self
    }

    /// Sets a custom task store (default: in-memory).
    ///
    /// Aligned with Go's `WithClusterMode` which injects a custom `TaskStore`.
    pub fn with_task_store(mut self, store: Arc<dyn TaskStore>) -> Self {
        self.task_store = store;
        self
    }

    /// Sets a custom push config store (default: in-memory).
    pub fn with_push_config_store(mut self, store: Arc<dyn PushConfigStore>) -> Self {
        self.push_config_store = store;
        self
    }

    /// Sets a push sender for delivering notifications to client endpoints.
    pub fn with_push_sender(mut self, sender: Arc<dyn PushSender>) -> Self {
        self.push_sender = Some(sender);
        self
    }

    /// Adds a request context interceptor.
    ///
    /// Aligned with Go's `WithRequestContextInterceptor`. Interceptors run
    /// in registration order before [`RequestContext`] is passed to [`AgentExecutor`].
    pub fn with_request_context_interceptor(
        mut self,
        interceptor: Arc<dyn RequestContextInterceptor>,
    ) -> Self {
        self.req_context_interceptors.push(interceptor);
        self
    }

    /// Returns the agent card.
    #[must_use]
    pub fn agent_card(&self) -> &crate::types::AgentCard {
        &self.agent_card
    }

    /// Gets a task by ID from the task store.
    async fn get_task_internal(&self, task_id: &str) -> Option<Task> {
        self.task_store
            .get(task_id)
            .await
            .ok()
            .flatten()
            .map(|(task, _version)| task)
    }

    /// Persists a task snapshot to the task store.
    async fn store_task(&self, task: &Task) {
        if let Err(e) = self
            .task_store
            .save(task, None, crate::types::TaskVersion::MISSING)
            .await
        {
            error!(error = %e, task_id = %task.id, "Failed to persist task");
        }
    }

    /// Applies history length limit to a task.
    fn apply_history_length(task: &mut Task, history_length: Option<i32>) {
        if let Some(len) = history_length {
            if len <= 0 {
                task.history.clear();
            } else {
                let len = len as usize;
                if task.history.len() > len {
                    let start = task.history.len() - len;
                    task.history = task.history.drain(start..).collect();
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
            return Err(A2AError::InvalidParams("message ID is required".into()));
        }
        if message.parts.is_empty() {
            return Err(A2AError::InvalidParams("message parts is required".into()));
        }

        // Resolve task_id and context_id (empty string = not set, aligned with Go)
        let has_task_id = message.has_task_id();
        let has_context_id = message.has_context_id();

        let (task_id, context_id, stored_task) = match (has_task_id, has_context_id) {
            (true, true) => {
                let stored = self.get_task_internal(&message.task_id).await;
                (message.task_id.clone(), message.context_id.clone(), stored)
            }
            (true, false) => {
                let stored = self
                    .get_task_internal(&message.task_id)
                    .await
                    .ok_or_else(|| A2AError::TaskNotFound(message.task_id.clone()))?;
                let cid = stored.context_id.clone();
                (message.task_id.clone(), cid, Some(stored))
            }
            (false, true) => (
                uuid::Uuid::new_v4().to_string(),
                message.context_id.clone(),
                None,
            ),
            (false, false) => (
                uuid::Uuid::new_v4().to_string(),
                uuid::Uuid::new_v4().to_string(),
                None,
            ),
        };

        // Reject if task is in terminal state
        if let Some(ref t) = stored_task
            && t.status.state.is_terminal()
        {
            return Err(A2AError::InvalidParams(format!(
                "Task {} is in terminal state: {:?}",
                task_id, t.status.state
            )));
        }

        // Store push notification config if provided
        if let Some(ref config) = params.configuration
            && let Some(ref push_config) = config.push_notification_config
            && let Err(e) = self.save_push_config(&task_id, push_config).await
        {
            warn!(error = %e, "Failed to save push config");
        }

        // Build RequestContext (aligned with Go's RequestContext)
        let mut ctx = RequestContext::new(&task_id, &context_id);
        ctx.message = Some(message.clone());
        ctx.stored_task = stored_task;
        ctx.metadata = params.metadata.clone();

        // Run RequestContextInterceptors (aligned with Go's reqContextInterceptors)
        for interceptor in &self.req_context_interceptors {
            interceptor.intercept(&mut ctx).await?;
        }

        let queue = self.queue_manager.get_or_create_queue(&task_id).await;

        debug!(task_id = %task_id, context_id = %context_id, "Built request context");
        Ok((ctx, queue))
    }

    /// Spawns agent execution in a background task, writing events to the queue.
    fn spawn_execution(&self, ctx: RequestContext, queue: Arc<EventQueue>) {
        let executor = Arc::clone(&self.executor);
        let task_store = Arc::clone(&self.task_store);
        let task_id = ctx.task_id.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = executor.execute(&ctx, &queue).await {
                error!(error = %e, task_id = %ctx.task_id, "Agent execution failed");
                // Emit a failed task event
                let mut task = Task::new(&ctx.task_id, &ctx.context_id);
                task.status = TaskStatus::failed(e.to_string());
                let _ = task_store
                    .save(&task, None, crate::types::TaskVersion::MISSING)
                    .await;
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
    ///
    /// Implements Go's `shouldInterruptNonStreaming` logic:
    /// - If `blocking == false`, return on the first non-Message event (load task from store).
    /// - If a task enters `AuthRequired` state, interrupt and return.
    async fn collect_result(
        &self,
        mut rx: tokio::sync::broadcast::Receiver<Event>,
        params: &MessageSendParams,
    ) -> Result<SendMessageResult> {
        let is_non_blocking = params
            .configuration
            .as_ref()
            .and_then(|c| c.blocking)
            .is_some_and(|b| !b);

        let mut last_event: Option<Event> = None;

        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Persist task snapshots and send push notifications
                    match &event {
                        Event::Task(t) => {
                            self.store_task(t).await;
                            self.notify_push(t).await;
                        }
                        Event::StatusUpdate(e) => {
                            if let Some(mut t) = self.get_task_internal(&e.task_id).await {
                                t.status = TaskStatus::new(e.status.state);
                                self.store_task(&t).await;
                                self.notify_push(&t).await;
                            }
                        }
                        _ => {}
                    }

                    // Check shouldInterruptNonStreaming conditions
                    if let Some((task_id, should_interrupt)) =
                        Self::should_interrupt_non_streaming(is_non_blocking, &event)
                        && should_interrupt
                    {
                        let t = self
                            .get_task_internal(&task_id)
                            .await
                            .unwrap_or_else(|| Task::new(&task_id, ""));
                        return Ok(SendMessageResult::Task(t));
                    }

                    if event.is_terminal() {
                        return match event {
                            Event::Task(t) => Ok(SendMessageResult::Task(t)),
                            Event::Message(m) => Ok(SendMessageResult::Message(m)),
                            Event::StatusUpdate(e) => {
                                let t = self
                                    .get_task_internal(&e.task_id)
                                    .await
                                    .unwrap_or_else(|| Task::new(&e.task_id, ""));
                                Ok(SendMessageResult::Task(t))
                            }
                            _ => Err(A2AError::Other("Unexpected terminal event".into())),
                        };
                    }

                    last_event = Some(event);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    // If we have a last event that is a SendMessageResult, use it
                    let Some(event) = last_event else {
                        return Err(A2AError::Other("Event queue closed unexpectedly".into()));
                    };
                    return match event {
                        Event::Task(t) => Ok(SendMessageResult::Task(t)),
                        Event::Message(m) => Ok(SendMessageResult::Message(m)),
                        _ => self.resolve_task_from_event(&event).await,
                    };
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Event stream lagged by {} messages", n);
                }
            }
        }
    }

    /// Resolves a task from an event by looking up or creating a fallback task.
    async fn resolve_task_from_event(&self, event: &Event) -> Result<SendMessageResult> {
        let tid = event.task_id();
        if tid.is_empty() {
            return Err(A2AError::Other("Event has no task ID".into()));
        }
        let t = self
            .get_task_internal(tid)
            .await
            .unwrap_or_else(|| Task::new(tid, ""));
        Ok(SendMessageResult::Task(t))
    }

    /// Determines if a non-streaming message/send should be interrupted.
    ///
    /// Aligned with Go's `shouldInterruptNonStreaming`:
    /// - Non-blocking clients receive a result on the first non-Message task event.
    /// - Blocking clients are interrupted when auth is required.
    fn should_interrupt_non_streaming(
        is_non_blocking: bool,
        event: &Event,
    ) -> Option<(String, bool)> {
        // Non-blocking: interrupt on first non-Message event
        if is_non_blocking {
            if matches!(event, Event::Message(_)) {
                return None;
            }
            let tid = event.task_id();
            if !tid.is_empty() {
                return Some((tid.to_string(), true));
            }
            return None;
        }

        // Blocking: interrupt only when auth is required
        match event {
            Event::Task(t) if t.status.state == crate::types::TaskState::AuthRequired => {
                Some((t.id.clone(), true))
            }
            Event::StatusUpdate(e) if e.status.state == crate::types::TaskState::AuthRequired => {
                Some((e.task_id.clone(), true))
            }
            _ => None,
        }
    }

    /// Saves a push notification config via the `PushConfigStore`.
    async fn save_push_config(
        &self,
        task_id: &str,
        config: &crate::types::PushConfig,
    ) -> Result<crate::types::PushConfig> {
        self.push_config_store.save(task_id, config).await
    }

    /// Sends push notifications for a task state change.
    ///
    /// Aligned with Go's `processor.processEvent`: after each task state change,
    /// look up all push configs for this task and deliver the task snapshot.
    async fn notify_push(&self, task: &Task) {
        let sender = match &self.push_sender {
            Some(s) => Arc::clone(s),
            None => return,
        };

        let configs = match self.push_config_store.list(&task.id).await {
            Ok(c) => c,
            Err(e) => {
                debug!(error = %e, task_id = %task.id, "Failed to list push configs");
                return;
            }
        };

        for config in &configs {
            if let Err(e) = sender.send_push(config, task).await {
                warn!(error = %e, task_id = %task.id, "Push notification delivery failed");
            }
        }
    }
}

#[async_trait]
impl RequestHandler for DefaultRequestHandler {
    async fn on_message_send(&self, params: MessageSendParams) -> Result<SendMessageResult> {
        let (ctx, queue) = self.build_request_context(&params).await?;
        let task_id = ctx.task_id.clone();

        // Subscribe BEFORE spawning so we don't miss any events
        let rx = queue.subscribe();
        self.spawn_execution(ctx, Arc::clone(&queue));

        // Collect events until terminal
        let mut result = self.collect_result(rx, &params).await?;

        // Apply history length
        if let SendMessageResult::Task(ref mut t) = result
            && let Some(ref config) = params.configuration
        {
            Self::apply_history_length(t, config.history_length);
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
                    Some((Err(A2AError::Other(format!("Lagged by {n} events"))), rx))
                }
            }
        });

        info!(task_id = %task_id, "Started streaming message");
        Ok(Box::pin(stream))
    }

    async fn on_list_tasks(&self, params: ListTasksRequest) -> Result<ListTasksResponse> {
        self.task_store.list(&params).await
    }

    async fn on_get_task(&self, params: TaskQueryParams) -> Result<Task> {
        let mut task = self
            .get_task_internal(&params.id)
            .await
            .ok_or_else(|| A2AError::TaskNotFound(params.id.clone()))?;

        Self::apply_history_length(&mut task, params.history_length);
        Ok(task)
    }

    async fn on_cancel_task(&self, params: TaskIdParams) -> Result<Task> {
        let task = self
            .get_task_internal(&params.id)
            .await
            .ok_or_else(|| A2AError::TaskNotFound(params.id.clone()))?;

        if task.status.state.is_terminal() {
            return Err(A2AError::TaskNotCancelable(params.id.clone()));
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
        let task_store = Arc::clone(&self.task_store);
        let tid = params.id.clone();
        tokio::spawn(async move {
            if let Err(e) = executor.cancel(&ctx, &q).await {
                error!(error = %e, task_id = %tid, "Cancel execution failed");
                let mut t = Task::new(&tid, &ctx.context_id);
                t.status = TaskStatus::failed(e.to_string());
                let _ = task_store
                    .save(&t, None, crate::types::TaskVersion::MISSING)
                    .await;
                let _ = q.send(Event::Task(t));
            }
        });

        // Cancel always blocks until completion (no non-blocking interrupt)
        let blocking_params = MessageSendParams::new(crate::types::Message::user(vec![]));
        let result = self.collect_result(rx, &blocking_params).await?;
        self.queue_manager.remove_queue(&params.id).await;

        match result {
            SendMessageResult::Task(t) => {
                info!(task_id = %params.id, "Task canceled");
                Ok(t)
            }
            _ => Err(A2AError::Other("Cancel did not produce a Task".into())),
        }
    }

    async fn on_resubscribe(&self, params: TaskIdParams) -> Result<EventStream> {
        let task = self
            .get_task_internal(&params.id)
            .await
            .ok_or_else(|| A2AError::TaskNotFound(params.id.clone()))?;

        if task.status.state.is_terminal() {
            return Err(A2AError::InvalidParams(format!(
                "Task {} is in terminal state: {:?}",
                params.id, task.status.state
            )));
        }

        let queue = self
            .queue_manager
            .get_queue(&params.id)
            .await
            .ok_or_else(|| A2AError::TaskNotFound(params.id.clone()))?;

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

    async fn on_set_task_push_config(&self, params: TaskPushConfig) -> Result<TaskPushConfig> {
        let card = &self.agent_card;
        if !card.capabilities.push_notifications {
            return Err(A2AError::PushNotificationNotSupported);
        }
        self.get_task_internal(&params.task_id)
            .await
            .ok_or_else(|| A2AError::TaskNotFound(params.task_id.clone()))?;

        let saved = self
            .push_config_store
            .save(&params.task_id, &params.push_notification_config)
            .await?;
        Ok(TaskPushConfig {
            task_id: params.task_id,
            push_notification_config: saved,
        })
    }

    async fn on_get_task_push_config(
        &self,
        params: GetTaskPushConfigParams,
    ) -> Result<TaskPushConfig> {
        let card = &self.agent_card;
        if !card.capabilities.push_notifications {
            return Err(A2AError::PushNotificationNotSupported);
        }
        self.get_task_internal(&params.id)
            .await
            .ok_or_else(|| A2AError::TaskNotFound(params.id.clone()))?;

        let config_id = if params.push_notification_config_id.is_empty() {
            ""
        } else {
            &params.push_notification_config_id
        };
        let config = self.push_config_store.get(&params.id, config_id).await?;
        Ok(TaskPushConfig {
            task_id: params.id,
            push_notification_config: config,
        })
    }

    async fn on_list_task_push_config(
        &self,
        params: ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>> {
        let card = &self.agent_card;
        if !card.capabilities.push_notifications {
            return Err(A2AError::PushNotificationNotSupported);
        }
        self.get_task_internal(&params.id)
            .await
            .ok_or_else(|| A2AError::TaskNotFound(params.id.clone()))?;

        let configs = self.push_config_store.list(&params.id).await?;
        Ok(configs
            .into_iter()
            .map(|c| TaskPushConfig {
                task_id: params.id.clone(),
                push_notification_config: c,
            })
            .collect())
    }

    async fn on_delete_task_push_config(&self, params: DeleteTaskPushConfigParams) -> Result<()> {
        let card = &self.agent_card;
        if !card.capabilities.push_notifications {
            return Err(A2AError::PushNotificationNotSupported);
        }
        self.get_task_internal(&params.id)
            .await
            .ok_or_else(|| A2AError::TaskNotFound(params.id.clone()))?;

        self.push_config_store
            .delete(&params.id, &params.push_notification_config_id)
            .await
    }

    async fn on_get_extended_agent_card(&self) -> Result<crate::types::AgentCard> {
        Ok(self.agent_card.clone())
    }
}

impl Clone for DefaultRequestHandler {
    fn clone(&self) -> Self {
        Self {
            executor: Arc::clone(&self.executor),
            agent_card: self.agent_card.clone(),
            task_store: Arc::clone(&self.task_store),
            queue_manager: Arc::clone(&self.queue_manager),
            push_config_store: Arc::clone(&self.push_config_store),
            push_sender: self.push_sender.clone(),
            req_context_interceptors: self.req_context_interceptors.clone(),
            running_tasks: Arc::clone(&self.running_tasks),
        }
    }
}
