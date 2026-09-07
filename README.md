<p align="center">
  <img src="./vthread-logo.svg" alt="vthread — virtual threads for Rust" width="680">
</p>

<p align="center">
  <strong>Synchronous Rust. Lightweight tasks. Structured lifetimes.</strong>
</p>

<p align="center">
  <a href="#quick-start">Quick start</a>
  <span> · </span>
  <a href="#built-in">Features</a>
  <span> · </span>
  <a href="#the-contract">Guarantees</a>
  <span> · </span>
  <a href="#explore">Explore</a>
</p>

Run ordinary functions on reusable stacks. Each task stays on one carrier thread
after it starts and belongs to a scope or supervisor.

## Quick start

Requires **Rust 1.96+**, Linux x86_64 or macOS ARM64, and unwinding panics.
`panic = "abort"` builds are rejected at compile time.

Try the unpublished `0.0.2-rc.2` candidate from Git.
[Release status](RELEASE.md) tracks qualification and open gates.

```toml
[dependencies]
vthread = { git = "https://github.com/zsumz/vthread", branch = "release/0.0.2-rc.2" }
```

```rust
fn main() -> vthread::Result<()> {
    vthread::run(|scope| {
        let mut answer = scope.spawn("answer", || 42)?;
        println!("{}", answer.join()?);
        Ok(())
    })
}
```

## Built in

- **Scoped tasks** — typed joins, borrowed children, cancellation and deadlines.
- **Synchronization** — FIFO mutexes, condition variables, semaphores and bounded channels.
- **Synchronous I/O** — TCP, UDP, Unix sockets, DNS and filesystem operations.
- **Blocking delegation** — a bounded native pool for work that cannot suspend.
- **Diagnostics** — named tasks, park reasons, snapshots, stall policies and opt-in evidence.

Task admission, queues, stacks, waiters, timers, I/O registrations and native jobs
have explicit bounds.

## The contract

- **Owned lifetimes.** Scopes own their children; dropping a join handle never detaches work.
- **Stable carriers.** Started tasks never migrate and can keep values such as `Rc` across suspension.
- **Cooperative cancellation.** Tasks observe cancellation at checkpoints and vthread operations.

Standard-library blocking calls are **not** virtualized. Use vthread APIs or
`vthread::blocking::run`; direct native I/O, sleeps, locks and blocking FFI occupy
the task's carrier.

## Explore

[Reference application](reference/README.md) ·
[Benchmarks](benchmarks/README.md) ·
[Runtime evidence](crates/vthread/README.md#runtime-evidence) ·
[Contributing](CONTRIBUTING.md) ·
[Security](SECURITY.md)

Run the complete project check with `zcheck run check`.

[Apache License 2.0](LICENSE)
