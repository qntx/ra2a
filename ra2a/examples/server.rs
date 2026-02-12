//! Example: A2A Server Implementation
//!
//! This example demonstrates how to build an A2A agent server using the SDK.

#![allow(unused_imports)]

use std::sync::Arc;

use async_trait::async_trait;
use ra2a::{
    error::Result,
    server::{
        A2AServer, A2AServerBuilder, AgentExecutor, Event, EventQueue, RequestContext, ServerConfig,
    },
    types::{AgentCapabilities, AgentCard, AgentSkill, Message, Part, Task, TaskState, TaskStatus},
};
use tokio::signal;

/// A simple "Hello World" agent executor.
struct HelloWorldExecutor {
    agent_card: AgentCard,
}

impl HelloWorldExecutor {
    fn new() -> Self {
        // Build the agent card describing this agent's capabilities
        let agent_card = AgentCard::builder("Hello World Agent", "http://localhost:8080")
            .description("A simple agent that greets users and answers basic questions.")
            .version("1.0.0")
            .capabilities(AgentCapabilities {
                streaming: false,
                push_notifications: false,
                state_transition_history: false,
                extensions: vec![],
            })
            .skill(AgentSkill::new(
                "greeting",
                "Greeting",
                "Greets users in a friendly manner",
                vec!["greeting".to_string(), "hello".to_string()],
            ))
            .skill(AgentSkill::new(
                "help",
                "Help",
                "Provides help and guidance",
                vec!["help".to_string(), "assistance".to_string()],
            ))
            .build();

        Self { agent_card }
    }
}

#[async_trait]
impl AgentExecutor for HelloWorldExecutor {
    async fn execute(&self, ctx: &RequestContext, queue: &EventQueue) -> Result<()> {
        // Get the text content from the triggering message
        let user_text = ctx
            .message
            .as_ref()
            .and_then(ra2a::Message::text_content)
            .unwrap_or_default();

        // Generate a response based on the input
        let response_text = if user_text.to_lowercase().contains("hello")
            || user_text.to_lowercase().contains("hi")
        {
            "Hello! Welcome to the Hello World Agent. How can I assist you today?"
        } else if user_text.to_lowercase().contains("help") {
            "I'm here to help! I can greet you and answer basic questions. \
             Just say 'hello' to get started!"
        } else if user_text.to_lowercase().contains("bye")
            || user_text.to_lowercase().contains("goodbye")
        {
            "Goodbye! Have a great day!"
        } else {
            "I received your message. I'm a simple hello world agent, \
             so I can respond to greetings and help requests."
        };

        // Create the response message
        let response_message = Message::agent(vec![Part::text(response_text)]);

        // Create the task with completed status and emit it
        let task = Task::new(&ctx.task_id, &ctx.context_id).with_status(TaskStatus::with_message(
            TaskState::Completed,
            response_message,
        ));

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
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    // Create the agent executor
    let executor = HelloWorldExecutor::new();

    // Build and configure the server
    let server = A2AServerBuilder::new()
        .executor(executor)
        .host("0.0.0.0")
        .port(8080)
        .cors(true)
        .build();

    println!("🚀 Starting Hello World Agent on http://localhost:8080");
    println!("📋 Agent card available at: http://localhost:8080/.well-known/agent.json");
    println!("🔧 Health check: http://localhost:8080/health");
    println!("\nPress Ctrl+C to stop the server...\n");

    // Run the server with graceful shutdown on Ctrl+C
    server
        .serve_with_shutdown(async {
            signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C handler");
            println!("\nShutting down...");
        })
        .await
}
