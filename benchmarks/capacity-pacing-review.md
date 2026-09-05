# Capacity scans, admission batches and idle work

This is a **rejected diagnostic experiment**, not an accepted optimization.
Baseline: `62378f4b065be66852a48d3a17799ceec20b6b29`. Both binaries enable
`scheduler-profiling`; these measurements are not default-build headline results.

The observer-only candidate moves remote wake-depth observation from carrier
publication to the external snapshot reader. It also preserves the published local
lane instead of overwriting it with the remote count. No shared pending counter,
admission-window change, spin-budget change or wake protocol change is included.

## Ordered snapshot checks

Three real-kernel tests check that carrier publication does not call the remote
depth scanner, that local and remote wake lanes both contribute, and that a reader
sees fresh remote depth without another carrier publication. The first two fail
the old implementation; all three pass the observer candidate. The test-only scan
counter is absent from measured binaries. This rejected candidate did not receive
full canonical qualification.

## Balanced diagnostic processes

Three independent process pairs per case alternate baseline/candidate order.
All use the same four-CPU process mask. This panel does not force carrier pinning
or prove measured pair locality. No compiler/test process was caught by its endpoint
host snapshots, but those snapshots do not establish uninterrupted host exclusivity.
Raw commands, snapshots, timing and hardware counters are retained.

The following values are medians of the three process results. Ns/op is derived
from whole-round throughput; process cycles include warm-up and shutdown. Lifecycle
uses 10,000 tasks, with 501 rounds at capacity 10,000 and 101 at capacity 65,536.
Park uses 64 tasks and 10,000 iterations, with five measured rounds at each capacity.

| Case / capacity | Baseline ns/op | Observer ns/op | Process cycles change |
| --- | ---: | ---: | ---: |
| Lifecycle / 10,000 | 350.85 | 441.46 | +30.95% |
| Lifecycle / 65,536 | 874.14 | 424.14 | -55.04% |
| Park / 64 | 111.85 | 110.12 | -2.54% |
| Park / 1,024 | 133.72 | 120.20 | -6.94% |
| Park / 65,536 | 1,325.01 | 96.63 | -92.57% |

The large-capacity benefit is clear directionally, but the tightly provisioned
lifecycle regression rejects this candidate. Fewer instructions are not enough:
that lifecycle case executes 43.65% fewer instructions while taking 30.95% more
cycles. These profiles explain what changed; they do not satisfy tail/fairness or
uninstrumented performance acceptance.

## Work accomplished per admission and idle episode

The lifecycle profile includes warm-up, measured rounds and shutdown: 5.02 million
admitted/completed tasks at capacity 10,000. Every final profile accounts for those
tasks after shutdown, without inserting a snapshot between rounds.

| Final owner-local metric | Baseline process range | Observer process range |
| --- | ---: | ---: |
| Mean nonempty receive batch | 12.15-14.49 | 1.0064-1.0082 |
| Poll probes per admitted task | 0.279-0.619 | 16.887-19.262 |
| Empty idle entries | 6-46 | 3,796,000-14,191,000 (rounded) |
| Native wait-function calls | 2,159-4,811 | 2,456-3,310 |

The millions of empty idle entries are not millions of kernel sleeps. A wait-call
counter also does not establish a syscall: the native wait predicate can return
without blocking. The substantial changes are smaller useful batches and much
more polling, not just the removed observation instructions.

This led to a separate ordered reproduction: an accepted ingress packet could be
visible after queue unlock while its notifier was paused. Idle detected work but
failed to remember it for the next carrier receive. The separately qualified
[ingress repair](ingress-visibility-review.md) closes that dependency while leaving
capacity scans enabled. It does **not** prove that dependency explains all of this
candidate's lifecycle penalty.

## Preservation and next gate

Candidate source SHA-256:
`244bc8a1fa2d5f9625c0045f84c2bdd27836cbc2566b11b047460f6b08d9dd9f`.
The source is shelved at stash `0b76c091e851474dbda06788ee1a10890c0f61fe`.
The [source-keyed archive](evidence/capacity-pacing-244bc8a1.tar.gz) includes the patch, negative and passing snapshot tests,
both source/binary identities, all 30 processes, analysis and replay commands.
Executables remain local and are represented by hashes in the archive.
Its digest is recorded in [the evidence index](evidence/README.md).

The next bounded experiment is the same observer-only change on the ingress-repair
baseline. Compare its batching and polling with these preserved binaries before
changing idle policy. Retention still requires default-build cycles, lifecycle and
synchronization at tight and production capacities, tails, fairness and idle CPU.
No new May comparison is claimed, and HTTP remains separate.
