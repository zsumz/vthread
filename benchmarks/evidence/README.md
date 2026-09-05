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
| `scheduler-profile-718e21c1.tar.gz` | [Scheduler activity](../scheduler-profile-review.md) | `7dc52997e93f3d3499eb02afa224c35a666fb25f658397c76f6a8a5598b5b397` |
| `capacity-pacing-244bc8a1.tar.gz` | [Rejected capacity experiment](../capacity-pacing-review.md) | `93539843d126e7cc2b1b3c71293c8bf9d64cd1b21a344c1522d80b58240a41b5` |
| `ingress-visibility-aa2fa66e.tar.gz` | [Idle ingress progress](../ingress-visibility-review.md) | `9d3d6c9ec47309922b4cb716c689f00b02ecd46d81f40ccc47faf4e59c5b1bdc` |
| `capacity-ingress-e6474864.tar.gz` | [Four-way capacity attribution](../capacity-pacing-review.md) | `766796d6657116476b550162cfe61046f336bc74f2c2ca7ebe9cf99252ac2836` |

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

The scheduler-profile bundle preserves the opt-in counter/report source, canonical
and both native feature configurations, benchmark gates, a final-only lifecycle
profile and noisy default controls explicitly excluded from performance acceptance.
Source/binary hashes, the exact macro-grant preview, raw output and analysis/replay
commands are included. No scan-free runtime change is part of that checkpoint.

The capacity-pacing bundle preserves a rejected observer-only patch against
`62378f4`, failing/passing snapshot checks and 30 balanced diagnostic processes.
It exposes the lifecycle batching/polling penalty rather than qualifying a speedup.
The source remains shelved; its full canonical and default-performance gates did
not run. Both instrumented binary identities and raw owner counters are included.

The ingress-visibility bundle contains the separate owner-local progress repair
against `62378f4`, a real-carrier paused-notifier negative control, canonical/native/
optimized qualification, 20 one-CPU replays and 705,249 mixed soak lifetimes. It
preserves initial and final test sources separately, along with mixed default-build
controls, excluded contaminated rounds and endpoint host observations. Its progress
proof passes; quiet-host multi-carrier performance/tail acceptance remains open.

The capacity-ingress bundle retains the rejected combined observer/ingress source
against `a4b5a96`, six targeted checks, 48 four-way diagnostic processes, all source
and binary identities, and the separate uninstrumented cycle sample/annotation.
It demonstrates that removing empty-idle reentry does not recover the lifecycle
batching penalty. No idle policy or scan-free change is retained in production.

Extract into a fresh directory and inspect the manifests before replaying commands.
Absolute paths in captured receipts describe the original host, not required output
locations. The ARM64 hosted artifacts are separate and still require durable archival.
