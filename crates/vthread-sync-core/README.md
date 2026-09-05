# vthread-sync-core

The narrow exclusive-value and protocol core supporting `vthread` synchronization.

Applications should depend on `vthread`. This support crate exists so the public runtime can
forbid unsafe Rust while its virtual mutex uses a linear ownership capability instead of a second
native mutex. Queueing, cancellation, bounds, and scheduling remain in the safe runtime crate.

The entirely safe `WakeMailbox` kernel is experimental; the runtime does not use it.
A test-only `WakeInbox` composes its first 63 encoded routes with reserved payloads and a bounded
overflow list. Shared standard/Loom tests cover payload publication, route reuse, captured-batch
and lane fairness, and carrier sleep registration. Runtime integration was measured and rejected
because handoff throughput regressed. See the
[mailbox review](../../benchmarks/mailbox-review.md) and
[integration evidence](../../benchmarks/mailbox-integration-review.md) for the proof boundary,
sleep-handshake correction, and complete retention decision.

```sh
cargo test --locked -p vthread-sync-core --all-targets
```

Loom is a development-only dependency. It does not enter the runtime dependency graph.

This support crate has no compatibility contract for direct downstream use. It is licensed under
the Apache License 2.0.
