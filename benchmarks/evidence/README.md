# Source-keyed slice evidence

These archives preserve raw experiment and qualification output with the source
patches, manifests and binary hashes used to produce it. See the linked review for
commands, ordering, limitations and retention decisions. This is not a complete
release qualification or an archive of executed binaries.

| Bundle | Review | SHA-256 |
| --- | --- | --- |
| `ready-fairness-bd80c2f1.tar.gz` | [Ready fairness](../ready-fairness-review.md) | `a60f5ed413af10412e64dbed83a9e8c0dcd5b02fc3735195eaa768f5b58071e8` |

The ready-fairness bundle includes original, FIFO, cohort-32 and cohort-2 logs;
the original failing production-queue regression; counter CSVs; native tests and
soaks; canonical receipt/logs; environment and source digests; and patches against
`4ce4f7a`. The timed cohort-2 source digest supplies the archive name. Qualification
also refreshed zrail's analysis counts, with no subsequent Rust change.

Extract into a fresh directory and inspect the manifests before replaying commands.
Absolute paths in captured receipts describe the original host, not required output
locations. The ARM64 hosted artifacts are separate and still require durable archival.
