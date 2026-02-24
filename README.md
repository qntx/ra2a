# ra2a

[![CI][ci-badge]][ci-url]
[![License][license-badge]][license-url]
[![Rust][rust-badge]][rust-url]

[ci-badge]: https://github.com/qntx/ra2a/actions/workflows/rust.yml/badge.svg
[ci-url]: https://github.com/qntx/ra2a/actions/workflows/rust.yml
[license-badge]: https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg
[license-url]: LICENSE-MIT
[rust-badge]: https://img.shields.io/badge/rust-edition%202024-orange.svg
[rust-url]: https://doc.rust-lang.org/edition-guide/

**Comprehensive Rust SDK for the [Agent2Agent (A2A) Protocol][a2a-spec] — event-driven server, streaming client, gRPC transport, push notifications, and pluggable SQL task storage.**

ra2a implements the full [A2A protocol specification][a2a-spec] **v0.3.0** with an idiomatic Rust API, providing a `Client` → `Transport` → `AgentCard` discovery flow on the client side and an `AgentExecutor` → `EventQueue` → `RequestHandler` pipeline on the server side. It is functionally aligned with the official [Go SDK][go-sdk] — same protocol version, same 11 JSON-RPC methods, same type definitions, same 15 error codes.

[a2a-spec]: https://a2a-protocol.org/latest/specification/
[go-sdk]: https://github.com/a2aproject/a2a-go

> **Protocol status:** The A2A specification is titled "Release Candidate v1.0", but the latest **officially released** version remains **v0.3.0** (2025-07-30). No SDK — including the Go reference — has shipped a v1.0 implementation yet. ra2a will track the specification as new versions are released.

## Crates

| Crate | | Description |
| --- | --- | --- |
| **[`ra2a`](ra2a/)** | [![crates.io][ra2a-crate]][ra2a-crate-url] [![docs.rs][ra2a-doc]][ra2a-doc-url] | SDK — Client, Server, Types, gRPC, task storage |
| **[`ra2a-ext`](ra2a-ext/)** | | Extensions — additional interceptors and utilities (WIP) |

[ra2a-crate]: https://img.shields.io/crates/v/ra2a.svg
[ra2a-crate-url]: https://crates.io/crates/ra2a
[ra2a-doc]: https://img.shields.io/docsrs/ra2a.svg
[ra2a-doc-url]: https://docs.rs/ra2a

## Quick Start

### Server

The SDK provides composable Axum handlers — mount A2A routes on your own router, just like Go SDK's `mux.Handle("/invoke", a2asrv.NewJSONRPCHandler(handler))`:

```rust
use ra2a::server::{ServerState, a2a_router, AgentExecutor, Event, EventQueue, RequestContext};
use ra2a::types::{AgentCard, AgentSkill, Message, Part, Task, TaskState, TaskStatus};

// 1. Implement your agent
struct EchoAgent;

#[async_trait::async_trait]
impl AgentExecutor for EchoAgent {
    async fn execute(&self, ctx: &RequestContext, queue: &EventQueue) -> ra2a::error::Result<()> {
        let input = ctx.message.as_ref()
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

    async fn cancel(&self, ctx: &RequestContext, queue: &EventQueue) -> ra2a::error::Result<()> {
        let mut task = Task::new(&ctx.task_id, &ctx.context_id);
        task.status = TaskStatus::new(TaskState::Canceled);
        queue.send(Event::Task(task))?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut card = AgentCard::new("Echo Agent", "http://localhost:8080");
    card.description = "A simple echo agent".into();
    card.skills.push(AgentSkill::new("echo", "Echo", "Echoes messages", vec![]));

    // 2. Build state and mount on your own router
    let state = ServerState::from_executor(EchoAgent, card);
    let app = axum::Router::new()
        .merge(a2a_router(state))
        .route("/health", axum::routing::get(|| async { "OK" }));

    // 3. You own the listener and server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async { tokio::signal::ctrl_c().await.ok(); })
        .await
}
```

Individual handlers (`handle_jsonrpc`, `handle_sse`, `handle_agent_card`) are also exported for fine-grained route control.

### Client

```rust
use ra2a::client::Client;
use ra2a::types::{Message, MessageSendParams, Part, SendMessageResult};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_url("http://localhost:8080")?;

    // Discover agent capabilities
    let card = client.get_agent_card().await?;
    println!("Agent: {} — {}", card.name, card.description);

    // Send a message (non-streaming)
    let msg = Message::user(vec![Part::text("Hello!")]);
    let params = MessageSendParams::new(msg);
    let result = client.send_message(&params).await?;

    match result {
        SendMessageResult::Task(task) => {
            let reply = task.status.message.as_ref().and_then(|m| m.text_content());
            println!("[{:?}] {}", task.status.state, reply.unwrap_or_default());
        }
        SendMessageResult::Message(msg) => {
            println!("{}", msg.text_content().unwrap_or_default());
        }
    }
    Ok(())
}
```

```bash
# Run the examples
cargo run --example server --features server
cargo run --example client --features client   # in another terminal
```

## Architecture

- **ra2a** — Full A2A v0.3.0 SDK. The server side exposes composable Axum handlers (`a2a_router`, `handle_jsonrpc`, `handle_sse`, `handle_agent_card`) that you mount on your own router — the SDK does not own the HTTP server, mirroring Go SDK's `NewJSONRPCHandler`/`NewStaticAgentCardHandler` pattern. `AgentExecutor` is the event-driven server trait; implementations write events to an `EventQueue`, and the `DefaultRequestHandler` coordinates persistence, streaming, and push notifications. `InterceptedHandler` decorates any `RequestHandler` with before/after interceptors. `TaskStore` is pluggable (in-memory, PostgreSQL, MySQL, SQLite). The client side wraps a `Transport` trait with `CallInterceptor` middleware, `ClientConfig` defaults, and automatic streaming fallback. All 11 JSON-RPC methods are supported on both client and server, plus gRPC transport via tonic/prost.
- **ra2a-ext** — Optional extensions crate (WIP). Will house advanced interceptors and utilities analogous to the Go SDK's `a2aext/` module (extension activators, metadata propagators).

## Feature Flags

| Feature | Default | Description |
| --- | :---: | --- |
| `client` | **yes** | HTTP/JSON-RPC client, SSE streaming, card resolver, interceptors |
| `server` | **yes** | Composable Axum handlers, event queue, task lifecycle, SSE streaming |
| `grpc` | — | gRPC transport via tonic/prost (requires protobuf compiler) |
| `telemetry` | — | OpenTelemetry tracing spans and metrics |
| `postgresql` | — | PostgreSQL task store via sqlx |
| `mysql` | — | MySQL task store via sqlx |
| `sqlite` | — | SQLite task store via sqlx |
| `sql` | — | All SQL backends (`postgresql` + `mysql` + `sqlite`) |
| `full` | — | Everything (`server` + `grpc` + `telemetry` + `sql`) |

```toml
# Server with PostgreSQL storage and telemetry
ra2a = { version = "0.6", features = ["server", "postgresql", "telemetry"] }

# Client only
ra2a = { version = "0.6", default-features = false, features = ["client"] }

# Everything
ra2a = { version = "0.6", features = ["full"] }
```

## A2A Protocol Overview

### Protocol Version

ra2a targets **A2A v0.3.0** — the latest officially released version of the [A2A specification][a2a-spec]. The specification document is titled "Release Candidate v1.0", but v1.0 has not been formally released yet. When v1.0 ships (breaking changes include removal of the `kind` discriminator and type renames), ra2a will publish a corresponding major version update.

### JSON-RPC Methods

All 11 methods defined by the A2A specification are implemented on both client and server:

| Method | Description |
| --- | --- |
| `message/send` | Send a message, receive Task or Message |
| `message/stream` | Send a message, receive SSE event stream |
| `tasks/get` | Retrieve task by ID with optional history |
| `tasks/list` | List tasks with pagination and filtering |
| `tasks/cancel` | Request task cancellation |
| `tasks/resubscribe` | Reconnect to an ongoing task's event stream |
| `tasks/pushNotificationConfig/*` | CRUD for per-task push notification webhooks |
| `agent/getAuthenticatedExtendedCard` | Retrieve authenticated extended agent card |

### Task Lifecycle

Tasks progress through 9 states with well-defined terminal conditions:

```text
Submitted → Working → Completed
                   → Failed
                   → Canceled
                   → Rejected
           Input Required ←→ Working
           Auth Required  ←→ Working
Unknown (initial/query state)
```

Terminal states (`Completed`, `Failed`, `Canceled`, `Rejected`) end the task lifecycle. `InputRequired` and `AuthRequired` are interactive states that resume to `Working` when the client responds.

### Agent Discovery

Agents publish an `AgentCard` at `/.well-known/agent-card.json` describing their identity, capabilities, skills, supported transports, and security requirements. The client's card resolver fetches and caches this card automatically. Agents may also expose an authenticated extended card via `agent/getAuthenticatedExtendedCard` for capabilities that require authorization to discover.

### Security Model

A2A supports five security scheme types in the `AgentCard`, aligned with OpenAPI 3.0:

| Scheme | Description |
| --- | --- |
| API Key | Static key in header, query, or cookie |
| HTTP Auth | Bearer token or Basic authentication |
| OAuth 2.0 | Authorization code, client credentials, device code, implicit flows |
| OpenID Connect | OIDC discovery-based authentication |
| Mutual TLS | Client certificate authentication |

Push notifications use HMAC-SHA256 verification to authenticate webhook deliveries.

## Security

This library has **not** been independently audited. See [SECURITY.md](SECURITY.md) for full disclaimer, supported versions, and vulnerability reporting instructions.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project shall be dual-licensed as above, without any additional terms or conditions.
