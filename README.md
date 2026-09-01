<p align="center">
  <img src="./vthread-logo.svg" alt="vthread" width="720">
</p>

<p align="center">
  <strong>Carrier-affine virtual threads for Rust.</strong>
</p>

<p align="center">
  vthread runs ordinary synchronous functions on reusable stacks. A task suspends at
  explicit vthread operations, stays on one carrier thread after it starts, and always
  belongs to a scope or supervisor.
</p>

<p align="center">
  <a href="#start">Start</a>
  <span> · </span>
  <a href="#requirements">Requirements</a>
  <span> · </span>
  <a href="#what-it-provides">Features</a>
  <span> · </span>
  <a href="#how-it-behaves">Behavior</a>
  <span> · </span>
  <a href="#check">Check</a>
</p>

<br />

## Start

```rust
fn main() -> vthread::Result<()> {
    vthread::run(|scope| {
        let mut answer = scope.spawn("answer", || 42)?;
        println!("{}", answer.join()?);
        Ok(())
    })
}
```

## Requirements

vthread 0.0.1 supports Linux x86_64 and macOS ARM64 with Rust 1.96 or newer. It
requires unwinding panics; applications configured with `panic = "abort"` are rejected at
compile time.

Install from crates.io:

```toml
[dependencies]
vthread = "0.0.1"
```

## What it provides

- Structured tasks with typed joins, borrowed local children, cancellation, and deadlines.
- FIFO virtual mutexes, condition variables, semaphores, notifications, and bounded channels.
- TCP, UDP, Unix sockets, DNS, and filesystem operations that suspend virtual threads.
- A bounded native pool for blocking functions that cannot run on a carrier.
- Named tasks, park reasons, runtime snapshots, stall policies, and explicit shutdown reports.
- Bounded task admission, queues, stacks, waiters, timers, readiness registrations, and native jobs.

## How it behaves

Scopes own their children. Dropping a join handle does not detach work. A started virtual
thread never migrates, so it can keep carrier-local values such as `Rc` across suspension.
Cancellation is cooperative and is observed at checkpoints and vthread operations.

Standard-library blocking calls are not virtualized. Calling `std::fs`, `std::net`,
`std::thread::sleep`, a native mutex, or unknown FFI from a virtual thread blocks its carrier.
Use the matching vthread API or `vthread::blocking::run`.

## Reference application

The standalone [reference application](reference/README.md) uses only the public API. It
shows structured services, virtual networking, notifications, blocking delegation, and
controlled shutdown.

## Check

```sh
zcheck run check
```

## License

[Apache License 2.0](LICENSE)
