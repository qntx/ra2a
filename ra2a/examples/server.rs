#![allow(unused_crate_dependencies)]
//! Example: A2A Server
//!
//! Mount A2A routes on your own Axum router — the SDK provides composable
//! handlers, not a managed server.
//!
//! Run: `cargo run --example server --features server`

use std::future::Future;
use std::pin::Pin;

use ra2a::{
    error::Result,
    server::{AgentExecutor, Event, EventQueue, RequestContext, ServerState, a2a_router},
    types::{
        AgentCard, AgentInterface, AgentSkill, Message, Part, Task, TaskState, TaskStatus,
        TransportProtocol,
    },
};

struct EchoAgent;

impl AgentExecutor for EchoAgent {
    fn execute<'a>(
        &'a self,
        ctx: &'a RequestContext,
        queue: &'a EventQueue,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let input = ctx
                .message
                .as_ref()
                .and_then(Message::text_content)
                .unwrap_or_default();

            let mut task = Task::new(&ctx.task_id, &ctx.context_id);
            task.status = TaskStatus::with_message(
                TaskState::Completed,
                Message::agent(vec![Part::text(format!("Echo: {input}"))]),
            );
            queue.send(Event::Task(task))?;
            Ok(())
        })
    }

    fn cancel<'a>(
        &'a self,
        ctx: &'a RequestContext,
        queue: &'a EventQueue,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let mut task = Task::new(&ctx.task_id, &ctx.context_id);
            task.status = TaskStatus::new(TaskState::Canceled);
            queue.send(Event::Task(task))?;
            Ok(())
        })
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    let mut card = AgentCard::new(
        "Echo Agent",
        "A simple echo agent for demonstration.",
        vec![AgentInterface::new(
            "http://localhost:8080",
            TransportProtocol::new(TransportProtocol::JSONRPC),
        )],
    );
    card.skills.push(AgentSkill::new(
        "echo",
        "Echo",
        "Echoes user messages with a greeting",
        vec!["echo".into(), "hello".into()],
    ));

    let state = ServerState::from_executor(EchoAgent, card);
    let app = axum::Router::new()
        .merge(a2a_router(state))
        .route("/health", axum::routing::get(|| async { "OK" }));

    println!("A2A server listening on http://localhost:8080");
    println!("Agent card: http://localhost:8080/.well-known/agent-card.json");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await
}
