//! `InterceptedHandler` wraps a [`RequestHandler`] to apply [`CallInterceptor`]s.
//!
//! Aligned with Go's `intercepted_handler.go`. Every handler method is wrapped
//! so that `before` interceptors run in order, then the inner handler is called,
//! then `after` interceptors run in reverse order.
//!
//! Interceptor-modified payloads are correctly propagated to the handler,
//! matching Go's `interceptBefore`/`interceptAfter` generic helpers.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures::StreamExt;

use super::{EventStream, RequestHandler};
use crate::error::{A2AError, Result};
use crate::jsonrpc;
use crate::server::middleware::{
    CallContext, CallInterceptor, Request, RequestMeta, Response, request_meta,
};
use crate::types::{
    AgentCard, CancelTaskRequest, DeleteTaskPushNotificationConfigRequest,
    GetExtendedAgentCardRequest, GetTaskPushNotificationConfigRequest, GetTaskRequest,
    ListTaskPushNotificationConfigsRequest, ListTaskPushNotificationConfigsResponse,
    ListTasksRequest, ListTasksResponse, SendMessageRequest, SendMessageResponse,
    SubscribeToTaskRequest, Task, TaskPushNotificationConfig,
};

/// A [`RequestHandler`] wrapper that applies [`CallInterceptor`]s before and after
/// every handler method call.
///
/// Aligned with Go's `InterceptedHandler` in `intercepted_handler.go`.
#[allow(missing_debug_implementations)]
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

    /// Runs before-interceptors, returning `(CallContext, modified_payload)`.
    ///
    /// Aligned with Go's `interceptBefore[T]`.
    async fn run_before_typed<P: Send + 'static>(
        &self,
        method: &str,
        params: P,
    ) -> Result<(CallContext, P)> {
        let mut ctx = CallContext::new(method, request_meta());
        let mut req = Request::new(params);

        for interceptor in &self.interceptors {
            interceptor.before(&mut ctx, &mut req).await?;
        }

        // Downcast the (possibly modified) payload back to P
        match req.downcast::<P>() {
            Ok(p) => Ok((ctx, p)),
            Err(_) => Err(A2AError::Other(
                "interceptor changed request payload type".into(),
            )),
        }
    }

    /// Runs after-interceptors in reverse order, returning the (possibly modified) result.
    ///
    /// Aligned with Go's `interceptAfter[T]`.
    async fn run_after_typed<R: Send + 'static>(
        &self,
        ctx: &CallContext,
        result: Result<R>,
    ) -> Result<R> {
        let mut resp = match result {
            Ok(r) => Response::ok(r),
            Err(e) => Response::error(e),
        };

        for interceptor in self.interceptors.iter().rev() {
            interceptor.after(ctx, &mut resp).await?;
        }

        if let Some(err) = resp.err {
            return Err(err);
        }
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
        let meta = request_meta();
        let wrapped = stream.then(move |event_result| {
            let interceptors = interceptors.clone();
            let meta = meta.clone();
            async move {
                let event = event_result?;
                apply_stream_after(&interceptors, method, meta, event).await
            }
        });
        Box::pin(wrapped)
    }
}

impl RequestHandler for InterceptedHandler {
    fn on_message_send(
        &self,
        req: SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SendMessageResponse>> + Send + '_>> {
        Box::pin(async move {
            let (ctx, req) = self
                .run_before_typed(jsonrpc::METHOD_MESSAGE_SEND, req)
                .await?;
            let result = self.inner.on_message_send(req).await;
            self.run_after_typed(&ctx, result).await
        })
    }

    fn on_message_stream(
        &self,
        req: SendMessageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + '_>> {
        Box::pin(async move {
            let (_ctx, req) = self
                .run_before_typed(jsonrpc::METHOD_MESSAGE_STREAM, req)
                .await?;
            let stream = self.inner.on_message_stream(req).await?;
            Ok(self.wrap_stream(jsonrpc::METHOD_MESSAGE_STREAM, stream))
        })
    }

    fn on_get_task(
        &self,
        req: GetTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + '_>> {
        Box::pin(async move {
            let (ctx, req) = self
                .run_before_typed(jsonrpc::METHOD_TASKS_GET, req)
                .await?;
            let result = self.inner.on_get_task(req).await;
            self.run_after_typed(&ctx, result).await
        })
    }

    fn on_cancel_task(
        &self,
        req: CancelTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Task>> + Send + '_>> {
        Box::pin(async move {
            let (ctx, req) = self
                .run_before_typed(jsonrpc::METHOD_TASKS_CANCEL, req)
                .await?;
            let result = self.inner.on_cancel_task(req).await;
            self.run_after_typed(&ctx, result).await
        })
    }

    fn on_subscribe_to_task(
        &self,
        req: SubscribeToTaskRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EventStream>> + Send + '_>> {
        Box::pin(async move {
            let (_ctx, req) = self
                .run_before_typed(jsonrpc::METHOD_TASKS_RESUBSCRIBE, req)
                .await?;
            let stream = self.inner.on_subscribe_to_task(req).await?;
            Ok(self.wrap_stream(jsonrpc::METHOD_TASKS_RESUBSCRIBE, stream))
        })
    }

    fn on_create_task_push_config(
        &self,
        req: TaskPushNotificationConfig,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + '_>> {
        Box::pin(async move {
            let (ctx, req) = self
                .run_before_typed(jsonrpc::METHOD_PUSH_CONFIG_SET, req)
                .await?;
            let result = self.inner.on_create_task_push_config(req).await;
            self.run_after_typed(&ctx, result).await
        })
    }

    fn on_get_task_push_config(
        &self,
        req: GetTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TaskPushNotificationConfig>> + Send + '_>> {
        Box::pin(async move {
            let (ctx, req) = self
                .run_before_typed(jsonrpc::METHOD_PUSH_CONFIG_GET, req)
                .await?;
            let result = self.inner.on_get_task_push_config(req).await;
            self.run_after_typed(&ctx, result).await
        })
    }

    fn on_list_task_push_configs(
        &self,
        req: ListTaskPushNotificationConfigsRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTaskPushNotificationConfigsResponse>> + Send + '_>>
    {
        Box::pin(async move {
            let (ctx, req) = self
                .run_before_typed(jsonrpc::METHOD_PUSH_CONFIG_LIST, req)
                .await?;
            let result = self.inner.on_list_task_push_configs(req).await;
            self.run_after_typed(&ctx, result).await
        })
    }

    fn on_delete_task_push_config(
        &self,
        req: DeleteTaskPushNotificationConfigRequest,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            let (ctx, req) = self
                .run_before_typed(jsonrpc::METHOD_PUSH_CONFIG_DELETE, req)
                .await?;
            let result = self.inner.on_delete_task_push_config(req).await;
            self.run_after_typed(&ctx, result).await
        })
    }

    fn on_list_tasks(
        &self,
        req: ListTasksRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ListTasksResponse>> + Send + '_>> {
        Box::pin(async move {
            let (ctx, req) = self
                .run_before_typed(jsonrpc::METHOD_TASKS_LIST, req)
                .await?;
            let result = self.inner.on_list_tasks(req).await;
            self.run_after_typed(&ctx, result).await
        })
    }

    fn on_get_extended_agent_card(
        &self,
        req: GetExtendedAgentCardRequest,
    ) -> Pin<Box<dyn Future<Output = Result<AgentCard>> + Send + '_>> {
        Box::pin(async move {
            let (ctx, req) = self
                .run_before_typed(jsonrpc::METHOD_GET_EXTENDED_AGENT_CARD, req)
                .await?;
            let result = self.inner.on_get_extended_agent_card(req).await;
            self.run_after_typed(&ctx, result).await
        })
    }
}

/// Applies after-interceptors to a single stream event.
async fn apply_stream_after(
    interceptors: &[Arc<dyn CallInterceptor>],
    method: &str,
    meta: RequestMeta,
    event: crate::server::event::Event,
) -> Result<crate::server::event::Event> {
    let mut resp = Response::ok(event);
    let ctx = CallContext::new(method, meta);
    for interceptor in interceptors.iter().rev() {
        interceptor.after(&ctx, &mut resp).await?;
    }
    if let Some(err) = resp.err {
        return Err(err);
    }
    resp.payload
        .and_then(|p| p.downcast::<crate::server::event::Event>().ok())
        .map(|e| *e)
        .ok_or_else(|| A2AError::Other("missing event after interceptor".into()))
}
