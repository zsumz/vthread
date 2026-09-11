# vthreads

Compatibility alias for [`vthread`](https://github.com/zsumz/vthread), the bounded
carrier-affine virtual-thread runtime for Rust.

Use `vthread` for new applications. This crate re-exports the same public API
without adding another runtime.
Its `runtime-evidence` and `qualification` features forward directly to `vthread`.

It shares vthread's Linux x86_64 and macOS ARM64 targets, Rust 1.96 minimum, and
`panic = "unwind"` requirement.
See [release notes](https://github.com/zsumz/vthread/blob/v0.1.0-rc.2/RELEASE.md)
for verification coverage and known limitations.

This release candidate follows vthread's pre-1.0 compatibility policy. The eventual
`0.1.0` release and subsequent `0.1.x` releases will keep compatible public API
updates within `0.1`; breaking API or contract changes move to `0.2`.

## Using the alias

Add the alias to your project:

```toml
[dependencies]
vthreads = "=0.1.0-rc.2"
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
