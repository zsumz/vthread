# vthreads

Compatibility alias for [`vthread`](https://crates.io/crates/vthread), the bounded
carrier-affine virtual-thread runtime for Rust.

New applications should depend on `vthread` directly. This crate re-exports the public
`vthread 0.0.1` API without adding another runtime or a separate API.

```toml
[dependencies]
vthreads = "0.0.1"
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

Licensed under the Apache License 2.0.
