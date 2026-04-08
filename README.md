<!-- markdownlint-disable MD033 MD041 MD036 -->

# RA2A

[![CI][ci-badge]][ci-url]
[![crates.io][ra2a-crate]][ra2a-crate-url]
[![docs.rs][ra2a-doc]][ra2a-doc-url]
[![License][license-badge]][license-url]
[![Rust][rust-badge]][rust-url]

[ci-badge]: https://github.com/qntx/ra2a/actions/workflows/rust.yml/badge.svg
[ci-url]: https://github.com/qntx/ra2a/actions/workflows/rust.yml
[ra2a-crate]: https://img.shields.io/crates/v/ra2a.svg
[ra2a-crate-url]: https://crates.io/crates/ra2a
[ra2a-doc]: https://img.shields.io/docsrs/ra2a.svg
[ra2a-doc-url]: https://docs.rs/ra2a
[license-badge]: https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg
[license-url]: LICENSE-MIT
[rust-badge]: https://img.shields.io/badge/rust-edition%202024-orange.svg
[rust-url]: https://doc.rust-lang.org/edition-guide/

**Comprehensive Rust SDK for the [Agent2Agent (A2A) Protocol][a2a-spec] v1.0 — event-driven server, streaming client, gRPC transport, push notifications, and pluggable SQL task storage.**

ra2a implements the full [A2A v1.0 specification][a2a-spec] (released 2026-03-12) with an idiomatic Rust API. Functionally aligned with the official [Go SDK][go-sdk] — same protocol version, same type definitions, same error codes.

[a2a-spec]: https://a2a-protocol.org/latest/specification/
[go-sdk]: https://github.com/a2aproject/a2a-go

## Features

- **Three protocol bindings** — JSON-RPC, HTTP+JSON/REST, and gRPC from the same `AgentExecutor`
- **Composable server** — Axum handlers you mount on your own router; you own the listener, TLS, and middleware
- **Transport-agnostic client** — `ClientFactory` auto-selects transport from `AgentCard.supported_interfaces`
- **Streaming** — SSE (`message/stream`, `tasks/subscribe`) with automatic non-streaming fallback
- **Push notifications** — webhook delivery with HMAC-SHA256 verification
- **Pluggable storage** — in-memory, PostgreSQL, MySQL, SQLite via sqlx
- **Multi-tenancy** — `a2a_tenant_router` with path-based tenant isolation
- **Interceptors** — `CallInterceptor` on both client and server for auth, telemetry, extension propagation
- **Extension support** — via companion crate `ra2a-ext`

## Crates

| Crate | | Description |
| --- | --- | --- |
| **[`ra2a`](ra2a/)** | [![crates.io][ra2a-crate]][ra2a-crate-url] [![docs.rs][ra2a-doc]][ra2a-doc-url] | Core SDK — types, client, server, gRPC, storage |
| **[`ra2a-ext`](ra2a-ext/)** | [![crates.io][ext-crate]][ext-crate-url] [![docs.rs][ext-doc]][ext-doc-url] | Extensions — `ExtensionActivator`, `ServerPropagator`/`ClientPropagator` interceptors |

[ext-crate]: https://img.shields.io/crates/v/ra2a-ext.svg
[ext-crate-url]: https://crates.io/crates/ra2a-ext
[ext-doc]: https://img.shields.io/docsrs/ra2a-ext.svg
[ext-doc-url]: https://docs.rs/ra2a-ext

## Quick Start

### Server

```rust
use std::{future::Future, pin::Pin};
use ra2a::{
    error::Result,
    server::{AgentExecutor, Event, EventQueue, RequestContext, ServerState, a2a_router},
    types::{
        AgentCard, AgentInterface, AgentSkill, Message, Part,
        Task, TaskState, TaskStatus, TransportProtocol,
    },
};

struct EchoAgent;

impl AgentExecutor for EchoAgent {
    fn execute<'a>(
        &'a self, ctx: &'a RequestContext, queue: &'a EventQueue,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let input = ctx.message.as_ref()
                .and_then(Message::text_content)
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
    let mut card = AgentCard::new(
        "Echo Agent",
        "A simple echo agent.",
        vec![AgentInterface::new(
            "http://localhost:8080",
            TransportProtocol::new(TransportProtocol::JSONRPC),
        )],
    );
    card.skills.push(AgentSkill::new(
        "echo", "Echo", "Echoes user messages",
        vec!["echo".into(), "hello".into()],
    ));

    let state = ServerState::from_executor(EchoAgent, card);
    let app = axum::Router::new().merge(a2a_router(state));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    axum::serve(listener, app).await
}
```

### Client

```rust
use ra2a::client::Client;
use ra2a::types::{Message, Part, SendMessageRequest, SendMessageResponse};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_url("http://localhost:8080")?;

    let card = client.get_agent_card().await?;
    println!("Agent: {} — {}", card.name, card.description);

    let msg = Message::user(vec![Part::text("Hello!")]);
    let result = client.send_message(&SendMessageRequest::new(msg)).await?;

    match result {
        SendMessageResponse::Task(task) => {
            let reply = task.status.message.as_ref().and_then(|m| m.text_content());
            println!("[{:?}] {}", task.status.state, reply.unwrap_or_default());
        }
        SendMessageResponse::Message(msg) => {
            println!("{}", msg.text_content().unwrap_or_default());
        }
    }
    Ok(())
}
```

## Feature Flags

| Feature | Default | Description |
| --- | :---: | --- |
| `client` | **yes** | JSON-RPC + REST client transports, SSE streaming, `ClientFactory`, interceptors |
| `server` | **yes** | JSON-RPC + REST Axum handlers, event queue, task lifecycle, multi-tenant routing |
| `grpc` | — | gRPC transport via tonic/prost (requires `protoc`) |
| `telemetry` | — | OpenTelemetry tracing spans and metrics |
| `postgresql` | — | PostgreSQL task store (sqlx) |
| `mysql` | — | MySQL task store (sqlx) |
| `sqlite` | — | SQLite task store (sqlx) |
| `sql` | — | All SQL backends (`postgresql` + `mysql` + `sqlite`) |
| `full` | — | Everything (`server` + `grpc` + `telemetry` + `sql`) |

## A2A Protocol Reference

### Protocol Operations

All 12 operations defined by the A2A v1.0 specification are implemented on both client and server. Method names follow PascalCase per the spec (§9.1):

| Operation | JSON-RPC | REST | Description |
| --- | --- | --- | --- |
| Send message | `SendMessage` | `POST /message:send` | Send a message, receive Task or Message |
| Stream message | `SendStreamingMessage` | `POST /message:stream` | Send a message, receive SSE event stream |
| Get task | `GetTask` | `GET /tasks/{id}` | Retrieve task by ID with optional history |
| List tasks | `ListTasks` | `GET /tasks` | List tasks with pagination and filtering |
| Cancel task | `CancelTask` | `POST /tasks/{id}:cancel` | Request task cancellation |
| Subscribe to task | `SubscribeToTask` | `POST /tasks/{id}:subscribe` | Reconnect to an ongoing task's event stream |
| Create push config | `CreateTaskPushNotificationConfig` | `POST /tasks/{id}/pushNotificationConfigs` | Create a push notification config |
| Get push config | `GetTaskPushNotificationConfig` | `GET /tasks/{id}/pushNotificationConfigs/{configId}` | Retrieve a push notification config |
| List push configs | `ListTaskPushNotificationConfigs` | `GET /tasks/{id}/pushNotificationConfigs` | List push notification configs |
| Delete push config | `DeleteTaskPushNotificationConfig` | `DELETE /tasks/{id}/pushNotificationConfigs/{configId}` | Delete a push notification config |
| Get extended card | `GetExtendedAgentCard` | `GET /extendedAgentCard` | Retrieve authenticated extended agent card |

### Task Lifecycle

```text
Submitted → Working → Completed
                    → Failed
                    → Canceled
                    → Rejected
           Input Required ←→ Working
           Auth Required  ←→ Working
Unknown (initial/query state)
```

**Terminal states** — `Completed`, `Failed`, `Canceled`, `Rejected` — end the task lifecycle. **Interactive states** — `InputRequired`, `AuthRequired` — resume to `Working` when the client responds.

### Agent Discovery

Agents declare `AgentInterface` entries in their `AgentCard`, each specifying a URL, transport protocol (`JSONRPC`, `GRPC`, or `HTTP+JSON`), and protocol version. The card is published at `/.well-known/agent-card.json` and fetched automatically by the client. Agents may also expose an authenticated extended card via `GetExtendedAgentCard`.

### Security Model

| Scheme | Description |
| --- | --- |
| API Key | Static key in header, query, or cookie |
| HTTP Auth | Bearer token or Basic authentication |
| OAuth 2.0 | Authorization code, client credentials, device code flows |
| OpenID Connect | OIDC discovery-based authentication |
| Mutual TLS | Client certificate authentication |

Push notifications use HMAC-SHA256 verification to authenticate webhook deliveries.

## Security

This library has **not** been independently audited. See [SECURITY.md](SECURITY.md) for supported versions and vulnerability reporting.

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
