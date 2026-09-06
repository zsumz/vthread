# Selected timers: prove the winner before resumption

The loaded selected-timer finding is closed as an invalid test-ordering oracle,
without changing timer, parking or checkpoint behavior. The old captured failure
was an unlabeled five-second synchronization timeout; its exact schedule and gate
cannot be recovered. This is not evidence of a repaired production timer defect.

The old fixture blocked its carrier and waited for clock expiry. That prevented
the same carrier from selecting the timer, so elapsed time did not establish the
ordering the test claimed. Its unconditional parked-state gate also assumed that
admission guaranteed first mount before an inherited deadline.

## Ordered coverage

The replacement drives real native fibers and the production timer-expiry method
from an ordinary test thread. With explicit deadlines earlier than, equal to and
later than inherited expiry, it records the exact task route and park generation
as **Published before any resumption**, rejects a competing ready selection, and
only then permits resumption after inherited expiry.

An earlier explicit timeout remains `TimedOut`; equal/later timers return
`DeadlineExceeded`. The next checkpoint and separate root policy observation both
report the inherited deadline. Task panic/error outcomes and final park/wake/timer
accounting remain visible. No timer is rescheduled and no fake clock is used.
`Shared::wait` proves drainage, not final root policy; the test checks each apart.

A second ordered case accepts the packet before expiry but mounts it afterwards.
The valid typed `DeadlineExceeded` result has zero parks and zero pending waits.
That directly disproves the old unconditional parked-state oracle without relying
on host-speed assumptions. A failed first-park precondition now includes the actual
task result instead of only a watchdog failure.

## Negative controls

- Omit timer expiry: the required publication assertion fails `Stale` versus
  `Published`; elapsed time alone cannot satisfy it.
- Recheck current policy before retiring the selected park: the earlier explicit
  winner is incorrectly replaced by `DeadlineExceeded` and fails.
- Change inherited `<=` explicit to `<`: the equal-deadline case incorrectly
  returns `TimedOut` and fails.
- Require a pending park after deliberately late first mount: observed `(0, 0)`
  and the typed deadline result fail the old `(0, 1)` oracle.

All mutations are restored. Production files used for the checkpoint/tie controls
have no retained diff. The only runtime-file change declares the separate test
module; the reviewed architecture delta is one file/five contexts, no grants.

## Qualification

All fourteen canonical tasks pass under receipt
`run-1788695622-824131821-3035971`, preserving the repository state. Both required
default-native debug/release suites pass 542 runtime tests; all-features passes
571. Two explicitly manual/performance tests remain ignored.

Source SHA-256:
`889c2843ba7fbeebde25fe8f01237f02b6e2c9740942e4022d958e2487070a91`.
Ten further debug and ten release invocations on CPU 6 pass both tests: forty
Rust test executions, comprising eighty explicitly ordered subcases. These are
constrained correctness repetitions, not latency measurements.

The [113-file verified bundle](evidence/selected-timer-order-889c2843.tar.gz)
preserves the original failure, initial fixture compile error, all four negative
controls, restored source, full canonical output, commands, source/binary hashes,
repetitions and replay ledger. Archive SHA-256:
`0dfba52efb61fd30b8826767307d744da2be42486c3293d25ceee485d8eaf14a`.

These tests prove ordered runtime semantics, not OS scheduling latency, ARM64 or
sanitizer qualification, a new performance result, or the unique historical host
schedule. Three loaded findings remain unclassified: inbox refill, interrupted
cross-runtime join and deadline-first join. The historical cancellation-history
timing excursion remains a separate open release-performance finding.
