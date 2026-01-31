//! Result aggregator for processing streaming events.
//!
//! Consumes events from an event queue and aggregates them into
//! task updates, handling both streaming and non-streaming scenarios.

use futures::Stream;
use std::pin::Pin;

use crate::error::Result;
use crate::types::{
    Artifact, Message, Part, Task, TaskArtifactUpdateEvent, TaskState, TaskStatus,
    TaskStatusUpdateEvent,
};

/// Event types that can be processed by the aggregator.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// A status update event.
    StatusUpdate(TaskStatusUpdateEvent),
    /// An artifact update event.
    ArtifactUpdate(TaskArtifactUpdateEvent),
    /// A message event.
    Message(Message),
    /// End of stream marker.
    EndOfStream,
}

impl AgentEvent {
    /// Returns true if this is a final event.
    pub fn is_final(&self) -> bool {
        match self {
            Self::StatusUpdate(e) => e.r#final,
            Self::EndOfStream => true,
            _ => false,
        }
    }

    /// Returns true if this event indicates an interrupt (input required).
    pub fn is_interrupt(&self) -> bool {
        match self {
            Self::StatusUpdate(e) => e.status.state == TaskState::InputRequired,
            _ => false,
        }
    }
}

/// Single-task state manager for aggregating streaming events.
#[derive(Debug, Clone)]
pub struct SingleTaskManager {
    /// The current task.
    task: Task,
}

impl SingleTaskManager {
    /// Creates a new single task manager.
    pub fn new(task: Task) -> Self {
        Self { task }
    }

    /// Creates a new task with the given IDs.
    pub fn create(task_id: impl Into<String>, context_id: impl Into<String>) -> Self {
        Self::new(Task::new(task_id, context_id))
    }

    /// Returns a reference to the task.
    pub fn get_task(&self) -> &Task {
        &self.task
    }

    /// Returns a mutable reference to the task.
    pub fn get_task_mut(&mut self) -> &mut Task {
        &mut self.task
    }

    /// Takes ownership of the task.
    pub fn take_task(self) -> Task {
        self.task
    }

    /// Updates the task status.
    pub fn update_status(&mut self, status: TaskStatus) -> Result<()> {
        self.task.status = status;
        Ok(())
    }

    /// Adds a message to task history.
    pub fn add_message(&mut self, message: Message) -> Result<()> {
        self.task.add_message(message);
        Ok(())
    }

    /// Adds an artifact to the task.
    pub fn add_artifact(&mut self, artifact: Artifact) -> Result<()> {
        self.task.add_artifact(artifact);
        Ok(())
    }

    /// Appends parts to an existing artifact.
    pub fn append_to_artifact(&mut self, artifact_id: &str, parts: &[Part]) -> Result<()> {
        if let Some(ref mut artifacts) = self.task.artifacts {
            if let Some(artifact) = artifacts.iter_mut().find(|a| a.artifact_id == artifact_id) {
                artifact.parts.extend(parts.iter().cloned());
            }
        }
        Ok(())
    }
}

/// Aggregates streaming events into task state updates.
#[derive(Debug)]
pub struct ResultAggregator {
    /// The task manager for persisting task updates.
    task_manager: SingleTaskManager,
}

impl ResultAggregator {
    /// Creates a new result aggregator with a task.
    pub fn new(task: Task) -> Self {
        Self {
            task_manager: SingleTaskManager::new(task),
        }
    }

    /// Creates a new result aggregator with task IDs.
    pub fn create(task_id: impl Into<String>, context_id: impl Into<String>) -> Self {
        Self::new(Task::new(task_id, context_id))
    }

    /// Consumes all events until a final event is received.
    ///
    /// Returns the final task state after processing all events.
    pub async fn consume_all<S>(&mut self, mut stream: S) -> Result<Task>
    where
        S: Stream<Item = Result<AgentEvent>> + Unpin,
    {
        use futures::StreamExt;

        while let Some(event_result) = stream.next().await {
            let event = event_result?;
            let is_final = event.is_final();

            self.process_event(&event)?;

            if is_final {
                break;
            }
        }

        Ok(self.task_manager.get_task().clone())
    }

    /// Consumes events and emits them as a stream while updating task state.
    pub fn consume_and_emit<S>(
        self,
        stream: S,
    ) -> Pin<Box<dyn Stream<Item = Result<AgentEvent>> + Send>>
    where
        S: Stream<Item = Result<AgentEvent>> + Send + Unpin + 'static,
    {
        use futures::StreamExt;

        let output_stream = futures::stream::unfold(
            (self, stream),
            |(mut aggregator, mut input_stream)| async move {
                match input_stream.next().await {
                    Some(event_result) => match event_result {
                        Ok(event) => {
                            if let Err(e) = aggregator.process_event(&event) {
                                return Some((Err(e), (aggregator, input_stream)));
                            }
                            Some((Ok(event), (aggregator, input_stream)))
                        }
                        Err(e) => Some((Err(e), (aggregator, input_stream))),
                    },
                    None => None,
                }
            },
        );

        Box::pin(output_stream)
    }

    /// Consumes events until a final or interrupt event is received.
    pub async fn consume_until_interrupt<S>(&mut self, mut stream: S) -> Result<(AgentEvent, bool)>
    where
        S: Stream<Item = Result<AgentEvent>> + Unpin,
    {
        use futures::StreamExt;

        while let Some(event_result) = stream.next().await {
            let event = event_result?;
            let is_final = event.is_final();
            let is_interrupt = event.is_interrupt();

            self.process_event(&event)?;

            if is_final || is_interrupt {
                return Ok((event, is_interrupt));
            }
        }

        Ok((AgentEvent::EndOfStream, false))
    }

    /// Processes a single event and updates task state.
    fn process_event(&mut self, event: &AgentEvent) -> Result<()> {
        match event {
            AgentEvent::StatusUpdate(update) => {
                self.task_manager.update_status(update.status.clone())?;
                if let Some(ref msg) = update.status.message {
                    self.task_manager.add_message(msg.clone())?;
                }
            }
            AgentEvent::ArtifactUpdate(update) => {
                if update.append.unwrap_or(false) {
                    self.task_manager
                        .append_to_artifact(&update.artifact.artifact_id, &update.artifact.parts)?;
                } else {
                    self.task_manager.add_artifact(update.artifact.clone())?;
                }
            }
            AgentEvent::Message(msg) => {
                self.task_manager.add_message(msg.clone())?;
            }
            AgentEvent::EndOfStream => {}
        }
        Ok(())
    }

    /// Returns a reference to the current task.
    pub fn task(&self) -> &Task {
        self.task_manager.get_task()
    }

    /// Takes ownership of the task.
    pub fn into_task(self) -> Task {
        self.task_manager.take_task()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    #[tokio::test]
    async fn test_consume_all_single_event() {
        let mut aggregator = ResultAggregator::create("task-1", "ctx-1");

        let events = vec![Ok(AgentEvent::StatusUpdate(TaskStatusUpdateEvent::new(
            "task-1",
            "ctx-1",
            TaskStatus::completed(),
            true,
        )))];

        let stream = stream::iter(events);
        let task = aggregator.consume_all(stream).await.unwrap();

        assert_eq!(task.status.state, TaskState::Completed);
    }

    #[tokio::test]
    async fn test_consume_all_multiple_events() {
        let mut aggregator = ResultAggregator::create("task-1", "ctx-1");

        let events = vec![
            Ok(AgentEvent::StatusUpdate(TaskStatusUpdateEvent::new(
                "task-1",
                "ctx-1",
                TaskStatus::working(),
                false,
            ))),
            Ok(AgentEvent::Message(Message::agent_text("Working on it..."))),
            Ok(AgentEvent::StatusUpdate(TaskStatusUpdateEvent::new(
                "task-1",
                "ctx-1",
                TaskStatus::completed(),
                true,
            ))),
        ];

        let stream = stream::iter(events);
        let task = aggregator.consume_all(stream).await.unwrap();

        assert_eq!(task.status.state, TaskState::Completed);
        assert!(task.history.is_some());
    }

    #[tokio::test]
    async fn test_consume_until_interrupt() {
        let mut aggregator = ResultAggregator::create("task-1", "ctx-1");

        let events = vec![
            Ok(AgentEvent::StatusUpdate(TaskStatusUpdateEvent::new(
                "task-1",
                "ctx-1",
                TaskStatus::working(),
                false,
            ))),
            Ok(AgentEvent::StatusUpdate(TaskStatusUpdateEvent::new(
                "task-1",
                "ctx-1",
                TaskStatus::input_required(),
                false,
            ))),
        ];

        let stream = stream::iter(events);
        let (_, is_interrupt) = aggregator.consume_until_interrupt(stream).await.unwrap();

        assert!(is_interrupt);
    }

    #[test]
    fn test_event_is_final() {
        let final_event = AgentEvent::StatusUpdate(TaskStatusUpdateEvent::new(
            "t1",
            "c1",
            TaskStatus::completed(),
            true,
        ));
        assert!(final_event.is_final());

        let non_final = AgentEvent::StatusUpdate(TaskStatusUpdateEvent::new(
            "t1",
            "c1",
            TaskStatus::working(),
            false,
        ));
        assert!(!non_final.is_final());
    }
}
