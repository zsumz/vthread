# Handoff attribution before another policy change

This slice adds an opt-in `handoff-profiling` feature, not a performance policy.
It preserves the 640-probe idle budget, channel notification/ownership protocol,
wait publication ordering, affinity, admission and capacity limits. No May result
is rerun or improved by this patch. Capacity-independent maintenance, safe
out-of-lock channel publication and readiness scaling remain open.

Source SHA-256: `f57b1db1457fc45f855e561e5eb89e489c72bcf166ef4c50400596057840c976`,
on base `88e2fd1549ec840e9407c065f68fb6294d5d2cdc`.

## What is measured

The feature implies the existing clock-free `scheduler-profiling` feature but
does not add clocks to that feature when used alone. Fixed-size, owner-local
histograms distinguish the complete idle episode, work/control-only polling,
exhausted polling, idle snapshot publication, wait API, condition-variable call,
wake publication, claim completion, channel lock acquisition and lock-held time.
Channel counters partition calls, transfers, errors, actual park crossings,
successful park returns, retries and notification outcomes.

There are no new shared counter atomics or per-observation allocations. No
profile borrow crosses suspension. Totals are copied through existing carrier
snapshot publication and checked by the benchmark only after shutdown. The
report rejects inconsistent histograms, idle partitions, channel conservation,
active carriers and incorrect whole-runtime send/receive totals.

Interpretation limits are part of the contract:

- Regions overlap, include preemption and clock overhead, and are not additive
  CPU-time categories. Enabling clocks and larger snapshots changes scheduling.
- A `PollWork` result means a visible inbox/wake, not a validated task dispatch.
- `WaitApi` and actual `NativeWait` calls differ. A condition-variable call does
  not prove an OS deschedule; futex syscall totals include waits and wakes.
- Notification direction describes the recipient waiter. Attribution belongs
  to the executing source carrier; shared-hub routing is not necessarily a
  distinct-carrier transfer. Native callers without a carrier route are omitted.
- `Stored` includes coalescing an already stored permit, not unique future wakes.
  Ineligibility is an under-lock observation, not a promise about future state.
- Lock timing covers send/receive critical sections, not every channel metadata
  acquisition in ticket cleanup or endpoint destruction.

## Evidence panel

There are 72 counter processes: 12 cases, three independent processes each, in
both profiling and default builds. Case order reverses in the middle repetition.
The profiling and default panels run separately; they are not paired randomized
A/B performance acceptance. Raw ranges are retained, not just the medians below.

The shared-MPMC cases use one channel, capacities 1/64/1,024, one/four carriers,
8/64 live tasks, and an additional 65,536 task-capacity control. Other cases are
park, mutex, 320,000 total spawn/reclaims at spare capacity, native-origin burst
wakes at 1 ms/100 us gaps, and a one-second quiet control. Counts include startup,
one warm-up round, validation and shutdown. The burst control verifies actual
initial parks and exact acknowledgements, but is closed-loop, not offered load.

The host is an eight-vCPU KVM guest exposing AMD EPYC 9555P, Linux 5.15.0-187,
Rust 1.96.1 / LLVM 22.1.2. CPU masks and per-carrier pinning are recorded; spawn
and burst controls use normal placement within the process mask. Before/after
observations catch no guarded build process. Own builds do not overlap timing;
these checks cannot establish exclusive physical-host access. Guest frequency
policy and physical topology unavailable to the guest are not inferred.

### Useful channel progress

Each entry is the median of three **instrumented** process results. A value
denotes one send plus its corresponding receive; parks/retries include both.

| Carriers / tasks / channel capacity | Notifications/value | Ineligible notifications | Retries/value | Actual parks/value |
| --- | ---: | ---: | ---: | ---: |
| 1 / 8 / 1 | 4.000 | 50.00% | 1.000 | 3.000 |
| 4 / 8 / 1 | 3.971 | 49.86% | 0.616 | 2.564 |
| 4 / 64 / 1 | 3.976 | 49.96% | 0.586 | 2.575 |
| 4 / 64 / 64 | 3.915 | 10.91% | 0.184 | 2.183 |
| 4 / 64 / 1,024 | 3.906 | 5.93% | 0.086 | 2.040 |

This identifies protocol work without a corresponding immediate transfer. It
does not predict a 50% speedup: notifications have different outcomes/costs,
same-direction wakes can be necessary, and instrumentation changes contention.
The deterministic real-kernel regression separately reproduces an ineligible
wake, resumed retry/repark and disconnection exit without relying on timing.

### Polling and sleeping are workload-dependent

| Instrumented case | Poll episodes ending with visible task work | Native-wait calls / wait-API calls |
| --- | ---: | ---: |
| MPMC, 4 carriers / 8 tasks | 99.78% | 0.993 |
| MPMC, 4 / 64 | 97.13% | 0.985 |
| MPMC, 4 / 64, task capacity 65,536 | 58.31% | 0.248 |
| Park, 4 / 64 | 99.97% | 1.000 |
| Mutex, 4 / 64 | 99.15% | 0.996 |
| Burst, 4 / 8, 1 ms gaps | 18.33% | 1.000 |
| Burst, 4 / 8, 100 us gaps | 33.40% | 1.000 |

These percentages concern episodes that actually enter polling, not all
dispatches. Changed-epoch/no-work hits are negligible in these particular runs.
Exhausting 640 probes usually costs about 16-18 us in the timed build. The burst
cases perform about 4,011 and 8,015 native waits across all carriers respectively;
the current profiler reports the sleeping path distinctly instead of treating
every signal check as useful work. This supports retaining burst CPU as an
independent gate, not extending polling from the busy channel result alone.

### Spare capacity is a confirmed production hotspot

At 64 live tasks, the default uninstrumented capacity-1 MPMC result moves from
1,378 ns/value at task capacity 64 to 21,123 ns/value at capacity 65,536. These
are whole-round throughput-derived medians, not individual operation latency.
Whole-process median cycles rise from 2.68 billion to 35.04 billion for the same
256,000 warm-up-plus-measured values.

A separate default-build cycle sample assigns **63.82% of sampled user-cycle
weight to `Kernel::snapshot`**. Its instruction annotation locates virtually all
of that function's sampled weight in the four-slot-unrolled pending-depth scan.
This is one 1,677-sample process, not a precise stable percentage or proof that
all runtime overhead is in that loop. A tightly provisioned sample instead puts
40.60% in the combined channel send/receive symbols and 30.21% in the carrier
thread wrapper; inlining prevents assigning the entire wrapper to idle polling.

The instrumented idle-publication mean rises from approximately 121 ns to
51.6 us. Only about a quarter of subsequent wait-API calls invoke the condition
variable. This demonstrates why observation cost and sleep/admission behavior
cannot be optimized independently. It does not establish that every skipped
native wait was caused by an arrival during that scan.

Keep the previously rejected scan-free experiments rejected. The next capacity
experiment must replace accidental pacing with bounded, intentional service and
qualify lifecycle/admission, mixed synchronization and burst CPU together. Do not
restore the scan to hide a pacing regression or add an unproven shared RMW to
every wake merely to make the observation constant-time.

### Do not mistake instrumentation for production contention

The default shared-channel syscall medians are 1,729 `sched_yield` calls for
160,000 values at 4/8 and 1,267 for 256,000 values at 4/64. With timing enabled
they rise to 127,071 and 69,132 respectively. Default ranges are wide too:
595-44,014 and 490-22,893 calls. The profiler significantly lengthens metadata
critical sections. Its lock durations cannot establish the production lock
budget or justify changing `SpinMutex`'s backoff.

Two attempted syscall call-stack recordings additionally distort execution and
report lost chunks (17 and 86). Their decoded stacks locate channel call sites,
but they are retained as **non-quantitative diagnostic attempts**, not accepted
frequency/latency evidence. The two separate cycle recordings report no lost
chunks. Raw records, warnings, reports and annotation are preserved.

Natural claim-finish observations are rare here, generally zero to a few per
process; one spare-capacity observation reaches about 38 us. The ordered
publisher-pause test verifies that both sides of the existing dependency are
counted. Neither rarity nor a clocked sample disproves the preemption hazard,
explains an end-to-end p99.9, or authorizes changing Claim/Selected ordering.

## Qualification and decision

The opt-out build's complete `.text` section is byte-identical to the pushed
baseline: SHA-256 `fa50a946a3d6771e66ab4a3d28421b49fc27dbd3371f64af97f80236d06085dd`.
The full executables differ in metadata/location data; they are not claimed
byte-identical. Binary hashes, exact sources and the superseded two-byte codegen
experiment are retained. The final patch restores the original branch expression.

All 11 canonical gates pass with preserved repository state under receipt
`run-1788653744-301476461-2531550`. The architecture refresh grants no new
permissions or debt and adds an exact feature world rather than widening macro
allowances. Separate native-stack qualification passes 515 default runtime tests,
529 with handoff profiling (one existing manual probe remains ignored in each),
the native workspace/stack/protocol suites, 52 standalone benchmark tests, strict
benchmark Clippy and three burst-control/report tests. Two optimized feature-on
mixed-traffic soaks complete 266,823 task lifetimes across one/four carriers with
exact completion and park/wake accounting. These short soaks are not release
burn-in. Correction after the `0b67337` source audit: both default and all-features
use the native `vthread-stack` engine; no corosensei backend remains. All-features
enables diagnostics/evidence and does not replace default-feature debug/release
runtime qualification. No new atomic protocol or unsafe boundary is introduced. This is
not full ARM64, sanitizer, loaded-tail or release proof.

The next small optimization should target **useful channel notification and a
cancellation-safe reservation/publication boundary**, with default cycles and
tails measured independently of these clocks. Capacity-independent maintenance
needs its own admission/idle experiment. Incremental readiness remains another
separate slice. No fixed/adaptive polling prototype is accepted by this evidence.

The [source-keyed archive](evidence/handoff-attribution-f57b1db1.tar.gz) contains
the exact source patch, binary/text hashes, commands, 72 raw counter processes,
four raw compressed perf recordings, parser and derived results, host manifests,
canonical receipt/logs, native/benchmark checks and soaks. `LEDGER.md` identifies
accepted evidence, exploratory failures and replay commands. Executables are
not included; the artifact index records the archive checksum.
