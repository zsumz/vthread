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

Extract into a fresh directory and inspect the manifests before replaying commands.
Absolute paths in captured receipts describe the original host, not required output
locations. The ARM64 hosted artifacts are separate and still require durable archival.
