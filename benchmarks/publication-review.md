# Wake publication: ordered pause evidence

Initial baseline: `2abae58d63fdce689251f26603dca1df98402fd3` on
`perf/scheduler-hot-path`. This is a test-only slice, not a wake protocol repair
or a performance candidate. Production atomic ordering, layouts, admission,
scheduling and ownership transfer are unchanged. All hooks and storage are
`cfg(test)`; the diagnostic timestamps are not benchmark measurements.

Final qualification is atop `23822ed`, which separately repairs an existing
completion-observer test assumption without changing production behavior. The
captured failure and its ordered proof are in
[completion-observer-review.md](completion-observer-review.md).

## Observed dependency

The production selector claims a generation, publishes its notice, and then
publishes the selected wait word. A per-wait test hook pauses immediately after
`with_target` and wake routing return, before `publish_claim`. Unlike the existing
inbox hook, this is **after** queue publication and any native notification.

The owner can consume the notice and mount the recipient while the wait word is
still claimed. Both ordinary and resource-aware finish paths then spin. The probe
records the first and 1,024th finish-loop visits, with the exact wait token, OS-thread
identity and elapsed diagnostic time. Its observer releases the publisher afterward.
No elapsed-time assertion is used to establish unrelated-task non-progress.

Real `Kernel::tick` and native-stack execution reproduce this for an external ready
wake and a direct mutex ownership handoff. Before selection, an unrelated task has
already run once, yielded, and returned to the same owner's ready queue. At the
1,024th finish visit it still has not run again. After publication is released,
both tasks complete. This demonstrates a carrier-wide scheduling dependency on the
publisher, not just delayed return from one park operation. The characterization
tests deliberately record the existing defect; passing them is **not** a guarantee
of progress while a publisher is paused.

The native kernel is driven explicitly on its owning test thread so arrivals and
dispatches are ordered, not inferred from warm-up placement or a snapshot. It is
not a test of the full carrier idle loop. A separate test observes a registered
native `WaitHub::wait` predicate waiter, delivers its notice without advancing the
signal epoch, and confirms the same claimed finish dependency. Registration does
not prove the OS actually descheduled the waiter; no such claim is made.

## Ownership and generation checks

| Injected interval | Checked result |
| --- | --- |
| Notice visible, word still claimed | Ready, permit, timeout, direct cancel, inherited cancel and close each retain their sole winning cause |
| Competing selectors during the pause | Exact-generation ready/timeout/cancel/close and another resource offer all lose; no second notice |
| Recipient finishes during the pause | Finish cannot retire or consume the resource before claim publication |
| Cleanup abandons a visible claim | Retirement waits; after release the old notice is discarded |
| Abandoned selected permit | The permit remains recoverable exactly once |
| Reuse of the same route with a different task identity | Old-generation events lose; the new notice contains only the new token/task |
| Real mutex recipient cancelled after resource selection | The post-resume checkpoint returns cancellation, the ticket drains, and ownership is returned without exposing the value |
| Observer panic during the injected pause | RAII releases the publisher before scoped joins; the generation remains consumable |

The mutex test also confirms no barging while the ownership is selected and the
publisher is paused, exact waiter accounting, one successful value modification
without cancellation, zero modifications with cancellation, and unchanged carrier
identity across suspension. It uses the production mutex queue, linear capability,
resource-aware finish, checkpoint and ticket cleanup; it does not simulate ownership.

These are ordered native protocol tests, not exhaustive model checking of the
composed queue, signaling, lifecycle and scheduler. The existing Loom mutex model
uses the production wait-word definitions but counts routed wakes rather than
scheduling them; it does not prove this composition either.

## Consequences and next boundary

Do not move the selected-word store earlier as an isolated fix. Retirement currently
waits for that store before discarding a notice or permitting reuse. Earlier
retirement would let old publication overlap a reclaimed route or a newer generation.
The pause tests preserve evidence of that protection as well as its liveness cost.

A repair needs bounded owner progress without permitting early retirement or losing
the eventual publication notification. Before changing ordering, extract/model the
actual route-publication/retirement composition and exercise paused publishers,
active and sleeping owners, cancellation, ownership abandonment, stale notices and
route reuse. Keeping a claim out of a mounted task must also preserve idle wakeup
and cleanup progress; merely moving the spin into scheduler maintenance is not enough.

This finding is not a lost wake, leaked mutex, or proof of the cause of the historical
p99.9. The recorded microsecond intervals include deliberate fault injection, test
atomics, channels, logging and debug execution. Correlation with OS scheduling and
the uninstrumented latency distribution remains open. No throughput or May claim
is updated by this slice.

## Qualification

Final source SHA-256 is
`f5cde19ee14419b4ab6643170cfb5a9e6f959dbdd8dbad6ca876bdf3e1f57421`.
All 11 canonical gates passed under receipt
`/root/.cache/zcheck/run-1788635297-979072792-2315519/receipt.json`, including 527
all-feature runtime tests (one ignored). The separate default-native workspace run
passed 512 runtime tests (one ignored), 70 stack tests and all other workspace suites.
All 25 selected publication tests passed with optimized native fibers.
The native debug publication suite also passed 20 serial one-CPU invocations
(500 selected test executions). These are repeated ordered schedules, not exhaustive
interleaving coverage.

Raw output, replay commands, source patches, environment, executed-binary hashes
and the valid canonical receipt are preserved in
[`publication-f5cde19e.tar.gz`](evidence/publication-f5cde19e.tar.gz), SHA-256
`2fa11ac9c72cee83611daac9c3791231d422acc68216416dad940b0639f5c820`.
The archive does not contain executed binaries or constitute stable-release evidence.

Existing loaded-suite findings in
[wait-qualification-review.md](wait-qualification-review.md) remain open; this slice
neither modifies those tests nor silently treats them as resolved. The additional
completion-observer failure found during this slice is separately explained and
qualified in the linked completion review, not hidden by a quiet rerun.

The source inventory refresh adds three test files, with no architecture grants,
revocations, dependency or debt changes. HTTP remains separate.
