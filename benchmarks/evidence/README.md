# Source-keyed slice evidence

These archives preserve raw experiment and qualification output with the source
patches, manifests and binary hashes used to produce it. See the linked review for
commands, ordering, limitations and retention decisions. This is not a complete
release qualification or an archive of executed binaries.

| Bundle | Review | SHA-256 |
| --- | --- | --- |
| `ready-fairness-bd80c2f1.tar.gz` | [Ready fairness](../ready-fairness-review.md) | `a60f5ed413af10412e64dbed83a9e8c0dcd5b02fc3735195eaa768f5b58071e8` |
| `wait-qualification-22558d5c.tar.gz` | [Ordered wait qualification](../wait-qualification-review.md) | `eb70809ed123b79cd9d0c3b79cfaef32725b4b2520fc18dfc86d50d42b38518d` |
| `admission-fairness-fc9156a9.tar.gz` | [Admission fairness](../admission-fairness-review.md) | `398d1233d2e195d542768037c64bb4ecb0fb21ec8c7f1c0f84f55cfd720e4850` |
| `completion-observer-f8f1f6f5.tar.gz` | [Completion observer](../completion-observer-review.md) | `b080b4bc24ed6a274cc2755433cb928e052dbfc338c71be341e416ba7f3f4070` |
| `publication-f5cde19e.tar.gz` | [Wake publication](../publication-review.md) | `2fa11ac9c72cee83611daac9c3791231d422acc68216416dad940b0639f5c820` |

The ready-fairness bundle includes original, FIFO, cohort-32 and cohort-2 logs;
the original failing production-queue regression; counter CSVs; native tests and
soaks; canonical receipt/logs; environment and source digests; and patches against
`4ce4f7a`. The timed cohort-2 source digest supplies the archive name. Qualification
also refreshed zrail's analysis counts, with no subsequent Rust change.

The wait-qualification bundle includes captured stalled-test and late-service
shutdown failures, diagnostic and passive-observer patches/output, a missing-stop
negative control, ordered-test qualification, and the additional open loaded-suite
findings. Its source patch applies to `3372486`. It retains both the valid canonical
receipt and the earlier report-addition repository-state rejection.

The admission bundle contains real-kernel negative controls, the final check-based
quota patch against `a456324`, canonical/native/benchmark qualification, mixed
soaks, balanced and reversed timing panels, and local/remote park counters. It
preserves the initial default-native failure that prompted the separate ordered-
test slice, as well as the final passing qualification after that repair.

The completion-observer bundle records the naturally failed accounting assertion,
the ordered old-assumption and disabled-flush negative controls, the test-only repair
against `2abae58`, canonical/native/optimized qualification, and 100 one-CPU replays.
It includes the earlier publication-source patch and rejected lock-refresh receipt
to preserve the exact context in which the original assertion failed.

The publication bundle contains the final test-only pause probes against `23822ed`,
native/all-feature/optimized qualification, and 20 one-CPU suite repeats. It confirms
a carrier-wide publication dependency while checking ownership and retirement safety;
it is not a protocol repair or a new performance comparison.

Extract into a fresh directory and inspect the manifests before replaying commands.
Absolute paths in captured receipts describe the original host, not required output
locations. The ARM64 hosted artifacts are separate and still require durable archival.
