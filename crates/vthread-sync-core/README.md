# vthread-sync-core

The narrow exclusive-value and protocol core supporting `vthread` synchronization.

Applications should depend on `vthread`. This support crate exists so the public runtime can
forbid unsafe Rust while its virtual mutex uses a linear ownership capability instead of a second
native mutex. Queueing, cancellation, bounds, and scheduling remain in the safe runtime crate.

`WakeMailbox` is an experimental, entirely safe route-publication kernel. It is not yet connected
to the runtime. Its eight sibling tests also run against Loom atomics using the same implementation;
they cover bounded publication, payload visibility, route reuse, batch fairness, and sleep arming.
The caller still owns exclusive route reservations and payload lifetime. See the
[mailbox review](../../benchmarks/mailbox-review.md) for assumptions and remaining integration gates.

```sh
cargo test --locked -p vthread-sync-core --all-targets
```

Loom is a development-only dependency. It does not enter the runtime dependency graph.

This support crate has no compatibility contract for direct downstream use. It is licensed under
the Apache License 2.0.
