# Channel reservation/publication: rejected performance candidates

The cancellation-safe boundary is implemented in replayable prototypes, but **no
runtime candidate is retained**. All five miss the combined cycle/throughput/tail
gate. This checkpoint preserves evidence, not a channel speedup or a completed
release milestone. Capacity-independent maintenance and readiness scaling remain
separate open slices; neither is changed, nor is the 640-probe idle policy.

Base: `3161514f20da1d08d8a5e863c9f3a0b7d2bf2920` on
`perf/scheduler-hot-path`. Restored production source SHA-256:
`f57b1db1457fc45f855e561e5eb89e489c72bcf166ef4c50400596057840c976`.

## Protocol and correctness evidence

The prototypes select a wait generation under the channel metadata lock and move
an owned RAII publication guard outside it. The FIFO ticket remains queued and
counted until consumption or cancellation cleanup. A newly arriving try operation
cannot take the selected turn. Payloads stay in the channel buffer: this is not
direct cross-stack transfer of arbitrary `T`.

The original-source regression deliberately pauses a publisher after notice
publication. An independent observer cannot read channel metadata while that
publisher is paused. The failure is captured before changing the runtime. The
five-second timeout is a failure bound, not a latency benchmark; the test releases
and joins both native threads and checks cancellation/value recovery before
asserting failure. The original compile/configuration setup mistakes are retained
separately from that genuine failed assertion.

Native prototype tests deliberately cover:

- Cancellation after reservation but before any wake publication, in both
  directions; no premature runnable task, barging, capacity escape or value loss.
- Cancellation winning before selection, with FIFO successor/value recovery.
- Unwind between unlock and front or close-broadcast publication; RAII guards
  release every reserved claim and remaining receivers observe closure.
- Cancellation, inherited cancellation and timeout retirement before queue-ticket
  cleanup, followed by resident-wait reuse and a delayed old publication.
- Existing FIFO, shutdown, disconnection, waiter limits, input recovery,
  destructor re-entry, checkpoint and one/four-carrier exact-delivery tests.

The first resource-grant model asserted that retirement itself cleared every
grant. Loom disproved that assertion: after cancellation retires the wait but
before ticket cleanup takes the channel lock, a producer can store an idle grant.
Production removes the ticket before `synchronization_wait()` recycles the record.
The repaired model tests that actual boundary and starts generation 42 while an
old generation-41 publisher may still be delayed. The corresponding ordered
native regression passes. This was a model-contract correction, not a discovered
production value-ownership leak; the failing model output is not discarded.

The resource-grant version passes 12 model tests. The final retained-handle design
passes 13: it includes the production notification-selection/RAII implementation,
production wait-word encoding and production queue-entry logic through Loom
atomic adapters. Exploration uses three threads, one buffered value, two park
generations, a 1,000-branch limit, and no permutation/preemption cap. Ready competes
with direct/inherited cancellation, timeout and close. An intentionally broken
dequeue-before-retirement control fails as expected. Native signaling, Binding,
the routed wake queue and the complete runtime composition are outside this model.

Targeted native channel suites pass through all four protocol designs; the last
retained-handle variant passes 23 feature-on channel/accounting tests. The final
inlining-only variant uses that same protocol and passes benchmark value checking,
but was rejected before a separate full native/canonical candidate qualification.
No rejected candidate is presented as release-qualified.

After shelving the candidates, the restored production checkout passes all 11
canonical `zcheck run check` gates under receipt
`run-1788659610-751414318-2581158`. Its receipt and logs are preserved in the
bundle. This includes the all-features workspace configuration, not a new full
native-stack/ARM64 release qualification. Source and architecture-lock digests
remain identical to the baseline; no new grants or debt are accepted.

## Experiments and acceptance screen

Every arm uses a separately saved default, uninstrumented release binary. This is
one shared bounded MPMC channel: four producers/four consumers at eight tasks,
32 of each at 64 tasks. Normal initial placement and existing `--pin-carriers`
are retained; tasks are not silently co-located. There is no new May measurement.

| Variant | Reservation and queue shape |
| --- | --- |
| A: eligible resource grant | 16-byte retained FIFO identity/handle; existing resource publication; notify eligible directions only |
| B: both-direction resource grant | Same representation and guard, retaining both-direction notification policy |
| C: compact notification guard | Original eight-byte entries; temporary strong reference for an active notification; no resource-grant bit |
| D: retained notification handle | 16-byte entries; move the existing handle only for an active selection, retain it for stored permits |
| E: inlined D | Identical protocol with an inlined two-slot reservation helper |

Normal publication batches have two fixed stack slots and no new heap allocation.
Close/disconnection batches allocate at most the already bounded outstanding
waiter population. A/B/D/E double each provisioned queue-entry payload from eight
to 16 bytes. C adds reference-count operations for active publication. D/E avoid
that temporary clone but still reattach a wait handle after an unsuccessful
resumed turn. Those are real costs, not erased by calling a handle task-resident.

The table gives changes in median **whole-process cycles** relative to each
variant's paired baseline. Warm-up, setup, validation and shutdown are included;
operation counts are identical within each pair. These are not isolated handoff
cycle counts. Each eight-task process transfers 320,000 values across warm-up
plus seven measured rounds; each 64-task process transfers 512,000.

| Variant | 1 carrier / 8 tasks | 4 carriers / 8 tasks | 4 carriers / 64 tasks | Sampled send p99.9 | Decision |
| --- | ---: | ---: | ---: | ---: | --- |
| A | -16.7% | +287.8% | -27.9% | +135.3% | Reject: small remote handoff and tail collapse |
| B | +11.1% | -10.9% | -12.6% | -53.5% | Reject: local regression; remote panel incomplete |
| C | +9.0% | +11.8% | +9.5% | -48.7% | Reject: unsampled cycle regressions |
| D | +9.1% | +7.5% | +10.8% | -27.3% | Reject: unsampled cycle regressions |
| E | +8.9% | +16.7% | +6.7% | -54.1% | Reject: inlining does not recover cycles |

A has three independent process pairs per case. B completes two pairs for the
first five cases and one for the remaining seven; a host build guard stops its
second round at the eight-carrier case. That case's unpaired candidate is retained
but excluded. The B tail figure is only **one pair**, not qualified tail evidence.
C/D/E each have four independent process pairs for all four displayed cases,
alternating A/B and B/A order. No process pair is excluded from those three panels.

The full A screen also includes buffer capacities 64/1,024, eight carriers,
65,536 runtime task capacity with 64 live tasks, and park/mutex/yield/lifecycle
controls. A reduces spare-capacity cycles by 50.8%, but raises eight-carrier cycles
19.6%. Neither selective win outweighs its other regressions. Subsequent variants
stop at the early rejection screen instead of claiming unrun wider qualification.

For E, sampled receive/send p99.9 improves from 53.19/52.04 us to 25.13/23.91 us.
Worst-stream p99.9 also improves in that sampled panel. But unsampled single-carrier
throughput-derived cost rises from 418.94 to 458.24 ns/value, and its cycle cost
is consistently about 9% higher. Clocked endpoint sampling changes the workload:
a win there does not establish loaded fairness or an unsampled-tail guarantee.

The code-generation audit found an outlined helper retaining replacement-drop
paths for initially empty publication slots. E removes that outlined symbol, but
the measured local penalty remains. Consequently inlining, temporary Arc churn,
and slot width alone are not established explanations for the entire gap. A
planned cycle-profile invocation was rejected by a host-build guard before any
recording; no profile result is inferred from that aborted attempt.

## Reproduction and limits

The host is an eight-vCPU KVM guest exposing AMD EPYC 9555P, Linux 5.15.0-187,
Rust 1.96.1 / LLVM 22.1.2. CPU 7 is used for single-carrier runs, 0-3 for four,
and 0-7 for eight. Exact commands, affinity verification, feature configuration,
per-process counter CSVs, raw round/tail output and endpoint host/CPU observations
are preserved. Endpoint checks cannot prove continuous physical-host isolation.
No own build or correctness test overlaps a timing panel.

The [evidence bundle](evidence/channel-publication-56f61f86.tar.gz) contains 203
completed counter processes: 202 in complete accepted-for-analysis pairs, plus
one explicitly excluded unpaired B process. "Accepted for analysis" does not
mean a runtime candidate passed acceptance. It also contains all source patches,
binary hashes, failed/passing targeted tests, model adapters, the old-code
negative control, analysis script/tests, disassembly and a replay ledger. B's
guard failure and the aborted profile attempt remain visible.

Patches apply independently to the base commit. `negative-complete.patch`
reconstructs the original regression file omitted from the first tracked-only
patch capture; its test body and failure output are preserved separately. Local
stash `6372fcbbd6c39a73c623a7d3683715fd9d9905ff` also retains E, but the archive,
not a moving stash index or `/tmp` path, is the durable recovery mechanism.

```sh
git apply --check candidate-inlined.patch
cargo test --locked -p vthread-sync-core --test channel_publication_model
cargo test --locked -p vthread --features handoff-profiling channel
cargo build --release --locked --manifest-path benchmarks/Cargo.toml
python3 analyze.py . screen notify compact retained inlined
python3 analyze_test.py
```

Use a separate checkout at the base, actually apply the chosen patch before
running its tests, and rebuild binaries before replaying the saved shell scripts.
Those scripts record the original absolute checkout/build paths and must be
adapted to the replay environment. Binaries are hashed, not stored in the archive.

The next channel experiment should first isolate the extra local reservation,
retirement and rearming work, then pass this same uninstrumented cycle screen.
The ownership/publication proof is reusable; the measured implementations are
not approved optimizations. No idle-policy change is justified by this result.
