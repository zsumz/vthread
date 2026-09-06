# Publication observer ordering: test-only repair

The runtime protocol is unchanged. A final canonical attempt during mutex
measurement work exposed a false ordering assumption in the sleeping-publisher
tests: `OwnerDeferred` reached the observer before `NoticePublished`.

The publisher enqueues/routes the wake before sending its diagnostic message.
A concurrent owner can consume and defer that wake before the publisher sends
the message. Message arrival order therefore does not establish a production
publication edge. The failure was not a lost wake or premature retirement.

The original failure, full source patch, receipt and exact command are retained.
That run passed 564 other runtime tests. The mutex lab was stashed before this
repair; no mutex performance panel had run, and no runtime optimization is part
of this checkpoint.

## Narrow repair and negative controls

Only two `_test.rs` files change. A helper consumes exactly two observations,
accepts `NoticePublished` and `OwnerDeferred` in either delivery order, checks
their exact park token and returns them by meaning. There is no queue, general
event buffer, larger timeout or change to wake publication/retirement.

The deterministic reversed-order regression fails the old consecutive
observation calls with the same assertion as the canonical failure. The repaired
helper passes both orders and rejects different generations and mounted
`FinishWaiting` in place of owner deferral. The native tests still require
unrelated work while the publisher remains held, retained mutex ownership,
completion notification to a sleeping owner and legal shutdown/reclamation.

All fourteen canonical tasks pass under receipt
`run-1788686133-417208506-2867869`, with repository state preserved. The runtime
suites pass 539 tests in each default debug/release profile and 568 with all
features, with one existing manual probe ignored in each configuration.

An additional 25 debug and 25 optimized invocations run all eight sleeping-owner
tests: **400 passes**. Exactly 34 observe the owner first and 366 observe the
publisher first. Both real orders preserve progress and ownership assertions.
An initial repetition-script path matcher refused ambiguous release executables
before running any; its correction and exclusion are recorded in the ledger.

This closes the newly found observer assumption, not the six earlier loaded
findings. It is neither a performance improvement nor new ARM64/sanitizer proof.

## Durable evidence

Source SHA-256:
`266e8624477156b75c9af529ea3722e91cbb5e87bf0bd13bd91f57c5e94f184f`.

The [source-keyed archive](evidence/publication-observer-266e8624.tar.gz)
contains the original failure, deterministic negative control, final patch,
canonical receipt/logs, all 50 repetition processes and token-keyed analysis.
All 187 internal hashes verify after extraction. Archive SHA-256:
`b411fd245b7f5e438da2d2d533570656f700b3ef007a9b39a21efb21f98f220a`.
