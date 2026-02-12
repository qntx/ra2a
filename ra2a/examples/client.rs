//! Demo: A2A Client connecting to local server
//!
//! Run this after starting the `server_example`:
//! 1. cargo run --example `server_example` --features server
//! 2. cargo run --example `demo_client` --features server

use ra2a::types::Message;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== A2A Client Demo ===\n");

    let client = reqwest::Client::new();
    let base_url = "http://localhost:8080";

    // 1. Fetch agent card
    println!("📋 Fetching agent card...");
    let card: serde_json::Value = client
        .get(format!("{base_url}/.well-known/agent.json"))
        .send()
        .await?
        .json()
        .await?;

    println!("   Agent: {}", card["name"]);
    println!("   Description: {}", card["description"]);
    println!("   Skills: {:?}\n", card["skills"]);

    // 2. Send a greeting message
    println!("💬 Sending: 'Hello!'");
    let response = send_message(&client, base_url, "Hello!").await?;
    print_response(&response);

    // 3. Send a help request
    println!("💬 Sending: 'Can you help me?'");
    let response = send_message(&client, base_url, "Can you help me?").await?;
    print_response(&response);

    // 4. Send a goodbye message
    println!("💬 Sending: 'Goodbye!'");
    let response = send_message(&client, base_url, "Goodbye!").await?;
    print_response(&response);

    // 5. Send an unknown message
    println!("💬 Sending: 'What is 2+2?'");
    let response = send_message(&client, base_url, "What is 2+2?").await?;
    print_response(&response);

    println!("=== Demo Complete ===");
    Ok(())
}

async fn send_message(
    client: &reqwest::Client,
    base_url: &str,
    text: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let message = Message::user_text(text);

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/send",
        "params": {
            "message": message
        },
        "id": uuid::Uuid::new_v4().to_string()
    });

    let response: serde_json::Value = client
        .post(base_url)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?
        .json()
        .await?;

    Ok(response)
}

fn print_response(response: &serde_json::Value) {
    if let Some(result) = response.get("result") {
        if let Some(status) = result.get("status") {
            let state = status
                .get("state")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            println!("   Status: {state}");

            if let Some(msg) = status.get("message")
                && let Some(parts) = msg.get("parts")
                    && let Some(first) = parts.as_array().and_then(|a| a.first())
                        && let Some(text) = first.get("text") {
                            println!("   Response: {}\n", text.as_str().unwrap_or(""));
                        }
        }
    } else if let Some(error) = response.get("error") {
        println!("   Error: {error:?}\n");
    }
}
