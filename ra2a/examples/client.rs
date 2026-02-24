//! Example: A2A Client
//!
//! Run the server example first, then:
//! `cargo run --example client --features client`

use ra2a::client::Client;
use ra2a::types::{Message, MessageSendParams, Part, SendMessageResult};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_url("http://localhost:8080")?;

    // Fetch the agent card
    let card = client.get_agent_card().await?;
    println!("Agent: {} — {}", card.name, card.description);

    // Send a message (non-streaming)
    let msg = Message::user(vec![Part::text("Hello!")]);
    let params = MessageSendParams::new(msg);
    let result = client.send_message(&params).await?;

    match result {
        SendMessageResult::Task(task) => {
            let state = &task.status.state;
            let reply = task.status.message.as_ref().and_then(|m| m.text_content());
            println!("[{state:?}] {}", reply.unwrap_or_default());
        }
        SendMessageResult::Message(msg) => {
            println!("{}", msg.text_content().unwrap_or_default());
        }
    }

    Ok(())
}
