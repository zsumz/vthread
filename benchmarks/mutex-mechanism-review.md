# Mutex mechanism controls: qualified fixture, panel pending

This checkpoint adds a bounded, unpublished Linux lab executable. **It does not
change the runtime or establish a mutex speedup.** Direct ownership, cancellation,
affinity, stacks, ready/admission fairness and polling remain unchanged.

The existing mutex benchmark remains the default-contract comparison. This lab
provides three separately labeled controls using real virtual mutexes:

| Control | Established condition before release |
| --- | --- |
| `local` | The source runs on the recipient's owner after that recipient queues and suspends |
| `remote-active` | A verified same-owner borrowed keeper runs after the recipient queues; that keeper remains runnable |
| `remote-sleep-observed` | The recipient has queued, and Linux reports its actual TID in S/futex wait before release |

The measured source/recipient TIDs and runtime carrier identities are checked,
and their CPUs are pinned and verified. There is no placement API or migration
change. A shared hub is not used as evidence of a different owner. The sleeping
control is explicitly **not** an atomic guarantee of sleep at publication;
spurious wakes and OS scheduling remain possible after observation.

The source holds the mutex until the required gate is observed. One command park
per round prevents accidental uncontended reacquisition. Each recipient checks
the exact generation and protected payload; successful results require the
expected handoff count, sufficient actual park crossings, no remaining mutex
waiters and clean runtime shutdown. Actual command parks can add to the reported
recipient park count. This is one FIFO successor, not a multi-waiter fairness test.

Timestamps cover immediately before guard drop through recipient lock return,
using `Instant` and an atomic timestamp slot. Raw samples preserve their original
sequence; quantiles and sorting occur after task completion. Control wakes,
source yields, procfs checks, the active borrowed keeper, setup, shutdown and
reporting remain in process counters. Those counters are not isolated mutex
cycles. The ordinary workspace release profile is used, not the separate
benchmark crate's thin-LTO profile. Existing handoff profiling can report routing
and native-wait counts in the separate diagnostic build; no new runtime profiler
or shared per-wake counter is introduced.

Example (Linux, with at least two allowed CPUs):

```sh
cargo run --locked --release -p vthread-lab --bin vthread-mutex-handoff -- remote-active 10000
```

The output includes verified pinning, a summary and every raw sample. Work counts
are bounded to 1–100,000. These are closed-loop controls, not offered-load tails.

## Qualification and explicit exclusions

Nine fixture tests pass. Negative controls reject an invalid timestamp while
retaining both the primary recipient error and secondary source stop, and reject
the old policy that treated partial startup visibility as failure. Four parser
tests reject corrupt samples, invalid topology and false sleep/count claims.
The analyzer also rejects an empty/incomplete formal panel.

Final receipt `run-1788687621-285904529-2911800` passes all fourteen canonical
tasks with repository state preserved, including default native debug/release.
The exact final source is restored after each mutation. The architecture review
adds one lab target and an explicit forwarding of the existing profiling feature
in its matching feature world: 409→417 Rust files and 2879→2939 source contexts.
There are no dependency, unsafe, macro, debt or runtime-policy grants. The exact
analysis-universe differences remain classified UNKNOWN by `zrail diff`; normal
`zrail check` passes the reviewed inventory.

An earlier canonical attempt exposed the separate
[publication observer assumption](publication-observer-review.md), repaired in
`7d4af02` before resuming this fixture. All attempts remain in the archive.

Five CLI smoke processes ran: four succeeded; one original remote setup failed
before admission because carrier names were not yet visible. The startup fix
matches the existing benchmark's bounded discovery and precedes all timing.
The three final 100-handoff smokes measured p50s of 200 ns local, 321 ns
remote-active and 10,666 ns after observed sleep. **These tiny smokes are not the
planned independent-process panel and do not justify a mutex optimization.**

Both formal-panel attempts stopped before the first process because an unrelated
cargo/zrail test was active. **Zero formal default or diagnostic panel processes
ran.** The failed guards retain distinct prefixes; no sample was silently retried
or counted as acceptance. The planned 18 default and nine diagnostic processes
remain pending. Work proceeds to independent ordered test repairs while those
measurement conditions are unavailable.

## Durable evidence

Source SHA-256:
`f0ce5246d87b7846b36b0d1ddc8f3f7c1b95e5509922f20f2fe8b4bbdfe9b9c5`.
Final binary SHA-256 values:

- Default: `8964d0d5983971d834a2c6543859a8e82224f032d62ec23870432b108a651d47`.
- Diagnostic: `c75b00416f82f854094d6048acddbd6a9306143e687632cbef2d28740903ecce`.

The [qualified fixture bundle](evidence/mutex-mechanism-f0ce5246.tar.gz) preserves
194 verified internal hashes, full source, all qualification attempts, negative
controls, raw smoke samples/counters, host observations and guarded non-runs.
Archive SHA-256:
`09a0228a1e9f4d6641f49b07f1ccfc78437d16599a39890fdfdeaab4999ca6db`.
No new May, loaded-tail, ARM64, sanitizer or large-lifetime result is claimed.
