# Readiness retry: ordered test contract

The loaded I/O finding is closed as a **test-oracle defect**, not a production
readiness repair. No retry budget, polling, admission, wake ordering or I/O behavior
changes. The retained runtime's performance claims are unchanged.

The original oversubscribed run observed one outstanding readiness subscription
where `blocked_io_yields_to_runnable_work_before_registering_readiness` expected
zero. Submission of the writer did not establish the needed ordering: the
scheduler samples its runnable hint before mounting the reader, and admission
can happen after that sample. The old test's admission flag did not close this
window. Its exact historical OS interleaving was not recorded.

A controlled counterexample now orders admission after selection through the
real `Shared`, `Kernel` and socket operation path. The reader legally registers;
the writer observes one subscription, writes, and both finish cleanly. Replacing
that expected one with the original zero fails with the same `left: 1 / right: 0`
assertion. This establishes the invalid unconditional oracle without attributing
an unobserved unique schedule to the original failure.

## Ordered replacement and negative controls

| Case | Required observation |
| --- | --- |
| Both peers materialized before first dispatch | Empty read yields once; writer runs next; two reads, zero parks/subscriptions |
| Continuously runnable peer, no data yet | 128 empty-read yields; attempt 129 parks with one subscription; explicit write enables attempt 130 |
| Peer admitted after selection | One subscription is legal before the writer runs; completion retires it |

The fixture drives real native fibers and a real nonblocking Unix socket/reactor.
It records the first crossing before allowing the write and asserts the retained
observations after cleanup. The completion watchdog is not an ordering oracle.
`readiness_waits` counts admitted outstanding subscriptions; it does not establish
that the backend installation has already finished.

Deliberately bypassing the yield fails the first behavior test; changing `< 128`
to `<= 128` fails the exact-boundary test. The old zero-subscription assertion fails
the legal late-arrival counterexample. All mutations are restored. No timeout was
increased to obtain a pass. The preliminary fixture configuration failure and the
first canonical attempt's unapplied inventory failure remain in the evidence.

Final receipt `run-1788690281-711800275-2964180` passes all fourteen canonical gates
with repository state preserved, including 540 default-native runtime tests in
debug and release and 569 with all features. One existing manual probe is ignored.
Fifty additional debug and fifty release invocations, each restricted to one CPU,
pass all three ordered tests: **300 additional executions**.

The architecture update is inventory only: 417→418 Rust files and 2939→2944 source
contexts. `zrail diff` reports one explicitly reviewed UNKNOWN inventory change,
zero grants/debt; `zrail check` passes. No dependency, feature or policy change.

## Evidence and remaining findings

Source SHA-256:
`7bb05ae659beb1db3a00c186bb6a79cbee5eb15dcf16ef777c6bb07a51d3de8e`.
The [366-file verified bundle](evidence/io-retry-order-7bb05ae6.tar.gz) preserves
the original failure, mutation patches, complete final test patch, commands,
source/binary identities, both canonical attempts and repeated native results.
Archive SHA-256:
`fc1a613a968e627a7e1f2572c59e7c8327a74db749bd5dcdf34edaedc5044c69`.

Five loaded findings remain open: inbox refill, interrupted cross-runtime join,
delayed selected timer, cancellation-history timing and deadline-first join.
This is not new May, ARM64, sanitizer, offered-load or release qualification.
