# Bounded wake service: remote handoff rejection

Retained baseline: `0aea68f`, native source `d38dcb86`. This slice keeps the
execution engine, task affinity, cancellation checkpoints, ownership transfer,
ready cohorts, admission quota, completion flushing and 640 polling probes.
No May comparison has been refreshed. **Reject candidate B**: it improves the
ordered burst diagnostic but fails protected counter-free remote handoff screens.
The retained production source is restored unchanged.

## Attribution before policy

Test-only work counters separate notices, reversed queue links and deferred
publication queries. Ordered native-owner fixtures place a normal-head sentinel
beside an already-published burst, with no subsequent producer work needed for
reclamation. Local and remote routes are checked explicitly; every parked task
asserts its original native thread after resumption. These are test-build
mechanism diagnostics, not counter-free default-build acceptance timings.

Nine sequential optimized controls per burst shape, in one process:

| Burst | Remote sentinel median | Local sentinel median | Retained maximum notices per tick |
| --- | ---: | ---: | ---: |
| 64 | 4,807 ns | 3,305 ns | 64 |
| 1,024 | 44,827 ns | 31,217 ns | 1,024 |
| 10,000 | 1,419,160 ns | 758,814 ns | 10,000 |

The sentinel runs on the third dispatch in these controls: selection fairness is
not the missing guarantee. The first remote pop reverses all 10,000 links, taking
11,448 ns median in the separate queue control. It cannot count as one bounded
unit. Held-publication controls check all 16/64/128 entries on a changed epoch,
then do zero further queries across eight unchanged ticks. Preserve that absence
of quiet rechecking. Neither result attributes the historical 64-task wake tails.

With B, the optimized test-build 10,000-notice sentinel medians are 18,127 ns
remote (first dispatch, after 32 reversed links) and 23,376 ns local (third
dispatch, after at most 32 notices per tick). All notices subsequently drain
without another producer signal, with affinity and reclamation checked. Held
populations retain zero quiet rechecks. These mechanism improvements do not waive
the default-build protected screen, and are not independent process replications.

Three ordered negative controls fail the retained implementation: 128 local
notices, 256 remote link/dequeue units and 128 deferred queries exceed their
32-unit limits. The diagnostic baseline and failing source snapshots are retained.

## One production candidate

Each tick shares a 32-notice local budget and 32-unit remote budget across all
wake-processing points, plus at most 32 deferred queries. A remote unit is one
reversed link or one dequeued notice. Producer slot writes and ordering remain
unchanged. Owner-only cursors retain partial reversal; no notice is emitted until
its detached batch is in FIFO order, and occupied routes cannot be reused early.
Queue visibility and sleep arming include unfinished reversal.

Deferred records retain an unvisited prefix and a visited suffix. Changed epochs
start a pass or request one coalesced refresh; they do not restart an unfinished
prefix. A still-claimed record moves to the visited suffix. Removal preserves
that partition and checks the exact route/token. A completed held pass does not
poll forever, while unfinished service remains visible to idle admission without
needing another signal. Storage remains bounded by live-task admission.

This bounds wake-service units, not every scheduler operation or wall-clock delay.
The retained capacity-dependent snapshot scan, timer work, abort/revocation work,
native scheduling and cooperative user-code duration remain separate costs.
In particular, this is not a claim of capacity-independent maintenance.

The service argument relies on these explicit invariants:

- A detached remote batch is disjoint from the producer head; each owner link
  step reduces its unreversed length. No partial FIFO prefix is emitted, and no
  route's occupancy is cleared before consuming its notice. Later batches cannot
  overtake that retained batch.
- `remaining <= entries.len()` holds at every deferred operation. Popping or
  removing an unvisited entry shrinks that prefix; appending a held/new entry
  cannot enlarge it. New epochs request a subsequent pass, not indefinite restart.
- Budget exhaustion retains a continuation predicate visible to the next tick
  and idle decision. Completion interest still supplies the wakeup when the last
  pass found only held publishers and the owner subsequently sleeps.
- A finite detached batch of N notices needs at most 2N link/dequeue units. Each
  deferred pass examines at most its initial population before any refresh pass.
  These are service-work bounds, conditional on continued owner execution.

## Counter-free acceptance result

All 144 planned processes complete: 48 local and 96 four-carrier/eight-task runs.
Each row has four independent A/B pairs per collection mode, 10,000 operations
per task and 101 measured rounds plus warm-up. Arm and collector order are
balanced. Local runs use CPU 0; remote runs use CPUs 0-3 with default placement
and individual pinning kept separate. Endpoint build-overlap guards are clear.
Here, "remote" names the four-carrier screen, not a claim that every measured
handoff crosses owners; execution placement was not sampled in headline runs.

| Case | Bare paired time change | Bare paired process CPU change | Counter-mode paired time change |
| --- | ---: | ---: | ---: |
| Local park | -0.47% | -0.93% | -0.41% |
| Local mutex | +0.56% | -0.47% | +0.04% |
| Local channel | +1.04% | +0.64% | +0.16% |
| Remote park, default | +7.64% | +9.84% | -0.60% |
| Remote park, pinned | +11.36% | +11.00% | +8.35% |
| Remote mutex, default | **+18.33%** | +28.16% | +5.80% |
| Remote mutex, pinned | **+78.49%** | +28.24% | +6.40% |
| Remote channel, default | **+12.27%** | +22.21% | +9.30% |
| Remote channel, pinned | +4.75% | +9.85% | +13.06% |

Changes are median paired ratios, not ratios of unpaired medians. Time is
throughput-derived whole-round time, not individual handoff latency. Post-exit
process CPU includes startup/teardown and wrappers. The bold rows cross the
predeclared bare stop: median ratio above 1.10 and at least three losses above
10%. No replacement samples, 64-task protected stage, wider tails/lifecycle/CPU
panel, 12-pair confirmation or default-build burst promotion panel follows.

Collection changes the result materially. Pinned mutex A's process-median time
is 229.45 ns/op bare versus 8,971.09 ns/op under counters; B is 461.46 versus
8,504.35 ns/op. Counter-mode voluntary switches reach process medians of about
3.14 million (A) and 2.82 million (B), versus 1,668 and 1,886.5 bare. These are
mode-sensitive observations, not a proven PMU/kernel cause. In particular, the
counter-mode +6.40% paired effect would conceal the much larger bare loss.

## Introduced-cost leads, not a second implementation

B's default disassembly places owner-written reversal cursors at queue offsets
0x80/0x88 alongside producer-read slot metadata at 0x90/0x98, on the same 64-byte
line. The existing consumer index is isolated at 0x40. Grouping owner cursors
inside that already allocated consumer record could avoid this sharing without
new padding, but actual batch shape must be established first: these cursor stores
occur on multi-entry reversal, not singleton handoffs. This finding alone does
not explain the serialized mutex loss.

The isolated test-build control also finds higher dequeue cost after reversal:
the remaining 9,999 pops take 20,571 ns in A versus 80,652 ns in B, while first-pop
times remain close (11,448 versus 11,689 ns). B emits an out-of-line budgeted pop
with a mutable budget argument. Default-build cycle/call-path attribution is
still needed; neither this diagnostic nor layout inspection justifies another
padding, polling or ownership experiment by itself.

## Qualification boundaries and recovery

The candidate's first native runtime unit run passes 561 tests. New queue tests
exercise every budget/reversal cut, FIFO order, occupied-route rejection, later
publication, reuse and unbounded cold cleanup. Deferred tests cover every removal
partition through eight entries, refresh coalescing, continuous epochs, stale
tokens and bounded accounting. Existing held ordinary/mutex publication, selected
cancellation, sleeping owners, abandonment, stop and fault tests remain enabled.

The composed model imports the production queue, deferred sweep and wait-owner
protocol, with the existing documented Signal SC adapter limitation. A deliberate
missing-continuation control deadlocks after all publishers have completed.
Concurrent two-producer budgeted routing completes 54 explored schedules.

The unrestricted three-actor sleep exploration was stopped incomplete. A smaller
multi-route publisher/owner composition completed 1,585,100 normal-resume schedules
then exposed an invalid test assumption: retirement may legally succeed after an
earlier InFlight query. Both debug and optimized failures are preserved. The
corrected double-query oracle and then a direct-retirement abort adapter again
complete the normal-resume panel but remain incomplete in the broader abandonment
exploration. Their stopped runs are preserved, not called passes. The candidate
does not have full composed-abandonment or canonical qualification. No production
correctness failure was established by the invalid test assertion.

Both default benchmark builds pass 47 benchmark tests, formatting and clippy.
All-feature workspace clippy and the production idle-predicate test pass. Removing
deferred idle visibility makes that test fail at the intended assertion; it is
restored before B is frozen. Baseline source: `d38dcb86`, executable `6c332ada`.
Candidate source: `aa72d843`, executable `ffae35bc`.

The full candidate is recoverable from stash
`d5a14b65cbba09aafb90335669606ef34b4a622d` and its source archive. Restored source
is exactly `d38dcb8627f61e49f3f95e46c270b9349c6629365060ac98e2dab6bbb00bfa94`.
The architecture inventory update was inventory-only, with zero grants/revokes;
the original lock is restored with the runtime. The first restored canonical run
passes all 14 task gates, including native debug and release, but its repository
state guard rejects this report being edited during execution. That tooling-use
failure and receipt are preserved. The subsequent untouched-checkout run
`run-1788725598-993674189-3309733` passes all 14 gates and the repository-state
guard. Native debug/release, application smoke and the architecture lock pass;
this is retained-runtime qualification, not candidate or release qualification.

The durable [evidence bundle](evidence/wake-service-aa72d843.tar.gz) contains
1,581 SHA-256-verified files: all 144 raw performance processes, both native
benchmark binaries, seven reconstructed source snapshots, negative controls,
failed/incomplete model attempts, disassembly, both qualification receipts and
their task logs. Archive SHA-256:
`0b6f9aab1650297c9f65c3b7749d88a02b4c4859480a778f8ee47283cd86b819`.
The bundled replay runs eight analysis tests and validates every raw result,
paired effect and ordered diagnostic summary without rerunning the benchmarks.

Next: attribute actual dequeue/batch and recipient scheduling costs before another
bounded-service candidate. Channel retry elimination remains the next independent
synchronization target. Capacity-independent maintenance, readiness acceptance,
current ARM64 execution, sanitizer support, large live/mixed populations, footprint,
loaded tails and the unresolved historical findings remain separate open work.
