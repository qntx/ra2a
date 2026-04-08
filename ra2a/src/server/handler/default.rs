//! Default implementation of the `RequestHandler` trait.
//!
//! This module provides `DefaultRequestHandler`, a complete implementation that
//! coordinates between the `AgentExecutor`, `TaskStore`, `QueueManager`, and
//! optional push notification components — aligned with Go's `handler.go`.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use super::{EventStream, RequestHandler};
use crate::error::{A2AError, Result};
use crate::server::event::{Event, EventQueue, QueueManager};
use crate::server::push::{
    InMemoryPushNotificationConfigStore, PushNotificationConfigStore, PushSender,
};
use crate::server::task_store::TaskVersion;
use crate::server::task_store::{InMemoryTaskStore, TaskStore};
use crate::server::{AgentExecutor, RequestContext, RequestContextInterceptor};
use crate::types::{
    CancelTaskRequest, DeleteTaskPushNotificationConfigRequest, GetExtendedAgentCardRequest,
    GetTaskPushNotificationConfigRequest, GetTaskRequest, ListTaskPushNotificationConfigsRequest,
    ListTaskPushNotificationConfigsResponse, ListTasksRequest, ListTasksResponse,
    PushNotificationConfig, SendMessageRequest, SendMessageResponse, SubscribeToTaskRequest, Task,
    TaskPushNotificationConfig, TaskStatus,
};

/// Flow-control result from [`DefaultRequestHandler::process_event`].
enum EventAction {
    /// The event produced a final response to return.
    Return(SendMessageResponse),
    /// The event was processed; continue with the next event.
    Continue(Event),
}

/// Default request handler for all incoming A2A requests.
///
/// Coordinates between `AgentExecutor`, task storage, `QueueManager`,
/// and optional push notification components — event-driven, aligned with Go.
pub struct DefaultRequestHandler {
    /// Agent executor that processes requests.
    executor: Arc<dyn AgentExecutor>,
    /// The agent card describing this agent's capabilities.
    agent_card: crate::types::AgentCard,
    /// Persistent task storage backend.
    task_store: Arc<dyn TaskStore>,
    /// SSE queue manager for streaming responses.
    queue_manager: Arc<QueueManager>,
    /// Push notification configuration storage.
    push_config_store: Arc<dyn PushNotificationConfigStore>,
    /// Optional push notification sender.
    push_sender: Option<Arc<dyn PushSender>>,
    /// Request context interceptors applied before execution.
    req_context_interceptors: Vec<Arc<dyn RequestContextInterceptor>>,
    /// Tracks currently running background task handles.
    running_tasks: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
    /// Optional producer for authenticated extended agent cards.
    authenticated_card_producer: Option<Arc<dyn crate::server::AgentCardProducer>>,
}

impl std::fmt::Debug for DefaultRequestHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultRequestHandler")
            .field("agent_card", &self.agent_card.name)
            .finish_non_exhaustive()
    }
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
    #[must_use]
    pub fn new_from_boxed(
        executor: Box<dyn AgentExecutor>,
        agent_card: crate::types::AgentCard,
    ) -> Self {
        Self {
            executor: Arc::from(executor),
            agent_card,
            task_store: Arc::new(InMemoryTaskStore::new()),
            queue_manager: Arc::new(QueueManager::new()),
            push_config_store: Arc::new(InMemoryPushNotificationConfigStore::new()),
            push_sender: None,
            req_context_interceptors: Vec::new(),
            running_tasks: Arc::new(RwLock::new(HashMap::new())),
            authenticated_card_producer: None,
        }
    }

    /// Sets a custom queue manager (default: in-memory).
    ///
    /// Aligned with Go's `WithEventQueueManager`.
    #[must_use]
    pub fn with_queue_manager(mut self, manager: Arc<QueueManager>) -> Self {
        self.queue_manager = manager;
        self
    }

    /// Sets a custom task store (default: in-memory).
    ///
    /// Aligned with Go's `WithClusterMode` which injects a custom `TaskStore`.
    #[must_use]
    pub fn with_task_store(mut self, store: Arc<dyn TaskStore>) -> Self {
        self.task_store = store;
        self
    }

    /// Sets a custom push config store (default: in-memory).
    #[must_use]
    pub fn with_push_config_store(mut self, store: Arc<dyn PushNotificationConfigStore>) -> Self {
        self.push_config_store = store;
        self
    }

    /// Sets a push sender for delivering notifications to client endpoints.
    #[must_use]
    pub fn with_push_sender(mut self, sender: Arc<dyn PushSender>) -> Self {
        self.push_sender = Some(sender);
        self
    }

    /// Adds a request context interceptor.
    ///
    /// Aligned with Go's `WithRequestContextInterceptor`. Interceptors run
    /// in registration order before [`RequestContext`] is passed to [`AgentExecutor`].
    #[must_use]
    pub fn with_request_context_interceptor(
        mut self,
        interceptor: Arc<dyn RequestContextInterceptor>,
    ) -> Self {
        self.req_context_interceptors.push(interceptor);
        self
    }

    /// Sets a static extended authenticated agent card.
    ///
    /// Aligned with Go's `WithExtendedAgentCard`.
    #[must_use]
    pub fn with_extended_agent_card(mut self, card: crate::types::AgentCard) -> Self {
        self.authenticated_card_producer = Some(Arc::new(card));
        self
    }

    /// Sets a dynamic extended authenticated agent card producer.
    ///
    /// Aligned with Go's `WithExtendedAgentCardProducer`.
    #[must_use]
    pub fn with_extended_agent_card_producer(
        mut self,
        producer: Arc<dyn crate::server::AgentCardProducer>,
    ) -> Self {
        self.authenticated_card_producer = Some(producer);
        self
    }

    /// Returns the agent card.
    #[must_use]
    pub const fn agent_card(&self) -> &crate::types::AgentCard {
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
        if let Err(e) = self.task_store.save(task, None, TaskVersion::MISSING).await {
            error!(error = %e, task_id = %task.id, "Failed to persist task");
        }
    }

    /// Applies history length limit to a task.
    fn apply_history_length(task: &mut Task, history_length: Option<i32>) {
        let Some(len) = history_length else { return };
        if len <= 0 {
            task.history.clear();
            return;
        }
        let len = usize::try_from(len).unwrap_or(0);
        if task.history.len() > len {
            let start = task.history.len() - len;
            task.history = task.history.drain(start..).collect();
        }
    }

    /// Builds a [`RequestContext`] from message params, resolving task/context IDs.
    #[allow(
        clippy::cognitive_complexity,
        reason = "task/context ID resolution logic is inherently complex"
    )]
    async fn build_request_context(
        &self,
        params: &SendMessageRequest,
    ) -> Result<(RequestContext, Arc<EventQueue>)> {
        let message = &params.message;

        if message.id.is_empty() {
            return Err(A2AError::InvalidParams("message ID is required".into()));
        }
        if message.parts.is_empty() {
            return Err(A2AError::InvalidParams("message parts is required".into()));
        }

        let (task_id, context_id, stored_task) =
            match (message.task_id.as_ref(), message.context_id.as_ref()) {
                (Some(tid), Some(_)) => {
                    let tid_str = tid.as_str();
                    let stored = self.get_task_internal(tid_str).await;
                    (
                        tid_str.to_owned(),
                        message.context_id.clone().unwrap_or_default(),
                        stored,
                    )
                }
                (Some(tid), None) => {
                    let tid_str = tid.as_str();
                    let stored = self
                        .get_task_internal(tid_str)
                        .await
                        .ok_or_else(|| A2AError::TaskNotFound(tid_str.to_owned()))?;
                    let cid = stored.context_id.to_string();
                    (tid_str.to_owned(), cid, Some(stored))
                }
                (None, Some(_)) => (
                    uuid::Uuid::now_v7().to_string(),
                    message.context_id.clone().unwrap_or_default(),
                    None,
                ),
                (None, None) => (
                    uuid::Uuid::now_v7().to_string(),
                    uuid::Uuid::now_v7().to_string(),
                    None,
                ),
            };

        if let Some(ref t) = stored_task
            && t.status.state.is_terminal()
        {
            return Err(A2AError::InvalidParams(format!(
                "Task {} is in terminal state: {:?}",
                task_id, t.status.state
            )));
        }

        if let Some(ref config) = params.configuration
            && let Some(ref tpc) = config.task_push_notification_config
        {
            let push_config = PushNotificationConfig {
                id: tpc.id.clone(),
                url: tpc.url.clone(),
                token: tpc.token.clone(),
                authentication: tpc.authentication.clone(),
            };
            if let Err(e) = self.save_push_config(&task_id, &push_config).await {
                warn!(error = %e, "Failed to save push config");
            }
        }

        let mut ctx = RequestContext::new(&task_id, &context_id);
        ctx.message = Some(message.clone());
        ctx.stored_task = stored_task;
        ctx.metadata = params.metadata.clone().unwrap_or_default();
        ctx.tenant = params.tenant.clone();

        let meta = super::super::request_meta();
        for (k, v) in meta.iter() {
            ctx.service_params.insert(k.to_owned(), v.to_vec());
        }

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
                drop(task_store.save(&task, None, TaskVersion::MISSING).await);
                drop(queue.send(Event::Task(task)));
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
    /// - If `is_non_blocking`, return on the first non-Message event (load task from store).
    /// - If a task enters `AuthRequired` state, interrupt and return.
    async fn collect_result(
        &self,
        mut rx: tokio::sync::broadcast::Receiver<Event>,
        is_non_blocking: bool,
    ) -> Result<SendMessageResponse> {
        let mut last_event: Option<Event> = None;

        loop {
            match rx.recv().await {
                Ok(event) => match self.process_event(event, is_non_blocking).await? {
                    EventAction::Return(resp) => return Ok(resp),
                    EventAction::Continue(ev) => last_event = Some(ev),
                },
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    return self.finalize_on_close(last_event).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Event stream lagged by {n} messages");
                }
            }
        }
    }

    /// Persists task snapshots and sends push notifications for an event.
    async fn persist_event_snapshot(&self, event: &Event) {
        match event {
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
    }

    /// Processes a single event received in `collect_result`.
    async fn process_event(&self, event: Event, is_non_blocking: bool) -> Result<EventAction> {
        self.persist_event_snapshot(&event).await;

        if let Some((task_id, true)) = Self::should_interrupt_non_streaming(is_non_blocking, &event)
        {
            let t = self
                .get_task_internal(&task_id)
                .await
                .unwrap_or_else(|| Task::new(&task_id, ""));
            return Ok(EventAction::Return(SendMessageResponse::Task(t)));
        }

        if event.is_terminal() {
            let resp = match event {
                Event::Task(t) => SendMessageResponse::Task(t),
                Event::Message(m) => SendMessageResponse::Message(m),
                Event::StatusUpdate(e) => {
                    let t = self
                        .get_task_internal(&e.task_id)
                        .await
                        .unwrap_or_else(|| Task::new(&e.task_id, ""));
                    SendMessageResponse::Task(t)
                }
                Event::ArtifactUpdate(_) => {
                    return Err(A2AError::Other("Unexpected terminal event".into()));
                }
            };
            return Ok(EventAction::Return(resp));
        }

        Ok(EventAction::Continue(event))
    }

    /// Handles the final event when the broadcast channel closes.
    async fn finalize_on_close(&self, last_event: Option<Event>) -> Result<SendMessageResponse> {
        let Some(event) = last_event else {
            return Err(A2AError::Other("Event queue closed unexpectedly".into()));
        };
        match event {
            Event::Task(t) => Ok(SendMessageResponse::Task(t)),
            Event::Message(m) => Ok(SendMessageResponse::Message(m)),
            ref ev => self.resolve_task_from_event(ev).await,
        }
    }

    /// Resolves a task from an event by looking up or creating a fallback task.
    async fn resolve_task_from_event(&self, event: &Event) -> Result<SendMessageResponse> {
        let tid = event.task_id();
        if tid.is_empty() {
            return Err(A2AError::Other("Event has no task ID".into()));
        }
        let t = self
            .get_task_internal(tid)
            .await
            .unwrap_or_else(|| Task::new(tid, ""));
        Ok(SendMessageResponse::Task(t))
    }

    /// Executes a cancellation in a spawned task.
    async fn run_cancel(
        executor: Arc<dyn AgentExecutor>,
        ctx: RequestContext,
        q: Arc<EventQueue>,
        task_store: Arc<dyn TaskStore>,
        tid: String,
    ) {
        if let Err(e) = executor.cancel(&ctx, &q).await {
            error!(error = %e, task_id = %tid, "Cancel execution failed");
            let mut t = Task::new(&tid, &ctx.context_id);
            t.status = TaskStatus::failed(e.to_string());
            drop(task_store.save(&t, None, TaskVersion::MISSING).await);
            drop(q.send(Event::Task(t)));
        }
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
                Some((t.id.to_string(), true))
            }
            Event::StatusUpdate(e) if e.status.state == crate::types::TaskState::AuthRequired => {
                Some((e.task_id.to_string(), true))
            }
            _ => None,
        }
    }

    /// Saves a push notification config via the `PushConfigStore`.
    async fn save_push_config(
        &self,
        task_id: &str,
        config: &PushNotificationConfig,
    ) -> Result<PushNotificationConfig> {
        self.push_config_store.save(task_id, config).await
    }

    /// Validates that push notifications are supported and the task exists.
    async fn require_push_support(&self, task_id: &str) -> Result<()> {
        if !self.agent_card.supports_push_notifications() {
            return Err(A2AError::PushNotificationNotSupported);
        }
        self.get_task_internal(task_id)
            .await
            .ok_or_else(|| A2AError::TaskNotFound(task_id.to_owned()))?;
        Ok(())
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

/// Receives the next event from a broadcast channel, mapping it for `stream::unfold`.
async fn broadcast_recv_next(
    mut rx: tokio::sync::broadcast::Receiver<Event>,
) -> Option<(Result<Event>, tokio::sync::broadcast::Receiver<Event>)> {
    match rx.recv().await {
        Ok(event) => Some((Ok(event), rx)),
        Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
            Some((Err(A2AError::Other(format!("Lagged by {n} events"))), rx))
        }
    }
}

impl RequestHandler for DefaultRequestHandler {
    fn on_message_send(
        &self,
        params: SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SendMessageResponse>> + Send + '_>> {
        Box::pin(async move {
            let (ctx, queue) = self.build_request_context(&params).await?;
            let task_id = ctx.task_id.clone();

            // Subscribe BEFORE spawning so we don't miss any events
            let rx = queue.subscribe();
            self.spawn_execution(ctx, Arc::clone(&queue));

            // Collect events until terminal
            let is_non_blocking = params
                .configuration
                .as_ref()
                .is_none_or(|c| c.return_immediately);
            let mut result = self.collect_result(rx, is_non_blocking).await?;

            // Apply history length
            if let SendMessageResponse::Task(ref mut t) = result
                && let Some(ref config) = params.configuration
            {
                Self::apply_history_length(t, config.history_length);
            }

            self.queue_manager.remove_queue(&task_id).await;
            info!(task_id = %task_id, "Message send completed");
            Ok(result)
        })
    }

    fn on_message_stream(
        &self,
        params: SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + '_>> {
        Box::pin(async move {
            let (ctx, queue) = self.build_request_context(&params).await?;
            let task_id = ctx.task_id.clone();

            // Subscribe BEFORE spawning so we don't miss any events
            let receiver = queue.subscribe();
            self.spawn_execution(ctx, Arc::clone(&queue));
            let stream = futures::stream::unfold(receiver, broadcast_recv_next);

            info!(task_id = %task_id, "Started streaming message");
            let pinned: EventStream = Box::pin(stream);
            Ok(pinned)
        })
    }

    fn on_list_tasks(
        &self,
        params: ListTasksRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTasksResponse>> + Send + '_>> {
        Box::pin(async move { self.task_store.list(&params).await })
    }

    fn on_get_task(
        &self,
        params: GetTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + '_>> {
        Box::pin(async move {
            let mut task = self
                .get_task_internal(&params.id)
                .await
                .ok_or_else(|| A2AError::TaskNotFound(params.id.to_string()))?;

            Self::apply_history_length(&mut task, params.history_length);
            Ok(task)
        })
    }

    fn on_cancel_task(
        &self,
        params: CancelTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + '_>> {
        Box::pin(async move {
            let task = self
                .get_task_internal(&params.id)
                .await
                .ok_or_else(|| A2AError::TaskNotFound(params.id.to_string()))?;

            if task.status.state.is_terminal() {
                return Err(A2AError::TaskNotCancelable(params.id.to_string()));
            }

            // Abort the running task if exists
            let mut running = self.running_tasks.write().await;
            if let Some(handle) = running.remove(params.id.as_ref()) {
                handle.abort();
            }
            drop(running);

            // Execute cancel via event queue (aligned with Go)
            let queue = self
                .queue_manager
                .get_or_create_queue(params.id.as_str())
                .await;
            let mut ctx = RequestContext::new(task.id.to_string(), task.context_id.to_string());
            ctx.stored_task = Some(task);
            ctx.metadata = HashMap::new();

            let rx = queue.subscribe();

            let executor = Arc::clone(&self.executor);
            let q = Arc::clone(&queue);
            let task_store = Arc::clone(&self.task_store);
            let tid = params.id.to_string();
            tokio::spawn(Self::run_cancel(executor, ctx, q, task_store, tid));

            // Cancel always blocks until completion
            let result = self.collect_result(rx, false).await?;
            self.queue_manager.remove_queue(params.id.as_str()).await;

            match result {
                SendMessageResponse::Task(t) => {
                    info!(task_id = %params.id, "Task canceled");
                    Ok(t)
                }
                SendMessageResponse::Message(_) => {
                    Err(A2AError::Other("Cancel did not produce a Task".into()))
                }
            }
        })
    }

    fn on_subscribe_to_task(
        &self,
        params: SubscribeToTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + '_>> {
        Box::pin(async move {
            let task = self
                .get_task_internal(&params.id)
                .await
                .ok_or_else(|| A2AError::TaskNotFound(params.id.to_string()))?;

            if task.status.state.is_terminal() {
                return Err(A2AError::InvalidParams(format!(
                    "Task {} is in terminal state: {:?}",
                    params.id, task.status.state
                )));
            }

            let queue = self
                .queue_manager
                .get_queue(params.id.as_str())
                .await
                .ok_or_else(|| A2AError::TaskNotFound(params.id.to_string()))?;

            let receiver = queue.subscribe();
            let stream = futures::stream::unfold(receiver, broadcast_recv_next);

            info!(task_id = %params.id, "Resubscribed to task");
            let pinned: EventStream = Box::pin(stream);
            Ok(pinned)
        })
    }

    fn on_create_task_push_config(
        &self,
        params: TaskPushNotificationConfig,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + '_>> {
        Box::pin(async move {
            let task_id_str = params
                .task_id
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default();
            self.require_push_support(&task_id_str).await?;

            let push_config = PushNotificationConfig {
                id: params.id.clone(),
                url: params.url.clone(),
                token: params.token.clone(),
                authentication: params.authentication.clone(),
            };
            let saved = self
                .push_config_store
                .save(&task_id_str, &push_config)
                .await?;
            Ok(TaskPushNotificationConfig {
                tenant: params.tenant,
                id: saved.id,
                task_id: params.task_id,
                url: saved.url,
                token: saved.token,
                authentication: saved.authentication,
            })
        })
    }

    fn on_get_task_push_config(
        &self,
        params: GetTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + '_>> {
        Box::pin(async move {
            self.require_push_support(&params.task_id).await?;

            let config = self
                .push_config_store
                .get(&params.task_id, &params.id)
                .await?;
            Ok(TaskPushNotificationConfig {
                tenant: params.tenant,
                id: Some(params.id),
                task_id: Some(params.task_id),
                url: config.url,
                token: config.token,
                authentication: config.authentication,
            })
        })
    }

    fn on_list_task_push_configs(
        &self,
        params: ListTaskPushNotificationConfigsRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTaskPushNotificationConfigsResponse>> + Send + '_>>
    {
        Box::pin(async move {
            self.require_push_support(&params.task_id).await?;

            let configs = self.push_config_store.list(&params.task_id).await?;
            Ok(ListTaskPushNotificationConfigsResponse {
                configs: configs
                    .into_iter()
                    .enumerate()
                    .map(|(i, c)| TaskPushNotificationConfig {
                        tenant: params.tenant.clone(),
                        id: Some(c.id.clone().unwrap_or_else(|| i.to_string())),
                        task_id: Some(params.task_id.clone()),
                        url: c.url,
                        token: c.token,
                        authentication: c.authentication,
                    })
                    .collect(),
                next_page_token: None,
            })
        })
    }

    fn on_delete_task_push_config(
        &self,
        params: DeleteTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            self.require_push_support(&params.task_id).await?;

            self.push_config_store
                .delete(&params.task_id, &params.id)
                .await
        })
    }

    fn on_get_extended_agent_card(
        &self,
        _req: GetExtendedAgentCardRequest,
    ) -> Pin<Box<dyn Future<Output = Result<crate::types::AgentCard>> + Send + '_>> {
        Box::pin(async move {
            match &self.authenticated_card_producer {
                Some(producer) => producer.card().await,
                None => Err(A2AError::ExtendedCardNotConfigured),
            }
        })
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
            authenticated_card_producer: self.authenticated_card_producer.clone(),
        }
    }
}
