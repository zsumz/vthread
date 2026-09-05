# Channel eligibility and carrier idling

Baseline: `5b63f266fd6dfc39de3c13d6e28c7ec850de2d42`, with the independently
qualified shared bounded-channel control. Neither prototype below is an accepted
runtime optimization yet. Both are preserved separately from the unchanged runtime.

## First slice: notify only eligible queue heads

The candidate checks buffer state and endpoint closure under the existing channel
metadata lock before notifying each FIFO head. A sender can transfer when space
exists or return an error after close/disconnection; a receiver can consume a value
or return closed after drain. Both directions are still checked: multiple values
or slots can remain after a successful operation. Close still notifies all waiters.

Turns remain queued until consumed. Cancellation still removes its ticket under
the same lock and considers the newly eligible heads. This does not detach a waiter
before reserving it, move wake publication outside the lock, transfer payloads across
stacks, or alter any wait-word/queue/signaling atomic protocol.

Two ordered native regressions reproduce unnecessary work on the old source. With
three queued senders and two available slots, the third sender resumes only to park
again. The receive-direction case behaves identically after consuming two values.
Both old runs record `(parks=4, wakes=3, remaining_waiters=1)` for two useful transfers.
After the predicate change they record `(3, 2, 1)`, preserving FIFO successor service.
Cleanup closes and drains before the negative assertion, including retained values.

All 18 channel tests pass after the change, including cancellation after selection,
deadline cleanup, retained input ownership, bounded selected tickets, shutdown and
destructor reentry. A 160-case matrix exercises the actual predicate with real wait
words across buffer levels, endpoint/closed states, active waits and stored permits.
This exhausts those serialized predicate states, not the full concurrent runtime.

## Standalone eligibility fails the performance gate

Three balanced default-build processes per arm and complete case use the same
benchmark binary contract. Values below are medians of process observations;
ns/op counts one transferred value. Process cycles include validation and warm-up.

| Shared channel: carriers/tasks/capacity | Baseline ns/value | Eligibility ns/value | Cycle change |
| --- | ---: | ---: | ---: |
| 1 / 8 / 1 | 995.75 | 311.22 | -28.60% |
| 1 / 8 / 64 | 501.19 | 302.15 | -2.12% |
| 1 / 8 / 1,024 | 307.70 | 297.61 | -2.49% |
| 4 / 8 / 1 | 1,286.66 | 16,192.11 | +470.97% |
| 4 / 8 / 64 | 1,267.22 | 1,227.68 | -4.37% |
| 4 / 8 / 1,024 | 1,196.63 | 1,186.51 | -6.33% |
| 4 / 64 / 1 | 1,288.42 | 879.22 | -31.93% |

The eight-task/four-carrier regression rejects eligibility alone. Its processes
record 324,739-516,545 native context switches, versus 1,174-1,209 on the baseline.
This is not explained by fewer source-level operations. A subsequent spare-capacity
case was interrupted by another project's build: its incomplete pair is not compared,
and the later unaffected controls did not run. All raw output is retained.

Separate final-only scheduler profiles reinforce the dependency. For the same
160,000 transfers, eligibility reduces mounts from 404,166-415,627 to 320,020-320,025,
but native wait-function calls grow from 121-176 to 11,574-57,953. A wait call is not
proof of an individual sleep/syscall; combined with OS context switches it motivates
investigating carrier sleep/wake pacing. Profiling perturbs the timing, so its ns/op
is not mixed with the default panel.

Source `5048ad210caf66f08536a3886553b71511a692337edf106d4589f183bf2dc81e`
is preserved in stash `19eb1d3c4d41b8ceecaa97f2771e30c3ed625af7` and the
[eligibility evidence bundle](evidence/channel-eligibility-5048ad21.tar.gz).
The architecture refresh was inventory-only, with zero new grants/dependencies/debt.
Only targeted qualification ran; no final canonical or tail acceptance is claimed.

## Second slice: independently vary the parked-carrier polling horizon

A separate prototype extends idle polling from 640 to at most 4,096 probes only
when this carrier has parked tasks. Admission-only idling remains at 640. Timed
waits and single-carrier idling still skip busy polling; each probe retains the
existing work and control checks. No new clock, allocation, shared atomic or field
is introduced. **The larger worst-case idle CPU budget is an explicit tradeoff.**
This is a fixed bounded experiment, not an adaptive policy or new event-word design.

Four independent process orders compare all four combinations. Each arm occupies
each order position once, across five cases: 80 processes total. The 160 endpoint
host snapshots caught no compiler or process over 10% lifetime-average CPU, without
establishing uninterrupted physical-host exclusivity. No observations are excluded.

| Case | Baseline cycles, billions | Eligibility only | Polling only | Combined | Combined change |
| --- | ---: | ---: | ---: | ---: | ---: |
| Shared channel, 4 carriers / 8 tasks / capacity 1 | 2.620 | 16.146 | 2.839 | 1.523 | -41.89% |
| Shared channel, 4 / 64 / capacity 1 | 21.628 | 16.417 | 22.177 | 13.660 | -36.84% |
| Park, 4 / 64 | 3.724 | 3.636 | 3.635 | 3.662 | -1.66% |
| Timestamped wake, 4 / 64 | 5.014 | 4.930 | 4.833 | 4.827 | -3.74% |
| Lifecycle, 4 / 10,000 | 15.320 | 14.913 | 14.390 | 14.620 | -4.57% |

The coupled result is promising, while longer polling alone adds 8.33% channel cycles
at eight tasks. This is why neither source-level change is promoted independently.
Small control differences are not claimed as general speedups. Shared-channel
combined median elapsed values are 382.81 and 868.48 ns/value for eight and 64 tasks.

Wake p50 remains about 230-240 ns. The four baseline p99.9 observations span
18.64-25.74 us; combined spans 19.00-20.07 us. Their p99.99/maxima still vary, and
this is not a channel-specific tail or loaded-fairness qualification.

The combined default-native workspace passes 519 runtime tests (one ignored),
70 stack tests and supporting suites. It remains shelved at
`1478da7272640a6539dfac698304471e26e1c3c7`, source SHA-256
`1d8e9448d21c821e2212c4cf3ba2c7ccfb8adf4b73cd16afaae2105c0167f652`.
The [four-way bundle](evidence/channel-idle-1d8e9448.tar.gz) retains all raw results,
source patches, commands and binary identities. Archive hashes are indexed in
[the evidence README](evidence/README.md).

Before retention: channel API-call tails and per-task spread, wider capacity/population
controls, idle CPU, replayable stress, and final canonical/architecture qualification.
The claim-publication dependency, full event-word design and capacity-scan removal
remain open. HTTP is separate. No new May comparison is claimed.
