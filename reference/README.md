# Reference application

This standalone crate depends only on vthread's public API. It demonstrates structured
pipelines, virtual TCP, dynamic services, notifications, blocking work, and controlled
shutdown.

```sh
cargo test --locked --manifest-path reference/Cargo.toml
cargo run --locked --manifest-path reference/Cargo.toml
```

Applications consume `vthread`; `vthread-stack` is an internal dependency. From another
workspace checkout:

```toml
[dependencies]
vthread = { path = "../vthread/crates/vthread", version = "=0.0.2-rc.1" }
```

A runtime should usually have one application-level owner. Set explicit limits for tasks,
carrier queues, stacks, synchronization waiters, readiness registrations, blocking jobs, and
application buffers. Use scopes for request or operation lifetimes and supervisors for named
long-lived services.

Use vthread networking, synchronization, sleep, DNS, and filesystem APIs. Delegate unknown
synchronous work with `blocking::run`; direct standard-library blocking calls block a carrier.
Cancellation can stop queued work but cannot undo a completed write or interrupt native work
that is already running.
