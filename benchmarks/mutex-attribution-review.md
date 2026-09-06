# Production mutex useful-path attribution

This slice adds opt-in owner-local accounting, not a mutex or scheduler optimization.
The default executable's entire `.text` section is unchanged. It establishes which
useful paths the existing mutex benchmark takes **under diagnostic instrumentation**;
it does not attribute the historical May gap to a measured stage or refresh May.

## Source and scope

- Base commit: `837f8d8134522961f9c5650a06d368995bd4460e`.
- Pristine A source: `d38dcb8627f61e49f3f95e46c270b9349c6629365060ac98e2dab6bbb00bfa94`.
- Accounting B source: `325396ac5f820b4714ccc439d994cb947e2fbb977ceb4a2a4be19a8491018cbe`.
- Default A binary: `6c332adadb87d387a8ee82881db2b2a06b57c9d835724f20468228f3d44cc5b7`.
- Default B binary: `6b1942226797bd4170c886fdfa5eeb698c3383a824d92f21e5a1610802b8a316`.
- Clocked B binary: `bdd4745c8e35c2151c93bd94208bd2d178ac283e68f26ffc1cbdb1bf3c36ebd6`.

Both default binaries contain the same 1,277,172 executable-code bytes, SHA-256
`b0dd4cc47f3fdc517ff1f77f561e114678627ea352bf775cedd84da88891cee9`.
Full executable hashes differ; this is code-section equivalence, not byte identity
of every section. No mutex profiling symbols exist in either default binary.

No ownership transition, cancellation checkpoint, task affinity, stack code,
polling budget, queue ordering, capacity bound or wake/sleep handshake changed.
The analysis lock changes only its inventory fingerprint/counts: no permissions,
dependencies or debt grants. The new module and its sibling tests remain inside
the existing `handoff-profiling` feature. This uses the current native engine in
both feature configurations, not a compatibility stack backend.

## What the counters mean

The report distinguishes immediate and queue-recheck acquisitions, admitted FIFO
tickets, successful permit returns, actual park crossings, normal ticket completion,
queued cleanup and selected-owner abandonment. Error/unwind calls are not counted
as useful acquisitions. A dropped ticket that the selector already rejected is
neither a queued removal nor a selected abandonment; total ticket conservation
includes that case. `try_lock` calls are excluded.

Resource outcomes distinguish rejected offers, inactive stored grants and active
selections. For an active selection, the owning hub is compared with the source
carrier's exact hub identity **while the publication guard still owns Claim**.
No target metadata is inspected for an inactive grant: it may already be consumed,
retired or rebound. Shared routing does not imply another owner. The ordered
test deliberately routes same-owner selections through the shared queue and also
reuses one wait record across primary/fallback targets.

These are logical owner identities, not timestamped task/TID traces. A different
owner does not establish that the recipient was asleep. Native callers without a
mounted carrier route are omitted, consistently with the existing profiler.
Source-side offers and recipient-side receipts may belong to different carriers;
their equality is checked globally only in this all-carrier-producer fixture.

## Complete balanced panel

120 fresh processes: ten cells, four triplets each, with ABP/PBA/BAP/PAB order.
A/B are default builds; P is B with the **whole existing clocked profiling feature**.
This does not isolate the incremental cost of the new counters from prior clocks.
No PMU collector runs. Every planned outcome completed; endpoint build guards were
clear, which is not continuous proof of an otherwise idle host.

Each process runs one warm-up and nine measured rounds, with capacity equal to
live tasks. Work counts are 1,000 or 10,000 acquisitions per task. Single-carrier
execution retains yield-under-lock; four-carrier execution retains the 32
`black_box` operations inside the critical section. Four-carrier default and
individually pinned cells remain separate. These are throughput-derived ns/op,
not individual acquisition latencies. CPU ratios cover the whole process,
including setup, warm-up and teardown.

Table values are medians of process results; ratios are medians of paired process
ratios, not ratios of the displayed medians.

| Carriers/tasks; calls/task; placement | Default B ns/op | Clocked P ns/op | P/B elapsed | P/B process CPU |
| --- | ---: | ---: | ---: | ---: |
| 1/8; 1k; process mask | 200.79 | 284.76 | 1.418x | 1.359x |
| 1/8; 10k; process mask | 199.94 | 282.25 | 1.416x | 1.421x |
| 4/8; 1k; default | 213.75 | 284.86 | 1.316x | 1.349x |
| 4/8; 1k; pinned | 198.83 | 323.35 | 1.627x | 1.722x |
| 4/8; 10k; default | 323.11 | 514.34 | 1.679x | 1.624x |
| 4/8; 10k; pinned | 394.56 | 625.77 | 1.565x | 1.518x |
| 4/64; 1k; default | 236.62 | 587.48 | 2.544x | 1.545x |
| 4/64; 1k; pinned | 264.83 | 665.76 | 2.435x | 1.552x |
| 4/64; 10k; default | 446.04 | 624.00 | 1.422x | 1.423x |
| 4/64; 10k; pinned | 444.89 | 625.51 | 1.428x | 1.484x |

The default A/B control is instructive: identical executable code still produces
wide multicarrier differences in this short diagnostic panel. Median paired B/A
ratios range from 0.785 to 1.159 across cells; the 4/64, 1k pinned cell's individual
ratios range from 0.391 to 1.930. Single-carrier ratios are within 0.1% of unity;
the longer 4/64 controls are 0.980 default and 1.003 pinned. Do not credit those
differences as optimization wins or retrofit acceptance rules around them. All
processes and four-pair uncertainty summaries remain available. Nine rounds/four
processes are a diagnostic screen, not precision protected-performance acceptance.

## Useful path findings

Across 40 clocked processes, all **66,880,000** acquisitions are accounted for:
139,149 immediate, 263 queue-recheck and 66,740,588 completed FIFO tickets. Those
tickets produce **66,740,567 actual park crossings and only 21 stored grants**.
There are no rejected grants, failed calls or dropped tickets in this successful
benchmark fixture. This is not evidence about cancellation workloads.

For the longer four-carrier cells, the following are ranges across four diagnostic
processes, not confidence intervals or uninstrumented state-frequency estimates:

| Tasks / placement | Actual parks per acquisition | Different owner among active grants |
| --- | ---: | ---: |
| 8 / default | 99.818-99.918% | 82.03-87.94% |
| 8 / pinned | 98.537-99.831% | 85.48-93.60% |
| 64 / default | 99.803-99.958% | 68.27-72.47% |
| 64 / pinned | 99.909-99.980% | 72.33-77.83% |

The mutex fixture does not exhibit the channel's three-crossings-per-value pattern:
almost every queued acquisition crosses once and receives ownership once. A stored-
grant fast-path improvement would target a very small observed population here.
The common useful path is an active selected handoff, usually to another owner.
That narrows the next investigation; it does **not** establish whether publication,
owner maintenance, ready residence, native sleep or OS descheduling dominates time.

## Qualification and limits

The new ordered tests cover immediate acquisition without wait attachment, successful
handoff, cancellation before/after selection, stored/rejected/queued ticket cleanup,
capacity rejection, unwind call accounting and target reuse. A deliberate
all-destinations-are-local mutation fails the owner-identity assertion; benchmark
report tests reject both missing and excess acquisitions. Four replay-parser tests
reject malformed counts, omitted/duplicate carriers and false sleep claims.

Targeted native mutex/progress suite: 26 passed. Default benchmark tests: 47 passed.
Profiling benchmark tests: 53 passed; strict benchmark clippy passes in both builds.
Initial fixture-capacity and stored-permit setup mistakes are retained in the attempt
ledger, alongside the macro-origin/inventory correction. No runtime defect was
inferred from those fixture failures.

Canonical `zcheck run check` passes all **14 gates**, with repository state preserved:
`run-1788735189-317388508-3362164`. All-feature native tests took 113.0s,
default native debug 111.0s and default native release 215.7s. The full logs and
receipt are bundled; this is Linux x86-64 execution, not ARM64 or sanitizer evidence.

## Decision and next slice

Retain the opt-in attribution counters after qualification; no runtime performance
candidate is promoted. Do not rewrite direct ownership, add polling, infer sleep
from shared routing, or subtract one-successor latency from whole-round throughput.

Next, sample the already identified useful remote handoff with exact task/generation
association through selection, publication/deferral, owner consumption, ready insertion,
dispatch and ownership receipt. Associate actual recipient state and targeted OS
scheduling evidence, and quantify that diagnostic mode's distortion separately.
Only those stage measurements should choose the next hot-path change.

Capacity-independent maintenance, readiness promotion, offered-load/tail acceptance,
large simultaneous mixed populations, current-source ARM64, sanitizer integration,
footprint and the unresolved historical/control findings remain separate open work.
The preceding eligible-channel candidate remains rejected; these diagnostic controls
do not retrospectively qualify it. There is no new May or release-readiness claim.

The [durable bundle](evidence/mutex-attribution-325396ac.tar.gz) contains 1,206
checksum-listed payload files plus the checksum manifest: both exact source
snapshots, all three binaries, all 120 process outcomes, parsers/negative controls,
code-section comparison and final canonical receipts. Replay verifies and
re-analyzes the complete panel without executing the benchmark binaries.
Archive SHA-256: `142e26e55100b2b34daf613df1366b08b0d276daad4496be580d6010b5958244`.
