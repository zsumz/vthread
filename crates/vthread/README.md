# vthread

Carrier-affine virtual threads for Rust.

Version 0.0.1 supports Linux x86_64 and macOS ARM64 with Rust 1.96 or newer. It requires
unwinding panics; applications configured with `panic = "abort"` are rejected at compile time.

Install from crates.io:

```toml
[dependencies]
vthread = "0.0.1"
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

A started task stays on one carrier thread. Scopes own their children, dropping a handle does
not detach work, and runtime resources have explicit bounds. The crate includes cancellation,
deadlines, virtual synchronization, bounded channels, readiness networking, native blocking
delegation, diagnostics, and controlled shutdown.

Standard-library blocking calls are not automatically virtualized. Use vthread operations or
`blocking::run` when work may block an OS thread. Cancellation is cooperative and cannot
preempt arbitrary Rust code, native calls, or destructors.

Licensed under the Apache License 2.0.
