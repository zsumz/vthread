# vthread

Carrier-affine virtual threads for Rust.

Write ordinary Rust functions with structured ownership, bounded resources, and explicit
suspension points. The public runtime crate forbids unsafe Rust.

Version `0.0.2-rc.2` is an unpublished release candidate, not yet release-qualified.
Supported targets are Linux x86_64 and macOS ARM64, with Rust 1.96 or newer and
`panic = "unwind"`. Builds with `panic = "abort"` are rejected.

## A first task

Use the release branch:

```toml
[dependencies]
vthread = { git = "https://github.com/zsumz/vthread", branch = "release/0.0.2-rc.2" }
```

```rust
fn main() -> vthread::Result<()> {
    vthread::run(|scope| {
        let mut task = scope.spawn("answer", || 42)?;
        println!("{}", task.join()?);
        Ok(())
    })
}
```

## Runtime contract

- A started task stays on its carrier thread for its lifetime.
- Scopes own their children. Dropping a handle never detaches work.
- Admission, queues, stacks, timers, wake permits, native work, and channels have explicit bounds.
- Every parked or yielded task has an observable reason. Each park generation selects one
  winner; timers and remote wakes carry the generation they target.

The runtime includes cancellation, deadlines, virtual synchronization, bounded channels,
readiness networking, native blocking delegation, diagnostics, and controlled shutdown.

Standard-library blocking calls are not automatically virtualized. Use vthread operations or
`blocking::run` when work may block an OS thread. Cancellation is cooperative and cannot
preempt arbitrary Rust code, native calls, or destructors.

## Runtime evidence

Enable `runtime-evidence` and set a positive `Runtime::builder().evidence_capacity(...)` to
record a bounded, typed event stream. Recording is disabled without that explicit capacity.

- Events cover owned root scopes and supervisors, task and wait lifecycles, wake origins, stacks,
  timers, queues, and shutdown. Borrowed local scopes share their containing owned scope.
- A single consumer can wait for a bounded batch without polling. This blocking wait belongs
  on an ordinary OS or monitoring thread; loss counters report full or disconnected buffers.
- The `qualification` feature adds wake probes bound to one exact wait identity and generation
  for stale-wake testing.

[Source](https://github.com/zsumz/vthread) · [Apache License 2.0](LICENSE)
