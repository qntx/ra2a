//! Server-Sent Events (SSE) streaming support for the A2A server.
//!
//! This module provides SSE-based streaming responses for real-time
//! event delivery to A2A clients.

use axum::response::IntoResponse;
use axum::response::sse::{Event as AxumSseEvent, KeepAlive, Sse};
use futures::{Stream, StreamExt};
use pin_project_lite::pin_project;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::broadcast;
use tracing::error;

use crate::types::{
    JsonRpcSuccessResponse, RequestId, StreamingMessageResult, Task, TaskArtifactUpdateEvent,
    TaskStatusUpdateEvent,
};

use super::Event;

/// Converts an `Event` to an SSE event data string.
fn event_to_sse_data(event: &Event, request_id: Option<&RequestId>) -> (String, String) {
    match event {
        Event::StatusUpdate(e) => {
            let result = StreamingMessageResult::StatusUpdate(e.clone());
            let response = JsonRpcSuccessResponse::new(request_id.cloned(), result);
            (
                "status-update".to_string(),
                serde_json::to_string(&response).unwrap_or_default(),
            )
        }
        Event::ArtifactUpdate(e) => {
            let result = StreamingMessageResult::ArtifactUpdate(e.clone());
            let response = JsonRpcSuccessResponse::new(request_id.cloned(), result);
            (
                "artifact-update".to_string(),
                serde_json::to_string(&response).unwrap_or_default(),
            )
        }
        Event::Task(t) => {
            let result = StreamingMessageResult::Task(t.clone());
            let response = JsonRpcSuccessResponse::new(request_id.cloned(), result);
            (
                "task".to_string(),
                serde_json::to_string(&response).unwrap_or_default(),
            )
        }
        Event::Message(m) => {
            let result = StreamingMessageResult::Message(m.clone());
            let response = JsonRpcSuccessResponse::new(request_id.cloned(), result);
            (
                "message".to_string(),
                serde_json::to_string(&response).unwrap_or_default(),
            )
        }
    }
}

/// Converts an `Event` to an Axum SSE event.
fn event_to_axum_sse(
    event: &Event,
    request_id: Option<&RequestId>,
) -> Result<AxumSseEvent, Infallible> {
    let (event_type, data) = event_to_sse_data(event, request_id);
    Ok(AxumSseEvent::default().event(event_type).data(data))
}

/// Type alias for SSE response streams.
pub type SseEventStream = Pin<Box<dyn Stream<Item = Result<AxumSseEvent, Infallible>> + Send>>;

/// An SSE response that can be returned from Axum handlers.
pub struct SseResponse {
    stream: SseEventStream,
}

impl SseResponse {
    /// Creates a new SSE response from a broadcast receiver.
    pub fn from_receiver(
        mut receiver: broadcast::Receiver<Event>,
        request_id: Option<RequestId>,
    ) -> Self {
        let stream = async_stream::stream! {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        yield event_to_axum_sse(&event, request_id.as_ref());
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        error!("SSE broadcast lagged by {} messages", n);
                        continue;
                    }
                }
            }
        };

        Self {
            stream: Box::pin(stream),
        }
    }

    /// Creates a new SSE response from an event stream.
    pub fn from_stream<S>(stream: S, request_id: Option<RequestId>) -> Self
    where
        S: Stream<Item = Event> + Send + 'static,
    {
        let mapped = stream.map(move |event| event_to_axum_sse(&event, request_id.as_ref()));

        Self {
            stream: Box::pin(mapped),
        }
    }
}

impl IntoResponse for SseResponse {
    fn into_response(self) -> axum::response::Response {
        Sse::new(self.stream)
            .keep_alive(KeepAlive::default())
            .into_response()
    }
}

pin_project! {
    /// A stream that emits events and terminates when a final event is received.
    pub struct TerminatingEventStream<S> {
        #[pin]
        inner: S,
        terminated: bool,
    }
}

impl<S> TerminatingEventStream<S> {
    /// Creates a new terminating event stream.
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            terminated: false,
        }
    }
}

impl<S: Stream<Item = Event>> Stream for TerminatingEventStream<S> {
    type Item = Event;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        if *this.terminated {
            return Poll::Ready(None);
        }

        match this.inner.poll_next(cx) {
            Poll::Ready(Some(event)) => {
                // Check if this is a final event
                if let Event::StatusUpdate(ref e) = event {
                    if e.r#final {
                        *this.terminated = true;
                    }
                }
                Poll::Ready(Some(event))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Builder for creating SSE streaming handlers.
#[derive(Debug)]
pub struct SseStreamBuilder {
    request_id: Option<RequestId>,
    initial_events: Vec<Event>,
}

impl SseStreamBuilder {
    /// Creates a new SSE stream builder.
    pub fn new() -> Self {
        Self {
            request_id: None,
            initial_events: Vec::new(),
        }
    }

    /// Sets the request ID for JSON-RPC responses.
    pub fn request_id(mut self, id: RequestId) -> Self {
        self.request_id = Some(id);
        self
    }

    /// Adds an initial event to be sent immediately.
    pub fn initial_event(mut self, event: Event) -> Self {
        self.initial_events.push(event);
        self
    }

    /// Builds an SSE response from a broadcast receiver.
    pub fn build_from_receiver(self, mut receiver: broadcast::Receiver<Event>) -> SseResponse {
        if self.initial_events.is_empty() {
            SseResponse::from_receiver(receiver, self.request_id)
        } else {
            // Prepend initial events to the stream
            let initial_events = self.initial_events;
            let request_id = self.request_id;
            let stream = async_stream::stream! {
                // Emit initial events first
                for event in initial_events {
                    yield event;
                }
                // Then emit events from the receiver
                loop {
                    match receiver.recv().await {
                        Ok(event) => yield event,
                        Err(broadcast::error::RecvError::Closed) => break,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    }
                }
            };
            SseResponse::from_stream(stream, request_id)
        }
    }

    /// Builds an SSE response from an event stream.
    pub fn build_from_stream<S>(self, stream: S) -> SseResponse
    where
        S: Stream<Item = Event> + Send + 'static,
    {
        if self.initial_events.is_empty() {
            SseResponse::from_stream(stream, self.request_id)
        } else {
            let initial_stream = futures::stream::iter(self.initial_events);
            let combined = initial_stream.chain(stream);
            SseResponse::from_stream(combined, self.request_id)
        }
    }
}

impl Default for SseStreamBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to create a status update event.
pub fn status_update_event(
    task_id: &str,
    context_id: &str,
    status: crate::types::TaskStatus,
    is_final: bool,
) -> Event {
    Event::StatusUpdate(TaskStatusUpdateEvent::new(
        task_id, context_id, status, is_final,
    ))
}

/// Helper function to create an artifact update event.
pub fn artifact_update_event(
    task_id: &str,
    context_id: &str,
    artifact: crate::types::Artifact,
) -> Event {
    Event::ArtifactUpdate(TaskArtifactUpdateEvent::new(task_id, context_id, artifact))
}

/// Helper function to create a task event.
pub fn task_event(task: Task) -> Event {
    Event::Task(task)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_to_sse_data() {
        let task = Task::new("task-1", "ctx-1");
        let event = Event::Task(task);
        let (event_type, data) = event_to_sse_data(&event, None);

        assert_eq!(event_type, "task");
        assert!(data.contains("task-1"));
    }

    #[test]
    fn test_sse_stream_builder() {
        let builder = SseStreamBuilder::new()
            .request_id(RequestId::String("test-1".to_string()))
            .initial_event(Event::Task(Task::new("task-1", "ctx-1")));

        assert!(builder.request_id.is_some());
        assert_eq!(builder.initial_events.len(), 1);
    }
}
