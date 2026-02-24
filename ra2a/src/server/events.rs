//! Event handling components for the A2A server.
//!
//! Provides event queues for streaming task updates to clients.
//! Re-exports `Event` from core types and adds broadcast-based queue
//! infrastructure for streaming responses.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{RwLock, broadcast};

use crate::error::{A2AError, Result};
// Re-export the unified Event type from core types.
pub use crate::types::Event;

/// A queue for sending events to a specific task's subscribers.
#[derive(Debug)]
pub struct EventQueue {
    sender: broadcast::Sender<Event>,
}

impl EventQueue {
    /// Creates a new event queue with the specified capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Sends an event to all subscribers.
    pub fn send(&self, event: Event) -> Result<()> {
        self.sender
            .send(event)
            .map_err(|e| A2AError::Other(format!("Failed to send event: {e}")))?;
        Ok(())
    }

    /// Subscribes to events from this queue.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Returns the number of active subscribers.
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new(100)
    }
}

/// Manages event queues for multiple tasks.
#[derive(Debug, Default)]
pub struct QueueManager {
    queues: Arc<RwLock<HashMap<String, Arc<EventQueue>>>>,
    capacity: usize,
}

impl QueueManager {
    /// Creates a new queue manager.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(100)
    }

    /// Creates a new queue manager with the specified queue capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            queues: Arc::new(RwLock::new(HashMap::new())),
            capacity,
        }
    }

    /// Creates a new event queue for a task.
    ///
    /// Returns `None` if a queue already exists for this task.
    pub async fn create_queue(&self, task_id: &str) -> Option<Arc<EventQueue>> {
        let mut queues = self.queues.write().await;
        if queues.contains_key(task_id) {
            return None;
        }
        let queue = Arc::new(EventQueue::new(self.capacity));
        queues.insert(task_id.to_string(), Arc::clone(&queue));
        Some(queue)
    }

    /// Gets an existing event queue for a task.
    pub async fn get_queue(&self, task_id: &str) -> Option<Arc<EventQueue>> {
        let queues = self.queues.read().await;
        queues.get(task_id).cloned()
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
            .ok_or_else(|| A2AError::Other(format!("No queue for task {task_id}")))?;
        queue.send(event)
    }
}
