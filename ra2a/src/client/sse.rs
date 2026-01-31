//! Simplified Server-Sent Events (SSE) streaming support for the A2A client.
//!
//! This module provides basic SSE parsing for real-time event updates
//! from A2A agents.

use futures::Stream;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::{debug, warn};

use crate::error::{A2AError, Result};
use crate::types::{
    Message, StreamingMessageResult, Task, TaskArtifactUpdateEvent, TaskStatusUpdateEvent,
};

use super::{ClientEvent, UpdateEvent};

/// SSE event types as defined by the A2A protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseEventType {
    /// A complete message event.
    Message,
    /// A status update event.
    StatusUpdate,
    /// An artifact update event.
    ArtifactUpdate,
    /// A task completion event.
    Task,
    /// An error event.
    Error,
    /// Unknown event type.
    Unknown(String),
}

impl From<&str> for SseEventType {
    fn from(s: &str) -> Self {
        match s {
            "message" => Self::Message,
            "status-update" | "statusUpdate" => Self::StatusUpdate,
            "artifact-update" | "artifactUpdate" => Self::ArtifactUpdate,
            "task" => Self::Task,
            "error" => Self::Error,
            other => Self::Unknown(other.to_string()),
        }
    }
}

/// A parsed SSE event from the A2A server.
#[derive(Debug, Clone)]
pub struct A2ASseEvent {
    /// The event type.
    pub event_type: SseEventType,
    /// The raw JSON data.
    pub data: String,
    /// Optional event ID.
    pub id: Option<String>,
}

impl A2ASseEvent {
    /// Parses the event data into a `ClientEvent`.
    pub fn into_client_event(self) -> Result<ClientEvent> {
        match self.event_type {
            SseEventType::Message => {
                let msg: Message = serde_json::from_str(&self.data)?;
                Ok(ClientEvent::Message(msg))
            }
            SseEventType::Task => {
                let task: Task = serde_json::from_str(&self.data)?;
                Ok(ClientEvent::TaskUpdate { task, update: None })
            }
            SseEventType::StatusUpdate => {
                let event: TaskStatusUpdateEvent = serde_json::from_str(&self.data)?;
                let task =
                    Task::new(&event.task_id, &event.context_id).with_status(event.status.clone());
                Ok(ClientEvent::TaskUpdate {
                    task,
                    update: Some(UpdateEvent::Status(event)),
                })
            }
            SseEventType::ArtifactUpdate => {
                let event: TaskArtifactUpdateEvent = serde_json::from_str(&self.data)?;
                let task = Task::new(&event.task_id, &event.context_id);
                Ok(ClientEvent::TaskUpdate {
                    task,
                    update: Some(UpdateEvent::Artifact(event)),
                })
            }
            SseEventType::Error => Err(A2AError::Stream(format!("Server error: {}", self.data))),
            SseEventType::Unknown(t) => {
                warn!("Unknown SSE event type: {}", t);
                // Try to parse as a generic streaming result
                if let Ok(result) = serde_json::from_str::<StreamingMessageResult>(&self.data) {
                    match result {
                        StreamingMessageResult::Task(task) => {
                            Ok(ClientEvent::TaskUpdate { task, update: None })
                        }
                        StreamingMessageResult::Message(msg) => Ok(ClientEvent::Message(msg)),
                        StreamingMessageResult::StatusUpdate(event) => {
                            let task = Task::new(&event.task_id, &event.context_id)
                                .with_status(event.status.clone());
                            Ok(ClientEvent::TaskUpdate {
                                task,
                                update: Some(UpdateEvent::Status(event)),
                            })
                        }
                        StreamingMessageResult::ArtifactUpdate(event) => {
                            let task = Task::new(&event.task_id, &event.context_id);
                            Ok(ClientEvent::TaskUpdate {
                                task,
                                update: Some(UpdateEvent::Artifact(event)),
                            })
                        }
                    }
                } else {
                    Err(A2AError::Stream(format!("Unknown event type: {}", t)))
                }
            }
        }
    }
}

/// Parses SSE lines into events.
///
/// This is a simple SSE parser that handles the standard SSE format:
/// - `event: <type>` sets the event type
/// - `data: <json>` provides the event data
/// - Empty line terminates the event
pub fn parse_sse_line(
    line: &str,
    current_event: &mut Option<String>,
    current_data: &mut String,
) -> Option<A2ASseEvent> {
    let line = line.trim();

    if line.is_empty() {
        // Empty line means end of event
        if !current_data.is_empty() {
            let event_type = current_event
                .take()
                .unwrap_or_else(|| "message".to_string());
            let data = std::mem::take(current_data);
            return Some(A2ASseEvent {
                event_type: SseEventType::from(event_type.as_str()),
                data,
                id: None,
            });
        }
        return None;
    }

    if let Some(event_name) = line.strip_prefix("event:") {
        *current_event = Some(event_name.trim().to_string());
    } else if let Some(data) = line.strip_prefix("data:") {
        if !current_data.is_empty() {
            current_data.push('\n');
        }
        current_data.push_str(data.trim());
    } else if line.starts_with(':') {
        // Comment, ignore
    } else if let Some(id) = line.strip_prefix("id:") {
        debug!("SSE event ID: {}", id.trim());
    }

    None
}

pin_project! {
    /// A stream that parses SSE text lines into client events.
    pub struct SseLineStream<S> {
        #[pin]
        inner: S,
        current_event: Option<String>,
        current_data: String,
        closed: bool,
    }
}

impl<S> SseLineStream<S> {
    /// Creates a new SSE line stream.
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            current_event: None,
            current_data: String::new(),
            closed: false,
        }
    }
}

impl<S, E> Stream for SseLineStream<S>
where
    S: Stream<Item = std::result::Result<String, E>>,
    E: std::fmt::Display,
{
    type Item = Result<ClientEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if *this.closed {
            return Poll::Ready(None);
        }

        loop {
            match this.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(line))) => {
                    if let Some(event) =
                        parse_sse_line(&line, this.current_event, this.current_data)
                    {
                        match event.into_client_event() {
                            Ok(client_event) => return Poll::Ready(Some(Ok(client_event))),
                            Err(e) => return Poll::Ready(Some(Err(e))),
                        }
                    }
                    // Continue reading more lines
                }
                Poll::Ready(Some(Err(e))) => {
                    *this.closed = true;
                    return Poll::Ready(Some(Err(A2AError::Stream(e.to_string()))));
                }
                Poll::Ready(None) => {
                    *this.closed = true;
                    // Process any remaining data
                    if !this.current_data.is_empty() {
                        let event_type = this
                            .current_event
                            .take()
                            .unwrap_or_else(|| "message".to_string());
                        let data = std::mem::take(this.current_data);
                        let event = A2ASseEvent {
                            event_type: SseEventType::from(event_type.as_str()),
                            data,
                            id: None,
                        };
                        match event.into_client_event() {
                            Ok(client_event) => return Poll::Ready(Some(Ok(client_event))),
                            Err(e) => return Poll::Ready(Some(Err(e))),
                        }
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_event_type_parsing() {
        assert_eq!(SseEventType::from("message"), SseEventType::Message);
        assert_eq!(
            SseEventType::from("status-update"),
            SseEventType::StatusUpdate
        );
        assert_eq!(
            SseEventType::from("statusUpdate"),
            SseEventType::StatusUpdate
        );
        assert_eq!(
            SseEventType::from("artifact-update"),
            SseEventType::ArtifactUpdate
        );
        assert_eq!(SseEventType::from("task"), SseEventType::Task);
        assert_eq!(SseEventType::from("error"), SseEventType::Error);
        assert_eq!(
            SseEventType::from("custom"),
            SseEventType::Unknown("custom".to_string())
        );
    }

    #[test]
    fn test_parse_sse_line() {
        let mut current_event = None;
        let mut current_data = String::new();

        // Set event type
        assert!(parse_sse_line("event: task", &mut current_event, &mut current_data).is_none());
        assert_eq!(current_event, Some("task".to_string()));

        // Set data
        assert!(
            parse_sse_line(
                "data: {\"id\":\"1\"}",
                &mut current_event,
                &mut current_data
            )
            .is_none()
        );
        assert_eq!(current_data, "{\"id\":\"1\"}");

        // Empty line triggers event
        let event = parse_sse_line("", &mut current_event, &mut current_data);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.event_type, SseEventType::Task);
        assert_eq!(event.data, "{\"id\":\"1\"}");
    }
}
