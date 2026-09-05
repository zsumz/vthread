# Ordered stall and shutdown qualification

Runtime baseline: `3372486`. This slice changes test orchestration and assertions,
not production shutdown, wait selection, scheduler policy, or resource ownership.
The admission repair was preserved separately in stash `c795920` during this
investigation. Its first canonical run passed, but its default-native run exposed
the late-service test failure; performance measurement was paused.

## Capture before repair

The original default-native failure was:

```text
502 passed; 1 failed; 1 ignored
stop_before_service_publication_still_drains_late_services
late services missed stop and stranded the coordinator
```

No task kernel is created by that test. It constructs a shutdown coordinator,
requests stop, publishes services after the request, and releases construction.
Its assertion treated failure to reach terminal shutdown within 200 ms as proof
that the late services missed stop.

Diagnostic assertions were added without changing either 200 ms deadline. Eight
ordinary default-native library invocations (eight test threads) each passed all
499 tests. That was not treated as resolution. Explicit oversubscription then used
32 test threads on process CPU mask 0-3. This is a correctness stress configuration,
not a performance benchmark or a passing release gate.

### The unresolved root-stall failure reproduced

`loaded-3.log`, `observer-2.log` and `observer-4.log` captured the exact failure:

```text
result=Ok(())
watchdog=Stored
last_stall=None
parks=0
wakes=0
completed=2
```

In two of these runs the unrelated supervisor had executed over 108,000 yields;
in another neither task yielded before the watchdog. In every captured failure the
watchdog had stored a permit before the supposed ownerless task suspended. Its
`park()` consumed that permit and completed. There was no indefinitely parked root
for the detector to report. This is observed ordering evidence, not a guess based
on a quiet rerun. It explains this reproduced failure class; it does not supply a
missing trace of every historical failure.

### Late-service shutdown completed without another stop

The first diagnostic runs reached `JoiningReadiness` with native stop already true,
but the coordinator had not returned and service joins were not complete. To avoid
masking a missed notification with the test's cleanup, a second diagnostic binary
waited up to five additional seconds **without issuing another stop**.

| Failed old 200 ms check | Phase at capture | Native stopped | Completed without rescue |
| --- | --- | --- | --- |
| `observer-2.log` | Requested | false | true |
| `observer-4.log` | JoiningReadiness | true | true |

Thus these failures were delayed completion, not permanently stranded services.
The exact split between OS scheduling, locks and worker cleanup is not attributed
by these snapshots. They do establish that another stop request was unnecessary.

## Ordered replacement

The root-stall regression now waits for the unrelated task to start and the target
record to report a real `Suspended(Park)` transition. Only then is a failure watchdog
armed. Scope completion cancels it. A rescue is explicitly a failed test, with the
pre-rescue snapshot and unpark result; it cannot be mistaken for successful stall
detection. The production 10 ms stall policy is unchanged. A deterministic stored-
permit control verifies that an early wake produces zero parks and no stall.

The late-service regression uses the existing test-only coordinator exit hook to
observe that service drain actually occurred, before asserting terminal completion.
A second test holds that hook behind a caller-owned channel: services must be joined
while the coordinator remains unreturned and shutdown is nonterminal. Releasing or
dropping the sender permits return. There is no timed auto-release of that gate.

Five-second waits are test-harness failure bounds, not a runtime latency contract.
Cleanup releases owned services before reporting a failed progress assertion.

The revised service test was also run with late-service `services.stop()` temporarily
omitted. It failed after the watchdog with:

```text
drain hook not reached: Err(Timeout), (JoiningReadiness, false, false, false)
```

Cleanup recovered the workers. The original production stop call was restored before
qualification. This negative control proves that the revised test still rejects the
missing-stop defect instead of merely accepting slower shutdown.

## Additional loaded-suite findings remain open

The deliberately oversubscribed full suites also exposed:

| Test | Captured failure |
| --- | --- |
| Continuous coalesced inbox refill | Five-second submission timeout, then the cleanup stop rejects the producer |
| Interrupted cross-runtime join | Target's five-second native gate times out; expected deadline/handle return assertions fail |
| Selected timer with delayed resume | Test synchronization timeout |
| Cancellation history | End-of-history wall-time ratio exceeds the test's timing allowance |
| I/O retry before readiness | One readiness registration observed where the test expects zero |
| Deadline-first join for both handle types | Expected result channel disconnected |

These are preserved findings, not silently discarded samples or established runtime
defects. Their setup ordering and actual progress require separate investigation.
No full oversubscribed-suite or release-ready claim is made while they remain open.

## Evidence and qualification

Local raw output and source patches are in `target/finish-line/wait-qualification`.
The diagnostic executable hash is
`c0fac1f2ddfa93bd939d2a59aa70ed5351ece643b9196e975ddfc8fbd8331c6c`;
the passive observer hash is
`16857394df3b73818aee97f174bfb2aeae612cbeedfc5c98ebcb4e52dc9368d8`.
Those completed diagnostic executables are losslessly gzip-compressed locally.

```sh
timeout 90s DIAGNOSTIC_TEST_BINARY --test-threads=8
timeout 120s taskset -c 0-3 DIAGNOSTIC_TEST_BINARY --test-threads=32
cargo test --locked -p vthread --lib control_wait_test -- --nocapture
cargo test --locked -p vthread --lib runtime_service_publication_test -- --nocapture
```

All six control-wait tests and both service-publication tests pass on the repaired
test orchestration. All three affected stall/drain/held-coordinator tests also pass
in each of four subsequent oversubscribed full-suite invocations. Two of those full
suites pass all 501 tests; two fail the I/O retry and deadline-first join tests above.
Those failures are retained, not counted as passing full-suite qualification.

All 11 canonical gates pass under receipt
`/root/.cache/zcheck/run-1788630191-988759931-2248084/receipt.json`.
The separate default-native workspace run passes 501 runtime tests (one ignored
manual probe), 70 stack tests, and all remaining model/lab/reexport suites.
An earlier canonical attempt passed its 11 code gates but failed repository-state
preservation because the evidence report was added during the run; the frozen-tree
rerun above is the valid receipt. The rejected receipt is preserved too.

The architecture refresh only inventories the extracted test file; no dependency,
grant, revocation or production atomic protocol changes. The qualified source digest
is `22558d5cf4a9caf9995082187a7636b958415b0dd44442b476f6e0e2b9d85741`.
Raw output, patches and qualification receipts/logs are committed in
[`evidence/wait-qualification-22558d5c.tar.gz`](evidence/wait-qualification-22558d5c.tar.gz).
Its SHA-256 is `eb70809ed123b79cd9d0c3b79cfaef32725b4b2520fc18dfc86d50d42b38518d`.
Executed binaries remain local with hashes recorded; this is not a complete release bundle.
