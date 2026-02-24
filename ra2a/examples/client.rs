//! Example: A2A Client
//!
//! Run the server example first, then:
//! `cargo run --example client --features client`

use futures::StreamExt;
use ra2a::client::{A2AClient, Client, ClientEvent};
use ra2a::types::Message;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create the SDK client (streaming disabled for simple request/response)
    let client = A2AClient::with_config(
        "http://localhost:8080",
        ra2a::client::ClientConfig::new().streaming(false),
    )?;

    // Fetch the agent card via the SDK
    let card = client.get_agent_card().await?;
    println!("Agent: {} — {}", card.name, card.description);

    // Send messages and print responses
    for text in ["Hello!", "What is 2+2?"] {
        println!("\n> {text}");
        let mut stream = client.send_message(Message::user_text(text)).await?;
        while let Some(Ok(event)) = stream.next().await {
            match event {
                ClientEvent::TaskUpdate { task, .. } => {
                    let state = &task.status.state;
                    let reply = task
                        .status
                        .message
                        .as_ref()
                        .and_then(|m| m.text_content());
                    println!("  [{state:?}] {}", reply.unwrap_or_default());
                }
                ClientEvent::Message(msg) => {
                    println!("  {}", msg.text_content().unwrap_or_default());
                }
            }
        }
    }

    Ok(())
}
