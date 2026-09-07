# vthreads

Compatibility alias for [`vthread`](https://github.com/zsumz/vthread), the bounded
carrier-affine virtual-thread runtime for Rust.

Use `vthread` for new applications. This crate re-exports the same public API
without adding another runtime.
Its `runtime-evidence` and `qualification` features forward directly to `vthread`.

It shares vthread's Linux x86_64 and macOS ARM64 targets, Rust 1.96 minimum, and
`panic = "unwind"` requirement.
See [release notes](https://github.com/zsumz/vthread/blob/main/RELEASE.md)
for verification coverage and known limitations.

This alias follows vthread's early-development compatibility policy: compatible
public API updates within `0.1.x`, breaking API or contract changes in `0.2`.

## Using the alias

Add the alias to your project:

```toml
[dependencies]
vthreads = "0.1"
```

```rust
fn main() -> vthreads::Result<()> {
    vthreads::run(|scope| {
        let mut task = scope.spawn("answer", || 52)?;
        println!("{}", task.join()?);
        Ok(())
    })
}
```

Tasks keep their carrier affinity and structured ownership. Dropping a handle does not detach
work. Cancellation remains cooperative, and standard-library blocking calls still block a
carrier; use vthread operations or `blocking::run` for work that may block an OS thread.

[Apache License 2.0](LICENSE)
