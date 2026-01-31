//! Example: A2A Client Usage
//!
//! This example demonstrates how to use the A2A client to interact with an agent.

#![allow(unused_imports)]

use ra2a::{
    Result,
    client::{A2AClient, A2AClientBuilder, Client, ClientConfig},
    types::{Message, Part, TaskQueryParams},
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    // Create a client using the builder pattern
    let client = A2AClientBuilder::new("https://agent.example.com")
        .timeout(60)
        .streaming(true)
        .build()?;

    // Fetch the agent card to learn about the agent's capabilities
    println!("Fetching agent card...");
    let card = client.get_agent_card().await?;
    println!("Connected to agent: {}", card.name);
    println!("Description: {}", card.description);
    println!("Skills: {:?}", card.skill_ids());

    // Check if the agent supports streaming
    if card.supports_streaming() {
        println!("Agent supports streaming responses");
    }

    // Create a message to send to the agent
    let message = Message::user_text("Hello! What can you help me with?");

    // Send the message and process the response stream
    println!("\nSending message...");
    let mut stream = client.send_message(message).await?;

    // Process events from the stream
    use futures::StreamExt;
    while let Some(event) = stream.next().await {
        match event? {
            ra2a::client::ClientEvent::TaskUpdate { task, update } => {
                println!("Task status: {:?}", task.status.state);
                if let Some(update) = update {
                    println!("Update: {:?}", update);
                }
            }
            ra2a::client::ClientEvent::Message(msg) => {
                if let Some(text) = msg.text_content() {
                    println!("Agent response: {}", text);
                }
            }
        }
    }

    // Query the task to get its current state
    // (assuming we have a task_id from a previous interaction)
    // let task = client.get_task(TaskQueryParams::new("task-id")).await?;
    // println!("Task state: {:?}", task.status.state);

    println!("\nDone!");
    Ok(())
}
