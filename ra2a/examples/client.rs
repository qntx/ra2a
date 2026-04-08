#![allow(unused_crate_dependencies)]
//! Example: A2A Client
//!
//! Run the server example first, then:
//! `cargo run --example client --features client`

use ra2a::client::Client;
use ra2a::types::{Message, Part, SendMessageRequest, SendMessageResponse};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_url("http://localhost:8080")?;

    // Fetch the agent card
    let card = client.get_agent_card().await?;
    println!("Agent: {} — {}", card.name, card.description);

    // Send a message (non-streaming)
    let msg = Message::user(vec![Part::text("Hello!")]);
    let params = SendMessageRequest::new(msg);
    let result = client.send_message(&params).await?;

    match result {
        SendMessageResponse::Task(task) => {
            let state = &task.status.state;
            let reply = task.status.message.as_ref().and_then(Message::text_content);
            println!("[{state:?}] {}", reply.unwrap_or_default());
        }
        SendMessageResponse::Message(msg) => {
            println!("{}", msg.text_content().unwrap_or_default());
        }
    }

    Ok(())
}
