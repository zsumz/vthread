# Wake dequeue experiment: 2026-09-05

The forced-inline runtime experiment is not retained: its mutex gain did not clear
the cross-workload tail guardrail. The immutable baseline is the runtime at `ea5a919` on
`perf/scheduler-hot-path`; its benchmark binary was built from the equivalent
source checkpoint `88bfdad`.

## Profile and code generation

Serial cycle profiles used four carriers/workers, 64 tasks, 100,000 mutex
acquisitions per task, one warm-up, and nine measured rounds. Sampling frequency
was 199 Hz. Vthread recorded 10,447 samples and May 2,160, with no lost samples.
These are sampled profiles, not authoritative throughput comparisons.

Vthread's carrier-entry function accounted for 63.71% of sampled cycles. Its
instruction annotations concentrated 93.77% of that function's weight immediately
after the idle-loop `pause`. The single shared mutex makes other carriers wait;
this profile does not establish that all those cycles can be removed while keeping
handoff latency. Wake dequeue accounted for another 6.99%; its annotations placed
92.66% of local weight immediately after the wake-head exchange. Sampling skid
prevents treating either adjacent instruction as an exact cost attribution.

The candidate marks `WaitHub::pop` `#[inline(always)]`, allowing the owner scheduler
to consume a wake without a separate aggregate-return call. No queue, wake selection,
memory ordering, spin bound, allocation, cancellation checkpoint, or fairness policy
changes. The ordinary `#[inline]` hint produced byte-identical `.text`, so no timing
comparison was attempted for that no-op. Its `.text` SHA-256 was
`6c25813ce3364bb5165d279c217dead9ee3e77fa9dd14753153419ee990cbc2b`, matching the baseline.

The forced-inline candidate removes the calls to `WaitHub::pop` from
`Kernel::process_wakes`. That function shrinks from 3,523 to 3,125 bytes in this
release build. The targeted native tests passed: four wait-hub tests, seven
wake-queue tests, and twelve mutex tests. This is not full qualification.

## Protocol and environment

Rust 1.96.1 / LLVM 22.1.2, Linux 5.15.0-187-generic, eight-vCPU KVM guest presenting
an AMD EPYC 9555P. Processes were restricted to guest CPUs 0-3. No builds, tests,
soaks, or other benchmark invocations ran concurrently with timing. Physical-host
placement is unknown; this guest shows substantial timing variation.

May 0.3.51 defaults `PIN_WORKERS` to true in `src/config.rs`, and its scheduler
pins individual OS workers to the available cores. Vthread does not pin individual
carrier OS threads. Restricting both processes with `taskset` therefore does not
give them identical per-worker placement. Neither default was changed for this
experiment. May's mutex warm-up also observed migration in all 64 coroutines;
that observation does not describe every timed handoff or quantify migration's
effect on the comparison.

A denotes the baseline, B the forced-inline candidate. All elapsed-time tables
use serial A/B/B/A invocations. Normal throughput runs use 100,000 operations per
task, 64 tasks, four carriers, one warm-up, and nine measured rounds. Vthread
capacity is 64. Values are invocation medians in ns/operation.

| Initial comparison | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Mutex | 368.98 | 351.16 | 354.08 | 378.84 |
| Park | 88.86 | 99.23 | 91.25 | 90.20 |

The mutex improvement in this first comparison is about 5-7%, but park regresses
in the same experiment. The mutex result alone is insufficient to accept it.

## Counter checks

Each counter group repeats a fresh process three times. Each process includes
one warm-up plus three measured rounds, or 25,600,000 operations. Means include
process setup and idle-carrier work, not only the dequeue instructions.

| Park counter | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Cycles | 21,890,315,412 | 20,654,102,512 | 20,385,322,368 | 22,629,663,783 |
| Instructions | 39,547,391,521 | 38,668,425,412 | 38,597,143,022 | 39,773,173,351 |
| Cycle variability | 2.51% | 1.79% | 0.97% | 2.27% |

| Mutex counter | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Cycles | 117,063,724,561 | 81,951,065,754 | 88,321,366,620 | 123,954,231,155 |
| Instructions | 72,953,556,593 | 61,587,817,178 | 62,283,455,766 | 74,963,942,336 |
| Cycle variability | 7.08% | 19.71% | 7.36% | 4.93% |

The cycle and instruction reductions occur in both counter orders. Mutex cycle
variability is too large to present the percentage as a stable throughput gain.
One B1 counter invocation also recorded an 11.28-second whole-round maximum;
that outlier remains in the raw evidence. Counter-run round maxima are not
individual handoff latency percentiles.

## Cross-workload checks

The repeat park comparison is approximately flat. Channel results remain variable.
Spawn uses 10,000 tasks and 501 measured rounds, or 5,020,000 lifetimes including
warm-up per invocation; its values below are ns/task.

| Verification comparison | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Park | 92.27 | 92.15 | 91.91 | 90.61 |
| Historical channel | 137.84 | 134.18 | 139.55 | 145.46 |
| Yield | 15.58 | 13.33 | 14.09 | 15.40 |
| Spawn/reclaim | 343.78 | 335.21 | 341.37 | 340.49 |

Each wake-tail invocation collected 5,760,000 measured observations with 10,000
operations per task and nine measured rounds. Values are individual wake-to-resume
nanoseconds, not whole-round percentiles.

| Wake metric | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Median | 201 | 201 | 201 | 201 |
| p99.9 | 140,002 | 143,356 | 142,094 | 139,100 |
| p99.99 | 149,045 | 151,268 | 150,206 | 146,071 |
| Worst-pair p99.9 | 141,144 | 145,319 | 143,156 | 142,394 |
| Task-median range | 180-230 | 190-230 | 181-250 | 180-240 |
| Maximum | 61,035,314 | 49,703,000 | 49,852,836 | 49,577,340 |

Median latency is unchanged, but candidate p99.9 is approximately 2% higher in
this batch. That motivated the longer balanced follow-up below. No new May win
is claimed.

### Longer tail follow-up

Each invocation instead used 100,000 operations per task and nine measured rounds:
57,600,000 wake observations per invocation, 230,400,000 across the four processes.
The task count, capacity, carrier count, affinity mask, and engine settings were
unchanged. Values are individual wake-to-resume nanoseconds.

| Wake metric | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Median | 210 | 210 | 200 | 210 |
| p99.9 | 137,587 | 115,394 | 140,421 | 117,086 |
| p99.99 | 146,190 | 142,425 | 148,664 | 144,519 |
| Worst-pair p99.9 | 140,552 | 116,917 | 141,903 | 138,379 |
| Task-median range | 181-230 | 190-230 | 190-220 | 191-221 |
| Maximum | 88,991,900 | 85,011,178 | 77,107,484 | 89,487,746 |

The tail ordering reverses: B1 is lower than A1, while B2 is higher than A2.
These data do not establish a tail improvement or a stable magnitude of regression;
the smaller initial increase is not hidden or discarded. They also do not complete
the load/topology/cancellation/deadline fairness matrix requested in the review.

## Qualification and decision

The candidate passed all 11 canonical gates under receipt
`/root/.cache/zcheck/run-1788584286-206390977-1970456/receipt.json`. The separate
default-native workspace suite passed, including 484 runtime tests, 66 stack tests,
11 synchronization-core tests, 19 lab tests, and the alias test. No architecture
grants changed. These passes do not resolve the mixed performance evidence.

An earlier concurrent native run failed the unrelated-runtime Drop test, which
required the drop task to execute within 150 ms. The retained test now waits for
explicit inner-task entry, holds that carrier behind a gate, and requires the
foreign Drop to return before releasing it. A blocking Drop still fails the event
ordering when the safety watchdog releases the inner task. No runtime shutdown
code was changed.

The candidate lab binary completed two 60-second mixed soaks:

| Carriers | Tasks/batch | Batches | Completed lifetimes | Mutex updates | Parks/wakes each |
| --- | ---: | ---: | ---: | ---: | ---: |
| 4 | 512 | 572 | 295,724 | 2,342,912 | 3,222,276 |
| 1 | 64 | 5,392 | 372,048 | 2,760,704 | 3,941,553 |

Both checked payloads, affinity, queue/resource drain, and reclamation. These runs
overlapped correctness qualification and are not throughput comparisons. Candidate
lab SHA-256: `0bfbb51aac40f409e29b4dfc351d268afd4801b924fbcc3cd65bd35c7722aac9`.

Despite functional correctness and lower counter totals, the first park comparison
regressed, the short wake-tail comparison increased p99.9, and the longer follow-up
did not establish a consistent ordering. Under the review's no-worse-tail rule,
the 5-7% initial mutex gain is insufficient. The one-line runtime change is removed;
its complete reproduction is the `#[inline(always)]` attribute on `WaitHub::pop`
described above. The ownership regression and this evidence are retained.

After removing the candidate, the retained tree passed all 11 canonical gates under
`/root/.cache/zcheck/run-1788585608-600866428-1981399/receipt.json` and the separate
default-native workspace suite (484 runtime tests, 66 stack tests, 11 sync-core tests,
19 lab tests, and one alias test). The rebuilt benchmark is byte-identical to the
immutable baseline binary. The architecture lock is unchanged.

## Next experiment boundary

The sampled owner-head exchange remains the next protocol target. A bounded first
prototype can give the first bitmap word of routes a versioned mailbox while keeping
the existing bounded queue for overflow routes. A producer toggles its route bit with
a Release RMW; the sole owner observes the word with Acquire and keeps acknowledgements
locally, avoiding a consumer exchange on the producer word. No implementation or
performance claim for this protocol exists yet.

Model it before changing the runtime. Required invariants include one outstanding
publication per route; acknowledgement before freeing a route for republication;
payload visibility through the publication word; stale-token/task rejection; route
reuse without parity cancellation; and arming/sleep notification without lost wakes.
Capture a bounded consumption batch so rapid reuse cannot starve older notices,
and preserve fairness between inline and overflow lanes. Retain the existing capacity
observations and admission policy for this experiment to isolate it from the rejected
snapshot/idle-policy work. Synchronization FIFO/no-barging, eight-byte ready indices,
and task affinity remain unchanged requirements.

## Reproduction and local artifacts

Build immutable binaries before timing; do not rebuild during measurements.

```sh
cargo build --release --locked --manifest-path benchmarks/Cargo.toml
taskset -c 0-3 BINARY vthread mutex 100000 4 64 9
taskset -c 0-3 BINARY vthread park 100000 4 64 9
taskset -c 0-3 BINARY vthread channel 100000 4 64 9
taskset -c 0-3 BINARY vthread wake-tail 10000 4 64 9
taskset -c 0-3 BINARY vthread spawn 4 10000 501
perf stat -x, -r 3 -e cycles,instructions taskset -c 0-3 BINARY vthread mutex 100000 4 64 3
perf record -F 199 -e cycles -o PROFILE.data taskset -c 0-3 BINARY vthread mutex 100000 4 64 9
```

Baseline binary SHA-256:
`6a9b1b28d8e601f1270bb5e3a9898e55280325b34483515b97f8a55e4add56b8`.
Forced-inline binary SHA-256:
`1211d58abbf1e774bd59daccf540f9643e3d0a884463ff70c06e2aa46189e657`.
Raw logs and counters use `/tmp/vthread-fused-pop-*.log` and `*.perf`. Baseline
profiles are `/tmp/{vthread,may}-ea5a919-mutex-profile.data`, with corresponding
`*.log` output and `/tmp/vthread-ea5a919-{carrier,wake-pop}.annotate` disassembly.
These are local experiment artifacts, not a release-evidence bundle.
