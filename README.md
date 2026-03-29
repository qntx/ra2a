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

**Comprehensive Rust SDK for the [Agent2Agent (A2A) Protocol v1.0][a2a-spec] — event-driven server, streaming client, gRPC transport, push notifications, and pluggable SQL task storage.**

ra2a implements the full [A2A protocol specification][a2a-spec] **v1.0** with an idiomatic Rust API, providing a `Client` → `Transport` → `AgentCard` discovery flow on the client side and an `AgentExecutor` → `EventQueue` → `RequestHandler` pipeline on the server side. It is functionally aligned with the official [Go SDK][go-sdk] — same protocol version, same 12 JSON-RPC methods, same type definitions, same error codes.

[a2a-spec]: https://a2a-protocol.org/latest/specification/
[go-sdk]: https://github.com/a2aproject/a2a-go

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

## Architecture

### ra2a crate

| Layer | Key types | Role |
| --- | --- | --- |
| **Types** | `AgentCard`, `AgentInterface`, `Task`, `Message`, `Part` | Full A2A v1.0 type definitions with serde, proto-aligned |
| **Server** | `AgentExecutor`, `EventQueue`, `DefaultRequestHandler` | Event-driven agent execution; composable Axum handlers (`a2a_router`) — SDK does not own the HTTP server |
| **Client** | `Client`, `Transport`, `CallInterceptor` | Transport-agnostic client with interceptor middleware, streaming fallback, and `ClientConfig` defaults |
| **Storage** | `TaskStore`, `PushNotificationConfigStore` | Pluggable persistence (in-memory, PostgreSQL, MySQL, SQLite) |
| **gRPC** | `GrpcTransport`, `GrpcServiceImpl` | Alternative transport via tonic/prost, compiled from the [official proto][a2a-proto] |

[a2a-proto]: https://github.com/a2aproject/A2A/blob/main/specification/a2a.proto

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

## Proto Source

The protobuf definition (`a2a.proto`) is sourced directly from the [official A2A repository][a2a-repo] via git submodule, pinned to the `v1.0.0` tag. The `googleapis` dependency is also vendored as a submodule.

[a2a-repo]: https://github.com/a2aproject/A2A

After cloning, initialize both submodules:

```sh
git submodule update --init --recursive
```

To upgrade the proto to a new release:

```sh
git -C ra2a/proto/a2a checkout <new-tag>
git add ra2a/proto/a2a
git commit -m "build(proto): bump A2A proto to <new-tag>"
```

## A2A Protocol Overview

### Protocol Version

ra2a targets [**A2A v1.0**][a2a-spec] (released 2026-03-12). The `a2a.proto` is the normative source for the protocol data model; JSON-RPC and gRPC are separate protocol bindings derived from it.

### JSON-RPC Methods

All 12 methods defined by the A2A specification are implemented on both client and server:

| Method | Description |
| --- | --- |
| `message/send` | Send a message, receive Task or Message |
| `message/stream` | Send a message, receive SSE event stream |
| `tasks/get` | Retrieve task by ID with optional history |
| `tasks/list` | List tasks with pagination and filtering |
| `tasks/cancel` | Request task cancellation |
| `tasks/resubscribe` | Reconnect to an ongoing task's event stream |
| `tasks/pushNotificationConfig/create` | Create a push notification config for a task |
| `tasks/pushNotificationConfig/get` | Retrieve a push notification config |
| `tasks/pushNotificationConfig/list` | List push notification configs for a task |
| `tasks/pushNotificationConfig/delete` | Delete a push notification config |
| `agent/getAuthenticatedExtendedCard` | Retrieve authenticated extended agent card |

### Task Lifecycle

Tasks progress through well-defined states with terminal conditions:

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

Agents declare one or more `AgentInterface` entries in their `AgentCard`, each specifying a URL, transport protocol (`JSONRPC`, `GRPC`, or `HTTP+JSON`), and protocol version. The card is published at `/.well-known/agent-card.json`. The client's card resolver fetches and caches it automatically. Agents may also expose an authenticated extended card via `agent/getAuthenticatedExtendedCard` for capabilities that require authorization to discover.

### Security Model

A2A supports five security scheme types in the `AgentCard`:

| Scheme | Description |
| --- | --- |
| API Key | Static key in header, query, or cookie |
| HTTP Auth | Bearer token or Basic authentication |
| OAuth 2.0 | Authorization code, client credentials, device code flows |
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
