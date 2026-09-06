# Fixed offered-load control, not another runtime policy

Base `fe53d4e`. This slice extends the existing binary-framed TCP application
harness. It changes no Rust, execution engine, admission policy, polling, channel,
mutex ownership or readiness implementation. It is not HTTP work.

## Measurement contract

Request `i` is scheduled at `origin + floor(i * 1e9 / rate)`. Responses do not
advance or postpone that schedule. At each arrival the native generator either
claims a free persistent client immediately or records a client-capacity drop.
There is one bounded mail slot per client, at most 256 clients and no unbounded
per-request future/command queue. An accepted slot returns only after response
receipt, payload validation and recording. Normal shutdown drains accepted work.

Every scheduled request remains in the raw report, including drops. Accepted
requests record offer, slot admission, request start and completion timestamps;
the existing socket-exchange duration is also retained. The separate admission
timestamp matters: a slot can become free between observing an offer and claiming
it. The verifier rejects reuse before prior completion without rejecting that
legitimate race.

The report separates:

- Generator dispatch lag, including catch-up after native scheduling delays.
- Scheduled-arrival-to-admission, start and completion.
- Actual-offer-to-completion and socket-exchange duration.
- Completed requests, client-capacity drops and the server's distinct rejection
  and failure counters.

Each timing distribution retains nearest-rank p50, p90, p99, p99.9, p99.99 and
maximum. Per-client sequence lists and response digests retain distribution and
delivery evidence. Worker/socket failures invalidate the case and preserve partial
records; they are not censored samples or backpressure. Existing descriptor/RSS,
capacity, recovery and shutdown verification remains required.

This is an end-to-end offered-load control, **not isolated scheduler latency**.
Native client scheduling, Python execution, protocol construction/validation and
the service all contribute. Generator lag remains visible and may create catch-up
bursts; client-slot drops must not be mislabeled as runtime admission rejection.
Small smokes cannot establish p99.99 stability, a fairness bound or release tails.

## Qualification and negative controls

Nineteen application evidence tests pass. Ordered controls hold a real worker
inside its exchange while all later scheduled arrivals are still recorded. They
also prove fixed-point schedule arithmetic, delayed dispatch, bounded storage,
capacity return, failure propagation and exact response/tail accounting.

Deliberately shifting late schedules to the current time fails the schedule test.
Stopping arrivals when no client is free fails four ordered tests. Disabling tail
verification fails both corrupted-summary controls. Removing requested offered
cases from an actual successful receipt fails complete-matrix verification.
All code mutations are restored; 100 further test-process repetitions pass
700 ordered tests.

Canonical receipt `run-1788702445-108011326-3183809` passes all fourteen tasks with
preserved repository state and unchanged zrail lock. It includes default-native
debug/release, all-feature tests, nineteen application evidence tests and the new
required offered-load smoke alongside the existing load/failure cases.

An additional final-source matrix covers one/four carriers, one/sixteen clients,
100/20,000 scheduled arrivals per second and 128 offers per case. All eight cases
verify. The four lower-rate cases complete all offers. Higher-rate cases retain
10–64 client-capacity drops apiece, with every other offer validated and all
service resources reclaimed. These shared-host results prove accounting and
recovery, **not throughput acceptance or a new May comparison**. An earlier smoke
is preserved under its own source identity, not relabeled as final-source proof.

## Run and verify

```sh
python3 scripts/run-application.py --carriers 1 4 --concurrency 1 16 \
  --rounds 8 --fault-rounds 1 --offered-rates 1000 20000 --offered-count 1000
python3 scripts/application_verify.py /absolute/run/receipt.json --current-source
```

The runner prints its receipt path. `CARGO_TARGET_DIR`, when set, must be absolute.
Offered cases are optional outside the canonical smoke; old receipt verification
and the default closed-loop matrix remain compatible. One case is bounded to
100,000 offers and five scheduled seconds, and one invocation to one million
offered records. Independently repeated, controlled-host acceptance remains open.

Full tested source SHA-256:
`e344a60e6098206922dc615bad1bbbbfbff7c32badedc164d5b67f862f31a1b4`.
The [durable bundle](evidence/offered-load-e344a60e.tar.gz) preserves the exact
patch, initial/negative/final runs, canonical receipt/logs, application manifests,
raw requests, binary hashes and replay ledger. All 485 internal hashes verify
after fresh-directory extraction. Archive SHA-256:
`7d6165cae50a9474fc159db946da7e2276f9c0d0bc3f7addb9d5209a9d67186a`.
