# Cancellation history: separate semantic and timing gates

The test-oracle ambiguity is repaired without changing the cancellation runtime.
The historical elapsed-time excursion remains an explicitly open performance
finding; it is not silently waived as host noise or treated as a demonstrated
ownership/cancellation defect.

The captured failure was `ending.1 <= beginning.1 * 128 + 10_000_000`. The old test
panicked before printing its samples, so the actual values and unique cause
cannot be recovered from that run. Graph bounds and elapsed-time ratios answer
different questions and now have separate acceptance paths.

## Required semantic proof

The default test still traverses 100,000 dynamic successor generations for both
direct-ancestor and owning-scope cancellation. Every generation checks nodes ≤8,
relays ≤1, edges ≤12 and retained task diagnostics ≤2. Both cancellation paths,
typed completion and clean shutdown remain mandatory.

Parent completion now proves the bounded successor send has finished before
`try_recv`. An absent successor fails from ordered evidence immediately, instead
of waiting on a ten-second test gate. The production pruning mutation fails at
generation zero with `(104, 0, 103)`; omitting the send fails with the missing
handoff. Both mutations are restored. No cancellation or graph algorithm changes.

## Preserved performance check

```sh
zcheck run perf-cancellation-history
```

This separately selected optimized/default-feature test keeps the original
**128× + 10 ms** limits unchanged and reports both samples on failure. It is
intentionally ignored by ordinary correctness suites. Canonical policy tests
reject a missing task or missing `--release`, `--ignored` or `--exact` selection,
so a successful no-op cannot replace the check.

A Rust boundary test accepts the exact limits and rejects either excessive
sample. Deliberately bypassing the limits makes that regression fail. The timing
task passes under receipt `run-1788691832-161701542-3004520`, with clean build guards
before and after. This single process proves the retained check executes; it is
not independent cycle non-regression evidence or a new performance result.

## Qualification and limits

Canonical receipt `run-1788691295-877243152-2986611` passes all fourteen tasks with
repository state preserved: 541 default-native runtime tests in debug/release,
570 with all features, and eight qualification-policy tests. Two explicitly
manual/performance tests are ignored by those runtime suites. The required native
commands and final dependencies are unchanged. `zrail check` passes without any
lock, grant, dependency or feature change.

Three further debug and three release invocations on one CPU pass all non-ignored
history tests: **1.2 million checked dynamic successor generations**, both
cancellation choices, at most two live tasks. This is deliberately not the large
mixed-population lifetime qualification. Diagnostic timing samples from those
constrained repetitions are not performance acceptance.

Source SHA-256:
`fb834f0f199144a6b8cb81fb66ec9361008035264e05eb8f4aae5f54a8b7191b`.
The [78-file verified bundle](evidence/cancellation-history-order-fb834f0f.tar.gz)
contains the original failure/receipt, all mutations and restored source,
canonical and timing receipts, exact commands, source/binary identities and raw
repetitions. Archive SHA-256:
`d809bde75d32fc3d0ee7db1ae9be0e9da5089220c767f21aa0a477c9bb72416e`.

Four loaded findings remain unclassified: inbox refill, interrupted cross-runtime
join, delayed selected timer and deadline-first join. The original history timing
excursion remains separate release-performance evidence to resolve. The I/O retry
finding was closed in the preceding ordered test slice. No new May or
cross-platform release qualification is claimed here.
