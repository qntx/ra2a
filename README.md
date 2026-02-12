# ra2a

[![CI][ci-badge]][ci-url]
[![License][license-badge]][license-url]
[![Rust][rust-badge]][rust-url]
[![crates.io][crate-badge]][crate-url]
[![docs.rs][doc-badge]][doc-url]

[ci-badge]: https://github.com/qntx/ra2a/actions/workflows/rust.yml/badge.svg
[ci-url]: https://github.com/qntx/ra2a/actions/workflows/rust.yml
[license-badge]: https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg
[license-url]: LICENSE-MIT
[rust-badge]: https://img.shields.io/badge/rust-edition%202024-orange.svg
[rust-url]: https://doc.rust-lang.org/edition-guide/
[crate-badge]: https://img.shields.io/crates/v/ra2a.svg
[crate-url]: https://crates.io/crates/ra2a
[doc-badge]: https://img.shields.io/docsrs/ra2a.svg
[doc-url]: https://docs.rs/ra2a

**Comprehensive Rust SDK for the [Agent2Agent (A2A) Protocol][a2a-spec] — client transport, server framework, streaming, gRPC, and multi-backend task storage.**

ra2a implements the full [A2A protocol specification][a2a-spec] (v0.3.0) with an idiomatic Rust API — Axum server middleware, reqwest-based client, SSE streaming, gRPC transport, push notifications, and pluggable SQL storage backends. For the upstream protocol specification, see [google/A2A][a2a-repo].

[a2a-spec]: https://google.github.io/A2A/
[a2a-repo]: https://github.com/google/A2A

See [Security](SECURITY.md) before using in production.

## Architecture

```text
┌─────────────────────────────────────────────────┐
│                      ra2a                       │
├──────────┬──────────┬──────────┬────────────────┤
│  client  │  server  │   grpc   │   telemetry    │
│          │  (Axum)  │ (tonic)  │ (OpenTelemetry)│
├──────────┴──────────┴──────────┴────────────────┤
│            types · error · crypto               │
├─────────────────────────────────────────────────┤
│     postgresql  │    mysql    │     sqlite      │
└─────────────────┴─────────────┴─────────────────┘
```

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
ra2a = "0.4"
```

### Build an Agent (Server)

```rust
use async_trait::async_trait;
use ra2a::{
    error::Result,
    server::{A2AServerBuilder, AgentExecutor, ExecutionContext},
    types::{AgentCard, AgentCapabilities, AgentSkill, Message, Part, Task, TaskState, TaskStatus},
};

struct MyAgent { card: AgentCard }

#[async_trait]
impl AgentExecutor for MyAgent {
    async fn execute(&self, ctx: &ExecutionContext, message: &Message) -> Result<Task> {
        let reply = Message::agent(vec![Part::text("Hello from ra2a!")]);
        Ok(Task::new(&ctx.task_id, &ctx.context_id)
            .with_status(TaskStatus::with_message(TaskState::Completed, reply)))
    }

    async fn cancel(&self, ctx: &ExecutionContext, task_id: &str) -> Result<Task> {
        Ok(Task::new(task_id, &ctx.context_id)
            .with_status(TaskStatus::new(TaskState::Canceled)))
    }

    fn agent_card(&self) -> &AgentCard { &self.card }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let card = AgentCard::builder("My Agent", "http://localhost:8080")
        .description("A minimal A2A agent")
        .version("1.0.0")
        .skill(AgentSkill::new("chat", "Chat", "General chat", vec![]))
        .build();

    A2AServerBuilder::new()
        .executor(MyAgent { card })
        .host("0.0.0.0")
        .port(8080)
        .cors(true)
        .build()
        .serve()
        .await
}
```

### Talk to an Agent (Client)

```rust
use ra2a::client::{A2AClient, Client};

#[tokio::main]
async fn main() -> ra2a::Result<()> {
    let client = A2AClient::new("https://agent.example.com")?;

    // Discover agent capabilities
    let card = client.get_agent_card().await?;
    println!("Agent: {} — {}", card.name, card.description.unwrap_or_default());

    // Send a message and stream responses
    let message = ra2a::types::Message::user_text("Hello!");
    let mut stream = client.send_message(message).await?;

    while let Some(event) = futures::StreamExt::next(&mut stream).await {
        println!("{:?}", event?);
    }
    Ok(())
}
```

## Feature Flags

Feature flags keep compile-time dependencies minimal — enable only what you need:

| Flag | Default | Description |
| --- | :---: | --- |
| `client` | **yes** | HTTP/JSON-RPC client, SSE streaming, card resolver, middleware |
| `server` | **yes** | Axum HTTP server, event queue, task lifecycle, REST + SSE endpoints |
| `grpc` | — | gRPC transport via tonic/prost (requires protobuf) |
| `telemetry` | — | OpenTelemetry tracing spans and metrics |
| `postgresql` | — | PostgreSQL task store via sqlx |
| `mysql` | — | MySQL task store via sqlx |
| `sqlite` | — | SQLite task store via sqlx |
| `sql` | — | All SQL backends (`postgresql` + `mysql` + `sqlite`) |
| `full` | — | Everything (`server` + `grpc` + `telemetry` + `sql`) |

```toml
# Server with PostgreSQL storage and telemetry
ra2a = { version = "0.4", features = ["server", "postgresql", "telemetry"] }

# Client only
ra2a = { version = "0.4", default-features = false, features = ["client"] }

# Everything
ra2a = { version = "0.4", features = ["full"] }
```

## Design

| Aspect | Detail |
| --- | --- |
| **Protocol version** | A2A v0.3.0 |
| **Transport** | HTTP/JSON-RPC + SSE streaming + gRPC |
| **Server framework** | Axum with Tower middleware |
| **Client** | reqwest + reqwest-eventsource (SSE) |
| **Task storage** | In-memory (default), PostgreSQL, MySQL, SQLite |
| **Authentication** | HMAC-SHA256 push notification verification |
| **Serialization** | serde (JSON) + prost (protobuf) |
| **Async runtime** | tokio |
| **Linting** | **pedantic + nursery** (warn), **correctness** (deny) |

## Examples

Run the built-in examples to see ra2a in action:

```bash
# Start the server
cargo run --example server --features server

# In another terminal, run the client
cargo run --example client --features client
```

## Security

See [SECURITY.md](SECURITY.md) for disclaimers, supported versions, and vulnerability reporting.

## Acknowledgments

- [Agent2Agent Protocol Specification](https://google.github.io/A2A/) — protocol design by Google
- [google/A2A](https://github.com/google/A2A) — official reference implementations (Python, TypeScript)

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project shall be dual-licensed as above, without any additional terms or conditions.
