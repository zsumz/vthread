# vthread-sync-core

The narrow exclusive-value and protocol core supporting `vthread` synchronization.

Applications should depend on [`vthread`](https://github.com/zsumz/vthread). This support crate
has no compatibility contract for direct downstream use.

Version `0.0.2-rc.2` is an unpublished release candidate, not yet release-qualified.

## Runtime boundary

The crate isolates unsafe value access behind a linear ownership capability, allowing the
public runtime to forbid unsafe Rust without adding a second native mutex to its virtual mutex.
Queueing, cancellation, bounds, and scheduling remain in the safe runtime crate.

## Experimental wake protocol

The entirely safe `WakeMailbox` kernel is experimental and unused by the runtime.
Its test-only `WakeInbox` combines the first 63 encoded routes, reserved payloads, and a
bounded overflow list.

Shared standard and Loom tests cover payload publication, route reuse, captured-batch and
lane fairness, and carrier sleep registration. Their evidence applies only to this bounded
experimental protocol. It does not qualify the runtime's production wake queue or native
signaling; the runtime retains its existing owner-routed wake mechanism.

Run from the repository root:

```sh
cargo test --locked -p vthread-sync-core --all-targets
```

Loom is a development-only dependency. It does not enter the runtime dependency graph.

[Apache License 2.0](LICENSE)
