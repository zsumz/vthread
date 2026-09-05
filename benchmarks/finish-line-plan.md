# Finish-line review plan

Starting checkpoint: `6853fb8` on `perf/scheduler-hot-path`. The user's follow-up
review changes the order of work: correctness and bounded progress precede further
handoff tuning. The pasted review is available in the session; its linked ZIP and
separate documents are not mounted in this workspace. Findings are checked against
the source rather than treating the supplied patches as already qualified.

Each item is a separate source and performance-attribution baseline. HTTP remains
out of scope. The overall May goal and stable-release qualification remain open.

| Order | Slice | Exit evidence | State |
| --- | --- | --- | --- |
| 1 | ARM64 terminal result register | Real ARM64 debug/optimized completion, yield/park, panic, forced unwind, reuse and FP state | Defect confirmed in source; regression/CI preparation |
| 2 | Ready-queue bounded fairness | Old code fails mixed hot-wake/normal-work counterexample; repaired dispatch bound and measured tradeoffs | Pending |
| 3 | Pending-admission fairness | Real late starts under sustained mixed park/yield/wake and borrowed work | Pending |
| 4 | Unexplained stall test | Actual failure state; ordered evidence replacing unproven temporal assumptions | Pending |
| 5 | Handoff publication evidence | Publisher-pause regressions and stage attribution without changing production ordering | Pending |
| 6 | Capacity/admission/idle interaction | Scan-free maintenance without rejected lifecycle/idle costs, multiple provisioned capacities | Pending |
| 7 | Useful channel handoff | Notification/progress counts; reserve under lock, publish outside, cancellation/close/panic proof | Pending |
| 8 | Mutex round-trip cost | Local/remote and active/sleeping recipient attribution; exact ownership and waiter progress | Pending |
| 9 | Incremental readiness | Bounded command processing proportional to changes/events and large-connection validation | Pending |
| 10 | Release evidence | Durable source-keyed cross-platform correctness, stress, footprint, CPU and performance bundle | Pending |

## Slice rules

- Add a reproducer before changing scheduler or stack state. Record where it ran;
  compilation or a runner label does not establish native execution.
- Express cooperative progress guarantees in dispatch opportunities, not elapsed
  nanoseconds. Preserve bounded queues and exact generation/ownership semantics.
- Keep ready policy, wake-word ordering and channel payload ownership in separate
  changes. No post-mount migration or weakened checkpoints to improve a headline.
- Preserve rejected experiment evidence. Fewer instructions or atomic operations
  alone are not a reason to retry already rejected ownership/polling designs.
- Separate default-contract comparisons from controlled mechanism experiments,
  throughput-derived ns/op from sampled latency, and permits from actual suspension.
- Qualify unaffected workloads, provisioned capacity, tail and worst-task progress.
  A correctness repair can justify a measured cost; report it rather than calling
  it neutral. Performance candidates still require the review's acceptance evidence.
- Preserve raw failures, commands, source/binary identity, seeds and environment.
  Temporary local artifacts and expiring CI artifacts alone are not stable-release
  evidence. Unexplained correctness failures remain release blockers.

## ARM64 initial audit

`context_finish(restore, transfer)` restores the parent link register and returns
directly to the suspended `context_switch` caller. The current `mov x2, x1` does
not put the scalar result in `x0`; the ordinary switch's `mov x0, x2` is not on
that return path. The fix is `mov x0, x1`, consistent with
[AAPCS64 result return](https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst#69-result-return)
and the [Rust naked-assembly contract](https://doc.rust-lang.org/reference/inline-assembly.html#rules-for-naked-inline-assembly).

The existing workflow does not trigger on perf-branch pushes. A read-only GitHub
Actions query returned zero runs for this branch at the start of the slice.
The added native-stack workflow checks the actual host architecture and Rust host
triple, executes the native crate in debug and optimized profiles on Linux x86-64
and macOS ARM64, and uploads source-keyed logs. This workflow is a route to proof,
not proof until its exact source revision executes successfully.

The new terminal regressions cover immediate completion and 256 same-stack reuses,
completion after yield and park with distinct resume decisions, exact panic payload
and destructor behavior across 32 reuses, and forced unwind through nested parked
frames. Existing architecture tests cover floating-point control and live register
preservation. Linux results cannot validate the macOS assembly path.

The pre-repair test/CI slice passed all 11 local canonical gates under receipt
`/root/.cache/zcheck/run-1788620068-145511225-2138749/receipt.json`.
All 70 default-native stack tests passed locally in the optimized profile; the
four new terminal regressions also passed in debug. The lock refresh changes
source inventory only, with zero new architecture grants or revocations. The
terminal instruction is deliberately still unchanged in this first checkpoint
so the real ARM64 CI run can serve as a negative control.
