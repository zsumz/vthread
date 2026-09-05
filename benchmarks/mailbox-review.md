# Bounded mailbox model slice: 2026-09-05

This follows the [dequeue profile](dequeue-review.md). It adds an experimental
publication kernel in `vthread-sync-core`, **not a runtime queue replacement**.
No scheduler, wake priority, capacity, cancellation checkpoint, task affinity,
admission policy, or existing mutex/channel behavior changes. No speedup is claimed.

## Protocol boundary

One 64-bit word holds 63 route parity bits and one sleeping bit. A producer writes
its externally reserved payload, then toggles its route with a Release RMW. The
single owner acquires a snapshot and acknowledges consumed routes in a separate
cacheline. Normal dequeue does not modify the producer word. A captured batch
drains before fresh publications so repeated low-route reuse cannot starve an
older high route. Route order is not publication FIFO.

The standard-atomic representation occupies exactly 128 bytes and performs no
allocation. Payload storage and overflow routes are not part of this kernel.

The caller must enforce all of these preconditions:

- Exactly one producer can win publication for a given active route generation.
- At most one publication is outstanding per route, including payload consumption.
- The owner acknowledges the route and copies the payload before releasing its
  reservation; the next publisher acquires that release before overwriting it.
- Only one consumer calls `pop`, `has_pending`, `arm_wait`, or `disarm_wait`.
- The owner registers with its sleep primitive before the final arming check.

There is no route generation encoded in the parity bit. Task identity, park
generation, cancellation arbitration, and stale-wake rejection remain external
obligations. Two unacknowledged publications could cancel the bit: the exclusive
reservation is necessary, not an optional optimization.

## Shared implementation, bounded model

`src/wake_mailbox.rs` imports its atomics through a small adapter. Ordinary unit
tests use standard atomics. `tests/wake_mailbox_model.rs` includes that same source
and its sibling tests with Loom atomics. There is no parallel model implementation
that could silently drift from the candidate.

Eight tests cover:

- Owner acknowledgement without clearing the publication word, including reuse.
- Captured-batch fairness when a low route is republished.
- Two concurrent producers, exact-once consumption, and payload visibility.
- Arming versus publication: the owner sees work or the producer requests a wake.
- Disarming versus publication without erasing work.
- Payload reservation release versus route reuse.
- Sleep arming with previously acknowledged parity bits.
- Rejection of a route that would overwrite the sleeping bit.

The concurrent tests use at most three threads with finite operations. The Loom
builder permits 200 branches per execution and does not set a preemption,
permutation, or elapsed-time cutoff. Exceeding the branch bound fails the test;
there is no budget-based successful early return. This is exhaustive exploration
within those small scenarios, not a proof of arbitrary scheduler executions.

The payload assertions run before producer joins where publication ordering is
being tested; a join must not accidentally supply the missing synchronization.
As a negative control, changing only `publish`'s `fetch_xor` from Release to
Relaxed made `racing_producers_publish_payloads_exactly_once` fail with payload
`0` instead of `1`. Release ordering was restored. The negative control demonstrates
that the test detects a real publication fault; it does not prove every ordering
is minimal or that every possible bug is covered.

```sh
cargo test --locked -p vthread-sync-core --all-targets
cargo clippy --locked -p vthread-sync-core --all-targets --all-features -- -D warnings
cargo tree --locked -p vthread-sync-core -e normal,build
```

The reviewed architecture change is the exact development-only dependency on
Loom 0.7.2 and its Cargo lock closure. No existing dependency version changes;
the synchronization core still has no normal or build dependencies. No source
policy, feature-world, macro allowance, or unsafe boundary is relaxed.

## Qualification

All 11 canonical `zcheck run check` gates passed under receipt
`/root/.cache/zcheck/run-1788586989-207511489-1998471/receipt.json`, including
the 499 all-feature runtime tests and eight modeled mailbox scenarios. A separate
default-native workspace run passed 484 runtime, 66 stack, 21 sync-core, eight
model, 19 lab, and one alias test. Its log is
`/tmp/vthread-mailbox-model-native.log`. Both runs used `CARGO_INCREMENTAL=0` to
avoid refilling the nearly full filesystem with rebuildable compiler cache.

The initial restricted canonical run failed 15 socket tests with
`PermissionDenied` and the late-service shutdown test's 200 ms deadline. The
identical source passed the unrestricted canonical run and separate native suite;
no runtime or test ordering was changed to make that rerun pass. This observation
does not establish the cause of the shutdown-test timeout.

These checks qualify the isolated code slice, not runtime integration, sanitizer
coverage, or a performance improvement. The benchmark executable was not rebuilt
for this slice; the retained baseline artifact remains untouched.

## Before runtime integration

Still required: the real route-reservation and stale-generation boundary,
composition with the bounded overflow queue, fairness between both lanes, and
composition with carrier registration/condition-variable notification. The model
above does not claim those properties. Resource FIFO/no-barging must remain at
the primitive even though generic wake publication is no longer globally FIFO.

Only after those checks should a runtime A/B candidate be built. Retain existing
capacity accounting and idle policy so the experiment isolates the owner-head
exchange. Acceptance still requires lower cycles per operation, cross-workload
regression checks, wake tails and fairness, seeded stress, and canonical
qualification. The existing May comparison and full battle-hardening goal remain
open.
