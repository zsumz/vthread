# Channel endpoint latency control

This benchmark-only slice extends the qualified shared bounded MPMC workload at
`5ec3c9d`. Runtime source, channel ownership, scheduling and `zrail.lock` are unchanged.

`--sample-channel-latency` selects separate timed producer/consumer loops, leaving
the plain transfer loops untouched. Each task preallocates its own sample vector.
`Instant` brackets each send or receive API call; recording follows the interval.
Sender handles are retained only in the timed path. Both modes retain exact
per-round delivery checking outside elapsed timing.

The `-sampled` label distinguishes the timing workload. Throughput still counts
one transferred value; latency counts both endpoint calls. Separate receive/send
reports include p50/p90/p95/p99/p99.9/p99.99/max and per-stream tail spread. Streams
are logical admission slots across independent rounds, not persistent task IDs.
Warm-up is validated but excluded. Missing task samples, wrong quotas, unexpected
samples in the plain control and unsupported CLI combinations fail explicitly.

Clock overhead, timing-vector allocation and recording affect scheduling. This is
a closed-loop API-call distribution, not message arrival-to-delivery latency,
fixed offered load, worst waiter age or a progress/fairness bound. Hardware counters
around the entire process also include validation and distribution processing;
keep sampled and untimed comparisons separate.

## Qualification

Source SHA-256:
`30bb48693c14080d0a289f76a23943f9d67caf1507b20e7e69b8db302dcdad9c`.

- All 51 benchmark tests and all-target/all-feature warning-denying Clippy pass.
- All 11 canonical gates pass, receipt
  `/root/.cache/zcheck/run-1788646342-4688439-2473375/receipt.json`.
- Native tests cover both modes, one/four carriers and capacities 1/64/1024.
- Twelve optimized CLI smokes independently verify directional counts, warm-up
  exclusion, quantile ordering, labels and May rejection. Their timings are not
  performance acceptance results.

The source-keyed bundle in [the evidence index](evidence/README.md) preserves the
patch, commands, hashes, tests, raw smokes and complete canonical receipt. An
immutable default-build baseline is retained locally for the separate channel/
parked-idle experiment. No runtime optimization is accepted by this slice.
