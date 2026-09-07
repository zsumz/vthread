<p align="center">
  <img src="./vthread-logo.svg" alt="vthread" width="720">
</p>

<p align="center">
  <strong>Carrier-affine virtual threads for Rust.</strong>
</p>

<p align="center">
  vthread runs ordinary synchronous functions on reusable stacks, with structured
  task ownership, bounded resources, and explicit suspension.
</p>

<p align="center">
  <a href="#model">Model</a>
  <span> · </span>
  <a href="#start">Start</a>
  <span> · </span>
  <a href="#qualification">Qualification</a>
  <span> · </span>
  <a href="#docs">Docs</a>
</p>

<br />

## Model

Tasks belong to a scope or supervisor. Scopes own their children; dropping a join
handle never detaches work. Started tasks never migrate between carrier threads,
so they can keep values such as `Rc` across suspension.

Task admission, queues, stacks, waiters, timers, I/O registrations, and native jobs
have explicit bounds. Cancellation is cooperative and observed at checkpoints.

The runtime provides synchronization, bounded channels, networking, DNS,
filesystem operations, and native blocking delegation. Diagnostics expose task
names, park reasons, and snapshots.

## Start

Add vthread to your project:

```toml
[dependencies]
vthread = "0.1"
```

```rust
fn main() -> vthread::Result<()> {
    vthread::run(|scope| {
        let mut answer = scope.spawn("answer", || 52)?;
        println!("{}", answer.join()?);
        Ok(())
    })
}
```

Standard-library blocking calls are not virtualized. Use vthread operations or
`vthread::blocking::run`; blocking native calls occupy the task's carrier.

## Qualification

```sh
zcheck run check
```

The complete gate covers formatting, Clippy, native debug and release tests,
feature combinations, rustdoc, architecture, and application and benchmark checks.

vthread requires Rust 1.96 or newer, Linux x86_64 or macOS ARM64, and unwinding
panics. Builds with `panic = "abort"` are rejected. See [release notes](RELEASE.md)
for verification coverage and known limitations.

## Compatibility

vthread is in early development, intended for evaluation and feedback. The `0.1.x`
series preserves public API compatibility; breaking API or contract changes move
to `0.2`. Tested configurations and known limitations are in the [release notes](RELEASE.md).

## Docs

[Reference application](reference/README.md) ·
[Benchmarks](benchmarks/README.md) ·
[Runtime evidence](crates/vthread/README.md#runtime-evidence) ·
[Contributing](CONTRIBUTING.md) ·
[Security](SECURITY.md)

## License

Apache-2.0. See [LICENSE](LICENSE).
