//! Example: A2A Server
//!
//! Run: `cargo run --example server --features server`

use async_trait::async_trait;
use ra2a::{
    error::Result,
    server::{A2AServerBuilder, AgentExecutor, Event, EventQueue, RequestContext},
    types::{AgentCard, AgentSkill, Message, Part, Task, TaskState, TaskStatus},
};

/// A simple echo agent that responds to greetings.
struct EchoAgent {
    agent_card: AgentCard,
}

impl EchoAgent {
    fn new() -> Self {
        let agent_card = AgentCard::builder("Echo Agent", "http://localhost:8080")
            .description("A simple echo agent for demonstration.")
            .version("1.0.0")
            .skill(AgentSkill::new(
                "echo",
                "Echo",
                "Echoes user messages with a greeting",
                vec!["echo".into(), "hello".into()],
            ))
            .build();
        Self { agent_card }
    }
}

#[async_trait]
impl AgentExecutor for EchoAgent {
    async fn execute(&self, ctx: &RequestContext, queue: &EventQueue) -> Result<()> {
        let input = ctx
            .message
            .as_ref()
            .and_then(ra2a::Message::text_content)
            .unwrap_or_default();

        let reply = format!("Echo: {input}");
        let task = Task::new(&ctx.task_id, &ctx.context_id).with_status(
            TaskStatus::with_message(
                TaskState::Completed,
                Message::agent(vec![Part::text(reply)]),
            ),
        );

        queue.send(Event::Task(task))?;
        Ok(())
    }

    async fn cancel(&self, ctx: &RequestContext, queue: &EventQueue) -> Result<()> {
        let task = Task::new(&ctx.task_id, &ctx.context_id)
            .with_status(TaskStatus::new(TaskState::Canceled));
        queue.send(Event::Task(task))?;
        Ok(())
    }

    fn agent_card(&self) -> &AgentCard {
        &self.agent_card
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    let server = A2AServerBuilder::new()
        .executor(EchoAgent::new())
        .port(8080)
        .build();

    println!("A2A server listening on http://localhost:8080");
    println!("Agent card: http://localhost:8080/.well-known/agent-card.json");

    server
        .serve_with_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await
}
