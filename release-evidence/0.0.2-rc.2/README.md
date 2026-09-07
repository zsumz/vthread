# RC2 local qualification evidence

This is a prepared release candidate, not a published release or permission to
waive the [remaining release gates](../../RELEASE.md).

[Qualification bundle](qualification-8ac4c314.tar.gz), approximately 2.5 MiB.
SHA-256:
`abc33254ab6fdd756522e772457f9c47a9fda7758228074e6414c685d80f3654`.

The bundle includes the complete source snapshot at
`58976649f5fb2bf716cb5e2a3694dbb6bf2b2548`, final distributable crate archives,
raw logs, review and cross-review notes, negative-control patches/results,
compiler/host metadata, source/binary identities, and qualification receipts.
The code/manifest digest is
`8ac4c314fdfe944a53ddf4395b1feb4cfa5bd18465f0c559201823030e3ef73d`.
The source checkpoint's release document precedes the final package audit; this
documentation checkpoint records that result without changing qualified code.

## Observed results

- All 18 final canonical gates passed, including native debug/release,
  all-feature tests, bounded models, Clippy, formatting, docs, architecture and
  application/benchmark qualification. Repository state was preserved.
- Native stack: 79 debug and 79 release tests passed. Standalone harness:
  45 default and 53 all-feature tests passed. Reference: 13 passed.
- Full application: 22 final-source cases passed, including fixed arrivals,
  overload, deadlines, disconnection, recovery and shutdown.
- Three bounded 30-second mixed soaks completed and reclaimed 530,361 task
  lifetimes on one/four carriers with 64/1,024-task batches. These precede the
  final cleanup-comment correction; their exact earlier source/binary identities
  remain labeled separately in the bundle.
- Four clean RC archives passed offline Cargo build verification and independent
  audit. Versions, internal pins, sibling archive checksums, all source files and
  all four license files matched committed source. No publication was performed.

The initial full canonical pass also passed. Earlier failing tests are deliberate
negative controls for the repaired queue inspection, context borrowing, unstarted
destruction and cookie exhaustion. Initial archives are preserved separately with
their license-omission finding and dirty VCS identities; use `packages-final/`,
not `packages-initial/`, for the final candidate artifacts.

## Integrity and replay

A fresh extraction verified the exact inventory and all **310 payload hashes**.
Both application receipts passed the existing evidence verifier after extraction;
the final one also matched the current code digest. Both archived canonical
receipts record 18 passing gates and preserved repository state. Hashing the
539-file archived checkout independently reproduced the code/manifest digest.

From this directory:

```sh
sha256sum qualification-8ac4c314.tar.gz
rc_evidence_dir=$(mktemp -d)
tar -xzf qualification-8ac4c314.tar.gz -C "$rc_evidence_dir"
(cd "$rc_evidence_dir" && sha256sum --check SHA256SUMS)
```

Read the extracted `README.md` for exact commands and evidence boundaries. Extract
`source.tar` separately to inspect the frozen checkout. Canonical qualification
uses the repository's `zcheck`, `zrail`, and pinned Rust tooling; no substitute
qualification wrapper was introduced.

## Limits

These are local Linux x86-64 correctness and package receipts. They do not qualify
native ARM64 execution, alternate-stack sanitizer integration, all historical
findings, large simultaneous populations, the full mixed-lifetime target, or
controlled-host performance and loaded tails. Two explicit manual performance
probes remain excluded from canonical tests; cancellation semantics remain
mandatory. Historical performance material is preserved outside the RC tree.
No optional held optimization was promoted, and no performance win is claimed.
