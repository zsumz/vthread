# Wake mailbox integration experiment: 2026-09-05

The runtime integration is **not retained**: both candidates regressed four-carrier
handoffs. The runtime keeps its bounded MPSC queue, publication FIFO, capacity
accounting, idle policy, and eight-byte ready indices. The retained slice adds
proof coverage and repairs the experimental mailbox's sleep-handshake ordering.
No new May win is claimed.

## Retained correctness work

The [initial mailbox slice](mailbox-review.md) did not cover payload/overflow and
notification composition. Additional shared implementation tests cover payload
reservation, exact-once consumption, reuse after acknowledgement, and fairness
between captured inline and overflow batches. `WakeInbox` is now test-only: it is
neither exported by the synchronization core nor used by the runtime.

The 15 Loom scenarios use at most three threads and 1,000 branches per execution.
Exceeding the branch bound fails; there is no preemption, permutation, or time
cutoff. Optimizing only the Loom and generator test dependencies reduced a complete
run from roughly 437 seconds to 41 seconds without reducing exploration. Production
profiles and runtime test optimization are unchanged.

The notification composition follows `Signal::wait_while` with an unchanged epoch
and no deadline: register under the gate, arm the queue, and release the gate into
the condition-variable wait. The producer publishes work and checks the waiter count
before taking the gate and notifying. This is an explicit small model of that
ordering, not execution of native stacks or the complete Signal epoch/deadline protocol.

That composition exposed a lost wake in the experimental mailbox: a Release-only
route toggle could report `SLEEPING` without acquiring the owner's earlier waiter
registration. The waiter-count load could then see zero and skip notification.
An Acquire observation of the publication word after the Release toggle closes
this gap. Weakening that observation to Relaxed reproduces the modeled deadlock;
Acquire was restored. The runtime had never adopted the faulty mailbox.

Discarding the toggle's old value also lets this x86-64 build emit `lock xor`
instead of a value-returning compare-exchange loop. That code-generation improvement
does not establish a runtime performance improvement.

New deterministic kernel tests reject old wait generations before and after reuse
of a physical task route, verify a changed task identity, and require that only
the live generation resumes. Queue tests cover all five wake causes, owned and
borrowed routes, packed generation boundaries, and concurrent reuse across low and
high route indices. Original queue layout and publication-order assertions remain.

## Candidates and measurement protocol

The baseline is the retained runtime at `cc61c3f`, with the byte-identical benchmark
artifact from `88bfdad` documented in the [dequeue review](dequeue-review.md).
The first candidate uses a 63-route parity mailbox and the old bounded MPSC protocol
for overflow. It preserves one reserved payload per encoded route and alternates
consumption lanes. It adds fixed carrier-local state, not per-wake allocation.
The revision additionally inlines the empty overflow probe, outlines only the cold
batch exchange/reversal, and uses the Release toggle plus Acquire sleep observation.

Runs used Rust 1.96.1 / LLVM 22.1.2, Linux 5.15.0-187-generic, and an eight-vCPU KVM
guest reporting an AMD EPYC 9555P, microcode `0x1000065`. Timed processes used guest
CPUs 0-3; physical-host placement and frequency policy are unknown. No builds, tests,
soaks, disassembly work, or other benchmarks overlapped these timed comparisons.

Each invocation has four carriers, 64 tasks, capacity 64, 100,000 operations per task,
one warm-up, and nine measured rounds. Tables give medians in ns/operation in serial
A/B/B/A order. A is the baseline; B is the named candidate. Park/channel observations
report 32 cross-carrier pairs and no same-carrier pairs. Placement was not changed.

| First candidate | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Mutex | 357.97 | 404.90 | 405.69 | 361.34 |
| Park | 94.32 | 97.47 | 98.86 | 90.96 |
| Historical channel | 150.85 | 151.25 | 158.66 | 138.78 |

| Revised candidate | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Mutex | 356.89 | 386.29 | 377.27 | 362.52 |
| Park | 90.97 | 102.96 | 98.07 | 90.84 |
| Historical channel | 138.64 | 141.87 | 141.55 | 141.91 |

Revised mutex whole-round maxima, normalized per operation, were
368.34/404.15/392.52/385.19 ns; park was 111.93/110.43/106.44/102.38 ns; channel was
158.14/161.25/183.55/159.79 ns. These are **not individual latency percentiles**.
Throughput regressions already reject both candidates; new wake-tail, lifecycle,
and May qualification was not claimed or replaced with whole-round tails.

The historical channel remains bounded-one vthread versus May's unbounded MPSC.
An exact-capacity semaphore-gated comparison is a separate suite. May's default
worker pinning and coroutine migration also differ from vthread's fixed task affinity;
equal process CPU masks do not establish equal handoff topology.

## Counter diagnostic and unusually slow invocation

A baseline invocation requesting cycles, instructions, and three locked-event counters
did not complete within more than eight minutes. It was confirmed live, profiled for
diagnosis, then terminated; it is not a completed counter result. A three-second
software-clock profile collected 469 samples with no reported loss: about 47% in
carrier entry, 41% in a kernel wake path under `WaitHub::push`, and 7% in a kernel
wait path. Carrier annotations concentrated around the idle-loop pause. This does
not establish a deadlock or the cause of the slowdown.

A fresh unprofiled baseline completed at 359.46 ns/operation. Bounded single-process
counter runs requesting only cycles and instructions also completed normally:

| Mutex diagnostic | Baseline | Revised candidate |
| --- | ---: | ---: |
| Cycles | 98,116,152,486 | 103,310,700,363 |
| Instructions | 67,766,307,351 | 73,589,914,041 |
| Measured-round median, ns/op | 376.88 | 385.32 |

Each includes one warm-up and three measured rounds, or 25,600,000 operations, plus
setup and idle-carrier work. This single-order diagnostic corroborates rejection;
it is not a balanced repeated estimate of a portable effect. The interrupted run
was not included in those totals.

## Qualification and reproduction

Before its final code-generation revision, the candidate passed all 11 canonical
gates under `/root/.cache/zcheck/run-1788589301-264272553-2034255/receipt.json` and the
separate default-native workspace suite. After that revision, all 484 native runtime
tests, 27 synchronization-core tests, and 15 modeled scenarios passed. Functional
passes did not override the performance rejection.

After removing the integration, all 11 canonical gates passed under
`/root/.cache/zcheck/run-1788592984-786203909-2065796/receipt.json`. The final default-native
workspace rerun passed 486 runtime tests, 66 stack tests, 27 synchronization-core tests,
15 modeled scenarios, 19 lab tests, and the alias test. Its log is
`retained-native-final.log` in the artifact directory below. The initial retained-tree
canonical attempt caught leftover public visibility on the now-test-only router;
visibility was restricted before these successful checks. No lint was exempted.

The architecture update changes only analyzed source inventory and counts: no grants,
revokes, dependency closure, feature world, macro allowance, or unsafe policy changes.
These checks do not complete sanitizer, architecture, loaded-tail, or
ten-million-lifetime qualification.

The rebuilt benchmark's `.text`, `.rodata`, and `.data.rel.ro` are byte-identical to
the baseline, with identical load-segment layouts and entry point. Its `.text` hash
is `6c25813ce3364bb5165d279c217dead9ee3e77fa9dd14753153419ee990cbc2b`.
Comparing the file-backed load segments finds differences only in the GNU build ID,
not executed instructions or data. Retained full executable SHA-256 is
`a46d9bc19b857fb686aec4137c56e33ec92c46b8ec06b90951f2ecbd626a086f`.

```sh
cargo build --release --locked --manifest-path benchmarks/Cargo.toml
timeout 120s taskset -c 0-3 BINARY vthread mutex 100000 4 64 9
timeout 120s taskset -c 0-3 BINARY vthread park 100000 4 64 9
timeout 120s taskset -c 0-3 BINARY vthread channel 100000 4 64 9
timeout 60s perf stat -x, -e cycles,instructions taskset -c 0-3 BINARY vthread mutex 100000 4 64 3
cargo test --locked -p vthread-sync-core --all-targets
```

Raw local artifacts are in `/tmp/vthread-mailbox-routing-woVPjH`; these are experiment
artifacts, not a release-evidence bundle. Logs include `{mutex,park,channel}-{a1,b1,b2,a2}.log`,
their `revised-` counterparts, and `*-counter-recheck.{log,perf}`. Stalled-run profiles
use `stalled-baseline*`. Two source archives overlay the relevant files on `cc61c3f`
and preserve the rejected integrations for reproduction.

| Artifact | SHA-256 |
| --- | --- |
| `baseline` | `6a9b1b28d8e601f1270bb5e3a9898e55280325b34483515b97f8a55e4add56b8` |
| `candidate` | `c69958dd50d8e24e864d4d28a8818c4757476a14b9c6204ee63d5532f7115548` |
| `revised` | `ece01d7725396f764a9d6212903a54787bcc96b4365bf7f0fa5aba6d33fdc174` |
| `candidate-source.tar.gz` | `206e1b4d85309a6e05eb771f42289faba0d81b7833de80f39797836f654433e4` |
| `revised-source.tar.gz` | `1d2231fbd1b244fc78dcef364dab1589b33bb4741f5870c2d80051e3dfd42b27` |
