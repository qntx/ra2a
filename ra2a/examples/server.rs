//! Example: A2A Server
//!
//! Mount A2A routes on your own Axum router — the SDK provides composable
//! handlers, not a managed server.
//!
//! Run: `cargo run --example server --features server`

use async_trait::async_trait;
use ra2a::{
    error::Result,
    server::{AgentExecutor, Event, EventQueue, RequestContext, ServerState, a2a_router},
    types::{AgentCard, AgentSkill, Message, Part, Task, TaskState, TaskStatus},
};

struct EchoAgent;

#[async_trait]
impl AgentExecutor for EchoAgent {
    async fn execute(&self, ctx: &RequestContext, queue: &EventQueue) -> Result<()> {
        let input = ctx
            .message
            .as_ref()
            .and_then(ra2a::Message::text_content)
            .unwrap_or_default();

        let mut task = Task::new(&ctx.task_id, &ctx.context_id);
        task.status = TaskStatus::with_message(
            TaskState::Completed,
            Message::agent(vec![Part::text(format!("Echo: {input}"))]),
        );
        queue.send(Event::Task(task))?;
        Ok(())
    }

    async fn cancel(&self, ctx: &RequestContext, queue: &EventQueue) -> Result<()> {
        let mut task = Task::new(&ctx.task_id, &ctx.context_id);
        task.status = TaskStatus::new(TaskState::Canceled);
        queue.send(Event::Task(task))?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    let mut card = AgentCard::new("Echo Agent", "http://localhost:8080");
    card.description = "A simple echo agent for demonstration.".into();
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
