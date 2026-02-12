//! Client wrapper that applies [`ClientCallInterceptor`]s around every call.
//!
//! Aligned with Go's `Client` which calls `interceptBefore` / `interceptAfter`
//! around each transport method invocation.

use async_trait::async_trait;

use super::middleware::{
    ClientCallInterceptor, ClientRequest, ClientResponse, CallMeta,
    run_interceptors_before, run_interceptors_after,
};
use super::{Client, EventStream};
use crate::error::Result;
use crate::types::{
    AgentCard, DeleteTaskPushConfigParams, GetTaskPushConfigParams, ListTaskPushConfigParams,
    Message, Task, TaskIdParams, TaskPushConfig, TaskQueryParams,
};

/// A [`Client`] wrapper that applies interceptors before and after each call.
///
/// Aligned with Go's `Client` struct which holds `[]CallInterceptor` and
/// invokes `interceptBefore` (in order) and `interceptAfter` (in reverse)
/// around every transport call.
///
/// # Example
/// ```ignore
/// use ra2a::client::{A2AClient, InterceptedClient, AuthInterceptor, LoggingInterceptor};
///
/// let inner = A2AClient::new("http://localhost:8080")?;
/// let client = InterceptedClient::new(inner)
///     .with_interceptor(AuthInterceptor::bearer("my-token"))
///     .with_interceptor(LoggingInterceptor::default());
/// ```
pub struct InterceptedClient<C: Client> {
    inner: C,
    interceptors: Vec<Box<dyn ClientCallInterceptor>>,
    agent_card: Option<AgentCard>,
}

impl<C: Client> InterceptedClient<C> {
    /// Wraps an existing client with interceptor support.
    pub fn new(inner: C) -> Self {
        Self {
            inner,
            interceptors: Vec::new(),
            agent_card: None,
        }
    }

    /// Adds an interceptor to the chain.
    pub fn with_interceptor<I: ClientCallInterceptor + 'static>(mut self, interceptor: I) -> Self {
        self.interceptors.push(Box::new(interceptor));
        self
    }

    /// Sets the cached agent card (used in interceptor Request.card).
    pub fn with_agent_card(mut self, card: AgentCard) -> Self {
        self.agent_card = Some(card);
        self
    }

    /// Builds a [`ClientRequest`] for the given method and payload.
    fn build_request(&self, method: &str, payload: Option<serde_json::Value>) -> ClientRequest {
        ClientRequest {
            method: method.to_string(),
            meta: CallMeta::new(),
            card: self.agent_card.clone(),
            payload,
        }
    }

    /// Builds a [`ClientResponse`] for the given method.
    fn build_response(
        &self,
        method: &str,
        payload: Option<serde_json::Value>,
        error: Option<crate::error::A2AError>,
    ) -> ClientResponse {
        ClientResponse {
            method: method.to_string(),
            meta: CallMeta::new(),
            card: self.agent_card.clone(),
            payload,
            error,
        }
    }

    /// Runs before-interceptors, executes a unary call, runs after-interceptors.
    async fn intercept_unary<T, F, Fut>(
        &self,
        method: &str,
        payload: Option<serde_json::Value>,
        call: F,
    ) -> Result<T>
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        // Before
        let mut req = self.build_request(method, payload);
        run_interceptors_before(&self.interceptors, &mut req).await?;

        // Execute
        let result = call().await;

        // After
        let (resp_payload, resp_err) = match &result {
            Ok(val) => (serde_json::to_value(val).ok(), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let mut resp = self.build_response(
            method,
            resp_payload,
            resp_err.map(crate::error::A2AError::Other),
        );
        run_interceptors_after(&self.interceptors, &mut resp).await?;

        result
    }
}

#[async_trait]
impl<C: Client + 'static> Client for InterceptedClient<C> {
    async fn send_message(&self, message: Message) -> Result<EventStream> {
        let payload = serde_json::to_value(&message).ok();
        let mut req = self.build_request("message/send", payload);
        run_interceptors_before(&self.interceptors, &mut req).await?;

        let result = self.inner.send_message(message).await;

        let mut resp = self.build_response(
            "message/send",
            None,
            result.as_ref().err().map(|e| crate::error::A2AError::Other(e.to_string())),
        );
        run_interceptors_after(&self.interceptors, &mut resp).await?;

        result
    }

    async fn get_task(&self, params: TaskQueryParams) -> Result<Task> {
        let payload = serde_json::to_value(&params).ok();
        self.intercept_unary("tasks/get", payload, || self.inner.get_task(params.clone()))
            .await
    }

    async fn cancel_task(&self, params: TaskIdParams) -> Result<Task> {
        let payload = serde_json::to_value(&params).ok();
        self.intercept_unary("tasks/cancel", payload, || {
            self.inner.cancel_task(params.clone())
        })
        .await
    }

    async fn set_task_callback(&self, config: TaskPushConfig) -> Result<TaskPushConfig> {
        let payload = serde_json::to_value(&config).ok();
        self.intercept_unary("tasks/pushNotificationConfig/set", payload, || {
            self.inner.set_task_callback(config.clone())
        })
        .await
    }

    async fn get_task_callback(&self, params: GetTaskPushConfigParams) -> Result<TaskPushConfig> {
        let payload = serde_json::to_value(&params).ok();
        self.intercept_unary("tasks/pushNotificationConfig/get", payload, || {
            self.inner.get_task_callback(params.clone())
        })
        .await
    }

    async fn resubscribe(&self, params: TaskIdParams) -> Result<EventStream> {
        let payload = serde_json::to_value(&params).ok();
        let mut req = self.build_request("tasks/resubscribe", payload);
        run_interceptors_before(&self.interceptors, &mut req).await?;

        let result = self.inner.resubscribe(params).await;

        let mut resp = self.build_response(
            "tasks/resubscribe",
            None,
            result.as_ref().err().map(|e| crate::error::A2AError::Other(e.to_string())),
        );
        run_interceptors_after(&self.interceptors, &mut resp).await?;

        result
    }

    async fn list_task_push_notification_config(
        &self,
        params: ListTaskPushConfigParams,
    ) -> Result<Vec<TaskPushConfig>> {
        let payload = serde_json::to_value(&params).ok();
        self.intercept_unary("tasks/pushNotificationConfig/list", payload, || {
            self.inner.list_task_push_notification_config(params.clone())
        })
        .await
    }

    async fn delete_task_push_notification_config(
        &self,
        params: DeleteTaskPushConfigParams,
    ) -> Result<()> {
        let payload = serde_json::to_value(&params).ok();
        let mut req = self.build_request("tasks/pushNotificationConfig/delete", payload);
        run_interceptors_before(&self.interceptors, &mut req).await?;

        let result = self.inner.delete_task_push_notification_config(params).await;

        let mut resp = self.build_response(
            "tasks/pushNotificationConfig/delete",
            None,
            result.as_ref().err().map(|e| crate::error::A2AError::Other(e.to_string())),
        );
        run_interceptors_after(&self.interceptors, &mut resp).await?;

        result
    }

    async fn get_agent_card(&self) -> Result<AgentCard> {
        let mut req = self.build_request("agent/getAuthenticatedExtendedCard", None);
        run_interceptors_before(&self.interceptors, &mut req).await?;

        let result = self.inner.get_agent_card().await;

        let (resp_payload, resp_err) = match &result {
            Ok(card) => (serde_json::to_value(card).ok(), None),
            Err(e) => (None, Some(crate::error::A2AError::Other(e.to_string()))),
        };
        let mut resp = self.build_response(
            "agent/getAuthenticatedExtendedCard",
            resp_payload,
            resp_err,
        );
        run_interceptors_after(&self.interceptors, &mut resp).await?;

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::middleware::ClientCallInterceptor;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // A mock client for testing
    struct MockClient;

    #[async_trait]
    impl Client for MockClient {
        async fn send_message(&self, _message: Message) -> Result<EventStream> {
            Err(crate::error::A2AError::Other("mock".into()))
        }
        async fn get_task(&self, params: TaskQueryParams) -> Result<Task> {
            Ok(Task::new(&params.id, "ctx-1"))
        }
        async fn cancel_task(&self, params: TaskIdParams) -> Result<Task> {
            Ok(Task::new(&params.id, "ctx-1"))
        }
        async fn set_task_callback(&self, config: TaskPushConfig) -> Result<TaskPushConfig> {
            Ok(config)
        }
        async fn get_task_callback(
            &self,
            _params: GetTaskPushConfigParams,
        ) -> Result<TaskPushConfig> {
            Err(crate::error::A2AError::Other("mock".into()))
        }
        async fn resubscribe(&self, _params: TaskIdParams) -> Result<EventStream> {
            Err(crate::error::A2AError::Other("mock".into()))
        }
        async fn list_task_push_notification_config(
            &self,
            _params: ListTaskPushConfigParams,
        ) -> Result<Vec<TaskPushConfig>> {
            Ok(vec![])
        }
        async fn delete_task_push_notification_config(
            &self,
            _params: DeleteTaskPushConfigParams,
        ) -> Result<()> {
            Ok(())
        }
        async fn get_agent_card(&self) -> Result<AgentCard> {
            Ok(AgentCard::builder("Mock Agent", "http://localhost").build())
        }
    }

    struct CountingInterceptor {
        before_count: Arc<AtomicUsize>,
        after_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ClientCallInterceptor for CountingInterceptor {
        async fn before(&self, _req: &mut super::ClientRequest) -> Result<()> {
            self.before_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn after(&self, _resp: &mut super::ClientResponse) -> Result<()> {
            self.after_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_intercepted_client_calls_interceptors() {
        let before = Arc::new(AtomicUsize::new(0));
        let after = Arc::new(AtomicUsize::new(0));

        let client = InterceptedClient::new(MockClient)
            .with_interceptor(CountingInterceptor {
                before_count: Arc::clone(&before),
                after_count: Arc::clone(&after),
            });

        // get_task should trigger before + after
        let _ = client.get_task(TaskQueryParams::new("t1")).await;
        assert_eq!(before.load(Ordering::SeqCst), 1);
        assert_eq!(after.load(Ordering::SeqCst), 1);

        // cancel_task should trigger before + after
        let _ = client.cancel_task(TaskIdParams::new("t1")).await;
        assert_eq!(before.load(Ordering::SeqCst), 2);
        assert_eq!(after.load(Ordering::SeqCst), 2);

        // get_agent_card should trigger before + after
        let _ = client.get_agent_card().await;
        assert_eq!(before.load(Ordering::SeqCst), 3);
        assert_eq!(after.load(Ordering::SeqCst), 3);
    }
}
