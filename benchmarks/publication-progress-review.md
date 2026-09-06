# Paused publication: recipient progress repair

Base: `060ba6c`; retained runtime before this candidate is still `3161514`.
This is a correctness/progress repair, not a claimed speedup. No stack assembly,
placement, cancellation checkpoint, resource ownership policy, fairness quota,
polling budget, channel buffering or readiness algorithm changes are included.

## Protocol and retained ownership

The owner distinguishes Published, InFlight and Stale before ready insertion.
InFlight remains parked with its exact task identity, route and generation. A
per-park membership flag admits at most one deferred notice per outstanding route;
the owner vector is bounded by task admission. Ordinary published wakes do not
allocate deferred storage. Changed completion/control epochs revisit the vector.

Owner interest temporarily uses the otherwise absent permit bit during Claim.
The publisher replaces its old release store with a CAS, clearing that interest
when publishing Selected. If interest was registered, it retains the exact hub
before selection and signals that independent hub afterward. No production target
or route read follows releasing the claim. The existing epoch/waiter/native-sleep
handshake covers completion before, during and after the owner's sleep setup.
Generation width, resource bits and the notice-before-selected boundary remain.

Scope abort atomically preflights every matching parked wait before destroying
any mounted frame or ready ancestor. An Active observation alone is insufficient:
it must retire successfully, or register interest and retain the whole root's
frames. Deferred abort requests are coalesced and bounded by owned-scope admission;
global stop supersedes scoped requests. Unrelated ready tasks remain eligible.
Selected resources stay in their existing primitive tickets until actual cleanup.
The failed-carrier path also waits for legal retirement before reclaiming stacks.

Registration error/panic before suspension is a separate path. Registration guards
and the original failure remain owned outside the catch boundary. A failure-only
private park defers retirement until publication completes; it neither suspends an
active unwind nor invokes public yield/checkpoint policy. Forced cleanup retains
the original engine unwind token and must have preflighted its parked generations.

This does not promise that a paused publisher's task or scope can finish while
held, nor that producer-side channel/metadata lock contention is nonblocking.
Those are separate from dispatching the recipient into an incomplete claim.

## Ordered native evidence so far

The old runtime fails both strengthened dispatch regressions with unrelated
progress stuck at one of two steps. It also fails four independently run cleanup
regressions: owned wait, borrowed ancestor, registration error and registration
panic. Their observers release the old implementation after observing its finish
or retirement dependency, so the failures are retained without hanging the suite.

The integrated candidate passes fifteen ordered kernel tests. They cover ordinary
and real mutex wakes, selected cancellation, actual sleeping owners, scoped
abandonment, a dispatch-selected ready ancestor, global stop and injected carrier
failure. Unrelated arrivals run while publication is still held; resources and
destructors remain retained until completion. Three owner tests additionally check
permit-bit cleanup, selected-resource recovery and notification of the original
hub after rebinding to another owner hub.

A native negative control removes only completion notification. The actual
sleeping-owner regression fails at its bounded result watchdog; restoring the
identical signal makes it pass. The carrier's sleep state is observed before
releasing publication, and failure cleanup explicitly stops the test carrier.

The frozen source (`90bc31261224939e4e1f15fbecd09ca29784d348ce2175dbe901a6f22541511d`)
passes all fourteen canonical gates in
`run-1788670701-39504088-2699589`, with preserved repository state. Native default
debug and release each pass 536 runtime tests; all-features passes 565. The one
ignored case is the manual borrowed-maintenance performance probe. Stack tests,
models, doctests, architecture policy and application smoke are included.

Thirty repetitions per native profile, pinned to one CPU, pass all fifteen ordered
kernel cases and four owner/protocol cases: 1,140 test executions across 120
independent test-process runs. These repeat explicit schedules; they are not 1,140
distinct randomized schedules.

Sequential optimized 60-second mixed soaks complete 380,052 lifetimes on one
carrier and 398,130 on four. All 778,182 admitted lifetimes complete; 8,130,458
parks match wakes, 5,774,336 mutex updates are checked, services drain and shutdown
assertions pass. This bounded stress is not the planned ten-million-lifetime,
large-simultaneous-population or cross-platform release qualification.

## Shared-source composition model

The model now imports the actual production wait word, owner publication/retirement
protocol and retained MPSC queue. The queue was split for source sharing, not
replaced; slot payloads, list reversal, reuse and sleep arming are the same code.
All protocol atomics are Loom primitives. Identity stand-ins are immutable types;
the outer execution counter is harness bookkeeping, invisible to protocol choices.

Seventeen tests pass without permutation, preemption or time cutoffs. Two controls
deliberately reject early mounting and omitted completion notification. Two-route
publication explores 54 schedules; reuse with explicit publication preemption
points explores 23. The first reuse fixture joined away the publication window,
and a later coverage assertion still saw only one reduced execution. Those runs
are retained separately from the final overlapping/preempted fixture.

Signal remains a documented adapter: Loom 0.7.2 weakens SeqCst accesses, so two
model-only fences enforce the epoch/waiter SC obligation. A separate exhaustive
six-order lemma checks it. This is not a full native/C11 Signal proof. Target
binding, lexical stack destruction, full multi-scope scheduling, OS behavior and
the entire runtime composition are not exhaustively modeled. Native ordered
regressions cover the integrated lifetime boundaries; no assembly is modeled.

## Default-build cost screen

The standalone default benchmark suite passes 47 tests. The release candidate is
`e3a6a5f7647de0efbb773fd2c59d96e790c011f824df2677d082ad9ddf74620a`;
the preserved pristine `0b67337` binary is
`9e28c2e1697674c253c2b70137317b3d826fcdaf18a17405b0ff358d89b03a5f`.
Neither enables handoff timing or profiling. This is a correctness-repair cost
screen against that control, not a new May comparison.

Thirteen cases each have four independent process pairs in AB/BA/BA/AB order.
One-carrier cases use CPU 7; four-carrier cases use CPUs 0-3, with existing carrier
pinning enabled except in lifecycle cases. Runtime task capacity equals live tasks
unless marked spare (65,536). Channel cases use one shared bounded MPMC channel,
capacity one, with half producers and half consumers. No placement policy changes.

Two attempts stopped when endpoint guards caught unrelated builds. Keep their
data, but exclude the affected pairs. The table uses only the first five complete
case groups of cohort 2 (40 processes) and all eight remaining groups of cohort 3
(64 processes). Cohort 1 and the incomplete cohort-2 park group are not pooled into
these medians. Endpoint guards are not continuous dedicated-host proof; physical
topology, frequency policy and host scheduling remain VM limitations.

Ns/op is derived from whole-round elapsed time. Cycles/instructions are whole
process counts, including setup, warm-up and shutdown, not isolated switch costs.

| Case / carriers / tasks | Baseline ns/op | Repair ns/op | Process cycles | Process instructions |
| --- | ---: | ---: | ---: | ---: |
| Park / 1 / 8 | 120.29 | 137.40 | +12.17% | +13.31% |
| Contended mutex / 1 / 8 | 196.31 | 205.93 | +3.07% | +10.34% |
| Yield / 4 / 64 | 14.19 | 15.32 | +5.53% | +0.03% |
| Lifecycle / 4 / 1,000 | 444.83 | 437.91 | -1.83% | -6.66% |
| Lifecycle / 4 / 10,000 | 323.55 | 337.47 | +3.75% | +2.27% |
| Lifecycle / 4 / 1,000, spare | 1,679.14 | 1,617.53 | +10.09% | -2.08% |
| Park / 4 / 64 | 107.00 | 117.07 | +15.55% | +12.51% |
| Contended mutex / 4 / 64 | 428.76 | 461.94 | -9.52% | +1.69% |
| Shared channel / 1 / 8 | 429.46 | 444.92 | +3.94% | +14.56% |
| Shared channel / 4 / 8 | 1,414.39 | 1,485.63 | -0.09% | +12.03% |
| Shared channel / 4 / 64 | 1,451.78 | 1,462.99 | +1.91% | +11.18% |
| Shared channel / 4 / 64, spare | 17,651.36 | 20,827.01 | +8.46% | +6.96% |
| Wake throughput / 4 / 64 | 83.64 | 94.13 | +9.03% | +9.14% |

Park's cycle cost repeats in every pair: +10.62% to +18.81% locally and +6.50% to
+18.65% with four carriers. The four-carrier mutex's paired cycle changes span
-30.07% to +25.24%; its lower median cycle count is not a demonstrated mutex win.
Yield's cycle cost also repeats despite almost unchanged instructions. The engine
was not edited; these controls do not establish the cause of that difference.

Separate sampled wake latency has process-median p50 231 to 265.5 ns, p99.9 49.53
to 50.79 us, and worst-task p99.9 68.88 to 73.47 us. P99.99 is highly unstable:
one repair process reaches 17.00 ms versus 0.192 ms in its paired control. Neither
the median p99.99 reduction nor the noisy maxima qualify a tail improvement.
Closed-loop wake samples can include stored permits; warm-up placement is not a
proof of every measured handoff's topology. Offered-load and burst/idle acceptance
remain open.

Retain this as a demonstrated progress repair with an explicit measured cost,
not an optional optimization. Do not recover that cost by increasing polling,
weakening cancellation/ownership or changing the frozen engine/fairness contracts.

## Remaining qualification

The [durable source-keyed bundle](evidence/publication-progress-90bc3126.tar.gz)
contains the complete candidate patch, canonical receipt/logs, negative controls,
model iterations, repeated native tests, soaks, raw cost processes and replay
scripts. Its SHA-256 is
`10e3d51d55621bfa04f2da5799dc315435609ce5f8b30e9735e1a77b9725f2ae`.
Extraction and every internal digest were verified. Executables are represented
by hashes; this is not a complete release artifact bundle.

Capacity/admission-only polling, incremental readiness, channel attribution,
mutex mechanism attribution, the six loaded findings and the complete release
matrix remain separate open slices. The May table remains historical.
