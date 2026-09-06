# Publisher deferral: model before runtime integration

This is a test-only design slice based on `6150e0a`. It does not repair the
production carrier dependency, change runtime behavior, or establish a speedup.
The [finishing pass](finishing-pass.md) remains the active order of work.

## Proposed protocol

Keep the existing notice-before-selected publication boundary. The owner queries
the exact generation without mounting a claimed wait. A claimed generation records
owner interest; publication completes that generation and signals the retained
owner hub. A deferred route retains its task, generation and resources until
selection or legal abandonment. It never authorizes premature stack reclamation.

The model reuses the otherwise absent stored-permit flag only while claimed.
Activation must have consumed the permit before entering Active. The publisher's
CAS clears this temporary interest when entering Selected. This preserves the
existing generation width and does not allocate per ordinary wake. Production
integration must establish this reachability invariant across every selector.

The publisher must retain the exact completion hub before its successful release
CAS. Only signaling that independent hub may follow selection: no old target,
route or wait-state access may race retirement and generation reuse. The extra
CAS is a correctness cost to measure, not an assumed throughput improvement.

## Model and counterexamples

Thirteen targeted tests pass, including four tests imported with the production
`WaitWord`. The model covers claim/publication, one bounded route, owner deferral,
completion registration, sleeping-owner notification, abandonment, selected
resource recovery, stale notices and reuse. Ready competes with timeout, direct
and inherited cancellation, and close. Two negative controls fail deliberately
when an incomplete claim is mounted or completion notification is omitted.

An initial owner algorithm contained a real abandonment bug: publication could
finish between an unsuccessful abandonment attempt and the next publication
query, which then consumed the resource normally. The corrected Published branch
preserves the abandonment decision and recovers ownership exactly once. The
failed assertion and repaired run are retained as evidence.

An earlier apparent Signal deadlock was **not** evidence of a production lost
wake. Loom 0.7.2 documents that its SeqCst accesses are weakened to AcqRel. The
trace permitted both epoch/waiter observers to miss, although the production SC
total order forbids that execution. Two model-only SC fences enforce this adapter
obligation; a separate test enumerates all six legal four-event SC orders.
Production memory ordering is unchanged. This is not a complete native Signal or
C11 proof.

The initial three-actor cause-competition-plus-sleep exploration was stopped
without claiming a pass. Its final replacement factors active-owner competition
from the single-winner sleep exploration: a failed selector performs no route,
ownership or signal writes. That assumption must be checked against production
integration. Completed explorations use at most three Loom threads and 1,000
branches, with no permutation, preemption or time cutoff. They finished in 44.13
seconds in the targeted debug run; an incomplete predecessor is not included in
the passing count.

The frozen model-only tree passes all fourteen canonical gates in
`run-1788665847-986082641-2648609`, including default-native debug and release.
Its [durable evidence bundle](evidence/publication-deferral-e9214dc3.tar.gz)
contains failed, incomplete and passing runs, the source patch, model binary
hashes, architecture inventory preview and canonical receipt. SHA-256:
`bbae5ee124ef57a618aa351d320598d450b9a99d37ab03bc1b54b65eaa7d9144`.
Archive extraction and all internal hashes were verified. Final evidence-link
additions do not change the tested Rust source or lock.

## Limits and next integration obligations

- WaitWord is production source; the one-route transport is an abstraction, not
  the production MPSC queue. Queue reversal, multiple routes and target binding
  remain outside this model.
- The model's ownership counter is not a proof of native mutex ticket cleanup.
  Production retirement must retain selected ownership until its actual guard
  or cancellation path recovers it.
- Kernel abort, cancellation and borrowed-parent destruction need nonblocking
  retirement preflight and bounded deferred cleanup. Deferring a child's wake
  does not authorize destroying its ancestor's borrowed environment.
- Registration failure or panic before suspension can also reach spinning
  rollback. That path needs its own ordered regression and safe retention;
  fixing only `process_wakes` would leave the carrier dependency.
- Ordinary and mutex native regressions must require unrelated work while the
  publisher is held, including active/sleeping owners, abandonment and reuse.

This model is a design gate, not the requested composed production proof. The
runtime repair, its measured cost, and all later capacity/readiness/channel
attribution slices remain open.
