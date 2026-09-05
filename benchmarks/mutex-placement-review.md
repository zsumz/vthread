# Mutex layout and worker placement: 2026-09-05

No core runtime optimization is retained in this slice. A cache-line padding
experiment did not establish a repeatable win and was removed. The retained change
adds an explicit, verified Linux-only benchmark control for carrier CPU pinning.
It leaves vthread's default configuration and all runtime source unchanged.

## Protocol and environment

Rust 1.96.1 / LLVM 22.1.2, Linux 5.15.0-187-generic, eight-vCPU KVM guest reporting
an AMD EPYC 9555P. Processes use guest CPUs 0-3; physical-host placement and frequency
policy are unknown. Each invocation runs four carriers/workers, 64 tasks, 100,000
mutex acquisitions per task, one warm-up, and nine measured rounds. Vthread capacity
is 64. Values below are invocation medians in ns/operation in serial A/B/B/A order.
No builds, tests, soaks, or other benchmarks overlapped the timed invocations.

## Rejected layout experiment

The candidate adds `#[repr(align(64))]` to `MutexQueue`, separating waiter bookkeeping
from the protected value/ownership cell without changing atomic operations or queue
policy. A layout assertion failed on the baseline and passed on the candidate for
zero-sized, word-sized, and 128-byte values. The word-sized mutex becomes 128 bytes;
the zero-sized instance grows from 80 to 128 bytes. All 13 targeted mutex-related
tests passed, including FIFO, exclusion, selected cancellation, and panic handoff.
This was targeted qualification, not a full candidate qualification.

| A = baseline; B = padded queue | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Mutex ns/op | 370.01 | 365.92 | 374.85 | 363.33 |
| Whole-round maximum / operations | 390.95 | 394.41 | 407.05 | 417.77 |

The footprint cost is clear; the throughput benefit is not repeatable. The padding
and its layout assertion were removed. No cycle reduction or individual tail-latency
improvement is claimed from these whole-round measurements.

## Explicit carrier pinning

`--pin-carriers` selects distinct CPUs from the main thread's allowed mask before
constructing the runtime. After construction, it finds only this benchmark's own
carrier TIDs, pins each with `taskset`, and verifies the singleton mask through
procfs before starting warm-up. It does not admit setup tasks or alter task placement.
Linux truncates the longer Rust carrier name in `comm`; the reported sorted TID rank
is not asserted to equal runtime `CarrierId`.

The flag rejects insufficient CPUs, unavailable support, failed pinning, and failed
readback. CPU-mask expansion is bounded by the requested worker count. Default runs
do none of this pinning work. The configuration line records the mode, and each
successful explicit binding has a `phase=worker-affinity ... verified=true` line.
No dependency, runtime API, scheduler, cancellation, affinity, or shutdown policy changes.

Both sides below use the same immutable benchmark executable. The only control
change is the flag; the process CPU mask remains 0-3.

| A = default; B = explicitly pinned | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Mutex ns/op | 362.86 | 364.93 | 361.29 | 371.28 |
| Whole-round maximum / operations | 381.62 | 375.30 | 374.96 | 387.36 |

These runs are approximately flat. They do not support attributing the large mutex
gap to OS-worker pinning alone, nor establish a general pinning speedup. No loaded
tail, fairness, or cross-workload performance matrix was completed in this slice.

## Refreshed May comparison

This next comparison keeps the explicit vthread pinning and May 0.3.51's unchanged
defaults. May's configured worker pinning is source-verified; this flag does not
add kernel readback for May's internal pinning calls. Equal process CPU masks and
pinning intent still do not make the coroutine migration policies equivalent.

| A = May; B = explicitly pinned vthread | A1 | B1 | B2 | A2 |
| --- | ---: | ---: | ---: | ---: |
| Mutex ns/op | 159.39 | 363.38 | 361.01 | 159.94 |
| Whole-round maximum / operations | 160.72 | 380.98 | 371.99 | 161.97 |

May remains about 2.3 times faster in this comparison. Each May warm-up observed
migration in all 64 tasks; those observations do not describe every timed handoff
or quantify migration's contribution. These fresh guest measurements are not a
claimed improvement over older runs, and the full May target remains open.

## Qualification and reproduction

The benchmark suite passes 32 default tests, 33 all-feature tests, and its all-feature
clippy check. All 11 canonical gates pass under
`/root/.cache/zcheck/run-1788595927-20609703-2080122/receipt.json`.
The standalone benchmark checks are additional to that root workspace gate. A live
four-carrier smoke run verifies all four requested CPU masks. A four-carrier request
under a one-CPU mask fails before runtime construction. Smoke timings are not part
of the performance tables. Core runtime source, dependency locks, architecture
policy, and its reviewed lock are unchanged from `8ba6a7e`.

```sh
cargo build --release --locked --manifest-path benchmarks/Cargo.toml
timeout 120s taskset -c 0-3 BINARY vthread mutex 100000 4 64 9
timeout 120s taskset -c 0-3 BINARY vthread mutex 100000 4 64 9 --pin-carriers
timeout 120s taskset -c 0-3 BINARY may mutex 100000 4 64 9
```

Layout artifacts are in `/tmp/vthread-mutex-layout-lPxXpn`, including both immutable
binaries, `candidate.patch`, and `mutex-{a1,b1,b2,a2}.log`. The baseline is the
runtime at `8ba6a7e`. Baseline SHA-256:
`a46d9bc19b857fb686aec4137c56e33ec92c46b8ec06b90951f2ecbd626a086f`;
layout candidate: `63769628f093cd754cb520e063e440dcdc48b83b125296a1d08a8f8ee18d94c2`;
patch: `f4eec45c4de064bca9f26a10ebe02a3463a6f9a4a5b11b23b68d1873e7bf371a`.

Pinning artifacts are in `/tmp/vthread-worker-pinning-cOggax`: `mutex-*.log` is the
default/pinned comparison; `engines-mutex-*.log` compares the engines. Immutable
benchmark SHA-256:
`8bfe3ffd6d9c81ecc0a2d72269bd8f4c75e3cf8a670a0cc3298bc691adcf29e6`.
These are local experiment artifacts, not a release-evidence bundle.
