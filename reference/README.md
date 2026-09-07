# Reference application

This standalone crate depends only on vthread's public API. It demonstrates structured
pipelines, virtual TCP, dynamic services, notifications, blocking work, and controlled
shutdown.

It follows the unpublished `0.0.2-rc.2` release candidate; see the [release status](../RELEASE.md)
for qualification progress.

## Run it

From the repository root:

```sh
cargo test --locked --manifest-path reference/Cargo.toml
cargo run --locked --manifest-path reference/Cargo.toml
```

The default run exercises all scenarios. To run one example, append `-- dynamic-service`,
`-- worker`, or `-- walk` to the `cargo run` command.

## Use it as a guide

Applications consume `vthread`; its support crates remain internal dependencies.
For an application beside a checkout named `vthread`:

```toml
[dependencies]
vthread = { path = "../vthread/crates/vthread", version = "=0.0.2-rc.2" }
```

- Give the runtime one application-level owner. Use scopes for requests and operations,
  and supervisors for named, long-lived services.
- Set explicit limits for tasks, carrier queues, stacks, synchronization waiters, readiness
  registrations, blocking jobs, and application buffers.
- Use vthread networking, synchronization, sleep, DNS, and filesystem APIs. Delegate unknown
  synchronous work with `blocking::run`; direct standard-library blocking calls block a carrier.
- Treat cancellation as cooperative. It can stop queued work, but cannot undo a completed
  write or interrupt native work that is already running.

[Runtime overview](../README.md) · [Apache License 2.0](../LICENSE)
