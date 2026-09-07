# vthreads

Compatibility alias for [`vthread`](https://github.com/zsumz/vthread), the bounded
carrier-affine virtual-thread runtime for Rust.

New applications should depend on `vthread` directly. This crate re-exports the public
`vthread 0.0.2` API without adding another runtime or a separate API.
Its `runtime-evidence` and `qualification` features forward directly to `vthread`.

Version `0.0.2` is prepared for early development and evaluation; crates.io publication is
pending. See the [release status and limitations](https://github.com/zsumz/vthread/blob/release/0.0.2/RELEASE.md).
It shares vthread's Linux x86_64 and macOS ARM64 targets, Rust 1.96 minimum, and
`panic = "unwind"` requirement.

## Using the alias

Once published, add the crates.io dependency:

```toml
[dependencies]
vthreads = "0.0.2"
```

```rust
fn main() -> vthreads::Result<()> {
    vthreads::run(|scope| {
        let mut task = scope.spawn("answer", || 42)?;
        println!("{}", task.join()?);
        Ok(())
    })
}
```

Tasks keep their carrier affinity and structured ownership. Dropping a handle does not detach
work. Cancellation remains cooperative, and standard-library blocking calls still block a
carrier; use vthread operations or `blocking::run` for work that may block an OS thread.

[Apache License 2.0](LICENSE)
