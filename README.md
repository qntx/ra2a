<!-- markdownlint-disable MD033 MD041 MD036 -->

# RA2A

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

ra2a implements the full [A2A protocol specification][a2a-spec] **v0.3.0** with an idiomatic Rust API, providing a `Client` → `Transport` → `AgentCard` discovery flow on the client side and an `AgentExecutor` → `EventQueue` → `RequestHandler` pipeline on the server side. It is functionally aligned with the official [Go SDK][go-sdk] — same protocol version, same 11 JSON-RPC methods, same type definitions, same 16 error codes.

[a2a-spec]: https://a2a-protocol.org/latest/specification/
[go-sdk]: https://github.com/a2aproject/a2a-go

> **Protocol status:** The A2A specification is titled "Release Candidate v1.0", but the latest **officially released** version remains **v0.3.0** (2025-07-30). No SDK — including the Go reference — has shipped a v1.0 implementation yet. ra2a will track the specification as new versions are released.

## Crates

| Crate | | Description |
| --- | --- | --- |
| **[`ra2a`](ra2a/)** | [![crates.io][ra2a-crate]][ra2a-crate-url] [![docs.rs][ra2a-doc]][ra2a-doc-url] | SDK — Client, Server, Types, gRPC, task storage |
| **[`ra2a-ext`](ra2a-ext/)** | [![crates.io][ext-crate]][ext-crate-url] [![docs.rs][ext-doc]][ext-doc-url] | Extensions — extension activator, metadata propagator interceptors |

[ra2a-crate]: https://img.shields.io/crates/v/ra2a.svg
[ra2a-crate-url]: https://crates.io/crates/ra2a
[ra2a-doc]: https://img.shields.io/docsrs/ra2a.svg
[ra2a-doc-url]: https://docs.rs/ra2a
[ext-crate]: https://img.shields.io/crates/v/ra2a-ext.svg
[ext-crate-url]: https://crates.io/crates/ra2a-ext
[ext-doc]: https://img.shields.io/docsrs/ra2a-ext.svg
[ext-doc-url]: https://docs.rs/ra2a-ext

## Quick Start

### Server

The SDK provides composable Axum handlers — you own the router, listener, TLS, and middleware.

```rust
use std::{future::Future, pin::Pin};
use ra2a::{
    error::Result,
    server::{AgentExecutor, Event, EventQueue, RequestContext, ServerState, a2a_router},
    types::{AgentCard, Message, Part, Task, TaskState, TaskStatus},
};

struct EchoAgent;

impl AgentExecutor for EchoAgent {
    fn execute<'a>(
        &'a self, ctx: &'a RequestContext, queue: &'a EventQueue,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
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
        })
    }

    fn cancel<'a>(
        &'a self, ctx: &'a RequestContext, queue: &'a EventQueue,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let mut task = Task::new(&ctx.task_id, &ctx.context_id);
            task.status = TaskStatus::new(TaskState::Canceled);
            queue.send(Event::Task(task))?;
            Ok(())
        })
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let card = AgentCard::new("Echo Agent", "http://localhost:8080");
    let state = ServerState::from_executor(EchoAgent, card);
    let app = axum::Router::new().merge(a2a_router(state));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    axum::serve(listener, app).await
}
```

### Client

```rust
use ra2a::{
    client::Client,
    types::{Message, MessageSendParams, Part, SendMessageResult},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_url("http://localhost:8080")?;

    let msg = Message::user(vec![Part::text("Hello!")]);
    let result = client.send_message(&MessageSendParams::new(msg)).await?;

    match result {
        SendMessageResult::Task(task) => {
            let reply = task.status.message.as_ref().and_then(|m| m.text_content());
            println!("[{:?}] {}", task.status.state, reply.unwrap_or_default());
        }
        SendMessageResult::Message(msg) => println!("{}", msg.text_content().unwrap_or_default()),
    }
    Ok(())
}
```

## Architecture

### ra2a crate

| Layer | Key types | Role |
| --- | --- | --- |
| **Server** | `AgentExecutor`, `EventQueue`, `DefaultRequestHandler` | Event-driven agent execution; composable Axum handlers (`a2a_router`) — SDK does not own the HTTP server |
| **Client** | `Client`, `Transport`, `CallInterceptor` | Transport-agnostic client with interceptor middleware, streaming fallback, and `ClientConfig` defaults |
| **Types** | `AgentCard`, `Task`, `Message`, `Event` | Full A2A v0.3.0 type definitions with serde serialization |
| **Storage** | `TaskStore`, `PushConfigStore` | Pluggable persistence (in-memory, PostgreSQL, MySQL, SQLite) |
| **gRPC** | `GrpcTransport`, `GrpcServiceImpl` | Alternative transport via tonic/prost |

### ra2a-ext crate

| Component | Role |
| --- | --- |
| `ExtensionActivator` | Client interceptor — requests extension activation filtered by `AgentCard` capabilities |
| `ServerPropagator` / `ClientPropagator` | Interceptor pair — propagates extension metadata and headers across agent chains (A → B → C) |
| `PropagatorContext` | Task-local data carrier connecting server and client interceptors |

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

---

<div align="center">

A **[QNTX](https://qntx.fun)** open-source project.

<a href="https://qntx.fun"><img alt="QNTX" width="369" src="https://raw.githubusercontent.com/qntx/.github/main/profile/qntx-banner.svg" /></a>

<!--prettier-ignore-->
Code is law. We write both.

</div>
