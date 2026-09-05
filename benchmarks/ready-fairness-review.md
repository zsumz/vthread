# Ready-queue fairness experiments

Runtime baseline: `1b39b48` (ARM64 terminal repair), with the Linux scheduler
unchanged from `6853fb8`. Documentation checkpoint: `4ce4f7a`.
This is a separate ready-policy slice; no wait-word ordering, channel ownership,
admission policy or context-switch optimization is bundled with it.
Retained: a two-newest-wake cohort. This is a qualified correctness/fairness repair
with explicit costs, not a free throughput win. FIFO and cohort-32 remain experiments.

## Reproduced failure and dispatch contract

The production queue failed the new mixed-work regression before any policy change.
It holds one old wake, one of two alternating hot wakers, and one normal task that
requeues itself. After 34 selections the normal task ran once and the old wake zero
times; the queue never exceeded three entries. The exact assertion is
`normal work erased the oldest-wake quota`.

The original counter resets after serving normal work, indefinitely restarting the
32 newest-wake allowance without requiring oldest-wake service. This is not a queue
capacity failure and does not require task migration or unbounded arrivals.

The bounded-cohort candidate reuses that one counter: serve at most B newest wakes,
then one normal head if present, then one oldest wake if present. Only then restart
the cohort. No extra allocation, atomic, queue entry or counter is introduced.
The normal head and oldest wake have a B+2 selection bound. An entry with N older
entries has a (B+2)*(N+1) bound if it stays queued; normal arrivals must join the back.
Cleanup's explicit front insertion changes normal FIFO rank. These are dispatch
opportunities, not wall-clock guarantees against non-yielding user code.

Both B=32 and B=2 pass the mixed-work regression, empty/refill checks, static
priority checks, and production-queue enumeration of every counter phase and every
combination of one through eight old wakes and normal tasks. Hot wakers replenish
continuously, normal tasks requeue, and the test checks every entry's deadline,
bounded queue length, unique old-wake selection and peek/selection agreement.
There are 2,176 starting cases for B=32 and 256 for B=2. This tests the actual
carrier-local policy; no new concurrent atomic protocol needs a separate Loom adapter.

## FIFO wake reference

FIFO passed the new mixed-work regression. This experimental reference was built
and timed, but was not fully canonically qualified or retained. Its policy-specific
newest-first tests do not describe FIFO. A is the original policy and F is FIFO;
each column is a fresh process in serial A/F/F/A order. Values are invocation-median
throughput-derived ns/operation, not individual operation latency.

| Workload, 4 carriers / 64 tasks | A1 | F1 | F2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Park | 95.54 | 103.60 | 100.02 | 91.08 |
| Historical channel | 140.79 | 151.01 | 143.99 | 140.31 |
| Timestamped wake throughput | 135.81 | 139.76 | 151.06 | 135.08 |

Each wake invocation records 5,760,000 atomic-timestamp observations:

| Individual wake statistic, ns | A1 | F1 | F2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Median | 201 | 1,952 | 1,993 | 210 |
| p95 | 1,021 | 4,657 | 5,218 | 1,222 |
| p99 | 68,553 | 5,098 | 5,648 | 70,486 |
| p99.9 | 113,912 | 8,472 | 9,084 | 113,872 |
| p99.99 | 145,450 | 20,661 | 21,513 | 145,409 |
| Worst-pair p99.9 | 118,319 | 10,246 | 10,005 | 116,237 |
| Maximum | 49,558,780 | 49,558,429 | 48,993,857 | 53,025,240 |
| Task-median range | 200-211 | 691-3,095 | 210-4,026 | 190-230 |

FIFO materially reduces p99 through p99.99 while increasing median latency and
slowing these throughput measurements. Approximately 50 ms maxima remain. This
establishes a ready-order tradeoff in this workload, not that the mixed normal/wake
starvation counterexample explains all historical tails. Timestamped ping-pong can
consume stored permits; it is not a count of only actually suspended tasks.

## Bounded-cohort comparison

Each row below uses six fresh processes in serial A/C32/C2/C2/C32/A order. C32
retains the old 32-wake allowance but makes oldest-wake service independent of
normal service. C2 shortens that same proven cohort to two newest wakes. Both fix
the counterexample; C32 alone does not substantially reduce these wake tails.

| Workload, ns/operation | A1 | C32a | C2a | C2b | C32b | A2 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Park | 107.39 | 90.90 | 92.66 | 96.08 | 90.97 | 95.80 |
| Historical channel | 154.62 | 142.02 | 147.67 | 145.12 | 141.38 | 153.17 |
| Timestamped wake throughput | 135.48 | 135.78 | 144.99 | 144.30 | 136.59 | 145.44 |

| Individual wake statistic, ns | A1 | C32a | C2a | C2b | C32b | A2 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Median | 200 | 201 | 240 | 240 | 210 | 220 |
| p95 | 590 | 1,212 | 12,158 | 13,181 | 451 | 701 |
| p99 | 71,999 | 65,959 | 13,530 | 13,601 | 81,022 | 86,761 |
| p99.9 | 120,822 | 112,650 | 16,545 | 16,835 | 143,326 | 143,998 |
| p99.99 | 146,221 | 145,139 | 29,716 | 27,191 | 152,912 | 153,272 |
| Worst-pair p99.9 | 140,762 | 120,573 | 17,456 | 17,647 | 146,851 | 146,270 |
| Maximum | 33,453,283 | 49,524,089 | 49,467,504 | 49,014,410 | 53,434,538 | 53,882,983 |

## Final two-wake regression panel

Fresh serial A/C2/C2/A processes, with no timed overlap. The change column compares
the means of the two invocation medians, not a confidence interval. Positive means
more time. Nine measured rounds per process except lifecycle (101 rounds).

| Workload | A1 | C2a | C2b | A2 | Change |
| --- | ---: | ---: | ---: | ---: | ---: |
| Contended mutex, 4 / 64 | 369.88 | 371.48 | 367.53 | 364.86 | +0.6% |
| Park, 4 / 64 | 92.05 | 92.68 | 93.58 | 92.58 | +0.9% |
| Historical channel, 4 / 64 | 145.53 | 149.34 | 148.91 | 140.54 | +4.3% |
| Yield, 4 / 64 | 13.47 | 14.54 | 13.50 | 13.43 | +4.2% |
| Lifecycle, 4 / 1,000 | 397.69 | 392.98 | 396.48 | 396.38 | -0.6% |
| Lifecycle, 4 / 10,000 | 333.16 | 338.92 | 336.98 | 334.79 | +1.2% |
| Uncontended mutex, 1 / 1 | 27.65 | 27.46 | 27.44 | 27.63 | -0.7% |
| Timestamped wake throughput, 4 / 64 | 136.37 | 142.70 | 150.91 | 144.10 | +4.7% |

| Individual wake statistic, ns | A1 | C2a | C2b | A2 |
| --- | ---: | ---: | ---: | ---: |
| Median | 201 | 240 | 240 | 210 |
| p95 | 761 | 13,060 | 13,219 | 701 |
| p99 | 72,138 | 13,601 | 13,760 | 82,164 |
| p99.9 | 140,913 | 16,575 | 16,916 | 142,544 |
| p99.99 | 148,273 | 27,822 | 28,553 | 152,320 |
| Worst-pair p99.9 | 142,094 | 17,977 | 18,077 | 144,138 |
| Maximum | 49,527,238 | 52,998,345 | 49,860,142 | 53,557,988 |
| Task-median range | 191-220 | 190-340 | 190-351 | 181-261 |

The candidate services an old wake and normal head within four selections and
reduces measured p99.9 about 8.5x here. It raises the median by 30-39 ns, substantially
worsens p95, and does not fix approximately 50 ms maxima. The distribution change
is important: servicing older wakes more frequently costs some handoff locality.
This is not a blanket latency improvement or a claimed May p99.9 win.

Single-carrier park counters, serial A/C2/C2/A on CPU 7, include process setup and
one warm-up plus three measured rounds: 25,600,000 operations per process.

| Counter | A1 | C2a | C2b | A2 |
| --- | ---: | ---: | ---: | ---: |
| Cycles | 13,221,745,367 | 13,483,001,055 | 13,514,894,658 | 13,206,539,958 |
| Instructions | 37,038,323,107 | 37,066,538,247 | 37,066,457,224 | 37,038,228,810 |

Mean cycles/operation increase from 516.18 to 527.30 (+2.16%); instructions increase
from 1,446.81 to 1,447.91 (+0.08%). The fairness repair does not demonstrate a cycle
reduction. Channel and wake throughput costs cannot be called zero regression.
Yield-only has one elevated candidate invocation despite unchanged normal-only
queue operations; this panel does not establish the cause of that variation.

## Measurement discipline and local evidence

Rust 1.96.1 / LLVM 22.1.2, Linux 5.15.0-187-generic, eight-vCPU KVM guest reporting
AMD EPYC 9555P. Process CPU mask 0-3, four unpinned carriers, 64 tasks/capacity.
Physical-host placement and competing host load are unknown. No builds, tests,
soaks, profiling, disassembly or other benchmarks overlap timed invocations.
Park/channel use 100,000 operations/task, one warm-up and nine measured rounds;
wake-tail uses 10,000 observations/task and the same round counts.

Raw logs, the failing regression, patches, source/environment manifests and local
immutable executables are in `target/finish-line/ready-fairness`. The logs, patches,
manifests, binary hashes and qualification receipt/logs are also committed in
[`evidence/ready-fairness-bd80c2f1.tar.gz`](evidence/ready-fairness-bd80c2f1.tar.gz),
SHA-256 `a60f5ed413af10412e64dbed83a9e8c0dcd5b02fc3735195eaa768f5b58071e8`.
This preserves raw evidence beyond the ignored directory; benchmark executables,
the ARM64 CI artifacts and a full release matrix are not bundled. Binary hashes:

- A: `64919d5f738974b381c0dce3f7499f43b0f46d9e2ad89c84458ffccac1ff03be`
- FIFO: `3732265afdfbf80e822ced97cddb2d07699831bafd5edd8cf470cac90498fac9`
- Cohort 32: `2dddc38b3ce5c292b936d3acb6c2b53bbcce716fcf514cbe6a5054085182b230`
- Cohort 2: `0ff35577c7b29e97bee1cf9bcc64cf0ba4e7b4ed4233defc6031fc9fe7f7de95`

```sh
timeout 120s taskset -c 0-3 BINARY vthread park 100000 4 64 9
timeout 120s taskset -c 0-3 BINARY vthread channel 100000 4 64 9
timeout 120s taskset -c 0-3 BINARY vthread wake-tail 10000 4 64 9
timeout 120s taskset -c 0-3 BINARY vthread mutex 100000 4 64 9
timeout 120s taskset -c 0-3 BINARY vthread yield 100000 4 64 9
timeout 120s taskset -c 0-3 BINARY vthread spawn 4 1000 101
timeout 120s taskset -c 0-3 BINARY vthread spawn 4 10000 101
timeout 120s taskset -c 7 BINARY vthread mutex-uncontended 10000000 1 1 9
timeout 120s perf stat -x, -e cycles,instructions -o COUNTERS.csv \
  taskset -c 7 BINARY vthread park 100000 1 64 3
```

The timed C2 build has source digest
`bd80c2f1d5cb4e38e456dfade253fdc43bdf3f2d4c54bf64bcdcb7a75f4b14c3`.
Its patch applies to documentation checkpoint `4ce4f7a`. The qualified patch adds
only the subsequent zrail analysis-count refresh; no Rust changed after timing.
There are no new architecture grants, revocations, dependencies or ratchets.

## Qualification and decision

- All 11 canonical gates passed: receipt
  `/root/.cache/zcheck/run-1788627442-909099854-2153379/receipt.json`, archived with logs.
- Default-native `cargo test --locked --workspace --all-targets` passed: 499 runtime
  tests (one ignored manual probe), 70 stack tests, and the remaining model/lab suites.
- Standalone benchmark tests (32) and all-target clippy passed.
- Sequential optimized default-native 60-second mixed soaks passed: one carrier
  completed 261,096 lifetimes and four carriers completed 340,101. All 601,197
  admitted lifetimes completed, services/wakes drained and shutdown assertions passed.

Retain this as the review's correctness-repair exception: the actual starvation
counterexample is closed with a four-selection queue-head bound, and timestamped
p99.9 improves repeatedly, in exchange for the measured cycle/throughput/p95 costs.
It is not accepted under a claim of cycle reduction or zero unaffected regression.
The next performance slices must use this fair scheduler as their baseline.

The historical channel retains the previous unequal-contract May comparison,
though this experiment compares only vthread policies. No May win, loaded-tail
qualification or release-ready claim is made. The old unexplained stall-test
failure remains open despite passing this run; admission fairness is a separate slice.
