//! Event handling components for the A2A server.
//!
//! Provides event queues for streaming task updates to clients.
//! This module implements a broadcast-based event system similar to Python's
//! `EventQueue` and `QueueManager` for managing streaming responses.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

use crate::error::{A2AError, Result};
use crate::types::{Message, Task, TaskArtifactUpdateEvent, TaskStatusUpdateEvent};

/// An event that can be sent to clients.
#[derive(Debug, Clone)]
pub enum Event {
    /// A status update event.
    StatusUpdate(TaskStatusUpdateEvent),
    /// An artifact update event.
    ArtifactUpdate(TaskArtifactUpdateEvent),
    /// A complete task snapshot.
    Task(Task),
    /// A message response.
    Message(Message),
}

impl Event {
    /// Returns the task ID from this event, if available.
    pub fn task_id(&self) -> Option<&str> {
        match self {
            Self::StatusUpdate(e) => Some(&e.task_id),
            Self::ArtifactUpdate(e) => Some(&e.task_id),
            Self::Task(t) => Some(&t.id),
            Self::Message(m) => m.task_id.as_deref(),
        }
    }

    /// Returns true if this is a final event.
    pub fn is_final(&self) -> bool {
        match self {
            Self::StatusUpdate(e) => e.r#final,
            _ => false,
        }
    }

    /// Creates an event from a task status update.
    pub fn status_update(event: TaskStatusUpdateEvent) -> Self {
        Self::StatusUpdate(event)
    }

    /// Creates an event from an artifact update.
    pub fn artifact_update(event: TaskArtifactUpdateEvent) -> Self {
        Self::ArtifactUpdate(event)
    }

    /// Creates an event from a task.
    pub fn task(task: Task) -> Self {
        Self::Task(task)
    }

    /// Creates an event from a message.
    pub fn message(message: Message) -> Self {
        Self::Message(message)
    }
}

/// A queue for sending events to a specific task's subscribers.
#[derive(Debug)]
pub struct EventQueue {
    sender: broadcast::Sender<Event>,
}

impl EventQueue {
    /// Creates a new event queue with the specified capacity.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Sends an event to all subscribers.
    pub fn send(&self, event: Event) -> Result<()> {
        self.sender
            .send(event)
            .map_err(|e| A2AError::Stream(format!("Failed to send event: {}", e)))?;
        Ok(())
    }

    /// Subscribes to events from this queue.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Returns the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new(100)
    }
}

/// Error when no queue exists for a task.
#[derive(Debug, Clone)]
pub struct NoTaskQueue {
    /// The task ID that has no queue.
    pub task_id: String,
}

impl std::fmt::Display for NoTaskQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "No event queue exists for task: {}", self.task_id)
    }
}

impl std::error::Error for NoTaskQueue {}

/// Error when a queue already exists for a task.
#[derive(Debug, Clone)]
pub struct TaskQueueExists {
    /// The task ID that already has a queue.
    pub task_id: String,
}

impl std::fmt::Display for TaskQueueExists {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Event queue already exists for task: {}", self.task_id)
    }
}

impl std::error::Error for TaskQueueExists {}

/// Manages event queues for multiple tasks.
#[derive(Debug, Default)]
pub struct QueueManager {
    queues: Arc<RwLock<HashMap<String, Arc<EventQueue>>>>,
    capacity: usize,
}

impl QueueManager {
    /// Creates a new queue manager.
    pub fn new() -> Self {
        Self::with_capacity(100)
    }

    /// Creates a new queue manager with the specified queue capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            queues: Arc::new(RwLock::new(HashMap::new())),
            capacity,
        }
    }

    /// Creates a new event queue for a task.
    pub async fn create_queue(
        &self,
        task_id: &str,
    ) -> std::result::Result<Arc<EventQueue>, TaskQueueExists> {
        let mut queues = self.queues.write().await;
        if queues.contains_key(task_id) {
            return Err(TaskQueueExists {
                task_id: task_id.to_string(),
            });
        }
        let queue = Arc::new(EventQueue::new(self.capacity));
        queues.insert(task_id.to_string(), Arc::clone(&queue));
        Ok(queue)
    }

    /// Gets an existing event queue for a task.
    pub async fn get_queue(
        &self,
        task_id: &str,
    ) -> std::result::Result<Arc<EventQueue>, NoTaskQueue> {
        let queues = self.queues.read().await;
        queues.get(task_id).cloned().ok_or(NoTaskQueue {
            task_id: task_id.to_string(),
        })
    }

    /// Gets or creates an event queue for a task.
    pub async fn get_or_create_queue(&self, task_id: &str) -> Arc<EventQueue> {
        // Try to get existing queue first
        {
            let queues = self.queues.read().await;
            if let Some(queue) = queues.get(task_id) {
                return Arc::clone(queue);
            }
        }
        // Create new queue
        let mut queues = self.queues.write().await;
        let queue = Arc::new(EventQueue::new(self.capacity));
        queues.insert(task_id.to_string(), Arc::clone(&queue));
        queue
    }

    /// Removes an event queue for a task.
    pub async fn remove_queue(&self, task_id: &str) -> Option<Arc<EventQueue>> {
        let mut queues = self.queues.write().await;
        queues.remove(task_id)
    }

    /// Returns the number of active queues.
    pub async fn queue_count(&self) -> usize {
        let queues = self.queues.read().await;
        queues.len()
    }

    /// Sends an event to a specific task's queue.
    pub async fn send_event(&self, task_id: &str, event: Event) -> Result<()> {
        let queue = self
            .get_queue(task_id)
            .await
            .map_err(|e| A2AError::Stream(format!("No queue for task {}: {}", task_id, e)))?;
        queue.send(event)
    }
}

/// An in-memory implementation of QueueManager.
pub type InMemoryQueueManager = QueueManager;

/// A consumer for processing events from a queue.
///
/// Mirrors Python's `EventConsumer` from `server/events/event_consumer.py`.
#[async_trait::async_trait]
pub trait EventConsumer: Send + Sync {
    /// Processes an event.
    async fn consume(&self, event: &Event) -> Result<()>;

    /// Called when the stream ends normally.
    async fn on_complete(&self) -> Result<()> {
        Ok(())
    }

    /// Called when an error occurs.
    async fn on_error(&self, _error: &A2AError) -> Result<()> {
        Ok(())
    }

    /// Called when a timeout occurs.
    async fn on_timeout(&self) -> Result<()> {
        Ok(())
    }

    /// Called when the consumer is interrupted/canceled.
    async fn on_interrupt(&self) -> Result<()> {
        Ok(())
    }
}

/// Helper to create an event consumer from a closure.
pub fn event_consumer_fn<F>(f: F) -> impl EventConsumer
where
    F: Fn(&Event) -> Result<()> + Send + Sync + 'static,
{
    struct FnConsumer<F>(F);

    #[async_trait::async_trait]
    impl<F> EventConsumer for FnConsumer<F>
    where
        F: Fn(&Event) -> Result<()> + Send + Sync + 'static,
    {
        async fn consume(&self, event: &Event) -> Result<()> {
            (self.0)(event)
        }
    }

    FnConsumer(f)
}

/// Runs an event consumer on a broadcast receiver until the stream ends.
pub async fn run_consumer<C: EventConsumer>(
    consumer: &C,
    mut receiver: broadcast::Receiver<Event>,
) -> Result<()> {
    loop {
        match receiver.recv().await {
            Ok(event) => {
                if let Err(e) = consumer.consume(&event).await {
                    consumer.on_error(&e).await?;
                }
            }
            Err(broadcast::error::RecvError::Closed) => {
                consumer.on_complete().await?;
                break;
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("Event consumer lagged by {} events", n);
            }
        }
    }
    Ok(())
}

/// Runs an event consumer with a timeout.
///
/// Returns Ok(true) if the consumer completed normally,
/// Ok(false) if it timed out.
pub async fn run_consumer_with_timeout<C: EventConsumer>(
    consumer: &C,
    mut receiver: broadcast::Receiver<Event>,
    timeout: std::time::Duration,
) -> Result<bool> {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            consumer.on_timeout().await?;
            return Ok(false);
        }

        match tokio::time::timeout(remaining, receiver.recv()).await {
            Ok(Ok(event)) => {
                if let Err(e) = consumer.consume(&event).await {
                    consumer.on_error(&e).await?;
                }
            }
            Ok(Err(broadcast::error::RecvError::Closed)) => {
                consumer.on_complete().await?;
                return Ok(true);
            }
            Ok(Err(broadcast::error::RecvError::Lagged(n))) => {
                tracing::warn!("Event consumer lagged by {} events", n);
            }
            Err(_) => {
                consumer.on_timeout().await?;
                return Ok(false);
            }
        }
    }
}

/// Runs an event consumer until a final event or cancellation.
///
/// Stops consuming when an event with `is_final() == true` is received
/// or when the cancellation token is triggered.
pub async fn run_consumer_until_final<C: EventConsumer>(
    consumer: &C,
    mut receiver: broadcast::Receiver<Event>,
    cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    loop {
        tokio::select! {
            biased;

            _ = async {
                let mut cancel = cancel.clone();
                loop {
                    if *cancel.borrow() {
                        break;
                    }
                    if cancel.changed().await.is_err() {
                        break;
                    }
                }
            } => {
                consumer.on_interrupt().await?;
                break;
            }

            result = receiver.recv() => {
                match result {
                    Ok(event) => {
                        let is_final = event.is_final();
                        if let Err(e) = consumer.consume(&event).await {
                            consumer.on_error(&e).await?;
                        }
                        if is_final {
                            consumer.on_complete().await?;
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        consumer.on_complete().await?;
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Event consumer lagged by {} events", n);
                    }
                }
            }
        }
    }
    Ok(())
}

/// A collecting consumer that stores all events.
#[derive(Debug, Default)]
pub struct CollectingEventConsumer {
    events: std::sync::Mutex<Vec<Event>>,
}

impl CollectingEventConsumer {
    /// Creates a new collecting consumer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns all collected events.
    pub fn events(&self) -> Vec<Event> {
        self.events.lock().unwrap().clone()
    }

    /// Returns the number of collected events.
    pub fn len(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    /// Returns true if no events have been collected.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait::async_trait]
impl EventConsumer for CollectingEventConsumer {
    async fn consume(&self, event: &Event) -> Result<()> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

/// A callback-based consumer that calls a function for each event.
pub struct CallbackEventConsumer<F>
where
    F: Fn(&Event) -> Result<()> + Send + Sync,
{
    callback: F,
}

impl<F> CallbackEventConsumer<F>
where
    F: Fn(&Event) -> Result<()> + Send + Sync,
{
    /// Creates a new callback consumer.
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

#[async_trait::async_trait]
impl<F> EventConsumer for CallbackEventConsumer<F>
where
    F: Fn(&Event) -> Result<()> + Send + Sync,
{
    async fn consume(&self, event: &Event) -> Result<()> {
        (self.callback)(event)
    }
}

/// Builder for creating event streams.
pub struct EventStreamBuilder {
    task_id: String,
    queue: Arc<EventQueue>,
}

impl EventStreamBuilder {
    /// Creates a new event stream builder.
    pub fn new(task_id: impl Into<String>, queue: Arc<EventQueue>) -> Self {
        Self {
            task_id: task_id.into(),
            queue,
        }
    }

    /// Converts to an async stream.
    pub fn into_stream(self) -> impl futures::Stream<Item = Result<Event>> {
        let receiver = self.queue.subscribe();
        futures::stream::unfold(receiver, |mut rx| async move {
            match rx.recv().await {
                Ok(event) => Some((Ok(event), rx)),
                Err(broadcast::error::RecvError::Closed) => None,
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    Some((Err(A2AError::Stream("Event stream lagged".to_string())), rx))
                }
            }
        })
    }

    /// Returns the task ID.
    pub fn task_id(&self) -> &str {
        &self.task_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_queue() {
        let queue = EventQueue::new(10);
        let mut receiver = queue.subscribe();

        let task = Task::new("task-1", "ctx-1");
        queue.send(Event::Task(task.clone())).unwrap();

        let received = receiver.recv().await.unwrap();
        match received {
            Event::Task(t) => assert_eq!(t.id, "task-1"),
            _ => panic!("Expected Task event"),
        }
    }

    #[tokio::test]
    async fn test_queue_manager() {
        let manager = QueueManager::new();

        let queue = manager.create_queue("task-1").await.unwrap();
        assert_eq!(manager.queue_count().await, 1);

        let retrieved = manager.get_queue("task-1").await.unwrap();
        assert_eq!(Arc::as_ptr(&queue), Arc::as_ptr(&retrieved));

        manager.remove_queue("task-1").await;
        assert_eq!(manager.queue_count().await, 0);
    }
}
