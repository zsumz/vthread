# Scheduler benchmarks

This standalone harness compares the default `vthread` scheduler with May 0.3.51. It uses
structured scopes, 64 KiB stacks, the same worker and task counts, one untimed warm-up, and the
median of an odd number of samples. Runtime construction is outside the measurements; scope,
task, yield, completion, and join work is inside them.

Build once, then run each engine in a fresh process. Pin single-worker measurements to one CPU
when `taskset` is available:

```sh
cargo build --release --manifest-path benchmarks/Cargo.toml
taskset -c 0 benchmarks/target/release/vthread-benchmarks vthread yield 100000 1 1 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks may yield 100000 1 1 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks vthread spawn 1 1000 11
taskset -c 0 benchmarks/target/release/vthread-benchmarks may spawn 1 1000 11
```

The yield scenario reports nanoseconds per cooperative yield. The spawn scenario reports
nanoseconds per task. Use a quiet machine and compare distributions as well as medians; carrier
placement and host scheduling make short multi-worker samples noisy.

For attributed vthread lifecycle timings, rebuild with the opt-in profiling feature:

```sh
cargo build --release --manifest-path benchmarks/Cargo.toml --features lifecycle-profiling
benchmarks/target/release/vthread-benchmarks vthread spawn 1 1000 101
benchmarks/target/release/vthread-benchmarks vthread spawn 1 10000 101
```

The additional lifecycle line separates stack/fiber materialization, physical reclaim, terminal
completion publication, and unattributed scheduler work. Admission remains the independently
timed producer-side phase. The profiler verifies that every requested task appears exactly once in
all three carrier phases. Clock reads and atomic accounting intentionally make profiled totals
slower; use the default build above for engine-to-engine comparisons.

Heap allocation counts are independently available with `--features allocation-probe`. They cover
the measured process-wide interval and therefore should be collected with one worker on a quiet
machine. Rebuild without either feature before recording headline latency results.
