//! `InterceptedHandler` wraps a [`RequestHandler`] to apply [`CallInterceptor`]s.
//!
//! Aligned with Go's `intercepted_handler.go`. Every handler method is wrapped
//! so that `before` interceptors run in order, then the inner handler is called,
//! then `after` interceptors run in reverse order.

use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;

use super::handler::{EventStream, RequestHandler};
use super::middleware::{CallContext, CallInterceptor, Request, RequestMeta, Response};
use crate::error::{A2AError, Result};
use crate::jsonrpc;
use crate::types::{
    AgentCard, DeleteTaskPushConfigParams, GetTaskPushConfigParams, ListTaskPushConfigParams,
    ListTasksRequest, ListTasksResponse, MessageSendParams, SendMessageResult, Task, TaskIdParams,
    TaskPushConfig, TaskQueryParams,
};

/// A [`RequestHandler`] wrapper that applies [`CallInterceptor`]s before and after
/// every handler method call.
///
/// Aligned with Go's `InterceptedHandler` in `intercepted_handler.go`.
pub struct InterceptedHandler {
    inner: Arc<dyn RequestHandler>,
    interceptors: Vec<Arc<dyn CallInterceptor>>,
}

impl InterceptedHandler {
    /// Creates a new `InterceptedHandler` wrapping the given handler.
    pub fn new(inner: Arc<dyn RequestHandler>) -> Self {
        Self {
            inner,
            interceptors: Vec::new(),
        }
    }

    /// Adds a call interceptor. Interceptors are applied in the order they are added
    /// for `before`, and in reverse order for `after`.
    pub fn with_interceptor(mut self, interceptor: Arc<dyn CallInterceptor>) -> Self {
        self.interceptors.push(interceptor);
        self
    }

    /// Runs all `before` interceptors in order.
    async fn run_before(
        &self,
        ctx: &mut CallContext,
        req: &mut Request,
    ) -> std::result::Result<(), A2AError> {
        for interceptor in &self.interceptors {
            interceptor.before(ctx, req).await?;
        }
        Ok(())
    }

    /// Runs all `after` interceptors in reverse order.
    async fn run_after(
        &self,
        ctx: &CallContext,
        resp: &mut Response,
    ) -> std::result::Result<(), A2AError> {
        for interceptor in self.interceptors.iter().rev() {
            interceptor.after(ctx, resp).await?;
        }
        Ok(())
    }

    /// Runs before-interceptors with a typed payload, then executes `handler_fn`,
    /// then runs after-interceptors. No JSON serialization involved.
    async fn intercept_unary<P, R, F>(&self, method: &str, params: P, handler_fn: F) -> Result<R>
    where
        P: Send + 'static,
        R: Send + 'static,
        F: std::future::Future<Output = Result<R>>,
    {
        let mut ctx = CallContext::new(method, RequestMeta::empty());
        let mut req = Request::new(params);
        self.run_before(&mut ctx, &mut req).await?;

        let result = handler_fn.await;

        let mut resp = match result {
            Ok(r) => Response::ok(r),
            Err(e) => Response::error(e),
        };
        self.run_after(&ctx, &mut resp).await?;

        if let Some(err) = resp.err {
            return Err(err);
        }
        // Try to extract the response payload (may have been replaced by interceptor)
        match resp.payload {
            Some(p) => match p.downcast::<R>() {
                Ok(r) => Ok(*r),
                Err(_) => Err(A2AError::Other(
                    "interceptor changed response payload type".into(),
                )),
            },
            None => Err(A2AError::Other("no response payload".into())),
        }
    }

    /// Wraps a streaming response to apply after-interceptors on each event.
    fn wrap_stream(&self, method: &'static str, stream: EventStream) -> EventStream {
        let interceptors = self.interceptors.clone();
        let wrapped = stream.then(move |event_result| {
            let interceptors = interceptors.clone();
            async move {
                match event_result {
                    Ok(event) => {
                        let mut resp = Response::ok(event);
                        let ctx = CallContext::new(method, RequestMeta::empty());
                        for interceptor in interceptors.iter().rev() {
                            interceptor.after(&ctx, &mut resp).await?;
                        }
                        if let Some(err) = resp.err {
                            return Err(err);
                        }
                        match resp
                            .payload
                            .and_then(|p| p.downcast::<super::events::Event>().ok())
                        {
                            Some(e) => Ok(*e),
                            None => Err(A2AError::Other("missing event after interceptor".into())),
                        }
                    }
                    Err(e) => Err(e),
                }
            }
        });
        Box::pin(wrapped)
    }
}

#[async_trait]
impl RequestHandler for InterceptedHandler {
    async fn on_message_send(&self, params: MessageSendParams) -> Result<SendMessageResult> {
        let inner = Arc::clone(&self.inner);
        let p = params.clone();
        self.intercept_unary(jsonrpc::METHOD_MESSAGE_SEND, params, async move {
            inner.on_message_send(p).await
        })
        .await
    }

    async fn on_message_stream(&self, params: MessageSendParams) -> Result<EventStream> {
        let mut ctx = CallContext::new(jsonrpc::METHOD_MESSAGE_STREAM, RequestMeta::empty());
        let mut req = Request::new(params.clone());
        self.run_before(&mut ctx, &mut req).await?;

        let stream = self.inner.on_message_stream(params).await?;
        Ok(self.wrap_stream(jsonrpc::METHOD_MESSAGE_STREAM, stream))
    }

    async fn on_get_task(&self, params: TaskQueryParams) -> Result<Task> {
        let inner = Arc::clone(&self.inner);
        let p = params.clone();
        self.intercept_unary(jsonrpc::METHOD_TASKS_GET, params, async move {
            inner.on_get_task(p).await
        })
        .await
    }

    async fn on_cancel_task(&self, params: TaskIdParams) -> Result<Task> {
        let inner = Arc::clone(&self.inner);
        let p = params.clone();
        self.intercept_unary(jsonrpc::METHOD_TASKS_CANCEL, params, async move {
            inner.on_cancel_task(p).await
        })
        .await
    }

    async fn on_resubscribe(&self, params: TaskIdParams) -> Result<EventStream> {
        let mut ctx = CallContext::new(jsonrpc::METHOD_TASKS_RESUBSCRIBE, RequestMeta::empty());
        let mut req = Request::new(params.clone());
        self.run_before(&mut ctx, &mut req).await?;

        let stream = self.inner.on_resubscribe(params).await?;
        Ok(self.wrap_stream(jsonrpc::METHOD_TASKS_RESUBSCRIBE, stream))
    }

    async fn on_set_task_push_config(&self, params: TaskPushConfig) -> Result<TaskPushConfig> {
        let inner = Arc::clone(&self.inner);
        let p = params.clone();
        self.intercept_unary(jsonrpc::METHOD_PUSH_CONFIG_SET, params, async move {
            inner.on_set_task_push_config(p).await
        })
        .await
    }

    async fn on_get_task_push_config(
        &self,
        params: GetTaskPushConfigParams,
    ) -> Result<TaskPushConfig> {
        let inner = Arc::clone(&self.inner);
        let p = params.clone();
        self.intercept_unary(jsonrpc::METHOD_PUSH_CONFIG_GET, params, async move {
            inner.on_get_task_push_config(p).await
        })
        .await
    }

    async fn on_list_task_push_config(
        &self,
        params: ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>> {
        let inner = Arc::clone(&self.inner);
        let p = params.clone();
        self.intercept_unary(jsonrpc::METHOD_PUSH_CONFIG_LIST, params, async move {
            inner.on_list_task_push_config(p).await
        })
        .await
    }

    async fn on_delete_task_push_config(&self, params: DeleteTaskPushConfigParams) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        let p = params.clone();
        self.intercept_unary(jsonrpc::METHOD_PUSH_CONFIG_DELETE, params, async move {
            inner.on_delete_task_push_config(p).await
        })
        .await
    }

    async fn on_list_tasks(&self, params: ListTasksRequest) -> Result<ListTasksResponse> {
        let inner = Arc::clone(&self.inner);
        let p = params.clone();
        self.intercept_unary(jsonrpc::METHOD_TASKS_LIST, params, async move {
            inner.on_list_tasks(p).await
        })
        .await
    }

    async fn on_get_extended_agent_card(&self) -> Result<AgentCard> {
        let mut ctx = CallContext::new(
            jsonrpc::METHOD_GET_EXTENDED_AGENT_CARD,
            RequestMeta::empty(),
        );
        let mut req = Request::new(());
        self.run_before(&mut ctx, &mut req).await?;

        let result = self.inner.on_get_extended_agent_card().await;

        let mut resp = match result {
            Ok(card) => Response::ok(card),
            Err(e) => Response::error(e),
        };
        self.run_after(&ctx, &mut resp).await?;

        if let Some(err) = resp.err {
            return Err(err);
        }
        match resp.payload.and_then(|p| p.downcast::<AgentCard>().ok()) {
            Some(card) => Ok(*card),
            None => Err(A2AError::Other(
                "missing agent card after interceptor".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::error::A2AError;
    use crate::server::events::Event;
    use crate::server::middleware::PassthroughInterceptor;
    use crate::types::{AgentCapabilities, AgentCard, Message, Part, TaskState, TaskStatus};

    /// A simple interceptor that counts before/after calls.
    struct CountingInterceptor {
        before_count: AtomicUsize,
        after_count: AtomicUsize,
    }

    impl CountingInterceptor {
        fn new() -> Self {
            Self {
                before_count: AtomicUsize::new(0),
                after_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl CallInterceptor for CountingInterceptor {
        async fn before(
            &self,
            _ctx: &mut CallContext,
            _req: &mut Request,
        ) -> std::result::Result<(), A2AError> {
            self.before_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn after(
            &self,
            _ctx: &CallContext,
            _resp: &mut Response,
        ) -> std::result::Result<(), A2AError> {
            self.after_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// A simple rejecting interceptor that blocks all requests.
    struct RejectingInterceptor;

    #[async_trait]
    impl CallInterceptor for RejectingInterceptor {
        async fn before(
            &self,
            _ctx: &mut CallContext,
            _req: &mut Request,
        ) -> std::result::Result<(), A2AError> {
            Err(A2AError::Unauthenticated("rejected by interceptor".into()))
        }

        async fn after(
            &self,
            _ctx: &CallContext,
            _resp: &mut Response,
        ) -> std::result::Result<(), A2AError> {
            Ok(())
        }
    }

    struct TestAgent;

    #[async_trait]
    impl crate::server::AgentExecutor for TestAgent {
        async fn execute(
            &self,
            ctx: &crate::server::RequestContext,
            queue: &crate::server::EventQueue,
        ) -> Result<()> {
            let mut task = Task::new(&ctx.task_id, &ctx.context_id);
            task.status = TaskStatus::new(TaskState::Completed);
            queue
                .send(Event::Task(task))
                .map_err(|e| A2AError::Other(e.to_string()))?;
            Ok(())
        }

        async fn cancel(
            &self,
            ctx: &crate::server::RequestContext,
            queue: &crate::server::EventQueue,
        ) -> Result<()> {
            let mut task = Task::new(&ctx.task_id, &ctx.context_id);
            task.status = TaskStatus::new(TaskState::Canceled);
            queue
                .send(Event::Task(task))
                .map_err(|e| A2AError::Other(e.to_string()))?;
            Ok(())
        }
    }

    fn create_test_handler() -> Arc<crate::server::DefaultRequestHandler> {
        let card = AgentCard::builder("Test Agent", "http://localhost:8080")
            .capabilities(AgentCapabilities {
                streaming: true,
                push_notifications: true,
                ..Default::default()
            })
            .build();
        Arc::new(crate::server::DefaultRequestHandler::new(TestAgent, card))
    }

    #[tokio::test]
    async fn test_interceptor_is_called() {
        let inner = create_test_handler();
        let counting = Arc::new(CountingInterceptor::new());
        let handler = InterceptedHandler::new(inner)
            .with_interceptor(Arc::clone(&counting) as Arc<dyn CallInterceptor>);

        let message = Message::user(vec![Part::text("Hello")]);
        let params = MessageSendParams::new(message);
        let result = handler.on_message_send(params).await;
        assert!(result.is_ok());

        assert_eq!(counting.before_count.load(Ordering::SeqCst), 1);
        assert_eq!(counting.after_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_rejecting_interceptor_blocks_request() {
        let inner = create_test_handler();
        let handler = InterceptedHandler::new(inner)
            .with_interceptor(Arc::new(RejectingInterceptor) as Arc<dyn CallInterceptor>);

        let message = Message::user(vec![Part::text("Hello")]);
        let params = MessageSendParams::new(message);
        let result = handler.on_message_send(params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_passthrough_interceptor_allows_request() {
        let inner = create_test_handler();
        let handler = InterceptedHandler::new(inner)
            .with_interceptor(Arc::new(PassthroughInterceptor) as Arc<dyn CallInterceptor>);

        let message = Message::user(vec![Part::text("Hello")]);
        let params = MessageSendParams::new(message);
        let result = handler.on_message_send(params).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_interceptors_order() {
        let inner = create_test_handler();
        let c1 = Arc::new(CountingInterceptor::new());
        let c2 = Arc::new(CountingInterceptor::new());
        let handler = InterceptedHandler::new(inner)
            .with_interceptor(Arc::clone(&c1) as Arc<dyn CallInterceptor>)
            .with_interceptor(Arc::clone(&c2) as Arc<dyn CallInterceptor>);

        let message = Message::user(vec![Part::text("Hello")]);
        let params = MessageSendParams::new(message);
        handler.on_message_send(params).await.unwrap();

        assert_eq!(c1.before_count.load(Ordering::SeqCst), 1);
        assert_eq!(c2.before_count.load(Ordering::SeqCst), 1);
        assert_eq!(c1.after_count.load(Ordering::SeqCst), 1);
        assert_eq!(c2.after_count.load(Ordering::SeqCst), 1);
    }
}
