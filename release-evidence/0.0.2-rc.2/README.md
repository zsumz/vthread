# Release candidate 0.0.2-rc.2 evidence

This is a prepared release candidate, not a published release or permission to
waive the [remaining release gates](../../RELEASE.md).

## Archived identity

| Artifact | Identity |
| --- | --- |
| [Qualification bundle](qualification-8ac4c314.tar.gz) | Approximately 2.5 MiB |
| Complete source snapshot | `58976649f5fb2bf716cb5e2a3694dbb6bf2b2548` |
| Final crate archives | `packages-final/` inside the bundle |

Bundle SHA-256:

`abc33254ab6fdd756522e772457f9c47a9fda7758228074e6414c685d80f3654`.

Archived code/manifest digest:

`8ac4c314fdfe944a53ddf4395b1feb4cfa5bd18465f0c559201823030e3ef73d`.

The bundle also includes raw logs, review and cross-review notes, negative-control
patches and results, compiler and host metadata, source and binary identities, and
qualification receipts. The source checkpoint's release document precedes the
final package audit; this page records the completed audit.

The archive is immutable historical evidence. Later README presentation changes
do not alter executable source, but they are not the bytes of the archived source
or package snapshot. These hashes identify the preserved artifacts; they are not
updated package hashes for the edited checkout.

## Observed results

| Check | Recorded result |
| --- | --- |
| Final canonical qualification | All 18 gates passed: native debug/release, all-feature tests, bounded models, Clippy, formatting, docs, architecture, and application/benchmark qualification. Repository state was preserved. |
| Native stack | 79 debug and 79 release tests passed |
| Standalone benchmark harness | 45 default and 53 all-feature tests passed |
| Standalone reference | 13 tests passed |
| Full application | 22 final-source cases passed, including fixed arrivals, overload, deadlines, disconnection, recovery and shutdown |
| Bounded mixed soak | Three 30-second processes completed and reclaimed 530,361 task lifetimes on one/four carriers with 64/1,024-task batches |
| Distributable packages | Four clean RC archives passed offline Cargo build verification and independent audit |

The soaks precede the final cleanup-comment correction. Their exact earlier source
and binary identities remain labeled separately in the bundle.

Final package versions, internal pins, sibling archive checksums, all source files
and all four license files matched committed source. No publication was performed.

The initial full canonical pass also passed. Earlier failing tests are deliberate
negative controls for the repaired queue inspection, context borrowing, unstarted
destruction and cookie exhaustion.

Initial archives are preserved in `packages-initial/` with their license-omission
finding and dirty VCS identities. Use `packages-final/` for final candidate
artifacts.

## Integrity and replay

A fresh extraction verified the exact inventory and all 310 payload hashes.
Both application receipts passed the existing evidence verifier after extraction;
the final one also matched the archived final code digest. Both archived canonical
receipts record 18 passing gates and preserved repository state. Hashing the
539-file archived checkout independently reproduced the archived code/manifest
digest.

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

These are local Linux x86-64 correctness and package receipts. The following remain
outside their qualification:

- Native ARM64 execution and alternate-stack sanitizer integration.
- All historical findings, large simultaneous populations, and the full
  mixed-lifetime target.
- Controlled-host performance and loaded tails.

Two explicit manual performance probes remain excluded from canonical tests;
cancellation semantics remain mandatory. Historical performance material is
preserved outside the RC tree. No optional held optimization was promoted, and no
performance win is claimed.
